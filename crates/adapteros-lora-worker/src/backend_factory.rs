//! Backend factory for creating kernel implementations
//!
//! This module provides factory functions for creating different kernel backends
//! (Metal, CoreML, MLX) and capability detection.
//!
//! ## Model Caching
//!
//! The factory uses a per-worker model cache to deduplicate loaded models.
//! Models are cached by `(backend_type, manifest_hash, kernel_version, quantization, fusion_mode)`
//! to align with the context manifest and avoid cross-build reuse.

use crate::model_handle_cache::{ModelHandle, ModelHandleCache};
use crate::model_key::{ModelCacheIdentity, ModelKey};
use adapteros_config::{
    model, reject_tmp_persistent_path, BackendPreference, ConfigLoader, ModelConfig,
};
use adapteros_core::{
    backend::BackendKind, constants::BYTES_PER_MB, AosError, B3Hash, ExecutionProfile, Result,
};
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_telemetry::TelemetryWriter;
use once_cell::sync::Lazy;
use safetensors::{tensor::TensorView, SafeTensors};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

#[cfg(any(target_os = "macos", feature = "multi-backend"))]
use serde_json::Value;

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_config::CoreMLComputePreference;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use std::str::FromStr;

/// Structure representing the safetensors index file for sharded models
#[derive(serde::Deserialize)]
struct SafeTensorsIndex {
    weight_map: std::collections::HashMap<String, String>,
    /// Metadata from index file (unused but part of the format)
    #[serde(default)]
    #[allow(dead_code)]
    metadata: Option<serde_json::Value>,
}

/// Shared kernel object type with Send + Sync for use across async boundaries
pub type KernelBox = Box<dyn FusedKernels + Send + Sync>;

fn resolve_config_toml_path() -> Result<Option<String>> {
    if let Ok(val) = std::env::var("AOS_CONFIG_TOML") {
        if !val.is_empty() {
            reject_tmp_persistent_path(Path::new(&val), "config-toml")?;
            return Ok(Some(val));
        }
    }

    let default_path = Path::new("configs/cp.toml");
    if default_path.exists() {
        reject_tmp_persistent_path(default_path, "config-toml")?;
        return Ok(Some(default_path.to_string_lossy().to_string()));
    }

    Ok(None)
}

/// Resolve model cache budget in bytes (env overrides config).
fn resolve_model_cache_budget_bytes() -> Result<u64> {
    // 1) Environment override
    if let Ok(raw) = std::env::var("AOS_MODEL_CACHE_MAX_MB") {
        let parsed: u64 = raw.parse().map_err(|_| {
            AosError::Config(format!(
                "Invalid AOS_MODEL_CACHE_MAX_MB '{}': must be a positive integer (MB)",
                raw
            ))
        })?;
        if parsed == 0 {
            return Err(AosError::Config(
                "AOS_MODEL_CACHE_MAX_MB must be greater than zero".to_string(),
            ));
        }
        return Ok(parsed * 1024 * 1024);
    }

    // 2) Config file (env + TOML via ConfigLoader). Prefer explicit path override if provided.
    let toml_path = resolve_config_toml_path()?;

    let config = ConfigLoader::new()
        .load(Vec::new(), toml_path.clone())
        .map_err(|e| {
            let scope = toml_path
                .as_ref()
                .map(|p| format!(" from {}", p))
                .unwrap_or_default();
            AosError::Config(format!(
                "Failed to load configuration{} for model cache budget: {}",
                scope, e
            ))
        })?;

    if let Some(raw) = config.get("model.cache.max.mb") {
        let parsed: u64 = raw.parse().map_err(|_| {
            AosError::Config(format!(
                "Invalid model.cache.max.mb '{}': must be a positive integer (MB)",
                raw
            ))
        })?;
        if parsed == 0 {
            return Err(AosError::Config(
                "model.cache.max.mb must be greater than zero".to_string(),
            ));
        }
        return Ok(parsed * 1024 * 1024);
    }

    Err(AosError::Config(
        "Model cache budget not configured. Set AOS_MODEL_CACHE_MAX_MB or model.cache.max.mb in the config TOML (MB).".to_string(),
    ))
}

/// Per-worker model cache singleton
///
/// This cache ensures that the same model is only loaded once per worker process.
/// Budget must be explicitly provided (env or TOML); missing/zero budgets return errors
/// at first use rather than panicking at initialization.
static MODEL_CACHE: Lazy<std::result::Result<ModelHandleCache, String>> =
    Lazy::new(|| match resolve_model_cache_budget_bytes() {
        Ok(max_bytes) => {
            let max_mb = max_bytes / BYTES_PER_MB;
            info!(
                max_memory_mb = max_mb,
                "Initializing per-worker model cache with explicit budget"
            );
            Ok(ModelHandleCache::new(max_bytes))
        }
        Err(err) => {
            error!(
                error = %err,
                "Model cache budget missing or invalid; model loading will fail"
            );
            Err(err.to_string())
        }
    });

/// Get reference to the model cache, returning an error if initialization failed
///
/// This function is public to allow cleanup during worker shutdown.
pub fn get_model_cache() -> Result<&'static ModelHandleCache> {
    MODEL_CACHE
        .as_ref()
        .map_err(|e| AosError::Config(format!("Model cache initialization failed: {}", e)))
}

/// Validate model cache budget is configured and return the budget in bytes.
///
/// Call this early in worker startup to fail fast with a clear error message
/// rather than discovering the problem during backend creation.
///
/// # Returns
/// - `Ok(budget_bytes)` if the cache is properly configured
/// - `Err(AosError::Config(...))` if budget is missing or invalid
pub fn validate_model_cache_budget() -> Result<u64> {
    // Check environment variable first
    let env_value = std::env::var("AOS_MODEL_CACHE_MAX_MB").ok();

    // Check config file
    let toml_path = resolve_config_toml_path()?;
    let config_value = ConfigLoader::new()
        .load(Vec::new(), toml_path.clone())
        .ok()
        .and_then(|cfg| cfg.get("model.cache.max.mb").map(String::from));

    debug!(
        env_var = ?env_value,
        config_value = ?config_value,
        "Model cache budget configuration check"
    );

    match get_model_cache() {
        Ok(cache) => {
            let budget_bytes = cache.max_memory_bytes();
            let budget_mb = budget_bytes / BYTES_PER_MB;
            info!(
                budget_mb = budget_mb,
                budget_bytes = budget_bytes,
                source = if env_value.is_some() {
                    "AOS_MODEL_CACHE_MAX_MB"
                } else {
                    "config file"
                },
                "Model cache budget validated"
            );
            Ok(budget_bytes)
        }
        Err(e) => {
            // Build a helpful error message with context
            let mut error_msg = String::from("Model cache budget not configured.\n\n");

            error_msg.push_str("Configuration Status:\n");
            if let Some(ref val) = env_value {
                error_msg.push_str(&format!(
                    "  - AOS_MODEL_CACHE_MAX_MB: '{}' (found but may be invalid)\n",
                    val
                ));
            } else {
                error_msg.push_str("  - AOS_MODEL_CACHE_MAX_MB: not set\n");
            }

            if let Some(ref val) = config_value {
                error_msg.push_str(&format!(
                    "  - model.cache.max.mb (config): '{}' (found but may be invalid)\n",
                    val
                ));
            } else {
                error_msg.push_str("  - model.cache.max.mb (config): not set\n");
            }

            error_msg.push_str("\nHow to fix:\n");
            error_msg.push_str("  1. Set environment variable:\n");
            error_msg.push_str("     export AOS_MODEL_CACHE_MAX_MB=8192  # For 8GB cache\n\n");
            error_msg
                .push_str("  2. Or add to config file (configs/cp.toml or configs/aos.toml):\n");
            error_msg.push_str("     [model.cache]\n");
            error_msg.push_str("     max.mb = 8192  # For 8GB cache\n\n");

            error_msg.push_str("Recommended minimums by model size:\n");
            error_msg.push_str("  - 7B models (4-bit):   4096 MB (4GB)\n");
            error_msg.push_str("  - 7B models (fp16):    16384 MB (16GB)\n");
            error_msg.push_str("  - 13B models (4-bit):  8192 MB (8GB)\n");
            error_msg.push_str("  - 32B+ models:         24576+ MB (24GB+)\n\n");

            error_msg.push_str("Documentation: docs/ARCHITECTURE.md#model-caching\n");
            error_msg.push_str(&format!("\nOriginal error: {}", e));

            Err(AosError::Config(error_msg))
        }
    }
}

/// Configure telemetry writer for model cache failure reporting.
pub fn configure_model_cache_telemetry(writer: TelemetryWriter) {
    if let Ok(cache) = get_model_cache() {
        cache.set_telemetry(writer);
    }
}

/// Load and validate model configuration from config.json
///
/// Load and validate model config - returns error on GQA validation failure
///
/// This helper function:
/// 1. Loads the config.json from the model path
/// 2. Validates GQA configuration (attention heads divisible by KV heads)
/// 3. Returns FATAL error on validation failures (BREAKING CHANGE from prior behavior)
///
/// Returns `Ok(Some(config))` if loading and validation succeed.
/// Returns `Ok(None)` if config.json is missing (backend will use defaults).
/// Returns `Err(...)` if validation fails (GQA misconfiguration is FATAL).
#[cfg(any(target_os = "macos", feature = "multi-backend"))]
fn load_and_validate_model_config(model_path: &Path) -> Result<Option<ModelConfig>> {
    match ModelConfig::from_config_json(model_path) {
        Ok(config) => {
            // Validate GQA configuration - this is now FATAL on failure
            config.validate().map_err(|e| {
                error!(
                    model_path = %model_path.display(),
                    error = %e,
                    "Model configuration validation FAILED - cannot proceed with invalid config"
                );
                AosError::Config(format!(
                    "Model config validation failed for '{}': {}",
                    model_path.display(),
                    e
                ))
            })?;
            Ok(Some(config))
        }
        Err(e) => {
            warn!(
                model_path = %model_path.display(),
                error = %e,
                "Failed to load model config.json - backend may use default parameters"
            );
            Ok(None)
        }
    }
}

#[cfg(any(target_os = "macos", feature = "multi-backend"))]
fn build_model_cache_identity(backend_type: BackendType, model_path: &Path) -> ModelCacheIdentity {
    let kernel_version_id = adapteros_core::version::VERSION.to_string();
    let quantization_mode = detect_quantization_mode(model_path).unwrap_or_else(|| {
        crate::model_key::quantization_tag_for_backend(backend_type).to_string()
    });
    let fusion_mode =
        fusion_interval_mode_from_env().unwrap_or_else(crate::model_key::default_fusion_tag);

    ModelCacheIdentity {
        kernel_version_id,
        quantization_mode,
        fusion_mode,
        build_id: Some(adapteros_core::version::VERSION.to_string()),
        adapter_dir_hash: None,
    }
}

#[cfg(any(target_os = "macos", feature = "multi-backend"))]
fn fusion_interval_mode_from_env() -> Option<String> {
    let raw = std::env::var("AOS_FUSION_INTERVAL_MODE")
        .or_else(|_| std::env::var("AOS_FUSION_MODE"))
        .ok()?;

    let normalized = raw.to_lowercase();
    if normalized == "per_request" {
        Some("per_request".to_string())
    } else if normalized == "per_token" {
        Some("per_token".to_string())
    } else if let Some(rest) = normalized.strip_prefix("per_segment:") {
        Some(format!("per_segment-{}", rest))
    } else {
        warn!(
            mode = %raw,
            "Unrecognized fusion interval mode; falling back to default"
        );
        None
    }
}

