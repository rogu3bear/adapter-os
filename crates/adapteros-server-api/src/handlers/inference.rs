//! Inference endpoint handler
//!
//! This module handles inference requests by proxying them to InferenceCore.
//! It includes:
//! - Permission validation
//! - Memory pressure checks
//! - Audit logging
//! - Response validation
//!
//! All inference execution is routed through InferenceCore for unified handling.

use crate::auth::Claims;
use crate::backpressure::check_uma_backpressure;
use crate::chat_context::build_chat_prompt;
use crate::inference_core::InferenceCore;
use crate::middleware::policy_enforcement::{
    compute_policy_mask_digest, create_hook_context, enforce_at_hook,
};
use crate::middleware::request_id::RequestId;
use crate::middleware::ApiKeyToken;
use crate::permissions::Permission;
use crate::state::AppState;
use crate::types::{
    ErrorResponse, InferRequest, InferResponse, InferenceError, InferenceRequestInternal,
};
use adapteros_api_types::FailureCode;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::telemetry::{
    determinism_violation_event, emit_observability_event, STRICT_DETERMINISM_METRIC,
};
use adapteros_core::DeterminismViolationKind;
use adapteros_policy::hooks::PolicyHook;
use axum::{extract::State, http::StatusCode, Extension, Json};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

struct DispatchCancelGuard {
    token: CancellationToken,
    armed: bool,
}

