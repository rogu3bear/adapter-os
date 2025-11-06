use crate::auth::Claims;
use crate::state::AppState;
use crate::types::{
    BatchInferItemResponse, BatchInferRequest, BatchInferResponse, ErrorResponse, WorkerInferRequest,
};
use adapteros_api_types::InferenceTrace;
use adapteros_api_types::InferResponse;
use crate::uds_client::{UdsClient, UdsClientError};
use axum::{extract::State, http::StatusCode, Extension, Json};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::{timeout, Instant};

const MAX_BATCH_SIZE: usize = 32;
const BATCH_TIMEOUT: Duration = Duration::from_secs(30);
const WORKER_TIMEOUT: Duration = Duration::from_secs(30);

#[utoipa::path(
    post,
    path = "/v1/infer/batch",
    request_body = BatchInferRequest,
    responses(
        (
            status = 200,
            description = "Batch inference successful",
            body = BatchInferResponse
        ),
        (
            status = 400,
            description = "Invalid batch request",
            body = ErrorResponse
        ),
        (
            status = 503,
            description = "No workers available",
            body = ErrorResponse
        ),
        (
            status = 500,
            description = "Batch inference failed",
            body = ErrorResponse
        )
    ),
    tag = "inference"
)]
pub async fn batch_infer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<BatchInferRequest>,
) -> Result<Json<BatchInferResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.requests.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("requests cannot be empty")
                    .with_code("BAD_REQUEST")
                    .with_string_details("Provide at least one inference request"),
            ),
        ));
    }

    if req.requests.len() > MAX_BATCH_SIZE {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("batch size exceeded")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!(
                        "Maximum batch size is {} requests",
                        MAX_BATCH_SIZE
                    )),
            ),
        ));
    }

    let workers = match state.db.list_all_workers().await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::warn!(
                "Failed to list workers (falling back to default UDS): {}",
                e
            );
            Vec::new()
        }
    };

    // Resolve UDS path: prefer registered worker; otherwise fall back to per-tenant default
    let uds_path = if let Some(worker) = workers.first() {
        PathBuf::from(&worker.uds_path)
    } else {
        let fallback = std::env::var("AOS_WORKER_SOCKET")
            .unwrap_or_else(|_| format!("/var/run/aos/{}/aos.sock", claims.tenant_id));
        PathBuf::from(fallback)
    };
    let uds_client = UdsClient::new(WORKER_TIMEOUT);
    let deadline = Instant::now() + BATCH_TIMEOUT;
    let cpid = claims.sub.clone();

    let mut responses = Vec::with_capacity(req.requests.len());

    for item in req.requests {
        if item.request.prompt.trim().is_empty() {
            responses.push(BatchInferItemResponse {
                id: item.id,
                response: None,
                error: Some(
                    ErrorResponse::new("prompt cannot be empty")
                        .with_code("BAD_REQUEST")
                        .with_string_details("Each batch item must include a prompt"),
                ),
            });
            continue;
        }

        let now = Instant::now();
        if now >= deadline {
            responses.push(timeout_error(item.id));
            continue;
        }

        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            responses.push(timeout_error(item.id));
            continue;
        }

        let worker_request = WorkerInferRequest {
            cpid: cpid.clone(),
            prompt: item.request.prompt.clone(),
            max_tokens: item.request.max_tokens.unwrap_or(100),
            require_evidence: item.request.require_evidence.unwrap_or(false),
            adapter_hints: None, // No pre-routing for batch infer endpoint
            router_features: None,
        };

        // Record batch inference latency
        let inference_start = std::time::Instant::now();
        match timeout(
            remaining,
            uds_client.infer(uds_path.as_path(), worker_request),
        )
        .await
        {
            Ok(Ok(worker_response)) => {
                let inference_latency_secs = inference_start.elapsed().as_secs_f64();

                // Record real inference latency metrics
                state.metrics_collector.record_inference_latency(
                    &claims.tenant_id,
                    "qwen2.5-7b", // adapter_id - could be dynamic
                    inference_latency_secs,
                );

                // Record tokens generated
                let tokens_generated = worker_response
                    .text
                    .as_ref()
                    .map(|text| text.split_whitespace().count() as u64)
                    .unwrap_or(0);
                if tokens_generated > 0 {
                    state.metrics_collector.record_tokens_generated(
                        &claims.tenant_id,
                        "qwen2.5-7b",
                        tokens_generated,
                    );
                }
                let response = InferResponse {
                    text: worker_response.text.unwrap_or_default(),
                    tokens: vec![],
                    finish_reason: worker_response.status.clone(),
                    trace: InferenceTrace {
                        adapters_used: worker_response.trace.router_summary.adapters_used.clone(),
                        router_decisions: vec![],
                        latency_ms: 0,
                    },
                };

                responses.push(BatchInferItemResponse {
                    id: item.id,
                    response: Some(response),
                    error: None,
                });
            }
            Ok(Err(err)) => {
                responses.push(map_worker_error(item.id, err));
            }
            Err(_) => {
                responses.push(timeout_error(item.id));
            }
        }
    }

    Ok(Json(BatchInferResponse { responses }))
}

fn map_worker_error(id: String, err: UdsClientError) -> BatchInferItemResponse {
    match err {
        UdsClientError::WorkerNotAvailable(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("worker not available")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(msg),
            ),
        },
        UdsClientError::Timeout(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("inference timeout")
                    .with_code("REQUEST_TIMEOUT")
                    .with_string_details(msg),
            ),
        },
        other => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("inference failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(other.to_string()),
            ),
        },
    }
}

fn timeout_error(id: String) -> BatchInferItemResponse {
    BatchInferItemResponse {
        id,
        response: None,
        error: Some(
            ErrorResponse::new("batch timeout exceeded")
                .with_code("REQUEST_TIMEOUT")
                .with_string_details("Batch processing exceeded the configured deadline"),
        ),
    }
}
