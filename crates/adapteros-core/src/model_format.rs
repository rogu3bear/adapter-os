use crate::backend::BackendKind;
use crate::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::{fmt, str::FromStr};

/// Canonical model storage format.
///
/// Determines how model weights are stored on disk and which inference backend
/// is appropriate. Follows the same derive/trait pattern as [`BackendKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ModelFormat {
    /// SafeTensors (.safetensors) — default for MLX models
    SafeTensors,
    /// CoreML ML Package (.mlpackage)
    MlPackage,
    /// GGUF quantized format (.gguf) — typically Metal backend
    Gguf,
}

impl ModelFormat {
    /// Canonical string for storage/config surface.
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelFormat::SafeTensors => "safetensors",
            ModelFormat::MlPackage => "mlpackage",
            ModelFormat::Gguf => "gguf",
        }
    }

    /// Default inference backend for this model format.
    pub fn default_backend(&self) -> BackendKind {
        match self {
            ModelFormat::SafeTensors => BackendKind::Mlx,
            ModelFormat::MlPackage => BackendKind::CoreML,
            ModelFormat::Gguf => BackendKind::Metal,
        }
    }

    /// List of canonical variants for error reporting.
    pub fn variants() -> &'static [&'static str] {
        &["safetensors", "mlpackage", "gguf"]
    }

    /// Detect model format from directory contents by scanning file extensions.
    ///
    /// Scans the given directory for model files and returns the detected format:
    /// - `.mlpackage` → `MlPackage` (takes priority, breaks immediately)
    /// - `.gguf` → `Gguf`
    /// - Default → `SafeTensors`
    pub fn detect_from_dir(path: &Path) -> ModelFormat {
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(_) => return ModelFormat::SafeTensors,
        };

        let mut format = ModelFormat::SafeTensors;

        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("mlpackage") {
                    return ModelFormat::MlPackage;
                }
                if ext.eq_ignore_ascii_case("gguf") {
                    format = ModelFormat::Gguf;
                }
            }
        }

        format
    }
}

/// Discover model directories under a root path.
///
/// If `root` itself contains `config.json`, returns it as a single-element vec.
/// Otherwise, scans immediate subdirectories for those containing `config.json`.
/// Returns an empty vec if `root` is not a directory or contains no model dirs.
pub fn discover_model_dirs(root: &Path) -> Vec<PathBuf> {
    if root.join("config.json").exists() {
        return vec![root.to_path_buf()];
    }

    if !root.is_dir() {
        return vec![];
    }

    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return vec![],
    };

    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && p.join("config.json").exists())
        .collect()
}

impl fmt::Display for ModelFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ModelFormat {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        let normalized = s.trim().to_ascii_lowercase().replace(['-', '_'], "");
        match normalized.as_str() {
            "safetensors" | "st" => Ok(ModelFormat::SafeTensors),
            "mlpackage" | "coreml" => Ok(ModelFormat::MlPackage),
            "gguf" => Ok(ModelFormat::Gguf),
            _ => Err(AosError::Config(format!(
                "Invalid model format '{}'. Expected one of: {}",
                s,
                ModelFormat::variants().join(", ")
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_round_trips() {
        for format in [
            ModelFormat::SafeTensors,
            ModelFormat::MlPackage,
            ModelFormat::Gguf,
        ] {
            let rendered = format.to_string();
            let parsed = ModelFormat::from_str(&rendered).unwrap();
            assert_eq!(parsed, format);
        }
    }

    #[test]
    fn default_backends() {
        assert_eq!(ModelFormat::SafeTensors.default_backend(), BackendKind::Mlx);
        assert_eq!(
            ModelFormat::MlPackage.default_backend(),
            BackendKind::CoreML
        );
        assert_eq!(ModelFormat::Gguf.default_backend(), BackendKind::Metal);
    }

    #[test]
    fn parses_aliases() {
        assert_eq!(
            ModelFormat::from_str("st").unwrap(),
            ModelFormat::SafeTensors
        );
        assert_eq!(
            ModelFormat::from_str("coreml").unwrap(),
            ModelFormat::MlPackage
        );
        assert_eq!(ModelFormat::from_str("GGUF").unwrap(), ModelFormat::Gguf);
    }

    #[test]
    fn rejects_unknown_format() {
        let err = ModelFormat::from_str("unknown").unwrap_err();
        assert!(err.to_string().contains("Expected one of:"));
    }

    #[test]
    fn detect_defaults_to_safetensors() {
        // Non-existent path defaults to SafeTensors
        let format = ModelFormat::detect_from_dir(Path::new("/nonexistent/path"));
        assert_eq!(format, ModelFormat::SafeTensors);
    }

    #[test]
    fn discover_empty_returns_empty() {
        let dirs = discover_model_dirs(Path::new("/nonexistent/path"));
        assert!(dirs.is_empty());
    }
}