impl DispatchCancelGuard {
    fn new(token: CancellationToken) -> Self {
        Self { token, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for DispatchCancelGuard {
    fn drop(&mut self) {
        if self.armed {
            self.token.cancel();
        }
    }
}

/// Inference endpoint
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/infer",
    request_body = InferRequest,
    responses(
        (status = 200, description = "Inference successful", body = InferResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Inference failed", body = ErrorResponse),
        (status = 501, description = "Worker not initialized", body = ErrorResponse)
    )
)]
pub async fn infer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(identity): Extension<IdentityEnvelope>,
    request_id: Option<Extension<RequestId>>,
    api_key: Option<Extension<ApiKeyToken>>,
    Json(req): Json<InferRequest>,
) -> Result<Json<InferResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Extract request_id for hook context
    let request_id_str = request_id
        .map(|r| r.0 .0.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Role check: Operator, SRE, and Admin can execute inference (Viewer and Compliance cannot)
    crate::permissions::require_permission(&claims, Permission::InferenceExecute)?;

    // Validate request
    if req.prompt.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("prompt cannot be empty").with_code("BAD_REQUEST")),
        ));
    }

    // Audit log: inference execution start
    let adapters_requested = req
        .adapters
        .as_ref()
        .map(|a| a.join(","))
        .or_else(|| req.adapter_stack.as_ref().map(|s| s.join(",")));

    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::INFERENCE_EXECUTE,
        crate::audit_helper::resources::ADAPTER,
        adapters_requested.as_deref(),
    )
    .await
    {
        tracing::warn!(
            request_id = %request_id_str,
            tenant_id = %claims.tenant_id,
            error = %e,
            "Audit log failed"
        );
    }

    check_uma_backpressure(&state)?;

    // Validate tenant isolation if tenant_id provided in request
    if let Some(ref tenant_id) = req.tenant_id {
        crate::security::validate_tenant_isolation(&claims, tenant_id)?;
    }

    // PRD-06: Enforce policies at OnRequestBeforeRouting hook (before adapter selection)
    // Capture policy decisions for digest computation
    let routing_hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnRequestBeforeRouting,
        "inference",
        None, // No adapter selected yet
    );
    let routing_decisions = match enforce_at_hook(&state, &routing_hook_ctx).await {
        Ok(decisions) => decisions,
        Err(violation) => {
            let code = violation.code.as_deref().unwrap_or("POLICY_HOOK_VIOLATION");
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("policy hook violation (pre-routing)")
                        .with_code(code)
                        .with_failure_code(FailureCode::PolicyDivergence)
                        .with_string_details(violation.message),
                ),
            ));
        }
    };

    // PRD-06: Enforce policies at OnBeforeInference hook
    let hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnBeforeInference,
        "inference",
        adapters_requested.as_deref(),
    );
    let inference_decisions = match enforce_at_hook(&state, &hook_ctx).await {
        Ok(decisions) => decisions,
        Err(violation) => {
            let code = violation.code.as_deref().unwrap_or("POLICY_HOOK_VIOLATION");
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("policy hook violation")
                        .with_code(code)
                        .with_failure_code(FailureCode::PolicyDivergence)
                        .with_string_details(violation.message),
                ),
            ));
        }
    };

    // Compute policy mask digest from all policy decisions
    // This enables deterministic replay verification
    let mut all_decisions = routing_decisions;
    all_decisions.extend(inference_decisions);
    let policy_mask_digest = if all_decisions.is_empty() {
        None
    } else {
        Some(compute_policy_mask_digest(&all_decisions))
    };

    // Build multi-turn prompt if session_id is provided
    // This loads chat history and formats it with role markers for context
    let (base_prompt, chat_context_hash) = if let Some(ref session_id) = req.session_id {
        // STABILITY: Use poison-safe lock access
        let chat_config = state
            .config
            .read()
            .unwrap_or_else(|e| {
                tracing::warn!("Config lock poisoned in inference, recovering");
                e.into_inner()
            })
            .chat_context
            .clone();
        match build_chat_prompt(&state.db, session_id, &req.prompt, &chat_config).await {
            Ok(result) => {
                info!(
                    request_id = %request_id_str,
                    tenant_id = %claims.tenant_id,
                    session_id = %session_id,
                    message_count = result.message_count,
                    truncated = result.truncated,
                    context_hash = %result.context_hash,
                    "Built multi-turn prompt from session history"
                );
                (result.prompt_text, Some(result.context_hash))
            }
            Err(e) => {
                warn!(
                    request_id = %request_id_str,
                    tenant_id = %claims.tenant_id,
                    session_id = %session_id,
                    error = %e,
                    "Failed to build multi-turn prompt, using single-turn"
                );
                (req.prompt.clone(), None)
            }
        }
    } else {
        // No session, use prompt directly (single-turn)
        (req.prompt.clone(), None)
    };

    // Convert to internal format with the (possibly multi-turn) prompt
    let mut internal = InferenceRequestInternal::from((&req, &claims));
    internal.prompt = base_prompt;
    internal.chat_context_hash = chat_context_hash;
    internal.policy_mask_digest = policy_mask_digest;
    if let Some(token) = api_key {
        internal.worker_auth_token = Some(token.0 .0.clone());
    }

    // Execute via InferenceCore - this is the single entry point for all inference
    let cancel_token = CancellationToken::new();
    let mut cancel_guard = DispatchCancelGuard::new(cancel_token.clone());
    let state_for_task = state.clone();
    let inference_task = tokio::spawn(async move {
        let core = InferenceCore::new(&state_for_task);
        core.route_and_infer(internal, None, Some(cancel_token))
            .await
    });

    let inference_result = match inference_task.await {
        Ok(res) => res,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("inference task join error")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };
    cancel_guard.disarm();

    let result = match inference_result {
        Ok(result) => result,
        Err(e) => {
            if matches!(e, InferenceError::ClientClosed(_)) {
                warn!(
                    request_id = %request_id_str,
                    tenant_id = %claims.tenant_id,
                    "Inference cancelled due to client disconnect"
                );
                return Err(<(StatusCode, Json<ErrorResponse>)>::from(e));
            }

            let failure_code = e.failure_code().map(|c| c.as_str().to_string());
            tracing::error!(
                target: "inference",
                code = %failure_code.as_deref().unwrap_or("INTERNAL_ERROR"),
                request_id = %request_id_str,
                tenant_id = %claims.tenant_id,
                error = %e,
                "Inference failed"
            );

            // Log failure to audit trail
            if let Err(audit_err) = crate::audit_helper::log_failure(
                &state.db,
                &claims,
                crate::audit_helper::actions::INFERENCE_EXECUTE,
                crate::audit_helper::resources::ADAPTER,
                adapters_requested.as_deref(),
                &e.to_string(),
            )
            .await
            {
                tracing::warn!(
                    request_id = %request_id_str,
                    tenant_id = %claims.tenant_id,
                    error = %audit_err,
                    "Audit log failed"
                );
            }

            if matches!(
                e,
                InferenceError::WorkerError(_) | InferenceError::RoutingBypass(_)
            ) {
                let msg = e.to_string();
                if msg.contains("DeterminismViolation") || msg.contains("strict") {
                    let event = determinism_violation_event(
                        DeterminismViolationKind::Unknown,
                        None,
                        None,
                        None,
                        true,
                        Some(claims.tenant_id.clone()),
                        Some(request_id_str.clone()),
                    );
                    emit_observability_event(&event);
                    let metrics_registry = state.metrics_registry.clone();
                    tokio::spawn(async move {
                        let _ = metrics_registry
                            .record_metric(STRICT_DETERMINISM_METRIC.to_string(), 1.0)
                            .await;
                    });
                }
            }

            return Err(<(StatusCode, Json<ErrorResponse>)>::from(e));
        }
    };

    // PRD-06: Enforce policies at OnAfterInference hook (e.g., Evidence, Refusal)
    let after_hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnAfterInference,
        "inference",
        adapters_requested.as_deref(),
    );
    if let Err(violation) = enforce_at_hook(&state, &after_hook_ctx).await {
        let code = violation.code.as_deref().unwrap_or("POLICY_HOOK_VIOLATION");
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("policy hook violation (post-inference)")
                    .with_code(code)
                    .with_failure_code(FailureCode::PolicyDivergence)
                    .with_string_details(violation.message),
            ),
        ));
    }

    // Convert result to API response format
    let response: InferResponse = result.into();

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
