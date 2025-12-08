//! Training job and configuration types
//!
//! This module provides canonical training types that consolidate definitions
//! from adapteros-core, adapteros-orchestrator, and adapteros-api-types into
//! a single source of truth.
//!
//! # Type Hierarchy
//!
//! - `TrainingJobStatus` - State machine for training job lifecycle
//! - `TrainingJob` - Complete training job information with metadata
//! - `TrainingConfig` - Training hyperparameters and configuration
//! - `TrainingTemplate` - Reusable training templates
//!
//! # Canonical Consolidation
//!
//! This module consolidates 3 previous definitions:
//! 1. `adapteros-core/src/training.rs` - Base definition with artifact metadata
//! 2. `adapteros-orchestrator/src/training.rs` - Orchestrator-specific variant
//! 3. Used by `adapteros-api-types/src/training.rs` for API responses

use serde::{Deserialize, Serialize};

/// Training job state machine
///
/// Represents the complete lifecycle of a training job from creation to completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrainingJobStatus {
    /// Job created but not yet started
    Pending,
    /// Job currently executing training
    Running,
    /// Job completed successfully
    Completed,
    /// Job failed during execution
    Failed,
    /// Job cancelled by user or system
    Cancelled,
}

impl TrainingJobStatus {
    /// Whether this status indicates the job is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TrainingJobStatus::Completed | TrainingJobStatus::Failed | TrainingJobStatus::Cancelled
        )
    }

    /// Whether this status allows state transitions
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            TrainingJobStatus::Pending | TrainingJobStatus::Running
        )
    }
}

impl std::fmt::Display for TrainingJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrainingJobStatus::Pending => write!(f, "pending"),
            TrainingJobStatus::Running => write!(f, "running"),
            TrainingJobStatus::Completed => write!(f, "completed"),
            TrainingJobStatus::Failed => write!(f, "failed"),
            TrainingJobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Canonical training job information