#[cfg(any(target_os = "macos", feature = "multi-backend"))]
fn detect_quantization_mode(model_path: &Path) -> Option<String> {
    let config_path = if model_path.is_dir() {
        model_path.join("config.json")
    } else {
        model_path.to_path_buf()
    };

    let contents = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            // Log instead of silently ignoring - this helps diagnose model loading issues
            debug!(
                config_path = %config_path.display(),
                error = %e,
                "Could not read config.json for quantization detection"
            );
            return None;
        }
    };

    let json: Value = match serde_json::from_str(&contents) {
        Ok(j) => j,
        Err(e) => {
            // Log parse errors - malformed config.json is a significant issue
            warn!(
                config_path = %config_path.display(),
                error = %e,
                "Failed to parse config.json - model may have invalid configuration"
            );
            return None;
        }
    };

    if let Some(qcfg) = json.get("quantization_config") {
        let digest = B3Hash::hash(serde_json::to_vec(qcfg).unwrap_or_default().as_slice());
        debug!(
            config_path = %config_path.display(),
            quantization_hash = %&digest.to_hex()[..12],
            "Detected quantization_config in model"
        );
        return Some(format!("quant_cfg-{}", &digest.to_hex()[..12]));
    }

    if let Some(q) = json.get("quantization") {
        if let Some(as_str) = q.as_str() {
            debug!(
                config_path = %config_path.display(),
                quantization = %as_str,
                "Detected quantization string in model"
            );
            return Some(format!("quantization-{as_str}"));
        }
        let digest = B3Hash::hash(serde_json::to_vec(q).unwrap_or_default().as_slice());
        debug!(
            config_path = %config_path.display(),
            quantization_hash = %&digest.to_hex()[..12],
            "Detected quantization object in model"
        );
        return Some(format!("quantization-{}", &digest.to_hex()[..12]));
    }

    debug!(
        config_path = %config_path.display(),
        "No quantization config found in model config.json"
    );
    None
}

/// Canonical backend choice for kernel creation.
///
/// This is an alias of `BackendKind` to keep public signatures stable while
/// consolidating backend parsing and display logic in a single place.
pub type BackendChoice = adapteros_core::backend::BackendKind;

/// Backend strategy for automatic selection
#[derive(Debug, Clone)]
pub enum BackendStrategy {
    /// Use Metal as primary with CoreML fallback
    MetalWithCoreMLFallback,
    /// Use CoreML as primary with Metal fallback (power-efficient)
    CoreMLWithMetalFallback,
    /// Use MLX as primary (experimental)
    MlxPrimary,
    /// Use Metal only without fallback
    MetalOnly,
}

/// Context used to make deterministic backend selection decisions.
///
/// Bundles the request `ExecutionProfile` with the detected hardware
/// `BackendCapabilities` so the selection logic always receives the same
/// inputs in a single value.
#[derive(Debug, Clone)]
pub struct SelectionContext {
    pub profile: ExecutionProfile,
    pub capabilities: BackendCapabilities,
}

impl SelectionContext {
    pub fn new(profile: ExecutionProfile, capabilities: BackendCapabilities) -> Self {
        Self {
            profile,
            capabilities,
        }
    }
}

impl BackendStrategy {
    /// Select the appropriate backend based on capabilities
    pub fn select_backend(
        &self,
        capabilities: &BackendCapabilities,
        _model_size_bytes: Option<usize>,
    ) -> Result<BackendChoice> {
        match self {
            BackendStrategy::MetalWithCoreMLFallback => {
                if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
                } else if capabilities.has_coreml && capabilities.has_ane {
                    Ok(BackendChoice::CoreML)
                } else {
                    Err(AosError::Config(
                        "No suitable backend available".to_string(),
                    ))
                }
            }
            BackendStrategy::CoreMLWithMetalFallback => {
                if capabilities.has_coreml && capabilities.has_ane {
                    Ok(BackendChoice::CoreML)
                } else if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
                } else {
                    Err(AosError::Config(
                        "No suitable backend available".to_string(),
                    ))
                }
            }
            BackendStrategy::MlxPrimary => {
                if capabilities.has_mlx {
                    Ok(BackendChoice::Mlx)
                } else {
                    Err(AosError::Config(
                        "MLX backend not available (requires multi-backend feature)".to_string(),
                    ))
                }
            }
            BackendStrategy::MetalOnly => {
                if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
                } else {
                    Err(AosError::Config("Metal backend not available".to_string()))
                }
            }
        }
    }
}

/// Backend capabilities detected on the system
#[derive(Debug, Clone, Default)]
pub struct BackendCapabilities {
    /// Metal GPU is available
    pub has_metal: bool,
    /// Metal device name (if available)
    pub metal_device_name: Option<String>,
    /// Apple Neural Engine is available
    pub has_ane: bool,
    /// CoreML framework is available
    pub has_coreml: bool,
    /// MLX library is available
    pub has_mlx: bool,
    /// MLX subprocess bridge is available (Python + mlx-lm)
    pub has_mlx_bridge: bool,
    /// Total GPU/unified memory in bytes
    pub gpu_memory_bytes: Option<u64>,
}

/// Detect available backend capabilities at runtime
pub fn detect_capabilities() -> BackendCapabilities {
    let mut caps = BackendCapabilities::default();

    // Detect Metal availability
    #[cfg(target_os = "macos")]
    {
        caps.has_metal = detect_metal_device(&mut caps);
    }

    // Detect CoreML/ANE availability
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    {
        caps.has_coreml = true;
        caps.has_ane = detect_neural_engine();
    }

    #[cfg(all(target_os = "macos", not(feature = "coreml-backend")))]
    {
        // CoreML feature not enabled, but we can still check if ANE would be available
        caps.has_coreml = false;
        caps.has_ane = is_apple_silicon();
    }

    // Detect MLX availability - only report true if real MLX is available
    #[cfg(feature = "multi-backend")]
    {
        #[cfg(feature = "mlx")]
        {
            // Real MLX available - check if runtime can be initialized
            use adapteros_lora_mlx_ffi::{mlx_runtime_init, mlx_runtime_is_initialized};
            caps.has_mlx = mlx_runtime_is_initialized() || mlx_runtime_init().is_ok();
        }
        #[cfg(not(feature = "mlx"))]
        {
            // Only stub available - be honest about it
            caps.has_mlx = false;
            debug!("MLX backend not available: 'mlx' feature not enabled (stub mode only)");
        }
    }

    // Detect MLX bridge availability (Python + mlx-lm)
    #[cfg(feature = "mlx-bridge")]
    {
        caps.has_mlx_bridge = detect_mlx_bridge_availability();
    }

    debug!(
        has_metal = caps.has_metal,
        metal_device = ?caps.metal_device_name,
        has_ane = caps.has_ane,
        has_coreml = caps.has_coreml,
        has_mlx = caps.has_mlx,
        has_mlx_bridge = caps.has_mlx_bridge,
        gpu_memory_mb = caps.gpu_memory_bytes.map(|b| b / BYTES_PER_MB),
        "Backend capabilities detected"
    );

    caps
}

/// Detect if the MLX subprocess bridge is available
///
/// This checks if Python 3 and mlx-lm are installed and accessible.
#[cfg(feature = "mlx-bridge")]
fn detect_mlx_bridge_availability() -> bool {
    use std::process::Command;

    // Try to run python3 with a quick mlx-lm import check
    let result = Command::new("python3")
        .args(["-c", "import mlx_lm; print('ok')"])
        .output();

    match result {
        Ok(output) => {
            let success = output.status.success();
            if success {
                debug!("MLX bridge available: python3 and mlx-lm installed");
            } else {
                debug!(
                    stderr = String::from_utf8_lossy(&output.stderr).as_ref(),
                    "MLX bridge unavailable: mlx-lm import failed"
                );
            }
            success
        }
        Err(e) => {
            debug!(error = %e, "MLX bridge unavailable: python3 not found");
            false
        }
    }
}

