use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::inference_core::InferenceCore;
use crate::middleware::ApiKeyToken;
use crate::permissions::{require_permission, Permission};
use crate::session_tokens::{
    ensure_no_adapter_overrides, resolve_session_token_lock, SessionTokenContext,
};
use crate::security::check_tenant_access;
use crate::state::AppState;
use crate::types::{
    BatchInferItemRequest, BatchInferItemResponse, BatchInferRequest, BatchInferResponse,
    BatchItemResultResponse, BatchItemsQuery, BatchItemsResponse, BatchJobResponse,
    BatchStatusResponse, CreateBatchJobRequest, ErrorResponse, InferResponse,
    InferenceRequestInternal, MAX_REPLAY_TEXT_SIZE,
};
use adapteros_db::{CreateBatchItemParams, CreateBatchJobParams};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use futures_util::stream::{self, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::{timeout, Instant};
use tracing::{debug, error, info, warn};

const MAX_BATCH_SIZE: usize = 32;
const BATCH_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CONCURRENT_BATCH_ITEMS: usize = 6;

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
    api_key: Option<Extension<ApiKeyToken>>,
    session_token: Option<Extension<SessionTokenContext>>,
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

    let session_lock = if let Some(token) = session_token.as_ref() {
        Some(resolve_session_token_lock(&state, &claims, &token.0.lock).await?)
    } else {
        None
    };

    let worker_token = api_key.as_ref().map(|t| t.0.clone());
    let deadline = Instant::now() + BATCH_TIMEOUT;

    // Process batch items concurrently using futures::stream
    let responses = stream::iter(req.requests.into_iter())
        .map(|item| {
            let state = state.clone();
            let claims = claims.clone();
            let worker_token = worker_token.clone();
            let session_lock = session_lock.clone();

            async move {
                // Early validation: empty prompt
                if item.request.prompt.trim().is_empty() {
                    return BatchInferItemResponse {
                        id: item.id,
                        response: None,
                        error: Some(
                            ErrorResponse::new("prompt cannot be empty")
                                .with_code("BAD_REQUEST")
                                .with_string_details("Each batch item must include a prompt"),
                        ),
                    };
                }
                if item.request.prompt.len() > MAX_REPLAY_TEXT_SIZE {
                    return BatchInferItemResponse {
                        id: item.id,
                        response: None,
                        error: Some(
                            ErrorResponse::new("prompt too long for context window")
                                .with_code("BAD_REQUEST")
                                .with_string_details("Prompt exceeds maximum size"),
                        ),
                    };
                }

                if let Some(lock) = session_lock.as_ref() {
                    if let Err(err) = ensure_no_adapter_overrides(&[
                        ("adapters", item.request.adapters.is_some()),
                        ("adapter_stack", item.request.adapter_stack.is_some()),
                        ("stack_id", item.request.stack_id.is_some()),
                        (
                            "effective_adapter_ids",
                            item.request.effective_adapter_ids.is_some(),
                        ),
                    ]) {
                        let (_, Json(error)) =
                            <(StatusCode, Json<ErrorResponse>)>::from(err);
                        return BatchInferItemResponse {
                            id: item.id,
                            response: None,
                            error: Some(error),
                        };
                    }

                    if let (Some(requested), Some(locked)) =
                        (item.request.backend, lock.backend_profile)
                    {
                        if requested != locked {
                            let (_, Json(error)) =
                                <(StatusCode, Json<ErrorResponse>)>::from(
                                    ApiError::forbidden("session token backend mismatch")
                                        .with_details(format!(
                                            "requested {}, token {}",
                                            requested.as_str(),
                                            locked.as_str()
                                        )),
                                );
                            return BatchInferItemResponse {
                                id: item.id,
                                response: None,
                                error: Some(error),
                            };
                        }
                    }

                    if let (Some(requested), Some(locked)) =
                        (item.request.coreml_mode, lock.coreml_mode)
                    {
                        if requested != locked {
                            let (_, Json(error)) =
                                <(StatusCode, Json<ErrorResponse>)>::from(
                                    ApiError::forbidden("session token coreml_mode mismatch")
                                        .with_details(format!(
                                            "requested {}, token {}",
                                            requested.as_str(),
                                            locked.as_str()
                                        )),
                                );
                            return BatchInferItemResponse {
                                id: item.id,
                                response: None,
                                error: Some(error),
                            };
                        }
                    }
                }

                // Check deadline before processing
                let now = Instant::now();
                if now >= deadline {
                    return timeout_error(item.id);
                }

                let remaining = deadline.saturating_duration_since(now);
                if remaining.is_zero() {
                    return timeout_error(item.id);
                }

                // Validate collection access if RAG is requested
                if let Some(collection_id) = &item.request.collection_id {
                    // CRITICAL: Validate collection belongs to user's tenant
                    match state
                        .db
                        .get_collection(&claims.tenant_id, collection_id)
                        .await
                    {
                        Ok(Some(collection)) => {
                            if !check_tenant_access(&claims, &collection.tenant_id) {
                                warn!(
                                    item_id = %item.id,
                                    collection_id = %collection_id,
                                    user_tenant = %claims.tenant_id,
                                    collection_tenant = %collection.tenant_id,
                                    "Collection access denied - tenant mismatch"
                                );
                                return BatchInferItemResponse {
                                    id: item.id,
                                    response: None,
                                    error: Some(
                                        ErrorResponse::new("Access denied to collection")
                                            .with_code("FORBIDDEN")
                                            .with_string_details(
                                                "Collection does not belong to your tenant",
                                            ),
                                    ),
                                };
                            }
                        }
                        Ok(None) => {
                            return BatchInferItemResponse {
                                id: item.id,
                                response: None,
                                error: Some(
                                    ErrorResponse::new("Collection not found")
                                        .with_code("NOT_FOUND"),
                                ),
                            };
                        }
                        Err(e) => {
                            error!(
                                item_id = %item.id,
                                collection_id = %collection_id,
                                error = %e,
                                "Failed to validate collection access"
                            );
                            return BatchInferItemResponse {
                                id: item.id,
                                response: None,
                                error: Some(
                                    ErrorResponse::new("Failed to validate collection access")
                                        .with_code("DATABASE_ERROR")
                                        .with_string_details(e.to_string()),
                                ),
                            };
                        }
                    }
                }

                // Convert batch item to InferenceRequestInternal
                let mut internal_request: InferenceRequestInternal = (&item, &claims).into();
                internal_request.worker_auth_token = worker_token.clone().map(|t| t.0);
                if let Some(lock) = session_lock.as_ref() {
                    internal_request.adapter_stack = None;
                    internal_request.adapters = Some(lock.adapter_ids.clone());
                    internal_request.effective_adapter_ids = Some(lock.adapter_ids.clone());
                    internal_request.stack_id = lock.stack_id.clone();
                    internal_request.pinned_adapter_ids = Some(lock.pinned_adapter_ids.clone());
                    if let Some(backend) = lock.backend_profile {
                        internal_request.backend_profile = Some(backend);
                        internal_request.allow_fallback =
                            backend == adapteros_core::BackendKind::Auto;
                    }
                    if let Some(coreml_mode) = lock.coreml_mode {
                        internal_request.coreml_mode = Some(coreml_mode);
                    }
                }

                // Create InferenceCore for this batch item
                let inference_core = InferenceCore::new(&state);

                // Execute inference via InferenceCore with timeout
                match timeout(
                    remaining,
                    inference_core.route_and_infer(internal_request, None, None, None),
                )
                .await
                {
                    Ok(Ok(result)) => {
                        // Convert InferenceResult to InferResponse
                        let response: InferResponse = result.into();
                        BatchInferItemResponse {
                            id: item.id,
                            response: Some(response),
                            error: None,
                        }
                    }
                    Ok(Err(err)) => map_inference_error(item.id, err),
                    Err(_) => timeout_error(item.id),
                }
            }
        })
        .buffer_unordered(MAX_CONCURRENT_BATCH_ITEMS)
        .collect::<Vec<_>>()
        .await;

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
        InferenceError::ClientClosed(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("client disconnected")
                    .with_code("CLIENT_CLOSED_REQUEST")
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
        InferenceError::ModelNotReady(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("model not ready")
                    .with_code("MODEL_NOT_READY")
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
            details,
        } => BatchInferItemResponse {
            id,
            response: None,
            error: {
                let reason_detail = format!(
                    "No worker with manifest {} for tenant {} ({} available). {}",
                    required_hash, tenant_id, available_count, reason
                );
                let mut response = ErrorResponse::new("no compatible worker available")
                    .with_code("NO_COMPATIBLE_WORKER");
                if let Some(value) = details {
                    response = response.with_details(serde_json::json!({
                        "reason": reason_detail,
                        "compatibility": value,
                    }));
                } else {
                    response = response.with_string_details(reason_detail);
                }
                Some(response)
            },
        },
        InferenceError::AdapterNotFound(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("adapter not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(msg),
            ),
        },
        InferenceError::AdapterTenantMismatch {
            adapter_id,
            tenant_id,
            adapter_tenant_id,
        } => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("adapter tenant mismatch")
                    .with_code("ADAPTER_TENANT_MISMATCH")
                    .with_string_details(format!(
                        "Adapter {} belongs to tenant {} (request tenant {})",
                        adapter_id, adapter_tenant_id, tenant_id
                    )),
            ),
        },
        InferenceError::AdapterBaseModelMismatch {
            adapter_id,
            expected_base_model_id,
            adapter_base_model_id,
        } => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("adapter base model mismatch")
                    .with_code("ADAPTER_BASE_MODEL_MISMATCH")
                    .with_string_details(format!(
                        "Adapter {} base model mismatch: expected {}, adapter has {}",
                        adapter_id,
                        expected_base_model_id,
                        adapter_base_model_id.unwrap_or_else(|| "unknown".to_string())
                    )),
            ),
        },
        InferenceError::WorkerIdUnavailable { tenant_id, reason } => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("worker ID unavailable")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(format!(
                        "Worker ID for tenant {} unavailable: {}",
                        tenant_id, reason
                    )),
            ),
        },
        InferenceError::CacheBudgetExceeded {
            needed_mb,
            freed_mb,
            pinned_count,
            active_count,
            max_mb,
            model_key,
        } => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("cache budget exceeded")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(format!(
                        "Model {} needs {}MB, freed {}MB ({} pinned, {} active), max {}MB",
                        model_key.unwrap_or_default(),
                        needed_mb,
                        freed_mb,
                        pinned_count,
                        active_count,
                        max_mb
                    )),
            ),
        },
        InferenceError::PolicyViolation {
            tenant_id,
            policy_id,
            reason,
        } => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("policy violation")
                    .with_code("FORBIDDEN")
                    .with_string_details(format!(
                        "Policy {} for tenant {} violated: {}",
                        policy_id, tenant_id, reason
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
        InferenceError::WorkerDegraded { tenant_id, reason } => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("worker degraded")
                    .with_code("WORKER_DEGRADED")
                    .with_string_details(format!(
                        "Worker degraded for tenant {}: {}",
                        tenant_id, reason
                    )),
            ),
        },
        InferenceError::DatabaseError(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(msg),
            ),
        },
        InferenceError::AdapterNotLoadable { adapter_id, reason } => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("adapter not loadable")
                    .with_code("ADAPTER_NOT_LOADABLE")
                    .with_string_details(format!(
                        "Adapter {} not loadable: {}",
                        adapter_id, reason
                    )),
            ),
        },
        InferenceError::ReplayError(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("replay error")
                    .with_code("REPLAY_ERROR")
                    .with_string_details(msg),
            ),
        },
        InferenceError::DeterminismError(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("determinism error")
                    .with_code("DETERMINISM_ERROR")
                    .with_string_details(msg),
            ),
        },
        InferenceError::InternalError(msg) => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("internal error")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(msg),
            ),
        },
        InferenceError::DuplicateRequest { request_id } => BatchInferItemResponse {
            id,
            response: None,
            error: Some(
                ErrorResponse::new("duplicate request")
                    .with_code("DUPLICATE_REQUEST")
                    .with_string_details(format!("Request {} is already in-flight", request_id)),
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

// ============================================================================
// Async Batch Job Handlers (New Persistent Batch System)
// ============================================================================

/// Create a new batch job for async processing
#[utoipa::path(
    post,
    path = "/v1/batches",
    request_body = CreateBatchJobRequest,
    responses(
        (
            status = 201,
            description = "Batch job created successfully",
            body = BatchJobResponse
        ),
        (
            status = 400,
            description = "Invalid batch request",
            body = ErrorResponse
        ),
        (
            status = 500,
            description = "Failed to create batch job",
            body = ErrorResponse
        )
    ),
    tag = "inference"
)]
pub async fn create_batch_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateBatchJobRequest>,
) -> Result<(StatusCode, Json<BatchJobResponse>), (StatusCode, Json<ErrorResponse>)> {
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

    let timeout_secs = req.timeout_secs.unwrap_or(30);
    let max_concurrent = req.max_concurrent.unwrap_or(6);

    // Validate limits
    if timeout_secs <= 0 || timeout_secs > 600 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid timeout")
                    .with_code("BAD_REQUEST")
                    .with_string_details("timeout_secs must be between 1 and 600"),
            ),
        ));
    }

    if max_concurrent <= 0 || max_concurrent > 20 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid max_concurrent")
                    .with_code("BAD_REQUEST")
                    .with_string_details("max_concurrent must be between 1 and 20"),
            ),
        ));
    }

    // Create batch job record (database generates the ID)
    let batch_job_params = CreateBatchJobParams {
        tenant_id: claims.tenant_id.clone(),
        user_id: claims.sub.clone(),
        total_items: req.requests.len() as i64,
        timeout_secs: timeout_secs as i64,
        max_concurrent: max_concurrent as i64,
    };

    let batch_id = state
        .db
        .create_batch_job(batch_job_params)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create batch job");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create batch job")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Create batch item records
    let batch_items: Vec<CreateBatchItemParams> = req
        .requests
        .iter()
        .map(|item| {
            let request_json = serde_json::to_string(&item).unwrap_or_default();
            CreateBatchItemParams {
                batch_job_id: batch_id.clone(),
                item_id: item.id.clone(),
                request_json,
            }
        })
        .collect();

    state
        .db
        .create_batch_items(batch_items)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create batch items");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create batch items")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Spawn background task to process the batch
    let state_clone = state.clone();
    let batch_id_clone = batch_id.clone();
    let claims_clone = claims.clone();

    tokio::spawn(async move {
        if let Err(e) = process_batch_job(
            &state_clone,
            &batch_id_clone,
            &claims_clone,
            timeout_secs,
            max_concurrent,
        )
        .await
        {
            error!(batch_id = %batch_id_clone, error = %e, "Batch job processing failed");
            // Update batch job status to failed
            let _ = state_clone
                .db
                .update_batch_job_status(&batch_id_clone, "failed", Some(&e.to_string()))
                .await;
        }
    });

    info!(batch_id = %batch_id, total_items = req.requests.len(), "Batch job created");

    Ok((
        StatusCode::CREATED,
        Json(BatchJobResponse {
            batch_id,
            status: "pending".to_string(),
        }),
    ))
}

