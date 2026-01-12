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
#![allow(clippy::items_after_test_module)]

mod cache;
pub mod capabilities;
mod model_config;
mod model_io;

pub use cache::{
    configure_model_cache_pinning, configure_model_cache_telemetry, get_model_cache,
    validate_model_cache_budget, BaseModelPinConfig,
};
pub use capabilities::{
    auto_select_backend, describe_available_backends, detect_capabilities,
    select_backend_from_execution_profile, BackendCapabilities, BackendSelection, BackendStrategy,
    SelectionContext,
};
pub use model_config::{resolve_base_model_pin_budget_bytes, resolve_base_model_pin_enabled};
pub use model_io::load_model_bytes_verified;

use crate::model_handle_cache::{ModelHandle, ModelHandleCache};
use crate::model_key::ModelKey;
use adapteros_config::{
    model, reject_tmp_persistent_path, resolve_base_model_location, BackendPreference, ModelConfig,
};
use adapteros_core::{constants::BYTES_PER_MB, AosError, B3Hash, Result};
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_secure_fs::path_policy::canonicalize_strict_in_allowed_roots;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use model_io::estimate_coreml_model_size_bytes;
#[cfg(target_os = "macos")]
use model_io::estimate_model_size_bytes;
use model_io::load_model_bytes_atomic_verified;
#[cfg(feature = "multi-backend")]
use model_io::verify_model_integrity;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(feature = "multi-backend")]
use tracing::debug;
use tracing::{error, info, warn};

#[cfg(any(target_os = "macos", feature = "multi-backend"))]
use model_config::{build_model_cache_identity, load_and_validate_model_config};

#[cfg(test)]
use adapteros_core::{backend::BackendKind, ExecutionProfile};
#[cfg(any(test, all(target_os = "macos", feature = "coreml-backend")))]
use model_io::compute_model_directory_hash;
#[cfg(test)]
use model_io::parse_safetensors_index;
#[cfg(all(test, not(feature = "multi-backend")))]
use model_io::verify_model_integrity;

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_config::CoreMLComputePreference;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use std::str::FromStr;

/// Shared kernel object type with Send + Sync for use across async boundaries
pub type KernelBox = Box<dyn FusedKernels + Send + Sync>;

/// Canonical backend choice for kernel creation.
///
/// This is an alias of `BackendKind` to keep public signatures stable while
/// consolidating backend parsing and display logic in a single place.
pub type BackendChoice = adapteros_core::backend::BackendKind;

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
            let selected = auto_select_backend(&capabilities)?;
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
                if settings.production_mode && !settings.ane_used {
                    return Err(AosError::Config(
                        "CoreML production mode requires deterministic ANE-only compute units; ANE unavailable or not selected"
                            .to_string(),
                    ));
                }
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

                let plan_path = model_path.to_str().ok_or_else(|| {
                    AosError::Config(format!(
                        "CoreML model path is not valid UTF-8: {}",
                        model_path.display()
                    ))
                })?;
                backend.load(plan_path.as_bytes())?;

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

fn effective_pin_budget_bytes(cache: &ModelHandleCache) -> Option<u64> {
    let state = cache.base_model_pin_state();
    if !state.enabled {
        return None;
    }
    let configured = state
        .budget_bytes
        .unwrap_or_else(|| cache.max_memory_bytes());
    Some(configured.min(cache.max_memory_bytes()))
}

fn validate_base_model_pin_budget(
    cache: &ModelHandleCache,
    required_bytes: u64,
    backend: BackendType,
) -> Result<()> {
    let Some(budget_bytes) = effective_pin_budget_bytes(cache) else {
        return Ok(());
    };

    if required_bytes <= budget_bytes {
        return Ok(());
    }

    let state = cache.base_model_pin_state();
    if let Some(configured) = state.budget_bytes {
        if configured > cache.max_memory_bytes() {
            warn!(
                configured_bytes = configured,
                cache_max_bytes = cache.max_memory_bytes(),
                "Pin budget exceeds cache max; cache max still applies"
            );
        }
    }
    let model_id = state.model_id.unwrap_or_else(|| "unknown".to_string());
    let required_mb = required_bytes.div_ceil(BYTES_PER_MB);
    let budget_mb = budget_bytes.div_ceil(BYTES_PER_MB);

    error!(
        model_id = %model_id,
        backend = ?backend,
        required_bytes,
        budget_bytes,
        required_mb,
        budget_mb,
        "Base model pin budget exceeded"
    );

    Err(AosError::Config(format!(
        "Base model pin budget exceeded for {model_id} on {backend:?}: required {required_mb} MB ({required_bytes} bytes) > budget {budget_mb} MB ({budget_bytes} bytes)"
    )))
}

