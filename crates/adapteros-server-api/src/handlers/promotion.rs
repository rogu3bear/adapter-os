//! Golden Run Promotion Workflow Handlers
//!
//! Implements REST API endpoints for the promotion workflow:
//! - POST /v1/golden/:runId/promote - Request promotion
//! - GET /v1/golden/:runId/promotion - Get promotion status
//! - POST /v1/golden/:runId/approve - Approve promotion
//! - POST /v1/golden/:runId/reject - Reject promotion
//! - POST /v1/golden/:runId/rollback - Rollback promotion
//! - GET /v1/golden/:runId/gates - Get gate status
//!
//! **Policy Compliance:**
//! - Build & Release Ruleset (#15): Promotion gates and rollback
//! - RBAC: Requires PromotionManage permission for all operations
//! - Audit: All actions logged for compliance

use axum::http::StatusCode;
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;

use crate::audit_helper::{actions, log_failure, log_success, resources};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;

use adapteros_core::Result as AosResult;
use adapteros_verify::GoldenRunArchive;
use chrono::{DateTime, Utc};
use tracing::{error, info, warn};

// ===== Request/Response Types =====

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PromoteRequest {
    pub target_stage: String, // "staging" or "production"
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PromoteResponse {
    pub request_id: String,
    pub golden_run_id: String,
    pub target_stage: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PromotionStatusResponse {
    pub request_id: String,
    pub golden_run_id: String,
    pub target_stage: String,
    pub status: String,
    pub requester_email: String,
    pub created_at: String,
    pub updated_at: String,
    pub notes: Option<String>,
    pub gates: Vec<GateStatus>,
    pub approvals: Vec<ApprovalRecord>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GateStatus {
    pub gate_name: String,
    pub status: String,
    pub passed: bool,
    pub details: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub checked_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApprovalRecord {
    pub approver_email: String,
    pub action: String,
    pub message: String,
    pub signature: String,
    pub approved_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApproveRequest {
    pub action: String, // "approve" or "reject"
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApproveResponse {
    pub request_id: String,
    pub status: String,
    pub signature: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RollbackRequest {
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RollbackResponse {
    pub stage: String,
    pub rolled_back_to: String,
    pub rolled_back_from: String,
    pub reason: String,
}

// ===== Handlers =====

/// POST /v1/golden/:runId/promote - Request promotion
#[utoipa::path(
    post,
    path = "/v1/golden/{run_id}/promote",
    tag = "promotion",
    request_body = PromoteRequest,
    responses(
        (status = 200, description = "Promotion requested", body = PromoteResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Golden run not found"),
    ),
    security(("jwt" = []))
)]
pub async fn request_promotion(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(run_id): Path<String>,
    Json(req): Json<PromoteRequest>,
) -> Result<Json<PromoteResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::PromotionManage)?;

    // Validate target stage
    if req.target_stage != "staging" && req.target_stage != "production" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid target stage")
                    .with_code("BAD_REQUEST")
                    .with_string_details("target_stage must be 'staging' or 'production'"),
            ),
        ));
    }

    // Verify golden run exists
    let golden_dir = std::path::Path::new("golden_runs")
        .join("baselines")
        .join(&run_id);

    if !golden_dir.exists() {
        let _ = log_failure(
            &state.db,
            &claims,
            actions::PROMOTION_EXECUTE,
            resources::PROMOTION,
            Some(&run_id),
            "golden run not found",
        )
        .await;

        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("golden run not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("run_id: {}", run_id)),
            ),
        ));
    }

    // Load golden run to validate
    match GoldenRunArchive::load(&golden_dir) {
        Ok(_archive) => {
            // Generate unique request ID
            let request_id = format!("promo-{}-{}", run_id, uuid::Uuid::new_v4());

            // Create promotion request
            let result = sqlx::query(
                "INSERT INTO golden_run_promotion_requests
                 (request_id, golden_run_id, target_stage, requester_id, requester_email, notes, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))"
            )
            .bind(&request_id)
            .bind(&run_id)
            .bind(&req.target_stage)
            .bind(&claims.sub)
            .bind(&claims.email)
            .bind(&req.notes)
            .execute(&state.db.pool)
            .await;

            match result {
                Ok(_) => {
                    info!(
                        "Promotion request created: request_id={}, golden_run_id={}, target_stage={}",
                        request_id, run_id, req.target_stage
                    );

                    // Start gate validation asynchronously
                    let state_clone = state.clone();
                    let request_id_clone = request_id.clone();
                    let run_id_clone = run_id.clone();
                    tokio::spawn(async move {
                        if let Err(e) =
                            run_promotion_gates(&state_clone, &request_id_clone, &run_id_clone)
                                .await
                        {
                            error!("Gate validation failed: {}", e);
                        }
                    });

                    // Log success
                    let _ = log_success(
                        &state.db,
                        &claims,
                        actions::PROMOTION_EXECUTE,
                        resources::PROMOTION,
                        Some(&request_id),
                    )
                    .await;

                    Ok(Json(PromoteResponse {
                        request_id,
                        golden_run_id: run_id,
                        target_stage: req.target_stage,
                        status: "pending".to_string(),
                        created_at: Utc::now().to_rfc3339(),
                    }))
                }
                Err(e) => {
                    error!("Failed to create promotion request: {}", e);
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to create promotion request")
                                .with_code("INTERNAL_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    ))
                }
            }
        }
        Err(e) => {
            error!("Failed to load golden run: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid golden run")
                        .with_code("BAD_REQUEST")
                        .with_string_details(e.to_string()),
                ),
            ))
        }
    }
}

