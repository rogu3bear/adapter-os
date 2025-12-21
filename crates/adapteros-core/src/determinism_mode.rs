use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Runtime determinism mode applied across control plane and workers.
///
/// Modes:
/// - `Strict`: Fail closed, require seeds, receipts, and deterministic backends.
/// - `BestEffort`: Attempt deterministic paths but allow limited fallback.
/// - `Relaxed`: Do not enforce determinism guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeterminismMode {
    Strict,
    BestEffort,
    Relaxed,
}

impl DeterminismMode {
    /// Canonical string representation (snake_case, no hyphens).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::BestEffort => "besteffort",
            Self::Relaxed => "relaxed",
        }
    }

    /// Whether strict enforcement should be applied.
    pub fn is_strict(self) -> bool {
        matches!(self, Self::Strict)
    }
}

impl fmt::Display for DeterminismMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for DeterminismMode {
    fn from(value: &str) -> Self {
        let normalized = value.to_ascii_lowercase().replace(['_', '-'], "");
        match normalized.as_str() {
            "strict" => DeterminismMode::Strict,
            "besteffort" => DeterminismMode::BestEffort,
            "relaxed" => DeterminismMode::Relaxed,
            // Fail safe to strict to avoid silent relaxation.
            _ => DeterminismMode::Strict,
        }
    }
}

impl FromStr for DeterminismMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.to_ascii_lowercase().replace(['_', '-'], "");
        match normalized.as_str() {
            "strict" => Ok(DeterminismMode::Strict),
            "besteffort" => Ok(DeterminismMode::BestEffort),
            "relaxed" => Ok(DeterminismMode::Relaxed),
            _ => Err(format!(
                "Invalid determinism mode: {} (expected strict, besteffort, relaxed)",
                s
            )),
        }
    }
}

impl Serialize for DeterminismMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DeterminismMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        DeterminismMode::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::DeterminismMode;
    use serde_json::json;
    use std::str::FromStr;

    #[test]
    fn parses_common_spellings() {
        assert_eq!(DeterminismMode::from("strict"), DeterminismMode::Strict);
        assert_eq!(
            DeterminismMode::from("best_effort"),
            DeterminismMode::BestEffort
        );
        assert_eq!(
            DeterminismMode::from("best-effort"),
            DeterminismMode::BestEffort
        );
        assert_eq!(DeterminismMode::from("relaxed"), DeterminismMode::Relaxed);
    }

    #[test]
    fn from_str_rejects_invalid() {
        let err = DeterminismMode::from_str("unknown").unwrap_err();
        assert!(err.contains("Invalid determinism mode"));
    }

    #[test]
    fn serde_round_trip() {
        let mode = DeterminismMode::Strict;
        let serialized = serde_json::to_value(mode).unwrap();
        assert_eq!(serialized, json!("strict"));

        let parsed: DeterminismMode = serde_json::from_value(json!("best-effort")).unwrap();
        assert_eq!(parsed, DeterminismMode::BestEffort);
    }
}
