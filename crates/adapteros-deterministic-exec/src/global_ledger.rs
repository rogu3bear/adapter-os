//! Global Tick Ledger - Cross-tenant and cross-host deterministic execution tracking
//!
//! Provides a persistent ledger of all deterministic executor events with Merkle chain
//! verification for cross-host consistency checks.

use crate::{ExecutorEvent, TaskId};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Entry in the global tick ledger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickLedgerEntry {
    /// Entry ID
    pub id: Option<String>,
    /// Tick number
    pub tick: u64,
    /// Tenant ID
    pub tenant_id: String,
    /// Host ID
    pub host_id: String,
    /// Task ID
    pub task_id: TaskId,
    /// Event type (serialized)
    pub event_type: String,
    /// BLAKE3 hash of event data
    pub event_hash: B3Hash,
    /// Timestamp (microseconds)
    pub timestamp_us: u64,
    /// Previous entry hash (for Merkle chain)
    pub prev_entry_hash: Option<B3Hash>,
}

/// Consistency report for cross-host verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyReport {
    /// Tenant ID
    pub tenant_id: String,
    /// First host ID
    pub host_a: String,
    /// Second host ID
    pub host_b: String,
    /// Tick range start
    pub tick_range_start: u64,
    /// Tick range end
    pub tick_range_end: u64,
    /// Whether execution is consistent
    pub consistent: bool,
    /// Number of divergence points
    pub divergence_count: usize,
    /// Divergence details
    pub divergences: Vec<DivergencePoint>,
}

/// Point of divergence between two hosts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergencePoint {
    /// Tick number where divergence occurred
    pub tick: u64,
    /// Event hash from host A
    pub hash_a: B3Hash,
    /// Event hash from host B
    pub hash_b: B3Hash,
}

/// Global tick ledger manager
pub struct GlobalTickLedger {
    /// Local tick counter
    local_tick: Arc<AtomicU64>,

    /// Database handle
    db: Arc<Db>,

    /// Tenant ID
    tenant_id: String,

    /// Host ID
    host_id: String,

    /// In-memory cache of recent entries
    entries: Arc<RwLock<VecDeque<TickLedgerEntry>>>,

    /// Maximum cache size
    max_cache_size: usize,

    /// Last entry hash (for Merkle chain)
    last_entry_hash: Arc<RwLock<Option<B3Hash>>>,
}

