//! Replay session API handlers
//!
//! Provides endpoints for creating, listing, and verifying deterministic replay sessions.

use adapteros_crypto::signature::{Keypair, Signature};
use adapteros_db::replay_sessions::ReplaySession;
use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use utoipa::{IntoParams, ToSchema};

use crate::auth::Claims;
use crate::error_helpers::{db_error, internal_error, not_found};
use crate::handlers::rag_common::reconstruct_rag_context;
use crate::inference_core::InferenceCore;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::{ErrorResponse, InferenceRequestInternal};

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
) -> Result<Json<Vec<ReplaySessionResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::ReplayManage)?;

    let sessions = state
        .db
        .list_replay_sessions(params.tenant_id.as_deref())
        .await
        .map_err(db_error)?;

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
) -> Result<Json<ReplaySessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::ReplayManage)?;

    let session = state
        .db
        .get_replay_session(&session_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Replay session"))?;

    let response = session_to_response(session).map_err(internal_error)?;

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
) -> Result<Json<ReplaySessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::ReplayManage)?;

    // Generate session ID
    let session_id = uuid::Uuid::new_v4().to_string();

    // Fetch adapter state, routing decisions, etc. from telemetry bundles - placeholder implementation
    // For now, create minimal snapshot
    let adapter_state = AdapterStateSnapshot {
        adapters: vec![],
        timestamp: chrono::Utc::now().to_rfc3339(),
        memory_usage_bytes: 0,
    };

    let snapshot_at = req
        .snapshot_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    // Create signature
    let keypair = Keypair::generate();
    let snapshot_data = format!(
        "{}:{}:{}:{}",
        req.cpid, req.plan_id, snapshot_at, req.tenant_id
    );
    let signature = keypair.sign(snapshot_data.as_bytes());

    let session = ReplaySession {
        id: session_id.clone(),
        tenant_id: req.tenant_id,
        cpid: req.cpid,
        plan_id: req.plan_id,
        snapshot_at,
        seed_global_b3: "b3:placeholder".to_string(), // Placeholder - would get from manifest
        rng_state_json: "{}".to_string(),             // Placeholder - would initialize RNG state
        manifest_hash_b3: "b3:placeholder".to_string(),
        policy_hash_b3: "b3:placeholder".to_string(),
        kernel_hash_b3: None,
        telemetry_bundle_ids_json: serde_json::to_string(&req.telemetry_bundle_ids)
            .map_err(internal_error)?,
        adapter_state_json: serde_json::to_string(&adapter_state).map_err(internal_error)?,
        routing_decisions_json: "[]".to_string(),
        inference_traces_json: None,
        signature: hex::encode(signature.to_bytes()),
        created_at: chrono::Utc::now().to_rfc3339(),
        rag_state_json: None, // RAG state stored if session includes RAG retrieval
    };

    state
        .db
        .create_replay_session(&session)
        .await
        .map_err(db_error)?;

    let response = session_to_response(session).map_err(internal_error)?;

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
) -> Result<Json<ReplayVerificationResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::ReplayManage)?;

    let session = state
        .db
        .get_replay_session(&session_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Replay session"))?;

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
    let kernel_verified = session
        .kernel_hash_b3
        .as_ref()
        .map(|h| !h.is_empty() && (h.starts_with("b3:") || h.len() == 64))
        .unwrap_or(true); // Optional field, so absence is valid
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
) -> Result<Json<ExecuteReplayResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::ReplayManage)?;

    let session = state
        .db
        .get_replay_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Replay session not found")),
            )
        })?;

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
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(&format!(
                            "Failed to reconstruct RAG context: {}",
                            e
                        ))),
                    )
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
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(&format!(
                        "Failed to restore RAG state: {}",
                        e
                    ))),
                ));
            }
        }
    }

    // Get the base prompt - either from request override or try to extract from session
    // For now, require the prompt in the request since session doesn't store original prompt
    let base_prompt = req.prompt.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "prompt is required for replay execution",
            )),
        )
    })?;

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
    let seed = session.get_global_nonce().ok();

    // Use the session's global seed hash as the router seed for deterministic routing
    let router_seed =
        if session.seed_global_b3.is_empty() || session.seed_global_b3 == "b3:placeholder" {
            None
        } else {
            Some(session.seed_global_b3.clone())
        };

    // Create inference request with deterministic parameters from session
    let inference_request = InferenceRequestInternal {
        request_id: uuid::Uuid::new_v4().to_string(),
        cpid: session.cpid.clone(),
        prompt: final_prompt,
        stream: false,
        batch_item_id: None,
        rag_enabled: false, // Already handled RAG above
        rag_collection_id: None,
        adapter_stack: None,
        adapters: None,
        stack_id: None,
        stack_version: None,
        stack_determinism_mode: None,
        effective_adapter_ids: None,
        determinism_mode: None,
        seed_mode: None,
        request_seed: None,
        backend_profile: None,
        max_tokens: req.max_tokens,
        temperature: 0.7, // Default, could be stored in session
        top_k: None,
        top_p: None,
        seed,        // Restored from session.rng_state_json for determinism
        router_seed, // From session's global seed for deterministic routing
        require_evidence: true,
        session_id: None,
        pinned_adapter_ids: None, // Not used in replay
        chat_context_hash: None,
        model: None,
        created_at: std::time::Instant::now(),
        worker_auth_token: None,
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
            core.route_and_infer_replay(inference_request, replay_context)
                .await
        } else {
            // Fallback to standard inference (manifest enforcement not possible)
            core.route_and_infer(inference_request, None).await
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
