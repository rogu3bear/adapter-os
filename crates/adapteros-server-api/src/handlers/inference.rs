use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::{ErrorResponse, InferRequest, InferResponse, InferenceTrace, WorkerInferRequest};
use crate::uds_client::{UdsClient, UdsClientError};
use adapteros_core::identity::IdentityEnvelope;
use adapteros_deterministic_exec::{ExecutorEvent, TaskId};
use axum::{extract::State, http::StatusCode, Extension, Json};

/// Inference endpoint
#[utoipa::path(
    post,
    path = "/v1/infer",
    request_body = InferRequest,
    responses(
        (
            status = 200,
            description = "Inference successful",
            body = InferResponse
        ),
        (
            status = 400,
            description = "Invalid request",
            body = ErrorResponse
        ),
        (
            status = 503,
            description = "Service unavailable",
            body = ErrorResponse
        ),
        (
            status = 408,
            description = "Request timeout",
            body = ErrorResponse
        ),
        (
            status = 500,
            description = "Inference failed",
            body = ErrorResponse
        )
    ),
    tag = "inference"
)]
pub async fn infer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(identity): Extension<IdentityEnvelope>,
    Json(req): Json<InferRequest>,
) -> Result<Json<InferResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator, SRE, and Admin can execute inference (Viewer and Compliance cannot)
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;

    // Validate request
    if req.prompt.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("prompt cannot be empty").with_code("INTERNAL_ERROR")),
        ));
    }

    // Audit log: inference execution start
    let adapters_requested = req
        .adapters
        .as_ref()
        .map(|a| a.join(","))
        .or_else(|| req.adapter_stack.as_ref().map(|s| s.join(",")));

    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::INFERENCE_EXECUTE,
        crate::audit_helper::resources::ADAPTER,
        adapters_requested.as_deref(),
    )
    .await;

    // Check UMA pressure - compare by string to avoid version conflicts between crates
    let pressure_str = state.uma_monitor.get_current_pressure().to_string();
    let is_high_pressure = pressure_str == "High" || pressure_str == "Critical";
    if is_high_pressure {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("service under memory pressure")
                    .with_code("BACKPRESSURE")
                    .with_string_details(format!(
                        "level={}, retry_after_secs=30, action=reduce max_tokens or retry later",
                        pressure_str
                    )),
            ),
        ));
    }

    // Real inference implementation - proxy to worker UDS server
    // 1. Look up available workers from database
    // 2. Select a healthy worker
    // 3. Connect to worker UDS server
    // 4. Forward inference request
    // 5. Return worker response

    // Get available workers from database
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list workers")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Resolve UDS path: prefer registered worker; otherwise fall back to local dev socket
    let uds_path_buf = if let Some(worker) = workers.get(0) {
        std::path::PathBuf::from(&worker.uds_path)
    } else {
        // Fallback: honor env override or default to /var/run/adapteros.sock
        let fallback = std::env::var("AOS_WORKER_SOCKET")
            .unwrap_or_else(|_| "/var/run/adapteros.sock".to_string());
        std::path::PathBuf::from(fallback)
    };
    let uds_path = uds_path_buf.as_path();

    // Create UDS client and send request
    let uds_client = UdsClient::new(std::time::Duration::from_secs(30));

    // Convert server API request to worker API request
    let worker_request = WorkerInferRequest {
        cpid: claims.tenant_id.clone(),
        prompt: req.prompt.clone(),
        max_tokens: req.max_tokens.unwrap_or(100),
        require_evidence: req.require_evidence.unwrap_or(false), // Get from request or default to false
    };

    // Record inference task spawned event to tick ledger
    let task_id = if let Some(ref ledger) = state.tick_ledger {
        // Generate a deterministic task ID from request ID
        let request_id_bytes = uuid::Uuid::new_v4().as_bytes().to_owned();
        let mut task_id_bytes = [0u8; 32];
        task_id_bytes[..16].copy_from_slice(&request_id_bytes);
        let task_id = TaskId::from_bytes(task_id_bytes);

        // Record task spawned event
        let spawn_event = ExecutorEvent::TaskSpawned {
            task_id,
            description: format!("inference: {}", &req.prompt[..req.prompt.len().min(50)]),
            tick: ledger.current_tick(),
            agent_id: Some(claims.tenant_id.clone()),
            hash: task_id_bytes,
        };

        if let Err(e) = ledger.record_tick(task_id, &spawn_event).await {
            tracing::warn!(
                error = %e,
                "Failed to record inference task spawn to tick ledger"
            );
        }

        Some(task_id)
    } else {
        None
    };

    match uds_client.infer(uds_path, worker_request).await {
        Ok(worker_response) => {
            // Record task completed event to tick ledger
            if let (Some(task_id), Some(ref ledger)) = (task_id, &state.tick_ledger) {
                let complete_event = ExecutorEvent::TaskCompleted {
                    task_id,
                    tick: ledger.current_tick(),
                    duration_ticks: 1, // Simplified - actual duration would be tracked
                    agent_id: Some(claims.tenant_id.clone()),
                    hash: task_id.as_bytes().to_owned(),
                };

                if let Err(e) = ledger.record_tick(task_id, &complete_event).await {
                    tracing::warn!(
                        error = %e,
                        "Failed to record inference task completion to tick ledger"
                    );
                }
            }

            // Convert worker response to server API response
            let response = InferResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                id: uuid::Uuid::new_v4().to_string(),
                text: worker_response.text.unwrap_or_default(),
                tokens: vec![],      // Worker doesn't expose token IDs in current API
                tokens_generated: 0, // Not tracked in current response
                finish_reason: worker_response.status.clone(),
                latency_ms: 0, // Not tracked in current response
                adapters_used: worker_response.trace.router_summary.adapters_used.clone(),
                trace: InferenceTrace {
                    adapters_used: worker_response.trace.router_summary.adapters_used.clone(),
                    router_decisions: vec![], // Router decisions not in simplified trace
                    latency_ms: 0,            // Not tracked in current response
                },
                model: None,
                prompt_tokens: None,
                error: None,
            };

            // Link session traces if session_id is provided
            if let Some(session_id) = &req.session_id {
                // Link adapters used
                for adapter_id in &response.adapters_used {
                    if let Err(e) = state
                        .db
                        .add_session_trace(session_id, "adapter", adapter_id)
                        .await
                    {
                        tracing::warn!(
                            session_id = %session_id,
                            adapter_id = %adapter_id,
                            error = %e,
                            "Failed to link adapter trace to session"
                        );
                    }
                }

                // Update session activity
                if let Err(e) = state.db.update_chat_session_activity(session_id).await {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %e,
                        "Failed to update session activity"
                    );
                }
            }

            // Validate response schema before returning
            let response_value = serde_json::to_value(&response).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("response serialization failed")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

            state
                .response_validator
                .validate_response(&response_value, "inference_response")
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("response validation failed")
                                .with_code("VALIDATION_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;

            Ok(Json(response))
        }
        Err(UdsClientError::WorkerNotAvailable(msg)) => {
            // Record task failed event to tick ledger
            if let (Some(task_id), Some(ref ledger)) = (task_id, &state.tick_ledger) {
                let failed_event = ExecutorEvent::TaskFailed {
                    task_id,
                    error: msg.clone(),
                    tick: ledger.current_tick(),
                    duration_ticks: 1,
                    agent_id: Some(claims.tenant_id.clone()),
                    hash: task_id.as_bytes().to_owned(),
                };

                if let Err(e) = ledger.record_tick(task_id, &failed_event).await {
                    tracing::warn!(
                        error = %e,
                        "Failed to record inference task failure to tick ledger"
                    );
                }
            }

            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new("worker not available")
                        .with_code("SERVICE_UNAVAILABLE")
                        .with_string_details(msg),
                ),
            ))
        }
        Err(UdsClientError::Timeout(msg)) => {
            // Record task timeout event to tick ledger
            if let (Some(task_id), Some(ref ledger)) = (task_id, &state.tick_ledger) {
                let timeout_event = ExecutorEvent::TaskTimeout {
                    task_id,
                    timeout_ticks: 1, // Simplified
                    tick: ledger.current_tick(),
                    agent_id: Some(claims.tenant_id.clone()),
                    hash: task_id.as_bytes().to_owned(),
                };

                if let Err(e) = ledger.record_tick(task_id, &timeout_event).await {
                    tracing::warn!(
                        error = %e,
                        "Failed to record inference task timeout to tick ledger"
                    );
                }
            }

            Err((
                StatusCode::REQUEST_TIMEOUT,
                Json(
                    ErrorResponse::new("inference timeout")
                        .with_code("REQUEST_TIMEOUT")
                        .with_string_details(msg),
                ),
            ))
        }
        Err(e) => {
            // Record task failed event to tick ledger
            if let (Some(task_id), Some(ref ledger)) = (task_id, &state.tick_ledger) {
                let failed_event = ExecutorEvent::TaskFailed {
                    task_id,
                    error: e.to_string(),
                    tick: ledger.current_tick(),
                    duration_ticks: 1,
                    agent_id: Some(claims.tenant_id.clone()),
                    hash: task_id.as_bytes().to_owned(),
                };

                if let Err(e) = ledger.record_tick(task_id, &failed_event).await {
                    tracing::warn!(
                        error = %e,
                        "Failed to record inference task failure to tick ledger"
                    );
                }
            }

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("inference failed")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ))
        }
    }
}