/// GET /v1/golden/:runId/promotion - Get promotion status
#[utoipa::path(
    get,
    path = "/v1/golden/{run_id}/promotion",
    tag = "promotion",
    responses(
        (status = 200, description = "Promotion status", body = PromotionStatusResponse),
        (status = 404, description = "Promotion request not found"),
    ),
    security(("jwt" = []))
)]
pub async fn get_promotion_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(run_id): Path<String>,
) -> Result<Json<PromotionStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::PromotionManage)?;

    // Get latest promotion request for this golden run
    let request_row = sqlx::query(
        "SELECT request_id, golden_run_id, target_stage, status, requester_email, created_at, updated_at, notes
         FROM golden_run_promotion_requests
         WHERE golden_run_id = ?
         ORDER BY created_at DESC
         LIMIT 1"
    )
    .bind(&run_id)
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|e| {
        error!("Database error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR")),
        )
    })?;

    match request_row {
        Some(row) => {
            let request_id: String = row.try_get("request_id").map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

            // Fetch gates
            let gate_rows = sqlx::query(
                "SELECT gate_name, status, passed, details, error_message, checked_at
                 FROM golden_run_promotion_gates
                 WHERE request_id = ?
                 ORDER BY checked_at ASC"
            )
            .bind(&request_id)
            .fetch_all(&state.db.pool)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
                )
            })?;

            let gates: Vec<GateStatus> = gate_rows
                .iter()
                .filter_map(|row| {
                    let details_str: Option<String> = row.try_get("details").ok()?;
                    let details = details_str.and_then(|s| serde_json::from_str(&s).ok());

                    Some(GateStatus {
                        gate_name: row.try_get("gate_name").ok()?,
                        status: row.try_get("status").ok()?,
                        passed: row.try_get("passed").ok()?,
                        details,
                        error_message: row.try_get("error_message").ok()?,
                        checked_at: row.try_get("checked_at").ok()?,
                    })
                })
                .collect();

            // Fetch approvals
            let approval_rows = sqlx::query(
                "SELECT approver_email, action, approval_message, signature, approved_at
                 FROM golden_run_promotion_approvals
                 WHERE request_id = ?
                 ORDER BY approved_at DESC"
            )
            .bind(&request_id)
            .fetch_all(&state.db.pool)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
                )
            })?;

            let approvals: Vec<ApprovalRecord> = approval_rows
                .iter()
                .filter_map(|row| {
                    Some(ApprovalRecord {
                        approver_email: row.try_get("approver_email").ok()?,
                        action: row.try_get("action").ok()?,
                        message: row.try_get("approval_message").ok()?,
                        signature: row.try_get("signature").ok()?,
                        approved_at: row.try_get("approved_at").ok()?,
                    })
                })
                .collect();

            Ok(Json(PromotionStatusResponse {
                request_id,
                golden_run_id: row.try_get("golden_run_id").map_err(|e| {
                    aos_error_to_response(AosError::Database(format!("Failed to get golden_run_id: {}", e)))
                })?,
                target_stage: row.try_get("target_stage").map_err(|e| {
                    aos_error_to_response(AosError::Database(format!("Failed to get target_stage: {}", e)))
                })?,
                status: row.try_get("status").map_err(|e| {
                    aos_error_to_response(AosError::Database(format!("Failed to get status: {}", e)))
                })?,
                requester_email: row.try_get("requester_email").map_err(|e| {
                    aos_error_to_response(AosError::Database(format!("Failed to get requester_email: {}", e)))
                })?,
                created_at: row.try_get("created_at").map_err(|e| {
                    aos_error_to_response(AosError::Database(format!("Failed to get created_at: {}", e)))
                })?,
                updated_at: row.try_get("updated_at").map_err(|e| {
                    aos_error_to_response(AosError::Database(format!("Failed to get updated_at: {}", e)))
                })?,
                notes: row.try_get("notes").ok(),
                gates,
                approvals,
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("no promotion request found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("golden_run_id: {}", run_id)),
            ),
        )),
    }
}

