//! Provenance types for inference audit trail
//!
//! This module provides types for the provenance endpoint that traces
//! inference decisions back through adapters to source documents.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response for provenance chain query
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProvenanceResponse {
    /// Inference trace ID
    pub trace_id: String,

    /// Tenant that owns this trace
    pub tenant_id: String,

    /// Request ID if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// When the inference occurred
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

    /// Adapters that contributed to this inference
    pub adapters: Vec<AdapterProvenanceInfo>,

    /// Source documents traced back from adapters
    pub source_documents: Vec<DocumentProvenanceInfo>,

    /// Whether full provenance could be resolved
    pub is_complete: bool,

    /// Any warnings about missing provenance links
    pub warnings: Vec<String>,

    /// Total confidence score
    pub confidence: f32,
}

/// Adapter provenance in API response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterProvenanceInfo {
    /// Adapter ID
    pub adapter_id: String,

    /// Normalized gate value (0.0-1.0)
    pub gate: f32,

    /// Training job that created this adapter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_job_id: Option<String>,

    /// Dataset version used for training
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_id: Option<String>,
}

/// Document provenance in API response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocumentProvenanceInfo {
    /// Source file path
    pub source_file: String,

    /// BLAKE3 content hash
    pub content_hash: String,

    /// Line range if known (format: "start-end" or "start+")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provenance_response_serialization() {
        let response = ProvenanceResponse {
            trace_id: "trace-123".to_string(),
            tenant_id: "tenant-456".to_string(),
            request_id: Some("req-789".to_string()),
            created_at: Some("2024-01-15T10:30:00Z".to_string()),
            adapters: vec![AdapterProvenanceInfo {
                adapter_id: "adapter-1".to_string(),
                gate: 0.85,
                training_job_id: Some("job-111".to_string()),
                dataset_version_id: Some("ds-v1".to_string()),
            }],
            source_documents: vec![DocumentProvenanceInfo {
                source_file: "docs/guide.md".to_string(),
                content_hash: "abc123".to_string(),
                lines: Some("10-50".to_string()),
            }],
            is_complete: true,
            warnings: vec![],
            confidence: 0.95,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("trace-123"));
        assert!(json.contains("0.85"));
        assert!(json.contains("docs/guide.md"));
    }

    #[test]
    fn test_provenance_with_warnings() {
        let response = ProvenanceResponse {
            trace_id: "trace-456".to_string(),
            tenant_id: "tenant-789".to_string(),
            request_id: None,
            created_at: None,
            adapters: vec![],
            source_documents: vec![],
            is_complete: false,
            warnings: vec![
                "Adapter training lineage not found".to_string(),
                "Source documents could not be resolved".to_string(),
            ],
            confidence: 0.0,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("is_complete\":false"));
        assert!(json.contains("Adapter training lineage not found"));
    }
}
