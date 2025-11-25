//! Training types

use adapteros_types::training::{TrainingConfig, TrainingJob, TrainingTemplate};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::schema_version;

// ===== Request/Response Types =====

/// Training configuration request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
pub struct StartTrainingRequest {
    pub adapter_name: String,
    pub config: TrainingConfigRequest,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
    pub dataset_id: Option<String>,
}

/// Training job response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TrainingJobResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub adapter_name: String,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
    pub dataset_id: Option<String>,
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

impl From<TrainingJob> for TrainingJobResponse {
    fn from(job: TrainingJob) -> Self {
        Self {
            schema_version: schema_version(),
            id: job.id,
            adapter_name: job.adapter_name,
            template_id: job.template_id,
            repo_id: job.repo_id,
            dataset_id: job.dataset_id,
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
        }
    }
}

/// Training template response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TrainingTemplateResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
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
            weight_group_config: None,
            lr_schedule: Some("cosine".to_string()),
            final_lr: Some(req.learning_rate * 0.1),
            early_stopping: Some(false),
            patience: Some(5),
            min_delta: Some(0.001),
            checkpoint_frequency: Some(5),
            max_checkpoints: Some(3),
        }
    }
}

impl From<TrainingTemplate> for TrainingTemplateResponse {
    fn from(template: TrainingTemplate) -> Self {
        Self {
            schema_version: schema_version(),
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
#[serde(rename_all = "snake_case")]
pub struct TrainingMetricsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
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
#[serde(rename_all = "snake_case")]
pub struct UploadDatasetRequest {
    pub name: String,
    pub description: Option<String>,
    pub format: String, // 'patches', 'jsonl', 'txt', 'custom'
}

/// Upload dataset response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UploadDatasetResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
    pub name: String,
    pub description: Option<String>,
    pub file_count: i32,
    pub total_size_bytes: i64,
    pub format: String,
    pub hash: String,
    pub created_at: String,
}

/// Dataset response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
    pub name: String,
    pub description: Option<String>,
    pub file_count: i32,
    pub total_size_bytes: i64,
    pub format: String,
    pub hash: String,
    pub storage_path: String,
    pub validation_status: String,
    pub validation_errors: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Dataset statistics response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetStatisticsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
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
#[serde(rename_all = "snake_case")]
pub struct DatasetFileResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub file_id: String,
    pub file_name: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub hash: String,
    pub mime_type: Option<String>,
    pub created_at: String,
}

/// Dataset validation request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ValidateDatasetRequest {
    pub check_format: Option<bool>,
}

/// Dataset validation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ValidateDatasetResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
    pub is_valid: bool,
    pub validation_status: String,
    pub errors: Option<Vec<String>>,
    pub validated_at: String,
}

// ===== Training Job List Types =====

/// Training job list query parameters
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams, Default)]
pub struct TrainingListParams {
    /// Filter by status (pending, running, completed, failed, cancelled)
    pub status: Option<String>,
    /// Page number (1-indexed)
    pub page: Option<u32>,
    /// Number of items per page (default: 20, max: 100)
    pub page_size: Option<u32>,
    /// Filter by adapter name
    pub adapter_name: Option<String>,
    /// Filter by template ID
    pub template_id: Option<String>,
    /// Filter by dataset ID
    pub dataset_id: Option<String>,
}

/// Training job list response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TrainingJobListResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub jobs: Vec<TrainingJobResponse>,
    pub total: usize,
    pub page: u32,
    pub page_size: u32,
}

impl Default for TrainingJobListResponse {
    fn default() -> Self {
        Self {
            schema_version: schema_version(),
            jobs: vec![],
            total: 0,
            page: 1,
            page_size: 20,
        }
    }
}
