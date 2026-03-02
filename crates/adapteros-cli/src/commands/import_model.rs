//! Import MLX model command
//!
//! Imports and validates an MLX model directory for use with the MLX backend.

use crate::output::OutputWriter;
use adapteros_core::AosError;
use anyhow::Result;
use std::path::Path;
use tracing::warn;

/// Import MLX model (currently disabled)
///
/// # Note
///
/// MLX backend is temporarily disabled - requires MLX C++ library.
/// Use Metal backend for inference instead.
///
/// See: `crates/adapteros-lora-mlx-ffi/README.md` for details.
pub async fn run(
    name: &str,
    weights: &Path,
    config: &Path,
    tokenizer: &Path,
    _tokenizer_cfg: &Path,
    _license: &Path,
    output: &OutputWriter,
) -> Result<()> {
    warn!(
        name = %name,
        weights = ?weights,
        config = ?config,
        tokenizer = ?tokenizer,
        "MLX model import requested but MLX backend is disabled"
    );

    output.info("Verifying MLX model weights...");

    #[cfg(feature = "multi-backend")]
    {
        // Defer to the FFI crate to perform the actual import/validation
        match adapteros_lora_mlx_ffi::import_model(weights, config, tokenizer) {
            Ok(_) => {
                output.success("MLX model imported successfully");
                Ok(())
            }
            Err(e) => {
                output.error(&format!("Failed to import MLX model: {}", e));
                Err(e.into())
            }
        }
    }

    #[cfg(not(feature = "multi-backend"))]
    {
        let msg = "MLX backend disabled - requires multi-backend feature to be compiled";
        output.error(msg);
        Err(AosError::FeatureDisabled {
            feature: "MLX Import".to_string(),
            reason: msg.to_string(),
            alternative: Some("Recompile adapteros-cli with --features multi-backend".to_string()),
        }
        .into())
    }
}
