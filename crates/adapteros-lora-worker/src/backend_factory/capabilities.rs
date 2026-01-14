use super::BackendChoice;
use adapteros_core::{
    backend::BackendKind, constants::BYTES_PER_MB, AosError, ExecutionProfile, Result,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

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

    // IMPORTANT: Detect MLX availability FIRST before other Metal usage
    // MLX runtime should initialize its Metal device before other Metal usage
    // to avoid resource conflicts
    #[cfg(feature = "multi-backend")]
    {
        #[cfg(feature = "mlx")]
        {
            // MLX available - check if runtime can be initialized
            use adapteros_lora_mlx_ffi::{
                mlx_runtime_init, mlx_runtime_is_initialized, mlx_selected_implementation,
            };
            caps.has_mlx = mlx_runtime_is_initialized() || mlx_runtime_init().is_ok();
            if caps.has_mlx {
                if let Some(selected) = mlx_selected_implementation() {
                    debug!(
                        implementation = selected.as_str(),
                        "MLX implementation selected"
                    );
                }
            } else {
                debug!("MLX backend not available: runtime initialization failed");
            }
        }
        #[cfg(not(feature = "mlx"))]
        {
            // Only stub available - be honest about it
            caps.has_mlx = false;
            debug!("MLX backend not available: 'mlx' feature not enabled");
        }
    }

    // Detect Metal availability (after MLX to avoid device conflicts)
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
/// MLX → CoreML → MlxBridge → Metal → CPU. CPU remains an observability-only terminal entry
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
        "No suitable backend available. Checked priority MLX → CoreML → MlxBridge → Metal → CPU."
            .to_string(),
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

/// Backend capability reporting types
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
    let caps = detect_capabilities();

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
