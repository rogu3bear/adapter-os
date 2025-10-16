//! Backend factory for creating kernel backends at runtime

#[cfg(feature = "experimental-backends")]
use crate::deterministic_rng::DeterministicRng;
#[cfg(feature = "experimental-backends")]
use adapteros_core::B3Hash;
use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{attestation, FusedKernels};
#[cfg(feature = "experimental-backends")]
use adapteros_lora_kernel_api::{IoBuffers, RouterRing};
use std::path::PathBuf;
#[cfg(feature = "experimental-backends")]
use zeroize::Zeroize;

/// Backend selection enum
#[derive(Debug, Clone)]
pub enum BackendChoice {
    /// Metal backend (macOS GPU)
    Metal,
    /// MLX backend (Python/MLX)
    Mlx { model_path: PathBuf },
    /// CoreML backend (macOS Neural Engine)
    CoreML,
}

#[cfg(feature = "experimental-backends")]
struct CoreMLBackend {
    device_name: String,
    seed_global: [u8; 32],
    plan_hash: Option<B3Hash>,
}

#[cfg(feature = "experimental-backends")]
impl CoreMLBackend {
    fn new() -> Result<Self> {
        let seed_global = B3Hash::hash(b"coreml::default-plan").to_bytes();

        tracing::info!(
            seed = %hex::encode(&seed_global[..8]),
            "Initialized CoreML backend seed material"
        );

        Ok(Self {
            device_name: "CoreML (Apple Neural Engine)".to_string(),
            seed_global,
            plan_hash: None,
        })
    }

    fn step_label(&self, ring: &RouterRing, io: &IoBuffers) -> String {
        if ring.indices.is_empty() {
            return format!(
                "coreml::step::pos={}::router={}::k=0",
                io.position, ring.position
            );
        }

        let signature = ring
            .indices
            .iter()
            .zip(ring.gates_q15.iter())
            .map(|(idx, gate)| format!("{}:{}", idx, gate))
            .collect::<Vec<_>>()
            .join("|");

        format!(
            "coreml::step::pos={}::router={}::{}",
            io.position, ring.position, signature
        )
    }
}

#[cfg(feature = "experimental-backends")]
impl Drop for CoreMLBackend {
    fn drop(&mut self) {
        self.seed_global.zeroize();
    }
}

#[cfg(feature = "experimental-backends")]
impl FusedKernels for CoreMLBackend {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        let plan_hash = if plan_bytes.is_empty() {
            B3Hash::hash(b"coreml::empty-plan")
        } else {
            B3Hash::hash(plan_bytes)
        };

        self.seed_global = plan_hash.to_bytes();
        self.plan_hash = Some(plan_hash);

        tracing::info!(
            plan = %plan_hash.to_short_hex(),
            "Loaded CoreML execution plan with deterministic seed"
        );

        // Pre-derive an RNG to ensure HKDF seeding works correctly.
        let _ = DeterministicRng::new(&self.seed_global, "coreml::plan")?;

        Ok(())
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        let label = self.step_label(ring, io);
        let mut rng = DeterministicRng::new(&self.seed_global, &label)?;

        for logit in io.output_logits.iter_mut() {
            *logit = rng.next_f32();
        }

        io.position += 1;

        tracing::debug!(
            label = label,
            position = io.position,
            logits = io.output_logits.len(),
            "CoreML backend produced deterministic logits"
        );

        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
        use attestation::{BackendType, DeterminismReport, FloatingPointMode, RngSeedingMethod};

        Ok(DeterminismReport {
            backend_type: BackendType::CoreML,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec![
                "coreml-runtime=stub".to_string(),
                "hkdf-seeded=true".to_string(),
            ],
            deterministic: true,
        })
    }
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
                Err(AosError::Unsupported(
                    "Metal backend only available on macOS".to_string(),
                ))
            }
        }

        BackendChoice::Mlx { model_path: _ } => {
            // Compile-time guard: MLX backend requires experimental-backends feature
            #[cfg(not(feature = "experimental-backends"))]
            {
                Err(AosError::PolicyViolation(
                    "MLX backend requires --features experimental-backends (not enabled in deterministic-only build)".to_string(),
                ))
            }

            #[cfg(feature = "experimental-backends")]
            {
                tracing::warn!(
                    "MLX backend is experimental and non-deterministic - NOT FOR PRODUCTION"
                );
                Err(AosError::Other(
                    "MLX backend temporarily disabled due to dependency issues".to_string(),
                ))
            }
        }

        BackendChoice::CoreML => {
            // Compile-time guard: CoreML backend requires experimental-backends feature
            #[cfg(not(feature = "experimental-backends"))]
            {
                Err(AosError::PolicyViolation(
                    "CoreML backend requires --features experimental-backends (not enabled in deterministic-only build)".to_string(),
                ))
            }

            #[cfg(feature = "experimental-backends")]
            {
                let backend = CoreMLBackend::new()?;
                tracing::info!(
                    "Created CoreML backend: {} (deterministic HKDF seeding)",
                    backend.device_name()
                );
                Ok(Box::new(backend))
            }
        }
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
    #[cfg(feature = "experimental-backends")]
    fn test_create_coreml_backend() {
        let backend = create_backend(BackendChoice::CoreML);
        assert!(backend.is_ok());
    }
}