impl std::fmt::Debug for GlobalTickLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalTickLedger")
            .field("tenant_id", &self.tenant_id)
            .field("host_id", &self.host_id)
            .field("max_cache_size", &self.max_cache_size)
            .field("current_tick", &self.local_tick.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl GlobalTickLedger {
    /// Create a new global tick ledger
    pub fn new(db: Arc<Db>, tenant_id: String, host_id: String) -> Self {
        Self {
            local_tick: Arc::new(AtomicU64::new(0)),
            db,
            tenant_id,
            host_id,
            entries: Arc::new(RwLock::new(VecDeque::new())),
            max_cache_size: 1000,
            last_entry_hash: Arc::new(RwLock::new(None)),
        }
    }

    /// Get current tick
    pub fn current_tick(&self) -> u64 {
        self.local_tick.load(Ordering::SeqCst)
    }

    /// Increment tick counter
    pub fn increment_tick(&self) -> u64 {
        self.local_tick.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Record a tick event
    pub async fn record_tick(&self, task_id: TaskId, event: &ExecutorEvent) -> Result<B3Hash> {
        let tick = self.current_tick();
        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        // Serialize event type
        let event_type = match event {
            ExecutorEvent::TaskSpawned { .. } => "TaskSpawned",
            ExecutorEvent::TaskCompleted { .. } => "TaskCompleted",
            ExecutorEvent::TaskFailed { .. } => "TaskFailed",
            ExecutorEvent::TaskTimeout { .. } => "TaskTimeout",
            ExecutorEvent::TickAdvanced { .. } => "TickAdvanced",
        };

        // Compute event hash
        let event_hash = self.compute_event_hash(tick, task_id, event);

        // Get previous entry hash
        let prev_entry_hash = {
            let lock = self.last_entry_hash.read().unwrap();
            *lock
        };

        // Compute entry hash (combines prev_hash, event_hash)
        let entry_hash = self.compute_entry_hash(&event_hash, prev_entry_hash.as_ref());

        // Create entry
        let entry = TickLedgerEntry {
            id: None,
            tick,
            tenant_id: self.tenant_id.clone(),
            host_id: self.host_id.clone(),
            task_id,
            event_type: event_type.to_string(),
            event_hash,
            timestamp_us,
            prev_entry_hash,
        };

        // Store in database
        self.store_entry(&entry).await?;

        // Update cache
        {
            let mut entries = self.entries.write().unwrap();
            entries.push_back(entry.clone());

            // Trim cache if needed
            while entries.len() > self.max_cache_size {
                entries.pop_front();
            }
        }

        // Update last entry hash
        {
            let mut lock = self.last_entry_hash.write().unwrap();
            *lock = Some(entry_hash);
        }

        // Log to tracing
        tracing::debug!(
            tick,
            tenant_id = %self.tenant_id,
            host_id = %self.host_id,
            task_id = %task_id,
            event_type = %event_type,
            event_hash = %event_hash.to_hex(),
            "Tick ledger entry recorded"
        );

        debug!(
            tick = tick,
            tenant_id = %self.tenant_id,
            host_id = %self.host_id,
            task_id = %task_id,
            "Tick ledger entry recorded"
        );

        Ok(entry_hash)
    }

    /// Get ledger entries for a tick range
    pub async fn get_entries(
        &self,
        tick_start: u64,
        tick_end: u64,
    ) -> Result<Vec<TickLedgerEntry>> {
        let pool = self.db.pool();

        let rows = sqlx::query(
            r#"
            SELECT id, tick, tenant_id, host_id, task_id, event_type, event_hash, 
                   timestamp_us, prev_entry_hash
            FROM tick_ledger_entries
            WHERE tenant_id = ? AND host_id = ? AND tick >= ? AND tick <= ?
            ORDER BY tick ASC
            "#,
        )
        .bind(&self.tenant_id)
        .bind(&self.host_id)
        .bind(tick_start as i64)
        .bind(tick_end as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch ledger entries: {}", e)))?;

        let mut entries = Vec::new();
        for row in rows {
            let event_hash_hex: String = row.try_get("event_hash")?;
            let event_hash = B3Hash::from_hex(&event_hash_hex)?;

            let prev_hash_hex: Option<String> = row.try_get("prev_entry_hash").ok();
            let prev_entry_hash = prev_hash_hex.and_then(|hex| B3Hash::from_hex(&hex).ok());

            let task_id_hex: String = row.try_get("task_id")?;
            let task_id_bytes = hex::decode(&task_id_hex)
                .map_err(|e| AosError::Database(format!("Invalid task ID: {}", e)))?;
            let mut task_id_array = [0u8; 32];
            if task_id_bytes.len() == 32 {
                task_id_array.copy_from_slice(&task_id_bytes);
            }
            let task_id = TaskId::from_bytes(task_id_array);

            entries.push(TickLedgerEntry {
                id: Some(row.try_get("id")?),
                tick: row.try_get::<i64, _>("tick")? as u64,
                tenant_id: row.try_get("tenant_id")?,
                host_id: row.try_get("host_id")?,
                task_id,
                event_type: row.try_get("event_type")?,
                event_hash,
                timestamp_us: row.try_get::<i64, _>("timestamp_us")? as u64,
                prev_entry_hash,
            });
        }

        Ok(entries)
    }

    /// Verify consistency with another host
    pub async fn verify_cross_host(
        &self,
        peer_host_id: &str,
        tick_range: (u64, u64),
    ) -> Result<ConsistencyReport> {
        info!(
            tenant_id = %self.tenant_id,
            host_a = %self.host_id,
            host_b = %peer_host_id,
            tick_range = ?tick_range,
            "Verifying cross-host consistency"
        );

        // Fetch our entries
        let our_entries = self.get_entries(tick_range.0, tick_range.1).await?;

        // Fetch peer entries
        let peer_entries = self.get_peer_entries(peer_host_id, tick_range).await?;

        // Compare entries
        let divergences = self.compute_divergences(&our_entries, &peer_entries);

        let consistent = divergences.is_empty();
        let divergence_count = divergences.len();

        let report = ConsistencyReport {
            tenant_id: self.tenant_id.clone(),
            host_a: self.host_id.clone(),
            host_b: peer_host_id.to_string(),
            tick_range_start: tick_range.0,
            tick_range_end: tick_range.1,
            consistent,
            divergence_count,
            divergences: divergences.clone(),
        };

        // Store report
        self.store_consistency_report(&report).await?;

        // Log to tracing
        if consistent {
            tracing::info!(
                tenant_id = %self.tenant_id,
                host_a = %self.host_id,
                host_b = %peer_host_id,
                tick_range_start = tick_range.0,
                tick_range_end = tick_range.1,
                divergence_count,
                "Cross-host consistency check: PASS (divergences: {})",
                divergence_count
            );
        } else {
            tracing::warn!(
                tenant_id = %self.tenant_id,
                host_a = %self.host_id,
                host_b = %peer_host_id,
                tick_range_start = tick_range.0,
                tick_range_end = tick_range.1,
                divergence_count,
                "Cross-host consistency check: FAIL (divergences: {})",
                divergence_count
            );
        }

        if consistent {
            info!(
                "Cross-host consistency verified: {} entries match",
                our_entries.len()
            );
        } else {
            warn!(
                "Cross-host inconsistency detected: {} divergences found",
                divergence_count
            );
        }

        Ok(report)
    }

    /// Compute event hash
    fn compute_event_hash(&self, tick: u64, task_id: TaskId, event: &ExecutorEvent) -> B3Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&tick.to_le_bytes());
        hasher.update(task_id.as_bytes());

        // Hash event-specific data
        let event_json = serde_json::to_string(event).unwrap_or_default();
        hasher.update(event_json.as_bytes());

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Compute entry hash (for Merkle chain)
    fn compute_entry_hash(&self, event_hash: &B3Hash, prev_hash: Option<&B3Hash>) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        if let Some(prev) = prev_hash {
            hasher.update(prev.as_bytes());
        }

        hasher.update(event_hash.as_bytes());

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Store entry in database
    async fn store_entry(&self, entry: &TickLedgerEntry) -> Result<()> {
        let pool = self.db.pool();

        let task_id_hex = hex::encode(entry.task_id.as_bytes());
        let event_hash_hex = entry.event_hash.to_hex();
        let prev_hash_hex = entry.prev_entry_hash.as_ref().map(|h| h.to_hex());

        sqlx::query(
            r#"
            INSERT INTO tick_ledger_entries 
                (tick, tenant_id, host_id, task_id, event_type, event_hash, timestamp_us, prev_entry_hash)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(entry.tick as i64)
        .bind(&entry.tenant_id)
        .bind(&entry.host_id)
        .bind(&task_id_hex)
        .bind(&entry.event_type)
        .bind(&event_hash_hex)
        .bind(entry.timestamp_us as i64)
        .bind(&prev_hash_hex)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store ledger entry: {}", e)))?;

        Ok(())
    }

    /// Get peer entries
    async fn get_peer_entries(
        &self,
        peer_host_id: &str,
        tick_range: (u64, u64),
    ) -> Result<Vec<TickLedgerEntry>> {
        let pool = self.db.pool();

        let rows = sqlx::query(
            r#"
            SELECT id, tick, tenant_id, host_id, task_id, event_type, event_hash, 
                   timestamp_us, prev_entry_hash
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

        let mut entries = Vec::new();
        for row in rows {
            let event_hash_hex: String = row.try_get("event_hash")?;
            let event_hash = B3Hash::from_hex(&event_hash_hex)?;

            let prev_hash_hex: Option<String> = row.try_get("prev_entry_hash").ok();
            let prev_entry_hash = prev_hash_hex.and_then(|hex| B3Hash::from_hex(&hex).ok());

            let task_id_hex: String = row.try_get("task_id")?;
            let task_id_bytes = hex::decode(&task_id_hex)
                .map_err(|e| AosError::Database(format!("Invalid task ID: {}", e)))?;
            let mut task_id_array = [0u8; 32];
            if task_id_bytes.len() == 32 {
                task_id_array.copy_from_slice(&task_id_bytes);
            }
            let task_id = TaskId::from_bytes(task_id_array);

            entries.push(TickLedgerEntry {
                id: Some(row.try_get("id")?),
                tick: row.try_get::<i64, _>("tick")? as u64,
                tenant_id: row.try_get("tenant_id")?,
                host_id: row.try_get("host_id")?,
                task_id,
                event_type: row.try_get("event_type")?,
                event_hash,
                timestamp_us: row.try_get::<i64, _>("timestamp_us")? as u64,
                prev_entry_hash,
            });
        }

        Ok(entries)
    }

    /// Compute divergences between two entry lists
    fn compute_divergences(
        &self,
        our_entries: &[TickLedgerEntry],
        peer_entries: &[TickLedgerEntry],
    ) -> Vec<DivergencePoint> {
        let mut divergences = Vec::new();

        // Create maps for quick lookup
        let our_map: std::collections::HashMap<u64, &TickLedgerEntry> =
            our_entries.iter().map(|e| (e.tick, e)).collect();
        let peer_map: std::collections::HashMap<u64, &TickLedgerEntry> =
            peer_entries.iter().map(|e| (e.tick, e)).collect();

        // Find all unique ticks
        let mut all_ticks: Vec<u64> = our_map.keys().chain(peer_map.keys()).copied().collect();
        all_ticks.sort_unstable();
        all_ticks.dedup();

        // Compare entries at each tick
        for tick in all_ticks {
            match (our_map.get(&tick), peer_map.get(&tick)) {
                (Some(our_entry), Some(peer_entry)) => {
                    // Both have entries, compare hashes
                    if our_entry.event_hash != peer_entry.event_hash {
                        divergences.push(DivergencePoint {
                            tick,
                            hash_a: our_entry.event_hash,
                            hash_b: peer_entry.event_hash,
                        });
                    }
                }
                (Some(_), None) | (None, Some(_)) => {
                    // One has entry, other doesn't - this is a divergence
                    // Use zero hash for missing entry
                    let zero_hash = B3Hash::new([0u8; 32]);
                    divergences.push(DivergencePoint {
                        tick,
                        hash_a: our_map
                            .get(&tick)
                            .map(|e| e.event_hash)
                            .unwrap_or(zero_hash),
                        hash_b: peer_map
                            .get(&tick)
                            .map(|e| e.event_hash)
                            .unwrap_or(zero_hash),
                    });
                }
                (None, None) => {
                    // Neither has entry, no divergence
                }
            }
        }

        divergences
    }

    /// Store consistency report
    async fn store_consistency_report(&self, report: &ConsistencyReport) -> Result<()> {
        let pool = self.db.pool();

        let divergence_json =
            serde_json::to_string(&report.divergences).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"
            INSERT INTO tick_ledger_consistency_reports 
                (tenant_id, host_a, host_b, tick_range_start, tick_range_end, 
                 consistent, divergence_count, divergence_details)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&report.tenant_id)
        .bind(&report.host_a)
        .bind(&report.host_b)
        .bind(report.tick_range_start as i64)
        .bind(report.tick_range_end as i64)
        .bind(if report.consistent { 1 } else { 0 })
        .bind(report.divergence_count as i64)
        .bind(&divergence_json)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store consistency report: {}", e)))?;

        Ok(())
    }

    /// Get the latest entry hash for Merkle chain linkage
    pub fn get_latest_entry_hash(&self) -> Option<B3Hash> {
        let lock = self.last_entry_hash.read().unwrap();
        *lock
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup_test_db() -> (Db, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Db::connect(db_path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();
        (db, temp_dir)
    }

    #[tokio::test]
    async fn test_record_and_get_entries() {
        let (db, _temp) = setup_test_db().await;
        let ledger = GlobalTickLedger::new(
            Arc::new(db),
            "test-tenant".to_string(),
            "test-host".to_string(),
        );

        let task_id = TaskId::from_bytes([1u8; 32]);
        let event = ExecutorEvent::TaskSpawned {
            task_id,
            description: "test task".to_string(),
            tick: 0,
            agent_id: None,
            hash: [0u8; 32],
        };

        ledger.record_tick(task_id, &event).await.unwrap();
        ledger.increment_tick();

        let entries = ledger.get_entries(0, 10).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tick, 0);
    }

    #[tokio::test]
    async fn test_cross_host_consistency() {
        let (db, _temp) = setup_test_db().await;
        let db = Arc::new(db);

        let ledger_a =
            GlobalTickLedger::new(db.clone(), "test-tenant".to_string(), "host-a".to_string());

        let ledger_b =
            GlobalTickLedger::new(db.clone(), "test-tenant".to_string(), "host-b".to_string());

        // Record same events on both hosts
        let task_id = TaskId::from_bytes([1u8; 32]);
        let event = ExecutorEvent::TaskSpawned {
            task_id,
            description: "test task".to_string(),
            tick: 0,
            agent_id: None,
            hash: [0u8; 32],
        };

        ledger_a.record_tick(task_id, &event).await.unwrap();
        ledger_b.record_tick(task_id, &event).await.unwrap();

        // Verify consistency
        let report = ledger_a.verify_cross_host("host-b", (0, 10)).await.unwrap();
        assert!(report.consistent);
        assert_eq!(report.divergence_count, 0);
    }
}
