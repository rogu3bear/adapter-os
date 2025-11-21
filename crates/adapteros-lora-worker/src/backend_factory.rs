//! Backend factory for creating kernel implementations
//!
//! This module provides factory functions for creating different kernel backends
//! (Metal, CoreML, MLX) and capability detection.

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::FusedKernels;
use tracing::{debug, info, warn};

/// Backend choice for kernel creation
#[derive(Debug, Clone)]
pub enum BackendChoice {
    /// Metal GPU backend (production, deterministic)
    Metal,
    /// CoreML backend with optional model path (ANE acceleration)
    CoreML { model_path: Option<String> },
    /// MLX backend with model path (experimental)
    Mlx { model_path: String },
    /// Automatic selection based on capabilities (CoreML -> Metal -> MLX fallback)
    Auto,
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
                    Ok(BackendChoice::CoreML { model_path: None })
                } else {
                    Err(AosError::Config("No suitable backend available".to_string()))
                }
            }
            BackendStrategy::CoreMLWithMetalFallback => {
                if capabilities.has_coreml && capabilities.has_ane {
                    Ok(BackendChoice::CoreML { model_path: None })
                } else if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
                } else {
                    Err(AosError::Config("No suitable backend available".to_string()))
                }
            }
            BackendStrategy::MlxPrimary => {
                if capabilities.has_mlx {
                    Err(AosError::Config(
                        "MLX backend requires explicit model path".to_string(),
                    ))
                } else {
                    Err(AosError::Config(
                        "MLX backend not available (requires experimental-backends feature)"
                            .to_string(),
                    ))
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
    #[cfg(feature = "experimental-backends")]
    {
        caps.has_mlx = true;
    }

    debug!(
        has_metal = caps.has_metal,
        metal_device = ?caps.metal_device_name,
        has_ane = caps.has_ane,
        has_coreml = caps.has_coreml,
        has_mlx = caps.has_mlx,
        gpu_memory_mb = caps.gpu_memory_bytes.map(|b| b / (1024 * 1024)),
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
        return Ok(BackendChoice::CoreML { model_path: None });
    }

    // Priority 2: Metal (production, guaranteed determinism)
    if capabilities.has_metal {
        info!(
            device = ?capabilities.metal_device_name,
            "Auto-selected Metal backend"
        );
        return Ok(BackendChoice::Metal);
    }

    // Priority 3: MLX (experimental, requires explicit model path)
    if capabilities.has_mlx {
        warn!("MLX available but requires explicit model path - use BackendChoice::Mlx instead");
        return Err(AosError::Config(
            "MLX backend requires explicit model path".to_string(),
        ));
    }

    Err(AosError::Config(
        "No suitable backend available. Ensure Metal GPU or CoreML with ANE is present."
            .to_string(),
    ))
}

/// Create a kernel backend based on the choice
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
                Err(AosError::Config(
                    "Metal backend requires macOS".to_string(),
                ))
            }
        }
        BackendChoice::CoreML { model_path } => {
            #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
            {
                use adapteros_lora_kernel_coreml::{init_coreml, CoreMLBackend};

                // Initialize CoreML runtime
                init_coreml()?;

                info!(model_path = ?model_path, "Creating CoreML kernel backend");
                let backend = CoreMLBackend::new(model_path)?;
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
        BackendChoice::Mlx { model_path } => {
            #[cfg(feature = "experimental-backends")]
            {
                use adapteros_lora_mlx_ffi::{MLXFFIBackend, MLXFFIModel};

                info!(model_path = %model_path, "Creating MLX FFI kernel backend");

                // Load the model
                let model = MLXFFIModel::load(&model_path).map_err(|e| {
                    AosError::Config(format!("Failed to load MLX model from '{}': {}", model_path, e))
                })?;

                let backend = MLXFFIBackend::new(model);
                Ok(Box::new(backend))
            }
            #[cfg(not(feature = "experimental-backends"))]
            {
                let _ = model_path;
                Err(AosError::Config(
                    "MLX backend requires 'experimental-backends' feature to be enabled. \
                     Build with: cargo build --features experimental-backends"
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
                model_size_mb = model_size / (1024 * 1024),
                gpu_memory_mb = gpu_mem / (1024 * 1024),
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
            caps.gpu_memory_bytes.unwrap_or(0) / (1024 * 1024)
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
            if caps.has_ane { "available" } else { "not available" }
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
        Metal,   // Real Metal backend
        CoreML,  // Real CoreML backend
        Mlx,     // Real MLX backend
        Cpu,     // Fallback CPU
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
                    if caps.has_ane { "ANE available" } else { "ANE not available" }
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
                    "experimental-backends feature".to_string(),
                ],
            },
        ]
    }

    /// Print backend status report
    pub fn print_backend_status() {
        println!("AdapterOS Backend Status Report");
        println!("================================");
        println!();

        let backends = get_available_backends();
        let available_count = backends.iter().filter(|b| b.available).count();

        println!("Summary:");
        println!(
            "  Available backends: {}/{}",
            available_count,
            backends.len()
        );
        println!();

        println!("Backend Details:");
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

            println!(
                "  [{}] {} ({}) - {}",
                status, backend.name, determinism, backend.description
            );
            if !backend.available {
                println!("    Requirements: {}", backend.requirements.join(", "));
            }
            println!();
        }

        println!("For more details, see docs/ADR_MULTI_BACKEND_STRATEGY.md");
    }
}
