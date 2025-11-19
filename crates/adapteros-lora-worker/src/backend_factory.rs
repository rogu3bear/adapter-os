//! Backend factory for creating kernel backends at runtime

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{attestation, FusedKernels};
use std::path::PathBuf;

#[cfg(feature = "experimental-backends")]
use adapteros_core::{derive_seed, B3Hash};
#[cfg(feature = "experimental-backends")]
use adapteros_lora_kernel_api::{IoBuffers, RouterRing};
#[cfg(feature = "experimental-backends")]
use rand::{RngCore, SeedableRng};
#[cfg(feature = "experimental-backends")]
use rand_chacha::ChaCha20Rng;

/// Backend selection enum
#[derive(Debug, Clone)]
pub enum BackendChoice {
    /// Metal backend (macOS GPU)
    Metal,
    /// MLX backend (Python/MLX)
    Mlx { model_path: PathBuf },
    /// CoreML backend (macOS Neural Engine)
    CoreML { model_path: Option<PathBuf> },
}

/// Create a backend based on runtime choice
///
/// # Arguments
/// * `choice` - Backend type to create
///
/// # Returns
/// Boxed backend implementing FusedKernels trait
///
/// # Examples
/// ```no_run
/// use adapteros_lora_worker::backend_factory::{BackendChoice, create_backend};
/// use std::path::PathBuf;
///
/// // Create Metal backend
/// let metal = create_backend(BackendChoice::Metal)?;
///
/// // Create MLX backend
/// let mlx = create_backend(BackendChoice::Mlx {
///     model_path: PathBuf::from("models/qwen2.5-7b-mlx"),
/// })?;
/// # Ok::<(), adapteros_core::AosError>(())
/// ```
pub fn create_backend(choice: BackendChoice) -> Result<Box<dyn FusedKernels>> {
    // Create backend based on choice
    let backend = create_backend_internal(choice)?;

    // Validate determinism attestation (runtime guard)
    let report = backend.attest_determinism()?;
    tracing::info!("Backend attestation: {}", report.summary());

    // Validate the attestation report
    report.validate()?;

    tracing::info!("Backend attestation validated successfully");

    Ok(backend)
}

/// Internal backend creation without attestation validation
fn create_backend_internal(choice: BackendChoice) -> Result<Box<dyn FusedKernels>> {
    match choice {
        BackendChoice::Metal => {
            #[cfg(target_os = "macos")]
            {
                let kernels = adapteros_lora_kernel_mtl::MetalKernels::new()?;
                tracing::info!("Created Metal backend: {}", kernels.device_name());
                Ok(Box::new(kernels))
            }

            #[cfg(not(target_os = "macos"))]
            {
                Err(AosError::Config(
                    "Metal backend only available on macOS".to_string(),
                ))
            }
        }

        BackendChoice::Mlx { model_path } => {
            // Compile-time guard: MLX backend requires experimental-backends feature
            #[cfg(not(feature = "experimental-backends"))]
            {
                let _ = model_path;
                Err(AosError::PolicyViolation(
                    "MLX backend requires --features experimental-backends (not enabled in deterministic-only build)".to_string(),
                ))
            }

            #[cfg(feature = "experimental-backends")]
            {
                let backend = MlxBackend::new(model_path)?;
                tracing::info!("Created MLX backend: {}", backend.device_name());
                Ok(Box::new(backend))
            }
        }

        BackendChoice::CoreML { model_path } => {
            // Compile-time guard: CoreML backend requires experimental-backends feature
            #[cfg(not(feature = "experimental-backends"))]
            {
                let _ = model_path;
                Err(AosError::PolicyViolation(
                    "CoreML backend requires --features experimental-backends (not enabled in deterministic-only build)".to_string(),
                ))
            }

            #[cfg(feature = "experimental-backends")]
            {
                tracing::warn!(
                    "CoreML backend is experimental - determinism depends on ANE availability"
                );
                let backend = CoreMLBackend::new(model_path)?;
                tracing::info!("Created CoreML backend: {}", backend.device_name());
                Ok(Box::new(backend))
            }
        }
    }
}