#[cfg(any(target_os = "macos", feature = "multi-backend"))]
fn ensure_base_model_pinned(
    cache: &ModelHandleCache,
    key: &ModelKey,
    backend: BackendType,
) -> Result<()> {
    if cache.base_model_pin_enabled() && !cache.is_pinned(key) {
        return Err(AosError::Config(format!(
            "Base model pinning enabled but {backend:?} base model was not pinned (pin limit reached?)"
        )));
    }
    Ok(())
}

/// Internal helper to create MLX backend with optional manifest hash
#[cfg(feature = "multi-backend")]
fn create_mlx_backend(
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
    model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    // FAIL-FAST: Ensure at least one MLX implementation is compiled in.
    #[cfg(all(not(feature = "mlx"), not(feature = "mlx-rs-backend")))]
    {
        let _ = (model_path, manifest_hash, model_weights_hash);
        Err(AosError::Config(
            "MLX backend selected but no MLX implementation is enabled. Rebuild with --features mlx or --features mlx-rs-backend"
                .to_string(),
        ))
    }

    #[cfg(any(feature = "mlx", feature = "mlx-rs-backend"))]
    {
        let selected_impl = adapteros_lora_mlx_ffi::select_mlx_implementation()
            .map_err(|e| AosError::Config(format!("MLX backend selection failed: {}", e)))?;

        info!(
            implementation = selected_impl.as_str(),
            "Selected MLX implementation"
        );

        match selected_impl {
            adapteros_lora_mlx_ffi::MlxImplementation::Ffi => {
                create_mlx_ffi_backend(model_path, manifest_hash, model_weights_hash)
            }
            adapteros_lora_mlx_ffi::MlxImplementation::Rs => {
                let allow_rs = std::env::var("AOS_ALLOW_MLX_RS")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);
                if !allow_rs {
                    return Err(AosError::Config(
                        "mlx-rs backend is experimental (no LoRA fusion/cache). Set AOS_ALLOW_MLX_RS=1 to opt in."
                            .to_string(),
                    ));
                }

                create_mlx_rs_backend(model_path, manifest_hash, model_weights_hash)
            }
        }
    }
}

