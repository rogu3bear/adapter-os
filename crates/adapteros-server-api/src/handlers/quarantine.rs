//! Policy quarantine management handlers.
//!
//! API handlers for clearing and rolling back policy quarantine violations.

use crate::audit_helper::{actions, log_success_or_warn, resources};
use crate::auth::Claims;
use crate::state::AppState;
use adapteros_api_types::{
    ClearQuarantineRequest, ClearQuarantineResponse, ErrorResponse, QuarantineStatusResponse,
    RollbackQuarantineRequest, RollbackQuarantineResponse,
};
use adapteros_db::users::Role;
use axum::{extract::State, http::StatusCode, Extension, Json};
use tracing::{error, info, warn};

/// Get current quarantine status.
///
/// Returns the current quarantine state including whether the system is quarantined,
/// violation details, and count of active violations.
#[utoipa::path(
    get,
    path = "/v1/policy/quarantine/status",
    tag = "Policy",
    responses(
        (status = 200, description = "Quarantine status", body = QuarantineStatusResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_quarantine_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<QuarantineStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require admin role for quarantine status
    crate::middleware::require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let (quarantined, violation_summary, violation_count) =
        if let Some(ref policy_watcher) = state.policy_watcher {
            let violations = policy_watcher.get_violations();
            let is_quarantined = !violations.is_empty();
            let summary = if is_quarantined {
                Some(
                    violations
                        .iter()
                        .map(|v| format!("{}: {}", v.policy_pack_id, v.message))
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            } else {
                None
            };
            (is_quarantined, summary, violations.len())
        } else if let Some(ref federation_daemon) = state.federation_daemon {
            let is_quarantined = federation_daemon.is_quarantined();
            let summary = if is_quarantined {
                Some(federation_daemon.quarantine_status())
            } else {
                None
            };
            (is_quarantined, summary, if is_quarantined { 1 } else { 0 })
        } else {
            (false, None, 0)
        };

    let message = if quarantined {
        format!(
            "System is QUARANTINED with {} violation(s)",
            violation_count
        )
    } else {
        "System is OPERATIONAL - no quarantine violations".to_string()
    };

    Ok(Json(QuarantineStatusResponse {
        quarantined,
        violation_summary,
        violation_count,
        message,
    }))
}

/// Clear policy quarantine violations.
///
/// Clears violations for a specific policy pack or all violations. Optionally
/// reloads the baseline cache from the database (rollback mode).
#[utoipa::path(
    post,
    path = "/v1/policy/quarantine/clear",
    tag = "Policy",
    request_body = ClearQuarantineRequest,
    responses(
        (status = 200, description = "Quarantine cleared", body = ClearQuarantineResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - requires admin role"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn clear_policy_violations(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ClearQuarantineRequest>,
) -> Result<Json<ClearQuarantineResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require admin role for quarantine clear
    crate::middleware::require_any_role(&claims, &[Role::Admin])?;

    let operator = req.operator.clone().unwrap_or_else(|| claims.email.clone());

    info!(
        operator = %operator,
        pack_id = ?req.pack_id,
        rollback = req.rollback,
        cpid = ?req.cpid,
        "Clearing policy quarantine violations"
    );

    // Get policy watcher
    let policy_watcher = state.policy_watcher.as_ref().ok_or_else(|| {
        error!("Policy watcher not available");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Policy watcher not initialized")
                    .with_code("SERVICE_UNAVAILABLE"),
            ),
        )
    })?;

    // Get violations before clearing for counting
    let violations_before = if let Some(ref pack_id) = req.pack_id {
        policy_watcher.get_violations_for_pack(pack_id)
    } else {
        policy_watcher.get_violations()
    };
    let violations_cleared = violations_before.len();

    // Collect pack IDs that will be cleared
    let cleared_packs: Vec<String> = if let Some(ref pack_id) = req.pack_id {
        vec![pack_id.clone()]
    } else {
        violations_before
            .iter()
            .map(|v| v.policy_pack_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    };

    // Clear violations (with optional cache reload)
    if let Some(ref pack_id) = req.pack_id {
        policy_watcher
            .clear_violations_with_reload(pack_id, req.rollback)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to clear violations for pack");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to clear violations")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    } else {
        policy_watcher
            .clear_all_violations_with_reload(req.rollback)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to clear all violations");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to clear violations")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    // Also release quarantine on federation daemon if present
    if let Some(ref federation_daemon) = state.federation_daemon {
        if let Some(ref pack_id) = req.pack_id {
            federation_daemon.release_quarantine_for_pack(pack_id);
        } else {
            federation_daemon.release_quarantine();
        }
    }

    // Log to audit trail
    let action = if req.pack_id.is_some() {
        actions::POLICY_QUARANTINE_CLEAR_PACK
    } else {
        actions::POLICY_QUARANTINE_CLEAR
    };
    let resource_id = req.pack_id.as_deref().unwrap_or("all");
    log_success_or_warn(&state.db, &claims, action, resources::POLICY, Some(resource_id)).await;

    let message = if violations_cleared > 0 {
        format!(
            "Cleared {} violation(s) for {} pack(s){}",
            violations_cleared,
            cleared_packs.len(),
            if req.rollback {
                " (baseline cache reloaded)"
            } else {
                ""
            }
        )
    } else {
        "No violations to clear".to_string()
    };

    info!(
        operator = %operator,
        violations_cleared = violations_cleared,
        cleared_packs = ?cleared_packs,
        rollback = req.rollback,
        "Quarantine violations cleared"
    );

    Ok(Json(ClearQuarantineResponse {
        success: true,
        cleared_packs,
        violations_cleared,
        message,
        cache_reloaded: req.rollback,
    }))
}

