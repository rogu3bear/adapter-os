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

use crate::api_error::{ApiError, ApiResult};
use crate::auth::{is_dev_bypass_enabled, Claims};
use crate::backpressure::check_uma_backpressure;
use crate::chat_context::build_chat_prompt;
use crate::inference_core::InferenceCore;
use crate::ip_extraction::ClientIp;
use crate::middleware::policy_enforcement::{
    compute_policy_mask_digest, create_hook_context, enforce_at_hook,
};
use crate::middleware::request_id::RequestId;
use crate::middleware::ApiKeyToken;
use crate::permissions::Permission;
use crate::session_tokens::{
    ensure_no_adapter_overrides, resolve_session_token_lock, SessionTokenContext,
};
use crate::state::AppState;
use crate::types::{
    new_run_envelope, set_policy_mask, ErrorResponse, InferRequest, InferResponse, InferenceError,
    InferenceRequestInternal, MAX_REPLAY_TEXT_SIZE,
};
use adapteros_api_types::inference::InferenceTrace;
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
    Extension(client_ip): Extension<ClientIp>,
    Extension(identity): Extension<IdentityEnvelope>,
    request_id: Option<Extension<RequestId>>,
    api_key: Option<Extension<ApiKeyToken>>,
    session_token: Option<Extension<SessionTokenContext>>,
    Json(req): Json<InferRequest>,
) -> ApiResult<InferResponse> {
    // Extract request_id for hook context
    let request_id_str = request_id
        .map(|r| r.0 .0.clone())
        .unwrap_or_else(crate::id_generator::readable_request_id);

    // Role check: Operator, SRE, and Admin can execute inference (Viewer and Compliance cannot)
    crate::permissions::require_permission(&claims, Permission::InferenceExecute)?;

    // Validate request
    if req.prompt.trim().is_empty() {
        return Err(ApiError::bad_request("prompt cannot be empty"));
    }

    let session_lock = if let Some(token) = session_token.as_ref() {
        ensure_no_adapter_overrides(&[
            ("adapters", req.adapters.is_some()),
            ("adapter_stack", req.adapter_stack.is_some()),
            ("stack_id", req.stack_id.is_some()),
            ("effective_adapter_ids", req.effective_adapter_ids.is_some()),
        ])?;
        let resolved = resolve_session_token_lock(&state, &claims, &token.0.lock).await?;
        if let (Some(requested), Some(locked)) = (req.backend, resolved.backend_profile) {
            if requested != locked {
                return Err(
                    ApiError::forbidden("session token backend mismatch").with_details(format!(
                        "requested {}, token {}",
                        requested.as_str(),
                        locked.as_str()
                    )),
                );
            }
        }
        if let (Some(requested), Some(locked)) = (req.coreml_mode, resolved.coreml_mode) {
            if requested != locked {
                return Err(ApiError::forbidden("session token coreml_mode mismatch")
                    .with_details(format!(
                        "requested {}, token {}",
                        requested.as_str(),
                        locked.as_str()
                    )));
            }
        }
        Some(resolved)
    } else {
        None
    };

    // Audit log: inference execution start
    let adapters_requested = req
        .adapters
        .as_ref()
        .map(|a| a.join(","))
        .or_else(|| req.adapter_stack.as_ref().map(|s| s.join(",")));
    let adapters_requested = session_lock
        .as_ref()
        .map(|lock| lock.adapter_ids.join(","))
        .or(adapters_requested);

    // Build audit metadata for UI display (adapter_id for "Adapter selected" label)
    let audit_metadata = serde_json::json!({
        "adapter_id": adapters_requested.as_deref().unwrap_or("none"),
        "request_id": &request_id_str,
    });

    if let Err(e) = crate::audit_helper::log_success_with_metadata(
        &state.db,
        &claims,
        crate::audit_helper::actions::INFERENCE_EXECUTE,
        crate::audit_helper::resources::ADAPTER,
        adapters_requested.as_deref(),
        audit_metadata,
        Some(client_ip.0.as_str()),
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
            let code = violation
                .code
                .clone()
                .unwrap_or_else(|| "POLICY_HOOK_VIOLATION".to_string());
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "POLICY_HOOK_VIOLATION",
                "policy hook violation (pre-routing)",
            )
            .with_code(code)
            .with_details(violation.message));
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
            let code = violation
                .code
                .clone()
                .unwrap_or_else(|| "POLICY_HOOK_VIOLATION".to_string());
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "POLICY_HOOK_VIOLATION",
                "policy hook violation",
            )
            .with_code(code)
            .with_details(violation.message));
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
    let (base_prompt, session_messages, chat_context_hash) =
        if let Some(ref session_id) = req.session_id {
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
                    (
                        result.prompt_text,
                        Some(result.messages),
                        Some(result.context_hash),
                    )
                }
                Err(e) => {
                    warn!(
                        request_id = %request_id_str,
                        tenant_id = %claims.tenant_id,
                        session_id = %session_id,
                        error = %e,
                        "Failed to build multi-turn prompt, using single-turn"
                    );
                    (req.prompt.clone(), None, None)
                }
            }
        } else {
            // No session, use prompt directly (single-turn)
            (req.prompt.clone(), None, None)
        };

    if base_prompt.len() > MAX_REPLAY_TEXT_SIZE {
        return Err(ApiError::bad_request("prompt too long for context window"));
    }

    // Convert to internal format with the (possibly multi-turn) prompt
    let mut internal = InferenceRequestInternal::from((&req, &claims));
    internal.request_id = request_id_str.clone();
    internal.prompt = base_prompt;
    // Prefer session-built messages (multi-turn) over request-level messages
    if let Some(msgs) = session_messages {
        internal.messages = Some(msgs);
    }
    internal.chat_context_hash = chat_context_hash;
    internal.policy_mask_digest_b3 = policy_mask_digest;
    internal.run_envelope = Some(new_run_envelope(
        &state,
        &claims,
        request_id_str.clone(),
        internal.reasoning_mode,
    ));
    if let (Some(ref mut envelope), Some(digest)) = (&mut internal.run_envelope, policy_mask_digest)
    {
        set_policy_mask(envelope, Some(&digest));
    }
    if let Some(token) = api_key {
        internal.worker_auth_token = Some(token.0 .0.clone());
    }
    if let Some(lock) = session_lock.as_ref() {
        internal.adapter_stack = None;
        internal.adapters = Some(lock.adapter_ids.clone());
        internal.effective_adapter_ids = Some(lock.adapter_ids.clone());
        internal.stack_id = lock.stack_id.clone();
        internal.pinned_adapter_ids = Some(lock.pinned_adapter_ids.clone());
        if let Some(backend) = lock.backend_profile {
            internal.backend_profile = Some(backend);
            internal.allow_fallback = backend == adapteros_core::BackendKind::Auto;
        }
        if let Some(coreml_mode) = lock.coreml_mode {
            internal.coreml_mode = Some(coreml_mode);
        }
    }

    // Execute via InferenceCore - this is the single entry point for all inference
    let cancel_token = CancellationToken::new();
    let mut cancel_guard = DispatchCancelGuard::new(cancel_token.clone());
    let state_for_task = state.clone();
    let inference_task = tokio::spawn(async move {
        let core = InferenceCore::new(&state_for_task);
        core.route_and_infer(internal, None, Some(cancel_token), None, None)
            .await
    });

    let inference_result = match inference_task.await {
        Ok(res) => res,
        Err(e) => {
            return Err(ApiError::internal("inference task join error").with_details(e.to_string()));
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
                return Err(<(StatusCode, Json<ErrorResponse>)>::from(e).into());
            }

            // Dev echo mode: when no worker is available in dev bypass mode,
            // return a mock echo response instead of failing
            if is_dev_bypass_enabled()
                && matches!(
                    e,
                    InferenceError::WorkerDegraded { .. }
                        | InferenceError::NoCompatibleWorker { .. }
                        | InferenceError::WorkerError(_)
                )
            {
                info!(
                    request_id = %request_id_str,
                    tenant_id = %claims.tenant_id,
                    error = %e,
                    "Dev echo mode: returning mock response (no worker available)"
                );
                let echo_text = format!(
                    "[DEV ECHO] No inference worker available. Your message was: {}",
                    req.prompt.chars().take(500).collect::<String>()
                );
                return Ok(Json(InferResponse {
                    schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                    id: request_id_str.clone(),
                    text: echo_text,
                    tokens: vec![],
                    tokens_generated: 0,
                    finish_reason: "dev_echo".to_string(),
                    latency_ms: 0,
                    adapters_used: vec![],
                    run_receipt: None,
                    deterministic_receipt: None,
                    run_envelope: None,
                    citations: vec![],
                    trace: InferenceTrace {
                        adapters_used: vec![],
                        router_decisions: vec![],
                        router_decision_chain: None,
                        fusion_intervals: None,
                        latency_ms: 0,
                        model_type: None,
                    },
                    model: None,
                    prompt_tokens: None,
                    error: None,
                    unavailable_pinned_adapters: None,
                    pinned_routing_fallback: None,
                    backend_used: Some("dev_echo".to_string()),
                    coreml_compute_preference: None,
                    coreml_compute_units: None,
                    coreml_gpu_used: None,
                    fallback_backend: None,
                    fallback_triggered: false,
                    determinism_mode_applied: None,
                    replay_guarantee: None,
                    stop_reason_code: None,
                    stop_reason_token_index: None,
                    stop_policy_digest_b3: None,
                }));
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

            // Log failure to audit trail with metadata for UI display
            let failure_metadata = serde_json::json!({
                "adapter_id": adapters_requested.as_deref().unwrap_or("none"),
                "request_id": &request_id_str,
                "cache_hit": false, // Failure - no cache interaction
            });
            if let Err(audit_err) = crate::audit_helper::log_failure_with_metadata(
                &state.db,
                &claims,
                crate::audit_helper::actions::INFERENCE_EXECUTE,
                crate::audit_helper::resources::ADAPTER,
                adapters_requested.as_deref(),
                &e.to_string(),
                failure_metadata,
                Some(client_ip.0.as_str()),
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

            if e.is_determinism_violation() {
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

            return Err(<(StatusCode, Json<ErrorResponse>)>::from(e).into());
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
        let code = violation
            .code
            .clone()
            .unwrap_or_else(|| "POLICY_HOOK_VIOLATION".to_string());
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "POLICY_HOOK_VIOLATION",
            "policy hook violation (post-inference)",
        )
        .with_code(code)
        .with_details(violation.message));
    }

    // AARA Lifecycle: Log inference decision for audit trail
    {
        let abstained = result.abstention.is_some();
        let abstention_reason = result
            .abstention
            .as_ref()
            .map(|a| format!("{:?}", a.reason));
        let max_gate = result
            .router_decisions
            .iter()
            .flat_map(|d| d.candidates.iter())
            .map(|c| c.raw_score)
            .fold(0.0_f32, f32::max);

        // Get candidate adapters from routing decisions
        let adapters_considered: Vec<String> = result
            .router_decisions
            .iter()
            .flat_map(|d| d.candidates.iter())
            .map(|c| format!("adapter_{}", c.adapter_idx))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if let Err(e) = state
            .db
            .log_inference_decision(
                &request_id_str,
                &claims.tenant_id,
                &claims.sub,
                &adapters_considered,
                &result.adapters_used,
                max_gate,
                abstained,
                abstention_reason.as_deref(),
                result.latency_ms,
            )
            .await
        {
            tracing::warn!(
                request_id = %request_id_str,
                error = %e,
                "Failed to log inference decision to audit trail"
            );
        }
    }

    // Convert result to API response format
    let response: InferResponse = result.into();

    // Validate response schema before returning
    let response_value = serde_json::to_value(&response).map_err(|e| {
        ApiError::internal("response serialization failed").with_details(e.to_string())
    })?;

    state
        .response_validator
        .validate_response(&response_value, "inference_response")
        .await
        .map_err(|e| {
            ApiError::internal("response validation failed").with_details(e.to_string())
        })?;

    Ok(Json(response))
}