///
/// Complete training job record including runtime metrics, configuration,
/// and artifact metadata. This is the canonical definition consolidating
/// previous definitions from adapteros-core and adapteros-orchestrator.
///
/// All timestamps are in RFC3339 format (ISO 8601).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingJob {
    /// Unique training job identifier
    #[serde(rename = "id")]
    pub id: String,

    /// Name of the adapter being trained
    #[serde(rename = "adapter_name")]
    pub adapter_name: String,

    /// Optional reference to training template used
    #[serde(rename = "template_id")]
    pub template_id: Option<String>,

    /// Optional reference to source repository
    #[serde(rename = "repo_id")]
    pub repo_id: Option<String>,

    /// Optional reference to training dataset
    #[serde(rename = "dataset_id")]
    pub dataset_id: Option<String>,

    /// Optional reference to base model used for training
    #[serde(rename = "base_model_id", skip_serializing_if = "Option::is_none")]
    pub base_model_id: Option<String>,

    /// Optional reference to document collection used
    #[serde(rename = "collection_id", skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,

    /// Build ID for CI/CD traceability (git commit, version, etc.)
    #[serde(rename = "build_id", skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,

    /// Immutable snapshot of source documents used for training (JSON)
    #[serde(
        rename = "source_documents_json",
        skip_serializing_if = "Option::is_none"
    )]
    pub source_documents_json: Option<String>,

    /// BLAKE3 hash of training config for reproducibility
    #[serde(rename = "config_hash_b3", skip_serializing_if = "Option::is_none")]
    pub config_hash_b3: Option<String>,

    /// Current job status in lifecycle
    #[serde(rename = "status")]
    pub status: TrainingJobStatus,

    /// Training progress percentage [0.0, 100.0]
    #[serde(rename = "progress_pct")]
    pub progress_pct: f32,

    /// Current epoch in training (0-indexed)
    #[serde(rename = "current_epoch")]
    pub current_epoch: u32,

    /// Total epochs configured for training
    #[serde(rename = "total_epochs")]
    pub total_epochs: u32,

    /// Current loss value (lower is better)
    #[serde(rename = "current_loss")]
    pub current_loss: f32,

    /// Learning rate for current training session
    #[serde(rename = "learning_rate")]
    pub learning_rate: f32,

    /// Throughput metric: tokens processed per second
    #[serde(rename = "tokens_per_second")]
    pub tokens_per_second: f32,

    /// Job creation timestamp (RFC3339)
    #[serde(rename = "created_at")]
    pub created_at: String,

    /// Job start timestamp if running (RFC3339)
    #[serde(rename = "started_at", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,

    /// Job completion timestamp if terminal (RFC3339)
    #[serde(rename = "completed_at", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,

    /// Error message if failed
    #[serde(rename = "error_message", skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// Training hyperparameters
    #[serde(rename = "config")]
    pub config: TrainingConfig,

    /// Path to generated adapter artifact (.aos file)
    #[serde(rename = "artifact_path", skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,

    /// Adapter ID after packaging (populated on completion)
    #[serde(rename = "adapter_id", skip_serializing_if = "Option::is_none")]
    pub adapter_id: Option<String>,

    /// BLAKE3 hash of adapter weights (for verification)
    #[serde(rename = "weights_hash_b3", skip_serializing_if = "Option::is_none")]
    pub weights_hash_b3: Option<String>,

    /// Tenant ID that owns this training job
    #[serde(rename = "tenant_id", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,

    /// Stack ID created for this adapter (populated on completion)
    #[serde(rename = "stack_id", skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,

    /// User who initiated this training job (for audit logging)
    #[serde(rename = "initiated_by", skip_serializing_if = "Option::is_none")]
    pub initiated_by: Option<String>,

    /// Role of user who initiated this training job (for audit logging)
    #[serde(rename = "initiated_by_role", skip_serializing_if = "Option::is_none")]
    pub initiated_by_role: Option<String>,

    // Category metadata for adapter training
    /// Adapter category: code, framework, codebase, docs, domain
    #[serde(rename = "category", skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Human-readable description of the adapter
    #[serde(rename = "description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Programming language (for code adapters)
    #[serde(rename = "language", skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Symbol targets (for code adapters) - JSON array
    #[serde(
        rename = "symbol_targets_json",
        skip_serializing_if = "Option::is_none"
    )]
    pub symbol_targets_json: Option<String>,

    /// Framework ID (for framework adapters)
    #[serde(rename = "framework_id", skip_serializing_if = "Option::is_none")]
    pub framework_id: Option<String>,

    /// Framework version (for framework adapters)
    #[serde(rename = "framework_version", skip_serializing_if = "Option::is_none")]
    pub framework_version: Option<String>,

    /// API patterns to focus on (for framework adapters) - JSON array
    #[serde(rename = "api_patterns_json", skip_serializing_if = "Option::is_none")]
    pub api_patterns_json: Option<String>,

    /// Repository scope (for codebase adapters)
    #[serde(rename = "repo_scope", skip_serializing_if = "Option::is_none")]
    pub repo_scope: Option<String>,

    /// File patterns to include (for codebase adapters) - JSON array
    #[serde(rename = "file_patterns_json", skip_serializing_if = "Option::is_none")]
    pub file_patterns_json: Option<String>,

    /// File patterns to exclude (for codebase adapters) - JSON array
    #[serde(
        rename = "exclude_patterns_json",
        skip_serializing_if = "Option::is_none"
    )]
    pub exclude_patterns_json: Option<String>,

    /// Post-training actions configuration - JSON
    #[serde(rename = "post_actions_json", skip_serializing_if = "Option::is_none")]
    pub post_actions_json: Option<String>,

    /// Whether failed job can be retried
    #[serde(rename = "retryable", skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,

    /// If this is a retry, the original job ID
    #[serde(rename = "retry_of_job_id", skip_serializing_if = "Option::is_none")]
    pub retry_of_job_id: Option<String>,

    // Backend and determinism (new)
    /// Backend selected by trainer (CoreML (ANE), Metal, MLX, CPU)
    #[serde(rename = "backend", skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Reason/notes for backend selection or fallback
    #[serde(rename = "backend_reason", skip_serializing_if = "Option::is_none")]
    pub backend_reason: Option<String>,
    /// Determinism mode (e.g., hkdf_seeded, nondet_fallback)
    #[serde(rename = "determinism_mode", skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    /// 64-bit deterministic training seed (audit)
    #[serde(rename = "training_seed", skip_serializing_if = "Option::is_none")]
    pub training_seed: Option<u64>,
    /// Whether GPU was required for this run
    #[serde(rename = "require_gpu", skip_serializing_if = "Option::is_none")]
    pub require_gpu: Option<bool>,
    /// Max GPU memory budget in MB (0 = unlimited)
    #[serde(rename = "max_gpu_memory_mb", skip_serializing_if = "Option::is_none")]
    pub max_gpu_memory_mb: Option<u64>,

    // Extended metrics (new)
    /// Total examples processed
    #[serde(rename = "examples_processed", skip_serializing_if = "Option::is_none")]
    pub examples_processed: Option<u64>,
    /// Total tokens processed (if available)
    #[serde(rename = "tokens_processed", skip_serializing_if = "Option::is_none")]
    pub tokens_processed: Option<u64>,
    /// Wall-clock training time (ms)
    #[serde(rename = "training_time_ms", skip_serializing_if = "Option::is_none")]
    pub training_time_ms: Option<u64>,
    /// Examples/second throughput
    #[serde(
        rename = "throughput_examples_per_sec",
        skip_serializing_if = "Option::is_none"
    )]
    pub throughput_examples_per_sec: Option<f32>,
    /// Average GPU utilization percentage
    #[serde(rename = "gpu_utilization_pct", skip_serializing_if = "Option::is_none")]
    pub gpu_utilization_pct: Option<f32>,
    /// Peak GPU memory usage (MB)
    #[serde(rename = "peak_gpu_memory_mb", skip_serializing_if = "Option::is_none")]
    pub peak_gpu_memory_mb: Option<f32>,

    // Packaging summary (new)
    /// Path to .aos archive (if packaged)
    #[serde(rename = "aos_path", skip_serializing_if = "Option::is_none")]
    pub aos_path: Option<String>,
    /// Hash of packaged archive (BLAKE3)
    #[serde(rename = "package_hash_b3", skip_serializing_if = "Option::is_none")]
    pub package_hash_b3: Option<String>,
    /// Adapter manifest rank (if available)
    #[serde(rename = "manifest_rank", skip_serializing_if = "Option::is_none")]
    pub manifest_rank: Option<u32>,
    /// Adapter manifest base model (if available)
    #[serde(rename = "manifest_base_model", skip_serializing_if = "Option::is_none")]
    pub manifest_base_model: Option<String>,
    /// Whether per-layer hashes are present in manifest
    #[serde(
        rename = "manifest_per_layer_hashes",
        skip_serializing_if = "Option::is_none"
    )]
    pub manifest_per_layer_hashes: Option<bool>,
    /// Signature verification status for package
    #[serde(rename = "signature_status", skip_serializing_if = "Option::is_none")]
    pub signature_status: Option<String>,
}

