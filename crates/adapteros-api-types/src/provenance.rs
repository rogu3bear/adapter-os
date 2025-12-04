//! Chat Provenance API types
//!
//! Provides the response structure for tracing chat session lineage:
//! chat -> stack -> adapters[] -> training jobs -> datasets -> base model

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Complete provenance graph for a chat session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ChatProvenanceResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// The chat session being traced
    pub session: SessionSummary,
    /// The adapter stack used (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<StackProvenance>,
    /// Individual adapters with their provenance
    pub adapters: Vec<AdapterProvenance>,
    /// The base model used (derived from training jobs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_model: Option<BaseModelInfo>,
    /// Timeline of provenance events (chronological)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<Vec<ProvenanceEvent>>,
    /// BLAKE3 hash of the provenance graph (for audit trail)
    pub provenance_hash: String,
    /// Timestamp when provenance was computed
    pub computed_at: String,
}

/// Summary of the chat session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    pub created_at: String,
    pub last_activity_at: String,
    pub message_count: i64,
}

/// Stack-level provenance information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct StackProvenance {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_type: Option<String>,
    pub adapter_ids: Vec<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Adapter with full provenance chain
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterProvenance {
    pub id: String,
    pub name: String,
    pub hash_b3: String,
    pub tier: String,
    /// Flag indicating if this is an externally-created adapter (no training job)
    pub externally_created: bool,
    /// Training job that created this adapter (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_job: Option<TrainingJobProvenance>,
    pub created_at: String,
}

/// Training job provenance
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TrainingJobProvenance {
    pub id: String,
    pub status: String,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub created_by: String,
    /// Dataset used for training
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset: Option<DatasetProvenance>,
    /// Base model ID used for training
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_model_id: Option<String>,
    /// Config hash for reproducibility (BLAKE3)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_hash_b3: Option<String>,
}

/// Dataset provenance
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetProvenance {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub format: String,
    pub file_count: i32,
    pub total_size_bytes: i64,
    pub hash_b3: String,
    pub validation_status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Base model information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct BaseModelInfo {
    pub id: String,
    pub name: String,
    pub hash_b3: String,
    pub created_at: String,
}

/// Timeline event for provenance
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ProvenanceEvent {
    pub event_type: ProvenanceEventType,
    pub entity_id: String,
    pub entity_name: String,
    pub timestamp: String,
    pub description: String,
}

/// Types of provenance events
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceEventType {
    DatasetCreated,
    TrainingJobStarted,
    TrainingJobCompleted,
    AdapterRegistered,
    StackCreated,
    ChatStarted,
}