/// Check if a model is a Mixture of Experts (MoE) model
///
/// MoE models require special handling and typically need the MLX bridge
/// as they may not be supported by the standard FFI backends.
pub fn is_moe_model(model_path: &std::path::Path) -> bool {
    let config_path = model_path.join("config.json");
    if !config_path.exists() {
        return false;
    }

    match std::fs::read_to_string(&config_path) {
        Ok(contents) => {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
                // Check for num_experts field (common in MoE configs)
                if let Some(num_experts) = json
                    .get("num_experts")
                    .or_else(|| json.get("num_local_experts"))
                {
                    if let Some(n) = num_experts.as_u64() {
                        if n > 0 {
                            debug!(
                                model_path = %model_path.display(),
                                num_experts = n,
                                "Detected MoE model by num_experts field"
                            );
                            return true;
                        }
                    }
                }

                // Check model_type for known MoE architectures
                if let Some(model_type) = json.get("model_type").and_then(|v| v.as_str()) {
                    let model_type_lower = model_type.to_lowercase();
                    if model_type_lower.contains("moe")
                        || model_type_lower.contains("mixtral")
                        || model_type_lower == "qwen2moe"
                        || model_type_lower == "dbrx"
                    {
                        debug!(
                            model_path = %model_path.display(),
                            model_type = model_type,
                            "Detected MoE model by model_type"
                        );
                        return true;
                    }
                }

                // Check architectures array
                if let Some(archs) = json.get("architectures").and_then(|v| v.as_array()) {
                    for arch in archs {
                        if let Some(arch_str) = arch.as_str() {
                            let arch_lower = arch_str.to_lowercase();
                            if arch_lower.contains("moe") || arch_lower.contains("mixtral") {
                                debug!(
                                    model_path = %model_path.display(),
                                    architecture = arch_str,
                                    "Detected MoE model by architecture"
                                );
                                return true;
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            warn!(
                model_path = %model_path.display(),
                error = %e,
                "Failed to read config.json for MoE detection"
            );
        }
    }

    false
}

/// Detect Metal device and populate capability info
#[cfg(target_os = "macos")]
fn detect_metal_device(caps: &mut BackendCapabilities) -> bool {
    use metal::Device;

    if let Some(device) = Device::system_default() {
        caps.metal_device_name = Some(device.name().to_string());
        // Get recommended max working set size as GPU memory estimate
        caps.gpu_memory_bytes = Some(device.recommended_max_working_set_size());
        true
    } else {
        warn!("No Metal device found on macOS system");
        false
    }
}

/// Detect if Neural Engine is available via CoreML
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn detect_neural_engine() -> bool {
    use adapteros_lora_kernel_coreml::is_neural_engine_available;
    is_neural_engine_available()
}

/// Check if running on Apple Silicon (M1/M2/M3/M4)
#[cfg(target_os = "macos")]
fn is_apple_silicon() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        true
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        false
    }
}

/// Automatic backend selection with fallback chain
///
/// Selection order is defined centrally in `BackendKind::inference_priority()`:
/// CoreML → MLX → Metal → CPU. CPU remains an observability-only terminal entry
/// (no CPU kernels are implemented).
pub fn auto_select_backend(capabilities: &BackendCapabilities) -> Result<BackendChoice> {
    let mut skipped: Vec<String> = Vec::new();

    for backend in BackendKind::inference_priority() {
        match backend {
            BackendKind::CoreML => {
                if capabilities.has_coreml && capabilities.has_ane {
                    if !skipped.is_empty() {
                        info!(
                            selected = "coreml",
                            skipped = skipped.join("; "),
                            "Auto-selected CoreML after evaluating higher-priority fallbacks"
                        );
                    } else {
                        info!("Auto-selected CoreML backend with Neural Engine");
                    }
                    return Ok(BackendChoice::CoreML);
                }
                skipped.push(format!(
                    "coreml_unavailable(has_coreml={},has_ane={})",
                    capabilities.has_coreml, capabilities.has_ane
                ));
            }
            BackendKind::Mlx => {
                if cfg!(feature = "multi-backend") && capabilities.has_mlx {
                    info!(
                        selected = "mlx",
                        skipped = skipped.join("; "),
                        "Auto-selected MLX backend"
                    );
                    return Ok(BackendChoice::Mlx);
                }
                skipped.push("mlx_unavailable_or_feature_disabled".to_string());
            }
            BackendKind::MlxBridge => {
                if cfg!(feature = "mlx-bridge") && capabilities.has_mlx_bridge {
                    info!(
                        selected = "mlxbridge",
                        skipped = skipped.join("; "),
                        "Auto-selected MLX Bridge backend"
                    );
                    return Ok(BackendChoice::MlxBridge);
                }
                skipped.push("mlxbridge_unavailable_or_feature_disabled".to_string());
            }
            BackendKind::Metal => {
                if capabilities.has_metal {
                    info!(
                        selected = "metal",
                        device = ?capabilities.metal_device_name,
                        skipped = skipped.join("; "),
                        "Auto-selected Metal backend"
                    );
                    return Ok(BackendChoice::Metal);
                }
                skipped.push("metal_unavailable".to_string());
            }
            BackendKind::CPU => {
                skipped.push("cpu_backend_not_supported_for_inference".to_string());
            }
            BackendKind::Auto => {
                // Auto should never appear in the priority list
            }
        }
    }

    info!(
        skipped = skipped.join("; "),
        "Auto backend selection exhausted all options"
    );
    Err(AosError::Config(
        "No suitable backend available. Checked priority CoreML → MLX → Metal → CPU.".to_string(),
    ))
}

/// Automatic backend selection with MoE model awareness
///
/// This function checks if the model is a Mixture of Experts (MoE) model and
/// automatically selects the MLX Bridge backend if so, as MoE models may not
/// be fully supported by other backends.
///
/// # Arguments
/// * `model_path` - Path to the model directory
/// * `capabilities` - Backend capabilities
///
/// # Returns
/// The selected backend choice, preferring MLX Bridge for MoE models
pub fn auto_select_backend_with_model(
    model_path: &Path,
    capabilities: &BackendCapabilities,
) -> Result<BackendChoice> {
    // Check if model is MoE
    if is_moe_model(model_path) {
        info!(
            model_path = %model_path.display(),
            "Detected MoE model, checking if MLX Bridge is available"
        );

        // Prefer MLX Bridge for MoE models
        if cfg!(feature = "mlx-bridge") && capabilities.has_mlx_bridge {
            info!(
                model_path = %model_path.display(),
                "Auto-selected MLX Bridge backend for MoE model"
            );
            return Ok(BackendChoice::MlxBridge);
        } else {
            warn!(
                model_path = %model_path.display(),
                mlx_bridge_enabled = cfg!(feature = "mlx-bridge"),
                has_mlx_bridge = capabilities.has_mlx_bridge,
                "MoE model detected but MLX Bridge not available, falling back to standard selection"
            );
        }
    }

    // Fall back to standard auto-selection for non-MoE models or if MLX Bridge unavailable
    auto_select_backend(capabilities)
}

/// Result of selecting a backend from an ExecutionProfile.
#[derive(Debug, Clone)]
pub struct BackendSelection {
    pub selected: BackendChoice,
    pub overridden: bool,
    pub reason: Option<&'static str>,
}

impl BackendSelection {
    pub fn new(selected: BackendChoice) -> Self {
        Self {
            selected,
            overridden: false,
            reason: None,
        }
    }
}

/// Resolve backend choice using the canonical ExecutionProfile and capabilities.
pub fn select_backend_from_execution_profile(
    context: &SelectionContext,
) -> Result<BackendSelection> {
    let requested = context.profile.backend_profile;
    let capabilities = &context.capabilities;
    let selection = match requested {
        BackendKind::Auto => BackendSelection::new(auto_select_backend(capabilities)?),
        BackendKind::CoreML => match auto_select_backend(capabilities) {
            Ok(choice) => {
                if choice == BackendChoice::CoreML {
                    BackendSelection::new(BackendChoice::CoreML)
                } else {
                    BackendSelection {
                        selected: choice,
                        overridden: true,
                        reason: Some(match choice {
                            BackendChoice::Mlx => "coreml_unavailable_fallback_mlx",
                            BackendChoice::MlxBridge => "coreml_unavailable_fallback_mlxbridge",
                            BackendChoice::Metal => "coreml_unavailable_fallback_metal",
                            BackendChoice::CPU => "coreml_unavailable_fallback_cpu",
                            BackendChoice::CoreML => "coreml_unavailable_fallback_coreml",
                            BackendChoice::Auto => "coreml_unavailable_fallback_auto",
                        }),
                    }
                }
            }
            Err(_) => {
                return Err(AosError::Config(
                    "Requested CoreML backend is not available (ANE/CoreML missing)".to_string(),
                ))
            }
        },
        BackendKind::Metal => {
            if capabilities.has_metal {
                BackendSelection::new(BackendChoice::Metal)
            } else {
                return Err(AosError::Config(
                    "Requested Metal backend is not available".to_string(),
                ));
            }
        }
        BackendKind::Mlx => {
            if cfg!(feature = "multi-backend") {
                if capabilities.has_mlx {
                    BackendSelection::new(BackendChoice::Mlx)
                } else {
                    return Err(AosError::Config(
                        "Requested MLX backend is not available (enable multi-backend)".to_string(),
                    ));
                }
            } else {
                return Err(AosError::Config(
                    "Requested MLX backend is not available (enable multi-backend)".to_string(),
                ));
            }
        }
        BackendKind::MlxBridge => {
            if cfg!(feature = "mlx-bridge") {
                if capabilities.has_mlx_bridge {
                    BackendSelection::new(BackendChoice::MlxBridge)
                } else {
                    // Fall back to MLX FFI if available
                    if cfg!(feature = "multi-backend") && capabilities.has_mlx {
                        info!("MLX bridge unavailable, falling back to MLX FFI");
                        BackendSelection {
                            selected: BackendChoice::Mlx,
                            overridden: true,
                            reason: Some("mlxbridge_unavailable_fallback_mlx"),
                        }
                    } else {
                        return Err(AosError::Config(
                            "Requested MLX Bridge backend is not available (Python/mlx-lm missing)"
                                .to_string(),
                        ));
                    }
                }
            } else {
                return Err(AosError::Config(
                    "Requested MLX Bridge backend is not available (enable mlx-bridge feature)"
                        .to_string(),
                ));
            }
        }
        BackendKind::CPU => {
            return Err(AosError::Config(
                "CPU backend is not supported for inference kernels".to_string(),
            ))
        }
    };

    Ok(selection)
}

/// Create a kernel backend from unified ModelConfig
///
/// This is the preferred entry point for creating backends.
/// It uses the unified configuration system for consistent backend creation.
///
/// # Example
/// ```no_run
/// use adapteros_config::{ModelConfig, BackendPreference};
/// use adapteros_lora_worker::backend_factory::create_backend_from_config;
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), adapteros_core::AosError> {
/// let mut config = ModelConfig::new(PathBuf::from("./var/model-cache/models/qwen2.5-7b-instruct-bf16"));
/// config.backend = BackendPreference::CoreML;
/// let backend = create_backend_from_config(&config)?;
/// # Ok(())
/// # }
/// ```
pub fn create_backend_from_config(config: &ModelConfig) -> Result<KernelBox> {
    let choice = match config.backend {
        BackendPreference::Auto => BackendChoice::Auto,
        BackendPreference::CoreML => BackendChoice::CoreML,
        BackendPreference::Metal => BackendChoice::Metal,
        BackendPreference::Mlx => BackendChoice::Mlx,
        BackendPreference::MlxBridge => BackendChoice::MlxBridge,
        BackendPreference::CPU => BackendChoice::CPU,
    };
    create_backend_with_model(choice, &config.path)
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
#[derive(Debug, Clone)]
pub struct CoreMLBackendSettings {
    pub preference: CoreMLComputePreference,
    pub compute_units: adapteros_lora_kernel_coreml::ComputeUnits,
    pub production_mode: bool,
    pub gpu_available: bool,
    pub ane_available: bool,
    pub gpu_used: bool,
    pub ane_used: bool,
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn resolve_coreml_config_settings() -> (CoreMLComputePreference, bool) {
    if let Some(cfg) = adapteros_config::try_effective_config() {
        return (cfg.coreml.compute_preference, cfg.coreml.production_mode);
    }

    if let Ok(runtime) = adapteros_config::config_or_default() {
        let preference = runtime
            .get_string("AOS_COREML_COMPUTE_PREFERENCE")
            .and_then(|v| CoreMLComputePreference::from_str(v).ok())
            .unwrap_or_default();
        let production_mode = runtime
            .get_bool("AOS_COREML_PRODUCTION_MODE")
            .unwrap_or(false);
        return (preference, production_mode);
    }

    (CoreMLComputePreference::default(), false)
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn coreml_units_from_preference(
    preference: CoreMLComputePreference,
) -> adapteros_lora_kernel_coreml::ComputeUnits {
    match preference {
        CoreMLComputePreference::CpuOnly => adapteros_lora_kernel_coreml::ComputeUnits::CpuOnly,
        CoreMLComputePreference::CpuAndGpu => adapteros_lora_kernel_coreml::ComputeUnits::CpuAndGpu,
        CoreMLComputePreference::CpuAndNe => {
            adapteros_lora_kernel_coreml::ComputeUnits::CpuAndNeuralEngine
        }
        CoreMLComputePreference::All => adapteros_lora_kernel_coreml::ComputeUnits::All,
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub fn resolve_coreml_backend_settings() -> CoreMLBackendSettings {
    let (preference, production_mode) = resolve_coreml_config_settings();
    let caps = adapteros_lora_kernel_coreml::get_system_capabilities();
    let gpu_available = caps & adapteros_lora_kernel_coreml::capabilities::GPU != 0;
    let ane_available = caps & adapteros_lora_kernel_coreml::capabilities::NEURAL_ENGINE != 0;

    let mut compute_units = coreml_units_from_preference(preference);

    if !production_mode {
        let gpu_required = matches!(
            compute_units,
            adapteros_lora_kernel_coreml::ComputeUnits::CpuAndGpu
                | adapteros_lora_kernel_coreml::ComputeUnits::All
        );
        let ane_required = matches!(
            compute_units,
            adapteros_lora_kernel_coreml::ComputeUnits::CpuAndNeuralEngine
                | adapteros_lora_kernel_coreml::ComputeUnits::All
        );

        if gpu_required && !gpu_available {
            warn!(
                requested = ?compute_units,
                "CoreML GPU not available; falling back to CPU-only compute units"
            );
            compute_units = adapteros_lora_kernel_coreml::ComputeUnits::CpuOnly;
        }

        if ane_required && !ane_available {
            warn!(
                requested = ?compute_units,
                "CoreML Neural Engine not available; falling back to CPU-only compute units"
            );
            compute_units = adapteros_lora_kernel_coreml::ComputeUnits::CpuOnly;
        }
    }

    let gpu_used = gpu_available
        && matches!(
            compute_units,
            adapteros_lora_kernel_coreml::ComputeUnits::CpuAndGpu
                | adapteros_lora_kernel_coreml::ComputeUnits::All
        );
    let ane_used = ane_available
        && matches!(
            compute_units,
            adapteros_lora_kernel_coreml::ComputeUnits::CpuAndNeuralEngine
                | adapteros_lora_kernel_coreml::ComputeUnits::All
        );

    CoreMLBackendSettings {
        preference,
        compute_units,
        production_mode,
        gpu_available,
        ane_available,
        gpu_used,
        ane_used,
    }
}

/// Create a kernel backend with an explicit model path
///
/// Use this function when you have a `BackendChoice` and need to provide a model path.
/// For MLX backend, the model path is required. For CoreML and Metal, it's optional.
///
/// # Arguments
/// * `choice` - The backend choice
/// * `model_path` - Path to the model directory
///
/// # Example
/// ```rust,no_run
/// use std::path::Path;
/// use adapteros_lora_worker::backend_factory::{BackendChoice, create_backend_with_model};
///
/// let backend = create_backend_with_model(
///     BackendChoice::Mlx,
///     Path::new("${AOS_MODEL_CACHE_DIR}/${AOS_BASE_MODEL_ID}"),
/// )?;
/// # Ok::<(), adapteros_core::AosError>(())
/// ```
pub fn create_backend_with_model(choice: BackendChoice, model_path: &Path) -> Result<KernelBox> {
    match choice {
        BackendChoice::Auto => {
            let capabilities = detect_capabilities();
            // Use MoE-aware selection that checks model configuration
            let selected = auto_select_backend_with_model(model_path, &capabilities)?;
            create_backend_with_model(selected, model_path)
        }
        BackendChoice::CPU => Err(AosError::Config(
            "CPU backend is not supported for inference kernels".to_string(),
        )),
        BackendChoice::Metal => {
            // Delegate to helper (no manifest hash in this legacy path, no integrity verification)
            create_metal_backend(model_path, None, None)
        }
        BackendChoice::CoreML => {
            #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
            {
                use adapteros_lora_kernel_coreml::{init_coreml, CoreMLBackend, CoreMLModelParams};

                // Initialize CoreML runtime
                init_coreml()?;

                let settings = resolve_coreml_backend_settings();

                // Load model configuration from config.json if available (with validation)
                let model_config = load_and_validate_model_config(model_path)?;
                if let Some(ref cfg) = model_config {
                    info!(
                        architecture = %cfg.architecture,
                        hidden_size = cfg.hidden_size,
                        num_attention_heads = cfg.num_attention_heads,
                        num_kv_heads = cfg.num_key_value_heads,
                        rope_theta = cfg.rope_theta,
                        "Loaded model configuration for CoreML backend"
                    );
                }

                info!(
                    backend = "coreml",
                    model_path = %model_path.display(),
                    compute_preference = %settings.preference,
                    compute_units = ?settings.compute_units,
                    production_mode = settings.production_mode,
                    gpu_available = settings.gpu_available,
                    ane_available = settings.ane_available,
                    gpu_used = settings.gpu_used,
                    ane_used = settings.ane_used,
                    "Creating CoreML kernel backend"
                );
                let mut backend =
                    CoreMLBackend::new(settings.compute_units, settings.production_mode)?;

                // Set model parameters from config.json if available
                if let Some(cfg) = model_config {
                    backend.set_model_params(CoreMLModelParams::new(
                        cfg.hidden_size,
                        cfg.num_attention_heads,
                        cfg.num_key_value_heads,
                        cfg.intermediate_size,
                        cfg.rope_theta,
                        cfg.max_seq_len,
                    ));
                }

                Ok(Box::new(backend))
            }
            #[cfg(all(target_os = "macos", not(feature = "coreml-backend")))]
            {
                let _ = model_path;
                Err(AosError::Config(
                    "CoreML backend requires 'coreml-backend' feature to be enabled. \
                     Build with: cargo build --features coreml-backend"
                        .to_string(),
                ))
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = model_path;
                Err(AosError::Config(
                    "CoreML backend requires macOS".to_string(),
                ))
            }
        }
        BackendChoice::Mlx => create_mlx_backend(model_path, None, None),
        BackendChoice::MlxBridge => create_mlx_bridge_backend(model_path, None),
    }
}

/// Create a kernel backend with an explicit model path and manifest hash for determinism
///
/// Use this function when you need deterministic execution with HKDF-seeded RNG.
/// The manifest hash is used to derive the MLX RNG seed for reproducible results.
///
/// # Arguments
/// * `choice` - The backend choice
/// * `model_path` - Path to the model directory
/// * `manifest_hash` - Optional manifest hash for deterministic seeding
///
/// # Example
/// ```rust,ignore
/// use std::path::Path;
/// use adapteros_core::B3Hash;
/// use adapteros_lora_worker::backend_factory::{BackendChoice, create_backend_with_model_and_hash};
///
/// let hash = B3Hash::hash(b"model-manifest");
/// let backend = create_backend_with_model_and_hash(
///     BackendChoice::Mlx,
///     Path::new("./var/model-cache/models/qwen2.5-7b-instruct-bf16"),
///     Some(&hash)
/// )?;
/// ```
#[deprecated(
    since = "0.12.0",
    note = "Use create_backend_with_model_hashes() which properly separates manifest_hash (for cache key) from model_weights_hash (for integrity verification). This function skips integrity verification."
)]
pub fn create_backend_with_model_and_hash(
    choice: BackendChoice,
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    warn!(
        "create_backend_with_model_and_hash is deprecated: model integrity verification SKIPPED. \
         Use create_backend_with_model_hashes with manifest.base.model_hash for proper verification."
    );
    create_backend_with_model_hashes(choice, model_path, manifest_hash, None)
}

/// Create a backend with proper hash verification
///
/// # Arguments
/// * `choice` - Backend type to create
/// * `model_path` - Path to model directory
/// * `manifest_hash` - Hash of manifest JSON (used for cache key identity)
/// * `model_weights_hash` - Hash of model weights from manifest.base.model_hash (used for integrity verification)
///
/// If `model_weights_hash` is None, integrity verification is skipped with a warning.
pub fn create_backend_with_model_hashes(
    choice: BackendChoice,
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
    model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    match choice {
        BackendChoice::Mlx => create_mlx_backend(model_path, manifest_hash, model_weights_hash),
        BackendChoice::MlxBridge => create_mlx_bridge_backend(model_path, manifest_hash),
        BackendChoice::Metal => create_metal_backend(model_path, manifest_hash, model_weights_hash),
        BackendChoice::CoreML => {
            create_coreml_backend(model_path, manifest_hash, model_weights_hash)
        }
        // Other backends fallback to basic path (no hash verification)
        _ => create_backend_with_model(choice, model_path),
    }
}

/// Detect if model is MoE (Mixture of Experts) by checking config.json
#[cfg(feature = "multi-backend")]
fn is_moe_model(model_path: &Path) -> bool {
    let config_path = model_path.join("config.json");
    if !config_path.exists() {
        return false;
    }

    match std::fs::read_to_string(&config_path) {
        Ok(content) => {
            if let Ok(config) = serde_json::from_str::<Value>(&content) {
                // Check for num_experts field (MoE indicator)
                if let Some(num_experts) = config.get("num_experts").and_then(|v| v.as_u64()) {
                    if num_experts > 1 {
                        info!(
                            model_path = %model_path.display(),
                            num_experts = num_experts,
                            "Detected MoE model architecture"
                        );
                        return true;
                    }
                }
            }
            false
        }
        Err(_) => false,
    }
}

/// Internal helper to create MLX backend with optional manifest hash
#[cfg(feature = "multi-backend")]
fn create_mlx_backend(
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
    model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    // FAIL-FAST: Validate MLX feature is enabled before attempting backend creation
    #[cfg(not(feature = "mlx"))]
    {
        return Err(AosError::Config(
            "MLX backend selected but 'mlx' feature not enabled. Rebuild with --features mlx"
                .to_string(),
        ));
    }

    let model_path = validate_mlx_model_dir(model_path)?;
    let model_path_str = model_path.to_string_lossy();

    // Check if this is a MoE model that requires subprocess bridge
    if is_moe_model(&model_path) {
        info!(
            model_path = %model_path_str,
            "Using MLX subprocess bridge for MoE model"
        );
        return create_mlx_subprocess_backend(&model_path, manifest_hash);
    }

    use adapteros_lora_mlx_ffi::{
        mlx_runtime_init, mlx_runtime_is_initialized, MLXFFIBackend, MLXFFIModel,
    };

    info!(
        model_path = %model_path_str,
        has_manifest_hash = manifest_hash.is_some(),
        has_model_weights_hash = model_weights_hash.is_some(),
        "Creating MLX FFI kernel backend"
    );

    let manifest_hash = manifest_hash.ok_or_else(|| {
        AosError::Config(
            "Manifest hash is required for MLX backend identity; pass manifest hash to backend factory"
                .to_string(),
        )
    })?;

    // Verify model integrity before loading (using model weights hash, not manifest hash)
    verify_model_integrity(&model_path, model_weights_hash, "MLX")?;

    // Ensure MLX runtime is initialized
    if !mlx_runtime_is_initialized() {
        mlx_runtime_init()
            .map_err(|e| AosError::Config(format!("Failed to initialize MLX runtime: {}", e)))?;
    }

    // Create cache key - prefer manifest hash when available for canonical identity
    let cache_key = ModelKey::new(
        BackendType::MLX,
        *manifest_hash,
        build_model_cache_identity(BackendType::MLX, &model_path),
    );
    let model_arc = get_model_cache()?
        .get_or_load(&cache_key, || {
            let model = MLXFFIModel::load(&model_path).map_err(|e| {
                AosError::Config(format!(
                    "Failed to load MLX model from '{}': {}",
                    model_path_str, e
                ))
            })?;
            // Estimate memory: use config if available, otherwise estimate from architecture
            let memory_bytes = estimate_mlx_model_memory(&model_path)?;
            Ok((ModelHandle::Mlx(Arc::new(model)), memory_bytes))
        })?
        .as_mlx_model()?;

    // Create backend with or without manifest hash for deterministic seeding
    info!("Creating MLX backend with HKDF-seeded determinism from manifest hash");
    let backend: KernelBox = Box::new(
        MLXFFIBackend::with_manifest_hash_arc(model_arc, manifest_hash.clone()).map_err(|e| {
            AosError::Config(format!(
                "Failed to create MLX backend with manifest hash: {}",
                e
            ))
        })?,
    );

    Ok(backend)
}

/// Create MLX subprocess bridge backend for MoE models
#[cfg(feature = "multi-backend")]
fn create_mlx_subprocess_bridge(
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    // FAIL-FAST: Validate MLX feature is enabled before attempting backend creation
    #[cfg(not(feature = "mlx"))]
    {
        return Err(AosError::Config(
            "MLX backend selected but 'mlx' feature not enabled. Rebuild with --features mlx"
                .to_string(),
        ));
    }

    use crate::mlx_subprocess_bridge::MLXSubprocessBridge;

    // Get vocab size from config
    let vocab_size = if let Some(config) = load_and_validate_model_config(model_path)? {
        config.vocab_size
    } else {
        // Default vocab size for common models
        warn!(
            model_path = %model_path.display(),
            "Could not load config.json for vocab size, using default 151936"
        );
        151936 // Qwen default
    };

    info!(
        model_path = %model_path.display(),
        vocab_size = vocab_size,
        "Creating MLX subprocess bridge backend"
    );

    let bridge = MLXSubprocessBridge::with_config(
        model_path.to_path_buf(),
        vocab_size,
        None, // Use default python3
        manifest_hash.cloned(),
    )?;

    Ok(Box::new(bridge))
}

/// Estimate MLX model memory usage from config.json
#[cfg(feature = "multi-backend")]
fn estimate_mlx_model_memory(model_path: &Path) -> Result<u64> {
    // Try to load config.json for accurate estimate (uses validated config loader)
    if let Some(config) = load_and_validate_model_config(model_path)? {
        // Estimate: 4 bytes per param (f32), with typical model structure
        // hidden_size * num_layers * 12 (approx params per layer) * 4 bytes
        let params_estimate = config.hidden_size as u64
            * config.num_layers as u64
            * 12  // Approximate number of weight matrices per layer
            * config.hidden_size as u64
            / 1000; // Normalize
        let memory_estimate = params_estimate * 4; // 4 bytes per f32 param

        // Add 10% overhead
        let estimate = (memory_estimate as f64 * 1.1) as u64;
        debug!(
            model_path = %model_path.display(),
            hidden_size = config.hidden_size,
            num_layers = config.num_layers,
            estimated_memory_mb = estimate / (1024 * 1024),
            "Estimated model memory from config.json"
        );
        return Ok(estimate);
    }

    // Fallback: assume 7B model (~14GB for fp16, ~28GB for fp32)
    warn!(
        model_path = %model_path.display(),
        default_estimate_gb = 14,
        "Could not load config.json for memory estimation, using 14GB default"
    );
    Ok(14 * 1024 * 1024 * 1024) // 14GB default estimate
}

#[cfg(not(feature = "multi-backend"))]
fn create_mlx_backend(
    _model_path: &Path,
    _manifest_hash: Option<&B3Hash>,
    _model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    Err(AosError::Config(
        "MLX backend requires 'multi-backend' feature to be enabled. \
         Build with: cargo build --features multi-backend"
            .to_string(),
    ))
}

/// Internal helper to create MLX subprocess bridge backend
#[cfg(feature = "mlx-bridge")]
fn create_mlx_bridge_backend(
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    use crate::mlx_subprocess_bridge::{MLXSubprocessBridge, MlxBridgeConfig};

    info!(
        model_path = %model_path.display(),
        has_manifest_hash = manifest_hash.is_some(),
        "Creating MLX subprocess bridge backend"
    );

    // Validate model path exists
    if !model_path.exists() {
        return Err(AosError::Config(format!(
            "Model path does not exist: {}",
            model_path.display()
        )));
    }

    // Load config to get vocab_size
    let config = load_and_validate_model_config(model_path)?;
    let vocab_size = config.map(|c| c.vocab_size).unwrap_or(152064); // Default for Qwen2.5

    // Create bridge configuration
    let bridge_config = MlxBridgeConfig::default();

    // Create the bridge backend
    let bridge = MLXSubprocessBridge::with_full_config(
        model_path.to_path_buf(),
        vocab_size,
        bridge_config,
        manifest_hash.cloned(),
    )?;

    info!(
        vocab_size = vocab_size,
        "MLX subprocess bridge backend created"
    );

    Ok(Box::new(bridge))
}

/// Stub for MLX bridge when feature is not enabled
#[cfg(not(feature = "mlx-bridge"))]
fn create_mlx_bridge_backend(
    _model_path: &Path,
    _manifest_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    Err(AosError::Config(
        "MLX Bridge backend requires 'mlx-bridge' feature to be enabled. \
         Build with: cargo build --features mlx-bridge"
            .to_string(),
    ))
}

/// Internal helper to create Metal backend with optional manifest hash for cache key
#[cfg(target_os = "macos")]
fn create_metal_backend(
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
    model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    use adapteros_lora_kernel_mtl::{GqaConfig, MetalKernels};
    let manifest_hash = manifest_hash.ok_or_else(|| {
        AosError::Config(
            "Manifest hash is required for Metal backend identity; pass manifest hash to backend factory"
                .to_string(),
        )
    })?;
    info!(
        model_path = %model_path.display(),
        has_manifest_hash = true,
        has_model_weights_hash = model_weights_hash.is_some(),
        "Creating Metal kernel backend"
    );

    // Load model configuration from config.json if available (with validation)
    let model_config = load_and_validate_model_config(model_path)?;
    if let Some(ref cfg) = model_config {
        info!(
            architecture = %cfg.architecture,
            hidden_size = cfg.hidden_size,
            num_attention_heads = cfg.num_attention_heads,
            num_kv_heads = cfg.num_key_value_heads,
            rope_theta = cfg.rope_theta,
            "Loaded model configuration from config.json"
        );
    }

    // Create cache key - prefer manifest hash when available for canonical identity
    let cache_key = ModelKey::new(
        BackendType::Metal,
        *manifest_hash,
        build_model_cache_identity(BackendType::Metal, model_path),
    );
    let model_bytes_arc = get_model_cache()?
        .get_or_load(&cache_key, || {
            // Load and verify atomically to eliminate TOCTOU gap
            let (bytes, computed_hash) =
                load_model_bytes_atomic_verified(model_path, model_weights_hash)?;
            let memory_bytes = bytes.len() as u64;
            info!(
                model_size_mb = memory_bytes / BYTES_PER_MB,
                computed_hash = %computed_hash.to_hex(),
                verified = model_weights_hash.is_some(),
                "Loaded model weights for Metal backend"
            );
            Ok((ModelHandle::Metal(Arc::new(bytes)), memory_bytes))
        })?
        .as_metal_bytes()?;

    // Create Metal backend
    let mut kernels = MetalKernels::new()?;

    // Set GQA config from model config if available
    if let Some(cfg) = model_config {
        let gqa_config = GqaConfig::from_params(
            cfg.num_attention_heads,
            cfg.num_key_value_heads,
            cfg.hidden_size,
            cfg.rope_theta,
        );
        kernels.set_gqa_config(gqa_config);
    }

    // Initialize with model weights (immutable after load). In debug and when
    // explicitly requested, re-hash after load to ensure the kernel never
    // mutates the shared base buffer (Arc-backed, unified memory).
    let plan_bytes: &[u8] = model_bytes_arc.as_slice();
    // Invariant: base model bytes must remain immutable after load. When we have a
    // manifest hash (deterministic path) or explicit verification is requested,
    // re-hash before/after load to catch any accidental mutation in the kernel.
    let verify_immutable =
        cfg!(debug_assertions) || std::env::var("AOS_VERIFY_MODEL_BYTES").is_ok();

    if verify_immutable {
        let before = B3Hash::hash(plan_bytes);
        kernels.load(plan_bytes)?;
        let after = B3Hash::hash(plan_bytes);
        if before != after {
            return Err(AosError::Internal(
                "Metal backend mutated base model bytes during load".to_string(),
            ));
        }
        debug_assert_eq!(
            before, after,
            "Metal backend must leave base bytes untouched"
        );
    } else {
        kernels.load(plan_bytes)?;
    }
    info!("Metal kernel backend initialized successfully");

    Ok(Box::new(kernels))
}

#[cfg(not(target_os = "macos"))]
fn create_metal_backend(
    _model_path: &Path,
    _manifest_hash: Option<&B3Hash>,
    _model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    Err(AosError::Config("Metal backend requires macOS".to_string()))
}

/// Internal helper to create CoreML backend with optional hash verification
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn create_coreml_backend(
    model_path: &Path,
    _manifest_hash: Option<&B3Hash>,
    model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    use adapteros_lora_kernel_coreml::{init_coreml, CoreMLBackend, CoreMLModelParams};

    // Initialize CoreML runtime
    init_coreml()?;

    let settings = resolve_coreml_backend_settings();

    // Load model configuration from config.json if available (with validation)
    let model_config = load_and_validate_model_config(model_path)?;
    if let Some(ref cfg) = model_config {
        info!(
            architecture = %cfg.architecture,
            hidden_size = cfg.hidden_size,
            num_attention_heads = cfg.num_attention_heads,
            num_kv_heads = cfg.num_key_value_heads,
            rope_theta = cfg.rope_theta,
            "Loaded model configuration for CoreML backend"
        );
    }

    // Verify model integrity before CoreML loads it
    if let Some(expected_hash) = model_weights_hash {
        let actual_hash = compute_model_directory_hash(model_path)?;
        if actual_hash != *expected_hash {
            return Err(AosError::CacheCorruption {
                path: model_path.display().to_string(),
                expected: expected_hash.to_hex(),
                actual: actual_hash.to_hex(),
            });
        }
        info!(
            model_path = %model_path.display(),
            verified_hash = %actual_hash.to_hex(),
            "CoreML model integrity verified"
        );
    }

    info!(
        backend = "coreml",
        model_path = %model_path.display(),
        compute_preference = %settings.preference,
        compute_units = ?settings.compute_units,
        production_mode = settings.production_mode,
        gpu_available = settings.gpu_available,
        ane_available = settings.ane_available,
        gpu_used = settings.gpu_used,
        ane_used = settings.ane_used,
        has_model_weights_hash = model_weights_hash.is_some(),
        "Creating CoreML kernel backend"
    );
    let mut backend = CoreMLBackend::new(settings.compute_units, settings.production_mode)?;

    // Set model parameters from config.json if available
    if let Some(cfg) = model_config {
        backend.set_model_params(CoreMLModelParams::new(
            cfg.hidden_size,
            cfg.num_attention_heads,
            cfg.num_key_value_heads,
            cfg.intermediate_size,
            cfg.rope_theta,
            cfg.max_seq_len,
        ));
    }

    // Note: MoE detection happens automatically in backend.load_model()
    // The backend will detect and log MoE architecture from config.json

    Ok(Box::new(backend))
}

#[cfg(all(target_os = "macos", not(feature = "coreml-backend")))]
fn create_coreml_backend(
    _model_path: &Path,
    _manifest_hash: Option<&B3Hash>,
    _model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    Err(AosError::Config(
        "CoreML backend requires 'coreml-backend' feature to be enabled. \
         Build with: cargo build --features coreml-backend"
            .to_string(),
    ))
}

#[cfg(not(target_os = "macos"))]
fn create_coreml_backend(
    _model_path: &Path,
    _manifest_hash: Option<&B3Hash>,
    _model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    Err(AosError::Config(
        "CoreML backend requires macOS".to_string(),
    ))
}

fn validate_mlx_model_dir(model_path: &Path) -> Result<PathBuf> {
    if !model_path.exists() {
        return Err(AosError::Config(format!(
            "MLX model path '{}' does not exist. Set AOS_MODEL_PATH to a directory containing MLX config.json and weights.",
            model_path.display()
        )));
    }

    if !model_path.is_dir() {
        return Err(AosError::Config(format!(
            "MLX model path '{}' is not a directory. Set AOS_MODEL_PATH to the MLX model directory.",
            model_path.display()
        )));
    }

    // SECURITY: Canonicalize path to resolve symlinks and validate location
    let canonical_path = model_path.canonicalize().map_err(|e| {
        AosError::Config(format!(
            "Failed to canonicalize model path '{}': {}",
            model_path.display(),
            e
        ))
    })?;

    // SECURITY: Reject /tmp paths for model storage (data persistence requirement)
    reject_tmp_persistent_path(&canonical_path, "model-path")?;

    let config_path = canonical_path.join("config.json");
    if !config_path.exists() {
        return Err(AosError::Config(format!(
            "config.json not found at '{}'. Set AOS_MODEL_PATH to a model directory containing config.json.",
            config_path.display()
        )));
    }

    Ok(canonical_path)
}

fn resolve_mlx_model_path_from_env() -> Result<PathBuf> {
    let model_path = model::get_model_path_with_fallback()?;
    validate_mlx_model_dir(&model_path)
}

/// Create a kernel backend based on the choice (backward-compatible)
///
/// This function maintains backward compatibility for code that doesn't need model paths.
/// - For Auto/Metal/CoreML: Works as before (no model path needed)
/// - For Mlx: Reads model path from `AOS_MODEL_PATH` env var, errors if not set
///
/// For new code, prefer using `create_backend_from_config` or `create_backend_with_model`.
pub fn create_backend(choice: BackendChoice) -> Result<KernelBox> {
    match choice {
        BackendChoice::Auto => {
            let capabilities = detect_capabilities();
            let selected = auto_select_backend(&capabilities)?;
            create_backend(selected)
        }
        BackendChoice::CPU => Err(AosError::Config(
            "CPU backend is not supported for inference kernels".to_string(),
        )),
        BackendChoice::Metal => {
            #[cfg(target_os = "macos")]
            {
                use adapteros_lora_kernel_mtl::MetalKernels;
                info!("Creating Metal kernel backend");
                Ok(Box::new(MetalKernels::new()?))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err(AosError::Config("Metal backend requires macOS".to_string()))
            }
        }
        BackendChoice::CoreML => {
            #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
            {
                use adapteros_lora_kernel_coreml::{init_coreml, CoreMLBackend};

                // Initialize CoreML runtime
                init_coreml()?;

                let settings = resolve_coreml_backend_settings();

                info!(
                    backend = "coreml",
                    compute_preference = %settings.preference,
                    compute_units = ?settings.compute_units,
                    production_mode = settings.production_mode,
                    gpu_available = settings.gpu_available,
                    ane_available = settings.ane_available,
                    gpu_used = settings.gpu_used,
                    ane_used = settings.ane_used,
                    "Creating CoreML kernel backend"
                );
                let backend = CoreMLBackend::new(settings.compute_units, settings.production_mode)?;
                Ok(Box::new(backend))
            }
            #[cfg(all(target_os = "macos", not(feature = "coreml-backend")))]
            {
                Err(AosError::Config(
                    "CoreML backend requires 'coreml-backend' feature to be enabled. \
                     Build with: cargo build --features coreml-backend"
                        .to_string(),
                ))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err(AosError::Config(
                    "CoreML backend requires macOS".to_string(),
                ))
            }
        }
        BackendChoice::Mlx => {
            // For backward compatibility, read model path from environment variable (validated)
            let model_path = resolve_mlx_model_path_from_env()?;

            #[cfg(feature = "multi-backend")]
            {
                use adapteros_lora_mlx_ffi::{
                    mlx_runtime_init, mlx_runtime_is_initialized, MLXFFIBackend, MLXFFIModel,
                };

                info!(model_path = %model_path.display(), "Creating MLX FFI kernel backend");

                // Ensure MLX runtime is initialized
                if !mlx_runtime_is_initialized() {
                    mlx_runtime_init().map_err(|e| {
                        AosError::Config(format!("Failed to initialize MLX runtime: {}", e))
                    })?;
                }

                // Load the model
                let model = MLXFFIModel::load(&model_path).map_err(|e| {
                    AosError::Config(format!(
                        "Failed to load MLX model from '{}': {}",
                        model_path.display(),
                        e
                    ))
                })?;

                let backend = MLXFFIBackend::new(model);
                Ok(Box::new(backend))
            }
            #[cfg(not(feature = "multi-backend"))]
            {
                let _ = model_path;
                Err(AosError::Config(
                    "MLX backend requires 'multi-backend' feature to be enabled. \
                     Build with: cargo build --features multi-backend"
                        .to_string(),
                ))
            }
        }
        BackendChoice::MlxBridge => {
            // MLX Bridge requires a model path from environment variable
            let model_path = resolve_mlx_model_path_from_env()?;

            #[cfg(feature = "mlx-bridge")]
            {
                create_mlx_bridge_backend(&model_path, None)
            }
            #[cfg(not(feature = "mlx-bridge"))]
            {
                let _ = model_path;
                Err(AosError::Config(
                    "MLX Bridge backend requires 'mlx-bridge' feature to be enabled. \
                     Build with: cargo build --features mlx-bridge"
                        .to_string(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("create temp dir")
    }

    #[test]
    fn backend_kind_identity_in_choice() {
        assert_eq!(
            BackendChoice::CoreML,
            adapteros_core::backend::BackendKind::CoreML
        );
        assert_eq!(
            BackendChoice::Mlx,
            adapteros_core::backend::BackendKind::Mlx
        );
    }

    #[test]
    fn auto_select_prefers_coreml_when_available() {
        let capabilities = BackendCapabilities {
            has_metal: true,
            metal_device_name: Some("Test Metal".to_string()),
            has_ane: true,
            has_coreml: true,
            has_mlx: true,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };

        let selected = auto_select_backend(&capabilities).expect("coreml should be selected");
        assert_eq!(selected, BackendChoice::CoreML);
    }

    #[test]
    fn coreml_request_falls_back_to_metal_when_unavailable() {
        let profile = ExecutionProfile {
            seed_mode: adapteros_core::SeedMode::BestEffort,
            backend_profile: BackendKind::CoreML,
        };
        let capabilities = BackendCapabilities {
            has_metal: true,
            metal_device_name: Some("Test Metal".to_string()),
            has_ane: false,
            has_coreml: false,
            has_mlx: false,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };
        let ctx = SelectionContext::new(profile, capabilities);

        let selection = select_backend_from_execution_profile(&ctx).expect("fallback works");
        assert_eq!(selection.selected, BackendChoice::Metal);
        assert!(selection.overridden);
        assert_eq!(
            selection.reason,
            Some("coreml_unavailable_fallback_metal"),
            "expected stable override reason when CoreML unavailable"
        );
    }

    #[test]
    fn coreml_request_honors_capability() {
        let profile = ExecutionProfile {
            seed_mode: adapteros_core::SeedMode::BestEffort,
            backend_profile: BackendKind::CoreML,
        };
        let capabilities = BackendCapabilities {
            has_metal: true,
            metal_device_name: Some("Test Metal".to_string()),
            has_ane: true,
            has_coreml: true,
            has_mlx: false,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };
        let ctx = SelectionContext::new(profile, capabilities);

        let selection = select_backend_from_execution_profile(&ctx).expect("coreml allowed");
        assert_eq!(selection.selected, BackendChoice::CoreML);
        assert!(!selection.overridden);
    }

    #[test]
    fn coreml_request_without_ane_falls_back() {
        let profile = ExecutionProfile {
            seed_mode: adapteros_core::SeedMode::BestEffort,
            backend_profile: BackendKind::CoreML,
        };
        let capabilities = BackendCapabilities {
            has_metal: true,
            metal_device_name: Some("Test Metal".to_string()),
            has_ane: false,
            has_coreml: true,
            has_mlx: false,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };
        let ctx = SelectionContext::new(profile, capabilities);

        let selection = select_backend_from_execution_profile(&ctx).expect("fallback allowed");
        assert_eq!(selection.selected, BackendChoice::Metal);
        assert!(selection.overridden);
        assert_eq!(selection.reason, Some("coreml_unavailable_fallback_metal"));
    }

    #[test]
    fn selection_context_deterministic_matrix() {
        let base_profile = ExecutionProfile {
            seed_mode: adapteros_core::SeedMode::BestEffort,
            backend_profile: BackendKind::Auto,
        };
        let full_caps = BackendCapabilities {
            has_metal: true,
            metal_device_name: Some("Test Metal".to_string()),
            has_ane: true,
            has_coreml: true,
            has_mlx: true,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };
        let ctx = SelectionContext::new(base_profile.clone(), full_caps);
        let selection = select_backend_from_execution_profile(&ctx).expect("auto resolves");
        assert_eq!(selection.selected, BackendChoice::CoreML);

        let no_coreml_caps = BackendCapabilities {
            has_metal: true,
            metal_device_name: Some("Test Metal".to_string()),
            has_ane: false,
            has_coreml: false,
            has_mlx: true,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };
        let ctx = SelectionContext::new(base_profile.clone(), no_coreml_caps);
        let selection =
            select_backend_from_execution_profile(&ctx).expect("fallback when coreml/ane missing");
        if cfg!(feature = "multi-backend") {
            assert_eq!(selection.selected, BackendChoice::Mlx);
        } else {
            assert_eq!(selection.selected, BackendChoice::Metal);
        }

        let metal_only_caps = BackendCapabilities {
            has_metal: true,
            metal_device_name: Some("Test Metal".to_string()),
            has_ane: false,
            has_coreml: false,
            has_mlx: false,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };
        let ctx = SelectionContext::new(base_profile, metal_only_caps);
        let selection = select_backend_from_execution_profile(&ctx).expect("fallback to metal");
        assert_eq!(selection.selected, BackendChoice::Metal);
    }

    #[cfg(not(feature = "multi-backend"))]
    #[test]
    fn mlx_selection_rejected_without_feature() {
        let profile = ExecutionProfile {
            seed_mode: adapteros_core::SeedMode::BestEffort,
            backend_profile: BackendKind::Mlx,
        };
        let capabilities = BackendCapabilities {
            has_metal: true,
            metal_device_name: Some("Test Metal".to_string()),
            has_ane: false,
            has_coreml: false,
            has_mlx: true,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };
        let ctx = SelectionContext::new(profile, capabilities);

        let err = select_backend_from_execution_profile(&ctx).unwrap_err();
        assert!(
            err.to_string().contains("multi-backend"),
            "expected feature gate error"
        );
    }

    #[cfg(feature = "multi-backend")]
    #[test]
    fn mlx_selection_requires_capability() {
        let profile = ExecutionProfile {
            seed_mode: adapteros_core::SeedMode::BestEffort,
            backend_profile: BackendKind::Mlx,
        };
        let capabilities = BackendCapabilities {
            has_metal: true,
            metal_device_name: Some("Test Metal".to_string()),
            has_ane: false,
            has_coreml: false,
            has_mlx: false,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };
        let ctx = SelectionContext::new(profile, capabilities);

        let err = select_backend_from_execution_profile(&ctx).unwrap_err();
        assert!(
            err.to_string()
                .contains("Requested MLX backend is not available"),
            "should reject when MLX capability missing"
        );
    }

    #[test]
    fn cpu_backend_is_rejected_for_inference() {
        let err = create_backend(BackendChoice::CPU)
            .err()
            .expect("CPU backend should be rejected");
        assert!(err
            .to_string()
            .contains("CPU backend is not supported for inference kernels"));
    }

    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    #[test]
    fn coreml_preference_maps_to_compute_units() {
        use adapteros_lora_kernel_coreml::ComputeUnits;

        assert_eq!(
            coreml_units_from_preference(CoreMLComputePreference::CpuOnly),
            ComputeUnits::CpuOnly
        );
        assert_eq!(
            coreml_units_from_preference(CoreMLComputePreference::CpuAndGpu),
            ComputeUnits::CpuAndGpu
        );
        assert_eq!(
            coreml_units_from_preference(CoreMLComputePreference::CpuAndNe),
            ComputeUnits::CpuAndNeuralEngine
        );
        assert_eq!(
            coreml_units_from_preference(CoreMLComputePreference::All),
            ComputeUnits::All
        );
    }

    #[cfg(not(feature = "coreml-backend"))]
    #[test]
    fn coreml_backend_is_rejected_when_feature_disabled() {
        let err = create_backend_with_model(BackendChoice::CoreML, Path::new("var/model-cache"))
            .err()
            .expect("CoreML backend should be gated behind feature flag");

        if cfg!(target_os = "macos") {
            assert!(
                err.to_string().contains("coreml-backend"),
                "expected feature gate error, got {}",
                err
            );
        } else {
            assert!(
                err.to_string().contains("requires macOS"),
                "expected platform error, got {}",
                err
            );
        }
    }

    #[test]
    fn test_compute_model_directory_hash_single_file() {
        let dir = new_test_tempdir();
        let model_file = dir.path().join("model.safetensors");
        std::fs::write(&model_file, b"test model content").unwrap();

        let hash = compute_model_directory_hash(dir.path()).unwrap();
        let expected = B3Hash::hash(b"test model content");
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_verify_model_integrity_mismatch() {
        let dir = new_test_tempdir();
        std::fs::write(dir.path().join("model.safetensors"), b"actual content").unwrap();

        let wrong_hash = B3Hash::hash(b"different content");
        let result = verify_model_integrity(dir.path(), Some(&wrong_hash), "Test");

        assert!(matches!(result, Err(AosError::CacheCorruption { .. })));
    }

    #[test]
    fn test_verify_model_integrity_success() {
        let dir = new_test_tempdir();
        let content = b"test model content for verification";
        std::fs::write(dir.path().join("model.safetensors"), content).unwrap();

        let correct_hash = B3Hash::hash(content);
        let result = verify_model_integrity(dir.path(), Some(&correct_hash), "Test");

        assert!(
            result.is_ok(),
            "Verification should succeed with matching hash"
        );
    }

    #[test]
    fn test_verify_model_integrity_skipped_without_hash() {
        let dir = new_test_tempdir();
        std::fs::write(dir.path().join("model.safetensors"), b"content").unwrap();

        // Should succeed (skip verification) when no hash provided
        let result = verify_model_integrity(dir.path(), None, "Test");
        assert!(
            result.is_ok(),
            "Should skip verification when no hash provided"
        );
    }

    #[test]
    fn test_compute_model_directory_hash_sharded() {
        let dir = new_test_tempdir();

        // Create sharded model files
        let shard1 = b"shard 1 content";
        let shard2 = b"shard 2 content";
        std::fs::write(dir.path().join("model-00001-of-00002.safetensors"), shard1).unwrap();
        std::fs::write(dir.path().join("model-00002-of-00002.safetensors"), shard2).unwrap();

        let hash = compute_model_directory_hash(dir.path()).unwrap();

        // Expected hash is BLAKE3 of shard1 || shard2 (sorted order)
        let mut hasher = blake3::Hasher::new();
        hasher.update(shard1);
        hasher.update(shard2);
        let expected = B3Hash::from_bytes(*hasher.finalize().as_bytes());

        assert_eq!(
            hash, expected,
            "Sharded hash should match concatenated shards"
        );
    }

    #[test]
    fn test_parse_safetensors_index() {
        let dir = new_test_tempdir();

        // Create a valid index file
        let index_content = r#"{
            "weight_map": {
                "layer.0.weight": "model-00001-of-00002.safetensors",
                "layer.1.weight": "model-00002-of-00002.safetensors",
                "layer.2.weight": "model-00001-of-00002.safetensors"
            }
        }"#;
        std::fs::write(
            dir.path().join("model.safetensors.index.json"),
            index_content,
        )
        .unwrap();

        let result = parse_safetensors_index(dir.path()).unwrap();
        assert!(result.is_some(), "Should parse valid index");

        let shards = result.unwrap();
        assert_eq!(shards.len(), 2, "Should deduplicate shard files");
        assert!(shards.contains(&"model-00001-of-00002.safetensors".to_string()));
        assert!(shards.contains(&"model-00002-of-00002.safetensors".to_string()));
    }

    #[test]
    fn test_parse_safetensors_index_missing() {
        let dir = new_test_tempdir();

        // No index file
        let result = parse_safetensors_index(dir.path()).unwrap();
        assert!(result.is_none(), "Should return None when no index exists");
    }

    #[test]
    fn test_validate_model_cache_budget_error_message() {
        // Temporarily unset the env var to test error message
        let original_env = std::env::var("AOS_MODEL_CACHE_MAX_MB").ok();
        std::env::remove_var("AOS_MODEL_CACHE_MAX_MB");

        // Force the cache to not be initialized by clearing it
        // (we can't actually clear the Lazy static, but we can test the error path)
        let result = validate_model_cache_budget();

        // Restore original env var if it existed
        if let Some(val) = original_env {
            std::env::set_var("AOS_MODEL_CACHE_MAX_MB", val);
        }

        // In CI/test environments, the cache may or may not be initialized
        // If it fails, verify the error message contains helpful information
        if let Err(e) = result {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("Model cache budget not configured")
                    || error_msg.contains("Configuration Status"),
                "Error message should contain configuration guidance: {}",
                error_msg
            );
            assert!(
                error_msg.contains("AOS_MODEL_CACHE_MAX_MB") || error_msg.contains("How to fix"),
                "Error message should mention AOS_MODEL_CACHE_MAX_MB: {}",
                error_msg
            );
            assert!(
                error_msg.contains("Recommended minimums") || error_msg.contains("7B models"),
                "Error message should include recommended minimums: {}",
                error_msg
            );
        }
        // If it succeeds, that's fine too - means cache was already initialized
    }
}

/// Create backend with automatic selection and model size consideration
pub fn create_backend_auto(model_size_bytes: Option<usize>) -> Result<KernelBox> {
    let capabilities = detect_capabilities();

    // Check if model fits in available GPU memory
    if let (Some(model_size), Some(gpu_mem)) = (model_size_bytes, capabilities.gpu_memory_bytes) {
        let required_headroom = (gpu_mem as f64 * 0.15) as u64; // 15% headroom policy
        if model_size as u64 > gpu_mem - required_headroom {
            warn!(
                model_size_mb = model_size / BYTES_PER_MB as usize,
                gpu_memory_mb = gpu_mem / BYTES_PER_MB,
                "Model may not fit in GPU memory with required headroom"
            );
        }
    }

    let choice = auto_select_backend(&capabilities)?;
    create_backend(choice)
}

/// Get a human-readable description of available backends
pub fn describe_available_backends() -> String {
    let caps = detect_capabilities();
    let mut desc = String::from("Available backends:\n");

    if caps.has_metal {
        desc.push_str(&format!(
            "  - Metal: {} ({}MB GPU memory)\n",
            caps.metal_device_name
                .as_deref()
                .unwrap_or("Unknown device"),
            caps.gpu_memory_bytes.unwrap_or(0) / BYTES_PER_MB
        ));
    }

    if caps.has_coreml {
        desc.push_str(&format!(
            "  - CoreML: Available (ANE {})\n",
            if caps.has_ane {
                "available"
            } else {
                "not available"
            }
        ));
    }

    if caps.has_mlx {
        desc.push_str("  - MLX: Available (experimental)\n");
    }

    if !caps.has_metal && !caps.has_coreml && !caps.has_mlx {
        desc.push_str("  No hardware-accelerated backends available\n");
    }

    desc
}

/// Backend capability detection and reporting
pub mod capabilities {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum BackendType {
        Metal,  // Real Metal backend
        CoreML, // Real CoreML backend
        #[serde(rename = "Mlx")]
        MLX, // Real MLX backend
        Cpu,    // Fallback CPU
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BackendCapability {
        pub backend_type: BackendType,
        pub name: String,
        pub available: bool,
        pub deterministic: bool,
        pub description: String,
        pub requirements: Vec<String>,
    }

    /// Get all backend capabilities with current availability
    pub fn get_available_backends() -> Vec<BackendCapability> {
        let caps = super::detect_capabilities();

        vec![
            BackendCapability {
                backend_type: BackendType::Metal,
                name: "Metal".to_string(),
                available: caps.has_metal,
                deterministic: true,
                description: format!(
                    "Metal GPU backend - {}",
                    caps.metal_device_name
                        .as_deref()
                        .unwrap_or("No device detected")
                ),
                requirements: vec!["macOS".to_string(), "Metal-capable GPU".to_string()],
            },
            BackendCapability {
                backend_type: BackendType::CoreML,
                name: "CoreML".to_string(),
                available: caps.has_coreml && caps.has_ane,
                deterministic: true, // Conditional on ANE
                description: format!(
                    "CoreML backend with Neural Engine - {}",
                    if caps.has_ane {
                        "ANE available"
                    } else {
                        "ANE not available"
                    }
                ),
                requirements: vec![
                    "macOS".to_string(),
                    "Apple Silicon".to_string(),
                    "coreml-backend feature".to_string(),
                ],
            },
            BackendCapability {
                backend_type: BackendType::MLX, // Uses MLX per naming contract (serde rename preserves wire format)
                name: "MLX".to_string(),
                available: caps.has_mlx,
                deterministic: false, // MLX execution order not guaranteed
                description: "MLX backend for research/prototyping".to_string(),
                requirements: vec![
                    "macOS".to_string(),
                    "Apple Silicon".to_string(),
                    "multi-backend feature".to_string(),
                ],
            },
        ]
    }

    /// Log backend status report using structured tracing
    pub fn log_backend_status() {
        use tracing::info;

        let backends = get_available_backends();
        let available_count = backends.iter().filter(|b| b.available).count();
        let total_count = backends.len();

        info!(
            available_count = available_count,
            total_count = total_count,
            "AdapterOS Backend Status Report"
        );

        for backend in backends {
            let status = if backend.available {
                "AVAILABLE"
            } else {
                "NOT AVAILABLE"
            };
            let determinism = if backend.deterministic {
                "deterministic"
            } else {
                "non-deterministic"
            };

            if backend.available {
                info!(
                    backend_name = %backend.name,
                    status = status,
                    determinism = determinism,
                    description = %backend.description,
                    "Backend available"
                );
            } else {
                info!(
                    backend_name = %backend.name,
                    status = status,
                    determinism = determinism,
                    description = %backend.description,
                    requirements = %backend.requirements.join(", "),
                    "Backend not available"
                );
            }
        }

        info!(
            docs_reference = "docs/ADR_MULTI_BACKEND_STRATEGY.md",
            "Backend status report complete"
        );
    }
}

/// Compute BLAKE3 hash of all model files in a directory
#[allow(dead_code)] // Used by verify_model_integrity for backwards compatibility
fn compute_model_directory_hash(model_path: &Path) -> Result<B3Hash> {
    // Check for CoreML mlpackage format first
    let mlpackage_weight_path = model_path.join("Data/com.apple.CoreML/weights/weight.bin");
    if mlpackage_weight_path.exists() {
        let bytes = std::fs::read(&mlpackage_weight_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read CoreML weight file '{}': {}",
                mlpackage_weight_path.display(),
                e
            ))
        })?;
        return Ok(B3Hash::hash(&bytes));
    }

    let single_model_path = model_path.join("model.safetensors");

    if single_model_path.exists() {
        let bytes = std::fs::read(&single_model_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read model file '{}': {}",
                single_model_path.display(),
                e
            ))
        })?;
        return Ok(B3Hash::hash(&bytes));
    }

    // Sharded model: collect all shards, sort, hash in order
    let mut shard_paths: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(model_path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with("model-") && file_name.ends_with(".safetensors") {
                shard_paths.push(entry.path());
            }
        }
    }

    if shard_paths.is_empty() {
        return Err(AosError::Config(format!(
            "No model files found in '{}'",
            model_path.display()
        )));
    }

    shard_paths.sort();
    let mut hasher = blake3::Hasher::new();
    for shard_path in &shard_paths {
        let bytes = std::fs::read(shard_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read shard '{}': {}",
                shard_path.display(),
                e
            ))
        })?;
        hasher.update(&bytes);
    }
    Ok(B3Hash::from_bytes(*hasher.finalize().as_bytes()))
}

