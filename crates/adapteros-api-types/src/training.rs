//! Training types

use adapteros_core::{TrainingConfig, TrainingJob, TrainingTemplate};
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
    // Optional dataset path for training examples (JSON as used by CLI)
    pub dataset_path: Option<String>,
    // Optional: build dataset directly from a directory
    // Absolute repository root and relative path under root
    pub directory_root: Option<String>,
    pub directory_path: Option<String>,
    // Optional tenant context for directory analysis and registration
    pub tenant_id: Option<String>,
    // Packaging/registration options
    pub adapters_root: Option<String>,
    pub package: Option<bool>,
    pub register: Option<bool>,
    pub adapter_id: Option<String>,
    pub tier: Option<i32>,
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
    // Artifact metadata (populated when packaging is enabled)
    pub artifact_path: Option<String>,
    pub adapter_id: Option<String>,
    pub weights_hash_b3: Option<String>,
}

impl From<TrainingJob> for TrainingJobResponse {
    fn from(job: TrainingJob) -> Self {
        Self {
            id: job.id,
            adapter_name: job.adapter_name,
            template_id: job.template_id,
            repo_id: job.repo_id,
            status: job.status.to_string(),
            progress_pct: job.progress_pct,
            current_epoch: job.current_epoch,
            total_epochs: job.total_epochs,
            current_loss: job.current_loss,
            learning_rate: job.learning_rate,
            tokens_per_second: job.tokens_per_second,
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
            error_message: job.error_message,
            estimated_completion: None, // Calculate if needed
            artifact_path: job.artifact_path,
            adapter_id: job.adapter_id,
            weights_hash_b3: job.weights_hash_b3,
        }
    }
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

impl From<TrainingConfigRequest> for TrainingConfig {
    fn from(req: TrainingConfigRequest) -> Self {
        Self {
            rank: req.rank,
            alpha: req.alpha,
            targets: req.targets,
            epochs: req.epochs,
            learning_rate: req.learning_rate,
            batch_size: req.batch_size,
            warmup_steps: req.warmup_steps,
            max_seq_length: req.max_seq_length,
            gradient_accumulation_steps: req.gradient_accumulation_steps,
        }
    }
}

impl From<TrainingTemplate> for TrainingTemplateResponse {
    fn from(template: TrainingTemplate) -> Self {
        Self {
            id: template.id,
            name: template.name,
            description: template.description,
            category: template.category,
            rank: template.config.rank,
            alpha: template.config.alpha,
            targets: template.config.targets,
            epochs: template.config.epochs,
            learning_rate: template.config.learning_rate,
            batch_size: template.config.batch_size,
        }
    }
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

// ===== Dataset Types =====

/// Upload dataset request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadDatasetRequest {
    pub name: String,
    pub description: Option<String>,
    pub format: String, // 'patches', 'jsonl', 'txt', 'custom'
}

/// Upload dataset response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadDatasetResponse {
    pub dataset_id: String,
    pub name: String,
    pub file_count: i32,
    pub total_size_bytes: i64,
    pub hash_b3: String,
    pub validation_status: String,
}

/// Dataset response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatasetResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub file_count: i32,
    pub total_size_bytes: i64,
    pub format: String,
    pub hash_b3: String,
    pub validation_status: String,
    pub validation_errors: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub statistics: Option<DatasetStatisticsResponse>,
}

/// Dataset statistics response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatasetStatisticsResponse {
    pub num_examples: i32,
    pub avg_input_length: f64,
    pub avg_target_length: f64,
    pub language_distribution: Option<serde_json::Value>,
    pub file_type_distribution: Option<serde_json::Value>,
    pub total_tokens: i64,
    pub computed_at: String,
}

/// Dataset file response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatasetFileResponse {
    pub id: String,
    pub dataset_id: String,
    pub file_name: String,
    pub size_bytes: i64,
    pub hash_b3: String,
    pub mime_type: Option<String>,
    pub created_at: String,
}

/// Dataset validation request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateDatasetRequest {
    pub dataset_id: String,
}

/// Dataset validation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateDatasetResponse {
    pub dataset_id: String,
    pub status: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub statistics: Option<DatasetStatisticsResponse>,
}

/// Training event for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingEvent {
    pub event_type: String, // "job_started", "job_completed", "job_failed", "epoch_completed", "progress_updated"
    pub job_id: String,
    pub timestamp: String,
    pub payload: serde_json::Value,
}
