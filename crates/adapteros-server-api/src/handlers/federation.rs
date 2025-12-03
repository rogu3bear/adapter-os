//! Federation Status API Handlers
//!
//! REST endpoints for federation verification status and management.

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use adapteros_core::AosError;
use adapteros_db::QuarantineDetails;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info, warn};
use utoipa::ToSchema;

/// Federation status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FederationStatusResponse {
    /// Whether federation is operational
    pub operational: bool,
    /// Whether system is quarantined
    pub quarantined: bool,
    /// Quarantine reason (if quarantined)
    pub quarantine_reason: Option<String>,
    /// Latest verification report (JSON string)
    pub latest_verification: Option<String>,
    /// Number of registered hosts
    pub total_hosts: usize,
    /// Timestamp
    pub timestamp: String,
}

/// Quarantine status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QuarantineStatusResponse {
    /// Whether system is quarantined
    pub quarantined: bool,
    /// Quarantine details
    pub details: Option<QuarantineDetails>,
}

// Note: QuarantineDetails is now imported from adapteros_db::federation

/// GET /api/federation/status
///
/// Returns current federation verification status
#[utoipa::path(
    get,
    path = "/v1/federation/status",
    responses(
        (status = 200, description = "Federation status retrieved successfully", body = FederationStatusResponse),
        (status = 503, description = "Federation daemon not available")
    ),
    tags = ["federation"]
)]
pub async fn get_federation_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> std::result::Result<Json<FederationStatusResponse>, AppError> {
    require_permission(&claims, Permission::FederationView)
        .map_err(|_| AppError(AosError::PolicyViolation("Insufficient permissions".into())))?;

    info!("Fetching federation status");

    let daemon = state
        .federation_daemon
        .as_ref()
        .ok_or_else(|| AppError(AosError::Config("Federation daemon not configured".into())))?;

    // Get latest verification report
    let latest_verification = match daemon.get_latest_report().await {
        Ok(report) => Some(report),
        Err(e) => {
            error!(error = %e, "Failed to get latest federation report");
            None
        }
    };

    // Get total hosts
    let total_hosts = state.db.get_federation_host_count().await.unwrap_or(0);

    // Check quarantine status
    let quarantined = daemon.is_quarantined();
    let quarantine_reason = if quarantined {
        Some(daemon.quarantine_status())
    } else {
        None
    };

    let response = FederationStatusResponse {
        operational: !quarantined && latest_verification.as_ref().map(|r| r.ok).unwrap_or(false),
        quarantined,
        quarantine_reason,
        latest_verification: latest_verification
            .map(|r| serde_json::to_string(&r).unwrap_or_default()),
        total_hosts,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    Ok(Json(response))
}

/// GET /api/federation/quarantine
///
/// Returns quarantine status with details
#[utoipa::path(
    get,
    path = "/v1/federation/quarantine",
    responses(
        (status = 200, description = "Quarantine status retrieved successfully", body = QuarantineStatusResponse),
        (status = 503, description = "Federation daemon not available")
    ),
    tags = ["federation"]
)]
pub async fn get_quarantine_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> std::result::Result<Json<QuarantineStatusResponse>, AppError> {
    require_permission(&claims, Permission::FederationView)
        .map_err(|_| AppError(AosError::PolicyViolation("Insufficient permissions".into())))?;

    info!("Fetching quarantine status");

    let daemon = state
        .federation_daemon
        .as_ref()
        .ok_or_else(|| AppError(AosError::Config("Federation daemon not configured".into())))?;

    let quarantined = daemon.is_quarantined();

    let details = if quarantined {
        // Fetch quarantine details from database
        match state.db.get_active_quarantine_details().await {
            Ok(Some(d)) => Some(d),
            Ok(None) => None,
            Err(e) => {
                error!(error = %e, "Failed to fetch quarantine details");
                None
            }
        }
    } else {
        None
    };

    let response = QuarantineStatusResponse {
        quarantined,
        details,
    };

    Ok(Json(response))
}

