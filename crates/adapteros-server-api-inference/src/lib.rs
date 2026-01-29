//! Inference and streaming endpoints for AdapterOS server
//!
//! This crate provides inference handlers extracted from `adapteros-server-api`:
//!
//! - **Standard inference** (`/v1/infer`) - Single request/response inference
//! - **Streaming inference** (`/v1/infer/stream`) - SSE streaming with OpenAI-compatible format
//! - **Streaming with progress** (`/v1/infer/stream/progress`) - Includes loading phases
//! - **Batch inference** (`/v1/infer/batch`) - Process multiple prompts concurrently
//! - **Async batch jobs** (`/v1/batches`) - Persistent batch processing with status polling
//! - **Provenance** (`/v1/inference/{trace_id}/provenance`) - Audit trail for inference decisions
//!
//! # SSE Event Format
//!
//! Streaming endpoints emit OpenAI-compatible Server-Sent Events:
//!
//! ```text
//! event: aos.run_envelope
//! data: {"run_id":"...","schema_version":"..."}
//!
//! event: stream_started
//! data: {"type":"stream_started","stream_id":"..."}
//!
//! data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"role":"assistant"}}]}
//! data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"token"}}]}
//! data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"finish_reason":"stop"}]}
//! data: [DONE]
//!
//! event: stream_finished
//! data: {"type":"stream_finished","total_tokens":42,"duration_ms":1234}
//! ```
//!
//! # Architecture
//!
//! This crate defines the types and route configuration. The actual handler implementations
//! are in `adapteros-server-api` due to tight coupling with `AppState` and `InferenceCore`.
//! The `inference_routes_with_state()` function is used by `adapteros-server` to mount
//! routes that delegate to the hub crate handlers.

pub mod batch;
pub mod handlers;
pub mod provenance;
pub mod routes;
pub mod streaming;
pub mod types;

// Re-export route builders
pub use routes::{inference_routes, inference_routes_with_state};

// Re-export key types for external use
pub use batch::{
    BatchInferItemRequest, BatchInferItemResponse, BatchInferRequest, BatchInferResponse,
    BatchItemResultResponse, BatchItemsQuery, BatchItemsResponse, BatchJobResponse,
    BatchStatusResponse, CreateBatchJobRequest,
};
pub use provenance::{AdapterProvenanceInfo, DocumentProvenanceInfo, ProvenanceResponse};
pub use streaming::{
    AdapterStateInfo, Delta, InferenceEvent, LoadPhase, StreamConfig, StreamError,
    StreamErrorPayload, StreamingChoice, StreamingChunk, StreamingInferRequest,
};
pub use types::{InferRequest, InferResponse};