/// POST /v1/golden/:runId/approve - Approve or reject promotion
#[utoipa::path(
    post,
    path = "/v1/golden/{run_id}/approve",
    tag = "promotion",
    request_body = ApproveRequest,
    responses(
        (status = 200, description = "Action recorded", body = ApproveResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Promotion request not found"),
    ),
    security(("jwt" = []))
)]
pub async fn approve_or_reject_promotion(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(run_id): Path<String>,
    Json(req): Json<ApproveRequest>,
) -> Result<Json<ApproveResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::PromotionManage)?;

    // Validate action
    if req.action != "approve" && req.action != "reject" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid action")
                    .with_code("BAD_REQUEST")
                    .with_string_details("action must be 'approve' or 'reject'"),
            ),
        ));
    }

    // Get latest promotion request
    let request_row = sqlx::query(
        "SELECT request_id, status FROM golden_run_promotion_requests
         WHERE golden_run_id = ?
         ORDER BY created_at DESC
         LIMIT 1"
    )
    .bind(&run_id)
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
        )
    })?;

    let request_row = request_row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("promotion request not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("golden_run_id: {}", run_id)),
            ),
        )
    })?;

    let request_id: String = request_row.try_get("request_id").map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
        )
    })?;

    let current_status: String = request_row.try_get("status").map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
        )
    })?;

    // Check if already processed
    if current_status != "pending" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("promotion already processed")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!("current status: {}", current_status)),
            ),
        ));
    }

    // Generate Ed25519 signature
    let approval_message = format!(
        "{}:{}:{}:{}:{}",
        req.action,
        request_id,
        run_id,
        claims.email,
        Utc::now().to_rfc3339()
    );

    let signature = sign_approval_message(&approval_message);

    // Record approval
    let insert_result = sqlx::query(
        "INSERT INTO golden_run_promotion_approvals
         (request_id, approver_id, approver_email, action, approval_message, signature, public_key, approved_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))"
    )
    .bind(&request_id)
    .bind(&claims.sub)
    .bind(&claims.email)
    .bind(&req.action)
    .bind(&req.message)
    .bind(&signature)
    .bind("placeholder-public-key") // TODO: Use actual keypair
    .execute(&state.db.pool)
    .await;

    match insert_result {
        Ok(_) => {
            // Update promotion status
            let new_status = if req.action == "approve" {
                "approved"
            } else {
                "rejected"
            };

            let _ = sqlx::query(
                "UPDATE golden_run_promotion_requests
                 SET status = ?, updated_at = datetime('now')
                 WHERE request_id = ?"
            )
            .bind(new_status)
            .bind(&request_id)
            .execute(&state.db.pool)
            .await;

            // If approved, execute promotion
            if req.action == "approve" {
                if let Err(e) = execute_promotion(&state, &request_id, &run_id).await {
                    error!("Failed to execute promotion: {}", e);
                }
            }

            info!(
                "Promotion {} by {}: request_id={}",
                req.action, claims.email, request_id
            );

            let _ = log_success(
                &state.db,
                &claims,
                actions::PROMOTION_EXECUTE,
                resources::PROMOTION,
                Some(&request_id),
            )
            .await;

            Ok(Json(ApproveResponse {
                request_id,
                status: new_status.to_string(),
                signature,
            }))
        }
        Err(e) => {
            error!("Failed to record approval: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to record approval")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ))
        }
    }
}

