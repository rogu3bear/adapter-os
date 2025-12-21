use adapteros_api_types::model_status::ModelLoadStatus;
use serde::{Deserialize, Serialize};

/// Unified lifecycle for base models and adapters (per tenant).
///
/// Canonical ordering:
/// unloaded → loading → loaded → active → unloading → unloaded | error
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleState {
    Unloaded,
    Loading,
    Loaded,
    Active,
    Unloading,
    Error,
}

impl LifecycleState {
    /// Convert lifecycle to canonical API status for persistence/telemetry.
    pub fn to_model_status(self) -> ModelLoadStatus {
        match self {
            LifecycleState::Unloaded => ModelLoadStatus::NoModel,
            LifecycleState::Loading => ModelLoadStatus::Loading,
            LifecycleState::Loaded | LifecycleState::Active => ModelLoadStatus::Ready,
            LifecycleState::Unloading => ModelLoadStatus::Unloading,
            LifecycleState::Error => ModelLoadStatus::Error,
        }
    }

    /// Whether the state is serving-capable.
    pub fn is_active(self) -> bool {
        matches!(self, LifecycleState::Loaded | LifecycleState::Active)
    }
}

impl From<ModelLoadStatus> for LifecycleState {
    fn from(status: ModelLoadStatus) -> Self {
        match status {
            ModelLoadStatus::NoModel => LifecycleState::Unloaded,
            ModelLoadStatus::Loading | ModelLoadStatus::Checking => LifecycleState::Loading,
            ModelLoadStatus::Ready => LifecycleState::Active,
            ModelLoadStatus::Unloading => LifecycleState::Unloading,
            ModelLoadStatus::Error => LifecycleState::Error,
        }
    }
}
