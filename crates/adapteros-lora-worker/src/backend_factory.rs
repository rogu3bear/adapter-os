//! Backend factory for creating kernel implementations
//!
//! This module provides factory functions for creating different kernel backends
//! (Metal, CoreML, MLX) and capability detection.

use adapteros_config::{BackendPreference, ModelConfig};
use adapteros_core::{constants::BYTES_PER_MB, AosError, B3Hash, Result};
use adapteros_lora_kernel_api::FusedKernels;
use std::path::Path;
use tracing::{debug, info, warn};

/// Backend choice for kernel creation
///
/// This enum mirrors `BackendPreference` from `adapteros-config` and provides
/// bidirectional conversion. New code should prefer using `BackendPreference`
/// directly when possible.
///
/// Use `create_backend_with_model` or `create_backend_from_config` when a model path is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendChoice {
    /// Metal GPU backend (production, deterministic)
    Metal,
    /// CoreML backend with ANE acceleration (production, deterministic)
    CoreML,
    /// MLX backend (research, training)
    Mlx,
    /// Automatic selection based on capabilities (CoreML -> Metal -> MLX fallback)
    Auto,
}

impl From<BackendPreference> for BackendChoice {
    fn from(pref: BackendPreference) -> Self {
        match pref {
            BackendPreference::Auto => BackendChoice::Auto,
            BackendPreference::CoreML => BackendChoice::CoreML,
            BackendPreference::Metal => BackendChoice::Metal,
            BackendPreference::Mlx => BackendChoice::Mlx,
        }
    }
}

