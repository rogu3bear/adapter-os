use crate::auth::Claims;
use crate::inference_core::InferenceCore;
use crate::permissions::{require_permission, Permission};
use crate::security::check_tenant_access;
use crate::state::AppState;
use crate::types::{
    BatchInferItemResponse, BatchInferRequest, BatchInferResponse, ErrorResponse, InferResponse,
    InferenceRequestInternal,
};
use axum::{extract::State, http::StatusCode, Extension, Json};
use std::time::Duration;
use tokio::time::{timeout, Instant};
use tracing::{error, warn};

const MAX_BATCH_SIZE: usize = 32;
const BATCH_TIMEOUT: Duration = Duration::from_secs(30);

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
    require_permission(&claims, Permission::InferenceExecute)?;

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

    // Create InferenceCore instance (reused for all batch items)
    let core = InferenceCore::new(&state);
    let deadline = Instant::now() + BATCH_TIMEOUT;

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

        // Validate collection access if RAG is requested
        if let Some(collection_id) = &item.request.collection_id {
            // CRITICAL: Validate collection belongs to user's tenant
            match state.db.get_collection(collection_id).await {
                Ok(Some(collection)) => {
                    if !check_tenant_access(&claims, &collection.tenant_id) {
                        warn!(
                            item_id = %item.id,
                            collection_id = %collection_id,
                            user_tenant = %claims.tenant_id,
                            collection_tenant = %collection.tenant_id,
                            "Collection access denied - tenant mismatch"
                        );
                        responses.push(BatchInferItemResponse {
                            id: item.id,
                            response: None,
                            error: Some(
                                ErrorResponse::new("Access denied to collection")
                                    .with_code("FORBIDDEN")
                                    .with_string_details(
                                        "Collection does not belong to your tenant",
                                    ),
                            ),
                        });
                        continue;
                    }
                }
                Ok(None) => {
                    responses.push(BatchInferItemResponse {
                        id: item.id,
                        response: None,
                        error: Some(
                            ErrorResponse::new("Collection not found").with_code("NOT_FOUND"),
                        ),
                    });
                    continue;
                }
                Err(e) => {
                    error!(
                        item_id = %item.id,
                        collection_id = %collection_id,
                        error = %e,
                        "Failed to validate collection access"
                    );
                    responses.push(BatchInferItemResponse {
                        id: item.id,
                        response: None,
                        error: Some(
                            ErrorResponse::new("Failed to validate collection access")
                                .with_code("DATABASE_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    });
                    continue;
                }
            }
        }

        // Convert batch item to InferenceRequestInternal
        let internal_request: InferenceRequestInternal = (&item, &claims).into();

        // Execute inference via InferenceCore with timeout
        match timeout(remaining, core.route_and_infer(internal_request, None)).await {
            Ok(Ok(result)) => {
                // Convert InferenceResult to InferResponse
                let response: InferResponse = result.into();
                responses.push(BatchInferItemResponse {
                    id: item.id,
                    response: Some(response),
                    error: None,
                });
            }
            Ok(Err(err)) => {
                responses.push(map_inference_error(item.id, err));
            }
            Err(_) => {
                responses.push(timeout_error(item.id));
            }
        }
    }

    Ok(Json(BatchInferResponse { responses }))
}

fn map_inference_error(id: String, err: crate::types::InferenceError) -> BatchInferItemResponse {
    use crate::types::InferenceError;

    match err {
        InferenceError::ValidationError(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("validation failed")
                    .with_code("BAD_REQUEST")
                    .with_string_details(msg),
            ),
        },
        InferenceError::WorkerNotAvailable(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("worker not available")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(msg),
            ),
        },
        InferenceError::Timeout(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("inference timeout")
                    .with_code("REQUEST_TIMEOUT")
                    .with_string_details(msg),
            ),
        },
        InferenceError::PermissionDenied(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("permission denied")
                    .with_code("FORBIDDEN")
                    .with_string_details(msg),
            ),
        },
        InferenceError::BackpressureError(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("backpressure")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(msg),
            ),
        },
        InferenceError::RoutingBypass(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("routing bypass detected")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(msg),
            ),
        },
        InferenceError::NoCompatibleWorker {
            required_hash,
            tenant_id,
            available_count,
            reason,
        } => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("no compatible worker available")
                    .with_code("NO_COMPATIBLE_WORKER")
                    .with_string_details(format!(
                        "No worker with manifest {} for tenant {} ({} available). {}",
                        required_hash, tenant_id, available_count, reason
                    )),
            ),
        },
        InferenceError::RagError(msg) | InferenceError::WorkerError(msg) => {
            BatchInferItemResponse {
                id,
                response: None,
                error: Some(
                    ErrorResponse::new("inference failed")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(msg),
                ),
            }
        }
        InferenceError::AdapterNotFound(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("adapter not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(msg),
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
