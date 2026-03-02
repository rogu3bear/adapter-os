//! MLX FFI integration for adapterOS
//!
//! This crate provides C FFI bindings for MLX's C++ API, avoiding PyO3 dependency issues.
//! The C++ FFI path is the primary/production backend.

#![allow(unexpected_cfgs)]
#![allow(deprecated)]
#![allow(unused_mut)]
#![allow(clippy::needless_return)]
#![allow(clippy::type_complexity)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(clippy::too_many_arguments)]

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_storage::secure_fs::path_policy::canonicalize_strict;
use std::path::{Path, PathBuf};

// Array abstraction layer
pub mod array;

// C++ FFI modules (primary backend)
pub mod attention;
pub mod backend;
pub mod embedding;
pub mod ffi_error;
pub mod generation;
pub mod kv_cache;
pub mod liquid;
pub mod lora;
pub mod memory_management;
pub mod memory_pool;
pub mod monitoring;
pub mod quantization;
pub mod routing;
pub mod safetensors_loader;
pub mod streaming;
pub mod tensor;
pub mod tokenizer;
pub mod training;
pub mod unified_loader;

// Adapter cache for efficient LoRA weight management
pub mod adapter_cache;

// LoRA fusion FFI wrappers
pub mod lora_ffi;
pub mod session_cache;

// Mock module for testing - always available since integration tests need it
pub mod mock;

include!(concat!(env!("OUT_DIR"), "/mlx_build_info.rs"));

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
pub use quantization::{
    MLXQuantizer, QuantizationConfig, QuantizationMetadata, QuantizationStats, QuantizedTensor,
    WeightCompressor,
};
pub use routing::apply_multi_lora;
pub use safetensors_loader::{SafetensorsLoader, TensorInfo};
pub use tensor::MLXFFITensor;
pub use tokenizer::MLXTokenizer;
pub use training::{
    mlx_clip_grad_norm_gpu, mlx_cross_entropy_loss_gpu, mlx_lora_backward_ce_gpu,
    mlx_lora_backward_gpu, mlx_mse_loss_gpu, mlx_zero_grad_gpu, LoraBackwardResult, MlxOptimizer,
    MlxOptimizerType,
};
pub use unified_loader::{LoadStrategy, TensorMetadata, UnifiedSafeTensorsLoader};

// LoRA fusion FFI re-exports
pub use lora_ffi::FFILoraAdapter;
pub use session_cache::SessionCacheManager;

// Re-export FFI error utilities for external use
pub use ffi_error::{
    check_ffi_ptr, check_ffi_result, clear_ffi_error, get_and_clear_ffi_error, get_ffi_error_or,
    MlxArrayGuard, MlxArrayVecGuard,
};

const MLX_VERSION_ENFORCEMENT_ENV: &str = "AOS_ENFORCE_MLX_VERSION_MATCH";

#[derive(Debug, Clone, PartialEq, Eq)]
enum MlxRuntimeVersionStatus {
    Unavailable,
    Match {
        build_version: &'static str,
        runtime_version: String,
    },
    Mismatch {
        build_version: &'static str,
        runtime_version: String,
    },
}

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
    match select_mlx_implementation()? {
        MlxImplementation::Ffi => mlx_set_seed_from_bytes_ffi(seed),
    }
}

/// Set MLX's global random seed using a TypedSeed with version validation.
///
/// This is the preferred method for setting MLX seeds across FFI boundaries.
/// It validates:
/// 1. The seed's checksum integrity (detects corruption)
/// 2. The seed's algorithm version (detects schema drift)
///
/// In strict determinism mode, version mismatches cause immediate failure.
/// In best-effort mode, version mismatches log a warning but proceed.
///
/// # Arguments
/// * `typed_seed` - A TypedSeed containing version metadata and checksum
///
/// # Errors
/// Returns `AosError::DeterminismViolation` if:
/// - Checksum validation fails (always)
/// - Version mismatch detected and strict mode enabled
///
/// # Example
/// ```ignore
/// use adapteros_core::seed::{derive_typed_seed, B3Hash};
/// use adapteros_lora_mlx_ffi::mlx_set_typed_seed;
///
/// let global = B3Hash::hash(b"model-manifest");
/// let typed_seed = derive_typed_seed(&global, "mlx-dropout");
/// mlx_set_typed_seed(&typed_seed)?;
/// ```
pub fn mlx_set_typed_seed(typed_seed: &adapteros_core::TypedSeed) -> Result<()> {
    // Validate seed with current determinism config (strict/best-effort mode)
    typed_seed.validate_with_config()?;

    tracing::debug!(
        seed_version = typed_seed.version,
        seed_checksum = %typed_seed.checksum.to_short_hex(),
        "Setting MLX seed with TypedSeed (version-validated)"
    );

    // Pass validated raw bytes to the backend
    mlx_set_seed_from_bytes(typed_seed.as_bytes())
}

