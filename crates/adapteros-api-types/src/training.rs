//! Training types

use adapteros_types::training::{TrainingConfig, TrainingJob, TrainingTemplate};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::schema_version;

/// Dataset validation status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DatasetValidationStatus {
    Draft,
    Validating,
    Valid,
    Invalid,
    Failed,
}

impl DatasetValidationStatus {
    pub fn from_db_string(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "validating" => Self::Validating,
            "valid" => Self::Valid,
            "invalid" => Self::Invalid,
            "failed" => Self::Failed,
            _ => Self::Draft,
        }
    }
}

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
    /// Base model ID for provenance tracking
    pub base_model_id: Option<String>,
    /// Document collection ID for provenance tracking
    pub collection_id: Option<String>,

    // Category & metadata
    /// Adapter category: code, framework, codebase, docs, domain
    pub category: Option<String>,
    /// Human-readable description
    pub description: Option<String>,

    // Category-specific configuration
    /// Programming language (for code adapters)
    pub language: Option<String>,
    /// Symbol targets (for code adapters)
    pub symbol_targets: Option<Vec<String>>,
    /// Framework ID (for framework adapters)
    pub framework_id: Option<String>,
    /// Framework version (for framework adapters)
    pub framework_version: Option<String>,
    /// API patterns to focus on (for framework adapters)
    pub api_patterns: Option<Vec<String>>,
    /// Repository scope (for codebase adapters)
    pub repo_scope: Option<String>,
    /// File patterns to include (for codebase adapters)
    pub file_patterns: Option<Vec<String>>,
    /// File patterns to exclude (for codebase adapters)
    pub exclude_patterns: Option<Vec<String>>,

    // Post-training actions
    /// Actions to perform after training completes
    pub post_actions: Option<PostActionsRequest>,
}

/// Post-training actions configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct PostActionsRequest {
    /// Package adapter after training (default: true)
    pub package: Option<bool>,
    /// Register adapter in registry after packaging (default: true)
    pub register: Option<bool>,
    /// Create a new stack with the adapter after registration (default: true).
    /// Note: The new stack will NOT be set as the tenant's default stack.
    pub create_stack: Option<bool>,
    /// Tier to assign: persistent, warm, ephemeral (default: warm)
    pub tier: Option<String>,
    /// Custom adapters root directory
    pub adapters_root: Option<String>,
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
    /// Base model ID used for training
    pub base_model_id: Option<String>,
    /// Document collection ID used for training
    pub collection_id: Option<String>,
    /// Build ID for CI/CD traceability
    pub build_id: Option<String>,
    /// BLAKE3 hash of training config for reproducibility
    pub config_hash_b3: Option<String>,
    /// Adapter ID after packaging (populated on completion)
    pub adapter_id: Option<String>,
    /// BLAKE3 hash of adapter weights (for verification)
    pub weights_hash_b3: Option<String>,

    // Category metadata
    /// Adapter category
    pub category: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Programming language
    pub language: Option<String>,
    /// Framework ID
    pub framework_id: Option<String>,
    /// Framework version
    pub framework_version: Option<String>,

    // Training progress
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

    // Backend and determinism
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_gpu: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_gpu_memory_mb: Option<u64>,

    // Extended metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples_processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_examples_per_sec: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_utilization_pct: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_gpu_memory_mb: Option<f32>,

    // Packaging summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aos_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_rank: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_base_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_per_layer_hashes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_status: Option<String>,
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
            base_model_id: job.base_model_id,
            collection_id: job.collection_id,
            build_id: job.build_id,
            config_hash_b3: job.config_hash_b3,
            adapter_id: job.adapter_id,
            weights_hash_b3: job.weights_hash_b3,
            // Category metadata - will be populated when TrainingJob is extended
            category: job.category,
            description: job.description,
            language: job.language,
            framework_id: job.framework_id,
            framework_version: job.framework_version,
            // Training progress
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
            // Backend/determinism
            backend: job.backend,
            backend_reason: job.backend_reason,
            determinism_mode: job.determinism_mode,
            training_seed: job.training_seed,
            require_gpu: job.require_gpu,
            max_gpu_memory_mb: job.max_gpu_memory_mb,
            // Extended metrics
            examples_processed: job.examples_processed,
            tokens_processed: job.tokens_processed,
            training_time_ms: job.training_time_ms,
            throughput_examples_per_sec: job.throughput_examples_per_sec,
            gpu_utilization_pct: job.gpu_utilization_pct,
            peak_gpu_memory_mb: job.peak_gpu_memory_mb,
            // Packaging summary
            aos_path: job.aos_path,
            package_hash_b3: job.package_hash_b3,
            manifest_rank: job.manifest_rank,
            manifest_base_model: job.manifest_base_model,
            manifest_per_layer_hashes: job.manifest_per_layer_hashes,
            signature_status: job.signature_status,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples_processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_examples_per_sec: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_utilization_pct: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_gpu_memory_mb: Option<f32>,
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
    pub validation_status: DatasetValidationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_errors: Option<Vec<String>>,
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
    pub validation_status: DatasetValidationStatus,
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

// ============================================================================
// Chat Bootstrap Types
// ============================================================================

/// Response for GET /v1/training/jobs/{id}/chat_bootstrap
///
/// Returns the "recipe" for starting a chat from a completed training job.
/// Used by any UI flow to quickly get the payload needed to create a chat session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChatBootstrapResponse {
    /// Whether the training job is ready for chat (completed with stack)
    pub ready: bool,
    /// Stack ID created from training (if ready)
    pub stack_id: Option<String>,
    /// Adapter IDs in the stack
    pub adapter_ids: Vec<String>,
    /// Base model ID used for training
    pub base_model: Option<String>,
    /// RAG collection ID if training involved RAG
    pub collection_id: Option<String>,
    /// Suggested title for the chat session
    pub suggested_chat_title: String,

    // Provenance fields for Bundle E readiness
    /// Training job ID (always present, echoed from path)
    pub training_job_id: String,
    /// Training job status ("pending"|"running"|"completed"|"failed"|"cancelled")
    pub status: String,
    /// Primary adapter ID from training job (set after training completes)
    pub adapter_id: Option<String>,
    /// Training dataset ID
    pub dataset_id: Option<String>,
}

/// Request for POST /v1/chats/from_training_job
///
/// Creates a chat session bound to a training job's stack in one call.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateChatFromJobRequest {
    /// Training job ID to create chat from
    pub training_job_id: String,
    /// Optional override for chat session name
    pub name: Option<String>,
    /// Optional metadata JSON for the chat session
    pub metadata_json: Option<String>,
}

/// Response for POST /v1/chats/from_training_job
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateChatFromJobResponse {
    /// Created chat session ID
    pub session_id: String,
    /// Stack ID the session is bound to
    pub stack_id: String,
    /// Session name (either provided or generated)
    pub name: String,
    /// Creation timestamp
    pub created_at: String,

    // Provenance fields for Bundle E readiness
    /// Training job ID (echoed from request for confirmation)
    pub training_job_id: String,
    /// Primary adapter ID from the training job
    pub adapter_id: Option<String>,
    /// Training dataset ID
    pub dataset_id: Option<String>,
    /// RAG collection ID if linked
    pub collection_id: Option<String>,
}
