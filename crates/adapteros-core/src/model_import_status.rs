use crate::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Status of a model import operation.
///
/// Tracks whether a model is currently being imported, available for use,
/// or failed during import. Values match existing SQLite strings exactly
/// so no migration is needed.
///
/// Follows the same derive/trait pattern as [`BackendKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ModelImportStatus {
    /// Model is currently being imported (transient state)
    Importing,
    /// Model is available for inference/training
    #[default]
    Available,
    /// Import failed — see `import_error` for details
    Failed,
}

impl ModelImportStatus {
    /// Canonical string matching existing SQLite values.
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelImportStatus::Importing => "importing",
            ModelImportStatus::Available => "available",
            ModelImportStatus::Failed => "failed",
        }
    }

    /// List of canonical variants for error reporting.
    pub fn variants() -> &'static [&'static str] {
        &["importing", "available", "failed"]
    }
}

impl fmt::Display for ModelImportStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ModelImportStatus {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "importing" => Ok(ModelImportStatus::Importing),
            "available" => Ok(ModelImportStatus::Available),
            "failed" => Ok(ModelImportStatus::Failed),
            other => Err(AosError::Config(format!(
                "Invalid model import status '{}'. Expected one of: {}",
                other,
                ModelImportStatus::variants().join(", ")
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_round_trips() {
        for status in [
            ModelImportStatus::Importing,
            ModelImportStatus::Available,
            ModelImportStatus::Failed,
        ] {
            let rendered = status.to_string();
            let parsed = ModelImportStatus::from_str(&rendered).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn default_is_available() {
        assert_eq!(ModelImportStatus::default(), ModelImportStatus::Available);
    }

    #[test]
    fn rejects_unknown_status() {
        let err = ModelImportStatus::from_str("pending").unwrap_err();
        assert!(err.to_string().contains("Expected one of:"));
    }

    #[test]
    fn matches_sqlite_strings() {
        assert_eq!(ModelImportStatus::Importing.as_str(), "importing");
        assert_eq!(ModelImportStatus::Available.as_str(), "available");
        assert_eq!(ModelImportStatus::Failed.as_str(), "failed");
    }
}
