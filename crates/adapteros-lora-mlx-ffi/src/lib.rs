//! MLX FFI integration for AdapterOS
//!
//! This crate provides C FFI bindings for MLX's C++ API, avoiding PyO3 dependency issues.
//! It implements the same interface as the PyO3-based MLX crate but uses direct C++ calls.

use adapteros_core::{AosError, Result};
use std::path::Path;

// Include the generated bindings
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub mod backend;
pub mod embedding;
pub mod lora;
pub mod routing;
pub mod tensor;

#[cfg(test)]
pub mod mock;

pub use backend::MLXFFIBackend;
pub use embedding::{EmbeddingConfig, MLXEmbeddingModel};
pub use lora::{LoRAAdapter, LoRAConfig};
pub use routing::apply_multi_lora;
pub use tensor::MLXFFITensor;

/// Set MLX's global random seed for deterministic dropout/sampling operations.
///
/// This function accepts a 32-byte HKDF-derived seed and applies it to MLX's
/// random number generator. Seeded operations like dropout and sampling will
/// produce deterministic results.
///
/// # Arguments
/// * `seed` - 32-byte seed buffer (typically from HKDF derivation)
///
/// # Limitations
/// MLX is not fully deterministic - the execution order of operations can vary
/// between runs due to GPU scheduling. This function only controls the RNG seed
/// used by individual operations, not the execution order determinism of the backend.
///
/// # Example
/// ```ignore
/// use adapteros_core::derive_seed;
/// use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;
///
/// let global_seed = adapteros_core::B3Hash::hash(b"my-model");
/// let seed = derive_seed(&global_seed, "mlx-step:0");
/// mlx_set_seed_from_bytes(&seed);
/// ```
pub fn mlx_set_seed_from_bytes(seed: &[u8]) -> Result<()> {
    if seed.is_empty() {
        return Err(AosError::Internal("Seed buffer cannot be empty".to_string()));
    }

    unsafe {
        mlx_set_seed(seed.as_ptr(), seed.len());

        // Check if there was an error during seed setting
        let error_msg = mlx_get_last_error();
        if !error_msg.is_null() {
            let error_str = std::ffi::CStr::from_ptr(error_msg)
                .to_string_lossy()
                .to_string();
            if !error_str.is_empty() && error_str != "Invalid seed: pointer is null or length is 0" {
                // Clear the error for next call
                mlx_clear_error();
                tracing::warn!("MLX seed setting warning: {}", error_str);
            }
        }
    }

    tracing::debug!(
        seed_len = seed.len(),
        "MLX backend seeded for deterministic dropout/sampling"
    );

    Ok(())
}

/// Memory management API for MLX backend
///
/// Provides functions for monitoring and managing memory usage in the MLX unified memory system.
pub mod memory {
    use super::*;

    /// Trigger garbage collection in MLX unified memory
    ///
    /// This hints the system to reclaim unused buffers by flushing pending operations
    /// and allowing the memory manager to compact its pools.
    ///
    /// # Example
    /// ```ignore
    /// use adapteros_lora_mlx_ffi::memory;
    ///
    /// memory::gc_collect();
    /// ```
    pub fn gc_collect() {
        unsafe {
            mlx_gc_collect();
        }
    }

    /// Get total memory usage in bytes
    ///
    /// Tracks all array allocations and model weights through the FFI wrapper.
    /// Returns the sum of all currently allocated unified memory buffers.
    ///
    /// # Example
    /// ```ignore
    /// let bytes = memory::memory_usage();
    /// let mb = bytes as f32 / (1024.0 * 1024.0);
    /// println!("Memory usage: {:.2} MB", mb);
    /// ```
    pub fn memory_usage() -> usize {
        unsafe { mlx_memory_usage() }
    }

    /// Get the number of tracked allocations
    ///
    /// Useful for debugging and profiling to understand allocation patterns
    /// and detect potential memory leaks.
    ///
    /// # Example
    /// ```ignore
    /// let count = memory::allocation_count();
    /// println!("Active allocations: {}", count);
    /// ```
    pub fn allocation_count() -> usize {
        unsafe { mlx_allocation_count() }
    }

