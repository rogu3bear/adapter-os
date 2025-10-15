//! Training types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Training configuration request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingConfigRequest {
    pub rank: u32,
    pub alpha: u32,
    pub targets: Vec<String>,
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch_size: u32,
    pub warmup_steps: Option<u32>,
    pub max_seq_length: Option<u32>,
    pub gradient_accumulation_steps: Option<u32>,
}

/// Start training request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StartTrainingRequest {
    pub adapter_name: String,
    pub config: TrainingConfigRequest,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
}

/// Training job response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingJobResponse {
    pub id: String,
    pub adapter_name: String,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
    pub status: String,
    pub progress_pct: f32,
    pub current_epoch: u32,
    pub total_epochs: u32,
    pub current_loss: f32,
    pub learning_rate: f32,
    pub tokens_per_second: f32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
    pub estimated_completion: Option<String>,
}

/// Training template response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingTemplateResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub rank: u32,
    pub alpha: u32,
    pub targets: Vec<String>,
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch_size: u32,
}

/// Training metrics response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingMetricsResponse {
    pub loss: f32,
    pub tokens_per_second: f32,
    pub learning_rate: f32,
    pub current_epoch: u32,
    pub total_epochs: u32,
    pub progress_pct: f32,
}
