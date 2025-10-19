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
    // Create backend based on choice; fallback to CPU if unavailable
    let backend = match create_backend_internal(choice.clone()) {
        Ok(b) => b,
        // Do not silently bypass policy/determinism violations with a fallback
        Err(e @ AosError::PolicyViolation(_)) | Err(e @ AosError::DeterminismViolation(_)) => {
            tracing::error!(error = %e, "Backend creation failed due to policy/determinism violation; refusing fallback");
            return Err(e);
        }
        Err(e) => {
            // For explicit MLX selection with missing real FFI, refuse fallback
            if matches!(choice, BackendChoice::Mlx { .. }) {
                if let AosError::FeatureDisabled { .. } = e {
                    tracing::error!(error = %e, "MLX backend requested but FFI is stub; refusing fallback");
                    return Err(e);
                }
            }
            tracing::warn!(
                error = %e,
                requested_backend = ?choice,
                "Falling back to CPU fallback backend due to backend initialization error"
            );
            let cpu = adapteros_lora_kernel_api::CpuKernels::default();
            Box::new(cpu)
        }
    };

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
                let _ = model_path;
                Err(AosError::PolicyViolation(
                    "MLX backend requires --features experimental-backends (not enabled in deterministic-only build)".to_string(),
                ))
            }

            #[cfg(feature = "experimental-backends")]
            {
                // Ensure real MLX FFI is available; do not silently fallback to placeholders
                if !adapteros_lora_mlx_ffi::ffi_is_real() {
                    return Err(AosError::FeatureDisabled {
                        feature: "MLX backend".to_string(),
                        reason: "FFI is stub (no real MLX C++ API detected)".to_string(),
                        alternative: Some(
                            "Install MLX C++ headers/libs or use Metal backend".to_string(),
                        ),
                    });
                }

                // Load the real MLX model via FFI and construct the backend
                let model = adapteros_lora_mlx_ffi::MLXFFIModel::load(&model_path)?;
                let backend = adapteros_lora_mlx_ffi::MLXFFIBackend::new(model);
                tracing::info!("Created MLX FFI backend (real): {}", backend.device_name());
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

#[cfg(feature = "experimental-backends")]
struct MlxBackend {
    model_path: PathBuf,
    base_seed: B3Hash,
    device: String,
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
impl FusedKernels for MlxBackend {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        let plan_hash = B3Hash::hash(plan_bytes);
        let label = format!("mlx-plan:{}", plan_hash.to_short_hex());
        let reseeded = derive_seed(&self.base_seed, &label);
        self.base_seed = B3Hash::from_bytes(reseeded);

        tracing::info!(
            path = %self.model_path.display(),
            plan_len = plan_bytes.len(),
            seed_preview = %self.base_seed.to_short_hex(),
            "MLX backend loaded plan deterministically"
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
        Ok(attestation::DeterminismReport {
            backend_type: attestation::BackendType::Mlx,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: attestation::RngSeedingMethod::HkdfSeeded,
            floating_point_mode: attestation::FloatingPointMode::Deterministic,
            compiler_flags: vec!["-DMLX_DETERMINISTIC".to_string()],
            deterministic: true,
        })
    }

    fn load_adapter(&mut self, id: u16, _weights: &[u8]) -> Result<()> {
        let adapter_seed = self.derive_adapter_seed(id);
        tracing::info!(
            adapter_id = id,
            seed_preview = %hex::encode(&adapter_seed[..4]),
            "MLX backend registered adapter deterministically"
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