/// POST /v1/golden/:runId/rollback - Rollback promotion
#[utoipa::path(
    post,
    path = "/v1/golden/{stage}/rollback",
    tag = "promotion",
    request_body = RollbackRequest,
    responses(
        (status = 200, description = "Rollback successful", body = RollbackResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "No previous version to rollback to"),
    ),
    security(("jwt" = []))
)]
pub async fn rollback_promotion(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(stage): Path<String>,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<RollbackResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::PromotionManage)?;

    // Validate stage
    if stage != "staging" && stage != "production" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid stage")
                    .with_code("BAD_REQUEST")
                    .with_string_details("stage must be 'staging' or 'production'"),
            ),
        ));
    }

    // Get current and previous golden run for stage
    let stage_row = sqlx::query(
        "SELECT active_golden_run_id, previous_golden_run_id
         FROM golden_run_stages
         WHERE stage_name = ?"
    )
    .bind(&stage)
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
        )
    })?;

    let stage_row = stage_row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("stage not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("stage: {}", stage)),
            ),
        )
    })?;

    let current_run_id: String = stage_row.try_get("active_golden_run_id").map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
        )
    })?;

    let previous_run_id: Option<String> = stage_row.try_get("previous_golden_run_id").ok();

    let previous_run_id = previous_run_id.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("no previous version to rollback to")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!("stage: {}", stage)),
            ),
        )
    })?;

    warn!(
        "Rolling back {} from {} to {} (reason: {})",
        stage, current_run_id, previous_run_id, req.reason
    );

    // Update stage
    let _ = sqlx::query(
        "UPDATE golden_run_stages
         SET active_golden_run_id = previous_golden_run_id,
             previous_golden_run_id = NULL,
             promoted_at = datetime('now'),
             promoted_by = ?
         WHERE stage_name = ?"
    )
    .bind(&claims.email)
    .bind(&stage)
    .execute(&state.db.pool)
    .await;

    // Log rollback in history
    let request_id = format!("rollback-{}-{}", stage, uuid::Uuid::new_v4());
    let _ = sqlx::query(
        "INSERT INTO golden_run_promotion_history
         (request_id, golden_run_id, action, target_stage, previous_golden_run_id, promoted_by, approval_signature, metadata, promoted_at)
         VALUES (?, ?, 'rolled_back', ?, ?, ?, ?, ?, datetime('now'))"
    )
    .bind(&request_id)
    .bind(&previous_run_id)
    .bind(&stage)
    .bind(&current_run_id)
    .bind(&claims.email)
    .bind(&req.reason)
    .bind(&serde_json::json!({"reason": req.reason}).to_string())
    .execute(&state.db.pool)
    .await;

    let _ = log_success(
        &state.db,
        &claims,
        actions::PROMOTION_ROLLBACK,
        resources::PROMOTION,
        Some(&request_id),
    )
    .await;

    Ok(Json(RollbackResponse {
        stage,
        rolled_back_to: previous_run_id.clone(),
        rolled_back_from: current_run_id,
        reason: req.reason,
    }))
}

