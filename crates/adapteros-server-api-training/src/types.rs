//! Local types for training handlers
//!
//! Types that are specific to the training spoke crate.
//! Shared types are re-exported from adapteros-api-types.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use adapteros_types::training::{LoraTier, TrainingBackendKind, TrainingBackendPolicy};

/// Query parameters for backend readiness
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BackendReadinessQuery {
    pub preferred_backend: Option<TrainingBackendKind>,
    pub backend_policy: Option<TrainingBackendPolicy>,
    pub coreml_fallback: Option<TrainingBackendKind>,
    pub require_gpu: Option<bool>,
}

/// Internal backend plan result
#[derive(Debug, Default)]
pub struct BackendPlan {
    pub resolved_backend: TrainingBackendKind,
    pub fallback_backend: Option<TrainingBackendKind>,
    pub fallback_reason: Option<String>,
    pub ready: bool,
    pub warnings: Vec<String>,
}

/// Query parameters for promoting a version
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "snake_case")]
pub struct PromoteVersionQuery {
    /// Branch to promote on; defaults to the version's branch
    pub branch: Option<String>,
}

/// Query parameters for training metrics
#[derive(Debug, Deserialize, IntoParams)]
pub struct TrainingMetricsQuery {
    pub metric_name: Option<String>,
    pub limit: Option<i64>,
}

/// Training progress event for SSE streaming
#[derive(Debug, Clone, Serialize)]
pub struct TrainingProgressEvent {
    pub epoch: u32,
    pub loss: f32,
    pub tokens_processed: Option<i64>,
    pub status: String,
    pub progress_pct: f32,
}

/// Request to update training job priority
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTrainingPriorityRequest {
    /// Priority value (0-100, higher = more urgent)
    pub priority: i32,
}

/// Response after updating training job priority
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTrainingPriorityResponse {
    pub job_id: String,
    pub priority: i32,
    pub message: String,
}

/// Batch training job status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchTrainingJobStatus {
    pub job_id: String,
    pub status: String,
    pub progress_pct: f32,
    pub current_epoch: u32,
    pub total_epochs: u32,
    pub current_loss: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// Batch status request body
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchStatusRequest {
    pub job_ids: Vec<String>,
}

/// Batch status response body
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchStatusResponse {
    pub schema_version: String,
    pub jobs: Vec<BatchTrainingJobStatus>,
}

/// Guardrail error for training validation
#[derive(Debug, PartialEq, Eq)]
pub struct GuardrailError {
    pub code: &'static str,
    pub message: String,
}

/// Parse LoRA tier from string
pub fn parse_lora_tier(value: Option<&str>) -> Option<LoraTier> {
    match value {
        Some("micro") => Some(LoraTier::Micro),
        Some("standard") => Some(LoraTier::Standard),
        Some("max") => Some(LoraTier::Max),
        _ => None,
    }
}

// Re-export canonical trust state normalization from the shared types crate.
pub use adapteros_api_types::training::canonical_trust_state;