impl TrainingJob {
    /// Create a new training job in Pending state
    pub fn new(id: String, adapter_name: String, config: TrainingConfig) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            adapter_name,
            template_id: None,
            repo_id: None,
            dataset_id: None,
            base_model_id: None,
            collection_id: None,
            build_id: None,
            source_documents_json: None,
            config_hash_b3: None,
            status: TrainingJobStatus::Pending,
            progress_pct: 0.0,
            current_epoch: 0,
            total_epochs: config.epochs,
            current_loss: 0.0,
            learning_rate: config.learning_rate,
            tokens_per_second: 0.0,
            created_at: now,
            started_at: None,
            completed_at: None,
            error_message: None,
            config,
            artifact_path: None,
            adapter_id: None,
            weights_hash_b3: None,
            tenant_id: None,
            stack_id: None,
            initiated_by: None,
            initiated_by_role: None,
            // Category metadata
            category: None,
            description: None,
            language: None,
            symbol_targets_json: None,
            framework_id: None,
            framework_version: None,
            api_patterns_json: None,
            repo_scope: None,
            file_patterns_json: None,
            exclude_patterns_json: None,
            post_actions_json: None,
            // Retry metadata
            retryable: None,
            retry_of_job_id: None,
            // Backend/determinism defaults
            backend: None,
            backend_reason: None,
            determinism_mode: None,
            training_seed: None,
            require_gpu: None,
            max_gpu_memory_mb: None,
            // Extended metrics defaults
            examples_processed: None,
            tokens_processed: None,
            training_time_ms: None,
            throughput_examples_per_sec: None,
            gpu_utilization_pct: None,
            peak_gpu_memory_mb: None,
            // Packaging defaults
            aos_path: None,
            package_hash_b3: None,
            manifest_rank: None,
            manifest_base_model: None,
            manifest_per_layer_hashes: None,
            signature_status: None,
        }
    }

    /// Builder method to set template ID
    pub fn with_template_id(mut self, template_id: String) -> Self {
        self.template_id = Some(template_id);
        self
    }

    /// Builder method to set repository ID
    pub fn with_repo_id(mut self, repo_id: String) -> Self {
        self.repo_id = Some(repo_id);
        self
    }

    /// Builder method to set dataset ID
    pub fn with_dataset_id(mut self, dataset_id: String) -> Self {
        self.dataset_id = Some(dataset_id);
        self
    }

    /// Builder method to set tenant ID
    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Builder method to set artifact path
    pub fn with_artifact_path(mut self, artifact_path: String) -> Self {
        self.artifact_path = Some(artifact_path);
        self
    }

    /// Builder method to set adapter ID
    pub fn with_adapter_id(mut self, adapter_id: String) -> Self {
        self.adapter_id = Some(adapter_id);
        self
    }

    /// Builder method to set weights hash
    pub fn with_weights_hash(mut self, hash: String) -> Self {
        self.weights_hash_b3 = Some(hash);
        self
    }

    /// Builder method to set base model ID
    pub fn with_base_model_id(mut self, base_model_id: String) -> Self {
        self.base_model_id = Some(base_model_id);
        self
    }

    /// Builder method to set collection ID
    pub fn with_collection_id(mut self, collection_id: String) -> Self {
        self.collection_id = Some(collection_id);
        self
    }

    /// Builder method to set build ID
    pub fn with_build_id(mut self, build_id: String) -> Self {
        self.build_id = Some(build_id);
        self
    }

    /// Builder method to set source documents JSON
    pub fn with_source_documents(mut self, source_documents_json: String) -> Self {
        self.source_documents_json = Some(source_documents_json);
        self
    }

    /// Builder method to set config hash
    pub fn with_config_hash(mut self, config_hash: String) -> Self {
        self.config_hash_b3 = Some(config_hash);
        self
    }

    /// Builder method to set category
    pub fn with_category(mut self, category: String) -> Self {
        self.category = Some(category);
        self
    }

    /// Builder method to set description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Builder method to set language
    pub fn with_language(mut self, language: String) -> Self {
        self.language = Some(language);
        self
    }

    /// Builder method to set framework ID
    pub fn with_framework_id(mut self, framework_id: String) -> Self {
        self.framework_id = Some(framework_id);
        self
    }

    /// Builder method to set framework version
    pub fn with_framework_version(mut self, framework_version: String) -> Self {
        self.framework_version = Some(framework_version);
        self
    }

    /// Builder method to set post actions JSON
    pub fn with_post_actions_json(mut self, post_actions_json: String) -> Self {
        self.post_actions_json = Some(post_actions_json);
        self
    }

    /// Mark job as started (transition from Pending to Running)
    pub fn start(&mut self) {
        if self.status == TrainingJobStatus::Pending {
            self.status = TrainingJobStatus::Running;
            self.started_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    /// Update training progress (typically called per epoch)
    pub fn update_progress(&mut self, epoch: u32, loss: f32, tokens_per_sec: f32) {
        self.current_epoch = epoch;
        self.current_loss = loss;
        self.tokens_per_second = tokens_per_sec;
        if self.total_epochs > 0 {
            self.progress_pct = (epoch as f32 / self.total_epochs as f32) * 100.0;
        }
        if self.status == TrainingJobStatus::Pending {
            self.start();
        }
    }

    /// Mark job as completed
    pub fn complete(&mut self) {
        self.status = TrainingJobStatus::Completed;
        self.progress_pct = 100.0;
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Mark job as failed with error message
    pub fn fail(&mut self, error: String) {
        self.status = TrainingJobStatus::Failed;
        self.error_message = Some(error);
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Cancel job
    pub fn cancel(&mut self) {
        if self.status.is_active() {
            self.status = TrainingJobStatus::Cancelled;
            self.completed_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    /// Check if job is in terminal state
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Get elapsed time in seconds since creation
    pub fn elapsed_seconds(&self) -> Option<u64> {
        use chrono::DateTime;

        let created = DateTime::parse_from_rfc3339(&self.created_at).ok()?;
        let reference = match &self.completed_at {
            Some(completed) => DateTime::parse_from_rfc3339(completed).ok()?,
            None => chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0)?),
        };

        let duration = reference.signed_duration_since(created);
        Some(duration.num_seconds() as u64)
    }
}

/// Training hyperparameters and configuration
///
/// Defines LoRA training configuration including rank, alpha scaling,
/// target modules, optimization parameters, and advanced options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    /// LoRA rank dimension (typically 4, 8, 16, 32)
    #[serde(rename = "rank")]
    pub rank: u32,

    /// LoRA alpha scaling factor (typically 2x rank)
    #[serde(rename = "alpha")]
    pub alpha: u32,

    /// Target linear layer names to apply LoRA
    #[serde(rename = "targets")]
    pub targets: Vec<String>,

    /// Number of training epochs
    #[serde(rename = "epochs")]
    pub epochs: u32,

    /// Learning rate for optimizer (e.g., 0.001)
    #[serde(rename = "learning_rate")]
    pub learning_rate: f32,

    /// Batch size for training
    #[serde(rename = "batch_size")]
    pub batch_size: u32,

    /// Warmup steps for learning rate schedule (optional)
    #[serde(rename = "warmup_steps", skip_serializing_if = "Option::is_none")]
    pub warmup_steps: Option<u32>,

    /// Maximum sequence length (optional, default 2048)
    #[serde(rename = "max_seq_length", skip_serializing_if = "Option::is_none")]
    pub max_seq_length: Option<u32>,

    /// Gradient accumulation steps for larger effective batch size (optional)
    #[serde(
        rename = "gradient_accumulation_steps",
        skip_serializing_if = "Option::is_none"
    )]
    pub gradient_accumulation_steps: Option<u32>,

    /// Advanced weight group configuration (optional, format TBD)
    #[serde(
        rename = "weight_group_config",
        skip_serializing_if = "Option::is_none"
    )]
    pub weight_group_config: Option<serde_json::Value>,

    /// Learning rate schedule type (constant, linear, cosine)
    #[serde(rename = "lr_schedule", skip_serializing_if = "Option::is_none")]
    pub lr_schedule: Option<String>,

    /// Final learning rate for decay schedules
    #[serde(rename = "final_lr", skip_serializing_if = "Option::is_none")]
    pub final_lr: Option<f32>,

    /// Enable early stopping
    #[serde(rename = "early_stopping", skip_serializing_if = "Option::is_none")]
    pub early_stopping: Option<bool>,

    /// Early stopping patience (epochs to wait)
    #[serde(rename = "patience", skip_serializing_if = "Option::is_none")]
    pub patience: Option<u32>,

    /// Minimum delta for early stopping improvement
    #[serde(rename = "min_delta", skip_serializing_if = "Option::is_none")]
    pub min_delta: Option<f32>,

    /// Save checkpoints every N epochs
    #[serde(
        rename = "checkpoint_frequency",
        skip_serializing_if = "Option::is_none"
    )]
    pub checkpoint_frequency: Option<u32>,

    /// Maximum number of checkpoints to keep
    #[serde(rename = "max_checkpoints", skip_serializing_if = "Option::is_none")]
    pub max_checkpoints: Option<u32>,
}

