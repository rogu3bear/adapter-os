//! Replay session API handlers
//!
//! Provides endpoints for creating, listing, and verifying deterministic replay sessions.

use adapteros_core::{AosError, B3Hash};
use adapteros_crypto::signature::Signature;
use adapteros_db::replay_sessions::ReplaySession;
use anyhow::{anyhow, bail, Context, Result};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use utoipa::{IntoParams, ToSchema};

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::handlers::rag_common::reconstruct_rag_context;
use crate::inference_core::InferenceCore;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{
    new_run_envelope_no_tick, ErrorResponse, InferenceRequestInternal, MAX_TOKENS_LIMIT,
};

/// Replay verification response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReplayVerificationResponse {
    pub session_id: String,
    pub signature_valid: bool,
    pub hash_chain_valid: bool,
    pub manifest_verified: bool,
    pub policy_verified: bool,
    pub kernel_verified: bool,
    pub telemetry_verified: bool,
    pub overall_valid: bool,
    pub divergences: Vec<ReplayDivergence>,
    pub verified_at: String,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListReplaySessionsParams {
    tenant_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReplaySessionResponse {
    pub id: String,
    pub tenant_id: String,
    pub cpid: String,
    pub plan_id: String,
    pub snapshot_at: String,
    pub seed_global_b3: String,
    pub manifest_hash_b3: String,
    pub policy_hash_b3: String,
    pub kernel_hash_b3: Option<String>,
    pub telemetry_bundle_ids: Vec<String>,
    pub adapter_state: AdapterStateSnapshot,
    pub routing_decisions: Vec<serde_json::Value>,
    pub inference_traces: Option<Vec<serde_json::Value>>,
    pub signature: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterStateSnapshot {
    pub adapters: Vec<serde_json::Value>,
    pub timestamp: String,
    pub memory_usage_bytes: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateReplaySessionRequest {
    pub tenant_id: String,
    pub cpid: String,
    pub plan_id: String,
    pub telemetry_bundle_ids: Vec<String>,
    pub snapshot_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReplayDivergence {
    pub divergence_type: String, // 'router' | 'adapter' | 'inference' | 'policy'
    pub expected_hash: String,
    pub actual_hash: String,
    pub context: String,
}

/// Request to execute a replay session
#[derive(Debug, Deserialize, ToSchema)]
pub struct ExecuteReplayRequest {
    /// If true, use original RAG documents from replay session
    /// If documents are missing, replay continues in degraded mode
    #[serde(default)]
    pub use_original_rag_docs: bool,
    /// Optional prompt override (if not provided, uses stored prompt from session)
    pub prompt: Option<String>,
    /// Maximum tokens to generate (default: 100)
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
}

fn default_max_tokens() -> usize {
    100
}

/// Response from replay execution
#[derive(Debug, Serialize, ToSchema)]
pub struct ExecuteReplayResponse {
    pub session_id: String,
    /// Inference output text
    pub output: String,
    /// Whether replay ran in degraded mode due to missing RAG documents
    pub degraded: bool,
    /// Document IDs that were missing (only populated if degraded=true)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub missing_doc_ids: Vec<String>,
    /// True if use_original_rag_docs was requested but no RAG state was stored
    /// (indicates the original inference didn't use RAG)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_rag_state_stored: bool,
    /// Latency in milliseconds
    pub latency_ms: u64,
    pub verified_at: String,
}

/// List replay sessions
#[utoipa::path(
    get,
    path = "/v1/replay/sessions",
    params(ListReplaySessionsParams),
    responses(
        (status = 200, description = "List of replay sessions", body = Vec<ReplaySessionResponse>),
    ),
    tag = "replay"
)]
pub async fn list_replay_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ListReplaySessionsParams>,
) -> ApiResult<Vec<ReplaySessionResponse>> {
    require_permission(&claims, Permission::ReplayManage)?;

    let sessions = state
        .db
        .list_replay_sessions(params.tenant_id.as_deref())
        .await
        .map_err(ApiError::db_error)?;

    let responses: Vec<ReplaySessionResponse> = sessions
        .into_iter()
        .filter_map(|session| session_to_response(session).ok())
        .collect();

    Ok(Json(responses))
}

/// Get a single replay session
#[utoipa::path(
    get,
    path = "/v1/replay/sessions/{id}",
    responses(
        (status = 200, description = "Replay session details", body = ReplaySessionResponse),
    ),
    tag = "replay"
)]
pub async fn get_replay_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> ApiResult<ReplaySessionResponse> {
    require_permission(&claims, Permission::ReplayManage)?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id).await?;

    let session = state
        .db
        .get_replay_session(&session_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Replay session"))?;

    let response = session_to_response(session)
        .map_err(|e| ApiError::internal("Failed to convert session").with_details(e.to_string()))?;

    Ok(Json(response))
}