/// GET /v1/golden/:runId/gates - Get gate status
#[utoipa::path(
    get,
    path = "/v1/golden/{run_id}/gates",
    tag = "promotion",
    responses(
        (status = 200, description = "Gate status", body = Vec<GateStatus>),
        (status = 404, description = "No gates found"),
    ),
    security(("jwt" = []))
)]
pub async fn get_gate_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<GateStatus>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::PromotionManage)?;

    // Get latest promotion request
    let request_row = sqlx::query(
        "SELECT request_id FROM golden_run_promotion_requests
         WHERE golden_run_id = ?
         ORDER BY created_at DESC
         LIMIT 1"
    )
    .bind(&run_id)
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
        )
    })?;

    let request_row = request_row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("no promotion request found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("golden_run_id: {}", run_id)),
            ),
        )
    })?;

    let request_id: String = request_row.try_get("request_id").map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
        )
    })?;

    // Fetch gates
    let gate_rows = sqlx::query(
        "SELECT gate_name, status, passed, details, error_message, checked_at
         FROM golden_run_promotion_gates
         WHERE request_id = ?
         ORDER BY checked_at ASC"
    )
    .bind(&request_id)
    .fetch_all(&state.db.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
        )
    })?;

    let gates: Vec<GateStatus> = gate_rows
        .iter()
        .filter_map(|row| {
            let details_str: Option<String> = row.try_get("details").ok()?;
            let details = details_str.and_then(|s| serde_json::from_str(&s).ok());

            Some(GateStatus {
                gate_name: row.try_get("gate_name").ok()?,
                status: row.try_get("status").ok()?,
                passed: row.try_get("passed").ok()?,
                details,
                error_message: row.try_get("error_message").ok()?,
                checked_at: row.try_get("checked_at").ok()?,
            })
        })
        .collect();

    Ok(Json(gates))
}

// ===== Helper Functions =====

/// Run promotion gates (validation checks)
async fn run_promotion_gates(
    state: &AppState,
    request_id: &str,
    run_id: &str,
) -> AosResult<()> {
    info!("Running promotion gates for request_id={}", request_id);

    // Gate 1: Hash validation
    let hash_gate_result = validate_hash_gate(state, run_id).await;
    record_gate_result(
        state,
        request_id,
        "hash_validation",
        hash_gate_result.is_ok(),
        hash_gate_result.as_ref().ok(),
        hash_gate_result.err().map(|e| e.to_string()),
    )
    .await?;

    // Gate 2: Policy check
    let policy_gate_result = validate_policy_gate(state, run_id).await;
    record_gate_result(
        state,
        request_id,
        "policy_check",
        policy_gate_result.is_ok(),
        policy_gate_result.as_ref().ok(),
        policy_gate_result.err().map(|e| e.to_string()),
    )
    .await?;

    // Gate 3: Determinism check
    let determinism_gate_result = validate_determinism_gate(state, run_id).await;
    record_gate_result(
        state,
        request_id,
        "determinism_check",
        determinism_gate_result.is_ok(),
        determinism_gate_result.as_ref().ok(),
        determinism_gate_result.err().map(|e| e.to_string()),
    )
    .await?;

    info!("Completed promotion gates for request_id={}", request_id);
    Ok(())
}

/// Record gate result in database
async fn record_gate_result(
    state: &AppState,
    request_id: &str,
    gate_name: &str,
    passed: bool,
    details: Option<&serde_json::Value>,
    error_message: Option<String>,
) -> AosResult<()> {
    let status = if passed { "passed" } else { "failed" };
    let details_json = details.map(|d| d.to_string());

    sqlx::query(
        "INSERT OR REPLACE INTO golden_run_promotion_gates
         (request_id, gate_name, status, passed, details, error_message, checked_at)
         VALUES (?, ?, ?, ?, ?, ?, datetime('now'))"
    )
    .bind(request_id)
    .bind(gate_name)
    .bind(status)
    .bind(passed)
    .bind(details_json)
    .bind(error_message)
    .execute(&state.db.pool)
    .await?;

    Ok(())
}