/// Process a batch job asynchronously
async fn process_batch_job(
    state: &AppState,
    batch_id: &str,
    claims: &Claims,
    timeout_secs: i32,
    max_concurrent: i32,
) -> Result<(), adapteros_core::AosError> {
    info!(batch_id = %batch_id, "Starting batch job processing");

    // Update job status to running
    state
        .db
        .update_batch_job_status(batch_id, "running", None)
        .await?;

    // Get batch items and filter for pending status
    let all_items = state.db.get_batch_items(batch_id).await?;
    let items: Vec<_> = all_items
        .into_iter()
        .filter(|item| item.status == "pending")
        .collect();

    if items.is_empty() {
        state
            .db
            .update_batch_job_status(batch_id, "completed", None)
            .await?;
        return Ok(());
    }

    // Create semaphore for concurrency control
    let semaphore = Arc::new(Semaphore::new(max_concurrent as usize));
    let deadline = Instant::now() + Duration::from_secs(timeout_secs as u64);

    // Process items with controlled parallelism
    let results = stream::iter(items.into_iter())
        .map(|item| {
            let state = state.clone();
            let claims = claims.clone();
            let semaphore = semaphore.clone();
            let batch_id = batch_id.to_string();

            async move {
                // Acquire semaphore permit
                let _permit = semaphore.acquire().await.ok()?;

                // Check deadline
                let now = Instant::now();
                if now >= deadline {
                    // Mark as timeout
                    let _ = state
                        .db
                        .update_batch_item_result(
                            &item.id,
                            "timeout",
                            None,
                            Some("REQUEST_TIMEOUT"),
                            Some("Batch processing deadline exceeded"),
                            None,
                        )
                        .await;
                    let _ = state.db.increment_batch_failed(&batch_id).await;
                    return Some(());
                }

                let remaining = deadline.saturating_duration_since(now);
                if remaining.is_zero() {
                    let _ = state
                        .db
                        .update_batch_item_result(
                            &item.id,
                            "timeout",
                            None,
                            Some("REQUEST_TIMEOUT"),
                            Some("Batch processing deadline exceeded"),
                            None,
                        )
                        .await;
                    let _ = state.db.increment_batch_failed(&batch_id).await;
                    return Some(());
                }

                // Parse the request
                let batch_item_request: BatchInferItemRequest =
                    match serde_json::from_str(&item.request_json) {
                        Ok(req) => req,
                        Err(e) => {
                            error!(item_id = %item.item_id, error = %e, "Failed to parse batch item request");
                            let _ = state
                                .db
                                .update_batch_item_result(
                                    &item.id,
                                    "failed",
                                    None,
                                    Some("PARSE_ERROR"),
                                    Some(&e.to_string()),
                                    None,
                                )
                                .await;
                            let _ = state.db.increment_batch_failed(&batch_id).await;
                            return Some(());
                        }
                    };

                // Validate prompt
                if batch_item_request.request.prompt.trim().is_empty() {
                    let _ = state
                        .db
                        .update_batch_item_result(
                            &item.id,
                            "failed",
                            None,
                            Some("BAD_REQUEST"),
                            Some("prompt cannot be empty"),
                            None,
                        )
                        .await;
                    let _ = state.db.increment_batch_failed(&batch_id).await;
                    return Some(());
                }

                // Validate collection access if RAG is requested
                if let Some(collection_id) = &batch_item_request.request.collection_id {
                    match state.db.get_collection(&claims.tenant_id, collection_id).await {
                        Ok(Some(collection)) => {
                            if !check_tenant_access(&claims, &collection.tenant_id) {
                                warn!(
                                    item_id = %batch_item_request.id,
                                    collection_id = %collection_id,
                                    user_tenant = %claims.tenant_id,
                                    collection_tenant = %collection.tenant_id,
                                    "Collection access denied - tenant mismatch"
                                );
                                let _ = state
                                    .db
                                    .update_batch_item_result(
                                        &item.id,
                                        "failed",
                                        None,
                                        Some("FORBIDDEN"),
                                        Some("Collection does not belong to your tenant"),
                                        None,
                                    )
                                    .await;
                                let _ = state.db.increment_batch_failed(&batch_id).await;
                                return Some(());
                            }
                        }
                        Ok(None) => {
                            let _ = state
                                .db
                                .update_batch_item_result(
                            &item.id,
                            "failed",
                            None,
                            Some("NOT_FOUND"),
                            Some("Collection not found"),
                            None,
                        )
                                .await;
                            let _ = state.db.increment_batch_failed(&batch_id).await;
                            return Some(());
                        }
                        Err(e) => {
                            error!(
                                item_id = %batch_item_request.id,
                                collection_id = %collection_id,
                                error = %e,
                                "Failed to validate collection access"
                            );
                            let _ = state
                                .db
                                .update_batch_item_result(
                                    &item.id,
                                    "failed",
                                    None,
                                    Some("DATABASE_ERROR"),
                                    Some(&e.to_string()),
                                    None,
                                )
                                .await;
                            let _ = state.db.increment_batch_failed(&batch_id).await;
                            return Some(());
                        }
                    }
                }

                // Mark as running
                let _ = state
                    .db
                    .update_batch_item_status(&item.id, "running")
                    .await;

                // Convert to InferenceRequestInternal
                let internal_request: InferenceRequestInternal =
                    (&batch_item_request, &claims).into();

                // Create InferenceCore for this batch item
                let core = InferenceCore::new(&state);

                // Execute inference via InferenceCore with timeout
                let start = Instant::now();
                let result =
                    timeout(remaining, core.route_and_infer(internal_request, None, None, None))
                        .await;
                let latency_ms = start.elapsed().as_millis() as i32;

                match result {
                    Ok(Ok(inference_result)) => {
                        // Convert InferenceResult to InferResponse
                        let response: InferResponse = inference_result.into();
                        let response_json = serde_json::to_string(&response).ok();

                        let _ = state
                            .db
                            .update_batch_item_result(
                                &item.id,
                                "completed",
                                response_json.as_deref(),
                                None,
                                None,
                                Some(latency_ms),
                            )
                            .await;
                        let _ = state.db.increment_batch_completed(&batch_id).await;
                    }
                    Ok(Err(err)) => {
                        let error_code = match &err {
                            crate::types::InferenceError::ValidationError(_) => "BAD_REQUEST",
                            crate::types::InferenceError::WorkerNotAvailable(_) => {
                                "SERVICE_UNAVAILABLE"
                            }
                            crate::types::InferenceError::Timeout(_) => "REQUEST_TIMEOUT",
                            crate::types::InferenceError::ClientClosed(_) => "CLIENT_CLOSED_REQUEST",
                            crate::types::InferenceError::PermissionDenied(_) => "FORBIDDEN",
                            crate::types::InferenceError::BackpressureError(_) => {
                                "SERVICE_UNAVAILABLE"
                            }
                            crate::types::InferenceError::RoutingBypass(_) => "INTERNAL_ERROR",
                            crate::types::InferenceError::NoCompatibleWorker { .. } => {
                                "NO_COMPATIBLE_WORKER"
                            }
                            crate::types::InferenceError::WorkerDegraded { .. } => {
                                "WORKER_DEGRADED"
                            }
                            crate::types::InferenceError::ModelNotReady(_) => "MODEL_NOT_READY",
                            crate::types::InferenceError::RagError(_) => "INTERNAL_ERROR",
                            crate::types::InferenceError::WorkerError(_) => "INTERNAL_ERROR",
                            crate::types::InferenceError::AdapterNotFound(_) => "NOT_FOUND",
                            crate::types::InferenceError::AdapterTenantMismatch { .. } => {
                                "ADAPTER_TENANT_MISMATCH"
                            }
                            crate::types::InferenceError::AdapterBaseModelMismatch { .. } => {
                                "ADAPTER_BASE_MODEL_MISMATCH"
                            }
                            crate::types::InferenceError::WorkerIdUnavailable { .. } => {
                                "SERVICE_UNAVAILABLE"
                            }
                            crate::types::InferenceError::CacheBudgetExceeded { .. } => {
                                "CACHE_BUDGET_EXCEEDED"
                            }
                            crate::types::InferenceError::PolicyViolation { .. } => "FORBIDDEN",
                            crate::types::InferenceError::DatabaseError(_) => "DATABASE_ERROR",
                            crate::types::InferenceError::AdapterNotLoadable { .. } => {
                                "ADAPTER_NOT_LOADABLE"
                            }
                            crate::types::InferenceError::ReplayError(_) => "REPLAY_ERROR",
                            crate::types::InferenceError::DeterminismError(_) => {
                                "DETERMINISM_ERROR"
                            }
                            crate::types::InferenceError::InternalError(_) => "INTERNAL_ERROR",
                            crate::types::InferenceError::DuplicateRequest { .. } => {
                                "DUPLICATE_REQUEST"
                            }
                        };

                        let _ = state
                            .db
                            .update_batch_item_result(
                                &item.id,
                                "failed",
                                None,
                                Some(error_code),
                                Some(&err.to_string()),
                                Some(latency_ms),
                            )
                            .await;
                        let _ = state.db.increment_batch_failed(&batch_id).await;
                    }
                    Err(_) => {
                        let _ = state
                            .db
                            .update_batch_item_result(
                            &item.id,
                            "timeout",
                            None,
                            Some("REQUEST_TIMEOUT"),
                            Some("Item processing timeout"),
                            Some(latency_ms),
                        )
                            .await;
                        let _ = state.db.increment_batch_failed(&batch_id).await;
                    }
                }

                Some(())
            }
        })
        .buffer_unordered(max_concurrent as usize)
        .collect::<Vec<_>>()
        .await;

    debug!(batch_id = %batch_id, processed = results.len(), "Batch processing completed");

    // Update final status
    state
        .db
        .update_batch_job_status(batch_id, "completed", None)
        .await?;

    info!(batch_id = %batch_id, "Batch job completed");
    Ok(())
}

