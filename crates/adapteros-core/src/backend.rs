use crate::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Canonical backend selector used across AdapterOS (inference + training).
///
/// All user- and config-facing backend strings must parse through this type so
/// we have a single source of truth (and error messages) for CoreML, MLX,
/// Metal, CPU, and Auto detection. CoreML is treated as a first-class option
/// (including ANE) and `Auto` preserves the current behavior of "pick the best
/// available backend" without changing defaults for existing callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    /// Deterministic auto-selection (preserves existing defaults)
    #[serde(alias = "autodev", alias = "auto_dev", alias = "default")]
    #[default]
    Auto,
    /// CoreML / ANE acceleration (macOS)
    #[serde(alias = "core-ml", alias = "ane")]
    CoreML,
    /// MLX FFI backend (macOS/Linux, research/training)
    #[serde(alias = "mlx")]
    Mlx,
    /// MLX subprocess bridge (Python mlx-lm for MoE models)
    #[serde(alias = "mlx-bridge", alias = "mlx_bridge", alias = "subprocess")]
    MlxBridge,
    /// Metal GPU backend (macOS)
    #[serde(alias = "metal")]
    Metal,
    /// CPU-only execution
    #[serde(alias = "cpu_only", alias = "cpu-only")]
    CPU,
}

impl BackendKind {
    /// Canonical string for logging/config surface.
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendKind::Auto => "auto",
            BackendKind::CoreML => "coreml",
            BackendKind::Mlx => "mlx",
            BackendKind::MlxBridge => "mlxbridge",
            BackendKind::Metal => "metal",
            BackendKind::CPU => "cpu",
        }
    }

    /// List of canonical variants for error reporting.
    pub fn variants() -> &'static [&'static str] {
        &["auto", "coreml", "mlx", "mlxbridge", "metal", "cpu"]
    }

    /// Canonical CoreML-first priority list for inference backends.
    ///
    /// Order: CoreML → MLX → MlxBridge → Metal → CPU. Use this helper anywhere
    /// a fallback chain is needed so the system stays consistent across control
    /// plane, worker selection, and UI hints.
    ///
    /// Note: MlxBridge is positioned after Mlx because it's intended for
    /// MoE models that Mlx FFI doesn't support. Auto-selection logic may
    /// prefer MlxBridge when it detects an MoE model.
    pub fn inference_priority() -> &'static [BackendKind] {
        // NOTE: CPU remains last for observability even though inference kernels
        // are not implemented for CPU today.
        static ORDER: [BackendKind; 5] = [
            BackendKind::CoreML,
            BackendKind::Mlx,
            BackendKind::MlxBridge,
            BackendKind::Metal,
            BackendKind::CPU,
        ];
        &ORDER
    }

    /// Check if this backend is an MLX variant (FFI or Bridge)
    pub fn is_mlx_variant(&self) -> bool {
        matches!(self, BackendKind::Mlx | BackendKind::MlxBridge)
    }

    /// CoreML-first default backend when capabilities allow it.
    ///
    /// On macOS builds with the CoreML backend enabled we default to CoreML.
    /// Other platforms fall back to Auto, which will still respect the
    /// `inference_priority()` order at selection time.
    pub fn default_inference_backend() -> BackendKind {
        if cfg!(all(target_os = "macos", feature = "coreml-backend")) {
            BackendKind::CoreML
        } else {
            BackendKind::Auto
        }
    }
}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BackendKind {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        let normalized = s.trim().to_ascii_lowercase().replace(['-', '_'], "");
        let kind = match normalized.as_str() {
            "auto" | "autodev" | "default" => BackendKind::Auto,
            "coreml" | "ane" => BackendKind::CoreML,
            "mlx" => BackendKind::Mlx,
            "mlxbridge" | "subprocess" => BackendKind::MlxBridge,
            "metal" => BackendKind::Metal,
            "cpu" | "cpuonly" => BackendKind::CPU,
            _ => {
                return Err(AosError::Config(format!(
                    "Invalid backend '{}'. Expected one of: {}",
                    s,
                    BackendKind::variants().join(", ")
                )))
            }
        };

        Ok(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_round_trips() {
        for kind in [
            BackendKind::Auto,
            BackendKind::CoreML,
            BackendKind::Mlx,
            BackendKind::MlxBridge,
            BackendKind::Metal,
            BackendKind::CPU,
        ] {
            let rendered = kind.to_string();
            let parsed = BackendKind::from_str(&rendered).unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn parses_aliases() {
        assert_eq!(BackendKind::from_str("autodev").unwrap(), BackendKind::Auto);
        assert_eq!(
            BackendKind::from_str("core-ml").unwrap(),
            BackendKind::CoreML
        );
        assert_eq!(
            BackendKind::from_str("mlx-bridge").unwrap(),
            BackendKind::MlxBridge
        );
        assert_eq!(
            BackendKind::from_str("mlx_bridge").unwrap(),
            BackendKind::MlxBridge
        );
        assert_eq!(
            BackendKind::from_str("subprocess").unwrap(),
            BackendKind::MlxBridge
        );
        assert_eq!(BackendKind::from_str("cpu_only").unwrap(), BackendKind::CPU);
    }

    #[test]
    fn rejects_unknown_backend() {
        let err = BackendKind::from_str("unknown-backend").unwrap_err();
        assert!(err
            .to_string()
            .contains("Expected one of: auto, coreml, mlx, mlxbridge, metal, cpu"));
    }

    #[test]
    fn is_mlx_variant() {
        assert!(BackendKind::Mlx.is_mlx_variant());
        assert!(BackendKind::MlxBridge.is_mlx_variant());
        assert!(!BackendKind::CoreML.is_mlx_variant());
        assert!(!BackendKind::Metal.is_mlx_variant());
        assert!(!BackendKind::Auto.is_mlx_variant());
    }

    #[test]
    fn inference_priority_includes_mlxbridge() {
        let priority = BackendKind::inference_priority();
        assert!(priority.contains(&BackendKind::MlxBridge));

        // Verify order: MLX before MlxBridge
        let mlx_pos = priority.iter().position(|&b| b == BackendKind::Mlx);
        let bridge_pos = priority.iter().position(|&b| b == BackendKind::MlxBridge);
        assert!(mlx_pos.unwrap() < bridge_pos.unwrap());
    }
}
