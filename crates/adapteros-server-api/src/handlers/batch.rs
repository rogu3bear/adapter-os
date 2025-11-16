use crate::auth::Claims;
use crate::state::AppState;
use crate::types::{
    BatchInferItemResponse, BatchInferRequest, BatchInferResponse, ErrorResponse, InferResponse,
    InferenceTrace, WorkerInferRequest,
};
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

    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list workers")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if workers.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("no workers available")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details("No active workers found for inference"),
            ),
        ));
    }

    let worker = &workers[0];
    let uds_path = PathBuf::from(&worker.uds_path);
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
        };

        match timeout(
            remaining,
            uds_client.infer(uds_path.as_path(), worker_request),
        )
        .await
        {
            Ok(Ok(worker_response)) => {
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