/// Verify model bytes against expected manifest hash
///
/// # Deprecation Warning
///
/// This function has a TOCTOU (time-of-check-time-of-use) vulnerability because
/// it verifies the model hash but doesn't return the verified bytes. The model
/// could theoretically change between verification and loading.
///
/// **Prefer `load_model_bytes_atomic_verified()` instead**, which computes the hash
/// from the exact bytes returned, eliminating the TOCTOU gap.
///
/// This function remains for backwards compatibility with existing Metal/CoreML
/// backend code that loads models through platform-specific APIs.
#[allow(dead_code)] // Retained for Metal/CoreML backend compatibility
fn verify_model_integrity(
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
    backend_name: &str,
) -> Result<()> {
    if std::env::var("AOS_SKIP_MODEL_HASH_VERIFY").is_ok() {
        warn!(backend = %backend_name, "Model hash verification SKIPPED");
        return Ok(());
    }

    let expected = match manifest_hash {
        Some(h) => h,
        None => {
            warn!(backend = %backend_name, "No manifest_hash provided; skipping verification");
            return Ok(());
        }
    };

    let actual = compute_model_directory_hash(model_path)?;
    if actual != *expected {
        error!(backend = %backend_name, expected = %expected.to_hex(), actual = %actual.to_hex(),
               "MODEL INTEGRITY VERIFICATION FAILED");
        return Err(AosError::CacheCorruption {
            path: model_path.display().to_string(),
            expected: expected.to_hex(),
            actual: actual.to_hex(),
        });
    }

    info!(backend = %backend_name, hash = %actual.to_short_hex(), "Model integrity verified");
    Ok(())
}