/// Get batch job status
#[utoipa::path(
    get,
    path = "/v1/batches/{batch_id}",
    params(
        ("batch_id" = String, Path, description = "Batch job identifier")
    ),
    responses(
        (
            status = 200,
            description = "Batch status retrieved successfully",
            body = BatchStatusResponse
        ),
        (
            status = 404,
            description = "Batch job not found",
            body = ErrorResponse
        ),
        (
            status = 500,
            description = "Failed to retrieve batch status",
            body = ErrorResponse
        )
    ),
    tag = "inference"
)]
pub async fn get_batch_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(batch_id): Path<String>,
) -> Result<Json<BatchStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute)?;

    let batch_job = state
        .db
        .get_batch_job(&batch_id, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!(batch_id = %batch_id, error = %e, "Failed to get batch job");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get batch job")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("batch job not found")
                        .with_code("NOT_FOUND")
                        .with_string_details("The specified batch job does not exist"),
                ),
            )
        })?;

    // CRITICAL: Validate tenant isolation
    if !check_tenant_access(&claims, &batch_job.tenant_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("access denied")
                    .with_code("FORBIDDEN")
                    .with_string_details("Batch job does not belong to your tenant"),
            ),
        ));
    }

    Ok(Json(BatchStatusResponse {
        batch_id: batch_job.id,
        status: batch_job.status,
        total_items: batch_job.total_items,
        completed_items: batch_job.completed_items,
        failed_items: batch_job.failed_items,
        created_at: batch_job.created_at,
        started_at: batch_job.started_at,
        completed_at: batch_job.completed_at,
    }))
}

