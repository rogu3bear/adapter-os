//! Inference handler implementations
//!
//! This module contains placeholder handler functions. The actual implementations
//! remain in `adapteros-server-api` due to tight coupling with:
//! - `AppState` (database, config, worker registry, metrics)
//! - `InferenceCore` (unified inference execution pipeline)
//! - Policy enforcement middleware
//! - UDS client for worker communication
//!
//! The handlers in this module are re-exported for documentation purposes.
//! The actual routes use the hub crate handlers directly.
//!
//! # Handler Summary
//!
//! | Endpoint | Handler | Description |
//! |----------|---------|-------------|
//! | `POST /v1/infer` | `infer` | Standard inference with policy hooks |
//! | `POST /v1/infer/stream` | `streaming_infer` | SSE streaming inference |
//! | `POST /v1/infer/stream/progress` | `streaming_infer_with_progress` | Streaming with loading phases |
//! | `POST /v1/infer/batch` | `batch_infer` | Synchronous batch processing |
//! | `POST /v1/batches` | `create_batch_job` | Create async batch job |
//! | `GET /v1/batches/{batch_id}` | `get_batch_status` | Get batch job status |
//! | `GET /v1/batches/{batch_id}/items` | `get_batch_items` | Get batch item results |
//! | `GET /v1/inference/{trace_id}/provenance` | `get_inference_provenance` | Get audit trail |
//!
//! # Policy Hook Integration
//!
//! Standard and streaming inference handlers enforce policies at three hooks:
//!
//! 1. **OnRequestBeforeRouting** - Before adapter selection
//! 2. **OnBeforeInference** - After routing, before worker dispatch
//! 3. **OnAfterInference** - After inference completes (audit-only for streaming)
//!
//! The policy mask digest is computed from all decisions and included in the
//! run envelope as a cryptographic commitment.

use axum::{response::IntoResponse, Json};

/// Placeholder for the standard inference handler
///
/// The actual handler is in `adapteros-server-api::handlers::inference::infer`.
/// It executes an 11-stage pipeline through InferenceCore.
///
/// # Pipeline Stages
///
/// 1. Request validation (tenant isolation, sampling params)
/// 2. Adapter resolution (DB lookup, pinned overrides)
/// 3. OnRequestBeforeRouting policy hook
/// 4. RAG context retrieval (if enabled)
/// 5. Router decision (K-sparse, Q15 gates)
/// 6. Worker selection (placement constraints)
/// 7. OnBeforeInference policy hook
/// 8. Worker inference (UDS)
/// 9. OnAfterInference policy hook
/// 10. Evidence & telemetry
/// 11. Response assembly
pub async fn infer_placeholder() -> impl IntoResponse {
    Json(serde_json::json!({
        "error": "This is a placeholder. Use the actual handler from adapteros-server-api.",
        "handler": "adapteros_server_api::handlers::inference::infer"
    }))
}

/// Placeholder for the streaming inference handler
///
/// The actual handler is in `adapteros-server-api::handlers::streaming_infer::streaming_infer`.
/// It returns an SSE stream of tokens as they are generated.
///
/// # SSE Event Types
///
/// - `aos.run_envelope` - Run context envelope (first event)
/// - `stream_started` - Stream lifecycle start with recovery info
/// - Data chunks - OpenAI-compatible format with delta content
/// - `[DONE]` - Sentinel indicating completion
/// - `stream_finished` - Stream lifecycle end with summary
/// - `error` - Error events with code and retryable flag
pub async fn streaming_infer_placeholder() -> impl IntoResponse {
    Json(serde_json::json!({
        "error": "This is a placeholder. Use the actual handler from adapteros-server-api.",
        "handler": "adapteros_server_api::handlers::streaming_infer::streaming_infer"
    }))
}

/// Placeholder for streaming inference with loading progress
///
/// The actual handler is in `adapteros-server-api::handlers::streaming_infer::streaming_infer_with_progress`.
/// It emits loading progress events before streaming tokens.
///
/// # Loading Phases
///
/// 1. `Loading { phase: LoadingWeights, progress: 0, eta_seconds }` - Initial load
/// 2. `Loading { phase: Warmup, progress: 50, eta_seconds }` - Warmup phase
/// 3. `Ready { warmup_latency_ms }` - Adapter ready
/// 4. `Token { text, token_id }` - Generated tokens
/// 5. `Done { total_tokens, latency_ms, citations, ... }` - Completion
pub async fn streaming_infer_with_progress_placeholder() -> impl IntoResponse {
    Json(serde_json::json!({
        "error": "This is a placeholder. Use the actual handler from adapteros-server-api.",
        "handler": "adapteros_server_api::handlers::streaming_infer::streaming_infer_with_progress"
    }))
}

/// Placeholder for synchronous batch inference
///
/// The actual handler is in `adapteros-server-api::handlers::batch::batch_infer`.
/// It processes up to 32 requests concurrently with a 30-second timeout.
pub async fn batch_infer_placeholder() -> impl IntoResponse {
    Json(serde_json::json!({
        "error": "This is a placeholder. Use the actual handler from adapteros-server-api.",
        "handler": "adapteros_server_api::handlers::batch::batch_infer"
    }))
}

/// Placeholder for creating async batch jobs
///
/// The actual handler is in `adapteros-server-api::handlers::batch::create_batch_job`.
/// It creates a persistent batch job that processes in the background.
pub async fn create_batch_job_placeholder() -> impl IntoResponse {
    Json(serde_json::json!({
        "error": "This is a placeholder. Use the actual handler from adapteros-server-api.",
        "handler": "adapteros_server_api::handlers::batch::create_batch_job"
    }))
}

/// Placeholder for getting batch job status
///
/// The actual handler is in `adapteros-server-api::handlers::batch::get_batch_status`.
pub async fn get_batch_status_placeholder() -> impl IntoResponse {
    Json(serde_json::json!({
        "error": "This is a placeholder. Use the actual handler from adapteros-server-api.",
        "handler": "adapteros_server_api::handlers::batch::get_batch_status"
    }))
}

/// Placeholder for getting batch items
///
/// The actual handler is in `adapteros-server-api::handlers::batch::get_batch_items`.
pub async fn get_batch_items_placeholder() -> impl IntoResponse {
    Json(serde_json::json!({
        "error": "This is a placeholder. Use the actual handler from adapteros-server-api.",
        "handler": "adapteros_server_api::handlers::batch::get_batch_items"
    }))
}

/// Placeholder for inference provenance
///
/// The actual handler is in `adapteros-server-api::handlers::inference::get_inference_provenance`.
/// It traces inference decisions back through adapters to source documents.
pub async fn get_inference_provenance_placeholder() -> impl IntoResponse {
    Json(serde_json::json!({
        "error": "This is a placeholder. Use the actual handler from adapteros-server-api.",
        "handler": "adapteros_server_api::handlers::inference::get_inference_provenance"
    }))
}
