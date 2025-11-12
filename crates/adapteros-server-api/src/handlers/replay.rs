//! Replay session API handlers
//!
//! Provides endpoints for creating, listing, and verifying deterministic replay sessions.

use adapteros_crypto::signature::Keypair;
use adapteros_db::replay_sessions::ReplaySession;
use anyhow::Result;
use axum::{
    extract::{Path, Query, State, Extension},
    http::StatusCode,
    Json,
};
use chrono;
use serde::{Deserialize, Serialize};
use tracing::warn;
use utoipa::{IntoParams, ToSchema};

use crate::state::AppState;
use crate::types::{ErrorResponse, ReplayVerificationResponse};
use crate::auth::Claims;
use crate::services::replay::reconstruct_bundle;

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
    Query(params): Query<ListReplaySessionsParams>,
) -> Result<Json<Vec<ReplaySessionResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let sessions = state
        .db
        .list_replay_sessions(params.tenant_id.as_deref())
        .await
        .map_err(db_error_to_response)?;

    let responses: Vec<ReplaySessionResponse> = sessions
        .into_iter()
        .filter_map(|session| match session_to_response(session) {
            Ok(response) => Some(response),
            Err(e) => {
                warn!("Failed to serialize replay session: {:?}", e);
                None
            }
        })
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
    Path(session_id): Path<String>,
) -> Result<Json<ReplaySessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let session = state
        .db
        .get_replay_session(&session_id)
        .await
        .map_err(db_error_to_response)?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("replay session not found").with_code("NOT_FOUND")),
        ))?;

    let response = session_to_response(session).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("serialization error").with_code("SERIALIZE_ERROR")),
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
    Json(req): Json<CreateReplaySessionRequest>,
) -> Result<Json<ReplaySessionResponse>, (StatusCode, Json<ErrorResponse>)> {
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
        telemetry_bundle_ids_json: serde_json::to_string(&req.telemetry_bundle_ids).map_err(
            |e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("serialization error").with_code("SERIALIZE_ERROR")),
                )
            },
        )?,
        adapter_state_json: serde_json::to_string(&adapter_state).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("serialization error").with_code("SERIALIZE_ERROR")),
            )
        })?,
        routing_decisions_json: "[]".to_string(),
        inference_traces_json: None,
        signature: hex::encode(signature.to_bytes()),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state
        .db
        .create_replay_session(&session)
        .await
        .map_err(db_error_to_response)?;

    let response = session_to_response(session).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("serialization error").with_code("SERIALIZE_ERROR")),
        )
    })?;

    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/v1/replay/{bundle_id}",
    params(
        ("bundle_id" = String, Path, description = "Telemetry bundle ID to replay from")
    ),
    responses(
        (status = 200, description = "Replay session created", body = ReplaySessionResponse),
    ),
    tag = "replay"
)]
pub async fn replay_from_bundle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(bundle_id): Path<String>,
) -> Result<Json<ReplaySessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (cpid, plan_id) = replay::fetch_bundle_metadata(&state.db, &bundle_id).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new_user_friendly("DB_ERROR", e.to_string()))) )?;
    let req = CreateReplaySessionRequest {
        tenant_id: claims.tenant_id,
        cpid,
        plan_id,
        telemetry_bundle_ids: vec![bundle_id],
        snapshot_at: None,
    };
    create_replay_session(State(state), Json(req)).await
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
    Path(session_id): Path<String>,
) -> Result<Json<ReplayVerificationResponse>, (StatusCode, String)> {
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

    // Perform cryptographic verification
    let mut verification = state
        .crypto
        .verify_replay_session(&session)
        .await
        .map_err(|e| {
            tracing::error!("Failed to verify replay session: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to verify replay session: {}", e),
            )
        })?;

    // Add verification timestamp
    verification.verified_at = chrono::Utc::now().to_rfc3339();

    Ok(Json(verification))
}

// Helper function to convert database model to API response
fn session_to_response(
    session: ReplaySession,
) -> Result<ReplaySessionResponse, (StatusCode, String)> {
    let telemetry_bundle_ids: Vec<String> =
        serde_json::from_str(&session.telemetry_bundle_ids_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to parse telemetry_bundle_ids: {}", e),
            )
        })?;
    let adapter_state: AdapterStateSnapshot = serde_json::from_str(&session.adapter_state_json)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to parse adapter_state: {}", e),
            )
        })?;
    let routing_decisions: Vec<serde_json::Value> =
        serde_json::from_str(&session.routing_decisions_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to parse routing_decisions: {}", e),
            )
        })?;
    let inference_traces = session
        .inference_traces_json
        .as_ref()
        .map(|json| serde_json::from_str(json))
        .transpose()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to parse inference_traces: {}", e),
            )
        })?;

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
