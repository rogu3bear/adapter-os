//! UI configuration types.

use serde::{Deserialize, Serialize};

/// UI profile for navigation and surface filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiProfile {
    Primary,
    Full,
    Hud,
}

impl UiProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            UiProfile::Primary => "primary",
            UiProfile::Full => "full",
            UiProfile::Hud => "hud",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            UiProfile::Primary => "Primary",
            UiProfile::Full => "Full",
            UiProfile::Hud => "HUD",
        }
    }

    pub fn parse(value: &str) -> Self {
        value.parse().unwrap_or(UiProfile::Primary)
    }
}

impl std::str::FromStr for UiProfile {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_lowercase().as_str() {
            "primary" => Ok(UiProfile::Primary),
            "full" => Ok(UiProfile::Full),
            "hud" => Ok(UiProfile::Hud),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UiProfile;

    #[test]
    fn parse_falls_back_to_primary_for_unknown_values() {
        assert_eq!(UiProfile::parse("hud"), UiProfile::Hud);
        assert_eq!(UiProfile::parse("unknown"), UiProfile::Primary);
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