impl TrainingConfig {
    /// Create default training configuration
    pub fn default_for_adapter() -> Self {
        Self {
            rank: 16,
            alpha: 32,
            targets: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
                "gate_proj".to_string(),
                "up_proj".to_string(),
                "down_proj".to_string(),
            ],
            epochs: 3,
            learning_rate: 0.001,
            batch_size: 32,
            warmup_steps: Some(100),
            max_seq_length: Some(2048),
            gradient_accumulation_steps: Some(4),
            weight_group_config: None,
            lr_schedule: Some("cosine".to_string()),
            final_lr: Some(0.0001),
            early_stopping: Some(false),
            patience: Some(5),
            min_delta: Some(0.001),
            checkpoint_frequency: Some(5),
            max_checkpoints: Some(3),
        }
    }

    /// Create minimal quick-training configuration
    pub fn quick_training() -> Self {
        Self {
            rank: 8,
            alpha: 16,
            targets: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
            ],
            epochs: 1,
            learning_rate: 0.002,
            batch_size: 16,
            warmup_steps: None,
            max_seq_length: Some(2048),
            gradient_accumulation_steps: None,
            weight_group_config: None,
            lr_schedule: Some("constant".to_string()),
            final_lr: None,
            early_stopping: Some(false),
            patience: None,
            min_delta: None,
            checkpoint_frequency: None,
            max_checkpoints: None,
        }
    }

    /// Create deep training configuration
    pub fn deep_training() -> Self {
        Self {
            rank: 32,
            alpha: 64,
            targets: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
                "gate_proj".to_string(),
                "up_proj".to_string(),
                "down_proj".to_string(),
                "mlp.dense_h_to_4h".to_string(),
                "mlp.dense_4h_to_h".to_string(),
            ],
            epochs: 5,
            learning_rate: 0.0005,
            batch_size: 64,
            warmup_steps: Some(500),
            max_seq_length: Some(4096),
            gradient_accumulation_steps: Some(8),
            weight_group_config: None,
            lr_schedule: Some("linear".to_string()),
            final_lr: Some(0.00001),
            early_stopping: Some(true),
            patience: Some(10),
            min_delta: Some(0.0001),
            checkpoint_frequency: Some(2),
            max_checkpoints: Some(5),
        }
    }
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self::default_for_adapter()
    }
}

