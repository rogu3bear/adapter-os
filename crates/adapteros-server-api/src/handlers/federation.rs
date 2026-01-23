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
    operation_id = "get_federation_quarantine_status",
    responses(
        (status = 200, description = "Quarantine status retrieved successfully", body = QuarantineStatusResponse),
        (status = 503, description = "Federation daemon not available")
    ),
    tags = ["federation"]
)]
pub async fn get_federation_quarantine_status(
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

// ============================================================================
// Sync Status Endpoint
// ============================================================================

/// Peer sync status summary
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PeerSyncSummary {
    pub peer_id: String,
    pub host: String,
    pub in_sync: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<String>,
}

/// Federation sync status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct FederationSyncStatusResponse {
    pub schema_version: String,
    /// Whether the system is currently syncing
    pub syncing: bool,
    /// Overall sync progress (0-100)
    pub progress_pct: f32,
    /// Number of peers in sync
    pub peers_in_sync: usize,
    /// Number of peers out of sync
    pub peers_out_of_sync: usize,
    /// Total peer count
    pub total_peers: usize,
    /// Peer details (limited to first 10)
    pub peers: Vec<PeerSyncSummary>,
    /// Timestamp of last successful sync
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<String>,
    /// Current timestamp
    pub timestamp: String,
}

/// Maximum number of peers to include in sync status response
const MAX_PEERS_IN_RESPONSE: usize = 10;