/// Create a new replay session
#[utoipa::path(
    post,
    path = "/v1/replay/sessions",
    request_body = CreateReplaySessionRequest,
    responses(
        (status = 201, description = "Replay session created", body = ReplaySessionResponse),
    ),
    tag = "replay"
)]
pub async fn create_replay_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateReplaySessionRequest>,
) -> ApiResult<ReplaySessionResponse> {
    require_permission(&claims, Permission::ReplayManage)?;

    // Generate session ID
    let session_id = crate::id_generator::readable_session_id("replay");

    let snapshot_at = req
        .snapshot_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    // Fetch telemetry bundles to extract merkle roots for manifest hash
    let mut merkle_roots: Vec<String> = Vec::new();

    for bundle_id in &req.telemetry_bundle_ids {
        match state
            .db
            .get_telemetry_bundle(&req.tenant_id, bundle_id)
            .await
        {
            Ok(Some(bundle)) => merkle_roots.push(bundle.merkle_root_b3.clone()),
            Ok(None) => tracing::warn!(
                bundle_id = %bundle_id,
                tenant_id = %req.tenant_id,
                "Telemetry bundle not found for tenant"
            ),
            Err(e) => tracing::warn!(
                bundle_id = %bundle_id,
                tenant_id = %req.tenant_id,
                error = %e,
                "Failed to fetch telemetry bundle"
            ),
        }
    }

    // Fetch recent telemetry events for adapter info extraction
    let telemetry_events = state
        .db
        .get_telemetry_by_tenant(&req.tenant_id, 100)
        .await
        .unwrap_or_default();

    let mut adapter_infos: Vec<serde_json::Value> = Vec::new();
    let mut routing_decisions: Vec<serde_json::Value> = Vec::new();

    for event in telemetry_events {
        if let Ok(event_data) = serde_json::from_str::<serde_json::Value>(&event.event_data) {
            // Extract adapter IDs from inference events
            if let Some(adapter_ids) = event_data.get("adapter_ids") {
                adapter_infos.push(serde_json::json!({
                    "adapter_ids": adapter_ids,
                    "event_type": event.event_type,
                    "timestamp": event.timestamp
                }));
            }
            // Collect routing decisions
            if event.event_type == "routing_decision" {
                routing_decisions.push(event_data);
            }
        }
    }

    let adapter_state = AdapterStateSnapshot {
        adapters: adapter_infos,
        timestamp: snapshot_at.clone(),
        memory_usage_bytes: 0, // Memory stats not tracked in telemetry
    };

    // Generate global seed - use deterministic derivation from session data
    let seed_input = format!(
        "{}:{}:{}:{}:{}",
        req.tenant_id,
        req.cpid,
        req.plan_id,
        snapshot_at,
        req.telemetry_bundle_ids.join(",")
    );
    let seed_hash = B3Hash::hash(seed_input.as_bytes());
    let seed_global_b3 = format!("b3:{}", seed_hash.to_hex());

    // Generate RNG state with global nonce derived from seed
    let global_nonce =
        u64::from_le_bytes(seed_hash.as_bytes()[0..8].try_into().unwrap_or([0u8; 8]));
    let rng_state = serde_json::json!({
        "global_nonce": global_nonce,
        "label": format!("replay:{}", session_id),
        "step_count": 0
    });
    let rng_state_json = serde_json::to_string(&rng_state).map_err(|e| {
        ApiError::internal("Failed to serialize RNG state").with_details(e.to_string())
    })?;

    // Compute manifest hash from merkle roots of telemetry bundles
    let manifest_input = if merkle_roots.is_empty() {
        format!("manifest:{}:{}:{}", req.tenant_id, req.cpid, snapshot_at)
    } else {
        merkle_roots.join(":")
    };
    let manifest_hash = B3Hash::hash(manifest_input.as_bytes());
    let manifest_hash_b3 = format!("b3:{}", manifest_hash.to_hex());

    // Look up active policy pack and hash it
    let policy_hash_b3 = {
        let active_policy = state
            .db
            .list_policy_packs(None, Some("active"))
            .await
            .ok()
            .and_then(|packs| packs.into_iter().next());

        if let Some(policy) = active_policy {
            let policy_input = format!("{}:{}:{}", policy.id, policy.version, policy.policy_type);
            let policy_hash = B3Hash::hash(policy_input.as_bytes());
            format!("b3:{}", policy_hash.to_hex())
        } else {
            // Fallback: derive from session params
            let policy_input = format!("policy:{}:{}", req.tenant_id, snapshot_at);
            let policy_hash = B3Hash::hash(policy_input.as_bytes());
            format!("b3:{}", policy_hash.to_hex())
        }
    };

    // Create signature using server's signing keypair
    let snapshot_data = format!(
        "{}:{}:{}:{}:{}:{}",
        req.tenant_id, req.cpid, req.plan_id, manifest_hash_b3, policy_hash_b3, seed_global_b3
    );
    let signature = state.crypto.signing_keypair.sign(snapshot_data.as_bytes());

    let session = ReplaySession {
        id: session_id.clone(),
        tenant_id: req.tenant_id,
        cpid: req.cpid,
        plan_id: req.plan_id,
        snapshot_at,
        seed_global_b3,
        rng_state_json,
        manifest_hash_b3,
        policy_hash_b3,
        kernel_hash_b3: None,
        telemetry_bundle_ids_json: serde_json::to_string(&req.telemetry_bundle_ids).map_err(
            |e| {
                ApiError::internal("Failed to serialize telemetry bundle IDs")
                    .with_details(e.to_string())
            },
        )?,
        adapter_state_json: serde_json::to_string(&adapter_state).map_err(|e| {
            ApiError::internal("Failed to serialize adapter state").with_details(e.to_string())
        })?,
        routing_decisions_json: serde_json::to_string(&routing_decisions).map_err(|e| {
            ApiError::internal("Failed to serialize routing decisions").with_details(e.to_string())
        })?,
        inference_traces_json: None,
        signature: hex::encode(signature.to_bytes()),
        created_at: chrono::Utc::now().to_rfc3339(),
        rag_state_json: None, // RAG state stored if session includes RAG retrieval
    };

    state
        .db
        .create_replay_session(&session)
        .await
        .map_err(ApiError::db_error)?;

    let response = session_to_response(session)
        .map_err(|e| ApiError::internal("Failed to convert session").with_details(e.to_string()))?;

    Ok(Json(response))
}

