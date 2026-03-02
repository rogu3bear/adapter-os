//! Tick Ledger Manager for Federation
//!
//! Manages tick ledger entries in the context of federation, providing:
//! - Bundle-to-tick linkage
//! - Chain continuity verification
//! - Cross-host tick reconciliation
//!
//! ## Policy Compliance
//!
//! - Determinism Ruleset (#2): Reproducible tick chains
//! - Isolation Ruleset (#8): Per-tenant tick isolation
//! - Federation integration: Cross-host consistency

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{debug, info, warn};

/// Entry in the tick ledger with federation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickLedgerEntry {
    /// Entry ID
    pub id: String,
    /// Tick number
    pub tick: u64,
    /// Tenant ID
    pub tenant_id: String,
    /// Host ID
    pub host_id: String,
    /// Task ID (hex)
    pub task_id: String,
    /// Event type
    pub event_type: String,
    /// BLAKE3 hash of event data (hex)
    pub event_hash: String,
    /// Timestamp (microseconds)
    pub timestamp_us: u64,
    /// Previous entry hash (for Merkle chain)
    pub prev_entry_hash: Option<String>,
    /// Bundle hash (federation linkage)
    pub bundle_hash: Option<String>,
}

/// Tick Ledger Manager - coordinates tick ledger operations for federation
pub struct TickLedgerManager {
    db: Db,
    /// Tenant ID for isolation
    pub tenant_id: String,
    /// Host ID for this node
    pub host_id: String,
}

impl TickLedgerManager {
    /// Create a new tick ledger manager
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection
    /// * `tenant_id` - Tenant identifier for isolation
    /// * `host_id` - Host identifier for this node
    pub fn new(db: Db, tenant_id: String, host_id: String) -> Self {
        Self {
            db,
            tenant_id,
            host_id,
        }
    }

