//! Backend factory for creating kernel implementations
//!
//! This module provides factory functions for creating different kernel backends
//! (Metal, CoreML, MLX) and capability detection.
//!
//! ## Model Caching
//!
//! The factory uses a per-worker model cache to deduplicate loaded models.
//! Models are cached by `(backend_type, manifest_hash)` key, so:
//! - Different backends cache separately (Metal vs MLX)
//! - Different model versions cache separately (different config.json)
//! - Same model requested twice returns the cached version

use crate::model_handle_cache::{ModelHandle, ModelHandleCache};
use crate::model_key::ModelKey;
use adapteros_config::{model, BackendPreference, ConfigLoader, ModelConfig};
use adapteros_core::{
    backend::BackendKind, constants::BYTES_PER_MB, AosError, B3Hash, ExecutionProfile, Result,
};
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_lora_kernel_api::FusedKernels;
use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_config::CoreMLComputePreference;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use std::str::FromStr;

/// Shared kernel object type with Send + Sync for use across async boundaries
pub type KernelBox = Box<dyn FusedKernels + Send + Sync>;

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
    let explicit_toml_path = std::env::var("AOS_CONFIG_TOML").ok();
    let default_toml_path = {
        let default_path = Path::new("configs/cp.toml");
        if default_path.exists() {
            Some(default_path.to_string_lossy().to_string())
        } else {
            None
        }
    };
    let toml_path = explicit_toml_path.or(default_toml_path);

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
/// Budget must be explicitly provided (env or TOML); missing/zero budgets abort startup.
static MODEL_CACHE: Lazy<ModelHandleCache> = Lazy::new(|| {
    let max_bytes = match resolve_model_cache_budget_bytes() {
        Ok(bytes) => bytes,
        Err(err) => {
            error!(
                error = %err,
                "Model cache budget missing or invalid; worker cannot start"
            );
            panic!("Model cache budget error: {}", err);
        }
    };
    let max_mb = max_bytes / BYTES_PER_MB;
    info!(
        max_memory_mb = max_mb,
        "Initializing per-worker model cache with explicit budget"
    );
    ModelHandleCache::new(max_bytes)
});

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
        }
    }

    debug!(
        has_metal = caps.has_metal,
        metal_device = ?caps.metal_device_name,
        has_ane = caps.has_ane,
        has_coreml = caps.has_coreml,
        has_mlx = caps.has_mlx,
        gpu_memory_mb = caps.gpu_memory_bytes.map(|b| b / BYTES_PER_MB),
        "Backend capabilities detected"
    );

    caps
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
            let selected = auto_select_backend(&capabilities)?;
            create_backend_with_model(selected, model_path)
        }
        BackendChoice::CPU => Err(AosError::Config(
            "CPU backend is not supported for inference kernels".to_string(),
        )),
        BackendChoice::Metal => {
            // Delegate to helper (no manifest hash in this legacy path)
            create_metal_backend(model_path, None)
        }
        BackendChoice::CoreML => {
            #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
            {
                use adapteros_lora_kernel_coreml::{init_coreml, CoreMLBackend, CoreMLModelParams};

                // Initialize CoreML runtime
                init_coreml()?;

                let settings = resolve_coreml_backend_settings();

                // Load model configuration from config.json if available
                let model_config = ModelConfig::from_config_json(model_path).ok();
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
        BackendChoice::Mlx => create_mlx_backend(model_path, None),
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
pub fn create_backend_with_model_and_hash(
    choice: BackendChoice,
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    match choice {
        BackendChoice::Mlx => create_mlx_backend(model_path, manifest_hash),
        BackendChoice::Metal => create_metal_backend(model_path, manifest_hash),
        // CoreML doesn't cache model bytes (FFI manages internally)
        _ => create_backend_with_model(choice, model_path),
    }
}

/// Internal helper to create MLX backend with optional manifest hash
#[cfg(feature = "multi-backend")]
fn create_mlx_backend(model_path: &Path, manifest_hash: Option<&B3Hash>) -> Result<KernelBox> {
    use adapteros_lora_mlx_ffi::{
        mlx_runtime_init, mlx_runtime_is_initialized, MLXFFIBackend, MLXFFIModel,
    };

    let model_path = validate_mlx_model_dir(model_path)?;
    let model_path_str = model_path.to_string_lossy();
    info!(
        model_path = %model_path_str,
        has_manifest_hash = manifest_hash.is_some(),
        "Creating MLX FFI kernel backend"
    );

    let manifest_hash = manifest_hash.ok_or_else(|| {
        AosError::Config(
            "Manifest hash is required for MLX backend identity; pass manifest hash to backend factory"
                .to_string(),
        )
    })?;

    // Ensure MLX runtime is initialized
    if !mlx_runtime_is_initialized() {
        mlx_runtime_init()
            .map_err(|e| AosError::Config(format!("Failed to initialize MLX runtime: {}", e)))?;
    }

    // Create cache key - prefer manifest hash when available for canonical identity
    let cache_key = ModelKey::new(BackendType::Mlx, *manifest_hash);
    let model_arc = MODEL_CACHE
        .get_or_load(&cache_key, || {
            let model = MLXFFIModel::load(&model_path).map_err(|e| {
                AosError::Config(format!(
                    "Failed to load MLX model from '{}': {}",
                    model_path_str, e
                ))
            })?;
            // Estimate memory: use config if available, otherwise estimate from architecture
            let memory_bytes = estimate_mlx_model_memory(&model_path);
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

/// Estimate MLX model memory usage from config.json
#[cfg(feature = "multi-backend")]
fn estimate_mlx_model_memory(model_path: &Path) -> u64 {
    // Try to load config.json for accurate estimate
    if let Ok(config) = ModelConfig::from_config_json(model_path) {
        // Estimate: 4 bytes per param (f32), with typical model structure
        // hidden_size * num_layers * 12 (approx params per layer) * 4 bytes
        let params_estimate = config.hidden_size as u64
            * config.num_layers as u64
            * 12  // Approximate number of weight matrices per layer
            * config.hidden_size as u64
            / 1000; // Normalize
        let memory_estimate = params_estimate * 4; // 4 bytes per f32 param

        // Add 10% overhead
        return (memory_estimate as f64 * 1.1) as u64;
    }

    // Fallback: assume 7B model (~14GB for fp16, ~28GB for fp32)
    14 * 1024 * 1024 * 1024 // 14GB default estimate
}

#[cfg(not(feature = "multi-backend"))]
fn create_mlx_backend(_model_path: &Path, _manifest_hash: Option<&B3Hash>) -> Result<KernelBox> {
    Err(AosError::Config(
        "MLX backend requires 'multi-backend' feature to be enabled. \
         Build with: cargo build --features multi-backend"
            .to_string(),
    ))
}

/// Internal helper to create Metal backend with optional manifest hash for cache key
#[cfg(target_os = "macos")]
fn create_metal_backend(model_path: &Path, manifest_hash: Option<&B3Hash>) -> Result<KernelBox> {
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
        "Creating Metal kernel backend"
    );

    // Load model configuration from config.json if available
    let model_config = ModelConfig::from_config_json(model_path).ok();
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
    let cache_key = ModelKey::new(BackendType::Metal, *manifest_hash);
    let model_bytes_arc = MODEL_CACHE
        .get_or_load(&cache_key, || {
            let bytes = load_model_bytes(model_path)?;
            let memory_bytes = bytes.len() as u64;
            info!(
                model_size_mb = memory_bytes / BYTES_PER_MB,
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
fn create_metal_backend(_model_path: &Path, _manifest_hash: Option<&B3Hash>) -> Result<KernelBox> {
    Err(AosError::Config("Metal backend requires macOS".to_string()))
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

    let config_path = model_path.join("config.json");
    if !config_path.exists() {
        return Err(AosError::Config(format!(
            "config.json not found at '{}'. Set AOS_MODEL_PATH to a model directory containing config.json.",
            config_path.display()
        )));
    }

    Ok(model_path.to_path_buf())
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        Mlx,    // Real MLX backend
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
                backend_type: BackendType::Mlx,
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

/// Load model bytes from a model directory
///
/// Supports both single model files (model.safetensors) and sharded models
/// (model-00001-of-00003.safetensors, etc.). For sharded models, loads the
/// first shard which typically contains the embedding weights needed for
/// the Metal backend initialization.
fn load_model_bytes(model_path: &Path) -> Result<Vec<u8>> {
    // Try single model file first
    let single_model_path = model_path.join("model.safetensors");
    if single_model_path.exists() {
        info!(path = %single_model_path.display(), "Loading single model file");
        return std::fs::read(&single_model_path).map_err(|e| {
            AosError::Config(format!(
                "Failed to read model file '{}': {}",
                single_model_path.display(),
                e
            ))
        });
    }

    // Try sharded model (first shard contains embeddings)
    let first_shard_path = model_path.join("model-00001-of-00003.safetensors");
    if first_shard_path.exists() {
        info!(path = %first_shard_path.display(), "Loading first shard of sharded model");
        return std::fs::read(&first_shard_path).map_err(|e| {
            AosError::Config(format!(
                "Failed to read model shard '{}': {}",
                first_shard_path.display(),
                e
            ))
        });
    }

    // Try to find any sharded model pattern
    if let Ok(entries) = std::fs::read_dir(model_path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with("model-00001-of-") && file_name.ends_with(".safetensors") {
                info!(path = %entry.path().display(), "Loading first shard (auto-detected)");
                return std::fs::read(entry.path()).map_err(|e| {
                    AosError::Config(format!(
                        "Failed to read model shard '{}': {}",
                        entry.path().display(),
                        e
                    ))
                });
            }
        }
    }

    Err(AosError::Config(format!(
        "No model file found in '{}'. Expected 'model.safetensors' or sharded model files.",
        model_path.display()
    )))
}