/// Load model bytes from a model directory with integrity verification
///
/// Supports both single model files (model.safetensors) and sharded models.
/// For sharded models, loads and merges ALL shards into a single byte buffer.
///
/// # Loading Strategy (Priority Order)
///
/// 1. **Single file**: If `model.safetensors` exists, load it directly
/// 2. **Index-based**: If `model.safetensors.index.json` exists, parse it and load all shards
/// 3. **Pattern-based**: Detect shard pattern (model-XXXXX-of-YYYYY.safetensors) and load all
///
/// # Sharded Model Detection
///
/// Detects when sharded models are incomplete (missing shards) and returns an error
/// with details about which shards are missing. Warns when shards are loaded without
/// an index.json file (Priority 3 fallback).
///
/// # Hash Verification
///
/// Computes BLAKE3 hash of loaded bytes and logs for audit. For full verification
/// against expected `weights_hash` from the model registry, use
/// [`load_model_bytes_verified`] with the expected hash from the control plane.
fn load_model_bytes(model_path: &Path) -> Result<Vec<u8>> {
    load_model_bytes_verified(model_path, None)
}

/// Load model bytes with atomic integrity verification
///
/// This function combines loading and hash verification in a single operation,
/// eliminating TOCTOU (time-of-check-time-of-use) vulnerabilities.
///
/// # Returns
///
/// Returns a tuple of `(bytes, computed_hash)` where:
/// - `bytes`: The loaded model bytes
/// - `computed_hash`: BLAKE3 hash of the exact bytes returned
///
/// # Errors
///
/// Returns `AosError::CacheCorruption` if:
/// - The computed hash doesn't match the expected hash (when provided)
/// - This indicates potential corruption or tampering
///
/// # TOCTOU Safety
///
/// Unlike the separate `verify_model_integrity()` + `load_model_bytes()` pattern,
/// this function computes the hash from the EXACT bytes returned, making it
/// impossible for the model to change between verification and use.
///
/// # Example
///
/// ```no_run
/// # use adapteros_core::{B3Hash, Result};
/// # use std::path::Path;
/// # fn example(model_path: &Path, expected: &B3Hash) -> Result<()> {
/// let (bytes, hash) = load_model_bytes_atomic_verified(model_path, Some(expected))?;
/// // bytes are guaranteed to match the hash - no TOCTOU gap
/// # Ok(())
/// # }
/// ```
fn load_model_bytes_atomic_verified(
    model_path: &Path,
    expected_hash: Option<&B3Hash>,
) -> Result<(Vec<u8>, B3Hash)> {
    let bytes = load_model_bytes(model_path)?;
    let computed_hash = B3Hash::hash(&bytes);

    if let Some(expected) = expected_hash {
        if computed_hash != *expected {
            error!(
                model_path = %model_path.display(),
                computed = %computed_hash.to_hex(),
                expected = %expected.to_hex(),
                "MODEL INTEGRITY FAILURE: Hash mismatch"
            );
            return Err(AosError::CacheCorruption {
                path: model_path.display().to_string(),
                expected: expected.to_hex(),
                actual: computed_hash.to_hex(),
            });
        }
        info!(
            model_path = %model_path.display(),
            hash = %computed_hash.to_short_hex(),
            "Model integrity verified"
        );
    }
    Ok((bytes, computed_hash))
}

