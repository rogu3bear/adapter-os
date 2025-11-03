//! Import MLX model command
//!
//! Validates and imports MLX models for use with the MLX C++ FFI backend.
//! Requires --features mlx-ffi-backend to be enabled.

use crate::output::OutputWriter;
use adapteros_core::AosError;
use anyhow::Result;
use std::path::Path;

/// Import MLX model
///
/// Validates model files and prepares them for use with MLX C++ FFI backend.
/// The model directory should contain weights, config, and tokenizer files.
///
/// # Feature Flag
///
/// This command requires `--features mlx-ffi-backend` to be enabled at build time.
#[allow(unused_variables)] // Variables are used conditionally based on feature flags
pub async fn run(
    name: &str,
    weights: &Path,
    config: &Path,
    tokenizer: &Path,
    tokenizer_cfg: &Path,
    license: &Path,
    output: &OutputWriter,
) -> Result<()> {
    #[cfg(not(feature = "mlx-ffi-backend"))]
    {
        output.error("MLX model import requires --features mlx-ffi-backend");
        output.info("Rebuild with: cargo build --features mlx-ffi-backend");
        return Err(AosError::FeatureDisabled {
            feature: "MLX model import".to_string(),
            reason: "mlx-ffi-backend feature not enabled".to_string(),
            alternative: Some("Rebuild with --features mlx-ffi-backend".to_string()),
        }
        .into());
    }

    #[cfg(feature = "mlx-ffi-backend")]
    {
        output.info(format!("Importing MLX model: {}", name));
        output.progress("Validating model files");

        // Validate all required files exist
        let files_to_check = vec![
            ("weights", weights),
            ("config", config),
            ("tokenizer", tokenizer),
            ("tokenizer_config", tokenizer_cfg),
            ("license", license),
        ];

        for (file_type, file_path) in &files_to_check {
            if !file_path.exists() {
                output.progress_done(false);
                return Err(anyhow::anyhow!(
                    "{} file not found: {}",
                    file_type,
                    file_path.display()
                ));
            }
        }

        output.progress_done(true);

        // Determine model directory (use directory containing weights as base)
        let model_dir = weights
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid weights path"))?;

        output.info(format!("Model directory: {}", model_dir.display()));

        // Try to load model via MLX FFI to validate it works
        output.progress("Validating model with MLX FFI");

        #[cfg(feature = "mlx-ffi-backend")]
        {
            use adapteros_lora_mlx_ffi::MLXFFIModel;

            match MLXFFIModel::load(model_dir) {
                Ok(_model) => {
                    output.progress_done(true);
                    tracing::info!(
                        model_name = %name,
                        model_dir = %model_dir.display(),
                        "MLX model validated successfully"
                    );
                }
                Err(e) => {
                    output.progress_done(false);
                    tracing::warn!(
                        model_name = %name,
                        error = %e,
                        "MLX model validation failed - model may still be usable"
                    );
                    output.warning(format!(
                        "Model validation warning: {}. Model files are present but MLX FFI reported an issue.",
                        e
                    ));
                    output.info("You may still be able to use this model - check MLX installation");
                }
            }
        }

        // Set environment variable for runtime use
        let model_dir_str = model_dir.to_string_lossy().to_string();
        std::env::set_var("AOS_MLX_FFI_MODEL", &model_dir_str);

        output.success(format!("MLX model '{}' imported successfully", name));
        output.info(format!(
            "Model directory: {}",
            model_dir.display()
        ));
        output.info(format!(
            "AOS_MLX_FFI_MODEL environment variable set to: {}",
            model_dir_str
        ));
        output.info("You can now use this model with: aosctl serve --backend mlx");

        Ok(())
    }
}