/// Training template for reusable configurations
///
/// Provides pre-configured training setups for common scenarios.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingTemplate {
    /// Unique template identifier
    #[serde(rename = "id")]
    pub id: String,

    /// Human-readable template name
    #[serde(rename = "name")]
    pub name: String,

    /// Template description and use cases
    #[serde(rename = "description")]
    pub description: String,

    /// Template category (e.g., "code", "creative", "domain-specific")
    #[serde(rename = "category")]
    pub category: String,

    /// Embedded training configuration
    #[serde(rename = "config")]
    pub config: TrainingConfig,
}

impl TrainingTemplate {
    /// Create a new training template
    pub fn new(
        id: String,
        name: String,
        description: String,
        category: String,
        config: TrainingConfig,
    ) -> Self {
        Self {
            id,
            name,
            description,
            category,
            config,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_training_job_status_display() {
        assert_eq!(TrainingJobStatus::Pending.to_string(), "pending");
        assert_eq!(TrainingJobStatus::Running.to_string(), "running");
        assert_eq!(TrainingJobStatus::Completed.to_string(), "completed");
        assert_eq!(TrainingJobStatus::Failed.to_string(), "failed");
        assert_eq!(TrainingJobStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_training_job_status_terminal() {
        assert!(!TrainingJobStatus::Pending.is_terminal());
        assert!(!TrainingJobStatus::Running.is_terminal());
        assert!(TrainingJobStatus::Completed.is_terminal());
        assert!(TrainingJobStatus::Failed.is_terminal());
        assert!(TrainingJobStatus::Cancelled.is_terminal());
    }

    #[test]
    fn test_training_job_status_active() {
        assert!(TrainingJobStatus::Pending.is_active());
        assert!(TrainingJobStatus::Running.is_active());
        assert!(!TrainingJobStatus::Completed.is_active());
        assert!(!TrainingJobStatus::Failed.is_active());
        assert!(!TrainingJobStatus::Cancelled.is_active());
    }

    #[test]
    fn test_training_job_creation() {
        let config = TrainingConfig::default();
        let job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config);

        assert_eq!(job.id, "job-123");
        assert_eq!(job.adapter_name, "my-adapter");
        assert_eq!(job.status, TrainingJobStatus::Pending);
        assert_eq!(job.progress_pct, 0.0);
        assert_eq!(job.total_epochs, 3);
    }

    #[test]
    fn test_training_job_builder() {
        let config = TrainingConfig::default();
        let job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config)
            .with_template_id("general-code".to_string())
            .with_repo_id("repo-456".to_string())
            .with_dataset_id("ds-789".to_string());

        assert_eq!(job.template_id, Some("general-code".to_string()));
        assert_eq!(job.repo_id, Some("repo-456".to_string()));
        assert_eq!(job.dataset_id, Some("ds-789".to_string()));
    }

    #[test]
    fn test_training_job_lifecycle() {
        let config = TrainingConfig::default();
        let mut job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config);

        // Start job
        job.start();
        assert_eq!(job.status, TrainingJobStatus::Running);
        assert!(job.started_at.is_some());

        // Update progress
        job.update_progress(1, 0.5, 1000.0);
        assert_eq!(job.current_epoch, 1);
        assert_eq!(job.current_loss, 0.5);
        assert!(job.progress_pct > 0.0);

        // Complete job
        job.complete();
        assert_eq!(job.status, TrainingJobStatus::Completed);
        assert_eq!(job.progress_pct, 100.0);
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_training_job_pause_resume() {
        let config = TrainingConfig::default();
        let mut job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config);

        job.start();
        assert_eq!(job.status, TrainingJobStatus::Running);

        // Pause/resume removed; ensure running remains unchanged by no-op pattern
        job.update_progress(1, 0.5, 1000.0);
        assert_eq!(job.status, TrainingJobStatus::Running);
    }

    #[test]
    fn test_training_job_cancel() {
        let config = TrainingConfig::default();
        let mut job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config);

        job.start();
        job.cancel();
        assert_eq!(job.status, TrainingJobStatus::Cancelled);
        assert!(job.completed_at.is_some());
        assert!(job.is_terminal());
    }