/// Load model bytes with optional hash verification against expected value
///
/// When `expected_hash` is `Some`, verifies loaded bytes match the expected hash
/// and returns an error if there's a mismatch (indicating corruption or tampering).
///
/// Implements a 3-priority loading strategy:
/// 1. Single model.safetensors (if exists)
/// 2. Sharded model via index.json (if exists) - loads ALL shards
/// 3. Sharded model via pattern detection - loads ALL shards with warning
///
/// # Arguments
///
/// * `model_path` - Path to model directory
/// * `expected_hash` - Optional expected BLAKE3 hash (e.g., from model registry `weights_hash`)
///
/// # Errors
///
/// Returns `AosError::Config` if:
/// - Model file/shards are missing
/// - Hash mismatch when `expected_hash` is provided
/// - Sharded model is incomplete (missing shards)
pub fn load_model_bytes_verified(
    model_path: &Path,
    expected_hash: Option<&B3Hash>,
) -> Result<Vec<u8>> {
    // Try single model file first
    let single_model_path = model_path.join("model.safetensors");
    if single_model_path.exists() {
        info!(path = %single_model_path.display(), "Loading single model file");
        let bytes = std::fs::read(&single_model_path).map_err(|e| {
            AosError::Config(format!(
                "Failed to read model file '{}': {}",
                single_model_path.display(),
                e
            ))
        })?;

        // Compute and log BLAKE3 hash for audit
        let computed_hash = B3Hash::hash(&bytes);
        info!(
            path = %single_model_path.display(),
            hash = %computed_hash,
            size_bytes = bytes.len(),
            "Model file loaded and hashed"
        );

        // Verify against expected hash if provided
        if let Some(expected) = expected_hash {
            if computed_hash != *expected {
                error!(
                    path = %single_model_path.display(),
                    computed = %computed_hash,
                    expected = %expected,
                    "MODEL INTEGRITY FAILURE: Hash mismatch detected!"
                );
                return Err(AosError::Config(format!(
                    "Model integrity verification failed for '{}': computed hash {} != expected {}. \
                     Model file may be corrupted or tampered with.",
                    single_model_path.display(),
                    computed_hash,
                    expected
                )));
            }
            info!(
                path = %single_model_path.display(),
                hash = %computed_hash,
                "Model integrity verified: hash matches expected value"
            );
        }

        return Ok(bytes);
    }

    // Priority 2: Try loading via index.json if present
    if let Some(shard_files) = parse_safetensors_index(model_path)? {
        info!(
            model_path = %model_path.display(),
            num_shards = shard_files.len(),
            "Loading sharded model via index.json"
        );

        let bytes = load_and_merge_shards(model_path, &shard_files)?;

        // Compute and log BLAKE3 hash for audit
        let computed_hash = B3Hash::hash(&bytes);
        info!(
            model_path = %model_path.display(),
            hash = %computed_hash,
            size_bytes = bytes.len(),
            num_shards = shard_files.len(),
            "Sharded model loaded and hashed via index.json"
        );

        // Verify against expected hash if provided
        if let Some(expected) = expected_hash {
            if computed_hash != *expected {
                error!(
                    model_path = %model_path.display(),
                    computed = %computed_hash,
                    expected = %expected,
                    "SHARDED MODEL INTEGRITY FAILURE: Hash mismatch detected!"
                );
                return Err(AosError::Config(format!(
                    "Model integrity verification failed for sharded model at '{}': computed hash {} != expected {}. \
                     Model may be corrupted or tampered with.",
                    model_path.display(),
                    computed_hash,
                    expected
                )));
            }
            info!(
                model_path = %model_path.display(),
                hash = %computed_hash,
                "Sharded model integrity verified: hash matches expected value"
            );
        }

        return Ok(bytes);
    }

    // Priority 3: Detect shard pattern and load all shards (warn about missing index)
    let sharded_model = detect_sharded_model(model_path)?;
    if let Some((_first_shard_path, total_shards, found_shards)) = sharded_model {
        warn!(
            model_path = %model_path.display(),
            total_shards = total_shards,
            "Sharded model detected but no index.json found - loading shards by pattern"
        );

        // Check for missing shards
        if found_shards.len() < total_shards {
            let missing: Vec<usize> = (1..=total_shards)
                .filter(|i| !found_shards.contains(i))
                .collect();
            warn!(
                model_path = %model_path.display(),
                total_shards = total_shards,
                found_shards = found_shards.len(),
                missing_shards = ?missing,
                "Sharded model is incomplete - some shards are missing"
            );
            return Err(AosError::Config(format!(
                "Sharded model at '{}' is incomplete: expected {} shards, found {}. Missing shards: {:?}",
                model_path.display(),
                total_shards,
                found_shards.len(),
                missing
            )));
        }

        // Build shard file list from pattern
        let shard_files: Vec<String> = (1..=total_shards)
            .map(|i| format!("model-{:05}-of-{:05}.safetensors", i, total_shards))
            .collect();

        info!(
            model_path = %model_path.display(),
            total_shards = total_shards,
            "Loading all shards by pattern"
        );

        let bytes = load_and_merge_shards(model_path, &shard_files)?;

        // Compute and log BLAKE3 hash for audit
        let computed_hash = B3Hash::hash(&bytes);
        info!(
            model_path = %model_path.display(),
            hash = %computed_hash,
            size_bytes = bytes.len(),
            num_shards = shard_files.len(),
            "All shards loaded and hashed (pattern-based)"
        );

        // Verify against expected hash if provided
        if let Some(expected) = expected_hash {
            if computed_hash != *expected {
                error!(
                    model_path = %model_path.display(),
                    computed = %computed_hash,
                    expected = %expected,
                    "SHARDED MODEL INTEGRITY FAILURE: Hash mismatch detected!"
                );
                return Err(AosError::Config(format!(
                    "Model integrity verification failed for sharded model at '{}': computed hash {} != expected {}. \
                     Model may be corrupted or tampered with.",
                    model_path.display(),
                    computed_hash,
                    expected
                )));
            }
            info!(
                model_path = %model_path.display(),
                hash = %computed_hash,
                "Sharded model integrity verified: hash matches expected value"
            );
        }

        return Ok(bytes);
    }

    Err(AosError::Config(format!(
        "No model file found in '{}'. Expected 'model.safetensors' or sharded model files (model-00001-of-NNNNN.safetensors).",
        model_path.display()
    )))
}