/// Rollback to last known good policy configuration.
///
/// Reloads baseline hashes from the database and clears all quarantine violations.
/// This is a convenience endpoint that combines cache reload with violation clearing.
#[utoipa::path(
    post,
    path = "/v1/policy/quarantine/rollback",
    tag = "Policy",
    request_body = RollbackQuarantineRequest,
    responses(
        (status = 200, description = "Rollback successful", body = RollbackQuarantineResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - requires admin role"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn rollback_policy_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RollbackQuarantineRequest>,
) -> Result<Json<RollbackQuarantineResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require admin role for rollback
    crate::middleware::require_any_role(&claims, &[Role::Admin])?;

    let operator = req.operator.clone().unwrap_or_else(|| claims.email.clone());

    info!(
        operator = %operator,
        cpid = ?req.cpid,
        "Rolling back policy configuration"
    );

    // Get policy watcher
    let policy_watcher = state.policy_watcher.as_ref().ok_or_else(|| {
        error!("Policy watcher not available");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Policy watcher not initialized")
                    .with_code("SERVICE_UNAVAILABLE"),
            ),
        )
    })?;

    // Get violations before clearing for counting
    let violations_before = policy_watcher.get_violations();
    let violations_cleared = violations_before.len();

    // Reload cache and clear all violations
    policy_watcher
        .clear_all_violations_with_reload(true)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to rollback policy configuration");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to rollback policy configuration")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Also release quarantine on federation daemon if present
    if let Some(ref federation_daemon) = state.federation_daemon {
        federation_daemon.release_quarantine();
    }

    // Log to audit trail
    log_success_or_warn(
        &state.db,
        &claims,
        actions::POLICY_QUARANTINE_ROLLBACK,
        resources::POLICY,
        Some("all"),
    )
    .await;

    // Check if system is still quarantined after rollback
    let still_quarantined = !policy_watcher.get_violations().is_empty();

    let message = if violations_cleared > 0 {
        format!(
            "Rolled back policy configuration, cleared {} violation(s), cache reloaded from database",
            violations_cleared
        )
    } else {
        "Policy configuration rolled back, baseline cache reloaded".to_string()
    };

    info!(
        operator = %operator,
        violations_cleared = violations_cleared,
        still_quarantined = still_quarantined,
        "Policy rollback completed"
    );

    Ok(Json(RollbackQuarantineResponse {
        success: true,
        violations_cleared,
        message,
        still_quarantined,
    }))
}