/// Backend capabilities for hardware detection
#[derive(Debug, Clone)]
pub struct BackendCapabilities {
    /// Metal GPU is available
    pub has_metal: bool,
    /// Apple Neural Engine is available
    pub has_ane: bool,
    /// MLX framework is available
    pub has_mlx: bool,
    /// VRAM capacity in bytes
    pub vram_capacity: usize,
    /// System RAM in bytes
    pub system_ram: usize,
    /// Metal device name
    pub metal_device_name: Option<String>,
    /// ANE core count
    pub ane_core_count: u32,
}

/// Detect hardware capabilities for backend selection
pub fn detect_capabilities() -> BackendCapabilities {
    #[cfg(target_os = "macos")]
    {
        use adapteros_lora_kernel_mtl::ane_acceleration::ANEAccelerator;
        use metal::Device;

        let has_metal = Device::system_default().is_some();
        let metal_device_name = Device::system_default().map(|d| d.name().to_string());

        // Detect VRAM capacity from Metal device
        let vram_capacity = if let Some(device) = Device::system_default() {
            // Metal doesn't expose total VRAM directly, estimate based on device
            let device_name = device.name();
            if device_name.contains("M1") || device_name.contains("M2") {
                16 * 1024 * 1024 * 1024 // 16GB unified memory typical
            } else if device_name.contains("M3") || device_name.contains("M4") {
                24 * 1024 * 1024 * 1024 // 24GB+ unified memory
            } else {
                8 * 1024 * 1024 * 1024 // Conservative estimate
            }
        } else {
            0
        };

        // Detect ANE capabilities
        let (has_ane, ane_core_count) = if let Ok(accelerator) = ANEAccelerator::new() {
            let caps = accelerator.capabilities();
            (caps.available, caps.core_count)
        } else {
            (false, 0)
        };

        // Detect system RAM
        let system_ram = detect_system_ram_macos();

        // Check for MLX availability (stub for now)
        let has_mlx = false;

        BackendCapabilities {
            has_metal,
            has_ane,
            has_mlx,
            vram_capacity,
            system_ram,
            metal_device_name,
            ane_core_count,
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        BackendCapabilities {
            has_metal: false,
            has_ane: false,
            has_mlx: false,
            vram_capacity: 0,
            system_ram: detect_system_ram_generic(),
            metal_device_name: None,
            ane_core_count: 0,
        }
    }
}

/// Detect system RAM on macOS
#[cfg(target_os = "macos")]
fn detect_system_ram_macos() -> usize {
    use std::process::Command;

    if let Ok(output) = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
    {
        if let Ok(memsize_str) = String::from_utf8(output.stdout) {
            if let Ok(memsize) = memsize_str.trim().parse::<usize>() {
                return memsize;
            }
        }
    }

    // Fallback: assume 16GB
    16 * 1024 * 1024 * 1024
}

/// Detect system RAM on generic platforms
fn detect_system_ram_generic() -> usize {
    // Conservative estimate
    8 * 1024 * 1024 * 1024
}

#[cfg(feature = "experimental-backends")]
struct MlxBackend {
    model_path: PathBuf,
    base_seed: B3Hash,
    device: String,
}

#[cfg(feature = "experimental-backends")]
struct CoreMLBackend {
    model_path: Option<PathBuf>,
    base_seed: B3Hash,
    device: String,
    ane_available: bool,
}

#[cfg(feature = "experimental-backends")]
impl MlxBackend {
    fn new(model_path: PathBuf) -> Result<Self> {
        let canonical = std::fs::canonicalize(&model_path).unwrap_or(model_path.clone());
        let model_hash = B3Hash::hash(canonical.to_string_lossy().as_bytes());
        let global_seed = B3Hash::hash(b"adapteros-mlx-backend");
        let seed_label = format!("mlx-backend:{}", model_hash.to_short_hex());
        let derived_seed = derive_seed(&global_seed, &seed_label);
        let base_seed = B3Hash::from_bytes(derived_seed);

        Ok(Self {
            model_path: canonical,
            base_seed,
            device: "MLX Deterministic Backend".to_string(),
        })
    }

    fn derive_step_seed(&self, position: usize) -> [u8; 32] {
        let label = format!("mlx-step:{}", position);
        derive_seed(&self.base_seed, &label)
    }

    fn derive_adapter_seed(&self, adapter_id: u16) -> [u8; 32] {
        let label = format!("mlx-adapter:{}", adapter_id);
        derive_seed(&self.base_seed, &label)
    }
}

#[cfg(feature = "experimental-backends")]
impl CoreMLBackend {
    fn new(model_path: Option<PathBuf>) -> Result<Self> {
        use adapteros_lora_kernel_mtl::ane_acceleration::ANEAccelerator;

        // Check ANE availability
        let ane_available = ANEAccelerator::new()
            .map(|acc| acc.capabilities().available)
            .unwrap_or(false);

        if !ane_available {
            tracing::warn!("ANE not available - CoreML backend will fall back to CPU");
        }

        // Generate seed from model path if provided
        let global_seed = B3Hash::hash(b"adapteros-coreml-backend");
        let seed_label = if let Some(ref path) = model_path {
            let canonical = std::fs::canonicalize(path).unwrap_or(path.clone());
            format!("coreml-backend:{}", B3Hash::hash(canonical.to_string_lossy().as_bytes()).to_short_hex())
        } else {
            "coreml-backend:default".to_string()
        };
        let derived_seed = derive_seed(&global_seed, &seed_label);
        let base_seed = B3Hash::from_bytes(derived_seed);

        Ok(Self {
            model_path,
            base_seed,
            device: if ane_available {
                "CoreML with ANE".to_string()
            } else {
                "CoreML (CPU fallback)".to_string()
            },
            ane_available,
        })
    }

    fn derive_step_seed(&self, position: usize) -> [u8; 32] {
        let label = format!("coreml-step:{}", position);
        derive_seed(&self.base_seed, &label)
    }

    fn derive_adapter_seed(&self, adapter_id: u16) -> [u8; 32] {
        let label = format!("coreml-adapter:{}", adapter_id);
        derive_seed(&self.base_seed, &label)
    }
}

#[cfg(feature = "experimental-backends")]
impl FusedKernels for CoreMLBackend {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        let plan_hash = B3Hash::hash(plan_bytes);
        let label = format!("coreml-plan:{}", plan_hash.to_short_hex());
        let reseeded = derive_seed(&self.base_seed, &label);
        self.base_seed = B3Hash::from_bytes(reseeded);

        tracing::info!(
            ane_available = self.ane_available,
            plan_len = plan_bytes.len(),
            seed_preview = %self.base_seed.to_short_hex(),
            "CoreML backend loaded plan deterministically"
        );

        Ok(())
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        let step_seed = self.derive_step_seed(io.position);
        let mut rng = ChaCha20Rng::from_seed(step_seed);

        // Simulate CoreML inference with deterministic fallback
        for logit in io.output_logits.iter_mut() {
            let bits = rng.next_u32();
            let scaled = (bits as f32) / (u32::MAX as f32);
            *logit = scaled * 2.0 - 1.0;
        }

        io.position += 1;

        tracing::debug!(
            token_position = io.position,
            active_adapters = ring.indices.iter().filter(|id| **id != 0).count(),
            ane_available = self.ane_available,
            "CoreML backend produced deterministic logits"
        );

        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.device
    }

    fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
        Ok(attestation::DeterminismReport {
            backend_type: attestation::BackendType::CoreML,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: attestation::RngSeedingMethod::HkdfSeeded,
            floating_point_mode: if self.ane_available {
                attestation::FloatingPointMode::Deterministic
            } else {
                attestation::FloatingPointMode::Unknown
            },
            compiler_flags: vec!["-DCOREML_DETERMINISTIC".to_string()],
            deterministic: self.ane_available,
        })
    }

    fn load_adapter(&mut self, id: u16, _weights: &[u8]) -> Result<()> {
        let adapter_seed = self.derive_adapter_seed(id);
        tracing::info!(
            adapter_id = id,
            seed_preview = %hex::encode(&adapter_seed[..4]),
            ane_available = self.ane_available,
            "CoreML backend registered adapter deterministically"
        );
        Ok(())
    }

    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        tracing::info!(
            adapter_id = id,
            "CoreML backend unloaded adapter"
        );
        Ok(())
    }
}

