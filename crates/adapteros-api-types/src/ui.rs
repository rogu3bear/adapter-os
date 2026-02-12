//! UI configuration types.

use serde::{Deserialize, Serialize};

/// UI profile for navigation and surface filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiProfile {
    Primary,
    Full,
}

impl UiProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            UiProfile::Primary => "primary",
            UiProfile::Full => "full",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            UiProfile::Primary => "Primary",
            UiProfile::Full => "Full",
        }
    }

    pub fn parse(value: &str) -> Self {
        value.parse().unwrap_or(UiProfile::Full)
    }
}

impl std::str::FromStr for UiProfile {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_lowercase().as_str() {
            "primary" => Ok(UiProfile::Primary),
            "full" => Ok(UiProfile::Full),
            _ => Err(()),
        }
    }
}

/// Public UI configuration for runtime clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct UiConfigResponse {
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    pub ui_profile: UiProfile,
    #[serde(default = "default_docs_url")]
    pub docs_url: String,
}

fn default_docs_url() -> String {
    "/docs".to_string()
}
