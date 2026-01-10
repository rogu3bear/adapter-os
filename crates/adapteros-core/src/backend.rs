use crate::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Canonical backend selector used across AdapterOS (inference + training).
///
/// All user- and config-facing backend strings must parse through this type so
/// we have a single source of truth (and error messages) for CoreML, MLX,
/// Metal, CPU, and Auto detection. MLX is the primary backend for its
/// flexibility and HKDF-seeded determinism, with CoreML as a high-performance
/// fallback when ANE acceleration is preferred. `Auto` preserves the current
/// behavior of "pick the best available backend".
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
    #[serde(alias = "mlx", alias = "mlx-ffi", alias = "mlx_ffi")]
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

    /// Canonical MLX-first priority list for inference backends.
    ///
    /// Order: MLX → CoreML → MlxBridge → Metal → CPU. Use this helper anywhere
    /// a fallback chain is needed so the system stays consistent across control
    /// plane, worker selection, and UI hints.
    ///
    /// MLX is primary for its flexibility and HKDF-seeded determinism. CoreML
    /// is the first fallback for ANE acceleration when MLX is unavailable.
    /// MlxBridge is positioned after CoreML because it's intended for MoE
    /// models that MLX FFI doesn't support.
    pub fn inference_priority() -> &'static [BackendKind] {
        // NOTE: CPU remains last for observability even though inference kernels
        // are not implemented for CPU today.
        static ORDER: [BackendKind; 5] = [
            BackendKind::Mlx,
            BackendKind::CoreML,
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

    /// MLX-first default backend when capabilities allow it.
    ///
    /// On macOS builds with the multi-backend feature enabled we default to MLX
    /// for its flexibility and HKDF-seeded determinism. Falls back to CoreML
    /// when only coreml-backend is enabled, and Auto otherwise.
    pub fn default_inference_backend() -> BackendKind {
        if cfg!(all(target_os = "macos", feature = "multi-backend")) {
            BackendKind::Mlx
        } else if cfg!(all(target_os = "macos", feature = "coreml-backend")) {
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

/// Convert from the training-specific backend kind.
///
/// This enables seamless interop between the training subsystem (which uses
/// `TrainingBackendKind` from `adapteros-types`) and the runtime backend
/// selection (which uses `BackendKind`).
impl From<adapteros_types::training::TrainingBackendKind> for BackendKind {
    fn from(training: adapteros_types::training::TrainingBackendKind) -> Self {
        use adapteros_types::training::TrainingBackendKind;
        match training {
            TrainingBackendKind::Auto => BackendKind::Auto,
            TrainingBackendKind::CoreML => BackendKind::CoreML,
            TrainingBackendKind::Mlx => BackendKind::Mlx,
            TrainingBackendKind::Metal => BackendKind::Metal,
            TrainingBackendKind::Cpu => BackendKind::CPU,
        }
    }
}

/// Convert to the training-specific backend kind.
///
/// Note: `BackendKind::MlxBridge` maps to `TrainingBackendKind::Mlx` since
/// the training subsystem doesn't distinguish between MLX variants.
impl From<BackendKind> for adapteros_types::training::TrainingBackendKind {
    fn from(backend: BackendKind) -> Self {
        use adapteros_types::training::TrainingBackendKind;
        match backend {
            BackendKind::Auto => TrainingBackendKind::Auto,
            BackendKind::CoreML => TrainingBackendKind::CoreML,
            BackendKind::Mlx | BackendKind::MlxBridge => TrainingBackendKind::Mlx,
            BackendKind::Metal => TrainingBackendKind::Metal,
            BackendKind::CPU => TrainingBackendKind::Cpu,
        }
    }
}

impl FromStr for BackendKind {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        let normalized = s.trim().to_ascii_lowercase().replace(['-', '_'], "");
        let kind = match normalized.as_str() {
            "auto" | "autodev" | "default" => BackendKind::Auto,
            "coreml" | "ane" => BackendKind::CoreML,
            "mlx" | "mlxffi" => BackendKind::Mlx,
            "mlxbridge" | "subprocess" => BackendKind::MlxBridge,
            "metal" => BackendKind::Metal,
            "cpu" | "cpuonly" => BackendKind::CPU,
            other => {
                // Handle descriptive backend names like "MLX FFI (Apple Silicon)"
                if other.starts_with("mlx ffi") || other.starts_with("mlxffi") {
                    BackendKind::Mlx
                } else if other.starts_with("mlx bridge") || other.starts_with("mlxbridge") {
                    BackendKind::MlxBridge
                } else {
                    return Err(AosError::Config(format!(
                        "Invalid backend '{}'. Expected one of: {}",
                        s,
                        BackendKind::variants().join(", ")
                    )));
                }
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
    fn inference_priority_mlx_first() {
        let priority = BackendKind::inference_priority();
        assert!(priority.contains(&BackendKind::MlxBridge));

        // Verify order: MLX is primary, then CoreML, then MlxBridge
        let mlx_pos = priority.iter().position(|&b| b == BackendKind::Mlx);
        let coreml_pos = priority.iter().position(|&b| b == BackendKind::CoreML);
        let bridge_pos = priority.iter().position(|&b| b == BackendKind::MlxBridge);

        assert_eq!(mlx_pos.unwrap(), 0, "MLX should be first priority");
        assert!(mlx_pos.unwrap() < coreml_pos.unwrap());
        assert!(coreml_pos.unwrap() < bridge_pos.unwrap());
    }

    #[test]
    fn converts_from_training_backend_kind() {
        use adapteros_types::training::TrainingBackendKind;

        assert_eq!(
            BackendKind::from(TrainingBackendKind::Auto),
            BackendKind::Auto
        );
        assert_eq!(
            BackendKind::from(TrainingBackendKind::CoreML),
            BackendKind::CoreML
        );
        assert_eq!(
            BackendKind::from(TrainingBackendKind::Mlx),
            BackendKind::Mlx
        );
        assert_eq!(
            BackendKind::from(TrainingBackendKind::Metal),
            BackendKind::Metal
        );
        assert_eq!(
            BackendKind::from(TrainingBackendKind::Cpu),
            BackendKind::CPU
        );
    }

    #[test]
    fn converts_to_training_backend_kind() {
        use adapteros_types::training::TrainingBackendKind;

        assert_eq!(
            TrainingBackendKind::from(BackendKind::Auto),
            TrainingBackendKind::Auto
        );
        assert_eq!(
            TrainingBackendKind::from(BackendKind::CoreML),
            TrainingBackendKind::CoreML
        );
        assert_eq!(
            TrainingBackendKind::from(BackendKind::Mlx),
            TrainingBackendKind::Mlx
        );
        // MlxBridge maps to Mlx in training context
        assert_eq!(
            TrainingBackendKind::from(BackendKind::MlxBridge),
            TrainingBackendKind::Mlx
        );
        assert_eq!(
            TrainingBackendKind::from(BackendKind::Metal),
            TrainingBackendKind::Metal
        );
        assert_eq!(
            TrainingBackendKind::from(BackendKind::CPU),
            TrainingBackendKind::Cpu
        );
    }
}