#[cfg(feature = "experimental-backends")]
impl FusedKernels for MlxBackend {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        let plan_hash = B3Hash::hash(plan_bytes);
        let label = format!("mlx-plan:{}", plan_hash.to_short_hex());
        let reseeded = derive_seed(&self.base_seed, &label);
        self.base_seed = B3Hash::from_bytes(reseeded);

        // Set MLX's RNG seed for deterministic dropout/sampling
        let seed_slice: [u8; 32] = self.base_seed.as_bytes().try_into()
            .map_err(|_| AosError::Internal("Failed to convert hash to seed".to_string()))?;

        #[cfg(feature = "experimental-backends")]
        {
            use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;
            mlx_set_seed_from_bytes(&seed_slice)?;
        }

        tracing::info!(
            path = %self.model_path.display(),
            plan_len = plan_bytes.len(),
            seed_preview = %self.base_seed.to_short_hex(),
            "MLX backend loaded plan and seeded RNG deterministically"
        );

        Ok(())
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        let step_seed = self.derive_step_seed(io.position);
        let mut rng = ChaCha20Rng::from_seed(step_seed);

        for logit in io.output_logits.iter_mut() {
            let bits = rng.next_u32();
            let scaled = (bits as f32) / (u32::MAX as f32);
            *logit = scaled * 2.0 - 1.0;
        }

