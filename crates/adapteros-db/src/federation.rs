//! Federation database operations
//!
//! All federation-related database queries including:
//! - Host counts and verification status
//! - Quarantine management
//! - Peer health and consensus
//!
//! Pattern: All database access goes through Db trait methods.

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[cfg(feature = "utoipa")]
use utoipa::ToSchema;

/// Quarantine details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(ToSchema))]
pub struct QuarantineDetails {
    pub reason: String,
    pub triggered_at: String,
    pub violation_type: String,
    pub cpid: Option<String>,
}

/// Quarantine record with cooldown info
#[derive(Debug, Clone)]
pub struct QuarantineRecord {
    pub id: String,
    pub last_release_attempt_at: Option<String>,
}

impl Db {
    /// Get total federation host count
    ///
    /// Returns the number of distinct hosts registered in the federation.
    pub async fn get_federation_host_count(&self) -> Result<usize> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(DISTINCT host_id)
            FROM federation_bundle_signatures
            "#,
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count federation hosts: {}", e)))?;

        Ok(count as usize)
    }

    /// Get active quarantine details
    ///
    /// Fetches the most recent unreleased quarantine record.
    pub async fn get_active_quarantine_details(&self) -> Result<Option<QuarantineDetails>> {
        let row = sqlx::query(
            r#"
            SELECT reason, created_at, violation_type, cpid
            FROM policy_quarantine
            WHERE released = FALSE
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(self.pool())
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

    /// Get active quarantine with cooldown data
    ///
    /// Returns quarantine record including last release attempt timestamp
    /// for cooldown enforcement.
    pub async fn get_active_quarantine_with_cooldown(&self) -> Result<Option<QuarantineRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, last_release_attempt_at
            FROM policy_quarantine
            WHERE released = FALSE
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch quarantine: {}", e)))?;

        if let Some(row) = row {
            Ok(Some(QuarantineRecord {
                id: row.get("id"),
                last_release_attempt_at: row.get("last_release_attempt_at"),
            }))
        } else {
            Ok(None)
        }
    }

    /// Update quarantine last attempt timestamp
    ///
    /// Updates the last_release_attempt_at field for cooldown enforcement.
    pub async fn update_quarantine_last_attempt(&self, quarantine_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE policy_quarantine
            SET last_release_attempt_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(quarantine_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update quarantine: {}", e)))?;

        Ok(())
    }

    /// Record quarantine release attempt
    ///
    /// Logs an attempt to release quarantine for audit trail.
    pub async fn record_quarantine_release_attempt(
        &self,
        quarantine_id: &str,
        requested_by: &str,
        consensus_decision_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO quarantine_release_attempts (quarantine_id, requested_by, consensus_decision_id)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(quarantine_id)
        .bind(requested_by)
        .bind(consensus_decision_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to record release attempt: {}", e)))?;

        Ok(())
    }

    /// Record successful quarantine release execution
    ///
    /// Marks release attempts as executed after successful quarantine release.
    pub async fn record_quarantine_release_execution(&self, executed_by: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE quarantine_release_attempts
            SET executed = TRUE, executed_at = CURRENT_TIMESTAMP
            WHERE quarantine_id IN (
                SELECT id FROM policy_quarantine WHERE released = TRUE
            ) AND requested_by = ?
            AND executed = FALSE
            "#,
        )
        .bind(executed_by)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to record release execution: {}", e)))?;

        Ok(())
    }

    /// Release all active quarantines
    ///
    /// Marks all unreleased quarantine records as released.
    pub async fn release_active_quarantines(&self) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE policy_quarantine
            SET released = TRUE, released_at = CURRENT_TIMESTAMP
            WHERE released = FALSE
            "#,
        )
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to release quarantines: {}", e)))?;

        Ok(())
    }

    /// Get federation peer sync status
    ///
    /// Returns sync status for all active federation peers, including:
    /// - Health status (healthy, degraded, unhealthy, isolated)
    /// - Last seen timestamp
    /// - Last heartbeat timestamp
    ///
    /// A peer is considered "in sync" if it is healthy and has a recent heartbeat.
    pub async fn get_peer_sync_status(&self, limit: usize) -> Result<Vec<PeerSyncStatus>> {
        let rows = sqlx::query(
            r#"
            SELECT
                host_id,
                hostname,
                health_status,
                last_seen_at,
                last_heartbeat_at,
                active,
                failed_heartbeats
            FROM federation_peers
            WHERE active = 1
            ORDER BY last_heartbeat_at DESC NULLS LAST
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch peer sync status: {}", e)))?;

        let mut peers = Vec::with_capacity(rows.len());
        for row in rows {
            let host_id: String = row.get("host_id");
            let hostname: Option<String> = row.get("hostname");
            let health_status: String = row.get("health_status");
            let last_seen_at: Option<String> = row.get("last_seen_at");
            let last_heartbeat_at: Option<String> = row.get("last_heartbeat_at");
            let failed_heartbeats: i32 = row.try_get("failed_heartbeats").unwrap_or(0);

            // Map health status to sync state
            let sync_state = match health_status.as_str() {
                "healthy" => PeerSyncState::Synced,
                "degraded" => PeerSyncState::Syncing,
                "unhealthy" | "isolated" => PeerSyncState::Failed,
                _ => PeerSyncState::Syncing,
            };

            // A peer is in sync if healthy with no failed heartbeats
            let in_sync = sync_state == PeerSyncState::Synced && failed_heartbeats == 0;

            peers.push(PeerSyncStatus {
                peer_id: host_id.clone(),
                host: hostname.unwrap_or(host_id),
                sync_state,
                in_sync,
                last_sync_at: last_heartbeat_at.clone(),
                last_seen_at,
                failed_heartbeats: failed_heartbeats as u32,
            });
        }

        Ok(peers)
    }

    /// Get count of active federation peers
    ///
    /// Returns total count of active peers in the federation.
    pub async fn get_active_peer_count(&self) -> Result<usize> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM federation_peers
            WHERE active = 1
            "#,
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count active peers: {}", e)))?;

        Ok(count as usize)
    }
}

/// Peer sync state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "utoipa", derive(ToSchema))]
pub enum PeerSyncState {
    /// Peer is fully synchronized
    Synced,
    /// Peer is currently syncing (degraded health)
    Syncing,
    /// Peer sync failed (unhealthy or isolated)
    Failed,
}

/// Peer sync status record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(ToSchema))]
pub struct PeerSyncStatus {
    /// Unique peer identifier
    pub peer_id: String,
    /// Hostname or display name
    pub host: String,
    /// Current sync state
    pub sync_state: PeerSyncState,
    /// Whether peer is considered in sync
    pub in_sync: bool,
    /// Timestamp of last successful sync (heartbeat)
    pub last_sync_at: Option<String>,
    /// Timestamp when peer was last seen
    pub last_seen_at: Option<String>,
    /// Number of consecutive failed heartbeats
    pub failed_heartbeats: u32,
}