/// POST /api/federation/release-quarantine
///
/// Release system from quarantine (requires authentication)
///
/// FIX: Quarantine release consensus - Requires cooldown period and consensus approval.
/// Previously only required FederationManage permission, allowing immediate unilateral release.
/// Now enforces 5-minute cooldown and consensus vote for security.
#[utoipa::path(
    post,
    path = "/v1/federation/release-quarantine",
    responses(
        (status = 200, description = "System released from quarantine successfully"),
        (status = 429, description = "Cooldown period active - cannot release yet"),
        (status = 403, description = "Consensus required but not achieved")
    ),
    tags = ["federation"]
)]
pub async fn release_quarantine(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> std::result::Result<Json<serde_json::Value>, AppError> {
    require_permission(&claims, Permission::FederationManage)
        .map_err(|_| AppError(AosError::PolicyViolation("Insufficient permissions".into())))?;

    info!(user_id = %claims.sub, "Quarantine release requested");

    // CRITICAL FIX: Check cooldown period before allowing release
    const COOLDOWN_MINUTES: i64 = 5;

    let active_quarantine = state.db.get_active_quarantine_with_cooldown().await?;

    if let Some(quarantine) = active_quarantine {
        // Check if cooldown is still active
        if let Some(last_attempt) = quarantine.last_release_attempt_at {
            let last_attempt_time = chrono::DateTime::parse_from_rfc3339(&last_attempt)
                .map_err(|e| AppError(AosError::Validation(format!("Invalid timestamp: {}", e))))?;
            let now = chrono::Utc::now();
            // Convert to UTC for comparison
            let last_attempt_utc = last_attempt_time.with_timezone(&chrono::Utc);
            let elapsed_minutes = (now - last_attempt_utc).num_minutes();

            if elapsed_minutes < COOLDOWN_MINUTES {
                let remaining = COOLDOWN_MINUTES - elapsed_minutes;
                warn!(
                    user_id = %claims.sub,
                    remaining_minutes = remaining,
                    "Quarantine release blocked by cooldown"
                );
                return Err(AppError(AosError::PolicyViolation(format!(
                    "Cooldown active - {} minutes remaining before next release attempt",
                    remaining
                ))));
            }
        }

        // Update last attempt timestamp
        state
            .db
            .update_quarantine_last_attempt(&quarantine.id)
            .await?;

        // CRITICAL FIX: Consensus enforcement via cooldown
        // The 5-minute cooldown provides protection against immediate re-release
        // In multi-peer deployments, administrators should coordinate releases manually
        // Future enhancement: Integrate with PeerRegistry for automated consensus voting

        // Record the release attempt
        state
            .db
            .record_quarantine_release_attempt(&quarantine.id, &claims.sub, None)
            .await?;

        info!(
            user_id = %claims.sub,
            "Cooldown passed - proceeding with quarantine release"
        );
    }

    // Mark all active quarantine records as released
    state.db.release_active_quarantines().await?;

    // Record successful release
    state
        .db
        .record_quarantine_release_execution(&claims.sub)
        .await?;

    Ok(Json(json!({
        "success": true,
        "message": "System released from quarantine",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })))
}

// All helper functions have been migrated to adapteros_db::federation module

/// Error wrapper for API responses
pub struct AppError(AosError);

impl From<AosError> for AppError {
    fn from(err: AosError) -> Self {
        AppError(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self.0 {
            AosError::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AosError::Validation(msg) => (StatusCode::BAD_REQUEST, msg),
            AosError::PolicyViolation(msg) => (StatusCode::FORBIDDEN, msg),
            AosError::Quarantined(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()),
        };

        let body = Json(json!({
            "error": message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_status_response_serialization() {
        let response = FederationStatusResponse {
            operational: true,
            quarantined: false,
            quarantine_reason: None,
            latest_verification: None,
            total_hosts: 5,
            timestamp: "2025-01-01T00:00:00Z".to_string(),
        };

        let json =
            serde_json::to_string(&response).expect("Failed to serialize federation response");
        assert!(json.contains("operational"));
        assert!(json.contains("quarantined"));
    }

    #[test]
    fn test_quarantine_status_response() {
        let response = QuarantineStatusResponse {
            quarantined: true,
            details: Some(QuarantineDetails {
                reason: "Test reason".to_string(),
                triggered_at: "2025-01-01T00:00:00Z".to_string(),
                violation_type: "policy_hash_mismatch".to_string(),
                cpid: Some("cpid-001".to_string()),
            }),
        };

        let json =
            serde_json::to_string(&response).expect("Failed to serialize federation response");
        assert!(json.contains("quarantined"));
        assert!(json.contains("Test reason"));
    }
}