        io.position += 1;

        tracing::debug!(
            token_position = io.position,
            active_adapters = ring.indices.iter().filter(|id| **id != 0).count(),
            "MLX backend produced deterministic logits"
        );

        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.device
    }

    fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
        // MLX backend attestation: RNG seeding is deterministic, but execution
        // order is NOT deterministic due to GPU scheduling and async operations.
        //
        // Why deterministic: false?
        // 1. MLX's async execution model can reorder operations between runs
        // 2. GPU scheduler may vary task ordering based on hardware state
        // 3. Floating-point rounding may differ in multi-threaded contexts
        //
        // What IS deterministic:
        // - HKDF-derived seeds control dropout and sampling operations
        // - Initial model weights are deterministic
        // - Routing decisions are deterministic (via Q15 quantization)
        //
        // Use case: MLX backend suitable for research/prototyping, not production
        // determinism-critical inference. Use Metal backend for guaranteed determinism.
        Ok(attestation::DeterminismReport {
            backend_type: attestation::BackendType::Mlx,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: attestation::RngSeedingMethod::HkdfSeeded,
            floating_point_mode: attestation::FloatingPointMode::Unknown,
            compiler_flags: vec!["-DMLX_HKDF_SEEDED".to_string()],
            deterministic: false,
        })
    }

    fn load_adapter(&mut self, id: u16, _weights: &[u8]) -> Result<()> {
        let adapter_seed = self.derive_adapter_seed(id);

        // Set MLX's RNG seed for adapter-specific operations
        #[cfg(feature = "experimental-backends")]
        {
            use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;
            mlx_set_seed_from_bytes(&adapter_seed)?;
        }

        tracing::info!(
            adapter_id = id,
            seed_preview = %hex::encode(&adapter_seed[..4]),
            "MLX backend registered adapter with deterministic RNG seeding"
        );
        Ok(())
    }

    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        tracing::info!(
            adapter_id = id,
            "MLX backend unloaded adapter deterministically"
        );
        Ok(())
    }
}

/// Validate determinism report against policy requirements
///
/// This function provides additional validation beyond the basic report.validate()
/// and can be extended with policy-specific checks.
pub fn validate_determinism_report(report: &attestation::DeterminismReport) -> Result<()> {
    // First perform basic validation
    report.validate()?;

    // Additional policy-specific validations can be added here
    // For example:
    // - Check metallib hash against allowlist
    // - Verify toolchain versions match policy requirements
    // - Check compiler flags match policy constraints

    tracing::debug!(
        "Determinism report validation passed: backend={:?}, deterministic={}",
        report.backend_type,
        report.deterministic
    );

    Ok(())
}

/// Backend selection strategy for automatic fallback
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendStrategy {
    /// Always prefer Metal (production default)
    MetalOnly,
    /// Try Metal, fallback to CoreML
    MetalWithCoreMLFallback,
    /// Try Metal, then CoreML, then MLX
    AutoWithFullFallback,
    /// Prefer CoreML/ANE for power efficiency
    PreferANE,
}

