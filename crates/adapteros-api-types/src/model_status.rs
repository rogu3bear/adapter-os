use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Canonical base model load status used across API, worker, and UI.
///
/// Allowed transitions:
/// - no-model -> loading (load requested)
/// - loading -> ready (load success)
/// - loading -> error (load failure)
/// - loading -> no-model (cancel/timeout)
/// - ready -> unloading (unload/eviction requested)
/// - unloading -> no-model (unload success)
/// - unloading -> error (unload failure)
/// - error -> loading (retry/ensure)
/// - error -> no-model (reset/cleanup)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ModelLoadStatus {
    NoModel,
    Loading,
    Ready,
    Unloading,
    Error,
    Checking,
}

impl ModelLoadStatus {
    /// Stable string rendering used in persistence and responses.
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelLoadStatus::NoModel => "no-model",
            ModelLoadStatus::Loading => "loading",
            ModelLoadStatus::Ready => "ready",
            ModelLoadStatus::Unloading => "unloading",
            ModelLoadStatus::Error => "error",
            ModelLoadStatus::Checking => "checking",
        }
    }

    /// Normalize arbitrary/legacy status strings into the canonical enum.
    ///
    /// Legacy compatibility:
    /// - "loaded" -> Ready
    /// - "unloaded" -> NoModel
    /// - "ready" -> Ready
    pub fn parse_status(status: &str) -> Self {
        match status {
            "ready" | "loaded" => ModelLoadStatus::Ready,
            "loading" => ModelLoadStatus::Loading,
            "unloading" => ModelLoadStatus::Unloading,
            "error" => ModelLoadStatus::Error,
            "checking" => ModelLoadStatus::Checking,
            "unloaded" | "no-model" | "none" => ModelLoadStatus::NoModel,
            _ => ModelLoadStatus::NoModel,
        }
    }

    /// Whether the model is eligible for routing.
    pub fn is_ready(&self) -> bool {
        matches!(self, ModelLoadStatus::Ready)
    }

    /// Whether the model is in a transitional state.
    pub fn is_transitioning(&self) -> bool {
        matches!(
            self,
            ModelLoadStatus::Loading | ModelLoadStatus::Unloading | ModelLoadStatus::Checking
        )
    }
}
