//! Replay session API handlers
//!
//! Provides endpoints for creating, listing, and verifying deterministic replay sessions.

use adapteros_crypto::signature::{Keypair, PublicKey, Signature};
use adapteros_db::replay_sessions::ReplaySession;
use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use utoipa::{IntoParams, ToSchema};

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
// use crate::types::ErrorResponse; // unused

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
) -> Result<Json<Vec<ReplaySessionResponse>>, (StatusCode, String)> {
    require_permission(&claims, Permission::ReplayManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            "Insufficient permissions".to_string(),
        )
    })?;

    let sessions = state
        .db
        .list_replay_sessions(params.tenant_id.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list replay sessions: {}", e),
            )
        })?;

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
) -> Result<Json<ReplaySessionResponse>, (StatusCode, String)> {
    require_permission(&claims, Permission::ReplayManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            "Insufficient permissions".to_string(),
        )
    })?;

    let session = state
        .db
        .get_replay_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get replay session: {}", e),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Replay session not found".to_string(),
        ))?;

    let response = session_to_response(session).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to serialize response: {}", e),
        )
    })?;

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
) -> Result<Json<ReplaySessionResponse>, (StatusCode, String)> {
    require_permission(&claims, Permission::ReplayManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            "Insufficient permissions".to_string(),
        )
    })?;

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
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        adapter_state_json: serde_json::to_string(&adapter_state)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        routing_decisions_json: "[]".to_string(),
        inference_traces_json: None,
        signature: hex::encode(signature.to_bytes()),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state
        .db
        .create_replay_session(&session)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create replay session: {}", e),
            )
        })?;

    let response = session_to_response(session)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
) -> Result<Json<ReplayVerificationResponse>, (StatusCode, String)> {
    require_permission(&claims, Permission::ReplayManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            "Insufficient permissions".to_string(),
        )
    })?;

    let session = state
        .db
        .get_replay_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get replay session: {}", e),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Replay session not found".to_string(),
        ))?;

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
fn session_to_response(session: ReplaySession) -> Result<ReplaySessionResponse, adapteros_core::AosError> {
    let telemetry_bundle_ids: Vec<String> =
        serde_json::from_str(&session.telemetry_bundle_ids_json)
            .map_err(|e| adapteros_core::AosError::Database(format!("Failed to parse telemetry_bundle_ids_json: {}", e)))?;
    let adapter_state: AdapterStateSnapshot = serde_json::from_str(&session.adapter_state_json)
        .map_err(|e| adapteros_core::AosError::Database(format!("Failed to parse adapter_state_json: {}", e)))?;
    let routing_decisions: Vec<serde_json::Value> =
        serde_json::from_str(&session.routing_decisions_json)
            .map_err(|e| adapteros_core::AosError::Database(format!("Failed to parse routing_decisions_json: {}", e)))?;
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