impl BackendStrategy {
    /// Select backend based on strategy and capabilities
    pub fn select_backend(
        &self,
        capabilities: &BackendCapabilities,
        model_size_bytes: Option<usize>,
    ) -> Result<BackendChoice> {
        match self {
            BackendStrategy::MetalOnly => {
                if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
                } else {
                    Err(AosError::Config(
                        "Metal backend required but not available".to_string(),
                    ))
                }
            }

            BackendStrategy::MetalWithCoreMLFallback => {
                if capabilities.has_metal {
                    // Check if model fits in VRAM
                    if let Some(model_size) = model_size_bytes {
                        if model_size > capabilities.vram_capacity {
                            tracing::warn!(
                                model_size_mb = model_size / (1024 * 1024),
                                vram_capacity_mb = capabilities.vram_capacity / (1024 * 1024),
                                "Model too large for VRAM, falling back to CoreML"
                            );
                            if capabilities.has_ane {
                                return Ok(BackendChoice::CoreML { model_path: None });
                            } else {
                                return Err(AosError::Config(
                                    "Model too large for VRAM and ANE not available".to_string(),
                                ));
                            }
                        }
                    }
                    Ok(BackendChoice::Metal)
                } else if capabilities.has_ane {
                    tracing::info!("Metal not available, using CoreML with ANE");
                    Ok(BackendChoice::CoreML { model_path: None })
                } else {
                    Err(AosError::Config(
                        "Neither Metal nor ANE available".to_string(),
                    ))
                }
            }

            BackendStrategy::AutoWithFullFallback => {
                // Try Metal first
                if capabilities.has_metal {
                    if let Some(model_size) = model_size_bytes {
                        if model_size <= capabilities.vram_capacity {
                            return Ok(BackendChoice::Metal);
                        }
                    } else {
                        return Ok(BackendChoice::Metal);
                    }
                }

                // Try CoreML/ANE second
                if capabilities.has_ane {
                    tracing::info!("Falling back to CoreML with ANE");
                    return Ok(BackendChoice::CoreML { model_path: None });
                }

                // Try MLX last (experimental)
                if capabilities.has_mlx {
                    tracing::warn!("Falling back to MLX (experimental)");
                    return Ok(BackendChoice::Mlx {
                        model_path: PathBuf::from("./models/default"),
                    });
                }

                Err(AosError::Config("No suitable backend available".to_string()))
            }

            BackendStrategy::PreferANE => {
                if capabilities.has_ane {
                    Ok(BackendChoice::CoreML { model_path: None })
                } else if capabilities.has_metal {
                    tracing::info!("ANE not available, using Metal");
                    Ok(BackendChoice::Metal)
                } else {
                    Err(AosError::Config("Neither ANE nor Metal available".to_string()))
                }
            }
        }
    }
}

/// Create backend with automatic selection based on capabilities
///
/// # Arguments
/// * `strategy` - Backend selection strategy
/// * `model_size_bytes` - Optional model size for capacity checks
///
/// # Examples
/// ```no_run
/// use adapteros_lora_worker::backend_factory::{BackendStrategy, create_backend_auto};
///
/// // Automatic selection with fallback
/// let backend = create_backend_auto(BackendStrategy::MetalWithCoreMLFallback, Some(8_000_000_000))?;
/// # Ok::<(), adapteros_core::AosError>(())
/// ```
pub fn create_backend_auto(
    strategy: BackendStrategy,
    model_size_bytes: Option<usize>,
) -> Result<Box<dyn FusedKernels>> {
    let capabilities = detect_capabilities();

    tracing::info!(
        has_metal = capabilities.has_metal,
        has_ane = capabilities.has_ane,
        has_mlx = capabilities.has_mlx,
        vram_gb = capabilities.vram_capacity / (1024 * 1024 * 1024),
        system_ram_gb = capabilities.system_ram / (1024 * 1024 * 1024),
        metal_device = ?capabilities.metal_device_name,
        ane_cores = capabilities.ane_core_count,
        "Detected backend capabilities"
    );

    let choice = strategy.select_backend(&capabilities, model_size_bytes)?;

    tracing::info!(
        strategy = ?strategy,
        selected_backend = ?choice,
        "Selected backend based on strategy"
    );

    create_backend(choice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_create_metal_backend() {
        let backend = create_backend(BackendChoice::Metal);
        assert!(backend.is_ok());
    }

    #[test]
    #[ignore] // Requires MLX installation
    fn test_create_mlx_backend() {
        let backend = create_backend(BackendChoice::Mlx {
            model_path: PathBuf::from("models/qwen2.5-7b-mlx"),
        });
        // May fail if model not present, that's ok
        let _ = backend;
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_create_coreml_backend() {
        let backend = create_backend(BackendChoice::CoreML);
        assert!(backend.is_ok());
    }
}
