use adapteros_core::{AosError, Result};
use adapteros_lora_worker::backend_factory::BackendChoice;
use std::str::FromStr;
use tracing::warn;

pub fn validate_backend_feature(choice: &BackendChoice) -> Result<()> {
    if matches!(choice, BackendChoice::Mlx) && !cfg!(feature = "multi-backend") {
        return Err(AosError::Config(
            "MLX backend requested but this binary was built without 'multi-backend'. \
             Rebuild with: cargo build --features multi-backend"
                .to_string(),
        ));
    }
    Ok(())
}

/// Parse backend choice from CLI flag using canonical BackendKind parser.
pub fn parse_backend_choice(raw: &str) -> BackendChoice {
    BackendChoice::from_str(raw).unwrap_or_else(|err| {
        warn!(
            backend = raw,
            error = %err,
            expected = %BackendChoice::variants().join(", "),
            "Invalid backend flag, falling back to auto"
        );
        BackendChoice::Auto
    })
}

pub fn is_mock_backend(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("mock")
}
