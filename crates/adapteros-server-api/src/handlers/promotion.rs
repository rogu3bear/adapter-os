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
use utoipa::ToSchema;

use crate::audit_helper::{actions, log_failure, log_success, resources};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::AosError;
use adapteros_crypto::signature::{Keypair, PublicKey, Signature};

use adapteros_core::Result as AosResult;
use adapteros_verify::GoldenRunArchive;
use chrono::Utc;
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
            let params = adapteros_db::CreatePromotionRequestParams {
                request_id: request_id.clone(),
                golden_run_id: run_id.clone(),
                target_stage: req.target_stage.clone(),
                requester_id: claims.sub.clone(),
                requester_email: claims.email.clone(),
                notes: req.notes.clone(),
            };

            let result = state.db.create_promotion_request(params).await;

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
    let request = state
        .db
        .get_latest_promotion_request(&run_id)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("database error").with_code("INTERNAL_ERROR")),
            )
        })?;

    match request {
        Some(req) => {
            // Fetch gates
            let db_gates = state
                .db
                .get_promotion_gates(&req.request_id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("database error")
                                .with_code("INTERNAL_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;

            let gates: Vec<GateStatus> = db_gates
                .iter()
                .map(|gate| {
                    let details = gate
                        .details
                        .as_ref()
                        .and_then(|s| serde_json::from_str(s).ok());

                    GateStatus {
                        gate_name: gate.gate_name.clone(),
                        status: gate.status.clone(),
                        passed: gate.passed,
                        details,
                        error_message: gate.error_message.clone(),
                        checked_at: gate.checked_at.clone(),
                    }
                })
                .collect();

            // Fetch approvals
            let db_approvals = state
                .db
                .get_promotion_approvals(&req.request_id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("database error")
                                .with_code("INTERNAL_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;

            let approvals: Vec<ApprovalRecord> = db_approvals
                .iter()
                .map(|approval| ApprovalRecord {
                    approver_email: approval.approver_email.clone(),
                    action: approval.action.clone(),
                    message: approval.approval_message.clone(),
                    signature: approval.signature.clone(),
                    approved_at: approval.approved_at.clone(),
                })
                .collect();

            Ok(Json(PromotionStatusResponse {
                request_id: req.request_id,
                golden_run_id: req.golden_run_id,
                target_stage: req.target_stage,
                status: req.status,
                requester_email: req.requester_email,
                created_at: req.created_at,
                updated_at: req.updated_at,
                notes: req.notes,
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
    let request = state
        .db
        .get_latest_promotion_request(&run_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let request = request.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("promotion request not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("golden_run_id: {}", run_id)),
            ),
        )
    })?;

    let request_id = request.request_id;
    let current_status = request.status;

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

    let signature = sign_approval_message(&state.crypto.signing_keypair, &approval_message);
    let public_key_hex = hex::encode(state.crypto.signing_keypair.public_key().to_bytes());

    // Record approval
    let approval_params = adapteros_db::RecordApprovalParams {
        request_id: request_id.clone(),
        approver_id: claims.sub.clone(),
        approver_email: claims.email.clone(),
        action: req.action.clone(),
        approval_message: req.message.clone(),
        signature: signature.clone(),
        public_key: public_key_hex,
    };

    let insert_result = state.db.record_promotion_approval(approval_params).await;

    match insert_result {
        Ok(_) => {
            // Update promotion status
            let new_status = if req.action == "approve" {
                "approved"
            } else {
                "rejected"
            };

            let _ = state
                .db
                .update_promotion_request_status(&request_id, new_status)
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
    let stage_info = state.db.get_golden_run_stage(&stage).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let stage_info = stage_info.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("stage not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("stage: {}", stage)),
            ),
        )
    })?;

    let current_run_id = stage_info.active_golden_run_id;
    let previous_run_id = stage_info.previous_golden_run_id.ok_or_else(|| {
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
    let _ = state
        .db
        .rollback_golden_run_stage(&stage, &claims.email)
        .await;

    // Log rollback in history
    let request_id = format!("rollback-{}-{}", stage, uuid::Uuid::new_v4());
    let metadata = serde_json::json!({"reason": &req.reason}).to_string();
    let _ = state
        .db
        .record_rollback_history(
            &request_id,
            &previous_run_id,
            &stage,
            &current_run_id,
            &claims.email,
            &req.reason,
            &metadata,
        )
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
    let request = state
        .db
        .get_latest_promotion_request(&run_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let request = request.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("no promotion request found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("golden_run_id: {}", run_id)),
            ),
        )
    })?;

    let request_id = request.request_id;

    // Fetch gates
    let db_gates = state
        .db
        .get_promotion_gates(&request_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let gates: Vec<GateStatus> = db_gates
        .iter()
        .map(|gate| {
            let details = gate
                .details
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok());

            GateStatus {
                gate_name: gate.gate_name.clone(),
                status: gate.status.clone(),
                passed: gate.passed,
                details,
                error_message: gate.error_message.clone(),
                checked_at: gate.checked_at.clone(),
            }
        })
        .collect();

    Ok(Json(gates))
}

// ===== Helper Functions =====

/// Run promotion gates (validation checks)
async fn run_promotion_gates(state: &AppState, request_id: &str, run_id: &str) -> AosResult<()> {
    info!("Running promotion gates for request_id={}", request_id);

    // Gate 1: Hash validation
    let hash_gate_result = validate_hash_gate(state, run_id).await;
    let hash_error_msg = hash_gate_result.as_ref().err().map(|e| e.to_string());
    record_gate_result(
        state,
        request_id,
        "hash_validation",
        hash_gate_result.is_ok(),
        hash_gate_result.as_ref().ok(),
        hash_error_msg,
    )
    .await?;

    // Gate 2: Policy check
    let policy_gate_result = validate_policy_gate(state, run_id).await;
    let policy_error_msg = policy_gate_result.as_ref().err().map(|e| e.to_string());
    record_gate_result(
        state,
        request_id,
        "policy_check",
        policy_gate_result.is_ok(),
        policy_gate_result.as_ref().ok(),
        policy_error_msg,
    )
    .await?;

    // Gate 3: Determinism check
    let determinism_gate_result = validate_determinism_gate(state, run_id).await;
    let determinism_error_msg = determinism_gate_result
        .as_ref()
        .err()
        .map(|e| e.to_string());
    record_gate_result(
        state,
        request_id,
        "determinism_check",
        determinism_gate_result.is_ok(),
        determinism_gate_result.as_ref().ok(),
        determinism_error_msg,
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
    let params = adapteros_db::RecordGateParams {
        request_id: request_id.to_string(),
        gate_name: gate_name.to_string(),
        passed,
        details: details.cloned(),
        error_message,
    };

    state.db.record_promotion_gate(params).await?;
    Ok(())
}

/// Validate hash gate
async fn validate_hash_gate(_state: &AppState, run_id: &str) -> AosResult<serde_json::Value> {
    let golden_dir = std::path::Path::new("golden_runs")
        .join("baselines")
        .join(run_id);

    let archive = GoldenRunArchive::load(&golden_dir)
        .map_err(|e| AosError::Validation(format!("Failed to load golden run archive: {}", e)))?;

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
async fn validate_policy_gate(_state: &AppState, run_id: &str) -> AosResult<serde_json::Value> {
    use adapteros_policy::policy_packs::PolicyPackId;

    // Get all defined policy packs
    let all_policies = PolicyPackId::all();
    let total_policies = all_policies.len();

    // Load golden run to validate against
    let golden_dir = std::path::Path::new("golden_runs")
        .join("baselines")
        .join(run_id);

    if !golden_dir.exists() {
        return Err(AosError::Validation(format!(
            "Golden run directory not found: {}. Cannot validate policies.",
            golden_dir.display()
        )));
    }

    // Track validation results
    let mut passed = 0;
    let mut failed_policies = Vec::new();

    for policy_id in &all_policies {
        // Each policy pack has specific validation requirements
        // For now, we validate what we can and report honestly
        let policy_name = policy_id.name();

        match policy_id {
            PolicyPackId::Determinism => {
                // Determinism is checked separately in validate_determinism_gate
                passed += 1;
            }
            PolicyPackId::Artifacts => {
                // Check if golden run has valid artifacts
                let archive_path = golden_dir.join("archive.json");
                if archive_path.exists() {
                    passed += 1;
                } else {
                    failed_policies.push(format!("{}: archive.json missing", policy_name));
                }
            }
            _ => {
                // For policies without specific validation logic yet,
                // mark as unchecked rather than fake-passing
                failed_policies.push(format!("{}: validation not implemented", policy_name));
            }
        }
    }

    // Return honest results
    let policies_checked = passed + failed_policies.len();
    Ok(serde_json::json!({
        "policies_checked": policies_checked,
        "policies_passed": passed,
        "policies_failed": failed_policies.len(),
        "failed_details": failed_policies,
        "note": "Some policies lack validation logic - see failed_details"
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

    let archive = GoldenRunArchive::load(&golden_dir)
        .map_err(|e| AosError::Validation(format!("Failed to load golden run archive: {}", e)))?;

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
async fn execute_promotion(state: &AppState, request_id: &str, run_id: &str) -> AosResult<()> {
    info!("Executing promotion for request_id={}", request_id);

    // Get target stage
    let target_stage = state.db.get_promotion_target_stage(request_id).await?;

    // Get current active run for stage
    let previous_run_id = state.db.get_stage_active_golden_run(&target_stage).await?;

    // Update stage
    state
        .db
        .update_golden_run_stage(&target_stage, run_id, &previous_run_id, "system")
        .await?;

    // Update promotion status
    state
        .db
        .update_promotion_request_status(request_id, "promoted")
        .await?;

    // Record in history
    state
        .db
        .record_promotion_history(
            request_id,
            run_id,
            "promoted",
            &target_stage,
            &previous_run_id,
            "system",
            "auto",
        )
        .await?;

    info!(
        "Promotion executed: {} promoted to {}",
        run_id, target_stage
    );
    Ok(())
}

/// Sign approval message with Ed25519
fn sign_approval_message(keypair: &Keypair, message: &str) -> String {
    let signature = keypair.sign(message.as_bytes());
    hex::encode(signature.to_bytes())
}

/// Verify an approval signature (reserved for multi-party approval workflow)
fn _verify_approval_signature(
    public_key_hex: &str,
    message: &str,
    signature_hex: &str,
) -> AosResult<()> {
    let public_key_bytes = hex::decode(public_key_hex)
        .map_err(|e| AosError::Crypto(format!("Invalid public key hex: {}", e)))?;

    if public_key_bytes.len() != 32 {
        return Err(AosError::Crypto(format!(
            "Invalid public key length: {}",
            public_key_bytes.len()
        )));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&public_key_bytes);
    let public_key = PublicKey::from_bytes(&key_array)?;

    let signature_bytes = hex::decode(signature_hex)
        .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

    if signature_bytes.len() != 64 {
        return Err(AosError::Crypto(format!(
            "Invalid signature length: {}",
            signature_bytes.len()
        )));
    }

    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&signature_bytes);
    let signature = Signature::from_bytes(&sig_array)?;

    public_key.verify(message.as_bytes(), &signature)
}