// =============================================================================
// Provenance Endpoint (AUDIT)
// =============================================================================

/// Response for provenance chain query
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ProvenanceResponse {
    /// Inference trace ID
    pub trace_id: String,
    /// Tenant that owns this trace
    pub tenant_id: String,
    /// Request ID if available
    pub request_id: Option<String>,
    /// When the inference occurred
    pub created_at: Option<String>,
    /// Adapters that contributed to this inference
    pub adapters: Vec<AdapterProvenanceInfo>,
    /// Source documents traced back from adapters
    pub source_documents: Vec<DocumentProvenanceInfo>,
    /// Whether full provenance could be resolved
    pub is_complete: bool,
    /// Any warnings about missing provenance links
    pub warnings: Vec<String>,
    /// Total confidence score
    pub confidence: f32,
}

/// Adapter provenance in API response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct AdapterProvenanceInfo {
    /// Adapter ID
    pub adapter_id: String,
    /// Normalized gate value (0.0-1.0)
    pub gate: f32,
    /// Training job that created this adapter
    pub training_job_id: Option<String>,
    /// Dataset version used for training
    pub dataset_version_id: Option<String>,
}

/// Document provenance in API response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct DocumentProvenanceInfo {
    /// Source file path
    pub source_file: String,
    /// BLAKE3 content hash
    pub content_hash: String,
    /// Line range if known
    pub lines: Option<String>,
}