    #[test]
    fn test_training_job_fail() {
        let config = TrainingConfig::default();
        let mut job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config);

        job.fail("Out of memory".to_string());
        assert_eq!(job.status, TrainingJobStatus::Failed);
        assert_eq!(job.error_message, Some("Out of memory".to_string()));
        assert!(job.is_terminal());
    }

    #[test]
    fn test_training_config_variants() {
        let quick = TrainingConfig::quick_training();
        assert_eq!(quick.rank, 8);
        assert_eq!(quick.epochs, 1);

        let deep = TrainingConfig::deep_training();
        assert_eq!(deep.rank, 32);
        assert_eq!(deep.epochs, 5);

        let default = TrainingConfig::default();
        assert_eq!(default.rank, 16);
        assert_eq!(default.epochs, 3);
    }

    #[test]
    fn test_training_config_serialization() {
        let config = TrainingConfig::default();
        let json = serde_json::to_string(&config).expect("serialize");

        // Verify snake_case serialization
        assert!(json.contains("\"rank\":"));
        assert!(json.contains("\"learning_rate\":"));
        assert!(json.contains("\"batch_size\":"));
        assert!(json.contains("\"warmup_steps\":"));
    }

    #[test]
    fn test_training_job_serialization() {
        let config = TrainingConfig::default();
        let job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config)
            .with_weights_hash("abc123".to_string());

        let json = serde_json::to_value(&job).expect("serialize");

        // Verify snake_case serialization
        assert!(json.get("adapter_name").is_some());
        assert!(json.get("current_epoch").is_some());
        assert!(json.get("progress_pct").is_some());
        assert!(json.get("learning_rate").is_some());
        assert!(json.get("tokens_per_second").is_some());
        assert!(json.get("weights_hash_b3").is_some());
    }
}
