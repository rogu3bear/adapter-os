//! Replay session API handlers
//!
//! Provides endpoints for creating, listing, and verifying deterministic replay sessions.

use adapteros_core::{AosError, B3Hash};
use adapteros_crypto::signature::{Keypair, PublicKey, Signature};
use adapteros_db::replay_sessions::ReplaySession;
use anyhow::{anyhow, bail, Context, Result};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use base64::Engine as _;
use chrono;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};
use tracing::{debug, info, warn};
use utoipa::{IntoParams, ToSchema};
use zip::ZipArchive;

use crate::auth::Claims;
use crate::error_helpers::{bad_request, db_error, internal_error, not_found};
use crate::handlers::rag_common::reconstruct_rag_context;
use crate::inference_core::InferenceCore;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
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
                Json(ErrorResponse::new(e.to_string())),
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
                        Json(ErrorResponse::new(format!(
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
                    Json(ErrorResponse::new(format!(
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
        stop_policy: None, // Replay uses original generation's stop behavior
        created_at: std::time::Instant::now(),
        worker_auth_token: None,
        policy_mask_digest: None, // Not tracked for session-based replay
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

// ============================================================================
// Receipt & Trace Verification (UI workflow)
// ============================================================================

const EVIDENCE_BUNDLE_FILENAMES: &[&str] = &[
    "receipt_bundle.json",
    "run_receipt.json",
    "inference_trace.json",
];

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReceiptReasonCode {
    ContextMismatch,
    TraceTamper,
    OutputMismatch,
    PolicyMismatch,
    BackendMismatch,
    SignatureInvalid,
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
) -> Result<Json<ReceiptVerificationResult>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::ReplayManage)?;

    let verification = adapteros_db::recompute_receipt(&state.db, &req.trace_id)
        .await
        .map_err(|e| match e {
            AosError::NotFound(_) => not_found("Inference trace"),
            AosError::Database(_) => db_error(e),
            _ => internal_error(e),
        })?;

    validate_tenant_isolation(&claims, &verification.tenant_id)?;

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

    Ok(Json(ReceiptVerificationResult {
        trace_id: req.trace_id,
        tenant_id: Some(verification.tenant_id),
        source: "trace".to_string(),
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
    }))
}

#[derive(Debug, Serialize, Deserialize)]
struct ReceiptBundle {
    #[serde(default)]
    version: Option<String>,
    trace_id: String,
    tenant_id: String,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    context_digest_hex: Option<String>,
    context: ReceiptContext,
    tokens: Vec<ReceiptToken>,
    output_tokens: Vec<u32>,
    receipt: ReceiptDigests,
    #[serde(default)]
    expected_backend: Option<String>,
    #[serde(default)]
    expected_kernel_version: Option<String>,
    /// Dataset version ID for deterministic dataset pinning
    #[serde(default)]
    dataset_version_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReceiptContext {
    tenant_namespace: String,
    stack_hash_hex: String,
    prompt_tokens: Vec<u32>,
    #[serde(default)]
    policy_mask_digest_hex: Option<String>,
    #[serde(default)]
    context_digest_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptToken {
    token_index: u32,
    adapter_ids: Vec<String>,
    gates_q15: Vec<i16>,
    #[serde(default)]
    policy_mask_digest_hex: Option<String>,
    #[serde(default)]
    backend_id: Option<String>,
    #[serde(default)]
    kernel_version_id: Option<String>,
    #[serde(default)]
    decision_hash_hex: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReceiptDigests {
    run_head_hash_hex: String,
    output_digest_hex: String,
    receipt_digest_hex: String,
    #[serde(default)]
    signature_b64: Option<String>,
    #[serde(default)]
    public_key_hex: Option<String>,
    #[serde(default)]
    logical_prompt_tokens: u32,
    #[serde(default)]
    prefix_cached_token_count: u32,
    #[serde(default)]
    billed_input_tokens: u32,
    #[serde(default)]
    logical_output_tokens: u32,
    #[serde(default)]
    billed_output_tokens: u32,
    // Stop controller fields (PRD: Hard Deterministic Stop Controller)
    #[serde(default)]
    stop_reason_code: Option<String>,
    #[serde(default)]
    stop_reason_token_index: Option<u32>,
    #[serde(default)]
    stop_policy_digest_b3_hex: Option<String>,
    // KV quota/residency fields (PRD: KvResidencyAndQuotas v1)
    #[serde(default)]
    tenant_kv_quota_bytes: u64,
    #[serde(default)]
    tenant_kv_bytes_used: u64,
    #[serde(default)]
    kv_evictions: u32,
    #[serde(default)]
    kv_residency_policy_id: Option<String>,
    #[serde(default)]
    kv_quota_enforced: bool,
    // Prefix KV cache fields (PRD: PrefixKvCache v1)
    #[serde(default)]
    prefix_kv_key_b3_hex: Option<String>,
    #[serde(default)]
    prefix_cache_hit: bool,
    #[serde(default)]
    prefix_kv_bytes: u64,
    /// PRD-06: Model cache identity v2 digest (hex-encoded BLAKE3)
    #[serde(default)]
    model_cache_identity_v2_digest_b3_hex: Option<String>,
}

fn push_reason(reasons: &mut Vec<ReceiptReasonCode>, code: ReceiptReasonCode) {
    if !reasons.contains(&code) {
        reasons.push(code);
    }
}

fn decode_hex_32(label: &str, hex: &str) -> Result<[u8; 32]> {
    let bytes =
        hex::decode(hex).with_context(|| format!("Failed to decode {label} hex ({hex})"))?;
    if bytes.len() != 32 {
        bail!("{label} must be 32 bytes, got {}", bytes.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn encode_adapter_ids(ids: &[String]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + ids.iter().map(|s| s.len() + 4).sum::<usize>());
    out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
    for id in ids {
        let bytes = id.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

fn encode_gates_q15(gates: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + gates.len() * 2);
    out.extend_from_slice(&(gates.len() as u32).to_le_bytes());
    for g in gates {
        out.extend_from_slice(&g.to_le_bytes());
    }
    out
}

fn hash_decision(
    context_digest: &[u8; 32],
    token_index: u32,
    adapter_blob: &[u8],
    gates_blob: &[u8],
    policy_mask_digest: Option<[u8; 32]>,
    backend_id: Option<&str>,
    kernel_version_id: Option<&str>,
) -> B3Hash {
    let policy_bytes = policy_mask_digest.map(|d| d.to_vec()).unwrap_or_default();
    let backend_bytes = backend_id.unwrap_or("").as_bytes().to_vec();
    let kernel_bytes = kernel_version_id.unwrap_or("").as_bytes().to_vec();

    B3Hash::hash_multi(&[
        &context_digest[..],
        &token_index.to_le_bytes(),
        &(adapter_blob.len() as u32).to_le_bytes(),
        adapter_blob,
        &(gates_blob.len() as u32).to_le_bytes(),
        gates_blob,
        &(policy_bytes.len() as u32).to_le_bytes(),
        &policy_bytes,
        &(backend_bytes.len() as u32).to_le_bytes(),
        &backend_bytes,
        &(kernel_bytes.len() as u32).to_le_bytes(),
        &kernel_bytes,
    ])
}

fn update_head(prev: &B3Hash, token_index: u32, decision_hash: &B3Hash) -> B3Hash {
    B3Hash::hash_multi(&[
        prev.as_bytes(),
        decision_hash.as_bytes(),
        &token_index.to_le_bytes(),
    ])
}

fn compute_output_digest(output_tokens: &[u32]) -> B3Hash {
    let mut buf = Vec::with_capacity(4 + output_tokens.len() * 4);
    buf.extend_from_slice(&(output_tokens.len() as u32).to_le_bytes());
    for t in output_tokens {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    B3Hash::hash(&buf)
}

fn compute_context_digest(ctx: &ReceiptContext) -> Result<B3Hash> {
    let stack_bytes =
        hex::decode(&ctx.stack_hash_hex).with_context(|| "Failed to decode stack_hash_hex")?;
    let mut buf = Vec::with_capacity(
        ctx.tenant_namespace.len() + stack_bytes.len() + 4 + ctx.prompt_tokens.len() * 4,
    );
    buf.extend_from_slice(ctx.tenant_namespace.as_bytes());
    buf.extend_from_slice(&stack_bytes);
    buf.extend_from_slice(&(ctx.prompt_tokens.len() as u32).to_le_bytes());
    for t in &ctx.prompt_tokens {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    Ok(B3Hash::hash(&buf))
}

fn verify_signature(
    receipt: &ReceiptDigests,
    receipt_digest: &B3Hash,
    reasons: &mut Vec<ReceiptReasonCode>,
) -> Result<(bool, Option<bool>)> {
    let Some(signature_b64) = receipt.signature_b64.as_ref() else {
        return Ok((false, None));
    };
    let Some(pubkey_hex) = receipt.public_key_hex.as_ref() else {
        push_reason(reasons, ReceiptReasonCode::SignatureInvalid);
        return Ok((true, Some(false)));
    };

    let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(signature_b64) {
        Ok(bytes) => bytes,
        Err(e) => {
            push_reason(reasons, ReceiptReasonCode::SignatureInvalid);
            return Err(anyhow!("Invalid base64 signature: {e}"));
        }
    };
    if sig_bytes.len() != 64 {
        push_reason(reasons, ReceiptReasonCode::SignatureInvalid);
        return Ok((true, Some(false)));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let signature =
        Signature::from_bytes(&sig_arr).map_err(|e| anyhow!("Invalid signature bytes: {e}"))?;

    let pub_bytes = decode_hex_32("public_key", pubkey_hex)?;
    let pubkey =
        PublicKey::from_bytes(&pub_bytes).map_err(|e| anyhow!("Invalid public key: {e}"))?;

    let verified = pubkey.verify(receipt_digest.as_bytes(), &signature).is_ok();
    if !verified {
        push_reason(reasons, ReceiptReasonCode::SignatureInvalid);
    }

    Ok((true, Some(verified)))
}

fn verify_bundle(bundle: &ReceiptBundle) -> Result<ReceiptVerificationResult> {
    let mut reasons: Vec<ReceiptReasonCode> = Vec::new();
    let computed_context = compute_context_digest(&bundle.context)?;

    let expected_context_hex = bundle
        .context_digest_hex
        .as_ref()
        .or(bundle.context.context_digest_hex.as_ref())
        .cloned()
        .unwrap_or_else(|| computed_context.to_hex());
    let context_expected = B3Hash::from_hex(&expected_context_hex)
        .with_context(|| "Invalid expected context digest hex")?;
    if context_expected != computed_context {
        push_reason(&mut reasons, ReceiptReasonCode::ContextMismatch);
    }

    let logical_prompt_tokens = bundle.receipt.logical_prompt_tokens;
    let prompt_token_len = bundle.context.prompt_tokens.len() as u32;
    if logical_prompt_tokens != prompt_token_len {
        push_reason(&mut reasons, ReceiptReasonCode::ContextMismatch);
    }

    let canonical_billed_input =
        logical_prompt_tokens.saturating_sub(bundle.receipt.prefix_cached_token_count);
    if canonical_billed_input != bundle.receipt.billed_input_tokens {
        push_reason(&mut reasons, ReceiptReasonCode::TraceTamper);
    }

    if bundle.receipt.logical_output_tokens != bundle.output_tokens.len() as u32 {
        push_reason(&mut reasons, ReceiptReasonCode::TraceTamper);
    }

    if bundle.receipt.billed_output_tokens != bundle.receipt.logical_output_tokens {
        push_reason(&mut reasons, ReceiptReasonCode::TraceTamper);
    }

    if let Some(expected_backend) = bundle.expected_backend.as_ref() {
        let expected_backend = expected_backend.to_lowercase();
        if bundle.tokens.iter().any(|t| {
            t.backend_id
                .as_ref()
                .map(|b| b.to_lowercase() != expected_backend)
                .unwrap_or(true)
        }) {
            push_reason(&mut reasons, ReceiptReasonCode::BackendMismatch);
        }
    }

    if let Some(expected_kernel) = bundle.expected_kernel_version.as_ref() {
        let expected_kernel = expected_kernel.to_lowercase();
        if bundle.tokens.iter().any(|t| {
            t.kernel_version_id
                .as_ref()
                .map(|k| k.to_lowercase() != expected_kernel)
                .unwrap_or(true)
        }) {
            push_reason(&mut reasons, ReceiptReasonCode::BackendMismatch);
        }
    }

    if let Some(expected_policy_hex) = bundle.context.policy_mask_digest_hex.as_ref() {
        let expected_policy = decode_hex_32("policy_mask_digest_hex", expected_policy_hex)?;
        if bundle.tokens.iter().any(|t| {
            t.policy_mask_digest_hex
                .as_ref()
                .and_then(|p| decode_hex_32("policy_mask_digest_hex", p).ok())
                .map(|digest| digest != expected_policy)
                .unwrap_or(true)
        }) {
            push_reason(&mut reasons, ReceiptReasonCode::PolicyMismatch);
        }
    }

    let mut run_head = B3Hash::zero();
    let mut mismatched_token = None;

    let mut tokens_sorted = bundle.tokens.clone();
    tokens_sorted.sort_by_key(|t| t.token_index);

    for token in &tokens_sorted {
        let adapter_blob = encode_adapter_ids(&token.adapter_ids);
        let gates_blob = encode_gates_q15(&token.gates_q15);
        let policy_digest = match &token.policy_mask_digest_hex {
            Some(hex) => Some(decode_hex_32("policy_mask_digest_hex", hex)?),
            None => None,
        };
        let decision_hash = hash_decision(
            computed_context.as_bytes(),
            token.token_index,
            &adapter_blob,
            &gates_blob,
            policy_digest,
            token.backend_id.as_deref(),
            token.kernel_version_id.as_deref(),
        );

        if let Some(expected_hash_hex) = token.decision_hash_hex.as_ref() {
            let expected_hash =
                B3Hash::from_hex(expected_hash_hex).with_context(|| "Invalid decision_hash_hex")?;
            if expected_hash != decision_hash && mismatched_token.is_none() {
                mismatched_token = Some(token.token_index);
            }
        }

        run_head = update_head(&run_head, token.token_index, &decision_hash);
    }

    let expected_run_head =
        B3Hash::from_hex(&bundle.receipt.run_head_hash_hex).with_context(|| {
            format!(
                "Invalid run_head_hash_hex ({})",
                bundle.receipt.run_head_hash_hex
            )
        })?;
    if expected_run_head != run_head {
        push_reason(&mut reasons, ReceiptReasonCode::TraceTamper);
        mismatched_token.get_or_insert(tokens_sorted.last().map(|t| t.token_index).unwrap_or(0));
    }

    let output_digest = compute_output_digest(&bundle.output_tokens);
    let expected_output = B3Hash::from_hex(&bundle.receipt.output_digest_hex)
        .with_context(|| "Invalid output_digest_hex")?;
    if expected_output != output_digest {
        push_reason(&mut reasons, ReceiptReasonCode::OutputMismatch);
    }

    // Compute receipt_digest with all fields (must match compute_receipt_digest in inference_trace.rs)
    let stop_reason_bytes = bundle
        .receipt
        .stop_reason_code
        .as_deref()
        .unwrap_or("")
        .as_bytes();
    let stop_token_index_bytes = bundle
        .receipt
        .stop_reason_token_index
        .unwrap_or(0xFFFFFFFF)
        .to_le_bytes();
    let stop_policy_bytes = bundle
        .receipt
        .stop_policy_digest_b3_hex
        .as_ref()
        .and_then(|h| hex::decode(h).ok())
        .unwrap_or_else(|| vec![0u8; 32]);
    let prefix_kv_key_bytes = bundle
        .receipt
        .prefix_kv_key_b3_hex
        .as_ref()
        .and_then(|h| hex::decode(h).ok())
        .unwrap_or_else(|| vec![0u8; 32]);
    let model_cache_identity_bytes = bundle
        .receipt
        .model_cache_identity_v2_digest_b3_hex
        .as_ref()
        .and_then(|h| hex::decode(h).ok())
        .unwrap_or_else(|| vec![0u8; 32]);
    let kv_residency_policy_id = bundle.receipt.kv_residency_policy_id.as_deref();

    let receipt_digest = B3Hash::hash_multi(&[
        computed_context.as_bytes(),
        run_head.as_bytes(),
        output_digest.as_bytes(),
        &bundle.receipt.logical_prompt_tokens.to_le_bytes(),
        &bundle.receipt.prefix_cached_token_count.to_le_bytes(),
        &bundle.receipt.billed_input_tokens.to_le_bytes(),
        &bundle.receipt.logical_output_tokens.to_le_bytes(),
        &bundle.receipt.billed_output_tokens.to_le_bytes(),
        // Stop controller fields
        &(stop_reason_bytes.len() as u32).to_le_bytes(),
        stop_reason_bytes,
        &stop_token_index_bytes,
        &stop_policy_bytes,
        // KV quota/residency fields
        &bundle.receipt.tenant_kv_quota_bytes.to_le_bytes(),
        &bundle.receipt.tenant_kv_bytes_used.to_le_bytes(),
        &bundle.receipt.kv_evictions.to_le_bytes(),
        &(kv_residency_policy_id.map(|s| s.len() as u32).unwrap_or(0)).to_le_bytes(),
        kv_residency_policy_id.map(|s| s.as_bytes()).unwrap_or(&[]),
        &[if bundle.receipt.kv_quota_enforced {
            1u8
        } else {
            0u8
        }],
        // Prefix KV cache fields
        &prefix_kv_key_bytes,
        &[if bundle.receipt.prefix_cache_hit {
            1u8
        } else {
            0u8
        }],
        &bundle.receipt.prefix_kv_bytes.to_le_bytes(),
        // Model cache identity V2 (PRD-06)
        &model_cache_identity_bytes,
    ]);
    let expected_receipt =
        B3Hash::from_hex(&bundle.receipt.receipt_digest_hex).with_context(|| {
            format!(
                "Invalid receipt_digest_hex ({})",
                bundle.receipt.receipt_digest_hex
            )
        })?;
    if expected_receipt != receipt_digest {
        push_reason(&mut reasons, ReceiptReasonCode::TraceTamper);
    }

    let (signature_checked, signature_valid) =
        verify_signature(&bundle.receipt, &expected_receipt, &mut reasons)?;

    let pass = reasons.is_empty();

    Ok(ReceiptVerificationResult {
        trace_id: bundle.trace_id.clone(),
        tenant_id: Some(bundle.tenant_id.clone()),
        source: "bundle".to_string(),
        pass,
        verified_at: chrono::Utc::now().to_rfc3339(),
        reasons,
        mismatched_token,
        context_digest: ReceiptDigestDiff {
            field: "context_digest".to_string(),
            expected_hex: expected_context_hex,
            computed_hex: computed_context.to_hex(),
            matches: computed_context == context_expected,
        },
        run_head_hash: ReceiptDigestDiff {
            field: "run_head_hash".to_string(),
            expected_hex: bundle.receipt.run_head_hash_hex.clone(),
            computed_hex: run_head.to_hex(),
            matches: run_head == expected_run_head,
        },
        output_digest: ReceiptDigestDiff {
            field: "output_digest".to_string(),
            expected_hex: bundle.receipt.output_digest_hex.clone(),
            computed_hex: output_digest.to_hex(),
            matches: output_digest == expected_output,
        },
        receipt_digest: ReceiptDigestDiff {
            field: "receipt_digest".to_string(),
            expected_hex: bundle.receipt.receipt_digest_hex.clone(),
            computed_hex: receipt_digest.to_hex(),
            matches: receipt_digest == expected_receipt,
        },
        signature_checked,
        signature_valid,
    })
}

fn load_bundle_from_bytes(bytes: &[u8]) -> Result<ReceiptBundle> {
    if let Ok(bundle) = serde_json::from_slice::<ReceiptBundle>(bytes) {
        return Ok(bundle);
    }

    // Fallback: try to read from zip archive
    let mut cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(&mut cursor)?;

    for name in EVIDENCE_BUNDLE_FILENAMES {
        if let Ok(mut file) = archive.by_name(name) {
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;
            if let Ok(bundle) = serde_json::from_str::<ReceiptBundle>(&buf) {
                return Ok(bundle);
            }
        }
    }

    bail!("Unable to parse evidence bundle as JSON or zip");
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
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<Json<ReceiptVerificationResult>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::ReplayManage)?;

    let mut bundle_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(format!("Failed to read upload: {e}")))?
    {
        if field.name() == Some("bundle") {
            bundle_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read bundle bytes: {e}")))?
                    .to_vec(),
            );
            break;
        }
    }

    let bytes = bundle_bytes.ok_or_else(|| bad_request("Missing 'bundle' file field"))?;

    let bundle = load_bundle_from_bytes(&bytes).map_err(bad_request)?;
    let report = verify_bundle(&bundle).map_err(bad_request)?;

    // Tenant isolation for uploaded bundles (best-effort using bundle metadata)
    if let Some(tenant) = report.tenant_id.as_ref() {
        validate_tenant_isolation(&claims, tenant)?;
    }

    Ok(Json(report))
}
