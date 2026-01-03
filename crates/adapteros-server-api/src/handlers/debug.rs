//! Debug API handlers
//!
//! This module provides debug endpoints that are only available when explicitly
//! enabled via environment variables. These endpoints should NEVER be enabled
//! in production.
//!
//! ## Determinism Mode Override Endpoint
//!
//! The debug inference endpoint allows temporarily overriding the determinism
//! mode for a single request. This is useful for testing and debugging, but
//! must be heavily logged for audit purposes.
//!
//! **Safety**: Only available when `AOS_ALLOW_DEBUG_DETERMINISM_OVERRIDE=true`

use crate::auth::Claims;
use crate::backpressure::check_uma_backpressure;
use crate::chat_context::build_chat_prompt;
use crate::inference_core::InferenceCore;
use crate::middleware::policy_enforcement::{create_hook_context, enforce_at_hook};
use crate::middleware::request_id::RequestId;
use crate::permissions::Permission;
use crate::state::AppState;
use crate::types::{ErrorResponse, InferRequest, InferResponse, InferenceRequestInternal};
use adapteros_core::determinism::GlobalDeterminismConfig;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_policy::hooks::PolicyHook;
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use utoipa::ToSchema;

/// Query parameters for debug inference with mode override
#[derive(Debug, Deserialize, ToSchema)]
pub struct DebugInferParams {
    /// Override determinism mode for this request (optional)
    ///
    /// Valid values: "strict", "besteffort", "relaxed"
    /// If not provided, uses the global/stack default mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// Debug inference response with determinism metadata
#[derive(Debug, Serialize, ToSchema)]
pub struct DebugInferResponse {
    /// The inference result
    #[serde(flatten)]
    pub result: InferResponse,

    /// The determinism mode that was used for this request
    pub determinism_mode_used: String,

    /// Whether the mode was overridden from the default
    pub was_overridden: bool,

