//! MLX FFI integration for AdapterOS
//!
//! This crate provides C FFI bindings for MLX's C++ API, avoiding PyO3 dependency issues.
//! It implements the same interface as the PyO3-based MLX crate but uses direct C++ calls.

#![allow(unexpected_cfgs)]
#![allow(deprecated)]
#![allow(unused_mut)]
#![allow(clippy::needless_return)]
#![allow(clippy::type_complexity)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(clippy::too_many_arguments)]

use adapteros_core::{AosError, B3Hash, Result};
use std::path::{Path, PathBuf};

// Pure Rust mlx-rs array abstraction (new backend)
pub mod array;

// Pure Rust model implementation using mlx-rs (new backend)
#[cfg(feature = "mlx-rs-backend")]
pub mod model;

// Legacy C++ FFI modules (deprecated - will be removed)
pub mod attention;
pub mod backend;
pub mod embedding;
pub mod ffi_error;
pub mod generation;
pub mod kv_cache;
pub mod liquid;
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
pub use array::{Dtype, MlxArray};
pub use attention::{
    mlx_multihead_attention, mlx_multihead_attention_legacy, mlx_rope,
    mlx_scaled_dot_product_attention, AttentionConfig, RoPEFrequencies,
};
pub use backend::MLXFFIBackend;
pub use embedding::{EmbeddingConfig, MLXEmbeddingModel};
pub use generation::{GenerationConfig, GenerationResult, MLXGenerator};
pub use kv_cache::{CacheLayer, CacheStats, KVCacheConfig, MLXKVCache, PrefixKvTensors};
pub use liquid::blend_and_forward_mlx;
pub use lora::{LoRAAdapter, LoRAConfig};
pub use memory_pool::{MLXMemoryPool, MLXMemoryPoolConfig, MemoryPoolStats, MemoryPressureEvent};
#[cfg(feature = "mlx-rs-backend")]
pub use model::{MlxRsModel, MlxRsModelConfig};
pub use quantization::{
    MLXQuantizer, QuantizationConfig, QuantizationMetadata, QuantizationStats, QuantizedTensor,
    WeightCompressor,
};
pub use routing::apply_multi_lora;
pub use safetensors_loader::{SafetensorsLoader, TensorInfo};
pub use tensor::MLXFFITensor;
pub use tokenizer::MLXTokenizer;
pub use unified_loader::{LoadStrategy, TensorMetadata, UnifiedSafeTensorsLoader};

// Re-export FFI error utilities for external use
pub use ffi_error::{
    check_ffi_ptr, check_ffi_result, clear_ffi_error, get_and_clear_ffi_error, get_ffi_error_or,
    MlxArrayGuard, MlxArrayVecGuard,
};

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

    ffi_error::clear_ffi_error();

    unsafe {
        mlx_set_seed(seed.as_ptr(), seed.len());
    }

    // Check if there was an error during seed setting
    if let Some(error_str) = ffi_error::get_and_clear_ffi_error() {
        // Ignore specific expected "error" that's not really an error
        if error_str != "Invalid seed: pointer is null or length is 0" {
            return Err(AosError::Mlx(format!(
                "Failed to set MLX seed: {}",
                error_str
            )));
        }
    }

    tracing::debug!(
        seed_len = seed.len(),
        "MLX backend seeded for deterministic dropout/sampling"
    );

    Ok(())
}

