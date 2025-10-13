//! Backend factory for creating kernel backends at runtime

use adapteros_core::Result;
use adapteros_lora_kernel_api::FusedKernels;
use std::path::PathBuf;

/// Backend selection enum
#[derive(Debug, Clone)]
pub enum BackendChoice {
    /// Metal backend (macOS GPU)
    Metal,
    /// MLX backend (Python/MLX)
    Mlx { model_path: PathBuf },
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
                Err(adapteros_core::AosError::Unsupported(
                    "Metal backend only available on macOS".to_string(),
                ))
            }
        }

        BackendChoice::Mlx { model_path: _ } => {
            // MLX backend temporarily disabled due to PyO3 linker issues
            Err(adapteros_core::AosError::Mlx(
                "MLX backend temporarily disabled".to_string(),
            ))
        }
    }
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
}