#[cfg(feature = "multi-backend")]
fn create_mlx_ffi_backend(
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
    model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    #[cfg(not(feature = "mlx"))]
    {
        let _ = (model_path, manifest_hash, model_weights_hash);
        Err(AosError::Config(
            "MLX FFI backend selected but 'mlx' feature not enabled. Rebuild with --features mlx"
                .to_string(),
        ))
    }

    #[cfg(feature = "mlx")]
    {
        let model_path = validate_mlx_model_dir(model_path)?;
        let model_path_str = model_path.to_string_lossy();

        use adapteros_lora_mlx_ffi::{
            mlx_get_backend_capabilities, mlx_runtime_init_with_device, mlx_runtime_is_initialized,
            mlx_runtime_shutdown, MLXFFIBackend, MLXFFIModel, MlxDeviceType,
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

        let mut wants_cpu = match mlx_get_backend_capabilities() {
            Ok(caps) => !caps.gpu_available,
            Err(_) => false,
        };
        #[cfg(target_os = "macos")]
        {
            if metal::Device::system_default().is_none() {
                wants_cpu = true;
                warn!("Metal device unavailable; forcing MLX CPU runtime");
            }
        }
        if wants_cpu {
            info!("MLX GPU unavailable; initializing MLX runtime on CPU");
        }

        // Ensure MLX runtime is initialized
        if !mlx_runtime_is_initialized() {
            if wants_cpu {
                mlx_runtime_init_with_device(MlxDeviceType::Cpu).map_err(|e| {
                    AosError::Config(format!("Failed to initialize MLX CPU runtime: {}", e))
                })?;
            } else {
                mlx_runtime_init_with_device(MlxDeviceType::Auto).map_err(|e| {
                    AosError::Config(format!("Failed to initialize MLX runtime: {}", e))
                })?;
            }
        } else if wants_cpu {
            mlx_runtime_shutdown();
            mlx_runtime_init_with_device(MlxDeviceType::Cpu).map_err(|e| {
                AosError::Config(format!("Failed to reinitialize MLX CPU runtime: {}", e))
            })?;
        }

        // Create cache key - prefer manifest hash when available for canonical identity
        let cache_key = ModelKey::new(
            BackendType::MLX,
            *manifest_hash,
            build_model_cache_identity(BackendType::MLX, &model_path),
        );
        let cache = get_model_cache()?;
        cache.set_base_model_key(&cache_key);

        let pin_enabled = cache.base_model_pin_enabled();
        let memory_estimate = estimate_mlx_model_memory(&model_path)?;
        if pin_enabled {
            validate_base_model_pin_budget(cache, memory_estimate, BackendType::MLX)?;
        }

        let model_handle = if pin_enabled {
            cache.get_or_load_base_model(&cache_key, || {
                let model = MLXFFIModel::load(&model_path).map_err(|e| {
                    AosError::Config(format!(
                        "Failed to load MLX model from '{}': {}",
                        model_path_str, e
                    ))
                })?;
                Ok((ModelHandle::Mlx(Arc::new(model)), memory_estimate))
            })?
        } else {
            cache.get_or_load(&cache_key, || {
                let model = MLXFFIModel::load(&model_path).map_err(|e| {
                    AosError::Config(format!(
                        "Failed to load MLX model from '{}': {}",
                        model_path_str, e
                    ))
                })?;
                Ok((ModelHandle::Mlx(Arc::new(model)), memory_estimate))
            })?
        };
        if pin_enabled {
            ensure_base_model_pinned(cache, &cache_key, BackendType::MLX)?;
        }
        let model_arc = model_handle.as_mlx_model()?;

        // Create backend with or without manifest hash for deterministic seeding
        info!("Creating MLX backend with HKDF-seeded determinism from manifest hash");
        let backend: KernelBox = Box::new(
            MLXFFIBackend::with_manifest_hash_arc(model_arc, manifest_hash.clone()).map_err(
                |e| {
                    AosError::Config(format!(
                        "Failed to create MLX backend with manifest hash: {}",
                        e
                    ))
                },
            )?,
        );

        Ok(backend)
    }
}

#[cfg(feature = "multi-backend")]
fn create_mlx_rs_backend(
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
    model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    #[cfg(not(feature = "mlx-rs-backend"))]
    {
        let _ = (model_path, manifest_hash, model_weights_hash);
        Err(AosError::Config(
            "mlx-rs backend selected but not compiled. Rebuild with --features mlx-rs-backend"
                .to_string(),
        ))
    }

    #[cfg(feature = "mlx-rs-backend")]
    {
        use adapteros_lora_mlx_ffi::{MlxRsBackend, MlxRsModel};

        let model_path = validate_mlx_model_dir(model_path)?;
        let model_path_str = model_path.to_string_lossy();

        info!(
            model_path = %model_path_str,
            has_manifest_hash = manifest_hash.is_some(),
            has_model_weights_hash = model_weights_hash.is_some(),
            "Creating MLX (mlx-rs) kernel backend (experimental)"
        );

        let manifest_hash = manifest_hash.ok_or_else(|| {
            AosError::Config(
                "Manifest hash is required for MLX backend identity; pass manifest hash to backend factory"
                    .to_string(),
            )
        })?;

        verify_model_integrity(&model_path, model_weights_hash, "MLX (mlx-rs)")?;

        tracing::warn!(
            "mlx-rs backend is experimental: LoRA adapters are not applied and model cache is bypassed"
        );

        let model = MlxRsModel::load(&model_path).map_err(|e| {
            AosError::Config(format!(
                "Failed to load mlx-rs model from '{}': {}",
                model_path_str, e
            ))
        })?;

        let backend =
            MlxRsBackend::with_manifest_hash(model, manifest_hash.clone()).map_err(|e| {
                AosError::Config(format!(
                    "Failed to create mlx-rs backend with manifest hash: {}",
                    e
                ))
            })?;

        Ok(Box::new(backend))
    }
}

/// Create MLX subprocess bridge backend for MoE models
#[cfg(feature = "mlx-bridge")]
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

    let model_path = validate_mlx_model_dir(model_path)?;

    // Load config to get vocab_size
    let config = load_and_validate_model_config(&model_path)?;
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
    let model_path = canonicalize_model_path(model_path)?;
    info!(
        model_path = %model_path.display(),
        has_manifest_hash = true,
        has_model_weights_hash = model_weights_hash.is_some(),
        "Creating Metal kernel backend"
    );

    // Load model configuration from config.json if available (with validation)
    let model_config = load_and_validate_model_config(&model_path)?;
    let model_config = model_config.ok_or_else(|| {
        AosError::Config(format!(
            "Metal backend requires config.json with model dimensions for deterministic GQA setup (path: {})",
            model_path.display()
        ))
    })?;
    info!(
        architecture = %model_config.architecture,
        hidden_size = model_config.hidden_size,
        num_attention_heads = model_config.num_attention_heads,
        num_kv_heads = model_config.num_key_value_heads,
        rope_theta = model_config.rope_theta,
        "Loaded model configuration from config.json"
    );

    // Create cache key - prefer manifest hash when available for canonical identity
    let cache_key = ModelKey::new(
        BackendType::Metal,
        *manifest_hash,
        build_model_cache_identity(BackendType::Metal, &model_path),
    );
    let cache = get_model_cache()?;
    cache.set_base_model_key(&cache_key);

    let pin_enabled = cache.base_model_pin_enabled();
    if pin_enabled {
        let estimated_bytes = estimate_model_size_bytes(&model_path)?;
        validate_base_model_pin_budget(cache, estimated_bytes, BackendType::Metal)?;
    }

    let model_handle = if pin_enabled {
        cache.get_or_load_base_model(&cache_key, || {
            // Load and verify atomically to eliminate TOCTOU gap
            let (bytes, computed_hash) =
                load_model_bytes_atomic_verified(&model_path, model_weights_hash)?;
            let memory_bytes = bytes.len() as u64;
            info!(
                model_size_mb = memory_bytes / BYTES_PER_MB,
                computed_hash = %computed_hash.to_hex(),
                verified = model_weights_hash.is_some(),
                "Loaded model weights for Metal backend"
            );
            Ok((ModelHandle::Metal(Arc::new(bytes)), memory_bytes))
        })?
    } else {
        cache.get_or_load(&cache_key, || {
            // Load and verify atomically to eliminate TOCTOU gap
            let (bytes, computed_hash) =
                load_model_bytes_atomic_verified(&model_path, model_weights_hash)?;
            let memory_bytes = bytes.len() as u64;
            info!(
                model_size_mb = memory_bytes / BYTES_PER_MB,
                computed_hash = %computed_hash.to_hex(),
                verified = model_weights_hash.is_some(),
                "Loaded model weights for Metal backend"
            );
            Ok((ModelHandle::Metal(Arc::new(bytes)), memory_bytes))
        })?
    };
    if pin_enabled {
        ensure_base_model_pinned(cache, &cache_key, BackendType::Metal)?;
    }
    let model_bytes_arc = model_handle.as_metal_bytes()?;

    // Create Metal backend
    let mut kernels = MetalKernels::new()?;

    // Set GQA config from model config (required for deterministic setup)
    let gqa_config = GqaConfig::from_params(
        model_config.num_attention_heads,
        model_config.num_key_value_heads,
        model_config.hidden_size,
        model_config.rope_theta,
    );
    kernels.set_gqa_config(gqa_config);

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
    manifest_hash: Option<&B3Hash>,
    model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox> {
    use adapteros_lora_kernel_coreml::{init_coreml, CoreMLBackend, CoreMLModelParams};

    let manifest_hash = manifest_hash.ok_or_else(|| {
        AosError::Config(
            "Manifest hash is required for CoreML backend identity; pass manifest hash to backend factory"
                .to_string(),
        )
    })?;
    let model_path = canonicalize_model_path(model_path)?;

    // Initialize CoreML runtime
    init_coreml()?;

    let settings = resolve_coreml_backend_settings();

    // Load model configuration from config.json if available (with validation)
    let model_config = load_and_validate_model_config(&model_path)?;
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

    let cache_key = ModelKey::new(
        BackendType::CoreML,
        *manifest_hash,
        build_model_cache_identity(BackendType::CoreML, &model_path),
    );
    let cache = get_model_cache()?;
    cache.set_base_model_key(&cache_key);

    let pin_enabled = cache.base_model_pin_enabled();
    let estimated_bytes = estimate_coreml_model_size_bytes(&model_path)?;
    if pin_enabled {
        validate_base_model_pin_budget(cache, estimated_bytes, BackendType::CoreML)?;
    }

    // Verify model integrity before CoreML loads it
    if let Some(expected_hash) = model_weights_hash {
        let actual_hash = compute_model_directory_hash(&model_path)?;
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
    if settings.production_mode && !settings.ane_used {
        return Err(AosError::Config(
            "CoreML production mode requires deterministic ANE-only compute units; ANE unavailable or not selected"
                .to_string(),
        ));
    }
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

    let plan_path = model_path.to_str().ok_or_else(|| {
        AosError::Config(format!(
            "CoreML model path is not valid UTF-8: {}",
            model_path.display()
        ))
    })?;
    backend.load(plan_path.as_bytes())?;

    if pin_enabled {
        cache.get_or_load_base_model(&cache_key, || Ok((ModelHandle::CoreML, estimated_bytes)))?;
        ensure_base_model_pinned(cache, &cache_key, BackendType::CoreML)?;
    } else {
        cache.get_or_load(&cache_key, || Ok((ModelHandle::CoreML, estimated_bytes)))?;
    }

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

pub(crate) fn model_allowed_roots() -> Result<Vec<PathBuf>> {
    let location = resolve_base_model_location(None, None, false)?;
    if !location.cache_root.exists() {
        std::fs::create_dir_all(&location.cache_root).map_err(|e| {
            AosError::Config(format!(
                "Failed to create model cache root {}: {}",
                location.cache_root.display(),
                e
            ))
        })?;
    }
    Ok(vec![location.cache_root])
}

pub(crate) fn canonicalize_model_path(model_path: &Path) -> Result<PathBuf> {
    let allowed_roots = model_allowed_roots()?;
    let canonical = canonicalize_strict_in_allowed_roots(model_path, &allowed_roots)
        .map_err(|e| AosError::Config(format!("Model path rejected: {}", e)))?;
    reject_tmp_persistent_path(&canonical, "model-path")?;
    Ok(canonical)
}

fn validate_mlx_model_dir(model_path: &Path) -> Result<PathBuf> {
    // SECURITY: Canonicalize path to resolve symlinks and validate location
    let canonical_path = canonicalize_model_path(model_path)?;

    if !canonical_path.is_dir() {
        return Err(AosError::Config(format!(
            "MLX model path '{}' is not a directory. Set AOS_MODEL_PATH to the MLX model directory.",
            canonical_path.display()
        )));
    }

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

fn derive_manifest_hash_from_config(model_path: &Path) -> Result<B3Hash> {
    let config_path = model_path.join("config.json");
    let bytes = std::fs::read(&config_path).map_err(|e| {
        AosError::Config(format!(
            "Unable to read config.json at '{}': {}",
            config_path.display(),
            e
        ))
    })?;
    Ok(B3Hash::hash(&bytes))
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
                if settings.production_mode && !settings.ane_used {
                    return Err(AosError::Config(
                        "CoreML production mode requires deterministic ANE-only compute units; ANE unavailable or not selected"
                            .to_string(),
                    ));
                }
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
                // Derive a deterministic manifest hash from config.json so the legacy
                // entry point still seeds MLX and uses the cache identity path.
                let manifest_hash = derive_manifest_hash_from_config(&model_path).map_err(|e| {
                    AosError::Config(format!(
                        "Failed to derive manifest hash for MLX backend: {}",
                        e
                    ))
                })?;

                create_backend_with_model_hashes(
                    BackendChoice::Mlx,
                    &model_path,
                    Some(&manifest_hash),
                    None,
                )
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
    fn auto_select_respects_priority_when_multiple_available() {
        let capabilities = BackendCapabilities {
            has_metal: true,
            metal_device_name: Some("Test Metal".to_string()),
            has_ane: true,
            has_coreml: true,
            has_mlx: true,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };

        let selected = auto_select_backend(&capabilities).expect("backend should be selected");
        if cfg!(feature = "multi-backend") {
            assert_eq!(selected, BackendChoice::Mlx);
        } else {
            assert_eq!(selected, BackendChoice::CoreML);
        }
    }

    #[test]
    fn coreml_request_falls_back_to_metal_when_unavailable() {
        let profile = ExecutionProfile {
            seed_mode: adapteros_core::SeedMode::BestEffort,
            backend_profile: BackendKind::CoreML,
            require_explicit_fallback_opt_out: false,
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
            require_explicit_fallback_opt_out: false,
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
            require_explicit_fallback_opt_out: false,
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
            require_explicit_fallback_opt_out: false,
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
        if cfg!(feature = "multi-backend") {
            assert_eq!(selection.selected, BackendChoice::Mlx);
        } else {
            assert_eq!(selection.selected, BackendChoice::CoreML);
        }

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
            require_explicit_fallback_opt_out: false,
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
            require_explicit_fallback_opt_out: false,
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