/// Internal shared implementation for token sampling (C++ FFI backend)
///
/// This function contains the core sampling logic using the MLX C++ FFI backend.
/// It performs the actual token sampling using the native MLX library.
#[cfg(not(feature = "mlx-rs-backend"))]
fn sample_token_impl(
    logits: &MLXFFITensor,
    temperature: f32,
    top_k: u32,
    top_p: f32,
) -> Result<u32> {
    validate_sampling_params(temperature, top_p)?;

    let mut sampled_token: u32 = 0;

    let success = unsafe {
        mlx_sample_token(
            logits.inner,
            temperature,
            top_k as i32,
            top_p,
            &mut sampled_token,
        )
    };

    if !success {
        let error = ffi_error::get_ffi_error_or("Unknown error");
        return Err(AosError::Mlx(format!("Token sampling failed: {}", error)));
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

/// Internal shared implementation for token sampling (mlx-rs backend)
///
/// Uses pure Rust mlx-rs API for sampling. Currently implements greedy decoding.
/// Full temperature/top-k/top-p sampling to be implemented.
#[cfg(feature = "mlx-rs-backend")]
fn sample_token_impl(
    logits: &MLXFFITensor,
    temperature: f32,
    _top_k: u32,
    top_p: f32,
) -> Result<u32> {
    validate_sampling_params(temperature, top_p)?;

    // Use argmax for greedy decoding (temperature ~= 0)
    if temperature < 0.01 {
        let result = logits.as_mlx_array().argmax(-1, false)?;
        let tokens = result.to_vec_i32()?;
        return Ok(tokens.first().copied().unwrap_or(0) as u32);
    }

    // For now, use argmax sampling (full sampling with top-k/top-p to be implemented)
    // TODO: Implement full mlx-rs based sampling with temperature/top-k/top-p
    let result = logits.as_mlx_array().argmax(-1, false)?;
    let tokens = result.to_vec_i32()?;
    Ok(tokens.first().copied().unwrap_or(0) as u32)
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
    sample_token_impl(logits, temperature, top_k, top_p)
}

/// Validate sampling parameters for token generation.
#[inline]
fn validate_sampling_params(temperature: f32, top_p: f32) -> Result<()> {
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

    Ok(())
}

/// Sample token from logits with confidence and alternative tokens metadata
///
/// Enhanced version of token sampling that returns probability information.
///
/// # Arguments
/// * `logits` - Model output logits tensor
/// * `temperature` - Sampling temperature (0.0 = greedy, higher = more random)
/// * `top_k` - Keep only top K tokens (0 = disabled)
/// * `top_p` - Nucleus sampling threshold (0.0-1.0, 0.0 = disabled)
/// * `repetition_penalty` - Penalty for repeated tokens (1.0 = no penalty)
/// * `seed` - Random seed for reproducibility
///
/// # Returns
/// Tuple of (token_id, confidence, alternatives)
/// - token_id: sampled token
/// - confidence: probability of the sampled token
/// - alternatives: vector of top alternative tokens with probabilities
#[cfg(not(feature = "mlx-rs-backend"))]
pub fn mlx_sample_token_with_metadata_safe(
    logits: &MLXFFITensor,
    temperature: f32,
    top_k: u32,
    top_p: f32,
    repetition_penalty: f32,
    seed: u64,
) -> Result<(u32, f32, Vec<(u32, f32)>)> {
    validate_sampling_params(temperature, top_p)?;

    let config = MlxSamplerConfig {
        temperature,
        top_p,
        top_k: top_k as i32,
        repetition_penalty,
        seed,
    };

    let mut metadata = MlxTokenMetadata {
        confidence: 0.0,
        alternatives: std::ptr::null_mut(),
        num_alternatives: 0,
    };

    unsafe {
        let token_id = mlx_sample_token_with_metadata(logits.inner, &config, &mut metadata);

        if token_id < 0 {
            let error = ffi_error::get_ffi_error_or("Unknown error");
            return Err(AosError::Mlx(format!(
                "Token sampling with metadata failed: {}",
                error
            )));
        }

        let confidence = metadata.confidence;

        // Extract alternatives
        let mut alternatives = Vec::new();
        if !metadata.alternatives.is_null() && metadata.num_alternatives > 0 {
            let alts_slice = std::slice::from_raw_parts(
                metadata.alternatives,
                metadata.num_alternatives as usize,
            );
            for alt in alts_slice {
                alternatives.push((alt.token_id, alt.prob));
            }
        }

        // Free metadata
        mlx_free_token_metadata(&mut metadata);

        tracing::debug!(
            token_id = token_id,
            confidence = confidence,
            num_alternatives = alternatives.len(),
            "MLX token sampled with metadata"
        );

        Ok((token_id as u32, confidence, alternatives))
    }
}

/// Sample token from logits with metadata (mlx-rs backend)
#[cfg(feature = "mlx-rs-backend")]
pub fn mlx_sample_token_with_metadata_safe(
    logits: &MLXFFITensor,
    temperature: f32,
    _top_k: u32,
    top_p: f32,
    _repetition_penalty: f32,
    _seed: u64,
) -> Result<(u32, f32, Vec<(u32, f32)>)> {
    validate_sampling_params(temperature, top_p)?;

    // Get argmax token
    let arr = logits.as_mlx_array();
    let result = arr.argmax(-1, false)?;
    let tokens = result.to_vec_i32()?;
    let token_id = tokens.first().copied().unwrap_or(0) as u32;

    // Compute softmax to get probabilities for confidence
    let probs = arr.softmax(-1)?;
    let probs_vec = probs.to_vec_f32()?;

    // Get confidence (probability of selected token)
    let confidence = probs_vec.get(token_id as usize).copied().unwrap_or(0.0);

    // Get top 5 alternatives
    let mut indexed: Vec<(usize, f32)> = probs_vec.iter().copied().enumerate().collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let alternatives: Vec<(u32, f32)> = indexed
        .into_iter()
        .take(5)
        .filter(|(id, _)| *id as u32 != token_id)
        .take(4)
        .map(|(id, prob)| (id as u32, prob))
        .collect();

    Ok((token_id, confidence, alternatives))
}

// Memory management API
pub mod memory;

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

/// Create initial health state for MLX model
fn create_initial_health() -> std::sync::Arc<std::sync::Mutex<ModelHealth>> {
    std::sync::Arc::new(std::sync::Mutex::new(ModelHealth {
        operational: false,
        consecutive_failures: 0,
        last_success: None,
        last_failure: None,
        circuit_breaker: CircuitBreakerState::Closed,
    }))
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
            health: create_initial_health(),
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
        let health = create_initial_health();

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

        if !model_path.exists() {
            return Err(AosError::Config(format!(
                "MLX model path '{}' does not exist. Set AOS_MODEL_PATH to a valid MLX model directory.",
                model_path.display()
            )));
        }

        if !model_path.is_dir() {
            return Err(AosError::Config(format!(
                "MLX model path '{}' is not a directory. Set AOS_MODEL_PATH to the MLX model directory containing config.json.",
                model_path.display()
            )));
        }

        // Load config
        let config_path = model_path.join("config.json");
        if !config_path.exists() {
            return Err(AosError::Config(format!(
                "config.json not found at '{}'. Set AOS_MODEL_PATH to a directory containing MLX config.json and weights.",
                config_path.display()
            )));
        }
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

        ffi_error::clear_ffi_error();

        // Load model via FFI
        let model = unsafe { mlx_model_load(path_cstr.as_ptr()) };
        let model = ffi_error::check_ffi_ptr(model, "load MLX model")?;

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
        let health = create_initial_health();

        // Safety: Check buffer validity
        if buffer.len() < 4 {
            return Err(AosError::Parse("Weight buffer too small".to_string()));
        }

        // Serialize config to JSON for FFI
        let config_json = serde_json::to_string(&config)
            .map_err(|e| AosError::Internal(format!("Failed to serialize config: {}", e)))?;
        let config_cstr = std::ffi::CString::new(config_json)
            .map_err(|e| AosError::Internal(format!("Invalid config string: {}", e)))?;

        ffi_error::clear_ffi_error();

        // Load model via FFI from buffer
        let model = unsafe {
            mlx_model_load_from_buffer(buffer.as_ptr(), buffer.len(), config_cstr.as_ptr())
        };
        let model = ffi_error::check_ffi_ptr(model, "load MLX model from buffer")?;

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

    /// Unified forward pass implementation that handles both regular and hidden-state-capturing modes
    fn forward_impl(
        &self,
        token_ids: &[u32],
        capture_hidden: bool,
    ) -> Result<(
        Vec<f32>,
        Option<std::collections::HashMap<String, Vec<f32>>>,
    )> {
        // Convert token_ids to C array (shared logic)
        let token_ints: Vec<i32> = token_ids.iter().map(|&x| x as i32).collect();

        // SAFETY: Validate token_ids length fits in i32 before FFI call.
        // In practice, token sequences are limited by model context length (typically < 128K).
        const MAX_TOKEN_LEN: usize = i32::MAX as usize;
        if token_ints.len() > MAX_TOKEN_LEN {
            return Err(AosError::Mlx(format!(
                "Token sequence length {} exceeds maximum {}",
                token_ints.len(),
                MAX_TOKEN_LEN
            )));
        }

        // Create MLX array from token IDs (RAII guard ensures cleanup)
        let input_guard = ffi_error::MlxArrayGuard::new_with_context(
            unsafe { mlx_array_from_ints(token_ints.as_ptr(), token_ints.len() as i32) },
            "create input array",
        )?;

        // Prepare hidden states tracking (only when needed)
        let mut hidden_states_ptr: *mut mlx_array_t = std::ptr::null_mut();
        let mut num_hidden: i32 = 0;

        // Maximum hidden states from FFI (used for bounds validation below)
        const MAX_HIDDEN_STATES: i32 = 1024;

        // Helper closure to cleanup hidden states when capture_hidden is true
        // SAFETY: This assumes the C++ FFI contract is upheld - that the returned
        // num_hidden matches the actual allocation size. We validate num_hidden
        // is in bounds (0..=MAX_HIDDEN_STATES) but cannot verify C++ consistency.
        let cleanup_hidden = |ptr: *mut mlx_array_t, count: i32| {
            if !ptr.is_null() {
                unsafe { mlx_hidden_states_free(ptr, count) };
            }
        };

        // Run forward pass - different FFI call based on capture_hidden flag
        let output_guard = if capture_hidden {
            // Call forward with hidden states
            let output_array = unsafe {
                mlx_model_forward_with_hidden_states(
                    self.model,
                    input_guard.as_ptr(),
                    &mut hidden_states_ptr,
                    &mut num_hidden,
                )
            };

            if output_array.is_null() {
                let error = ffi_error::get_ffi_error_or("Unknown error (null error message)");
                return Err(AosError::Mlx(format!(
                    "Failed to run model forward with hidden states: {}",
                    error
                )));
            }

            // SAFETY: Validate num_hidden from FFI before using as loop bound.
            // The C++ FFI returns this value and we must not trust it blindly.
            if num_hidden < 0 {
                return Err(AosError::Mlx(format!(
                    "Invalid hidden state count from FFI: {} (must be >= 0)",
                    num_hidden
                )));
            }
            if num_hidden > MAX_HIDDEN_STATES {
                return Err(AosError::Mlx(format!(
                    "Hidden state count {} exceeds maximum {} - possible FFI corruption",
                    num_hidden, MAX_HIDDEN_STATES
                )));
            }

            ffi_error::MlxArrayGuard::new(output_array)?
        } else {
            // Regular forward pass
            ffi_error::MlxArrayGuard::new_with_context(
                unsafe { mlx_model_forward(self.model, input_guard.as_ptr()) },
                "run model forward",
            )?
        };

        // CRITICAL: Force evaluation of lazy computation graph
        if capture_hidden {
            // Batch evaluate output and all hidden states
            let mut arrays_to_eval: Vec<*mut mlx_array_t> = vec![output_guard.as_ptr()];

            if !hidden_states_ptr.is_null() && num_hidden > 0 {
                // SAFETY: Pointer arithmetic is safe because:
                // 1. num_hidden was validated >= 0 and <= MAX_HIDDEN_STATES (line 797-808)
                // 2. hidden_states_ptr is non-null (checked above)
                // 3. The C++ FFI contract guarantees num_hidden matches allocated array size
                for i in 0..num_hidden {
                    let hs_array =
                        unsafe { *(hidden_states_ptr as *mut *mut mlx_array_t).add(i as usize) };
                    if !hs_array.is_null() {
                        arrays_to_eval.push(hs_array);
                    }
                }
            }

            // SAFETY: arrays_to_eval.len() is bounded by 1 + MAX_HIDDEN_STATES (1025),
            // which fits safely in i32.
            debug_assert!(arrays_to_eval.len() <= (MAX_HIDDEN_STATES as usize + 1));
            unsafe {
                mlx_eval_all(arrays_to_eval.as_mut_ptr(), arrays_to_eval.len() as i32);
                mlx_synchronize();
            }
        } else {
            // Simple evaluation for regular forward pass
            unsafe {
                mlx_eval(output_guard.as_ptr());
                mlx_synchronize();
            }
        }

        // Extract output data with safety validation (shared logic)
        let output_size = unsafe { mlx_array_size(output_guard.as_ptr()) };
        let output_data = unsafe { mlx_array_data(output_guard.as_ptr()) };

        // Safety validation
        if output_size == 0 {
            if capture_hidden {
                cleanup_hidden(hidden_states_ptr, num_hidden);
            }
            return Err(AosError::Mlx("Model returned empty output".to_string()));
        }

        const MAX_TENSOR_SIZE: usize = 1024 * 1024 * 100; // 100M elements max
        if output_size as usize > MAX_TENSOR_SIZE {
            if capture_hidden {
                cleanup_hidden(hidden_states_ptr, num_hidden);
            }
            return Err(AosError::Mlx(format!(
                "Output tensor too large: {} elements (max: {})",
                output_size, MAX_TENSOR_SIZE
            )));
        }

        if output_data.is_null() {
            if capture_hidden {
                cleanup_hidden(hidden_states_ptr, num_hidden);
            }
            return Err(AosError::Mlx("Invalid output data pointer".to_string()));
        }

        // SAFETY: We have validated:
        // 1. output_data is non-null (line 876-880)
        // 2. output_size > 0 (line 858-863)
        // 3. output_size <= MAX_TENSOR_SIZE (line 865-873)
        // 4. Alignment: mlx_array_data returns properly aligned f32*
        let logits: Vec<f32> =
            unsafe { std::slice::from_raw_parts(output_data, output_size as usize).to_vec() };

        // Extract hidden states only when capture_hidden=true
        let hidden_states = if capture_hidden {
            let mut hidden_map = std::collections::HashMap::new();

            if !hidden_states_ptr.is_null() && num_hidden > 0 {
                let hidden_array_ptr = hidden_states_ptr as *mut *mut mlx_array_t;

                // SAFETY: Pointer arithmetic is safe because:
                // 1. num_hidden was validated >= 0 and <= MAX_HIDDEN_STATES (line 797-808)
                // 2. hidden_states_ptr is non-null (checked above)
                // 3. The C++ FFI contract guarantees num_hidden matches allocated array size
                for i in 0..num_hidden {
                    // Get the module name for this hidden state
                    // SAFETY: name_buf is 256 bytes, which fits in i32 (256 < i32::MAX)
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
                    // SAFETY: i is bounded by num_hidden which was validated <= MAX_HIDDEN_STATES
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

                    // SAFETY: Validate hidden_size from FFI before using in from_raw_parts.
                    // This prevents reading beyond allocated memory if FFI returns corrupted size.
                    if hidden_size > MAX_TENSOR_SIZE {
                        tracing::warn!(
                            "Hidden state '{}' size {} exceeds max {}, skipping",
                            module_name,
                            hidden_size,
                            MAX_TENSOR_SIZE
                        );
                        continue;
                    }

                    // SAFETY: We have validated:
                    // 1. hidden_data is non-null (checked above)
                    // 2. hidden_size > 0 (checked above)
                    // 3. hidden_size <= MAX_TENSOR_SIZE (prevents OOB read)
                    // 4. Alignment: mlx_array_data returns properly aligned f32*
                    let hidden_vec: Vec<f32> =
                        unsafe { std::slice::from_raw_parts(hidden_data, hidden_size).to_vec() };

                    tracing::trace!(
                        "Extracted hidden state '{}': {} elements",
                        module_name,
                        hidden_vec.len()
                    );

                    hidden_map.insert(module_name, hidden_vec);
                }

                // Clean up hidden states array
                // SAFETY: num_hidden was validated at lines 813-822. We trust the C++
                // FFI contract that mlx_model_forward_with_hidden_states allocated
                // exactly num_hidden array slots.
                unsafe { mlx_hidden_states_free(hidden_states_ptr, num_hidden) };
            }

            Some(hidden_map)
        } else {
            None
        };

        // Guards automatically cleanup on drop

        if capture_hidden {
            tracing::debug!(
                "MLX FFI forward with hidden states: {} tokens -> {} logits, {} hidden state modules extracted",
                token_ids.len(),
                logits.len(),
                hidden_states.as_ref().map(|h| h.len()).unwrap_or(0)
            );
        } else {
            tracing::debug!(
                "MLX FFI forward pass complete: {} tokens -> {} logits",
                token_ids.len(),
                logits.len()
            );
        }

        Ok((logits, hidden_states))
    }

    /// Internal forward pass implementation
    fn forward_internal(&self, token_ids: &[u32], _position: usize) -> Result<Vec<f32>> {
        let (logits, _) = self.forward_impl(token_ids, false)?;
        Ok(logits)
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
            kv_num_layers: Some(self.config.num_hidden_layers),
        };

        // Create generator with deterministic seed based on model path
        let base_seed = B3Hash::hash(self.model_path.to_string_lossy().as_bytes());
        let mut generator = generation::MLXGenerator::new(base_seed, gen_config)?;

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

        let mut config = config;
        if config.use_cache && config.kv_num_layers.is_none() {
            config.kv_num_layers = Some(self.config.num_hidden_layers);
        }

        let base_seed = B3Hash::hash(self.model_path.to_string_lossy().as_bytes());
        let mut generator = generation::MLXGenerator::new(base_seed, config)?;

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
        let (logits, hidden_states) = self.forward_impl(token_ids, true)?;
        Ok((logits, hidden_states.unwrap_or_default()))
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
#[cfg_attr(all(feature = "mlx", not(mlx_stub)), link(name = "mlx_wrapper"))]
#[cfg_attr(any(mlx_stub, not(feature = "mlx")), link(name = "mlx_wrapper_stub"))]
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

    // Token sampling for text generation (legacy)
    fn mlx_sample_token(
        logits: *mut mlx_array_t,
        temperature: f32,
        top_k: i32,
        top_p: f32,
        out_token: *mut u32,
    ) -> bool;

    // Token sampling with metadata
    fn mlx_sample_token_with_metadata(
        logits: *mut mlx_array_t,
        config: *const MlxSamplerConfig,
        out_metadata: *mut MlxTokenMetadata,
    ) -> i32;

    // Free token metadata
    fn mlx_free_token_metadata(metadata: *mut MlxTokenMetadata);

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
/// Macro to define opaque FFI types
macro_rules! define_opaque_ffi_type {
    ($name:ident) => {
        #[repr(C)]
        pub struct $name {
            _private: [u8; 0],
        }
    };
}

define_opaque_ffi_type!(mlx_model_t);
define_opaque_ffi_type!(mlx_array_t);
define_opaque_ffi_type!(mlx_kv_cache_t);
define_opaque_ffi_type!(mlx_weights_t);

/// Sampler configuration for token generation
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MlxSamplerConfig {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub repetition_penalty: f32,
    pub seed: u64,
}

/// Alternative token with probability
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MlxTokenAlternative {
    pub token_id: u32,
    pub prob: f32,
}

/// Token metadata (confidence and alternatives)
#[repr(C)]
pub struct MlxTokenMetadata {
    pub confidence: f32,
    pub alternatives: *mut MlxTokenAlternative,
    pub num_alternatives: i32,
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
    /// Extract a null-terminated C string from a byte buffer
    fn extract_cstr(buf: &[u8]) -> &str {
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        std::str::from_utf8(&buf[..end]).unwrap_or("")
    }

    /// Get the device name as a string
    pub fn device_name_str(&self) -> &str {
        Self::extract_cstr(&self.device_name)
    }

    /// Get the MLX version as a string
    pub fn mlx_version_str(&self) -> &str {
        Self::extract_cstr(&self.mlx_version)
    }

    /// Get the Metal version as a string
    pub fn metal_version_str(&self) -> &str {
        Self::extract_cstr(&self.metal_version)
    }
}

// =============================================================================
// Safe Runtime Initialization Wrappers
// =============================================================================

use std::sync::atomic::{AtomicBool, Ordering};

static MLX_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Internal implementation for MLX runtime initialization
///
/// # Arguments
/// * `device` - Optional device type. None uses default device selection, Some uses specific device
///
/// # Returns
/// * `Ok(())` - Runtime initialized successfully (or already initialized)
/// * `Err(...)` - Initialization failed
fn mlx_runtime_init_internal(device: Option<MlxDeviceType>) -> Result<()> {
    // Check if already initialized (idempotent)
    if MLX_INITIALIZED.load(Ordering::SeqCst) {
        tracing::debug!("MLX runtime already initialized, skipping");
        return Ok(());
    }

    // Clear FFI error state before initialization
    ffi_error::clear_ffi_error();

    // Call appropriate initialization function
    let result = unsafe {
        match device {
            None => mlx_init_default(),
            Some(dev) => mlx_init(dev as i32),
        }
    };

    // Check result and handle errors using ffi_error helpers
    if result == 0 {
        MLX_INITIALIZED.store(true, Ordering::SeqCst);
        match device {
            None => tracing::info!("MLX runtime initialized successfully"),
            Some(dev) => tracing::info!(?dev, "MLX runtime initialized with specific device"),
        }
        Ok(())
    } else {
        let error = ffi_error::get_ffi_error_or("Unknown initialization error");
        Err(AosError::Mlx(format!(
            "Failed to initialize MLX runtime{}: {}",
            device
                .map(|d| format!(" with device {:?}", d))
                .unwrap_or_default(),
            error
        )))
    }
}

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
    mlx_runtime_init_internal(None)
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
    mlx_runtime_init_internal(Some(device))
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

    ffi_error::clear_ffi_error();
    let result = unsafe { mlx_backend_info(&mut capabilities) };
    ffi_error::check_ffi_result(result, "get backend capabilities")?;

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

    ffi_error::clear_ffi_error();
    mlx_eval(array);

    if let Some(error) = ffi_error::get_and_clear_ffi_error() {
        return Err(AosError::Mlx(format!("Evaluation failed: {}", error)));
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

    ffi_error::clear_ffi_error();

    unsafe {
        mlx_eval_all(
            valid_arrays.as_ptr() as *mut *mut mlx_array_t,
            valid_arrays.len() as i32,
        );
        mlx_synchronize();
    }

    if let Some(error) = ffi_error::get_and_clear_ffi_error() {
        return Err(AosError::Mlx(format!("Batch evaluation failed: {}", error)));
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

    /// Create a standard test ModelConfig
    fn test_model_config() -> ModelConfig {
        ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        }
    }

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
        let config = test_model_config();
        let model = MLXFFIModel::new_null(config.clone());
        assert_eq!(model.config.hidden_size, 4096);
        assert_eq!(model.config.num_hidden_layers, 32);
        assert!(model.model.is_null());
    }

    #[test]
    fn test_generate_requires_tokenizer() {
        let config = test_model_config();
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
        let config = test_model_config();
        let model = MLXFFIModel::new_null(config);

        let gen_config = generation::GenerationConfig {
            max_tokens: 100,
            temperature: 0.5,
            top_k: Some(40),
            top_p: Some(0.95),
            repetition_penalty: 1.2,
            eos_token: 2,
            use_cache: true,
            kv_num_layers: Some(32),
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
        let config = test_model_config();
        let model = MLXFFIModel::new_null(config);

        assert!(
            model.tokenizer().is_none(),
            "Tokenizer should be None for null model"
        );
    }

    #[test]
    fn test_model_path_accessor_returns_empty_for_null_model() {
        let config = test_model_config();
        let model = MLXFFIModel::new_null(config);

        assert!(
            model.model_path().as_os_str().is_empty(),
            "Model path should be empty for null model"
        );
    }

    #[test]
    fn test_new_null_creates_non_operational_model() {
        let config = test_model_config();
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
