//! Backend factory for creating kernel backends at runtime

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{attestation, FusedKernels};
use std::path::PathBuf;

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
    let mut backend = create_backend_internal(choice)?;
    
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
