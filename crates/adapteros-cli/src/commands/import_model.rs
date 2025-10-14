//! Import MLX model command
//!
//! MLX backend is temporarily disabled due to PyO3 linker issues.
//! See: crates/adapteros-lora-mlx-ffi/README.md for details.

use anyhow::Result;
use crate::output::OutputWriter;
use std::path::Path;
use tracing::warn;
use adapteros_core::AosError;

/// Import MLX model (currently disabled)
///
/// # Note
///
/// MLX backend is temporarily disabled due to PyO3 linker issues.
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
    
    output.error("MLX model import is temporarily disabled due to PyO3 linker issues");
    output.info("Alternative: Use Metal backend for inference");
    output.info("See: crates/adapteros-lora-mlx-ffi/README.md for details");
    
    Err(AosError::FeatureDisabled {
        feature: "MLX model import".to_string(),
        reason: "PyO3 linker issues - see crates/adapteros-lora-mlx-ffi/README.md".to_string(),
        alternative: Some("Use Metal backend for inference".to_string()),
    }.into())
}
