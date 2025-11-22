//! Federation Status API Handlers
//!
//! REST endpoints for federation verification status and management.

use crate::state::AppState;
use adapteros_core::AosError;
use adapteros_db::Db;
use adapteros_orchestrator::FederationVerificationReport;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use tracing::{error, info};
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

/// Quarantine details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QuarantineDetails {
    /// Reason for quarantine
    pub reason: String,
    /// When quarantine was triggered
    pub triggered_at: String,
    /// Violation type
    pub violation_type: String,
    /// Control plane ID
    pub cpid: Option<String>,
}

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
) -> std::result::Result<Json<FederationStatusResponse>, AppError> {
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
    let total_hosts = get_host_count(&state.db).await.unwrap_or(0);

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
) -> std::result::Result<Json<QuarantineStatusResponse>, AppError> {
    info!("Fetching quarantine status");

    let daemon = state
        .federation_daemon
        .as_ref()
        .ok_or_else(|| AppError(AosError::Config("Federation daemon not configured".into())))?;

    let quarantined = daemon.is_quarantined();

    let details = if quarantined {
        // Fetch quarantine details from database
        match get_active_quarantine_details(&state.db).await {
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
#[utoipa::path(
    post,
    path = "/v1/federation/release-quarantine",
    responses(
        (status = 200, description = "System released from quarantine successfully")
    ),
    tags = ["federation"]
)]
pub async fn release_quarantine(
    State(state): State<AppState>,
) -> std::result::Result<Json<serde_json::Value>, AppError> {
    info!("Releasing system from quarantine");

    // Mark all active quarantine records as released
    release_active_quarantines(&state.db).await?;

    Ok(Json(json!({
        "success": true,
        "message": "System released from quarantine",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })))
}

/// Helper: Get total host count
async fn get_host_count(db: &Db) -> adapteros_core::Result<usize> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(DISTINCT host_id)
        FROM federation_bundle_signatures
        "#,
    )
    .fetch_one(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to count hosts: {}", e)))?;

    Ok(count as usize)
}

/// Helper: Get active quarantine details
async fn get_active_quarantine_details(
    db: &Db,
) -> adapteros_core::Result<Option<QuarantineDetails>> {
    let row = sqlx::query(
        r#"
        SELECT reason, created_at, violation_type, cpid
        FROM policy_quarantine
        WHERE released = FALSE
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to fetch quarantine details: {}", e)))?;

    if let Some(row) = row {
        Ok(Some(QuarantineDetails {
            reason: row.get("reason"),
            triggered_at: row.get("created_at"),
            violation_type: row.get("violation_type"),
            cpid: row.get("cpid"),
        }))
    } else {
        Ok(None)
    }
}

/// Helper: Release active quarantines
async fn release_active_quarantines(db: &Db) -> adapteros_core::Result<()> {
    sqlx::query(
        r#"
        UPDATE policy_quarantine
        SET released = TRUE, released_at = CURRENT_TIMESTAMP
        WHERE released = FALSE
        "#,
    )
    .execute(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to release quarantine: {}", e)))?;

    Ok(())
}

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