/// Parse the safetensors index file and extract unique shard filenames
fn parse_safetensors_index(model_path: &Path) -> Result<Option<Vec<String>>> {
    let index_path = model_path.join("model.safetensors.index.json");
    if !index_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&index_path).map_err(|e| {
        AosError::Config(format!(
            "Failed to read index file '{}': {}",
            index_path.display(),
            e
        ))
    })?;

    let index: SafeTensorsIndex = serde_json::from_str(&content).map_err(|e| {
        AosError::Config(format!(
            "Failed to parse index JSON '{}': {}",
            index_path.display(),
            e
        ))
    })?;

    let mut shards: Vec<String> = index.weight_map.values().cloned().collect();
    shards.sort();
    shards.dedup();

    if shards.is_empty() {
        return Err(AosError::Config(format!(
            "Index file '{}' contains no shard references",
            index_path.display()
        )));
    }

    info!(index_path = %index_path.display(), num_shards = shards.len(), "Parsed safetensors index");
    Ok(Some(shards))
}

/// Load all shards and merge into a single valid SafeTensors buffer
///
/// Each shard file is a complete SafeTensors file with its own header.
/// This function parses each shard, extracts all tensors, and re-serializes
/// them into a single unified SafeTensors buffer that can be deserialized.
fn load_and_merge_shards(model_path: &Path, shard_files: &[String]) -> Result<Vec<u8>> {
    // Collect all tensor data from all shards
    // We need to keep the raw bytes alive while we build TensorViews
    let mut shard_bytes: Vec<Vec<u8>> = Vec::with_capacity(shard_files.len());

    for (idx, shard_file) in shard_files.iter().enumerate() {
        let shard_path = model_path.join(shard_file);
        if !shard_path.exists() {
            return Err(AosError::Config(format!(
                "Shard file '{}' referenced in index but not found",
                shard_path.display()
            )));
        }

        info!(shard = idx + 1, total = shard_files.len(), path = %shard_path.display(), "Loading shard");

        let bytes = std::fs::read(&shard_path).map_err(|e| {
            AosError::Config(format!(
                "Failed to read shard '{}': {}",
                shard_path.display(),
                e
            ))
        })?;
        shard_bytes.push(bytes);
    }

    // Parse all shards and collect tensor views
    let mut all_tensors: Vec<(String, TensorView<'_>)> = Vec::new();
    let mut parsed_shards: Vec<SafeTensors<'_>> = Vec::with_capacity(shard_bytes.len());

    // First pass: parse all shards (we need to keep SafeTensors alive for borrowing)
    for (idx, bytes) in shard_bytes.iter().enumerate() {
        let tensors = SafeTensors::deserialize(bytes).map_err(|e| {
            AosError::Config(format!(
                "Failed to parse shard {} as SafeTensors: {}",
                shard_files[idx], e
            ))
        })?;
        parsed_shards.push(tensors);
    }

    // Second pass: collect all tensor names and views
    for (shard_idx, shard) in parsed_shards.iter().enumerate() {
        for (name, _) in shard.tensors() {
            // Get the tensor view from this shard
            let view = shard.tensor(&name).map_err(|e| {
                AosError::Config(format!(
                    "Failed to get tensor '{}' from shard {}: {}",
                    name, shard_files[shard_idx], e
                ))
            })?;

            // Create a proper TensorView for serialization
            let tensor_view = TensorView::new(view.dtype(), view.shape().to_vec(), view.data())
                .map_err(|e| {
                    AosError::Config(format!(
                        "Failed to create tensor view for '{}': {}",
                        name, e
                    ))
                })?;

            all_tensors.push((name, tensor_view));
        }
    }

    info!(
        total_shards = shard_files.len(),
        total_tensors = all_tensors.len(),
        "Collected tensors from all shards, serializing unified buffer"
    );

    // Serialize all tensors into a single SafeTensors buffer
    let merged_bytes = safetensors::serialize(all_tensors, &None)
        .map_err(|e| AosError::Config(format!("Failed to serialize merged tensors: {}", e)))?;

    info!(
        total_shards = shard_files.len(),
        total_bytes = merged_bytes.len(),
        "Merged all shards into unified SafeTensors buffer"
    );

    Ok(merged_bytes)
}

/// Detect sharded model pattern and return shard information
///
/// Returns `Some((first_shard_path, total_shards, found_shard_indices))` if sharded model found,
/// `None` if no sharded model pattern detected.
fn detect_sharded_model(model_path: &Path) -> Result<Option<(PathBuf, usize, Vec<usize>)>> {
    let entries = match std::fs::read_dir(model_path) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(
                path = %model_path.display(),
                error = %e,
                "Failed to read model directory"
            );
            return Ok(None);
        }
    };

    // Pattern: model-XXXXX-of-YYYYY.safetensors
    let shard_pattern = regex::Regex::new(r"^model-(\d+)-of-(\d+)\.safetensors$").map_err(|e| {
        error!(
            error = %e,
            pattern = r"^model-(\d+)-of-(\d+)\.safetensors$",
            path = %model_path.display(),
            "Failed to compile shard regex"
        );
        AosError::Internal("Failed to compile shard regex".to_string())
    })?;

    let mut first_shard_path: Option<PathBuf> = None;
    let mut total_shards: Option<usize> = None;
    let mut found_shards: Vec<usize> = Vec::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if let Some(caps) = shard_pattern.captures(&file_name) {
            let shard_num: usize = caps[1].parse().unwrap_or(0);
            let total: usize = caps[2].parse().unwrap_or(0);

            if total_shards.is_none() {
                total_shards = Some(total);
            } else if total_shards != Some(total) {
                // Inconsistent total shard count - this shouldn't happen in valid models
                warn!(
                    file = %file_name,
                    expected_total = ?total_shards,
                    found_total = total,
                    "Inconsistent shard total in filename"
                );
            }

            found_shards.push(shard_num);

            if shard_num == 1 {
                first_shard_path = Some(entry.path());
            }
        }
    }

    match (first_shard_path, total_shards) {
        (Some(path), Some(total)) => {
            found_shards.sort();
            Ok(Some((path, total, found_shards)))
        }
        (None, Some(total)) => {
            // Found shard metadata but no first shard
            Err(AosError::Config(format!(
                "Sharded model at '{}' is missing first shard (model-00001-of-{:05}.safetensors)",
                model_path.display(),
                total
            )))
        }
        _ => Ok(None),
    }
}
