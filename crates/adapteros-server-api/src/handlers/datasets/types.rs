//! Request/response types for dataset handlers.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

/// Query parameters for listing datasets
#[derive(Deserialize, ToSchema, IntoParams)]
pub struct ListDatasetsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub format: Option<String>,
    pub validation_status: Option<String>,
    pub workspace_id: Option<String>,
}

/// Request to initiate a chunked upload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InitiateChunkedUploadRequest {
    /// File name being uploaded
    pub file_name: String,
    /// Total file size in bytes
    pub total_size: u64,
    /// Optional idempotency key for retry-safe session reuse
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// Optional expected BLAKE3 hash of the full file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_file_hash_b3: Option<String>,
    /// Content type (e.g., application/gzip)
    pub content_type: Option<String>,
    /// Chunk size preference (will be clamped to valid range)
    pub chunk_size: Option<usize>,
    /// Optional workspace ID for tenant isolation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// Response from initiating a chunked upload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InitiateChunkedUploadResponse {
    /// Unique session identifier
    pub session_id: String,
    /// Chunk size that will be used
    pub chunk_size: usize,
    /// Expected number of chunks
    pub expected_chunks: usize,
    /// Whether compression is detected
    pub compression_format: String,
}

/// Query parameters for uploading a chunk
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct UploadChunkQuery {
    /// Index of this chunk (0-based)
    pub chunk_index: usize,
}

/// Response from uploading a chunk
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadChunkResponse {
    /// Session ID
    pub session_id: String,
    /// Chunk index that was uploaded
    pub chunk_index: usize,
    /// BLAKE3 hash of this chunk
    pub chunk_hash: String,
    /// Total chunks received so far
    pub chunks_received: usize,
    /// Total expected chunks
    pub expected_chunks: usize,
    /// Is upload complete (all chunks received)?
    pub is_complete: bool,
    /// Resume token for resuming from next chunk (if not complete)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
}

/// Request to apply an admin trust override to a dataset version
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatasetTrustOverrideRequest {
    /// Override state: allowed | allowed_with_warning | blocked | needs_approval
    pub override_state: String,
    /// Optional human-readable reason for auditability
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Request to complete a chunked upload and create the dataset
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteChunkedUploadRequest {
    /// Dataset name (optional, defaults to file name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Dataset description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Dataset format (e.g., "jsonl", "json", "csv")
    #[serde(default = "default_format")]
    pub format: String,
    /// Optional workspace ID for tenant isolation (should match initiate request)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

fn default_format() -> String {
    "jsonl".to_string()
}

/// Response from completing a chunked upload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompleteChunkedUploadResponse {
    /// Created dataset ID
    pub dataset_id: String,
    /// The dataset version ID created for this upload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_id: Option<String>,
    /// Dataset name
    pub name: String,
    /// Dataset hash (manifest-derived BLAKE3)
    pub hash: String,
    /// Total file size in bytes
    pub total_size_bytes: i64,
    /// Storage path
    pub storage_path: String,
    /// Timestamp when dataset was created
    pub created_at: String,
    /// Workspace ID if dataset was scoped to a workspace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// Response for getting upload session status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadSessionStatusResponse {
    /// Session ID
    pub session_id: String,
    /// Original file name
    pub file_name: String,
    /// Total file size in bytes
    pub total_size: u64,
    /// Chunk size for this upload
    pub chunk_size: usize,
    /// Expected number of chunks
    pub expected_chunks: usize,
    /// Number of chunks received
    pub chunks_received: usize,
    /// List of chunk indices that have been received
    pub received_chunk_indices: Vec<usize>,
    /// Whether all chunks have been received
    pub is_complete: bool,
    /// Session creation timestamp (RFC3339)
    pub created_at: String,
    /// Compression format detected
    pub compression_format: String,
}

/// Query parameters for progress stream
#[derive(Deserialize, ToSchema)]
pub struct ProgressStreamQuery {
    pub dataset_id: Option<String>,
}

/// Request to create a dataset version
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateDatasetVersionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_json: Option<Value>,
}

/// Response from creating a dataset version
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateDatasetVersionResponse {
    pub dataset_id: String,
    pub dataset_version_id: String,
    pub version_number: i64,
    pub trust_state: String,
    pub created_at: String,
}

/// Update semantic/safety statuses for a dataset version (Tier 2).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateDatasetSafetyRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pii_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toxicity_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leak_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anomaly_status: Option<String>,
}

/// Response from updating dataset safety status
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UpdateDatasetSafetyResponse {
    pub dataset_id: String,
    pub dataset_version_id: String,
    pub trust_state: String,
    pub overall_safety_status: String,
}

/// Admin override for dataset trust_state.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TrustOverrideRequest {
    pub trust_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Response from trust override
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrustOverrideResponse {
    pub dataset_id: String,
    pub dataset_version_id: String,
    pub trust_state: String,
}

/// Request to create a dataset from existing documents or a collection
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDatasetFromDocumentsRequest {
    /// Single document ID (mutually exclusive with collection_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// Multiple document IDs (mutually exclusive with collection_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_ids: Option<Vec<String>>,
    /// Collection ID to convert (mutually exclusive with document_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    /// Name for the new dataset (auto-generated if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Query parameters for retrying a chunk upload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct RetryChunkQuery {
    /// Index of the chunk to retry (0-based)
    pub chunk_index: usize,
    /// Expected hash for validation (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
}

/// Response from retrying a chunk upload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RetryChunkResponse {
    /// Session ID
    pub session_id: String,
    /// Chunk index that was retried
    pub chunk_index: usize,
    /// BLAKE3 hash of the new chunk
    pub chunk_hash: String,
    /// Previous hash if this was replacing an existing chunk
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
    /// Total chunks received so far
    pub chunks_received: usize,
    /// Total expected chunks
    pub expected_chunks: usize,
    /// Is upload complete (all chunks received)?
    pub is_complete: bool,
    /// Whether this was actually a retry (chunk existed before)
    pub was_retry: bool,
}

/// Summary of an upload session for listing
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadSessionSummary {
    /// Session ID
    pub session_id: String,
    /// Original file name
    pub file_name: String,
    /// Total file size in bytes
    pub total_size: u64,
    /// Number of chunks received
    pub chunks_received: usize,
    /// Total expected chunks
    pub expected_chunks: usize,
    /// Upload progress percentage
    pub progress_percent: f32,
    /// Session creation timestamp (RFC3339)
    pub created_at: String,
    /// Age of the session in seconds
    pub age_seconds: u64,
    /// Whether the session has expired
    pub is_expired: bool,
}

/// Response for listing upload sessions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListUploadSessionsResponse {
    /// List of active upload sessions
    pub sessions: Vec<UploadSessionSummary>,
    /// Total number of active sessions
    pub total_count: usize,
    /// Maximum allowed concurrent sessions
    pub max_sessions: usize,
}