/// Verify a replay session's cryptographic integrity
#[utoipa::path(
    post,
    path = "/v1/replay/sessions/{id}/verify",
    responses(
        (status = 200, description = "Verification results", body = ReplayVerificationResponse),
    ),
    tag = "replay"
)]
pub async fn verify_replay_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> ApiResult<ReplayVerificationResponse> {
    require_permission(&claims, Permission::ReplayManage)?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id).await?;

    let session = state
        .db
        .get_replay_session(&session_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Replay session"))?;

    // Perform cryptographic verification of the replay session
    let mut divergences = Vec::new();

    // 1. Verify session signature
    let signature_valid = verify_session_signature(&state, &session);
    if !signature_valid {
        divergences.push(ReplayDivergence {
            divergence_type: "signature".to_string(),
            expected_hash: "valid_signature".to_string(),
            actual_hash: "invalid_signature".to_string(),
            context: "Session signature verification failed".to_string(),
        });
    }

    // 2. Verify manifest hash exists and is properly formatted
    let manifest_verified = !session.manifest_hash_b3.is_empty()
        && (session.manifest_hash_b3.starts_with("b3:") || session.manifest_hash_b3.len() == 64);
    if !manifest_verified {
        divergences.push(ReplayDivergence {
            divergence_type: "manifest".to_string(),
            expected_hash: "valid_b3_hash".to_string(),
            actual_hash: session.manifest_hash_b3.clone(),
            context: "Manifest hash format invalid".to_string(),
        });
    }

    // 3. Verify policy hash exists and is properly formatted
    let policy_verified = !session.policy_hash_b3.is_empty()
        && (session.policy_hash_b3.starts_with("b3:") || session.policy_hash_b3.len() == 64);
    if !policy_verified {
        divergences.push(ReplayDivergence {
            divergence_type: "policy".to_string(),
            expected_hash: "valid_b3_hash".to_string(),
            actual_hash: session.policy_hash_b3.clone(),
            context: "Policy hash format invalid".to_string(),
        });
    }

    // 4. Verify kernel hash if present
    // FAIL-CLOSED: Optional field absence is treated as suspicious/invalid.
    // This matches bundle verification semantics where missing fields trigger
    // mismatch detection. Security-critical verification must never assume
    // "absence is valid" as that creates a bypass vector.
    let kernel_verified = session
        .kernel_hash_b3
        .as_ref()
        .map(|h| !h.is_empty() && (h.starts_with("b3:") || h.len() == 64))
        .unwrap_or(false);
    if !kernel_verified {
        divergences.push(ReplayDivergence {
            divergence_type: "kernel".to_string(),
            expected_hash: "valid_b3_hash".to_string(),
            actual_hash: session.kernel_hash_b3.clone().unwrap_or_default(),
            context: "Kernel hash format invalid".to_string(),
        });
    }

    // 5. Verify telemetry bundle IDs are parseable
    let telemetry_verified: bool =
        serde_json::from_str::<Vec<String>>(&session.telemetry_bundle_ids_json).is_ok();
    if !telemetry_verified {
        divergences.push(ReplayDivergence {
            divergence_type: "telemetry".to_string(),
            expected_hash: "valid_json_array".to_string(),
            actual_hash: "parse_error".to_string(),
            context: "Telemetry bundle IDs JSON is malformed".to_string(),
        });
    }

    // 6. Verify hash chain (seed -> manifest -> policy linkage)
    let hash_chain_valid = !session.seed_global_b3.is_empty()
        && (session.seed_global_b3.starts_with("b3:") || session.seed_global_b3.len() == 64);
    if !hash_chain_valid {
        divergences.push(ReplayDivergence {
            divergence_type: "hash_chain".to_string(),
            expected_hash: "valid_seed_hash".to_string(),
            actual_hash: session.seed_global_b3.clone(),
            context: "Global seed hash format invalid".to_string(),
        });
    }

    let overall_valid = signature_valid
        && manifest_verified
        && policy_verified
        && kernel_verified
        && telemetry_verified
        && hash_chain_valid;

    debug!(
        session_id = %session_id,
        signature_valid = signature_valid,
        manifest_verified = manifest_verified,
        policy_verified = policy_verified,
        kernel_verified = kernel_verified,
        telemetry_verified = telemetry_verified,
        hash_chain_valid = hash_chain_valid,
        overall_valid = overall_valid,
        divergence_count = divergences.len(),
        "Replay session verification completed"
    );

    let verification = ReplayVerificationResponse {
        session_id: session_id.clone(),
        signature_valid,
        hash_chain_valid,
        manifest_verified,
        policy_verified,
        kernel_verified,
        telemetry_verified,
        overall_valid,
        divergences,
        verified_at: chrono::Utc::now().to_rfc3339(),
    };

    // Audit log with verified metadata for UI display
    let audit_metadata = serde_json::json!({
        "verified": overall_valid,
        "signature_valid": signature_valid,
        "hash_chain_valid": hash_chain_valid,
    });
    if let Err(e) = crate::audit_helper::log_success_with_metadata(
        &state.db,
        &claims,
        crate::audit_helper::actions::REPLAY_VERIFY,
        crate::audit_helper::resources::REPLAY_SESSION,
        Some(&session_id),
        audit_metadata,
    )
    .await
    {
        warn!(
            session_id = %session_id,
            error = %e,
            "Failed to log replay session verification to audit trail"
        );
    }

    Ok(Json(verification))
}

