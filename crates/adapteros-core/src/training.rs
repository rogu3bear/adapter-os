//! Training types shared across AdapterOS crates
//!
//! These types define the core training domain model and are used
//! by both the orchestrator and API types to avoid cyclic dependencies.

use serde::{Deserialize, Serialize};

/// Training job state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TrainingJobStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TrainingJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrainingJobStatus::Pending => write!(f, "pending"),
            TrainingJobStatus::Running => write!(f, "running"),
            TrainingJobStatus::Paused => write!(f, "paused"),
            TrainingJobStatus::Completed => write!(f, "completed"),
            TrainingJobStatus::Failed => write!(f, "failed"),
            TrainingJobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Training job information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingJob {
    pub id: String,
    pub adapter_name: String,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
    pub status: TrainingJobStatus,
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
    pub config: TrainingConfig,
    // Artifact metadata (populated when packaging is enabled)
    pub artifact_path: Option<String>,
    pub adapter_id: Option<String>,
    pub weights_hash_b3: Option<String>,
}

/// Training configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
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

impl Default for TrainingConfig {
    fn default() -> Self {
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
        }
    }
}

/// Training template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub config: TrainingConfig,
}