/// Get provenance chain for an inference trace
///
/// Traces the inference back through adapters to source documents,
/// enabling audit of which training data influenced the response.
#[utoipa::path(
    tag = "inference",
    get,
    path = "/v1/inference/{trace_id}/provenance",
    params(
        ("trace_id" = String, Path, description = "Inference trace ID to query provenance for")
    ),
    responses(
        (status = 200, description = "Provenance chain retrieved", body = ProvenanceResponse),
        (status = 404, description = "Trace not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn get_inference_provenance(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
) -> ApiResult<ProvenanceResponse> {
    // Permission check - allow audit/view access
    crate::permissions::require_permission(&claims, Permission::AuditView)?;

    // Get provenance chain from DB
    let chain = adapteros_db::inference_trace::get_provenance_chain(&state.db, &trace_id)
        .await
        .map_err(|e| {
            if e.to_string().contains("not found") {
                ApiError::not_found("Inference trace")
            } else {
                ApiError::db_error(e)
            }
        })?;

    // Convert to API response
    let confidence = chain.total_confidence();
    let response = ProvenanceResponse {
        trace_id: chain.trace_id,
        tenant_id: chain.tenant_id,
        request_id: chain.request_id,
        created_at: chain.created_at.map(|dt| dt.to_rfc3339()),
        adapters: chain
            .adapters_used
            .into_iter()
            .map(|a| AdapterProvenanceInfo {
                adapter_id: a.adapter_id,
                gate: a.gate_normalized,
                training_job_id: a.training_job_id,
                dataset_version_id: a.dataset_version_id,
            })
            .collect(),
        source_documents: chain
            .source_documents
            .into_iter()
            .map(|d| {
                let lines = match (d.line_start, d.line_end) {
                    (Some(s), Some(e)) => Some(format!("{}-{}", s, e)),
                    (Some(s), None) => Some(format!("{}+", s)),
                    _ => None,
                };
                DocumentProvenanceInfo {
                    source_file: d.source_file,
                    content_hash: d.source_hash_b3,
                    lines,
                }
            })
            .collect(),
        is_complete: chain.is_complete,
        warnings: chain.warnings,
        confidence,
    };

    Ok(Json(response))
}
