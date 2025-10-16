//! Backend factory for creating kernel backends at runtime

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{attestation, FusedKernels};
use std::path::PathBuf;

#[cfg(feature = "experimental-backends")]
mod mlx_backend {
    use super::*;
    use crate::deterministic_rng::DeterministicRng;
    use adapteros_core::B3Hash;
    use adapteros_lora_kernel_api::{IoBuffers, RouterRing};

    /// Deterministic MLX backend stub used for testing worker integration
    pub(super) struct MlxBackend {
        /// Path to the MLX model (used for deterministic seeding)
        model_path: PathBuf,
        /// Human readable device name
        device_name: String,
        /// Global seed derived from model path
        seed_global: [u8; 32],
        /// Optional hash of the most recently loaded plan
        plan_hash: Option<B3Hash>,
        /// Monotonic counter to mix into HKDF labels
        step_counter: u64,
    }

    impl MlxBackend {
        /// Create a new MLX backend using deterministic HKDF seeding
        pub fn new(model_path: PathBuf) -> Result<Self> {
            let model_path_str = model_path.to_string_lossy();
            let seed_hash = B3Hash::hash(model_path_str.as_bytes());
            let seed_global = seed_hash.to_bytes();

            // Bootstrap deterministic RNG to validate HKDF flow
            let bootstrap_label = format!("mlx::bootstrap::{}", seed_hash.to_short_hex());
            let mut bootstrap_rng = DeterministicRng::new(&seed_global, &bootstrap_label)?;
            // Advance RNG once to ensure identical bootstrap state across runs
            let _ = bootstrap_rng.next_u64();

            Ok(Self {
                device_name: format!("MLX Backend (deterministic stub)"),
                model_path,
                seed_global,
                plan_hash: None,
                step_counter: 0,
            })
        }

        fn plan_hash_prefix(&self) -> String {
            self.plan_hash
                .as_ref()
                .map(|hash| hash.to_short_hex())
                .unwrap_or_else(|| "no-plan".to_string())
        }

        fn seed_prefix(&self) -> String {
            hex::encode(&self.seed_global[..4])
        }
    }

    impl FusedKernels for MlxBackend {
        fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
            if plan_bytes.is_empty() {
                self.plan_hash = None;
                self.step_counter = 0;
                tracing::info!("MLX backend loaded with empty plan (noop)");
                return Ok(());
            }

            let plan_hash = B3Hash::hash(plan_bytes);
            let label = format!(
                "mlx::load::{}::{}",
                self.seed_prefix(),
                plan_hash.to_short_hex()
            );
            let mut rng = DeterministicRng::new(&self.seed_global, &label)?;
            // Mix plan hash into deterministic state
            self.step_counter = rng.next_u64();
            self.plan_hash = Some(plan_hash);

            tracing::info!(
                plan_hash = %self.plan_hash_prefix(),
                step_counter = self.step_counter,
                "MLX backend loaded deterministic plan"
            );

            Ok(())
        }

        fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
            let mut context =
                Vec::with_capacity(8 + 8 + ring.indices.len() * 2 + ring.gates_q15.len() * 2);
            context.extend_from_slice(&self.step_counter.to_le_bytes());
            context.extend_from_slice(&(ring.position as u64).to_le_bytes());
            context.extend_from_slice(&(io.position as u64).to_le_bytes());
            for idx in &ring.indices {
                context.extend_from_slice(&idx.to_le_bytes());
            }
            for gate in &ring.gates_q15 {
                context.extend_from_slice(&gate.to_le_bytes());
            }
            if let Some(plan_hash) = &self.plan_hash {
                context.extend_from_slice(plan_hash.as_bytes());
            }

            let ring_hash = B3Hash::hash(&context);
            let label = format!(
                "mlx::step::{}::{}",
                self.seed_prefix(),
                ring_hash.to_short_hex()
            );
            let mut rng = DeterministicRng::new(&self.seed_global, &label)?;

            for (i, logit) in io.output_logits.iter_mut().enumerate() {
                let raw = rng.next_u32();
                // Map deterministic u32 into [-1.0, 1.0) range for logits
                let normalized = (raw as f32) / (u32::MAX as f32);
                *logit = (normalized * 2.0) - 1.0 + (i as f32 * 1e-4);
            }

            io.position += 1;
            self.step_counter = self.step_counter.wrapping_add(1);

            tracing::debug!(
                plan_hash = %self.plan_hash_prefix(),
                step = self.step_counter,
                logits = io.output_logits.len(),
                "MLX backend produced deterministic logits"
            );

            Ok(())
        }

        fn device_name(&self) -> &str {
            &self.device_name
        }

        fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
            Ok(attestation::DeterminismReport {
                backend_type: attestation::BackendType::Mlx,
                metallib_hash: None,
                manifest: None,
                rng_seed_method: attestation::RngSeedingMethod::HkdfSeeded,
                floating_point_mode: attestation::FloatingPointMode::Deterministic,
                compiler_flags: vec!["mlx-deterministic-stub".to_string()],
                deterministic: true,
            })
        }
    }
}

#[cfg(feature = "experimental-backends")]
use mlx_backend::MlxBackend;

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

        BackendChoice::Mlx { model_path } => {
            // Compile-time guard: MLX backend requires experimental-backends feature
            #[cfg(not(feature = "experimental-backends"))]
            {
                Err(AosError::PolicyViolation(
                    "MLX backend requires --features experimental-backends (not enabled in deterministic-only build)"
                        .to_string(),
                ))
            }

            #[cfg(feature = "experimental-backends")]
            {
                tracing::info!(
                    "Initializing experimental MLX backend with deterministic HKDF seeding"
                );
                let backend = MlxBackend::new(model_path)?;
                tracing::info!(
                    device = backend.device_name(),
                    "Created MLX backend with deterministic attestation"
                );
                Ok(Box::new(backend))
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
                tracing::warn!(
                    "CoreML backend is experimental - determinism depends on ANE availability"
                );
                Err(AosError::Other(
                    "CoreML backend temporarily disabled due to dependency issues".to_string(),
                ))
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
    #[cfg(feature = "experimental-backends")]
    fn test_create_mlx_backend() {
        let backend = create_backend(BackendChoice::Mlx {
            model_path: PathBuf::from("models/qwen2.5-7b-mlx"),
        });
        assert!(backend.is_ok());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_create_coreml_backend() {
        let backend = create_backend(BackendChoice::CoreML);
        assert!(backend.is_ok());
    }
}