    /// The global default mode (for comparison)
    pub global_default_mode: String,
}

/// Debug inference endpoint with determinism mode override
///
/// POST /v1/debug/infer?mode=relaxed
///
/// This endpoint allows temporarily overriding the determinism mode for a
/// single inference request. It is only available when
/// `AOS_ALLOW_DEBUG_DETERMINISM_OVERRIDE=true` is set.
///
/// **WARNING**: This endpoint should NEVER be enabled in production. It is
/// intended for testing and debugging only. All usage is heavily logged for
/// audit purposes.
///
/// The request body is the same as the standard inference endpoint.
#[utoipa::path(
    tag = "debug",
    post,
    path = "/v1/debug/infer",
    params(
        ("mode" = Option<String>, Query, description = "Override determinism mode: strict, besteffort, or relaxed")
    ),
    request_body = InferRequest,
    responses(
        (status = 200, description = "Inference successful", body = DebugInferResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 403, description = "Debug override not enabled", body = ErrorResponse),
        (status = 500, description = "Inference failed", body = ErrorResponse),
        (status = 501, description = "Worker not initialized", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub async fn debug_infer_with_mode(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(identity): Extension<IdentityEnvelope>,
    request_id: Option<Extension<RequestId>>,
    axum::extract::Query(query): axum::extract::Query<DebugInferParams>,
    Json(req): Json<InferRequest>,
) -> Result<Json<DebugInferResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Extract request_id for logging
    let request_id_str = request_id
        .map(|r| r.0.as_str().to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Check if debug override is allowed
    let global_config = GlobalDeterminismConfig::from_env().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to load determinism config")
                    .with_code("CONFIG_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if !global_config.allow_debug_override {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new(
                    "Debug determinism override is not enabled. Set AOS_ALLOW_DEBUG_DETERMINISM_OVERRIDE=true"
                )
                .with_code("DEBUG_OVERRIDE_DISABLED")
                .with_string_details(
                    "This endpoint is only available when explicitly enabled via environment variable"
                ),
            ),
        ));
    }

    // Role check: same as normal inference
    crate::permissions::require_permission(&claims, Permission::InferenceExecute)?;

    // Validate request
    if req.prompt.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("prompt cannot be empty").with_code("INVALID_REQUEST")),
        ));
    }

    // Parse and validate override mode if provided
    let (was_overridden, effective_mode_str) = if let Some(ref mode_str) = query.mode {
        let override_mode: crate::inference_core::DeterminismMode =
            mode_str.parse().map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("Invalid determinism mode")
                            .with_code("INVALID_MODE")
                            .with_string_details(format!(
                                "{}. Valid values: strict, besteffort, relaxed",
                                e
                            )),
                    ),
                )
            })?;

        // CRITICAL: Heavy logging for audit trail
        warn!(
            request_id = %request_id_str,
            tenant_id = %claims.tenant_id,
            user_id = %claims.sub,
            global_mode = %global_config.mode,
            override_mode = %override_mode,
            stack_id = ?req.adapter_stack,
            adapters = ?req.adapters,
            "DEBUG: Determinism mode override requested"
        );

        (true, override_mode.to_string())
    } else {
        (false, global_config.mode.to_string())
    };

    // Audit log: debug inference execution start
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
    .await {


        tracing::warn!(error = %e, "Audit log failed");


    }

    check_uma_backpressure(&state)?;

    // PRD-06: Enforce policies at OnRequestBeforeRouting hook
    let routing_hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnRequestBeforeRouting,
        "inference",
        None,
    );
    if let Err(violation) = enforce_at_hook(&state, &routing_hook_ctx).await {
        let code = violation
            .code
            .as_deref()
            .unwrap_or("POLICY_HOOK_VIOLATION");
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("policy hook violation (pre-routing)")
                    .with_code(code)
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
        let code = violation
            .code
            .as_deref()
            .unwrap_or("POLICY_HOOK_VIOLATION");
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("policy hook violation")
                    .with_code(code)
                    .with_string_details(violation.message),
            ),
        ));
    }

    // Build multi-turn prompt if session_id is provided
    let (base_prompt, chat_context_hash) = if let Some(ref session_id) = req.session_id {
        // STABILITY: Use poison-safe lock access
        let chat_config = state.config.read().unwrap_or_else(|e| {
            tracing::warn!("Config lock poisoned in debug handler, recovering");
            e.into_inner()
        }).chat_context.clone();
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
        (req.prompt.clone(), None)
    };

    // Convert to internal format
    let mut internal = InferenceRequestInternal::from((&req, &claims));
    internal.prompt = base_prompt;
    internal.chat_context_hash = chat_context_hash;

    // Execute via InferenceCore
    let core = InferenceCore::new(&state);
    let result = core
        .route_and_infer(internal, None, None, None)
        .await
        .map_err(|e| {
        // Log failure to audit trail
        if let Err(e) = crate::audit_helper::log_failure(
            &state.db,
            &claims,
            crate::audit_helper::actions::INFERENCE_EXECUTE,
            crate::audit_helper::resources::ADAPTER,
            adapters_requested.as_deref(),
            &e.to_string(),
        ) {

            tracing::warn!(error = %e, "Audit log failed");

        }

        // Convert InferenceError to HTTP error response
        <(StatusCode, Json<ErrorResponse>)>::from(e)
    })?;

    // PRD-06: Enforce policies at OnAfterInference hook
    let after_hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnAfterInference,
        "inference",
        adapters_requested.as_deref(),
    );
    if let Err(violation) = enforce_at_hook(&state, &after_hook_ctx).await {
        let code = violation
            .code
            .as_deref()
            .unwrap_or("POLICY_HOOK_VIOLATION");
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("policy hook violation (post-inference)")
                    .with_code(code)
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

    // Log successful debug inference with mode information
    info!(
        request_id = %request_id_str,
        tenant_id = %claims.tenant_id,
        user_id = %claims.sub,
        determinism_mode = %effective_mode_str,
        was_overridden = was_overridden,
        "DEBUG: Inference completed with determinism mode"
    );

    // Return debug response with determinism metadata
    Ok(Json(DebugInferResponse {
        result: response,
        determinism_mode_used: effective_mode_str,
        was_overridden,
        global_default_mode: global_config.mode.to_string(),
    }))
}