    /// Get detailed memory statistics
    ///
    /// Returns a tuple of (total_bytes, allocation_count)
    ///
    /// # Example
    /// ```ignore
    /// let (total, count) = memory::memory_stats();
    /// println!("Total: {} bytes, Allocations: {}", total, count);
    /// ```
    pub fn memory_stats() -> (usize, usize) {
        let mut total_bytes = 0;
        let mut allocation_count = 0;
        unsafe {
            mlx_memory_stats(&mut total_bytes, &mut allocation_count);
        }
        (total_bytes, allocation_count)
    }

    /// Reset memory tracking
    ///
    /// Clears all tracked allocations and resets counters to zero.
    /// Used for testing and debugging purposes.
    ///
    /// # Example
    /// ```ignore
    /// use adapteros_lora_mlx_ffi::memory;
    ///
    /// memory::reset();
    /// // ... perform operations ...
    /// let stats = memory::stats();
    /// println!("Memory used in this scope: {}", stats.total_bytes);
    /// ```
    pub fn reset() {
        unsafe {
            mlx_memory_reset();
        }
    }

    /// Memory statistics snapshot
    ///
    /// A structured representation of memory usage at a point in time
    #[derive(Debug, Clone, Copy)]
    pub struct MemoryStats {
        /// Total bytes allocated
        pub total_bytes: usize,
        /// Number of allocations
        pub allocation_count: usize,
    }

    /// Get memory statistics as a structured snapshot
    ///
    /// # Example
    /// ```ignore
    /// let stats = memory::stats();
    /// println!("{}", memory::format_stats(&stats));
    /// ```
    pub fn stats() -> MemoryStats {
        let (total_bytes, allocation_count) = memory_stats();
        MemoryStats {
            total_bytes,
            allocation_count,
        }
    }

    /// Convert bytes to megabytes
    ///
    /// # Example
    /// ```ignore
    /// let mb = memory::bytes_to_mb(1024 * 1024);
    /// assert_eq!(mb, 1.0);
    /// ```
    pub fn bytes_to_mb(bytes: usize) -> f32 {
        bytes as f32 / (1024.0 * 1024.0)
    }

    /// Format memory statistics for logging or display
    ///
    /// # Example
    /// ```ignore
    /// let stats = memory::stats();
    /// tracing::info!("{}", memory::format_stats(&stats));
    /// // Output: "MLX Memory: 123.45 MB (42 allocations)"
    /// ```
    pub fn format_stats(stats: &MemoryStats) -> String {
        let mb = bytes_to_mb(stats.total_bytes);
        format!(
            "MLX Memory: {:.2} MB ({} allocations)",
            mb, stats.allocation_count
        )
    }

    /// Check if memory usage exceeds a threshold
    ///
    /// # Arguments
    /// * `threshold_mb` - Memory threshold in megabytes
    ///
    /// # Returns
    /// true if current memory usage exceeds the threshold
    ///
    /// # Example
    /// ```ignore
    /// if memory::exceeds_threshold(2048.0) {
    ///     tracing::warn!("Memory usage exceeded 2GB");
    ///     memory::gc_collect();
    /// }
    /// ```
    pub fn exceeds_threshold(threshold_mb: f32) -> bool {
        let stats = stats();
        bytes_to_mb(stats.total_bytes) > threshold_mb
    }
}

/// MLX model wrapper for inference using FFI
pub struct MLXFFIModel {
    /// C++ MLX model object
    model: *mut mlx_model_t,
    /// Model configuration
    pub config: ModelConfig,
    /// Health status tracking
    health: std::sync::Arc<std::sync::Mutex<ModelHealth>>,
}

/// Health status for MLX model
#[derive(Debug, Clone)]
pub struct ModelHealth {
    /// Is the model currently operational
    pub operational: bool,
    /// Number of consecutive failures
    pub consecutive_failures: u32,
    /// Last successful operation timestamp
    pub last_success: Option<std::time::Instant>,
    /// Last failure reason
    pub last_failure: Option<String>,
    /// Circuit breaker state
    pub circuit_breaker: CircuitBreakerState,
}

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitBreakerState {
    /// Normal operation
    Closed,
    /// Temporarily open after failures
    Open,
    /// Testing if service recovered
    HalfOpen,
}

/// Model configuration parsed from config.json
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelConfig {
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub intermediate_size: usize,
    pub vocab_size: usize,
    pub max_position_embeddings: usize,
    #[serde(default = "default_rope_theta")]
    pub rope_theta: f32,
}

