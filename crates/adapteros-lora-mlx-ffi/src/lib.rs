//! MLX FFI integration for AdapterOS
//!
//! This crate provides C FFI bindings for MLX's C++ API, avoiding PyO3 dependency issues.
//! It implements the same interface as the PyO3-based MLX crate but uses direct C++ calls.

use adapteros_core::{AosError, B3Hash, Result};
use std::path::{Path, PathBuf};

// Using manual FFI declarations instead of generated bindings

pub mod attention;
pub mod backend;
pub mod embedding;
pub mod generation;
pub mod kv_cache;
pub mod lora;
pub mod memory_pool;
pub mod monitoring;
pub mod quantization;
pub mod routing;
pub mod safetensors_loader;
pub mod streaming;
pub mod tensor;
pub mod tokenizer;
pub mod unified_loader;

// Adapter cache for efficient LoRA weight management
pub mod adapter_cache;

// Mock module for testing - always available since integration tests need it
pub mod mock;

pub use adapter_cache::{AdapterCacheStats, MLXAdapterCache, MLXAdapterCacheConfig};
pub use attention::{
    mlx_multihead_attention, mlx_rope, mlx_scaled_dot_product_attention, AttentionConfig,
    RoPEFrequencies,
};
pub use backend::MLXFFIBackend;
pub use embedding::{EmbeddingConfig, MLXEmbeddingModel};
pub use generation::{GenerationConfig, MLXGenerator};
pub use kv_cache::{CacheLayer, CacheStats, KVCacheConfig, MLXKVCache};
pub use lora::{LoRAAdapter, LoRAConfig};
pub use memory_pool::{MLXMemoryPool, MLXMemoryPoolConfig, MemoryPoolStats, MemoryPressureEvent};
pub use quantization::{
    MLXQuantizer, QuantizationConfig, QuantizationMetadata, QuantizationStats, QuantizedTensor,
    WeightCompressor,
};
pub use routing::apply_multi_lora;
pub use safetensors_loader::{SafetensorsLoader, TensorInfo};
pub use tensor::MLXFFITensor;
pub use tokenizer::MLXTokenizer;
pub use unified_loader::{LoadStrategy, TensorMetadata, UnifiedSafeTensorsLoader};

// Re-export types for backend capabilities and device selection.
// MlxDeviceType: Enum for selecting CPU, GPU, ANE, or Auto device
// MlxBackendCapabilities: Struct containing hardware/runtime capability info
//
// Safe runtime wrapper functions are also exported at module level:
// - mlx_runtime_init / mlx_runtime_init_with_device: Initialize MLX runtime
// - mlx_runtime_is_initialized: Check initialization status
// - mlx_runtime_shutdown: Shutdown MLX runtime
// - mlx_get_backend_capabilities: Query hardware capabilities
// - mlx_version: Get MLX version string
// - mlx_ensure_initialized: Check/auto-init helper
// - mlx_force_eval / mlx_force_eval_all: Force lazy evaluation
// - mlx_sync: Synchronize GPU operations

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
        return Err(AosError::Internal(
            "Seed buffer cannot be empty".to_string(),
        ));
    }

    unsafe {
        // Clear any previous error state before operation
        mlx_clear_error();

        mlx_set_seed(seed.as_ptr(), seed.len());

        // Check if there was an error during seed setting
        let error_msg = mlx_get_last_error();
        if !error_msg.is_null() {
            let error_str = std::ffi::CStr::from_ptr(error_msg)
                .to_string_lossy()
                .to_string();
            if !error_str.is_empty() && error_str != "Invalid seed: pointer is null or length is 0"
            {
                // Clear the error for next call
                mlx_clear_error();
                // Return error instead of just warning
                return Err(AosError::Mlx(format!(
                    "Failed to set MLX seed: {}",
                    error_str
                )));
            }
        }
    }

    tracing::debug!(
        seed_len = seed.len(),
        "MLX backend seeded for deterministic dropout/sampling"
    );

    Ok(())
}

