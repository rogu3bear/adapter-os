use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Repository assurance tier used for backend defaults and promotion policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RepoTier {
    /// High-assurance repositories (strict CoreML).
    HighAssurance,
    /// Standard repositories (CoreML preferred).
    #[default]
    Normal,
    /// Experimental repositories (auto backend).
    Experimental,
}

impl RepoTier {
    /// Canonical lowercase string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            RepoTier::HighAssurance => "high_assurance",
            RepoTier::Normal => "normal",
            RepoTier::Experimental => "experimental",
        }
    }
}

impl FromStr for RepoTier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.to_ascii_lowercase().replace(['-', ' '], "_");
        match normalized.as_str() {
            "high_assurance" | "highassurance" | "assurance_high" => Ok(RepoTier::HighAssurance),
            "normal" | "standard" => Ok(RepoTier::Normal),
            "experimental" => Ok(RepoTier::Experimental),
            other => Err(format!(
                "invalid repo tier '{}', expected high_assurance|normal|experimental",
                other
            )),
        }
    }
}