/// Get batch items with optional status filter
#[utoipa::path(
    get,
    path = "/v1/batches/{batch_id}/items",
    params(
        ("batch_id" = String, Path, description = "Batch job identifier"),
        BatchItemsQuery
    ),
    responses(
        (
            status = 200,
            description = "Batch items retrieved successfully",
            body = BatchItemsResponse
        ),
        (
            status = 404,
            description = "Batch job not found",
            body = ErrorResponse
        ),
        (
            status = 500,
            description = "Failed to retrieve batch items",
            body = ErrorResponse
        )
    ),
    tag = "inference"
)]
pub async fn get_batch_items(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(batch_id): Path<String>,
    Query(query): Query<BatchItemsQuery>,
) -> Result<Json<BatchItemsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute)?;

    // Verify batch job exists and user has access
    let batch_job = state
        .db
        .get_batch_job(&batch_id, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!(batch_id = %batch_id, error = %e, "Failed to get batch job");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get batch job")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("batch job not found")
                        .with_code("NOT_FOUND")
                        .with_string_details("The specified batch job does not exist"),
                ),
            )
        })?;

    // CRITICAL: Validate tenant isolation
    if !check_tenant_access(&claims, &batch_job.tenant_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("access denied")
                    .with_code("FORBIDDEN")
                    .with_string_details("Batch job does not belong to your tenant"),
            ),
        ));
    }

    // Get batch items
    let all_items = state.db.get_batch_items(&batch_id).await.map_err(|e| {
        error!(batch_id = %batch_id, error = %e, "Failed to get batch items");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to get batch items")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Filter by status if requested
    let mut items = all_items;
    if let Some(status) = query.status.as_deref() {
        items.retain(|item| item.status == status);
    }

    // Apply pagination
    let limit = query.limit.unwrap_or(100) as usize;
    let offset = query.offset.unwrap_or(0) as usize;
    let items: Vec<_> = items.into_iter().skip(offset).take(limit).collect();

    // Convert to response format
    let items: Vec<BatchItemResultResponse> = items
        .into_iter()
        .map(|item| {
            let response = if let Some(json) = &item.response_json {
                serde_json::from_str::<InferResponse>(json).ok()
            } else {
                None
            };

            BatchItemResultResponse {
                id: item.item_id,
                status: item.status,
                response,
                error: item.error_message,
                latency_ms: item.latency_ms,
            }
        })
        .collect();

    Ok(Json(BatchItemsResponse { items }))
}