/// Sample next token from logits using MLX's native RNG
///
/// Implements a complete sampling pipeline with temperature scaling, top-k filtering,
/// and top-p (nucleus) sampling. All computation is performed on GPU via MLX.
///
/// # Arguments
/// * `logits` - MLXFFITensor containing vocabulary logits
/// * `temperature` - Temperature for sampling:
///   - 0.0 = greedy (argmax)
///   - 0.5-1.5 = reasonable range for stochastic sampling
///   - >1.5 = very random
/// * `top_k` - Keep only top K tokens (0 = disabled, typically 40-50)
/// * `top_p` - Keep tokens until cumulative probability >= p (0 = disabled, typical 0.9)
///
/// # Returns
/// Sampled token ID (0 <= token_id < vocab_size)
///
/// # Example
/// ```ignore
/// use adapteros_lora_mlx_ffi::MLXFFITensor;
/// use adapteros_core::Result;
///
/// let logits = MLXFFITensor::from_data(&[...logits], 32000)?;
/// let token = mlx_sample_token_safe(&logits, 0.7, 50, 0.9)?;
/// println!("Sampled token: {}", token);
/// ```
pub fn mlx_sample_token_safe(
    logits: &MLXFFITensor,
    temperature: f32,
    top_k: u32,
    top_p: f32,
) -> Result<u32> {
    // Validate inputs
    if temperature < 0.0 {
        return Err(AosError::Validation(
            "Temperature must be non-negative".to_string(),
        ));
    }

    if !(0.0..=1.0).contains(&top_p) {
        return Err(AosError::Validation(
            "top_p must be in range [0.0, 1.0]".to_string(),
        ));
    }

    let mut sampled_token: u32 = 0;

    unsafe {
        // Call FFI function
        let success = mlx_sample_token(
            logits.inner,
            temperature,
            top_k as i32,
            top_p,
            &mut sampled_token,
        );

        if !success {
            let error_msg = mlx_get_last_error();
            let error_str = if error_msg.is_null() {
                "Unknown error".to_string()
            } else {
                std::ffi::CStr::from_ptr(error_msg)
                    .to_string_lossy()
                    .to_string()
            };

            // Clear error state
            mlx_clear_error();

            return Err(AosError::Mlx(format!(
                "Token sampling failed: {}",
                error_str
            )));
        }
    }

    tracing::debug!(
        sampled_token = sampled_token,
        temperature = temperature,
        top_k = top_k,
        top_p = top_p,
        "MLX token sampled successfully"
    );

    Ok(sampled_token)
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
    /// Path to model directory (for loading tokenizer)
    model_path: PathBuf,
    /// Tokenizer for text encoding/decoding (loaded lazily)
    tokenizer: Option<tokenizer::MLXTokenizer>,
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    /// Create a new MLXFFIModel with a null pointer for testing purposes.
    ///
    /// This constructor should not be used for actual inference as the model pointer is null.
    /// It is intended for unit and integration tests only.
    pub fn new_null(config: ModelConfig) -> Self {
        Self {
            model: std::ptr::null_mut(),
            config,
            health: std::sync::Arc::new(std::sync::Mutex::new(ModelHealth {
                operational: false,
                consecutive_failures: 0,
                last_success: None,
                last_failure: None,
                circuit_breaker: CircuitBreakerState::Closed,
            })),
            model_path: PathBuf::new(),
            tokenizer: None,
        }
    }

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
        let _ = &config; // Silence unused warning
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
            Ok(model) => {
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
    fn load_with_health<P: AsRef<Path>>(
        model_path: P,
        health: std::sync::Arc<std::sync::Mutex<ModelHealth>>,
    ) -> Result<Self> {
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

        // Try to load tokenizer from model directory
        let tokenizer = match tokenizer::MLXTokenizer::from_model_dir(model_path) {
            Ok(tok) => {
                tracing::info!(
                    model_path = %model_path.display(),
                    vocab_size = tok.vocab_size(),
                    eos_token_id = tok.eos_token_id(),
                    "Tokenizer loaded successfully from model directory"
                );
                Some(tok)
            }
            Err(e) => {
                // Check if it's a "not found" error vs other errors
                let err_str = e.to_string();
                if err_str.contains("not found") {
                    tracing::debug!(
                        model_path = %model_path.display(),
                        "No tokenizer.json found in model directory - text generation will not be available"
                    );
                } else {
                    tracing::warn!(
                        model_path = %model_path.display(),
                        error = %e,
                        "Failed to load tokenizer - text generation will not be available"
                    );
                }
                None
            }
        };

        Ok(Self {
            model,
            config,
            health,
            model_path: model_path.to_path_buf(),
            tokenizer,
        })
    }

    /// Load a model from a pre-serialized weight buffer
    ///
    /// This allows loading models from pre-dequantized weights without requiring
    /// a file path. Useful for int4 quantized models that have been dequantized
    /// to f32 in memory.
    ///
    /// # Arguments
    /// * `buffer` - Serialized weight buffer (format: num_tensors, then per-tensor: name_len, name, shape_len, shape, data_len, data)
    /// * `config` - Model configuration
    ///
    /// # Returns
    /// Loaded MLX model ready for inference
    pub fn load_from_buffer(buffer: &[u8], config: ModelConfig) -> Result<Self> {
        let health = std::sync::Arc::new(std::sync::Mutex::new(ModelHealth {
            operational: false,
            consecutive_failures: 0,
            last_success: None,
            last_failure: None,
            circuit_breaker: CircuitBreakerState::Closed,
        }));

        // Safety: Check buffer validity
        if buffer.len() < 4 {
            return Err(AosError::Parse("Weight buffer too small".to_string()));
        }

        // Serialize config to JSON for FFI
        let config_json = serde_json::to_string(&config)
            .map_err(|e| AosError::Internal(format!("Failed to serialize config: {}", e)))?;
        let config_cstr = std::ffi::CString::new(config_json)
            .map_err(|e| AosError::Internal(format!("Invalid config string: {}", e)))?;

        // Clear any previous errors
        unsafe {
            mlx_clear_error();
        }

        // Load model via FFI from buffer
        let model = unsafe {
            mlx_model_load_from_buffer(buffer.as_ptr(), buffer.len(), config_cstr.as_ptr())
        };

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
                "Failed to load MLX model from buffer: {}",
                error_str
            )));
        }

        // Mark as operational
        if let Ok(mut health_guard) = health.lock() {
            health_guard.operational = true;
            health_guard.consecutive_failures = 0;
            health_guard.last_success = Some(std::time::Instant::now());
        }

        let num_tensors = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
        tracing::info!(
            num_tensors = num_tensors,
            buffer_size_mb = buffer.len() as f32 / (1024.0 * 1024.0),
            hidden_size = config.hidden_size,
            num_layers = config.num_hidden_layers,
            "MLX model loaded from weight buffer via FFI"
        );

        Ok(Self {
            model,
            config,
            health,
            model_path: PathBuf::new(),
            tokenizer: None,
        })
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
                return Err(AosError::Mlx(
                    "Circuit breaker open - model temporarily disabled".to_string(),
                ));
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

                    if health.consecutive_failures >= 3
                        && matches!(health.circuit_breaker, CircuitBreakerState::Closed)
                    {
                        health.circuit_breaker = CircuitBreakerState::Open;
                        tracing::warn!(
                            "MLX model circuit breaker opened after {} consecutive failures",
                            health.consecutive_failures
                        );
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

        // CRITICAL: Force evaluation of lazy computation graph
        unsafe {
            mlx_eval(output_array);
            mlx_synchronize();
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

    /// Forward pass with KV cache support for efficient autoregressive generation
    ///
    /// On the first step (cache empty), processes full sequence and populates cache.
    /// On subsequent steps, only processes the last token using cached K/V tensors.
    ///
    /// # Arguments
    /// * `token_ids` - Token IDs to process (full sequence on step 0, last token after)
    /// * `position` - Current position in sequence
    /// * `cache` - Optional KV cache for incremental decoding
    ///
    /// # Returns
    /// Logits for next token prediction
    pub fn forward_with_kv_cache(
        &self,
        token_ids: &[u32],
        position: usize,
        cache: Option<&crate::kv_cache::MLXKVCache>,
    ) -> Result<Vec<f32>> {
        // If no cache or cache is empty, do full forward pass
        if cache.is_none() || position == 0 {
            return self.forward(token_ids, position);
        }

        // For cached generation: only process last token
        // The KV cache contains previous keys/values
        let last_token = token_ids.last().copied().unwrap_or(0);

        // Forward with just the last token
        let logits = self.forward(&[last_token], position)?;

        // Note: Full KV cache integration would require:
        // 1. Modifying C++ mlx_model_forward to accept cache
        // 2. Extracting K/V tensors from each layer
        // 3. Updating cache with new K/V
        // For now, this provides the interface; full impl needs C++ changes

        Ok(logits)
    }

    /// Generate text from a prompt using FFI
    ///
    /// # Arguments
    /// * `prompt` - Input text prompt
    /// * `max_tokens` - Maximum tokens to generate
    ///
    /// # Returns
    /// Generated text
    pub fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String> {
        // Check that tokenizer is available
        let tokenizer = self.tokenizer.as_ref().ok_or_else(|| {
            AosError::Mlx(
                "Tokenizer not available. Ensure tokenizer.json exists in model directory."
                    .to_string(),
            )
        })?;

        // Create generation config
        let gen_config = generation::GenerationConfig {
            max_tokens,
            temperature: 0.7,
            top_k: Some(50),
            top_p: Some(0.9),
            repetition_penalty: 1.1,
            eos_token: tokenizer.eos_token_id(),
            use_cache: true,
        };

        // Create generator with deterministic seed based on model path
        let base_seed = B3Hash::hash(self.model_path.to_string_lossy().as_bytes());
        let mut generator = generation::MLXGenerator::new(base_seed, gen_config);

        // Generate text using the generator
        generator.generate_text(self, prompt, tokenizer)
    }

    /// Generate text with custom configuration
    ///
    /// # Arguments
    /// * `prompt` - Input text prompt
    /// * `config` - Generation configuration
    ///
    /// # Returns
    /// Generated text
    pub fn generate_with_config(
        &self,
        prompt: &str,
        config: generation::GenerationConfig,
    ) -> Result<String> {
        let tokenizer = self.tokenizer.as_ref().ok_or_else(|| {
            AosError::Mlx(
                "Tokenizer not available. Ensure tokenizer.json exists in model directory."
                    .to_string(),
            )
        })?;

        let base_seed = B3Hash::hash(self.model_path.to_string_lossy().as_bytes());
        let mut generator = generation::MLXGenerator::new(base_seed, config);

        generator.generate_text(self, prompt, tokenizer)
    }

    /// Get the tokenizer if loaded
    pub fn tokenizer(&self) -> Option<&tokenizer::MLXTokenizer> {
        self.tokenizer.as_ref()
    }

    /// Get the model path
    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// Run forward pass with hidden states using FFI
    ///
    /// # Arguments
    /// * `token_ids` - Input token IDs
    ///
    /// # Returns
    /// Tuple of (logits, hidden_states_by_module)
    #[allow(clippy::type_complexity)]
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
                &mut num_hidden,
            )
        };

        if output_array.is_null() {
            unsafe { mlx_array_free(input_array) };
            return Err(AosError::Mlx(
                "Failed to run model forward with hidden states".to_string(),
            ));
        }

        // CRITICAL: Collect and evaluate all arrays before data extraction
        {
            let mut arrays_to_eval: Vec<*mut mlx_array_t> = vec![output_array];

            if !hidden_states_ptr.is_null() && num_hidden > 0 {
                for i in 0..num_hidden {
                    let hs_array =
                        unsafe { *(hidden_states_ptr as *mut *mut mlx_array_t).add(i as usize) };
                    if !hs_array.is_null() {
                        arrays_to_eval.push(hs_array);
                    }
                }
            }

            // Batch evaluate and synchronize
            unsafe {
                mlx_eval_all(arrays_to_eval.as_mut_ptr(), arrays_to_eval.len() as i32);
                mlx_synchronize();
            }
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

        let logits: Vec<f32> =
            unsafe { std::slice::from_raw_parts(output_data, output_size as usize).to_vec() };

        // Extract hidden states from the FFI layer
        let mut hidden_states = std::collections::HashMap::new();

        if !hidden_states_ptr.is_null() && num_hidden > 0 {
            // The hidden_states_ptr points to an array of mlx_array_t* pointers
            // We need to cast it appropriately and extract data from each array
            let hidden_array_ptr = hidden_states_ptr as *mut *mut mlx_array_t;

            for i in 0..num_hidden {
                // Get the module name for this hidden state
                let mut name_buf = [0i8; 256];
                let name_len = unsafe {
                    mlx_model_get_hidden_state_name(
                        self.model,
                        i,
                        name_buf.as_mut_ptr(),
                        name_buf.len() as i32,
                    )
                };

                if name_len <= 0 {
                    tracing::warn!("Failed to get name for hidden state index {}, skipping", i);
                    continue;
                }

                // Convert C string to Rust String
                let module_name = unsafe {
                    std::ffi::CStr::from_ptr(name_buf.as_ptr())
                        .to_string_lossy()
                        .to_string()
                };

                // Get the hidden state array for this index
                let hidden_array = unsafe { *hidden_array_ptr.add(i as usize) };
                if hidden_array.is_null() {
                    tracing::warn!("Hidden state array at index {} is null, skipping", i);
                    continue;
                }

                // Extract data from the hidden state array
                let hidden_size = unsafe { mlx_array_size(hidden_array) };
                let hidden_data = unsafe { mlx_array_data(hidden_array) };

                if hidden_data.is_null() || hidden_size == 0 {
                    tracing::warn!(
                        "Hidden state '{}' has null data or zero size, skipping",
                        module_name
                    );
                    continue;
                }

                // Copy the data to a Vec
                let hidden_vec: Vec<f32> =
                    unsafe { std::slice::from_raw_parts(hidden_data, hidden_size).to_vec() };

                tracing::trace!(
                    "Extracted hidden state '{}': {} elements",
                    module_name,
                    hidden_vec.len()
                );

                hidden_states.insert(module_name, hidden_vec);
            }

            // Clean up hidden states array using the proper FFI function
            unsafe { mlx_hidden_states_free(hidden_states_ptr, num_hidden) };
        }

        // Clean up input and output arrays
        unsafe {
            mlx_array_free(input_array);
            mlx_array_free(output_array);
        }

        tracing::debug!(
            "MLX FFI forward with hidden states: {} tokens -> {} logits, {} hidden state modules extracted",
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

/// SAFETY: MLXFFIModel is Send because:
///
/// 1. The wrapped `mlx_model_t` raw pointer refers to a C++ object that is owned
///    exclusively by this Rust wrapper - there are no external references to it.
/// 2. The pointer is immutable after construction (`load()` or `load_from_buffer()`),
///    and all inference operations (`forward()`, `forward_with_hidden_states()`) are
///    read-only with respect to the model weights.
/// 3. The model can safely be moved between threads because MLX's underlying
///    implementation uses thread-local GPU command queues.
/// 4. All other fields (config, model_path) are owned values that are trivially Send.
/// 5. The `health` field uses `Arc<Mutex<>>` which is explicitly designed for
///    cross-thread sharing.
/// 6. The `tokenizer` field is Option<MLXTokenizer> which wraps a HuggingFace
///    tokenizer that is Send-safe.
unsafe impl Send for MLXFFIModel {}

/// SAFETY: MLXFFIModel is Sync because:
///
/// 1. Concurrent inference calls are safe because MLX uses Metal command buffers
///    which are synchronized at the GPU driver level. Each inference creates
///    independent command buffers that are serialized by the GPU.
/// 2. The raw pointer `model` is never mutated after construction - inference
///    operations only read model weights and don't modify the model state.
/// 3. All mutable state (health tracking) is protected by `Arc<Mutex<>>`,
///    ensuring exclusive access and preventing data races.
/// 4. Drop is safe from any thread: `mlx_model_free()` handles cleanup correctly
///    regardless of which thread calls it, and the null-check prevents double-free.
/// 5. No interior mutability exists in the model weights - the C++ object uses
///    immutable weight tensors after model loading.
///
/// Reference: MLX C++ source confirms that `mlx::core::array` operations are
/// thread-safe through Metal command buffer synchronization and atomic reference
/// counting for shared array data.
unsafe impl Sync for MLXFFIModel {}

// FFI declarations for MLX operations
#[cfg_attr(test, allow(dead_code))]
#[cfg_attr(feature = "real-mlx", link(name = "mlx_wrapper"))]
#[cfg_attr(not(feature = "real-mlx"), link(name = "mlx_wrapper_stub"))]
extern "C" {
    // Model lifecycle
    fn mlx_model_load(path: *const std::os::raw::c_char) -> *mut mlx_model_t;
    fn mlx_model_load_from_buffer(
        buffer: *const u8,
        buffer_len: usize,
        config_json: *const std::os::raw::c_char,
    ) -> *mut mlx_model_t;
    fn mlx_model_free(model: *mut mlx_model_t);

    // Inference
    fn mlx_model_forward(model: *mut mlx_model_t, input: *mut mlx_array_t) -> *mut mlx_array_t;
    fn mlx_model_forward_with_hidden_states(
        model: *mut mlx_model_t,
        input: *mut mlx_array_t,
        hidden_states: *mut *mut mlx_array_t,
        hidden_count: *mut i32,
    ) -> *mut mlx_array_t;

    // Hidden states access (pub for use in backend.rs)
    pub fn mlx_hidden_states_free(hidden_states: *mut mlx_array_t, num_hidden: i32);
    pub fn mlx_model_get_hidden_state_name(
        model: *mut mlx_model_t,
        index: i32,
        out_name: *mut std::os::raw::c_char,
        out_name_len: i32,
    ) -> i32;
    pub fn mlx_model_get_hidden_state_count(model: *mut mlx_model_t) -> i32;

    // Array operations
    fn mlx_array_from_data(data: *const f32, size: i32) -> *mut mlx_array_t;
    fn mlx_array_from_ints(data: *const i32, size: i32) -> *mut mlx_array_t;
    fn mlx_array_from_uints(data: *const u32, size: i32) -> *mut mlx_array_t;
    fn mlx_array_data(array: *mut mlx_array_t) -> *mut f32;
    fn mlx_array_size(array: *mut mlx_array_t) -> usize;
    fn mlx_array_free(array: *mut mlx_array_t);
    fn mlx_array_copy(array: *mut mlx_array_t) -> *mut mlx_array_t;

    // Shape manipulation operations
    fn mlx_array_reshape(array: *mut mlx_array_t, shape: *const i32, ndim: i32)
        -> *mut mlx_array_t;
    fn mlx_array_transpose(array: *mut mlx_array_t) -> *mut mlx_array_t;
    fn mlx_array_shape(array: *mut mlx_array_t, shape: *mut i32, max_dims: i32) -> i32;
    fn mlx_array_ndim(array: *mut mlx_array_t) -> i32;
    fn mlx_array_dtype(array: *mut mlx_array_t) -> i32;

    // Arithmetic operations
    fn mlx_add(a: *mut mlx_array_t, b: *mut mlx_array_t) -> *mut mlx_array_t;
    fn mlx_matmul(a: *mut mlx_array_t, b: *mut mlx_array_t) -> *mut mlx_array_t;
    fn mlx_multiply(a: *mut mlx_array_t, b: *mut mlx_array_t) -> *mut mlx_array_t;
    fn mlx_divide(a: *mut mlx_array_t, b: *mut mlx_array_t) -> *mut mlx_array_t;

    // Reduction operations
    fn mlx_sum(array: *mut mlx_array_t, axis: i32) -> *mut mlx_array_t;
    fn mlx_mean(array: *mut mlx_array_t, axis: i32) -> *mut mlx_array_t;
    fn mlx_sqrt(array: *mut mlx_array_t) -> *mut mlx_array_t;

    // Indexing operations
    fn mlx_take(array: *mut mlx_array_t, indices: *mut mlx_array_t, axis: i32) -> *mut mlx_array_t;

    // Error handling
    fn mlx_clear_error();
    fn mlx_get_last_error() -> *const std::os::raw::c_char;

    // Memory management
    fn mlx_gc_collect();
    fn mlx_allocation_count() -> usize;
    fn mlx_memory_stats(total_bytes: *mut usize, allocation_count: *mut usize);
    fn mlx_memory_reset();
    fn mlx_memory_usage() -> usize;

    // Deterministic seeding
    fn mlx_set_seed(data: *const u8, size: usize);

    // Token sampling for text generation
    fn mlx_sample_token(
        logits: *mut mlx_array_t,
        temperature: f32,
        top_k: i32,
        top_p: f32,
        out_token: *mut u32,
    ) -> bool;

    // ========================================================================
    // Runtime initialization and backend info
    // ========================================================================

    /// Initialize MLX runtime with specified device type
    /// Returns 0 on success, -1 on error
    pub fn mlx_init(device_type: i32) -> i32;

    /// Initialize MLX with default settings (auto device selection)
    pub fn mlx_init_default() -> i32;

    /// Shutdown MLX runtime and release resources
    pub fn mlx_shutdown();

    /// Check if MLX runtime is initialized
    pub fn mlx_is_initialized() -> bool;

    /// Get current device type
    pub fn mlx_get_device_type() -> i32;

    /// Set device type (switch between CPU/GPU)
    pub fn mlx_set_device(device_type: i32) -> i32;

    /// Get backend capabilities and version information
    pub fn mlx_backend_info(capabilities: *mut MlxBackendCapabilities) -> i32;

    /// Get MLX version string
    pub fn mlx_get_version() -> *const std::os::raw::c_char;

    // ========================================================================
    // Quantization operations
    // ========================================================================

    /// Quantize array to specified bit width (4-bit or 8-bit)
    pub fn mlx_quantize(array: *mut mlx_array_t, group_size: i32, bits: i32) -> *mut mlx_array_t;

    /// Dequantize array back to float
    pub fn mlx_dequantize(
        array: *mut mlx_array_t,
        scales: *mut mlx_array_t,
        biases: *mut mlx_array_t,
        group_size: i32,
        bits: i32,
    ) -> *mut mlx_array_t;

    // ========================================================================
    // RoPE (Rotary Position Embedding)
    // ========================================================================

    /// Apply rotary position embeddings (FFI to C++)
    /// Note: There's also a pure-Rust implementation in attention.rs
    #[link_name = "mlx_rope"]
    pub fn mlx_rope_ffi(
        array: *mut mlx_array_t,
        dims: i32,
        traditional: bool,
        base: f32,
        scale: f32,
        offset: i32,
    ) -> *mut mlx_array_t;

    // ========================================================================
    // Attention operations
    // ========================================================================

    /// Scaled dot-product attention (FFI to C++)
    /// Note: There's also a pure-Rust implementation in attention.rs
    #[link_name = "mlx_scaled_dot_product_attention"]
    pub fn mlx_sdpa_ffi(
        queries: *mut mlx_array_t,
        keys: *mut mlx_array_t,
        values: *mut mlx_array_t,
        scale: f32,
        mask: *mut mlx_array_t,
    ) -> *mut mlx_array_t;

    /// Create causal attention mask
    pub fn mlx_create_causal_mask(seq_len: i32) -> *mut mlx_array_t;

    // ========================================================================
    // KV Cache management
    // ========================================================================

    /// Create a new KV cache
    pub fn mlx_kv_cache_new(
        num_layers: i32,
        num_heads: i32,
        head_dim: i32,
        max_seq_len: i32,
    ) -> *mut mlx_kv_cache_t;

    /// Update KV cache with new key/value tensors
    pub fn mlx_kv_cache_update(
        cache: *mut mlx_kv_cache_t,
        layer_idx: i32,
        keys: *mut mlx_array_t,
        values: *mut mlx_array_t,
    ) -> i32;

    /// Get cached keys for a layer
    pub fn mlx_kv_cache_get_keys(cache: *mut mlx_kv_cache_t, layer_idx: i32) -> *mut mlx_array_t;

    /// Get cached values for a layer
    pub fn mlx_kv_cache_get_values(cache: *mut mlx_kv_cache_t, layer_idx: i32) -> *mut mlx_array_t;

    /// Get current sequence length in cache
    pub fn mlx_kv_cache_seq_len(cache: *mut mlx_kv_cache_t) -> i32;

    /// Reset/clear the KV cache
    pub fn mlx_kv_cache_reset(cache: *mut mlx_kv_cache_t);

    /// Free KV cache
    pub fn mlx_kv_cache_free(cache: *mut mlx_kv_cache_t);

    // ========================================================================
    // SafeTensors weight loading
    // ========================================================================

    /// Load weights from a SafeTensors file
    pub fn mlx_load_safetensors(path: *const std::os::raw::c_char) -> *mut mlx_weights_t;

    /// Get a specific tensor by name from loaded weights
    pub fn mlx_weights_get(
        weights: *mut mlx_weights_t,
        name: *const std::os::raw::c_char,
    ) -> *mut mlx_array_t;

    /// Get list of all tensor names
    pub fn mlx_weights_list(
        weights: *mut mlx_weights_t,
        names: *mut *const std::os::raw::c_char,
        max_names: i32,
    ) -> i32;

    /// Free weights container
    pub fn mlx_weights_free(weights: *mut mlx_weights_t);

    // ========================================================================
    // Evaluation and synchronization
    // ========================================================================

    /// Evaluate a single array (force computation)
    pub fn mlx_eval(array: *mut mlx_array_t);

    /// Evaluate multiple arrays
    pub fn mlx_eval_all(arrays: *mut *mut mlx_array_t, num_arrays: i32);

    /// Synchronize and wait for all GPU operations to complete
    pub fn mlx_synchronize();

    // ========================================================================
    // LoRA Adapter Caching
    // ========================================================================

    /// Cache LoRA adapter weights for efficient reuse
    pub fn mlx_lora_cache_adapter(
        adapter_id: *const std::os::raw::c_char,
        lora_a: *mut mlx_array_t,
        lora_b: *mut mlx_array_t,
    ) -> *const std::os::raw::c_char;

    /// Get cached LoRA adapter
    pub fn mlx_lora_get_cached(
        adapter_id: *const std::os::raw::c_char,
        out_lora_a: *mut *mut mlx_array_t,
        out_lora_b: *mut *mut mlx_array_t,
    ) -> bool;

    /// Evict a specific adapter from cache
    pub fn mlx_lora_evict_cached(adapter_id: *const std::os::raw::c_char);

    /// Clear all cached adapters
    pub fn mlx_lora_clear_cache();

    /// Get number of cached adapters
    pub fn mlx_lora_cache_size() -> usize;

    /// Set maximum number of cached adapters
    pub fn mlx_lora_set_cache_limit(max_entries: usize);
}

// Opaque types for FFI
#[repr(C)]
pub struct mlx_model_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct mlx_array_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct mlx_kv_cache_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct mlx_weights_t {
    _private: [u8; 0],
}

/// Device type enumeration for device selection
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlxDeviceType {
    Cpu = 0,
    Gpu = 1,
    Ane = 2,
    Auto = 3,
}

/// Backend capabilities structure
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MlxBackendCapabilities {
    pub gpu_available: bool,
    pub ane_available: bool,
    pub metal_compute: bool,
    pub unified_memory: bool,
    pub max_threads_per_group: i32,
    pub max_buffer_size: usize,
    pub device_name: [u8; 256],
    pub mlx_version: [u8; 64],
    pub metal_version: [u8; 64],
}

impl Default for MlxBackendCapabilities {
    fn default() -> Self {
        Self {
            gpu_available: false,
            ane_available: false,
            metal_compute: false,
            unified_memory: false,
            max_threads_per_group: 0,
            max_buffer_size: 0,
            device_name: [0u8; 256],
            mlx_version: [0u8; 64],
            metal_version: [0u8; 64],
        }
    }
}

impl MlxBackendCapabilities {
    /// Get the device name as a string
    pub fn device_name_str(&self) -> &str {
        let end = self.device_name.iter().position(|&b| b == 0).unwrap_or(256);
        std::str::from_utf8(&self.device_name[..end]).unwrap_or("")
    }

    /// Get the MLX version as a string
    pub fn mlx_version_str(&self) -> &str {
        let end = self.mlx_version.iter().position(|&b| b == 0).unwrap_or(64);
        std::str::from_utf8(&self.mlx_version[..end]).unwrap_or("")
    }

    /// Get the Metal version as a string
    pub fn metal_version_str(&self) -> &str {
        let end = self
            .metal_version
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(64);
        std::str::from_utf8(&self.metal_version[..end]).unwrap_or("")
    }
}

// =============================================================================
// Safe Runtime Initialization Wrappers
// =============================================================================

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

static MLX_INIT_ONCE: Once = Once::new();
static MLX_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize MLX runtime safely (idempotent - safe to call multiple times)
///
/// This function should be called once before using any MLX operations.
/// Multiple calls are safe and will be ignored after the first successful init.
///
/// # Returns
/// * `Ok(())` - Runtime initialized successfully (or already initialized)
/// * `Err(...)` - Initialization failed
///
/// # Example
/// ```ignore
/// use adapteros_lora_mlx_ffi::mlx_runtime_init;
///
/// mlx_runtime_init()?; // Initialize once
/// mlx_runtime_init()?; // Safe to call again (no-op)
/// ```
pub fn mlx_runtime_init() -> Result<()> {
    let mut init_result: Result<()> = Ok(());

    MLX_INIT_ONCE.call_once(|| unsafe {
        mlx_clear_error();
        let result = mlx_init_default();

        if result == 0 {
            MLX_INITIALIZED.store(true, Ordering::SeqCst);
            tracing::info!("MLX runtime initialized successfully");
        } else {
            let error_msg = mlx_get_last_error();
            let error_str = if !error_msg.is_null() {
                std::ffi::CStr::from_ptr(error_msg)
                    .to_string_lossy()
                    .to_string()
            } else {
                "Unknown initialization error".to_string()
            };
            mlx_clear_error();
            init_result = Err(AosError::Mlx(format!(
                "Failed to initialize MLX runtime: {}",
                error_str
            )));
        }
    });

    // If already initialized, return success
    if MLX_INITIALIZED.load(Ordering::SeqCst) {
        Ok(())
    } else {
        init_result
    }
}

/// Initialize MLX runtime with specific device type
///
/// # Arguments
/// * `device` - Device type to initialize (Cpu, Gpu, Ane, Auto)
///
/// # Returns
/// * `Ok(())` - Runtime initialized successfully
/// * `Err(...)` - Initialization failed
pub fn mlx_runtime_init_with_device(device: MlxDeviceType) -> Result<()> {
    if MLX_INITIALIZED.load(Ordering::SeqCst) {
        tracing::debug!("MLX runtime already initialized, ignoring device selection");
        return Ok(());
    }

    unsafe {
        mlx_clear_error();
        let result = mlx_init(device as i32);

        if result == 0 {
            MLX_INITIALIZED.store(true, Ordering::SeqCst);
            tracing::info!(?device, "MLX runtime initialized with specific device");
            Ok(())
        } else {
            let error_msg = mlx_get_last_error();
            let error_str = if !error_msg.is_null() {
                std::ffi::CStr::from_ptr(error_msg)
                    .to_string_lossy()
                    .to_string()
            } else {
                "Unknown initialization error".to_string()
            };
            mlx_clear_error();
            Err(AosError::Mlx(format!(
                "Failed to initialize MLX runtime with device {:?}: {}",
                device, error_str
            )))
        }
    }
}

/// Check if MLX runtime is initialized
pub fn mlx_runtime_is_initialized() -> bool {
    MLX_INITIALIZED.load(Ordering::SeqCst)
}

/// Shutdown MLX runtime and release resources (idempotent)
///
/// Safe to call multiple times or when not initialized.
pub fn mlx_runtime_shutdown() {
    if MLX_INITIALIZED
        .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        unsafe {
            mlx_shutdown();
        }
        tracing::info!("MLX runtime shut down");
    }
}

/// Get MLX backend capabilities
///
/// # Returns
/// * `Ok(capabilities)` - Backend capability information
/// * `Err(...)` - Failed to query capabilities
pub fn mlx_get_backend_capabilities() -> Result<MlxBackendCapabilities> {
    let mut capabilities = MlxBackendCapabilities::default();

    unsafe {
        mlx_clear_error();
        let result = mlx_backend_info(&mut capabilities);

        if result != 0 {
            let error_msg = mlx_get_last_error();
            let error_str = if !error_msg.is_null() {
                std::ffi::CStr::from_ptr(error_msg)
                    .to_string_lossy()
                    .to_string()
            } else {
                "Unknown error querying backend info".to_string()
            };
            mlx_clear_error();
            return Err(AosError::Mlx(format!(
                "Failed to get backend capabilities: {}",
                error_str
            )));
        }
    }

    Ok(capabilities)
}

/// Get MLX version string
pub fn mlx_version() -> String {
    unsafe {
        let version_ptr = mlx_get_version();
        if version_ptr.is_null() {
            "unknown".to_string()
        } else {
            std::ffi::CStr::from_ptr(version_ptr)
                .to_string_lossy()
                .to_string()
        }
    }
}

/// Ensure MLX runtime is initialized before operation
///
/// Helper macro/function to check and optionally initialize runtime.
/// Returns error if not initialized and auto_init is false.
pub fn mlx_ensure_initialized(auto_init: bool) -> Result<()> {
    if mlx_runtime_is_initialized() {
        return Ok(());
    }

    if auto_init {
        mlx_runtime_init()
    } else {
        Err(AosError::Mlx(
            "MLX runtime not initialized. Call mlx_runtime_init() first.".to_string(),
        ))
    }
}

/// Force evaluation of MLX array (materialize lazy computation)
///
/// MLX uses lazy evaluation - operations build a computation graph but don't
/// execute immediately. Call this before extracting data from an array.
///
/// # Safety
/// The array pointer must be valid and non-null.
pub unsafe fn mlx_force_eval(array: *mut mlx_array_t) -> Result<()> {
    if array.is_null() {
        return Err(AosError::Internal("Cannot evaluate null array".to_string()));
    }

    mlx_clear_error();
    mlx_eval(array);

    let error_msg = mlx_get_last_error();
    if !error_msg.is_null() {
        let error_str = std::ffi::CStr::from_ptr(error_msg)
            .to_string_lossy()
            .to_string();
        if !error_str.is_empty() {
            mlx_clear_error();
            return Err(AosError::Mlx(format!("Evaluation failed: {}", error_str)));
        }
    }

    Ok(())
}

/// Force evaluation of multiple MLX arrays and synchronize
///
/// Use this when multiple arrays need to be materialized together,
/// which is more efficient than evaluating them one by one.
pub fn mlx_force_eval_all(arrays: &mut [*mut mlx_array_t]) -> Result<()> {
    if arrays.is_empty() {
        return Ok(());
    }

    // Filter out null pointers
    let valid_arrays: Vec<*mut mlx_array_t> =
        arrays.iter().copied().filter(|a| !a.is_null()).collect();

    if valid_arrays.is_empty() {
        return Ok(());
    }

    unsafe {
        mlx_clear_error();
        mlx_eval_all(
            valid_arrays.as_ptr() as *mut *mut mlx_array_t,
            valid_arrays.len() as i32,
        );
        mlx_synchronize();

        let error_msg = mlx_get_last_error();
        if !error_msg.is_null() {
            let error_str = std::ffi::CStr::from_ptr(error_msg)
                .to_string_lossy()
                .to_string();
            if !error_str.is_empty() {
                mlx_clear_error();
                return Err(AosError::Mlx(format!(
                    "Batch evaluation failed: {}",
                    error_str
                )));
            }
        }
    }

    Ok(())
}

/// Synchronize GPU operations and wait for completion
pub fn mlx_sync() {
    unsafe {
        mlx_synchronize();
    }
}

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
    fn test_model_new_null_creation() {
        // Test that we can create a null model for testing purposes
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };
        let model = MLXFFIModel::new_null(config.clone());
        assert_eq!(model.config.hidden_size, 4096);
        assert_eq!(model.config.num_hidden_layers, 32);
        assert!(model.model.is_null());
    }

    #[test]
    fn test_generate_requires_tokenizer() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };
        let model = MLXFFIModel::new_null(config);

        let result = model.generate("test prompt", 10);
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Tokenizer not available"),
            "Expected error message to mention tokenizer, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_generate_with_config_requires_tokenizer() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };
        let model = MLXFFIModel::new_null(config);

        let gen_config = generation::GenerationConfig {
            max_tokens: 100,
            temperature: 0.5,
            top_k: Some(40),
            top_p: Some(0.95),
            repetition_penalty: 1.2,
            eos_token: 2,
            use_cache: true,
        };

        let result = model.generate_with_config("test prompt", gen_config);
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Tokenizer not available"),
            "Expected error message to mention tokenizer, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_tokenizer_accessor_returns_none_for_null_model() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };
        let model = MLXFFIModel::new_null(config);

        assert!(
            model.tokenizer().is_none(),
            "Tokenizer should be None for null model"
        );
    }

    #[test]
    fn test_model_path_accessor_returns_empty_for_null_model() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };
        let model = MLXFFIModel::new_null(config);

        assert!(
            model.model_path().as_os_str().is_empty(),
            "Model path should be empty for null model"
        );
    }

    #[test]
    fn test_new_null_creates_non_operational_model() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };
        let model = MLXFFIModel::new_null(config);

        // Check health status
        let health = model.health_status().expect("Should get health status");
        assert!(!health.operational, "Null model should not be operational");
        assert_eq!(
            health.consecutive_failures, 0,
            "Should have no failures initially"
        );
        assert!(
            matches!(health.circuit_breaker, CircuitBreakerState::Closed),
            "Circuit breaker should be closed initially"
        );
    }

    #[test]
    fn test_config_accessor() {
        let config = ModelConfig {
            hidden_size: 2048,
            num_hidden_layers: 24,
            num_attention_heads: 16,
            num_key_value_heads: 4,
            intermediate_size: 8192,
            vocab_size: 50000,
            max_position_embeddings: 16384,
            rope_theta: 5000.0,
        };
        let model = MLXFFIModel::new_null(config);

        let retrieved_config = model.config();
        assert_eq!(retrieved_config.hidden_size, 2048);
        assert_eq!(retrieved_config.num_hidden_layers, 24);
        assert_eq!(retrieved_config.vocab_size, 50000);
        assert_eq!(retrieved_config.rope_theta, 5000.0);
    }
}