    /// Get the latest tick hash from the ledger
    ///
    /// Returns the event_hash of the most recent tick ledger entry
    /// for this tenant and host, if one exists.
    pub async fn get_latest_tick_hash(&self) -> Result<Option<String>> {
        let pool = self.db.pool_result()?;

        let row = sqlx::query(
            r#"
            SELECT event_hash
            FROM tick_ledger_entries
            WHERE tenant_id = ? AND host_id = ?
            ORDER BY tick DESC
            LIMIT 1
            "#,
        )
        .bind(&self.tenant_id)
        .bind(&self.host_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get latest tick hash: {}", e)))?;

        if let Some(row) = row {
            let hash: String = row
                .try_get("event_hash")
                .map_err(|e| AosError::Database(format!("Failed to get event_hash: {}", e)))?;
            Ok(Some(hash))
        } else {
            Ok(None)
        }
    }

    /// Link a bundle to a tick ledger entry
    ///
    /// Associates a federation bundle hash with the tick entry identified
    /// by the given event hash.
    ///
    /// # Arguments
    ///
    /// * `bundle_hash` - Bundle hash to link
    /// * `event_hash` - Event hash identifying the tick entry
    pub async fn link_bundle_to_tick(&self, bundle_hash: &str, event_hash: &B3Hash) -> Result<()> {
        let pool = self.db.pool_result()?;
        let event_hash_hex = event_hash.to_hex();

        let result = sqlx::query(
            r#"
            UPDATE tick_ledger_entries
            SET bundle_hash = ?
            WHERE event_hash = ? AND tenant_id = ? AND host_id = ?
            "#,
        )
        .bind(bundle_hash)
        .bind(&event_hash_hex)
        .bind(&self.tenant_id)
        .bind(&self.host_id)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to link bundle to tick: {}", e)))?;

        if result.rows_affected() == 0 {
            warn!(
                bundle_hash = %bundle_hash,
                event_hash = %event_hash_hex,
                "No tick entry found to link bundle to"
            );
        } else {
            debug!(
                bundle_hash = %bundle_hash,
                event_hash = %event_hash_hex,
                "Bundle linked to tick entry"
            );
        }

        Ok(())
    }

    /// Get all tick ledger entries associated with a bundle
    ///
    /// # Arguments
    ///
    /// * `bundle_hash` - Bundle hash to query
    ///
    /// # Returns
    ///
    /// Vector of tick ledger entries linked to the bundle
    pub async fn get_entries_for_bundle(&self, bundle_hash: &str) -> Result<Vec<TickLedgerEntry>> {
        let pool = self.db.pool_result()?;

        let rows = sqlx::query(
            r#"
            SELECT id, tick, tenant_id, host_id, task_id, event_type, event_hash,
                   timestamp_us, prev_entry_hash, bundle_hash
            FROM tick_ledger_entries
            WHERE bundle_hash = ? AND tenant_id = ?
            ORDER BY tick ASC
            "#,
        )
        .bind(bundle_hash)
        .bind(&self.tenant_id)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch entries for bundle: {}", e)))?;

        let entries = rows
            .into_iter()
            .map(|row| TickLedgerEntry {
                id: row.get::<String, _>("id"),
                tick: row.get::<i64, _>("tick") as u64,
                tenant_id: row.get::<String, _>("tenant_id"),
                host_id: row.get::<String, _>("host_id"),
                task_id: row.get::<String, _>("task_id"),
                event_type: row.get::<String, _>("event_type"),
                event_hash: row.get::<String, _>("event_hash"),
                timestamp_us: row.get::<i64, _>("timestamp_us") as u64,
                prev_entry_hash: row.get::<Option<String>, _>("prev_entry_hash"),
                bundle_hash: row.get::<Option<String>, _>("bundle_hash"),
            })
            .collect();

        Ok(entries)
    }

    /// Verify chain continuity for entries associated with a bundle
    ///
    /// Checks that the Merkle chain linkage (prev_entry_hash) is valid
    /// for all entries associated with the given bundle.
    ///
    /// # Arguments
    ///
    /// * `bundle_hash` - Bundle hash to verify
    ///
    /// # Returns
    ///
    /// `true` if chain is valid, `false` if broken
    pub async fn verify_chain_continuity(&self, bundle_hash: &str) -> Result<bool> {
        let entries = self.get_entries_for_bundle(bundle_hash).await?;

        if entries.is_empty() {
            debug!(
                bundle_hash = %bundle_hash,
                "No entries found for bundle, chain is trivially valid"
            );
            return Ok(true);
        }

        if entries.len() == 1 {
            debug!(
                bundle_hash = %bundle_hash,
                "Single entry in bundle, chain is trivially valid"
            );
            return Ok(true);
        }

        // Verify linkage: each entry's prev_entry_hash should match previous entry's event_hash
        for i in 1..entries.len() {
            let prev = &entries[i - 1];
            let curr = &entries[i];

            match &curr.prev_entry_hash {
                Some(prev_hash) if prev_hash == &prev.event_hash => {
                    // Valid link
                    debug!(tick = curr.tick, prev_tick = prev.tick, "Chain link valid");
                }
                Some(prev_hash) => {
                    // Chain break detected
                    warn!(
                        bundle_hash = %bundle_hash,
                        tick = curr.tick,
                        expected_prev = %prev.event_hash,
                        actual_prev = %prev_hash,
                        "Chain break detected"
                    );
                    return Ok(false);
                }
                None => {
                    // Missing prev_entry_hash on non-first entry
                    warn!(
                        bundle_hash = %bundle_hash,
                        tick = curr.tick,
                        "Missing prev_entry_hash on non-first entry"
                    );
                    return Ok(false);
                }
            }
        }

        info!(
            bundle_hash = %bundle_hash,
            entries = entries.len(),
            "Chain continuity verified"
        );

        Ok(true)
    }

    /// Detect tick drift between this host and a peer
    ///
    /// Compares the tick values between this host and a peer to detect
    /// any drift or synchronization issues.
    ///
    /// # Arguments
    ///
    /// * `peer_host_id` - Host ID of the peer to compare against
    /// * `tick_range` - Range of ticks to compare (start, end)
    ///
    /// # Returns
    ///
    /// Tuple of (max_drift, drift_details) where drift is the maximum
    /// tick difference observed
    pub async fn detect_tick_drift(
        &self,
        peer_host_id: &str,
        tick_range: (u64, u64),
    ) -> Result<(u64, Vec<TickDriftPoint>)> {
        let pool = self.db.pool_result()?;

        // Get our entries
        let our_entries = sqlx::query(
            r#"
            SELECT tick, event_hash, timestamp_us
            FROM tick_ledger_entries
            WHERE tenant_id = ? AND host_id = ? AND tick >= ? AND tick <= ?
            ORDER BY tick ASC
            "#,
        )
        .bind(&self.tenant_id)
        .bind(&self.host_id)
        .bind(tick_range.0 as i64)
        .bind(tick_range.1 as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch our entries: {}", e)))?;

        // Get peer entries
        let peer_entries = sqlx::query(
            r#"
            SELECT tick, event_hash, timestamp_us
            FROM tick_ledger_entries
            WHERE tenant_id = ? AND host_id = ? AND tick >= ? AND tick <= ?
            ORDER BY tick ASC
            "#,
        )
        .bind(&self.tenant_id)
        .bind(peer_host_id)
        .bind(tick_range.0 as i64)
        .bind(tick_range.1 as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch peer entries: {}", e)))?;

        // Build tick -> timestamp maps
        let mut our_ticks: std::collections::HashMap<u64, (String, u64)> =
            std::collections::HashMap::new();
        for row in our_entries {
            let tick = row.get::<i64, _>("tick") as u64;
            let event_hash: String = row.get("event_hash");
            let timestamp_us = row.get::<i64, _>("timestamp_us") as u64;
            our_ticks.insert(tick, (event_hash, timestamp_us));
        }

        let mut peer_ticks: std::collections::HashMap<u64, (String, u64)> =
            std::collections::HashMap::new();
        for row in peer_entries {
            let tick = row.get::<i64, _>("tick") as u64;
            let event_hash: String = row.get("event_hash");
            let timestamp_us = row.get::<i64, _>("timestamp_us") as u64;
            peer_ticks.insert(tick, (event_hash, timestamp_us));
        }

        // Find drift points
        let mut max_drift: u64 = 0;
        let mut drift_points = Vec::new();

        // Collect all unique ticks
        let mut all_ticks: Vec<u64> = our_ticks.keys().chain(peer_ticks.keys()).copied().collect();
        all_ticks.sort_unstable();
        all_ticks.dedup();

        for tick in all_ticks {
            let our = our_ticks.get(&tick);
            let peer = peer_ticks.get(&tick);

            match (our, peer) {
                (Some((our_hash, our_ts)), Some((peer_hash, peer_ts))) => {
                    // Both have the tick - check for timestamp drift
                    let time_diff = if *our_ts > *peer_ts {
                        our_ts - peer_ts
                    } else {
                        peer_ts - our_ts
                    };

                    // Also check hash mismatch
                    if our_hash != peer_hash {
                        drift_points.push(TickDriftPoint {
                            tick,
                            our_event_hash: Some(our_hash.clone()),
                            peer_event_hash: Some(peer_hash.clone()),
                            time_drift_us: time_diff,
                            drift_type: DriftType::HashMismatch,
                        });
                    } else if time_diff > 1_000_000 {
                        // More than 1 second drift
                        if time_diff > max_drift {
                            max_drift = time_diff;
                        }
                        drift_points.push(TickDriftPoint {
                            tick,
                            our_event_hash: Some(our_hash.clone()),
                            peer_event_hash: Some(peer_hash.clone()),
                            time_drift_us: time_diff,
                            drift_type: DriftType::TimeDrift,
                        });
                    }
                }
                (Some((our_hash, _)), None) => {
                    // We have it, peer doesn't
                    drift_points.push(TickDriftPoint {
                        tick,
                        our_event_hash: Some(our_hash.clone()),
                        peer_event_hash: None,
                        time_drift_us: 0,
                        drift_type: DriftType::MissingOnPeer,
                    });
                }
                (None, Some((peer_hash, _))) => {
                    // Peer has it, we don't
                    drift_points.push(TickDriftPoint {
                        tick,
                        our_event_hash: None,
                        peer_event_hash: Some(peer_hash.clone()),
                        time_drift_us: 0,
                        drift_type: DriftType::MissingLocally,
                    });
                }
                (None, None) => {
                    // Neither has it (shouldn't happen given how we built all_ticks)
                }
            }
        }

        Ok((max_drift, drift_points))
    }

    /// Sync tick ledger entries with a peer
    ///
    /// Identifies missing entries and prepares them for synchronization.
    ///
    /// # Arguments
    ///
    /// * `peer_host_id` - Host ID of the peer
    /// * `tick_range` - Range of ticks to sync
    ///
    /// # Returns
    ///
    /// Tuple of (entries_to_send, ticks_to_request) representing what we
    /// have that the peer doesn't, and what the peer has that we don't
    pub async fn prepare_sync(
        &self,
        peer_host_id: &str,
        tick_range: (u64, u64),
    ) -> Result<(Vec<TickLedgerEntry>, Vec<u64>)> {
        let (_, drift_points) = self.detect_tick_drift(peer_host_id, tick_range).await?;

        let mut ticks_to_request = Vec::new();
        let mut ticks_to_send = Vec::new();

        for point in drift_points {
            match point.drift_type {
                DriftType::MissingOnPeer => {
                    ticks_to_send.push(point.tick);
                }
                DriftType::MissingLocally => {
                    ticks_to_request.push(point.tick);
                }
                DriftType::HashMismatch => {
                    // Hash mismatch indicates potential determinism violation
                    warn!(
                        tick = point.tick,
                        our_hash = ?point.our_event_hash,
                        peer_hash = ?point.peer_event_hash,
                        "Hash mismatch detected - potential determinism violation"
                    );
                }
                DriftType::TimeDrift => {
                    // Time drift is informational, no sync action needed
                }
            }
        }

        // Fetch entries we need to send
        let entries_to_send = if ticks_to_send.is_empty() {
            Vec::new()
        } else {
            self.get_entries_by_ticks(&ticks_to_send).await?
        };

        Ok((entries_to_send, ticks_to_request))
    }

    /// Get entries by specific tick values
    async fn get_entries_by_ticks(&self, ticks: &[u64]) -> Result<Vec<TickLedgerEntry>> {
        if ticks.is_empty() {
            return Ok(Vec::new());
        }

        let pool = self.db.pool_result()?;

        // Build placeholders for IN clause
        let placeholders: Vec<String> = ticks.iter().map(|_| "?".to_string()).collect();
        let in_clause = placeholders.join(", ");

        let query = format!(
            r#"
            SELECT id, tick, tenant_id, host_id, task_id, event_type, event_hash,
                   timestamp_us, prev_entry_hash, bundle_hash
            FROM tick_ledger_entries
            WHERE tenant_id = ? AND host_id = ? AND tick IN ({})
            ORDER BY tick ASC
            "#,
            in_clause
        );

        let mut query_builder = sqlx::query(&query)
            .bind(&self.tenant_id)
            .bind(&self.host_id);

        for tick in ticks {
            query_builder = query_builder.bind(*tick as i64);
        }

        let rows = query_builder
            .fetch_all(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to fetch entries by ticks: {}", e)))?;

        let entries = rows
            .into_iter()
            .map(|row| TickLedgerEntry {
                id: row.get::<String, _>("id"),
                tick: row.get::<i64, _>("tick") as u64,
                tenant_id: row.get::<String, _>("tenant_id"),
                host_id: row.get::<String, _>("host_id"),
                task_id: row.get::<String, _>("task_id"),
                event_type: row.get::<String, _>("event_type"),
                event_hash: row.get::<String, _>("event_hash"),
                timestamp_us: row.get::<i64, _>("timestamp_us") as u64,
                prev_entry_hash: row.get::<Option<String>, _>("prev_entry_hash"),
                bundle_hash: row.get::<Option<String>, _>("bundle_hash"),
            })
            .collect();

        Ok(entries)
    }
}

/// Point of tick drift between two hosts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickDriftPoint {
    /// Tick where drift was detected
    pub tick: u64,
    /// Our event hash (if we have the entry)
    pub our_event_hash: Option<String>,
    /// Peer's event hash (if they have the entry)
    pub peer_event_hash: Option<String>,
    /// Time drift in microseconds
    pub time_drift_us: u64,
    /// Type of drift
    pub drift_type: DriftType,
}

/// Type of drift detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriftType {
    /// Entry missing on peer
    MissingOnPeer,
    /// Entry missing locally
    MissingLocally,
    /// Hash mismatch (determinism violation)
    HashMismatch,
    /// Timestamp drift (informational)
    TimeDrift,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Helper to setup a test database with proper cleanup
    // Returns (Db, TempDir) - caller must hold TempDir to keep DB files alive
    async fn setup_test_db() -> Result<(Db, TempDir)> {
        let temp_dir = TempDir::with_prefix("aos-tick-ledger-test-")
            .expect("failed to create temporary directory for tick ledger manager test database: system tmp directory should be writable");
        let db_path = temp_dir
            .path()
            .join(format!("test_{}.db", std::process::id()));
        let db = Db::connect(db_path.to_str().unwrap()).await?;
        db.migrate().await?;
        Ok((db, temp_dir))
    }

    #[tokio::test]
    async fn test_tick_ledger_manager_creation() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager = TickLedgerManager::new(db, "tenant-001".to_string(), "host-001".to_string());

        // Should not error on creation
        assert_eq!(manager.tenant_id, "tenant-001");
        assert_eq!(manager.host_id, "host-001");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_latest_tick_hash_empty() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager = TickLedgerManager::new(db, "tenant-001".to_string(), "host-001".to_string());

        // Should return None when no entries exist
        let result = manager.get_latest_tick_hash().await?;
        assert!(result.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_link_bundle_to_tick() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager =
            TickLedgerManager::new(db.clone(), "tenant-001".to_string(), "host-001".to_string());

        // Insert a tick ledger entry first
        let test_hash = B3Hash::hash(b"test_event");
        let pool = db.pool_result()?;

        sqlx::query(
            r#"
            INSERT INTO tick_ledger_entries
            (id, tick, tenant_id, host_id, task_id, event_type, event_hash, timestamp_us, prev_entry_hash)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("entry-001")
        .bind(1i64)
        .bind("tenant-001")
        .bind("host-001")
        .bind("task-001")
        .bind("TaskCompleted")
        .bind(test_hash.to_hex())
        .bind(1000000i64)
        .bind(None::<String>)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert test entry: {}", e)))?;

        // Link bundle to tick
        let bundle_hash = "bundle-001";
        manager.link_bundle_to_tick(bundle_hash, &test_hash).await?;

        // Verify the link was created
        let entries = manager.get_entries_for_bundle(bundle_hash).await?;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].bundle_hash.as_deref(), Some(bundle_hash));

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_chain_continuity_valid() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager =
            TickLedgerManager::new(db.clone(), "tenant-001".to_string(), "host-001".to_string());

        // Insert two linked tick ledger entries
        let pool = db.pool_result()?;
        let hash1 = B3Hash::hash(b"event_1");
        let hash2 = B3Hash::hash(b"event_2");
        let bundle_hash = "bundle-001";

        // Entry 1
        sqlx::query(
            r#"
            INSERT INTO tick_ledger_entries
            (id, tick, tenant_id, host_id, task_id, event_type, event_hash, timestamp_us, prev_entry_hash, bundle_hash)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("entry-001")
        .bind(1i64)
        .bind("tenant-001")
        .bind("host-001")
        .bind("task-001")
        .bind("TaskCompleted")
        .bind(hash1.to_hex())
        .bind(1000000i64)
        .bind(None::<String>)
        .bind(bundle_hash)
        .execute(pool)
        .await?;

        // Entry 2 (links to Entry 1)
        sqlx::query(
            r#"
            INSERT INTO tick_ledger_entries
            (id, tick, tenant_id, host_id, task_id, event_type, event_hash, timestamp_us, prev_entry_hash, bundle_hash)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("entry-002")
        .bind(2i64)
        .bind("tenant-001")
        .bind("host-001")
        .bind("task-002")
        .bind("TaskCompleted")
        .bind(hash2.to_hex())
        .bind(2000000i64)
        .bind(Some(hash1.to_hex()))
        .bind(bundle_hash)
        .execute(pool)
        .await?;

        // Should verify chain successfully
        let is_valid = manager.verify_chain_continuity(bundle_hash).await?;
        assert!(is_valid);

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_chain_continuity_broken() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager =
            TickLedgerManager::new(db.clone(), "tenant-001".to_string(), "host-001".to_string());

        // Insert two unlinked tick ledger entries
        let pool = db.pool_result()?;
        let hash1 = B3Hash::hash(b"event_1");
        let hash2 = B3Hash::hash(b"event_2");
        let wrong_hash = B3Hash::hash(b"wrong_prev");
        let bundle_hash = "bundle-001";

        // Entry 1
        sqlx::query(
            r#"
            INSERT INTO tick_ledger_entries
            (id, tick, tenant_id, host_id, task_id, event_type, event_hash, timestamp_us, prev_entry_hash, bundle_hash)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("entry-001")
        .bind(1i64)
        .bind("tenant-001")
        .bind("host-001")
        .bind("task-001")
        .bind("TaskCompleted")
        .bind(hash1.to_hex())
        .bind(1000000i64)
        .bind(None::<String>)
        .bind(bundle_hash)
        .execute(pool)
        .await?;

        // Entry 2 (broken link - points to wrong hash)
        sqlx::query(
            r#"
            INSERT INTO tick_ledger_entries
            (id, tick, tenant_id, host_id, task_id, event_type, event_hash, timestamp_us, prev_entry_hash, bundle_hash)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("entry-002")
        .bind(2i64)
        .bind("tenant-001")
        .bind("host-001")
        .bind("task-002")
        .bind("TaskCompleted")
        .bind(hash2.to_hex())
        .bind(2000000i64)
        .bind(Some(wrong_hash.to_hex()))
        .bind(bundle_hash)
        .execute(pool)
        .await?;

        // Should detect chain break
        let is_valid = manager.verify_chain_continuity(bundle_hash).await?;
        assert!(!is_valid);

        Ok(())
    }

    #[tokio::test]
    async fn test_detect_tick_drift() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager =
            TickLedgerManager::new(db.clone(), "tenant-001".to_string(), "host-001".to_string());

        let pool = db.pool_result()?;
        let hash1 = B3Hash::hash(b"event_1");

        // Insert entry for host-001
        sqlx::query(
            r#"
            INSERT INTO tick_ledger_entries
            (id, tick, tenant_id, host_id, task_id, event_type, event_hash, timestamp_us, prev_entry_hash)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("entry-001")
        .bind(1i64)
        .bind("tenant-001")
        .bind("host-001")
        .bind("task-001")
        .bind("TaskCompleted")
        .bind(hash1.to_hex())
        .bind(1000000i64)
        .bind(None::<String>)
        .execute(pool)
        .await?;

        // Detect drift against host-002 (which has no entries)
        let (max_drift, drift_points) = manager.detect_tick_drift("host-002", (0, 10)).await?;

        assert_eq!(max_drift, 0);
        assert_eq!(drift_points.len(), 1);
        assert_eq!(drift_points[0].drift_type, DriftType::MissingOnPeer);

        Ok(())
    }
}