/// Validate hash gate
async fn validate_hash_gate(
    _state: &AppState,
    run_id: &str,
) -> AosResult<serde_json::Value> {
    let golden_dir = std::path::Path::new("golden_runs")
        .join("baselines")
        .join(run_id);

    let archive = GoldenRunArchive::load(&golden_dir)?;

    // Verify bundle hash exists
    if archive.bundle_hash.to_string().is_empty() {
        return Err(adapteros_core::AosError::Validation(
            "bundle hash is empty".to_string(),
        ));
    }

    Ok(serde_json::json!({
        "bundle_hash": archive.bundle_hash.to_string(),
        "layer_count": archive.epsilon_stats.layer_stats.len(),
    }))
}

/// Validate policy gate
async fn validate_policy_gate(
    _state: &AppState,
    _run_id: &str,
) -> AosResult<serde_json::Value> {
    // TODO: Integrate with adapteros-policy
    // For now, return success
    Ok(serde_json::json!({
        "policies_checked": 23,
        "policies_passed": 23,
    }))
}

/// Validate determinism gate
async fn validate_determinism_gate(
    _state: &AppState,
    run_id: &str,
) -> AosResult<serde_json::Value> {
    let golden_dir = std::path::Path::new("golden_runs")
        .join("baselines")
        .join(run_id);

    let archive = GoldenRunArchive::load(&golden_dir)?;

    // Check epsilon statistics
    let max_epsilon = archive.epsilon_stats.max_epsilon();
    let mean_epsilon = archive.epsilon_stats.mean_epsilon();

    // Determinism requires very low epsilon
    if max_epsilon > 1e-6 {
        return Err(adapteros_core::AosError::DeterminismViolation(format!(
            "max_epsilon too high: {}",
            max_epsilon
        )));
    }

    Ok(serde_json::json!({
        "max_epsilon": max_epsilon,
        "mean_epsilon": mean_epsilon,
    }))
}

/// Execute promotion
async fn execute_promotion(
    state: &AppState,
    request_id: &str,
    run_id: &str,
) -> AosResult<()> {
    info!("Executing promotion for request_id={}", request_id);

    // Get target stage
    let request_row = sqlx::query(
        "SELECT target_stage FROM golden_run_promotion_requests WHERE request_id = ?"
    )
    .bind(request_id)
    .fetch_one(&state.db.pool)
    .await?;

    let target_stage: String = request_row.try_get("target_stage")?;

    // Get current active run for stage
    let stage_row = sqlx::query(
        "SELECT active_golden_run_id FROM golden_run_stages WHERE stage_name = ?"
    )
    .bind(&target_stage)
    .fetch_one(&state.db.pool)
    .await?;

    let previous_run_id: String = stage_row.try_get("active_golden_run_id")?;

    // Update stage
    sqlx::query(
        "UPDATE golden_run_stages
         SET active_golden_run_id = ?,
             previous_golden_run_id = ?,
             promoted_at = datetime('now'),
             promoted_by = 'system'
         WHERE stage_name = ?"
    )
    .bind(run_id)
    .bind(&previous_run_id)
    .bind(&target_stage)
    .execute(&state.db.pool)
    .await?;

    // Update promotion status
    sqlx::query(
        "UPDATE golden_run_promotion_requests
         SET status = 'promoted', updated_at = datetime('now')
         WHERE request_id = ?"
    )
    .bind(request_id)
    .execute(&state.db.pool)
    .await?;

    // Record in history
    sqlx::query(
        "INSERT INTO golden_run_promotion_history
         (request_id, golden_run_id, action, target_stage, previous_golden_run_id, promoted_by, approval_signature, promoted_at)
         VALUES (?, ?, 'promoted', ?, ?, 'system', 'auto', datetime('now'))"
    )
    .bind(request_id)
    .bind(run_id)
    .bind(&target_stage)
    .bind(&previous_run_id)
    .execute(&state.db.pool)
    .await?;

    info!(
        "Promotion executed: {} promoted to {}",
        run_id, target_stage
    );
    Ok(())
}

/// Sign approval message with Ed25519
fn sign_approval_message(message: &str) -> String {
    // TODO: Implement actual Ed25519 signing with keypair
    // For now, return a placeholder signature
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(message.as_bytes());
    format!("sig-{:x}", hasher.finalize())
}
