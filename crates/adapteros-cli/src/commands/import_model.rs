//! Import MLX model command
//!
//! MLX backend is temporarily disabled - requires MLX C++ library.
//! See: crates/adapteros-lora-mlx-ffi/README.md for details.

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
    tokenizer_cfg: &Path,
    license: &Path,
    output: &OutputWriter,
) -> Result<()> {
    warn!(
        name = %name,
        weights = ?weights,
        config = ?config,
        tokenizer = ?tokenizer,
        "MLX model import requested but MLX backend is disabled"
    );

    output.error("MLX model import is temporarily disabled - requires MLX C++ library");
    output.info("Alternative: Use Metal backend for inference");
    output.info("See: crates/adapteros-lora-mlx-ffi/README.md for details");

    Err(AosError::FeatureDisabled {
        feature: "MLX model import".to_string(),
        reason: "Requires MLX C++ library - see crates/adapteros-lora-mlx-ffi/README.md"
            .to_string(),
        alternative: Some("Use Metal backend for inference".to_string()),
    }
    .into())
}