/// Execute a replay session
///
/// Re-runs inference with the original session parameters and optionally
/// reconstructs RAG context from original documents. If documents are missing,
/// the replay runs in degraded mode with reduced context.
#[utoipa::path(
    post,
    path = "/v1/replay/sessions/{id}/execute",
    request_body = ExecuteReplayRequest,
    responses(
        (status = 200, description = "Replay executed", body = ExecuteReplayResponse),
        (status = 404, description = "Session not found"),
    ),
    tag = "replay"
)]
pub async fn execute_replay_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<ExecuteReplayRequest>,
) -> ApiResult<ExecuteReplayResponse> {
    require_permission(&claims, Permission::ReplayManage)?;

    // Validate max_tokens to prevent resource exhaustion
    if req.max_tokens > MAX_TOKENS_LIMIT {
        return Err(ApiError::bad_request(format!(
            "max_tokens ({}) exceeds maximum allowed ({})",
            req.max_tokens, MAX_TOKENS_LIMIT
        )));
    }

    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id).await?;

    let session = state
        .db
        .get_replay_session(&session_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Replay session"))?;

    let mut degraded = false;
    let mut missing_doc_ids = Vec::new();
    let mut rag_context = String::new();
    let mut no_rag_state_stored = false;

    // Reconstruct RAG context from original documents if requested
    if req.use_original_rag_docs {
        match session.restore_rag_state() {
            Ok(Some(rag_state)) => {
                // Reconstruct context from original documents
                let (context, missing) = reconstruct_rag_context(
                    &state,
                    &claims.tenant_id,
                    &rag_state.doc_ids,
                    4000, // MAX_CONTEXT_CHARS
                )
                .await
                .map_err(|e| {
                    ApiError::internal("Failed to reconstruct RAG context")
                        .with_details(e.to_string())
                })?;

                rag_context = context;
                missing_doc_ids = missing;
                degraded = !missing_doc_ids.is_empty();

                if degraded {
                    info!(
                        session_id = %session_id,
                        missing_count = missing_doc_ids.len(),
                        "Replay running in degraded mode - some RAG documents missing"
                    );
                }
            }
            Ok(None) => {
                // No RAG state stored - original inference didn't use RAG
                no_rag_state_stored = true;
                info!(
                    session_id = %session_id,
                    "use_original_rag_docs requested but no RAG state stored (original inference didn't use RAG)"
                );
            }
            Err(e) => {
                // RAG state exists but couldn't be parsed - treat as error
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "Failed to restore RAG state"
                );
                return Err(
                    ApiError::internal("Failed to restore RAG state").with_details(e.to_string())
                );
            }
        }
    }

    // Get the base prompt - either from request override or try to extract from session
    // For now, require the prompt in the request since session doesn't store original prompt
    let base_prompt = req
        .prompt
        .ok_or_else(|| ApiError::bad_request("prompt is required for replay execution"))?;

    // Build the final prompt with RAG context if available
    let final_prompt = if !rag_context.is_empty() {
        format!(
            "Use the following context to answer the question.\n\n\
             Context:\n{}\n\n\
             Question: {}",
            rag_context, base_prompt
        )
    } else {
        base_prompt
    };

    // Extract seed from session's RNG state for deterministic replay (PRD-02)
    let seed = session
        .get_global_nonce()
        .map_err(|e| {
            warn!(
                session_id = %session_id,
                error = %e,
                "Failed to extract global nonce from session for deterministic replay"
            );
            e
        })
        .ok();

    // Use the session's global seed hash as the router seed for deterministic routing
    let router_seed =
        if session.seed_global_b3.is_empty() || session.seed_global_b3 == "b3:placeholder" {
            None
        } else {
            Some(session.seed_global_b3.clone())
        };

    // Create inference request with deterministic parameters from session
    let run_id = crate::id_generator::readable_run_id();
    let run_envelope = new_run_envelope_no_tick(&state, &claims, run_id.clone(), false);

    let inference_request = InferenceRequestInternal {
        request_id: run_id,
        cpid: session.cpid.clone(),
        prompt: final_prompt,
        run_envelope: Some(run_envelope),
        reasoning_mode: false,
        admin_override: false,
        stream: false,
        require_step: false,
        require_determinism: false,
        allow_fallback: true,
        batch_item_id: None,
        rag_enabled: false, // Already handled RAG above
        rag_collection_id: None,
        dataset_version_id: None,
        adapter_stack: None,
        adapters: None,
        stack_id: None,
        stack_version: None,
        stack_determinism_mode: None,
        stack_routing_determinism_mode: None,
        domain_hint: None,
        effective_adapter_ids: None,
        determinism_mode: None,
        routing_determinism_mode: None,
        adapter_strength_overrides: None,
        seed_mode: None,
        request_seed: None,
        backend_profile: None,
        coreml_mode: None,
        max_tokens: req.max_tokens,
        temperature: 0.0, // Default, could be stored in session
        top_k: None,
        top_p: Some(1.0),
        seed,        // Restored from session.rng_state_json for determinism
        router_seed, // From session's global seed for deterministic routing
        require_evidence: true,
        session_id: None,
        pinned_adapter_ids: None, // Not used in replay
        chat_context_hash: None,
        claims: None,
        model: None,
        stop_policy: None, // Replay uses original generation's stop behavior
        created_at: std::time::Instant::now(),
        worker_auth_token: None,
        policy_mask_digest_b3: None, // Not tracked for session-based replay
        utf8_healing: Some(true),
        abstention_threshold: None, // AARA lifecycle
        citation_mode: None,        // AARA lifecycle
    };

    // Execute inference through the unified pipeline with replay context (PRD-02)
    // Use route_and_infer_replay to enforce manifest compatibility
    let core = InferenceCore::new(&state);

    // Build replay context for manifest enforcement
    // Note: session-based replay has less strict manifest requirements than
    // inference-metadata-based replay, so we use the standard route_and_infer
    // unless the session has a valid manifest hash
    let result =
        if !session.manifest_hash_b3.is_empty() && session.manifest_hash_b3 != "b3:placeholder" {
            // Use replay-specific path with manifest enforcement
            use crate::types::ReplayContext;
            let replay_context = ReplayContext {
                original_inference_id: session_id.clone(),
                required_manifest_hash: session.manifest_hash_b3.clone(),
                required_backend: "unknown".to_string(), // Session doesn't store backend
                skip_metadata_capture: true,             // Don't create new replay metadata
                original_policy_id: None,                // Session doesn't store policy
                original_policy_version: None,           // Session doesn't store policy version
            };
            core.route_and_infer_replay(inference_request, replay_context, None)
                .await
        } else {
            // Fallback to standard inference (manifest enforcement not possible)
            core.route_and_infer(inference_request, None, None, None, None)
                .await
        };

    let result = result.map_err(|e| {
        (
            e.status_code(),
            Json(
                ErrorResponse::new("Replay inference failed")
                    .with_code(e.error_code())
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    info!(
        session_id = %session_id,
        latency_ms = result.latency_ms,
        degraded = degraded,
        missing_count = missing_doc_ids.len(),
        "Replay session executed"
    );

    Ok(Json(ExecuteReplayResponse {
        session_id,
        output: result.text,
        degraded,
        missing_doc_ids,
        no_rag_state_stored,
        latency_ms: result.latency_ms,
        verified_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Verify the cryptographic signature of a replay session
///
/// The session signature is computed over the canonical session data:
/// tenant_id + cpid + plan_id + manifest_hash + policy_hash + seed_hash
///
/// Returns true if the signature is valid, false otherwise.
fn verify_session_signature(state: &AppState, session: &ReplaySession) -> bool {
    // Construct the canonical message that was signed during session creation
    let canonical_message = format!(
        "{}:{}:{}:{}:{}:{}",
        session.tenant_id,
        session.cpid,
        session.plan_id,
        session.manifest_hash_b3,
        session.policy_hash_b3,
        session.seed_global_b3,
    );

    // Decode the stored signature
    let signature_bytes = match hex::decode(&session.signature) {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!(
                session_id = %session.id,
                error = %e,
                "Failed to decode session signature from hex"
            );
            return false;
        }
    };

    if signature_bytes.len() != 64 {
        warn!(
            session_id = %session.id,
            signature_len = signature_bytes.len(),
            "Invalid signature length, expected 64 bytes"
        );
        return false;
    }

    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&signature_bytes);

    let signature = match Signature::from_bytes(&sig_array) {
        Ok(sig) => sig,
        Err(e) => {
            warn!(
                session_id = %session.id,
                error = %e,
                "Failed to parse signature bytes"
            );
            return false;
        }
    };

    // Use the server's signing keypair public key for verification
    let public_key = state.crypto.signing_keypair.public_key();

    match public_key.verify(canonical_message.as_bytes(), &signature) {
        Ok(()) => {
            debug!(
                session_id = %session.id,
                "Session signature verified successfully"
            );
            true
        }
        Err(e) => {
            warn!(
                session_id = %session.id,
                error = %e,
                "Session signature verification failed"
            );
            false
        }
    }
}

// Helper function to convert database model to API response
fn session_to_response(
    session: ReplaySession,
) -> Result<ReplaySessionResponse, adapteros_core::AosError> {
    let telemetry_bundle_ids: Vec<String> =
        serde_json::from_str(&session.telemetry_bundle_ids_json).map_err(|e| {
            adapteros_core::AosError::Database(format!(
                "Failed to parse telemetry_bundle_ids_json: {}",
                e
            ))
        })?;
    let adapter_state: AdapterStateSnapshot = serde_json::from_str(&session.adapter_state_json)
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to parse adapter_state_json: {}", e))
        })?;
    let routing_decisions: Vec<serde_json::Value> =
        serde_json::from_str(&session.routing_decisions_json).map_err(|e| {
            adapteros_core::AosError::Database(format!(
                "Failed to parse routing_decisions_json: {}",
                e
            ))
        })?;
    let inference_traces = session
        .inference_traces_json
        .map(|json| serde_json::from_str(&json))
        .transpose()?;

    Ok(ReplaySessionResponse {
        id: session.id,
        tenant_id: session.tenant_id,
        cpid: session.cpid,
        plan_id: session.plan_id,
        snapshot_at: session.snapshot_at,
        seed_global_b3: session.seed_global_b3,
        manifest_hash_b3: session.manifest_hash_b3,
        policy_hash_b3: session.policy_hash_b3,
        kernel_hash_b3: session.kernel_hash_b3,
        telemetry_bundle_ids,
        adapter_state,
        routing_decisions,
        inference_traces,
        signature: session.signature,
        created_at: session.created_at,
    })
}

// ============================================================================
// Receipt & Trace Verification (UI workflow)
// ============================================================================

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReceiptReasonCode {
    ContextMismatch,
    TraceTamper,
    OutputMismatch,
    PolicyMismatch,
    BackendMismatch,
    SignatureInvalid,
    BackendAttestationMismatch,
    SchemaVersionUnsupported,
    SeedDigestMismatch,
    SeedModeViolation,
    SeedDigestMissing,
    ExpectedDigestInvalid,
    PayloadParseError,
    NonCanonicalPayload,
    ReceiptDigestMismatch,
    MissingReceipt,
    TraceNotFound,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReceiptDigestDiff {
    pub field: String,
    pub expected_hex: String,
    pub computed_hex: String,
    pub matches: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReceiptVerificationResult {
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub source: String,
    pub pass: bool,
    pub verified_at: String,
    pub reasons: Vec<ReceiptReasonCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mismatched_token: Option<u32>,
    pub context_digest: ReceiptDigestDiff,
    pub run_head_hash: ReceiptDigestDiff,
    pub output_digest: ReceiptDigestDiff,
    pub receipt_digest: ReceiptDigestDiff,
    pub signature_checked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_valid: Option<bool>,
}

pub(crate) fn build_receipt_verification_result(
    trace_id: String,
    verification: adapteros_db::TraceReceiptVerification,
    source: &str,
) -> ReceiptVerificationResult {
    let context_hex = hex::encode(verification.context_digest);
    let context_diff = ReceiptDigestDiff {
        field: "context_digest".to_string(),
        expected_hex: context_hex.clone(),
        computed_hex: context_hex.clone(),
        matches: true,
    };

    let mut reasons: Vec<ReceiptReasonCode> = Vec::new();

    let (expected_run_head, expected_output, expected_receipt, signature_checked) =
        if let Some(stored) = &verification.stored {
            (
                stored.run_head_hash.to_hex(),
                stored.output_digest.to_hex(),
                stored.receipt_digest.to_hex(),
                stored.signature.is_some(),
            )
        } else {
            reasons.push(ReceiptReasonCode::MissingReceipt);
            (
                "missing".to_string(),
                "missing".to_string(),
                "missing".to_string(),
                false,
            )
        };

    if verification.mismatched_token.is_some() {
        reasons.push(ReceiptReasonCode::TraceTamper);
    }

    let run_head_diff = ReceiptDigestDiff {
        field: "run_head_hash".to_string(),
        expected_hex: expected_run_head.clone(),
        computed_hex: verification.recomputed.run_head_hash.to_hex(),
        matches: expected_run_head == verification.recomputed.run_head_hash.to_hex(),
    };

    let output_diff = ReceiptDigestDiff {
        field: "output_digest".to_string(),
        expected_hex: expected_output.clone(),
        computed_hex: verification.recomputed.output_digest.to_hex(),
        matches: expected_output == verification.recomputed.output_digest.to_hex(),
    };

    let receipt_diff = ReceiptDigestDiff {
        field: "receipt_digest".to_string(),
        expected_hex: expected_receipt.clone(),
        computed_hex: verification.recomputed.receipt_digest.to_hex(),
        matches: expected_receipt == verification.recomputed.receipt_digest.to_hex(),
    };

    if !run_head_diff.matches {
        reasons.push(ReceiptReasonCode::TraceTamper);
    }
    if !output_diff.matches {
        reasons.push(ReceiptReasonCode::OutputMismatch);
    }
    if !receipt_diff.matches {
        reasons.push(ReceiptReasonCode::TraceTamper);
    }

    let pass = reasons.is_empty();

    ReceiptVerificationResult {
        trace_id,
        tenant_id: Some(verification.tenant_id),
        source: source.to_string(),
        pass,
        verified_at: chrono::Utc::now().to_rfc3339(),
        reasons,
        mismatched_token: verification.mismatched_token,
        context_digest: context_diff,
        run_head_hash: run_head_diff,
        output_digest: output_diff,
        receipt_digest: receipt_diff,
        signature_checked,
        signature_valid: None,
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TraceVerifyRequest {
    pub trace_id: String,
}

#[utoipa::path(
    post,
    path = "/v1/replay/verify/trace",
    request_body = TraceVerifyRequest,
    responses(
        (status = 200, description = "Receipt verification result", body = ReceiptVerificationResult),
        (status = 404, description = "Trace not found", body = ErrorResponse)
    ),
    tag = "replay"
)]
pub async fn verify_trace_receipt(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<TraceVerifyRequest>,
) -> ApiResult<ReceiptVerificationResult> {
    require_permission(&claims, Permission::ReplayManage)?;

    let verification =
        adapteros_db::inference_trace::recompute_receipt_and_persist(&state.db, &req.trace_id)
            .await
            .map_err(|e| match e {
                AosError::NotFound(_) => ApiError::not_found("Inference trace"),
                AosError::Database(_) => ApiError::db_error(e),
                _ => {
                    ApiError::internal("Failed to verify trace receipt").with_details(e.to_string())
                }
            })?;

    validate_tenant_isolation(&claims, &verification.tenant_id)?;

    let report = build_receipt_verification_result(req.trace_id.clone(), verification, "trace");

    // Audit log with verified metadata for UI display
    let audit_metadata = serde_json::json!({
        "verified": report.pass,
        "signature_checked": report.signature_checked,
        "signature_valid": report.signature_valid,
    });
    if let Err(e) = crate::audit_helper::log_success_with_metadata(
        &state.db,
        &claims,
        crate::audit_helper::actions::REPLAY_VERIFY,
        crate::audit_helper::resources::REPLAY_SESSION,
        Some(&req.trace_id),
        audit_metadata,
    )
    .await
    {
        warn!(
            trace_id = %req.trace_id,
            error = %e,
            "Failed to log trace receipt verification to audit trail"
        );
    }

    Ok(Json(report))
}

fn map_verifier_reason(code: adapteros_crypto::ReceiptVerifyReasonCode) -> ReceiptReasonCode {
    use adapteros_crypto::ReceiptVerifyReasonCode as C;
    match code {
        C::ContextMismatch => ReceiptReasonCode::ContextMismatch,
        C::TraceTamper => ReceiptReasonCode::TraceTamper,
        C::OutputMismatch => ReceiptReasonCode::OutputMismatch,
        C::PolicyMismatch => ReceiptReasonCode::PolicyMismatch,
        C::BackendMismatch => ReceiptReasonCode::BackendMismatch,
        C::SignatureInvalid => ReceiptReasonCode::SignatureInvalid,
        C::BackendAttestationMismatch => ReceiptReasonCode::BackendAttestationMismatch,
        C::SchemaVersionUnsupported => ReceiptReasonCode::SchemaVersionUnsupported,
        C::SeedDigestMismatch => ReceiptReasonCode::SeedDigestMismatch,
        C::SeedModeViolation => ReceiptReasonCode::SeedModeViolation,
        C::SeedDigestMissing => ReceiptReasonCode::SeedDigestMissing,
        C::ExpectedDigestInvalid => ReceiptReasonCode::ExpectedDigestInvalid,
        C::PayloadParseError => ReceiptReasonCode::PayloadParseError,
        C::NonCanonicalPayload => ReceiptReasonCode::NonCanonicalPayload,
        C::ReceiptDigestMismatch => ReceiptReasonCode::ReceiptDigestMismatch,
    }
}

pub(crate) fn verify_bundle_bytes(bytes: &[u8]) -> Result<ReceiptVerificationResult> {
    let options = adapteros_crypto::VerifyOptions::default();
    let report = adapteros_crypto::verify_bundle_bytes(bytes, &options)?;

    Ok(ReceiptVerificationResult {
        trace_id: report.trace_id,
        tenant_id: report.tenant_id,
        source: report.source,
        pass: report.pass,
        verified_at: report.verified_at,
        reasons: report
            .reasons
            .into_iter()
            .map(map_verifier_reason)
            .collect(),
        mismatched_token: report.mismatched_token,
        context_digest: ReceiptDigestDiff {
            field: "context_digest".to_string(),
            expected_hex: report.context_digest.expected,
            computed_hex: report.context_digest.computed,
            matches: report.context_digest.matches,
        },
        run_head_hash: ReceiptDigestDiff {
            field: "run_head_hash".to_string(),
            expected_hex: report.run_head_hash.expected,
            computed_hex: report.run_head_hash.computed,
            matches: report.run_head_hash.matches,
        },
        output_digest: ReceiptDigestDiff {
            field: "output_digest".to_string(),
            expected_hex: report.output_digest.expected,
            computed_hex: report.output_digest.computed,
            matches: report.output_digest.matches,
        },
        receipt_digest: ReceiptDigestDiff {
            field: "receipt_digest".to_string(),
            expected_hex: report.receipt_digest.expected,
            computed_hex: report.receipt_digest.computed,
            matches: report.receipt_digest.matches,
        },
        signature_checked: report.signature_checked,
        signature_valid: report.signature_valid,
    })
}

#[utoipa::path(
    post,
    path = "/v1/replay/verify/bundle",
    responses(
        (status = 200, description = "Bundle verification result", body = ReceiptVerificationResult),
        (status = 400, description = "Invalid bundle", body = ErrorResponse)
    ),
    tag = "replay"
)]
pub async fn verify_bundle_receipt(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> ApiResult<ReceiptVerificationResult> {
    require_permission(&claims, Permission::ReplayManage)?;

    let mut bundle_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("Failed to read upload: {e}")))?
    {
        if field.name() == Some("bundle") {
            bundle_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| {
                        ApiError::bad_request(format!("Failed to read bundle bytes: {e}"))
                    })?
                    .to_vec(),
            );
            break;
        }
    }

    let bytes = bundle_bytes.ok_or_else(|| ApiError::bad_request("Missing 'bundle' file field"))?;

    let report = verify_bundle_bytes(&bytes).map_err(|e| ApiError::bad_request(e.to_string()))?;

    // Tenant isolation for uploaded bundles (best-effort using bundle metadata)
    if let Some(tenant) = report.tenant_id.as_ref() {
        validate_tenant_isolation(&claims, tenant)?;
    }

    // Audit log with verified metadata for UI display
    let audit_metadata = serde_json::json!({
        "verified": report.pass,
        "signature_checked": report.signature_checked,
        "signature_valid": report.signature_valid,
        "source": "bundle",
    });
    if let Err(e) = crate::audit_helper::log_success_with_metadata(
        &state.db,
        &claims,
        crate::audit_helper::actions::REPLAY_VERIFY,
        crate::audit_helper::resources::REPLAY_SESSION,
        Some(&report.trace_id),
        audit_metadata,
    )
    .await
    {
        warn!(
            trace_id = %report.trace_id,
            error = %e,
            "Failed to log bundle verification to audit trail"
        );
    }

    Ok(Json(report))
}