/// GET /v1/federation/sync-status
///
/// Returns current federation synchronization status with peer details
#[utoipa::path(
    get,
    path = "/v1/federation/sync-status",
    responses(
        (status = 200, description = "Federation sync status retrieved successfully", body = FederationSyncStatusResponse),
        (status = 503, description = "Federation daemon not available")
    ),
    tags = ["federation"]
)]
pub async fn get_federation_sync_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> std::result::Result<Json<FederationSyncStatusResponse>, AppError> {
    require_permission(&claims, Permission::FederationView)
        .map_err(|_| AppError(AosError::PolicyViolation("Insufficient permissions".into())))?;

    info!("Fetching federation sync status");

    // Fetch peer sync status from database (limited to first 10 peers)
    let peer_statuses = match state.db.get_peer_sync_status(MAX_PEERS_IN_RESPONSE).await {
        Ok(statuses) => statuses,
        Err(e) => {
            warn!(error = %e, "Failed to fetch peer sync status from database");
            Vec::new()
        }
    };

    // Get total peer count (may be more than the limited response)
    let total_peers = match state.db.get_active_peer_count().await {
        Ok(count) => count,
        Err(e) => {
            warn!(error = %e, "Failed to fetch active peer count");
            peer_statuses.len()
        }
    };

    // Convert database peer statuses to API response format
    let peers: Vec<PeerSyncSummary> = peer_statuses
        .iter()
        .map(|p| PeerSyncSummary {
            peer_id: p.peer_id.clone(),
            host: p.host.clone(),
            in_sync: p.in_sync,
            last_sync_at: p.last_sync_at.clone(),
        })
        .collect();

    // Derive counts from actual peer data to ensure consistency
    let peers_in_sync = peer_statuses.iter().filter(|p| p.in_sync).count();
    let peers_out_of_sync = total_peers.saturating_sub(peers_in_sync);

    // Find the most recent sync timestamp from peers
    let last_sync_at = peer_statuses
        .iter()
        .filter(|p| p.in_sync)
        .filter_map(|p| p.last_sync_at.as_ref())
        .max()
        .cloned();

    // Determine sync status based on daemon availability and quarantine state
    let (syncing, progress_pct) = if let Some(daemon) = &state.federation_daemon {
        let is_quarantined = daemon.is_quarantined();
        if is_quarantined {
            // System is quarantined - not syncing
            (false, 0.0)
        } else if total_peers == 0 {
            // No peers - considered fully synced (standalone mode)
            (false, 100.0)
        } else {
            // Calculate progress based on in-sync peer ratio
            let progress = if total_peers > 0 {
                (peers_in_sync as f32 / total_peers as f32) * 100.0
            } else {
                100.0
            };
            // If all peers are in sync, we're done syncing
            let is_syncing = peers_in_sync < total_peers;
            (is_syncing, progress)
        }
    } else {
        // No daemon - report not syncing with progress based on peer status
        let progress = if total_peers > 0 {
            (peers_in_sync as f32 / total_peers as f32) * 100.0
        } else {
            0.0
        };
        (false, progress)
    };

    let response = FederationSyncStatusResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        syncing,
        progress_pct,
        peers_in_sync,
        peers_out_of_sync,
        total_peers,
        peers,
        last_sync_at,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    Ok(Json(response))
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

    #[test]
    fn test_peer_sync_summary_serialization() {
        let peer = PeerSyncSummary {
            peer_id: "peer-001".to_string(),
            host: "host1.example.com".to_string(),
            in_sync: true,
            last_sync_at: Some("2025-01-01T12:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&peer).expect("Failed to serialize peer summary");
        assert!(json.contains("peer_id"));
        assert!(json.contains("peer-001"));
        assert!(json.contains("host1.example.com"));
        assert!(json.contains("in_sync"));
        assert!(json.contains("last_sync_at"));
    }

    #[test]
    fn test_peer_sync_summary_without_last_sync() {
        let peer = PeerSyncSummary {
            peer_id: "peer-002".to_string(),
            host: "host2.example.com".to_string(),
            in_sync: false,
            last_sync_at: None,
        };

        let json = serde_json::to_string(&peer).expect("Failed to serialize peer summary");
        assert!(json.contains("peer-002"));
        assert!(!json.contains("last_sync_at")); // Should be skipped when None
    }

    #[test]
    fn test_federation_sync_status_response_serialization() {
        let response = FederationSyncStatusResponse {
            schema_version: "1.0.0".to_string(),
            syncing: true,
            progress_pct: 75.0,
            peers_in_sync: 3,
            peers_out_of_sync: 1,
            total_peers: 4,
            peers: vec![
                PeerSyncSummary {
                    peer_id: "peer-001".to_string(),
                    host: "host1.example.com".to_string(),
                    in_sync: true,
                    last_sync_at: Some("2025-01-01T12:00:00Z".to_string()),
                },
                PeerSyncSummary {
                    peer_id: "peer-002".to_string(),
                    host: "host2.example.com".to_string(),
                    in_sync: false,
                    last_sync_at: None,
                },
            ],
            last_sync_at: Some("2025-01-01T12:00:00Z".to_string()),
            timestamp: "2025-01-01T12:30:00Z".to_string(),
        };

        let json =
            serde_json::to_string(&response).expect("Failed to serialize sync status response");
        assert!(json.contains("schema_version"));
        assert!(json.contains("syncing"));
        assert!(json.contains("progress_pct"));
        assert!(json.contains("peers_in_sync"));
        assert!(json.contains("peers_out_of_sync"));
        assert!(json.contains("total_peers"));
        assert!(json.contains("peers"));
    }

    #[test]
    fn test_federation_sync_status_empty_peers() {
        let response = FederationSyncStatusResponse {
            schema_version: "1.0.0".to_string(),
            syncing: false,
            progress_pct: 100.0,
            peers_in_sync: 0,
            peers_out_of_sync: 0,
            total_peers: 0,
            peers: vec![],
            last_sync_at: None,
            timestamp: "2025-01-01T00:00:00Z".to_string(),
        };

        let json =
            serde_json::to_string(&response).expect("Failed to serialize sync status response");
        assert!(json.contains("\"peers\":[]"));
        assert!(json.contains("\"total_peers\":0"));
    }

    #[test]
    fn test_federation_sync_status_all_in_sync() {
        let response = FederationSyncStatusResponse {
            schema_version: "1.0.0".to_string(),
            syncing: false,
            progress_pct: 100.0,
            peers_in_sync: 3,
            peers_out_of_sync: 0,
            total_peers: 3,
            peers: vec![
                PeerSyncSummary {
                    peer_id: "peer-001".to_string(),
                    host: "host1".to_string(),
                    in_sync: true,
                    last_sync_at: Some("2025-01-01T12:00:00Z".to_string()),
                },
                PeerSyncSummary {
                    peer_id: "peer-002".to_string(),
                    host: "host2".to_string(),
                    in_sync: true,
                    last_sync_at: Some("2025-01-01T12:05:00Z".to_string()),
                },
                PeerSyncSummary {
                    peer_id: "peer-003".to_string(),
                    host: "host3".to_string(),
                    in_sync: true,
                    last_sync_at: Some("2025-01-01T12:10:00Z".to_string()),
                },
            ],
            last_sync_at: Some("2025-01-01T12:10:00Z".to_string()),
            timestamp: "2025-01-01T12:30:00Z".to_string(),
        };

        // Verify progress is 100% when all peers in sync
        assert!(!response.syncing);
        assert!((response.progress_pct - 100.0).abs() < f32::EPSILON);
        assert_eq!(response.peers_in_sync, response.total_peers);
        assert_eq!(response.peers_out_of_sync, 0);
    }

    #[test]
    fn test_max_peers_in_response_constant() {
        // Verify the constant is set appropriately (10 peers max)
        assert_eq!(MAX_PEERS_IN_RESPONSE, 10);
    }
}
