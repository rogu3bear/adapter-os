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
use crate::chat_context::build_chat_prompt;
use crate::inference_core::InferenceCore;
use crate::middleware::policy_enforcement::{create_hook_context, enforce_at_hook};
use crate::middleware::request_id::RequestId;
use crate::middleware::ApiKeyToken;
use crate::permissions::Permission;
use crate::state::AppState;
use crate::types::{ErrorResponse, InferRequest, InferResponse, InferenceRequestInternal};
use adapteros_core::identity::IdentityEnvelope;
use adapteros_policy::hooks::PolicyHook;
use axum::{extract::State, http::StatusCode, Extension, Json};
use tracing::{info, warn};

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

    // PRD-06: Enforce policies at OnRequestBeforeRouting hook (before adapter selection)
    let routing_hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnRequestBeforeRouting,
        "inference",
        None, // No adapter selected yet
    );
    if let Err(violation) = enforce_at_hook(&state, &routing_hook_ctx).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("policy hook violation (pre-routing)")
                    .with_code("POLICY_HOOK_VIOLATION")
                    .with_string_details(violation.message),
            ),
        ));
    }

    // PRD-06: Enforce policies at OnBeforeInference hook
    let hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnBeforeInference,
        "inference",
        adapters_requested.as_deref(),
    );
    if let Err(violation) = enforce_at_hook(&state, &hook_ctx).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("policy hook violation")
                    .with_code("POLICY_HOOK_VIOLATION")
                    .with_string_details(violation.message),
            ),
        ));
    }

    // Build multi-turn prompt if session_id is provided
    // This loads chat history and formats it with role markers for context
    let (base_prompt, chat_context_hash) = if let Some(ref session_id) = req.session_id {
        let chat_config = state.config.read().unwrap().chat_context.clone();
        match build_chat_prompt(&state.db, session_id, &req.prompt, &chat_config).await {
            Ok(result) => {
                info!(
                    request_id = %request_id_str,
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
    if let Some(token) = api_key {
        internal.worker_auth_token = Some(token.0 .0.clone());
    }

    // Execute via InferenceCore - this is the single entry point for all inference
    let core = InferenceCore::new(&state);
    let result = core.route_and_infer(internal, None).await.map_err(|e| {
        // Log failure to audit trail
        let _ = crate::audit_helper::log_failure(
            &state.db,
            &claims,
            crate::audit_helper::actions::INFERENCE_EXECUTE,
            crate::audit_helper::resources::ADAPTER,
            adapters_requested.as_deref(),
            &e.to_string(),
        );
        // Don't await the audit log, just fire and forget

        // Convert InferenceError to HTTP error response
        <(StatusCode, Json<ErrorResponse>)>::from(e)
    })?;

    // PRD-06: Enforce policies at OnAfterInference hook (e.g., Evidence, Refusal)
    let after_hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnAfterInference,
        "inference",
        adapters_requested.as_deref(),
    );
    if let Err(violation) = enforce_at_hook(&state, &after_hook_ctx).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("policy hook violation (post-inference)")
                    .with_code("POLICY_HOOK_VIOLATION")
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