pub(crate) fn mlx_set_seed_from_bytes_ffi(seed: &[u8]) -> Result<()> {
    // INVARIANT: Validate seed meets HKDF requirements (32 bytes)
    adapteros_core::validate_seed_bytes(seed)?;

    ffi_error::clear_ffi_error();

    unsafe {
        mlx_set_seed(seed.as_ptr(), seed.len());
    }

    // Check if there was an error during seed setting
    if let Some(error_str) = ffi_error::get_and_clear_ffi_error() {
        // Seed failures are determinism violations - they break reproducibility
        return Err(AosError::DeterminismViolation(format!(
            "MLX RNG seeding failed: {}",
            error_str
        )));
    }

    tracing::debug!(
        seed_len = seed.len(),
        seed_checksum = %format!("{:02x}{:02x}{:02x}{:02x}", seed[0], seed[1], seed[2], seed[3]),
        "MLX FFI backend seeded for deterministic dropout/sampling"
    );

    Ok(())
}

/// Internal shared implementation for token sampling (C++ FFI backend)
///
/// This function contains the core sampling logic using the MLX C++ FFI backend.
/// It performs the actual token sampling using the native MLX library.
fn sample_token_impl(
    logits: &MLXFFITensor,
    temperature: f32,
    top_k: u32,
    top_p: f32,
) -> Result<u32> {
    validate_sampling_params(temperature, top_p)?;

    let config = MlxSamplerConfig {
        temperature,
        top_p,
        top_k: top_k as i32,
        repetition_penalty: 1.0,
        seed: 0,
    };

    let token = unsafe { mlx_sample_token(logits.inner, &config) };

    if token < 0 {
        let error = ffi_error::get_ffi_error_or("Unknown error");
        // Classify error: RNG/sampling failures are determinism violations
        let error_lower = error.to_lowercase();
        if error_lower.contains("rng")
            || error_lower.contains("seed")
            || error_lower.contains("random")
        {
            return Err(AosError::DeterminismViolation(format!(
                "Token sampling RNG failure: {}",
                error
            )));
        }
        return Err(AosError::Mlx(format!("Token sampling failed: {}", error)));
    }

    let sampled_token = token as u32;

    tracing::debug!(
        sampled_token = sampled_token,
        temperature = temperature,
        top_k = top_k,
        top_p = top_p,
        "MLX token sampled successfully"
    );

    Ok(sampled_token)
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

        // Extract alternatives with bounds check on FFI-provided size.
        // Cap at a reasonable maximum to prevent memory safety issues from
        // corrupted/malicious FFI data.
        const MAX_ALTERNATIVES: usize = 1024;
        let mut alternatives = Vec::new();
        if !metadata.alternatives.is_null() && metadata.num_alternatives > 0 {
            let num_alts = (metadata.num_alternatives as usize).min(MAX_ALTERNATIVES);
            if metadata.num_alternatives as usize > MAX_ALTERNATIVES {
                tracing::warn!(
                    requested = metadata.num_alternatives,
                    capped = MAX_ALTERNATIVES,
                    "FFI num_alternatives exceeded maximum, capping"
                );
            }
            let alts_slice = std::slice::from_raw_parts(metadata.alternatives, num_alts);
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

// Memory management API
pub mod memory;

/// MLX model wrapper for inference using FFI
pub struct MLXFFIModel {
    /// C++ MLX model object
    model: *mut mlx_model_t,
    /// Model configuration
    pub config: ModelConfig,
    /// Serialize all inference calls into the underlying C++ wrapper.
    ///
    /// The C++ model wrapper maintains mutable per-model state during inference
    /// (e.g., `hidden_states_vec` used by `forward_with_hidden_states`), and is
    /// not safe to access concurrently from multiple threads.
    inference_lock: parking_lot::Mutex<()>,
    /// Health status tracking
    health: std::sync::Arc<std::sync::Mutex<ModelHealth>>,
    /// Path to model directory (for loading tokenizer)
    model_path: PathBuf,
    /// Tokenizer for text encoding/decoding (loaded lazily)
    tokenizer: Option<tokenizer::MLXTokenizer>,
    /// Optional C++ KV cache for efficient autoregressive generation
    /// When present, enables O(1) per-token generation instead of O(n²)
    kv_cache: Option<*mut mlx_kv_cache_t>,
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
            inference_lock: parking_lot::Mutex::new(()),
            health: create_initial_health(),
            model_path: PathBuf::new(),
            tokenizer: None,
            kv_cache: None,
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

        // Canonicalize path to prevent directory traversal and resolve symlinks.
        // This normalizes ../.. sequences and converts relative paths to absolute.
        // Fails if path doesn't exist (defense in depth).
        let model_path = canonicalize_strict(model_path).map_err(|e| {
            AosError::Config(format!(
                "Failed to canonicalize model path '{}': {}. \
                 Ensure the path exists and is accessible.",
                model_path.display(),
                e
            ))
        })?;
        let model_path = model_path.as_path();

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
            inference_lock: parking_lot::Mutex::new(()),
            health,
            model_path: model_path.to_path_buf(),
            tokenizer,
            kv_cache: None,
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
            inference_lock: parking_lot::Mutex::new(()),
            health,
            model_path: PathBuf::new(),
            tokenizer: None,
            kv_cache: None,
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
                        // health.circuit_breaker = CircuitBreakerState::Open; // Temporarily disabled
                        tracing::warn!(
                            "MLX model circuit breaker opened after {} consecutive failures (temporarily disabled, remaining closed)",
                            health.consecutive_failures
                        );
                    }
                }
                Err(e)
            }
        }
    }

    /// Unified forward pass implementation that handles both regular and hidden-state-capturing modes
    ///
    /// # Arguments
    /// * `token_ids` - Input token IDs
    /// * `position_offset` - Starting position for RoPE computation (0 for prompt, N for step N)
    /// * `capture_hidden` - Whether to capture hidden states from each layer
    fn forward_impl(
        &self,
        token_ids: &[u32],
        position_offset: usize,
        capture_hidden: bool,
    ) -> Result<(
        Vec<f32>,
        Option<std::collections::HashMap<String, Vec<f32>>>,
    )> {
        // IMPORTANT: The MLX C++ wrapper is not safe for concurrent use on the
        // same model instance (it mutates internal state when capturing hidden
        // states). Serialize inference to avoid UB / SIGSEGV.
        let _inference_guard = self.inference_lock.lock();

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
            // position_offset is passed to ensure correct RoPE positions during incremental generation
            let output_array = unsafe {
                mlx_model_forward_with_hidden_states(
                    self.model,
                    input_guard.as_ptr(),
                    position_offset as i32,
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
            // position_offset is passed to ensure correct RoPE positions during incremental generation
            ffi_error::MlxArrayGuard::new_with_context(
                unsafe {
                    mlx_model_forward(self.model, input_guard.as_ptr(), position_offset as i32)
                },
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
            assert!(arrays_to_eval.len() <= (MAX_HIDDEN_STATES as usize + 1));
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
    ///
    /// # Arguments
    /// * `token_ids` - Input token IDs
    /// * `position` - Starting position for RoPE computation (critical for incremental generation)
    fn forward_internal(&self, token_ids: &[u32], position: usize) -> Result<Vec<f32>> {
        let (logits, _) = self.forward_impl(token_ids, position, false)?;
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
        &mut self,
        token_ids: &[u32],
        position: usize,
        _cache: Option<&crate::kv_cache::MLXKVCache>,
    ) -> Result<Vec<f32>> {
        // Acquire inference lock for thread safety
        let _guard = self.inference_lock.lock();

        // Initialize KV cache on first call if needed
        if self.kv_cache.is_none() {
            // Create C++ KV cache with model dimensions
            let kv_cache_ptr = unsafe {
                mlx_kv_cache_new(
                    self.config.num_hidden_layers as i32,
                    self.config.num_attention_heads as i32,
                    (self.config.hidden_size / self.config.num_attention_heads) as i32,
                    self.config.max_position_embeddings as i32,
                )
            };

            if kv_cache_ptr.is_null() {
                return Err(AosError::Mlx("Failed to create KV cache".to_string()));
            }

            self.kv_cache = Some(kv_cache_ptr);
            tracing::debug!(
                num_layers = self.config.num_hidden_layers,
                num_heads = self.config.num_attention_heads,
                head_dim = self.config.hidden_size / self.config.num_attention_heads,
                "Created C++ KV cache for efficient generation"
            );
        }

        // Determine which tokens to process
        // On first call (position == 0), process full sequence
        // On subsequent calls, only process new tokens
        let tokens_to_process: Vec<u32> = if position == 0 {
            token_ids.to_vec()
        } else {
            // For incremental generation, process only the new token(s)
            // Calculate which tokens haven't been processed yet based on position
            if position < token_ids.len() {
                token_ids[position..].to_vec()
            } else {
                // All tokens already processed, just return last logits
                return self.forward(&[token_ids.last().copied().unwrap_or(0)], position);
            }
        };

        if tokens_to_process.is_empty() {
            return Err(AosError::Validation("No tokens to process".to_string()));
        }

        let kv_cache = self
            .kv_cache
            .ok_or_else(|| AosError::Mlx("KV cache missing after initialization".to_string()))?;

        // Create input array from token IDs
        // SAFETY: `tokens_to_process` is a live Rust slice for this call and MLX either returns
        // a new handle or null; null is checked immediately below.
        let input_array = unsafe {
            mlx_array_from_uints(tokens_to_process.as_ptr(), tokens_to_process.len() as i32)
        };

        if input_array.is_null() {
            return Err(AosError::Mlx("Failed to create input array".to_string()));
        }

        // Run forward pass with KV cache
        // SAFETY: `self.model`, `input_array`, and `kv_cache` are valid handles owned by this
        // model instance for the duration of this call.
        let output_array = unsafe {
            mlx_model_forward_with_cache(self.model, input_array, position as i32, kv_cache)
        };

        // Clean up input array
        // SAFETY: `input_array` was allocated above and is still owned by this frame.
        unsafe { mlx_array_free(input_array) };

        if output_array.is_null() {
            // SAFETY: MLX guarantees the returned error pointer is valid C string or null on
            // failure paths; we only read it immediately and copy into owned Rust string.
            let error = unsafe {
                std::ffi::CStr::from_ptr(mlx_get_last_error())
                    .to_string_lossy()
                    .to_string()
            };
            return Err(AosError::Mlx(format!(
                "Forward with cache failed: {}",
                error
            )));
        }

        // Extract output data
        let output_size = unsafe { mlx_array_size(output_array) };
        let output_data = unsafe { mlx_array_data(output_array) };

        if output_data.is_null() || output_size == 0 {
            unsafe { mlx_array_free(output_array) };
            return Err(AosError::Mlx("Model returned empty output".to_string()));
        }

        // Copy logits to Vec
        let logits: Vec<f32> =
            unsafe { std::slice::from_raw_parts(output_data, output_size as usize).to_vec() };

        // Clean up output array
        unsafe { mlx_array_free(output_array) };

        Ok(logits)
    }

    /// Clear the KV cache to free memory
    pub fn clear_kv_cache(&mut self) {
        if let Some(cache_ptr) = self.kv_cache.take() {
            unsafe { mlx_kv_cache_free(cache_ptr) };
            tracing::debug!("Cleared KV cache");
        }
    }

    /// Forward pass with KV cache and fused LoRA adapters.
    ///
    /// This is the primary inference path for production use. It processes
    /// ALL transformer layers with per-layer LoRA application and KV caching.
    ///
    /// # Arguments
    /// * `token_ids` - Input token IDs
    /// * `position` - Position offset for RoPE
    /// * `cache_ptr` - KV cache pointer (null for no caching)
    /// * `adapter_ptrs` - Slice of FFI LoRA adapter raw pointers
    /// * `blend_weights` - Per-adapter blend weights from router
    ///
    /// # Returns
    /// Logits as Vec<f32>
    pub fn forward_with_cache_and_lora(
        &self,
        token_ids: &[u32],
        position: usize,
        cache_ptr: *mut mlx_kv_cache_t,
        adapter_ptrs: &[*mut mlx_lora_adapter_t],
        blend_weights: &[f32],
    ) -> Result<Vec<f32>> {
        if token_ids.is_empty() {
            return Err(AosError::Validation("Empty token IDs".to_string()));
        }

        if adapter_ptrs.len() != blend_weights.len() {
            return Err(AosError::Validation(format!(
                "Adapter count ({}) does not match blend weight count ({})",
                adapter_ptrs.len(),
                blend_weights.len()
            )));
        }

        let _guard = self.inference_lock.lock();

        ffi_error::clear_ffi_error();

        // Create input array from token IDs
        let input = unsafe { mlx_array_from_uints(token_ids.as_ptr(), token_ids.len() as i32) };
        if input.is_null() {
            return Err(AosError::Mlx("Failed to create input array".to_string()));
        }

        let result = unsafe {
            mlx_model_forward_with_cache_and_lora(
                self.model,
                input,
                position as i32,
                cache_ptr,
                if adapter_ptrs.is_empty() {
                    std::ptr::null()
                } else {
                    adapter_ptrs.as_ptr()
                },
                if blend_weights.is_empty() {
                    std::ptr::null()
                } else {
                    blend_weights.as_ptr()
                },
                adapter_ptrs.len() as i32,
            )
        };

        // Free input array
        unsafe { mlx_array_free(input) };

        if result.is_null() {
            let error = ffi_error::get_ffi_error_or("Forward with cache and LoRA failed");
            return Err(AosError::Mlx(error));
        }

        // Extract logits data
        let size = unsafe { mlx_array_size(result) };
        if size == 0 {
            unsafe { mlx_array_free(result) };
            return Err(AosError::Mlx("Empty logits from forward pass".to_string()));
        }

        let data_ptr = unsafe { mlx_array_data(result) };
        if data_ptr.is_null() {
            unsafe { mlx_array_free(result) };
            return Err(AosError::Mlx("Null logits data pointer".to_string()));
        }

        let logits = unsafe { std::slice::from_raw_parts(data_ptr, size) }.to_vec();

        unsafe { mlx_array_free(result) };

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
            seed: None,
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

    /// Generate text with an explicit 32-byte seed for deterministic output.
    ///
    /// This is the entry point for synthesis pipelines that require strict
    /// reproducibility: the seed flows through HKDF into every sampling step,
    /// making the output a deterministic function of (prompt, seed, config).
    pub fn generate_with_config_and_seed(
        &self,
        prompt: &str,
        mut config: generation::GenerationConfig,
        seed: [u8; 32],
    ) -> Result<String> {
        config.seed = Some(seed);
        self.generate_with_config(prompt, config)
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
    /// * `position` - Starting position for RoPE computation (0 for prompt, N for step N)
    ///
    /// # Returns
    /// Tuple of (logits, hidden_states_by_module)
    #[allow(clippy::type_complexity)]
    pub fn forward_with_hidden_states(
        &self,
        token_ids: &[u32],
        position: usize,
    ) -> Result<(Vec<f32>, std::collections::HashMap<String, Vec<f32>>)> {
        let (logits, hidden_states) = self.forward_impl(token_ids, position, true)?;
        Ok((logits, hidden_states.unwrap_or_default()))
    }

    /// Get model configuration
    pub fn config(&self) -> &ModelConfig {
        &self.config
    }

    /// Get a specific weight tensor from the model by name.
    ///
    /// This is useful for training to access the output projection (lm_head) weights
    /// needed for cross-entropy loss computation.
    ///
    /// # Arguments
    /// * `weight_name` - Name of the weight to retrieve (e.g., "lm_head.weight")
    ///
    /// # Returns
    /// Weight tensor as MLXFFITensor, or error if not found
    ///
    /// # Example
    /// ```ignore
    /// let lm_head = model.get_weight("lm_head.weight")?;
    /// ```
    pub fn get_weight(&self, weight_name: &str) -> Result<MLXFFITensor> {
        let weight_name_c = std::ffi::CString::new(weight_name)
            .map_err(|_| AosError::Validation("Invalid weight name".to_string()))?;

        let weight_ptr = unsafe { mlx_model_get_weight(self.model, weight_name_c.as_ptr()) };

        if weight_ptr.is_null() {
            return Err(AosError::Validation(format!(
                "Weight '{}' not found in model",
                weight_name
            )));
        }

        Ok(MLXFFITensor::from_raw(weight_ptr as *mut std::ffi::c_void))
    }
}

impl Drop for MLXFFIModel {
    fn drop(&mut self) {
        if !self.model.is_null() {
            unsafe {
                mlx_model_free(self.model);
            }
        }
        if let Some(cache_ptr) = self.kv_cache {
            unsafe {
                mlx_kv_cache_free(cache_ptr);
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
/// 1. All inference entrypoints in this wrapper serialize access via
///    `inference_lock`, preventing concurrent calls into the underlying C++
///    model object.
/// 2. The C++ wrapper maintains mutable per-model state during inference
///    (e.g. hidden state capture), so this lock is required for safety.
/// 3. All other mutable state (health tracking) is protected by `Arc<Mutex<>>`,
///    ensuring exclusive access and preventing Rust-side data races.
/// 4. Drop is safe from any thread: `mlx_model_free()` handles cleanup correctly
///    regardless of which thread calls it, and the null-check prevents double-free.
/// 5. Model weights are immutable after loading; inference reads weights only.
///
/// Reference: MLX C++ source confirms that `mlx::core::array` operations are
/// thread-safe through Metal command buffer synchronization and atomic reference
/// counting for shared array data.
unsafe impl Sync for MLXFFIModel {}

// FFI declarations for MLX operations (C++ FFI)
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
    // position_offset: Starting position for RoPE computation
    //   - For prompt processing (step 0): offset = 0
    //   - For incremental generation: offset = current position in sequence
    fn mlx_model_forward(
        model: *mut mlx_model_t,
        input: *mut mlx_array_t,
        position_offset: i32,
    ) -> *mut mlx_array_t;
    fn mlx_model_forward_with_hidden_states(
        model: *mut mlx_model_t,
        input: *mut mlx_array_t,
        position_offset: i32,
        hidden_states: *mut *mut mlx_array_t,
        hidden_count: *mut i32,
    ) -> *mut mlx_array_t;

    // Forward pass with KV cache for efficient generation
    fn mlx_model_forward_with_cache(
        model: *mut mlx_model_t,
        input: *mut mlx_array_t,
        position_offset: i32,
        kv_cache: *mut mlx_kv_cache_t,
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
    pub fn mlx_model_get_weight(
        model: *mut mlx_model_t,
        weight_name: *const std::os::raw::c_char,
    ) -> *mut mlx_array_t;

    // Array operations
    fn mlx_array_from_data(data: *const f32, size: i32) -> *mut mlx_array_t;
    fn mlx_array_from_ints(data: *const i32, size: i32) -> *mut mlx_array_t;
    fn mlx_array_from_uints(data: *const u32, size: i32) -> *mut mlx_array_t;
    fn mlx_array_data(array: *mut mlx_array_t) -> *mut f32;
    fn mlx_array_size(array: *mut mlx_array_t) -> usize; // C returns int; benign on ARM64 (zero-extended)
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
    // Returns sampled token index on success, -1 on error
    fn mlx_sample_token(logits: *mut mlx_array_t, config: *const MlxSamplerConfig) -> i32;

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
    // LoRA fusion forward pass
    // ========================================================================

    /// Create a new LoRA adapter handle.
    /// Returns null on failure (check mlx_get_last_error).
    pub fn mlx_lora_adapter_new(
        adapter_id: i32,
        num_layers: i32,
        scale: f32,
    ) -> *mut mlx_lora_adapter_t;

    /// Set LoRA weights for a specific module in a specific layer.
    /// Returns 0 on success, non-zero on error.
    pub fn mlx_lora_adapter_set_module(
        adapter: *mut mlx_lora_adapter_t,
        layer_idx: i32,
        module_name: *const std::os::raw::c_char,
        lora_a: *mut mlx_array_t,
        lora_b: *mut mlx_array_t,
    ) -> i32;

    /// Free a LoRA adapter handle.
    pub fn mlx_lora_adapter_free(adapter: *mut mlx_lora_adapter_t);

    /// Forward pass with KV cache AND fused LoRA adapters.
    ///
    /// This is the primary production inference path. It processes all
    /// transformer layers with per-layer LoRA application and KV caching.
    ///
    /// # Arguments
    /// * `model` - Model handle
    /// * `input` - Input token array
    /// * `position_offset` - RoPE position offset
    /// * `kv_cache` - KV cache (null for no caching)
    /// * `adapters` - Array of LoRA adapter pointers
    /// * `blend_weights` - Per-adapter blend weights from router
    /// * `num_active_adapters` - Number of active adapters
    ///
    /// # Returns
    /// Logits array, or null on error
    pub fn mlx_model_forward_with_cache_and_lora(
        model: *mut mlx_model_t,
        input: *mut mlx_array_t,
        position_offset: i32,
        kv_cache: *mut mlx_kv_cache_t,
        adapters: *const *mut mlx_lora_adapter_t,
        blend_weights: *const f32,
        num_active_adapters: i32,
    ) -> *mut mlx_array_t;

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
define_opaque_ffi_type!(mlx_lora_adapter_t);

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
use std::sync::{Mutex, OnceLock};

/// Selected MLX implementation (internal; user-facing backend remains `mlx`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlxImplementation {
    /// C++ FFI wrapper linked against Homebrew MLX (production)
    Ffi,
}

impl MlxImplementation {
    pub fn as_str(&self) -> &'static str {
        match self {
            MlxImplementation::Ffi => "ffi",
        }
    }
}

static MLX_IMPLEMENTATION: OnceLock<Mutex<Option<MlxImplementation>>> = OnceLock::new();
static MLX_INITIALIZED: AtomicBool = AtomicBool::new(false);
static MLX_INIT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

static MLX_TEST_LOCK: std::sync::OnceLock<parking_lot::ReentrantMutex<()>> =
    std::sync::OnceLock::new();

pub(crate) fn mlx_test_lock() -> parking_lot::ReentrantMutexGuard<'static, ()> {
    MLX_TEST_LOCK
        .get_or_init(|| parking_lot::ReentrantMutex::new(()))
        .lock()
}

pub fn mlx_test_lock_guard() -> parking_lot::ReentrantMutexGuard<'static, ()> {
    mlx_test_lock()
}

fn mlx_impl_slot() -> &'static Mutex<Option<MlxImplementation>> {
    MLX_IMPLEMENTATION.get_or_init(|| Mutex::new(None))
}

fn mlx_impl_override() -> Result<Option<MlxImplementation>> {
    let value = match std::env::var("AOS_MLX_IMPL") {
        Ok(val) => val,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(e) => {
            return Err(AosError::Config(format!(
                "Failed to read AOS_MLX_IMPL: {}",
                e
            )))
        }
    };

    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "auto" {
        return Ok(None);
    }

    match normalized.as_str() {
        "ffi" => Ok(Some(MlxImplementation::Ffi)),
        _ => Err(AosError::Config(format!(
            "Invalid AOS_MLX_IMPL '{}'; expected 'auto' or 'ffi'",
            value
        ))),
    }
}

fn mlx_test_device_override() -> Option<MlxDeviceType> {
    let raw = match std::env::var("AOS_MLX_TEST_DEVICE") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return None,
        Err(err) => {
            tracing::warn!("Failed to read AOS_MLX_TEST_DEVICE: {}", err);
            return None;
        }
    };

    match raw.trim().to_ascii_lowercase().as_str() {
        "cpu" => Some(MlxDeviceType::Cpu),
        "gpu" => Some(MlxDeviceType::Gpu),
        "ane" => Some(MlxDeviceType::Ane),
        "auto" => Some(MlxDeviceType::Auto),
        "" => None,
        other => {
            tracing::warn!(
                "Ignoring invalid AOS_MLX_TEST_DEVICE '{}'; expected cpu|gpu|ane|auto",
                other
            );
            None
        }
    }
}

fn resolve_mlx_runtime_device(device: Option<MlxDeviceType>) -> Option<MlxDeviceType> {
    if device.is_some() {
        return device;
    }

    if let Some(override_device) = mlx_test_device_override() {
        return Some(override_device);
    }

    if cfg!(test) {
        return Some(MlxDeviceType::Cpu);
    }

    None
}

pub(crate) fn mlx_test_auto_init() {
    if MLX_INITIALIZED.load(Ordering::SeqCst) || MLX_INIT_IN_PROGRESS.load(Ordering::SeqCst) {
        return;
    }

    let override_device = match mlx_test_device_override() {
        Some(device) => Some(device),
        None if cfg!(test) => Some(MlxDeviceType::Cpu),
        None => None,
    };

    if let Some(device) = override_device {
        let _ = mlx_runtime_init_internal_ffi(Some(device));
    }
}

fn ffi_build_available() -> bool {
    cfg!(feature = "mlx") && !cfg!(mlx_stub)
}

/// Return the selected MLX implementation, if already chosen.
pub fn mlx_selected_implementation() -> Option<MlxImplementation> {
    mlx_impl_slot().lock().ok().and_then(|guard| *guard)
}

/// Select MLX implementation (auto unless overridden via AOS_MLX_IMPL).
pub fn select_mlx_implementation() -> Result<MlxImplementation> {
    {
        let guard = mlx_impl_slot()
            .lock()
            .map_err(|_| AosError::Internal("MLX implementation lock poisoned".to_string()))?;
        if let Some(selected) = *guard {
            return Ok(selected);
        }
    }

    let override_choice = mlx_impl_override()?;
    let selected = if let Some(choice) = override_choice {
        match choice {
            MlxImplementation::Ffi => {
                if !ffi_build_available() {
                    return Err(AosError::Config(
                        "AOS_MLX_IMPL=ffi requested but MLX FFI build is unavailable".to_string(),
                    ));
                }
                choice
            }
        }
    } else if ffi_build_available() {
        MlxImplementation::Ffi
    } else {
        return Err(AosError::Config(
            "MLX backend unavailable (MLX FFI feature not enabled)".to_string(),
        ));
    };

    let mut guard = mlx_impl_slot()
        .lock()
        .map_err(|_| AosError::Internal("MLX implementation lock poisoned".to_string()))?;
    if guard.is_none() {
        *guard = Some(selected);
    }

    Ok(selected)
}

#[cfg(test)]
pub fn reset_mlx_selection_for_tests() {
    if let Ok(mut guard) = mlx_impl_slot().lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod mlx_impl_tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvVarGuard {
        key: &'static str,
        prev: Option<String>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: Option<&str>) -> Self {
            let lock = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
            let prev = std::env::var(key).ok();
            match value {
                Some(val) => std::env::set_var(key, val),
                None => std::env::remove_var(key),
            }
            Self {
                key,
                prev,
                _lock: lock,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(val) => std::env::set_var(self.key, val),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn selection_matches_compiled_features() {
        let _env = EnvVarGuard::set("AOS_MLX_IMPL", None);
        reset_mlx_selection_for_tests();

        let result = select_mlx_implementation();
        if ffi_build_available() {
            assert_eq!(result.unwrap(), MlxImplementation::Ffi);
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn override_rs_returns_error() {
        // mlx-rs backend has been removed
        let _env = EnvVarGuard::set("AOS_MLX_IMPL", Some("rs"));
        reset_mlx_selection_for_tests();

        let result = select_mlx_implementation();
        assert!(result.is_err(), "rs backend should return error (removed)");
    }

    #[test]
    fn override_ffi_respects_feature_gate() {
        let _env = EnvVarGuard::set("AOS_MLX_IMPL", Some("ffi"));
        reset_mlx_selection_for_tests();

        let result = select_mlx_implementation();
        if ffi_build_available() {
            assert_eq!(result.unwrap(), MlxImplementation::Ffi);
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn override_auto_uses_default_selection() {
        let _env = EnvVarGuard::set("AOS_MLX_IMPL", Some("auto"));
        reset_mlx_selection_for_tests();

        let result = select_mlx_implementation();
        if ffi_build_available() {
            assert_eq!(result.unwrap(), MlxImplementation::Ffi);
        } else {
            assert!(result.is_err());
        }
    }
}

/// Internal implementation for MLX runtime initialization (C FFI path)
fn mlx_runtime_init_internal_ffi(device: Option<MlxDeviceType>) -> Result<()> {
    if !ffi_build_available() {
        return Err(AosError::Config(
            "MLX FFI not available (build with --features mlx and ensure MLX is installed)"
                .to_string(),
        ));
    }

    // Check if already initialized (idempotent)
    if MLX_INITIALIZED.load(Ordering::SeqCst) {
        tracing::debug!("MLX runtime already initialized, skipping");
        return Ok(());
    }

    struct InitGuard;
    impl Drop for InitGuard {
        fn drop(&mut self) {
            MLX_INIT_IN_PROGRESS.store(false, Ordering::SeqCst);
        }
    }

    MLX_INIT_IN_PROGRESS.store(true, Ordering::SeqCst);
    let _guard = InitGuard;

    // Clear FFI error state before initialization
    ffi_error::clear_ffi_error();

    let device = resolve_mlx_runtime_device(device);

    // Call appropriate initialization function
    let result = unsafe {
        match device {
            None => mlx_init_default(),
            Some(dev) => mlx_init(dev as i32),
        }
    };

    // Check result and handle errors using ffi_error helpers
    if result == 0 {
        if let Err(error) = validate_mlx_runtime_version() {
            unsafe { mlx_shutdown() };
            return Err(error);
        }

        MLX_INITIALIZED.store(true, Ordering::SeqCst);
        if let Some(dev) = device {
            let set_result = unsafe { mlx_set_device(dev as i32) };
            if set_result != 0 {
                tracing::warn!(?dev, "MLX set device returned error after init");
            }
        }
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

fn runtime_version_enforcement_enabled() -> bool {
    std::env::var(MLX_VERSION_ENFORCEMENT_ENV)
        .map(|raw| {
            matches!(
                raw.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn read_runtime_version() -> String {
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

fn mlx_runtime_version_status() -> MlxRuntimeVersionStatus {
    if MLX_BUILD_VERSION == "unknown" || MLX_BUILD_VERSION == "stub" {
        return MlxRuntimeVersionStatus::Unavailable;
    }

    // Runtime library hashing is not reliable across install layouts; compare version strings.
    let runtime_version = read_runtime_version();

    if runtime_version == "unknown" {
        return MlxRuntimeVersionStatus::Unavailable;
    }

    if runtime_version == MLX_BUILD_VERSION {
        MlxRuntimeVersionStatus::Match {
            build_version: MLX_BUILD_VERSION,
            runtime_version,
        }
    } else {
        MlxRuntimeVersionStatus::Mismatch {
            build_version: MLX_BUILD_VERSION,
            runtime_version,
        }
    }
}

fn validate_mlx_runtime_version() -> Result<()> {
    match mlx_runtime_version_status() {
        MlxRuntimeVersionStatus::Unavailable => Ok(()),
        MlxRuntimeVersionStatus::Match { .. } => Ok(()),
        MlxRuntimeVersionStatus::Mismatch {
            build_version,
            runtime_version,
        } => {
            let enforce = runtime_version_enforcement_enabled();
            let remediation = "Install a matching MLX runtime or rebuild adapterOS against the installed MLX headers.";
            if enforce {
                Err(AosError::DeterminismViolation(format!(
                    "MLX runtime/build version mismatch in determinism-enforcing mode: build_version={}, runtime_version={}, build_hash={}, build_hash_source={}. Remediation: {}",
                    build_version, runtime_version, MLX_BUILD_HASH, MLX_BUILD_HASH_SOURCE, remediation
                )))
            } else {
                tracing::warn!(
                    build_version,
                    runtime_version,
                    build_hash = MLX_BUILD_HASH,
                    build_hash_source = MLX_BUILD_HASH_SOURCE,
                    remediation,
                    "MLX runtime version differs from build-time headers; results may drift across runs"
                );
                Ok(())
            }
        }
    }
}

pub(crate) fn mlx_runtime_init_ffi() -> Result<()> {
    mlx_runtime_init_internal_ffi(None)
}

pub(crate) fn mlx_runtime_init_with_device_ffi(device: MlxDeviceType) -> Result<()> {
    mlx_runtime_init_internal_ffi(Some(device))
}

pub(crate) fn mlx_runtime_is_initialized_ffi() -> bool {
    MLX_INITIALIZED.load(Ordering::SeqCst)
}

/// Initialize MLX runtime safely (idempotent - safe to call multiple times)
///
/// Uses MLX C++ FFI backend.
pub fn mlx_runtime_init() -> Result<()> {
    let _selected = select_mlx_implementation()?;
    mlx_runtime_init_ffi()
}

/// Initialize MLX runtime with specific device type
///
/// # Arguments
/// * `device` - Device type to initialize (Cpu, Gpu, Ane, Auto)
pub fn mlx_runtime_init_with_device(device: MlxDeviceType) -> Result<()> {
    let _selected = select_mlx_implementation()?;
    mlx_runtime_init_with_device_ffi(device)
}

/// Check if MLX runtime is initialized
pub fn mlx_runtime_is_initialized() -> bool {
    mlx_runtime_is_initialized_ffi()
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
    let _selected = select_mlx_implementation()?;
    let mut capabilities = MlxBackendCapabilities::default();
    ffi_error::clear_ffi_error();
    let result = unsafe { mlx_backend_info(&mut capabilities) };
    ffi_error::check_ffi_result(result, "get backend capabilities")?;
    Ok(capabilities)
}

/// Get MLX version string
pub fn mlx_version() -> String {
    match select_mlx_implementation() {
        Ok(MlxImplementation::Ffi) => unsafe {
            let version_ptr = mlx_get_version();
            if version_ptr.is_null() {
                "unknown".to_string()
            } else {
                std::ffi::CStr::from_ptr(version_ptr)
                    .to_string_lossy()
                    .to_string()
            }
        },
        Err(_) => "unknown".to_string(),
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
            seed: None,
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