impl From<BackendChoice> for BackendPreference {
    fn from(choice: BackendChoice) -> Self {
        match choice {
            BackendChoice::Auto => BackendPreference::Auto,
            BackendChoice::CoreML => BackendPreference::CoreML,
            BackendChoice::Metal => BackendPreference::Metal,
            BackendChoice::Mlx => BackendPreference::Mlx,
        }
    }
}

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

    // Detect MLX availability
    #[cfg(feature = "multi-backend")]
    {
        caps.has_mlx = true;
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
/// Selection order: CoreML (ANE) -> Metal -> MLX
/// This prioritizes power efficiency while maintaining determinism guarantees.
pub fn auto_select_backend(capabilities: &BackendCapabilities) -> Result<BackendChoice> {
    // Priority 1: CoreML with ANE (most power efficient)
    if capabilities.has_coreml && capabilities.has_ane {
        info!("Auto-selected CoreML backend with Neural Engine");
        return Ok(BackendChoice::CoreML);
    }

    // Priority 2: Metal (production, guaranteed determinism)
    if capabilities.has_metal {
        info!(
            device = ?capabilities.metal_device_name,
            "Auto-selected Metal backend"
        );
        return Ok(BackendChoice::Metal);
    }

    // Priority 3: MLX (experimental)
    if capabilities.has_mlx {
        info!("Auto-selected MLX backend (experimental)");
        return Ok(BackendChoice::Mlx);
    }

    Err(AosError::Config(
        "No suitable backend available. Ensure Metal GPU or CoreML with ANE is present."
            .to_string(),
    ))
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
/// let mut config = ModelConfig::new(PathBuf::from("./models/qwen2.5-7b"));
/// config.backend = BackendPreference::CoreML;
/// let backend = create_backend_from_config(&config)?;
/// # Ok(())
/// # }
/// ```
pub fn create_backend_from_config(config: &ModelConfig) -> Result<Box<dyn FusedKernels>> {
    let choice = match config.backend {
        BackendPreference::Auto => BackendChoice::Auto,
        BackendPreference::CoreML => BackendChoice::CoreML,
        BackendPreference::Metal => BackendChoice::Metal,
        BackendPreference::Mlx => BackendChoice::Mlx,
    };
    create_backend_with_model(choice, &config.path)
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
/// let backend = create_backend_with_model(BackendChoice::Mlx, Path::new("./models/qwen2.5-7b"))?;
/// # Ok::<(), adapteros_core::AosError>(())
/// ```
pub fn create_backend_with_model(
    choice: BackendChoice,
    model_path: &Path,
) -> Result<Box<dyn FusedKernels>> {
    match choice {
        BackendChoice::Auto => {
            let capabilities = detect_capabilities();
            let selected = auto_select_backend(&capabilities)?;
            create_backend_with_model(selected, model_path)
        }
        BackendChoice::Metal => {
            #[cfg(target_os = "macos")]
            {
                use adapteros_lora_kernel_mtl::MetalKernels;
                info!(model_path = %model_path.display(), "Creating Metal kernel backend");

                // Find and load model weights
                let model_bytes = load_model_bytes(model_path)?;
                info!(
                    model_size_mb = model_bytes.len() / BYTES_PER_MB as usize,
                    "Loaded model weights for Metal backend"
                );

                // Create and initialize Metal backend
                let mut kernels = MetalKernels::new()?;
                kernels.load(&model_bytes)?;
                info!("Metal kernel backend initialized successfully");

                Ok(Box::new(kernels))
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = model_path;
                Err(AosError::Config("Metal backend requires macOS".to_string()))
            }
        }
        BackendChoice::CoreML => {
            #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
            {
                use adapteros_lora_kernel_coreml::{init_coreml, ComputeUnits, CoreMLBackend};

                // Initialize CoreML runtime
                init_coreml()?;

                // Use CpuAndNeuralEngine for optimal ANE utilization
                let compute_units = ComputeUnits::CpuAndNeuralEngine;
                let production_mode = true;

                info!(
                    model_path = %model_path.display(),
                    compute_units = ?compute_units,
                    production_mode = production_mode,
                    "Creating CoreML kernel backend"
                );
                let backend = CoreMLBackend::new(compute_units, production_mode)?;
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
///     Path::new("./models/qwen2.5-7b"),
///     Some(&hash)
/// )?;
/// ```
pub fn create_backend_with_model_and_hash(
    choice: BackendChoice,
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
) -> Result<Box<dyn FusedKernels>> {
    match choice {
        BackendChoice::Mlx => create_mlx_backend(model_path, manifest_hash),
        // For non-MLX backends, manifest hash doesn't apply (they have their own determinism guarantees)
        _ => create_backend_with_model(choice, model_path),
    }
}

/// Internal helper to create MLX backend with optional manifest hash
#[cfg(feature = "multi-backend")]
fn create_mlx_backend(
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
) -> Result<Box<dyn FusedKernels>> {
    use adapteros_lora_mlx_ffi::{
        mlx_runtime_init, mlx_runtime_is_initialized, MLXFFIBackend, MLXFFIModel,
    };

    let model_path_str = model_path.to_string_lossy();
    info!(
        model_path = %model_path_str,
        has_manifest_hash = manifest_hash.is_some(),
        "Creating MLX FFI kernel backend"
    );

    // Ensure MLX runtime is initialized
    if !mlx_runtime_is_initialized() {
        mlx_runtime_init()
            .map_err(|e| AosError::Config(format!("Failed to initialize MLX runtime: {}", e)))?;
    }

    // Load the model
    let model = MLXFFIModel::load(&*model_path_str).map_err(|e| {
        AosError::Config(format!(
            "Failed to load MLX model from '{}': {}",
            model_path_str, e
        ))
    })?;

    // Create backend with or without manifest hash for deterministic seeding
    let backend: Box<dyn FusedKernels> = if let Some(hash) = manifest_hash {
        info!("Creating MLX backend with HKDF-seeded determinism from manifest hash");
        Box::new(
            MLXFFIBackend::with_manifest_hash(model, hash.clone()).map_err(|e| {
                AosError::Config(format!(
                    "Failed to create MLX backend with manifest hash: {}",
                    e
                ))
            })?,
        )
    } else {
        Box::new(MLXFFIBackend::new(model))
    };

    Ok(backend)
}

#[cfg(not(feature = "multi-backend"))]
fn create_mlx_backend(
    _model_path: &Path,
    _manifest_hash: Option<&B3Hash>,
) -> Result<Box<dyn FusedKernels>> {
    Err(AosError::Config(
        "MLX backend requires 'multi-backend' feature to be enabled. \
         Build with: cargo build --features multi-backend"
            .to_string(),
    ))
}

/// Create a kernel backend based on the choice (backward-compatible)
///
/// This function maintains backward compatibility for code that doesn't need model paths.
/// - For Auto/Metal/CoreML: Works as before (no model path needed)
/// - For Mlx: Reads model path from `AOS_MODEL_PATH` env var, errors if not set
///
/// For new code, prefer using `create_backend_from_config` or `create_backend_with_model`.
pub fn create_backend(choice: BackendChoice) -> Result<Box<dyn FusedKernels>> {
    match choice {
        BackendChoice::Auto => {
            let capabilities = detect_capabilities();
            let selected = auto_select_backend(&capabilities)?;
            create_backend(selected)
        }
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
                use adapteros_lora_kernel_coreml::{init_coreml, ComputeUnits, CoreMLBackend};

                // Initialize CoreML runtime
                init_coreml()?;

                // Use CpuAndNeuralEngine for optimal ANE utilization
                let compute_units = ComputeUnits::CpuAndNeuralEngine;
                let production_mode = true;

                info!(
                    compute_units = ?compute_units,
                    production_mode = production_mode,
                    "Creating CoreML kernel backend"
                );
                let backend = CoreMLBackend::new(compute_units, production_mode)?;
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
            // For backward compatibility, read model path from environment variable
            let model_path = std::env::var("AOS_MODEL_PATH").map_err(|_| {
                AosError::Config(
                    "MLX backend requires model path. Set AOS_MODEL_PATH environment variable \
                     or use create_backend_with_model()/create_backend_from_config() instead."
                        .to_string(),
                )
            })?;

            #[cfg(feature = "multi-backend")]
            {
                use adapteros_lora_mlx_ffi::{
                    mlx_runtime_init, mlx_runtime_is_initialized, MLXFFIBackend, MLXFFIModel,
                };

                info!(model_path = %model_path, "Creating MLX FFI kernel backend");

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
                        model_path, e
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

/// Create backend with automatic selection and model size consideration
pub fn create_backend_auto(model_size_bytes: Option<usize>) -> Result<Box<dyn FusedKernels>> {
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
            "  - CoreML: {} (ANE {})\n",
            if caps.has_coreml {
                "Available"
            } else {
                "Not available"
            },
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
