//! Batch inference types
//!
//! This module provides types for batch inference endpoints:
//! - Synchronous batch processing (`/v1/infer/batch`)
//! - Async batch jobs (`/v1/batches`)

use crate::types::InferResponse;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Maximum number of requests in a batch
pub const MAX_BATCH_SIZE: usize = 32;

/// Default batch timeout in seconds
pub const BATCH_TIMEOUT_SECS: u64 = 30;

/// Maximum concurrent batch items for processing
pub const MAX_CONCURRENT_BATCH_ITEMS: usize = 6;

/// Batch inference request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchInferRequest {
    /// List of inference requests to process
    pub requests: Vec<BatchInferItemRequest>,
}

/// Individual item in a batch request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchInferItemRequest {
    /// Unique identifier for this batch item
    pub id: String,

    /// The inference request payload
    pub request: BatchInferItemPayload,
}

/// Payload for a batch inference item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchInferItemPayload {
    /// The input prompt
    pub prompt: String,

    /// Maximum tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Sampling temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Optional model identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Optional collection ID for RAG
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,

    /// Optional adapters to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
}

fn default_max_tokens() -> usize {
    512
}

fn default_temperature() -> f32 {
    0.7
}

/// Batch inference response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchInferResponse {
    /// Results for each batch item
    pub responses: Vec<BatchInferItemResponse>,
}

/// Individual item in a batch response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchInferItemResponse {
    /// Unique identifier matching the request
    pub id: String,

    /// The inference response (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<InferResponse>,

    /// Error details (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BatchItemError>,
}

/// Error for a batch item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchItemError {
    /// Error message
    pub message: String,
    /// Error code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

// ============================================================================
// Async Batch Job Types
// ============================================================================

/// Request to create an async batch job
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateBatchJobRequest {
    /// List of inference requests to process
    pub requests: Vec<BatchInferItemRequest>,

    /// Timeout in seconds for the entire batch (default: 30, max: 600)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i32>,

    /// Maximum concurrent items to process (default: 6, max: 20)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<i32>,
}

/// Response when creating a batch job
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchJobResponse {
    /// Unique batch job identifier
    pub batch_id: String,
    /// Initial status (always "pending")
    pub status: String,
}

/// Batch job status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchStatusResponse {
    /// Batch job identifier
    pub batch_id: String,
    /// Current status: pending, running, completed, failed
    pub status: String,
    /// Total number of items in the batch
    pub total_items: i64,
    /// Number of completed items
    pub completed_items: i64,
    /// Number of failed items
    pub failed_items: i64,
    /// When the job was created
    pub created_at: String,
    /// When processing started
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// When processing completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// Query parameters for batch items endpoint
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct BatchItemsQuery {
    /// Filter by status (pending, running, completed, failed, timeout)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    /// Maximum number of items to return (default: 100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,

    /// Offset for pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
}

/// Response for batch items query
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchItemsResponse {
    /// List of batch item results
    pub items: Vec<BatchItemResultResponse>,
}

/// Individual batch item result
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchItemResultResponse {
    /// Item identifier from the original request
    pub id: String,
    /// Item status: pending, running, completed, failed, timeout
    pub status: String,
    /// The inference response (if completed successfully)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<InferResponse>,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Processing latency in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_request_deserialization() {
        let json = r#"{
            "requests": [
                {
                    "id": "item-1",
                    "request": {
                        "prompt": "Hello, world!"
                    }
                }
            ]
        }"#;

        let req: BatchInferRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.requests.len(), 1);
        assert_eq!(req.requests[0].id, "item-1");
        assert_eq!(req.requests[0].request.prompt, "Hello, world!");
        assert_eq!(req.requests[0].request.max_tokens, 512);
    }

    #[test]
    fn test_batch_response_serialization() {
        let response = BatchInferResponse {
            responses: vec![BatchInferItemResponse {
                id: "item-1".to_string(),
                response: None,
                error: Some(BatchItemError {
                    message: "Test error".to_string(),
                    code: Some("TEST_ERROR".to_string()),
                    details: None,
                }),
            }],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("TEST_ERROR"));
    }

    #[test]
    fn test_create_batch_job_request() {
        let json = r#"{
            "requests": [],
            "timeout_secs": 60,
            "max_concurrent": 10
        }"#;

        let req: CreateBatchJobRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.timeout_secs, Some(60));
        assert_eq!(req.max_concurrent, Some(10));
    }
}