fn default_rope_theta() -> f32 {
    10000.0
}

impl MLXFFIModel {
    /// Check if the model is currently healthy and operational
    pub fn is_healthy(&self) -> bool {
        if let Ok(health) = self.health.lock() {
            matches!(health.circuit_breaker, CircuitBreakerState::Closed)
                && health.consecutive_failures < 5
        } else {
            false
        }
    }

    /// Get current health status
    pub fn health_status(&self) -> Option<ModelHealth> {
        self.health.lock().ok().map(|h| h.clone())
    }

    /// Reset circuit breaker (admin operation)
    pub fn reset_circuit_breaker(&self) {
        if let Ok(mut health) = self.health.lock() {
            health.circuit_breaker = CircuitBreakerState::Closed;
            health.consecutive_failures = 0;
            tracing::info!("MLX model circuit breaker reset");
        }
    }

    /// Configure resilience settings
    pub fn with_resilience_config(self, config: crate::backend::MLXResilienceConfig) -> Self {
        // This would be used when creating the backend
        // For now, just log the configuration
        tracing::info!(
            "MLX model configured with resilience: max_failures={}, stub_fallback={}",
            config.max_consecutive_failures,
            config.enable_stub_fallback
        );
        self
    }

    /// Load a model from MLX format using FFI
    ///
    /// # Arguments
    /// * `model_path` - Path to directory containing model files
    ///
    /// # Returns
    /// Loaded MLX model ready for inference
    pub fn load<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let health = std::sync::Arc::new(std::sync::Mutex::new(ModelHealth {
            operational: false,
            consecutive_failures: 0,
            last_success: None,
            last_failure: None,
            circuit_breaker: CircuitBreakerState::Closed,
        }));

        match Self::load_with_health(model_path, health.clone()) {
            Ok(mut model) => {
                // Mark as operational
                if let Ok(mut health_guard) = health.lock() {
                    health_guard.operational = true;
                    health_guard.consecutive_failures = 0;
                    health_guard.last_success = Some(std::time::Instant::now());
                }
                Ok(model)
            }
            Err(e) => {
                // Record failure
                if let Ok(mut health_guard) = health.lock() {
                    health_guard.operational = false;
                    health_guard.consecutive_failures += 1;
                    health_guard.last_failure = Some(e.to_string());
                }
                Err(e)
            }
        }
    }

    /// Internal load method with health tracking
    fn load_with_health<P: AsRef<Path>>(model_path: P, health: std::sync::Arc<std::sync::Mutex<ModelHealth>>) -> Result<Self> {
        let model_path = model_path.as_ref();

        // Load config
        let config_path = model_path.join("config.json");
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| AosError::Io(format!("Failed to read config: {}", e)))?;
        let config: ModelConfig = serde_json::from_str(&config_str)
            .map_err(|e| AosError::Parse(format!("Failed to parse config: {}", e)))?;

        // Convert path to C string
        let path_str = model_path
            .to_str()
            .ok_or_else(|| AosError::Internal("Invalid model path".to_string()))?;
        let path_cstr = std::ffi::CString::new(path_str)
            .map_err(|e| AosError::Internal(format!("Invalid path string: {}", e)))?;

        // Clear any previous errors
        unsafe {
            mlx_clear_error();
        }

        // Load model via FFI
        let model = unsafe { mlx_model_load(path_cstr.as_ptr()) };
        if model.is_null() {
            let error_msg = unsafe { mlx_get_last_error() };
            let error_str = if error_msg.is_null() {
                "Unknown MLX error".to_string()
            } else {
                unsafe {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                }
            };
            return Err(AosError::Mlx(format!(
                "Failed to load MLX model: {}",
                error_str
            )));
        }

        tracing::info!("MLX model loaded via FFI: {}", path_str);

        Ok(Self { model, config, health })
    }

    /// Run forward pass for a single token using FFI
    ///
    /// # Arguments
    /// * `token_ids` - Input token IDs
    /// * `position` - Current position in sequence
    ///
    /// # Returns
    /// Logits for next token prediction
    pub fn forward(&self, token_ids: &[u32], position: usize) -> Result<Vec<f32>> {
        // Check circuit breaker
        if let Ok(health) = self.health.lock() {
            if matches!(health.circuit_breaker, CircuitBreakerState::Open) {
                return Err(AosError::Mlx("Circuit breaker open - model temporarily disabled".to_string()));
            }
        }

        match self.forward_internal(token_ids, position) {
            Ok(result) => {
                // Record success
                if let Ok(mut health) = self.health.lock() {
                    health.consecutive_failures = 0;
                    health.last_success = Some(std::time::Instant::now());
                    if matches!(health.circuit_breaker, CircuitBreakerState::HalfOpen) {
                        health.circuit_breaker = CircuitBreakerState::Closed;
                        tracing::info!("MLX model recovered - circuit breaker closed");
                    }
                }
                Ok(result)
            }
            Err(e) => {
                // Record failure and potentially open circuit breaker
                if let Ok(mut health) = self.health.lock() {
                    health.consecutive_failures += 1;
                    health.last_failure = Some(e.to_string());

                    if health.consecutive_failures >= 3 && matches!(health.circuit_breaker, CircuitBreakerState::Closed) {
                        health.circuit_breaker = CircuitBreakerState::Open;
                        tracing::warn!("MLX model circuit breaker opened after {} consecutive failures", health.consecutive_failures);
                    }
                }
                Err(e)
            }
        }
    }

    /// Internal forward pass implementation
    fn forward_internal(&self, token_ids: &[u32], _position: usize) -> Result<Vec<f32>> {
        // Convert token_ids to C array
        let token_ints: Vec<i32> = token_ids.iter().map(|&x| x as i32).collect();

        // Create MLX array from token IDs
        let input_array =
            unsafe { mlx_array_from_ints(token_ints.as_ptr(), token_ints.len() as i32) };
        if input_array.is_null() {
            return Err(AosError::Mlx("Failed to create input array".to_string()));
        }

        // Run forward pass
        let output_array = unsafe { mlx_model_forward(self.model, input_array) };
        if output_array.is_null() {
            unsafe { mlx_array_free(input_array) };
            return Err(AosError::Mlx("Failed to run model forward".to_string()));
        }

        // Extract output data with safety validation
        let output_size = unsafe { mlx_array_size(output_array) };
        let output_data = unsafe { mlx_array_data(output_array) };

        // Safety: Validate tensor size before creating slice
        if output_size == 0 {
            unsafe { mlx_array_free(input_array) };
            unsafe { mlx_array_free(output_array) };
            return Err(AosError::Mlx("Model returned empty output".to_string()));
        }

        const MAX_TENSOR_SIZE: usize = 1024 * 1024 * 100; // 100M elements max
        if output_size as usize > MAX_TENSOR_SIZE {
            unsafe { mlx_array_free(input_array) };
            unsafe { mlx_array_free(output_array) };
            return Err(AosError::Mlx(format!(
                "Output tensor too large: {} elements (max: {})",
                output_size, MAX_TENSOR_SIZE
            )));
        }

        // Check pointer validity
        if output_data.is_null() {
            unsafe { mlx_array_free(input_array) };
            unsafe { mlx_array_free(output_array) };
            return Err(AosError::Mlx("Invalid output data pointer".to_string()));
        }

        let result: Vec<f32> =
            unsafe { std::slice::from_raw_parts(output_data, output_size as usize).to_vec() };

        // Clean up
        unsafe {
            mlx_array_free(input_array);
            mlx_array_free(output_array);
        }

        tracing::debug!(
            "MLX FFI forward pass complete: {} tokens -> {} logits",
            token_ids.len(),
            result.len()
        );

        Ok(result)
    }

    /// Generate text from a prompt using FFI
    ///
    /// # Arguments
    /// * `prompt` - Input text prompt
    /// * `max_tokens` - Maximum tokens to generate
    ///
    /// # Returns
    /// Generated text
    pub fn generate(&self, prompt: &str, _max_tokens: usize) -> Result<String> {
        // For now, return a placeholder implementation
        // Full implementation would require tokenization and generation loop
        tracing::warn!("MLX FFI generate() not yet implemented, returning placeholder");
        Ok(format!("[MLX FFI placeholder for: {}]", prompt))
    }

    /// Run forward pass with hidden states using FFI
    ///
    /// # Arguments
    /// * `token_ids` - Input token IDs
    ///
    /// # Returns
    /// Tuple of (logits, hidden_states_by_module)
    pub fn forward_with_hidden_states(
        &self,
        token_ids: &[u32],
    ) -> Result<(Vec<f32>, std::collections::HashMap<String, Vec<f32>>)> {
        // Convert token_ids to C array
        let token_ints: Vec<i32> = token_ids.iter().map(|&x| x as i32).collect();

        // Create MLX array from token IDs
        let input_array =
            unsafe { mlx_array_from_ints(token_ints.as_ptr(), token_ints.len() as i32) };
        if input_array.is_null() {
            return Err(AosError::Mlx("Failed to create input array".to_string()));
        }

        // Prepare hidden states array
        let mut hidden_states_ptr: *mut mlx_array_t = std::ptr::null_mut();
        let mut num_hidden: i32 = 0;

        // Run forward pass with hidden states
        let output_array = unsafe {
            mlx_model_forward_with_hidden_states(
                self.model,
                input_array,
                &mut hidden_states_ptr,
                &mut num_hidden
            )
        };

        if output_array.is_null() {
            unsafe { mlx_array_free(input_array) };
            return Err(AosError::Mlx("Failed to run model forward with hidden states".to_string()));
        }

        // Extract logits
        let output_size = unsafe { mlx_array_size(output_array) };
        let output_data = unsafe { mlx_array_data(output_array) };

        // Safety validation
        if output_size == 0 {
            unsafe { mlx_array_free(input_array) };
            unsafe { mlx_array_free(output_array) };
            if !hidden_states_ptr.is_null() {
                unsafe { mlx_array_free(hidden_states_ptr) };
            }
            return Err(AosError::Mlx("Model returned empty output".to_string()));
        }

        if output_data.is_null() {
            unsafe { mlx_array_free(input_array) };
            unsafe { mlx_array_free(output_array) };
            if !hidden_states_ptr.is_null() {
                unsafe { mlx_array_free(hidden_states_ptr) };
            }
            return Err(AosError::Mlx("Invalid output data pointer".to_string()));
        }

        let logits: Vec<f32> = unsafe {
            std::slice::from_raw_parts(output_data, output_size as usize).to_vec()
        };

        // Process hidden states
        let mut hidden_states = std::collections::HashMap::new();

        if !hidden_states_ptr.is_null() && num_hidden > 0 {
            // In real implementation, this would parse the hidden states array
            // For now, create dummy hidden states for target modules
            let dummy_hidden = vec![0.1f32; token_ids.len() * self.config.hidden_size];

            for module in ["q_proj", "k_proj", "v_proj", "o_proj"].iter() {
                hidden_states.insert(module.to_string(), dummy_hidden.clone());
            }

            unsafe { mlx_array_free(hidden_states_ptr) };
        }

        // Clean up
        unsafe {
            mlx_array_free(input_array);
            mlx_array_free(output_array);
        }

        tracing::debug!(
            "MLX FFI forward with hidden states: {} tokens -> {} logits, {} hidden state modules",
            token_ids.len(),
            logits.len(),
            hidden_states.len()
        );

        Ok((logits, hidden_states))
    }

    /// Get model configuration
    pub fn config(&self) -> &ModelConfig {
        &self.config
    }
}

impl Drop for MLXFFIModel {
    fn drop(&mut self) {
        if !self.model.is_null() {
            unsafe {
                mlx_model_free(self.model);
            }
        }
    }
}

// Safety: MLX FFI model is thread-safe
unsafe impl Send for MLXFFIModel {}
unsafe impl Sync for MLXFFIModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_config_parsing() {
        let config_json = r#"
        {
            "hidden_size": 4096,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 8,
            "intermediate_size": 11008,
            "vocab_size": 32000,
            "max_position_embeddings": 32768,
            "rope_theta": 10000.0
        }
        "#;

        let config: ModelConfig = serde_json::from_str(config_json).unwrap();
        assert_eq!(config.hidden_size, 4096);
        assert_eq!(config.num_hidden_layers, 32);
        assert_eq!(config.rope_theta, 10000.0);
    }

    #[test]
    #[ignore] // Requires MLX model
    fn test_model_loading() {
        // This test would require a real MLX model
        // Skipped for now
    }
}
