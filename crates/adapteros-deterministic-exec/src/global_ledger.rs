//! Global Tick Ledger - Cross-tenant and cross-host deterministic execution tracking
//!
//! Provides a persistent ledger of all deterministic executor events with Merkle chain
//! verification for cross-host consistency checks.
//!
//! ## Determinism Notes
//!
//! - **Timestamps**: The `timestamp_us` field uses wall-clock time and is NOT deterministic
//!   across runs. However, it is excluded from `event_hash` computation, so it does not
//!   affect cross-host consistency checks or replay correctness. For fully deterministic
//!   timestamps, use `with_deterministic_timestamps()` constructor.
//! - **Merkle Chain**: The `prev_entry_hash` linkage is deterministic when events are
//!   recorded in the same order with the same global seed.

use crate::{ExecutorEvent, TaskId};
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::Db;
use adapteros_telemetry::{LogLevel, TelemetryEventBuilder, TelemetryWriter};
use serde::{Deserialize, Serialize};
use serde_json::json;
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
    /// Number of entries at this tick on host A (for detecting duplicate ticks)
    #[serde(default = "default_one")]
    pub entries_a_count: usize,
    /// Number of entries at this tick on host B (for detecting duplicate ticks)
    #[serde(default = "default_one")]
    pub entries_b_count: usize,
}

fn default_one() -> usize {
    1
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

    /// Telemetry writer
    telemetry: Option<Arc<TelemetryWriter>>,

    /// Last entry hash (for Merkle chain)
    last_entry_hash: Arc<RwLock<Option<B3Hash>>>,

    /// Use deterministic (tick-based) timestamps instead of wall-clock time
    /// When true, timestamp_us will be set to (tick * 1000) for reproducibility
    use_deterministic_timestamps: bool,
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
    ///
    /// NOTE: Defaults to deterministic timestamps (tick * 1000) for reproducibility.
    /// This ensures consistent behavior across runs and hosts. Use wall-clock time
    /// only when explicitly needed for debugging/audit purposes via `with_wall_clock_timestamps()`.
    pub fn new(db: Arc<Db>, tenant_id: String, host_id: String) -> Self {
        Self {
            local_tick: Arc::new(AtomicU64::new(0)),
            db,
            tenant_id,
            host_id,
            entries: Arc::new(RwLock::new(VecDeque::new())),
            max_cache_size: 1000,
            telemetry: None,
            last_entry_hash: Arc::new(RwLock::new(None)),
            // Default to deterministic timestamps for reproducibility (Issue D-1 fix)
            use_deterministic_timestamps: true,
        }
    }

    /// Create with wall-clock timestamps (non-deterministic, for debugging/audit only)
    ///
    /// WARNING: Wall-clock timestamps are NOT deterministic and should only be used
    /// when real-time correlation is required for debugging or audit purposes.
    /// For normal operation, use `new()` which defaults to deterministic timestamps.
    pub fn with_wall_clock_timestamps(db: Arc<Db>, tenant_id: String, host_id: String) -> Self {
        Self {
            local_tick: Arc::new(AtomicU64::new(0)),
            db,
            tenant_id,
            host_id,
            entries: Arc::new(RwLock::new(VecDeque::new())),
            max_cache_size: 1000,
            telemetry: None,
            last_entry_hash: Arc::new(RwLock::new(None)),
            use_deterministic_timestamps: false,
        }
    }

    /// Create with deterministic timestamps enabled (for replay scenarios)
    ///
    /// When enabled, `timestamp_us` is set to `tick * 1000` instead of wall-clock time,
    /// ensuring identical timestamps across replay runs.
    pub fn with_deterministic_timestamps(db: Arc<Db>, tenant_id: String, host_id: String) -> Self {
        Self {
            local_tick: Arc::new(AtomicU64::new(0)),
            db,
            tenant_id,
            host_id,
            entries: Arc::new(RwLock::new(VecDeque::new())),
            max_cache_size: 1000,
            telemetry: None,
            last_entry_hash: Arc::new(RwLock::new(None)),
            use_deterministic_timestamps: true,
        }
    }

    /// Create with telemetry writer
    ///
    /// NOTE: Defaults to deterministic timestamps for reproducibility.
    pub fn with_telemetry(
        db: Arc<Db>,
        tenant_id: String,
        host_id: String,
        telemetry: Arc<TelemetryWriter>,
    ) -> Self {
        Self {
            local_tick: Arc::new(AtomicU64::new(0)),
            db,
            tenant_id,
            host_id,
            entries: Arc::new(RwLock::new(VecDeque::new())),
            max_cache_size: 1000,
            telemetry: Some(telemetry),
            last_entry_hash: Arc::new(RwLock::new(None)),
            // Default to deterministic timestamps for reproducibility (Issue D-1 fix)
            use_deterministic_timestamps: true,
        }
    }

    /// Get current tick
    pub fn current_tick(&self) -> u64 {
        self.local_tick.load(Ordering::SeqCst)
    }

    /// Increment tick counter and return the assigned tick.
    pub fn increment_tick(&self) -> u64 {
        self.local_tick.fetch_add(1, Ordering::SeqCst)
    }

    /// Record a tick event
    ///
    /// ## Issue C-6 Fix: Atomic Tick Assignment
    /// Uses fetch_add to atomically assign a unique tick value to each event.
    /// This eliminates the race condition where multiple threads could call
    /// record_tick concurrently and get the same tick value.
    pub async fn record_tick(&self, task_id: TaskId, event: &ExecutorEvent) -> Result<B3Hash> {
        // Atomically fetch-and-increment tick to ensure uniqueness
        let tick = self.local_tick.fetch_add(1, Ordering::SeqCst);
        self.record_tick_with_assigned_tick(tick, task_id, event)
            .await
    }

    /// Record a tick event at a pre-assigned tick.
    pub async fn record_tick_at(
        &self,
        tick: u64,
        task_id: TaskId,
        event: &ExecutorEvent,
    ) -> Result<B3Hash> {
        self.local_tick
            .fetch_max(tick.saturating_add(1), Ordering::SeqCst);
        self.record_tick_with_assigned_tick(tick, task_id, event)
            .await
    }

    async fn record_tick_with_assigned_tick(
        &self,
        tick: u64,
        task_id: TaskId,
        event: &ExecutorEvent,
    ) -> Result<B3Hash> {
        // Use deterministic timestamps if enabled (tick * 1000us), otherwise wall-clock
        // Note: Wall-clock timestamps are non-deterministic but excluded from event_hash
        let timestamp_us = if self.use_deterministic_timestamps {
            tick * 1000 // Deterministic: 1ms per tick
        } else {
            // P1-1 Fix: In strict-determinism mode, panic on wall-clock access
            #[cfg(feature = "strict-determinism")]
            {
                panic!(
                    "DETERMINISM VIOLATION: Wall-clock time access attempted in global_ledger. \
                     Use deterministic timestamps (tick * 1000) or set use_deterministic_timestamps=true. \
                     This panic is enabled by the 'strict-determinism' feature flag."
                );
            }

            // P1-1 Fix: Log warning when wall-clock fallback is used
            #[cfg(not(feature = "strict-determinism"))]
            {
                warn!(
                    tick = tick,
                    tenant_id = %self.tenant_id,
                    host_id = %self.host_id,
                    "DETERMINISM WARNING: Using wall-clock timestamp instead of logical tick. \
                     This may cause non-deterministic behavior across replay runs. \
                     Consider using GlobalTickLedger::with_deterministic_timestamps() or \
                     enabling 'strict-determinism' feature for validation."
                );
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64
            }
        };

        // Serialize event type
        let event_type = match event {
            ExecutorEvent::TaskSpawned { .. } => "TaskSpawned",
            ExecutorEvent::TaskCompleted { .. } => "TaskCompleted",
            ExecutorEvent::TaskFailed { .. } => "TaskFailed",
            ExecutorEvent::TaskTimeout { .. } => "TaskTimeout",
            ExecutorEvent::InferenceStarted { .. } => "InferenceStarted",
            ExecutorEvent::TickAdvanced { .. } => "TickAdvanced",
        };

        // Compute event hash
        let event_hash = self.compute_event_hash(tick, task_id, event);

        // CRITICAL FIX: Acquire write lock BEFORE reading to prevent Merkle chain races
        // Issue: Concurrent threads could read the same last_entry_hash, causing duplicate prev values
        // Fix: Atomic read-modify-write sequence under single write lock
        let (entry_hash, entry) = {
            let mut lock = self.last_entry_hash.write().unwrap();
            let prev_entry_hash = *lock;

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

            // Update last entry hash BEFORE releasing lock (ensures atomicity)
            // CRITICAL: Store event_hash (what next entry will reference), not entry_hash
            *lock = Some(event_hash);

            (entry_hash, entry)
        };

        // Store in database (outside lock to avoid holding lock during I/O)
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

        // Log to telemetry
        if let Some(ref telemetry) = self.telemetry {
            let identity = IdentityEnvelope::new(
                self.tenant_id.clone(),
                "deterministic-exec".to_string(),
                "ledger".to_string(),
                IdentityEnvelope::default_revision(),
            );
            match TelemetryEventBuilder::new(
                adapteros_telemetry::EventType::Custom("tick_ledger.entry".to_string()),
                LogLevel::Debug,
                format!("Tick ledger entry recorded: tick {}", tick),
                identity,
            )
            .component("adapteros-deterministic-exec".to_string())
            .metadata(json!({
                "tick": tick,
                "tenant_id": &self.tenant_id,
                "host_id": &self.host_id,
                "task_id": task_id.to_string(),
                "event_type": event_type,
                "event_hash": event_hash.to_hex(),
            }))
            .build()
            {
                Ok(event) => {
                    let _ = telemetry.log_event(event);
                }
                Err(e) => {
                    warn!(
                        tenant_id = %self.tenant_id,
                        host_id = %self.host_id,
                        task_id = %task_id,
                        tick = tick,
                        error = %e,
                        "Failed to build tick ledger entry event"
                    );
                }
            }
        }

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
        let pool = self.db.pool_result()?;

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

        // Log to telemetry
        if let Some(ref telemetry) = self.telemetry {
            let identity = IdentityEnvelope::new(
                self.tenant_id.clone(),
                "deterministic-exec".to_string(),
                "consistency".to_string(),
                IdentityEnvelope::default_revision(),
            );
            let event_type = if consistent {
                "tick_ledger.consistent"
            } else {
                "tick_ledger.inconsistent"
            };

            match TelemetryEventBuilder::new(
                adapteros_telemetry::EventType::Custom(event_type.to_string()),
                if consistent {
                    LogLevel::Info
                } else {
                    LogLevel::Warn
                },
                format!(
                    "Cross-host consistency check: {} (divergences: {})",
                    if consistent { "PASS" } else { "FAIL" },
                    divergence_count
                ),
                identity,
            )
            .component("adapteros-deterministic-exec".to_string())
            .metadata(json!({
                "tenant_id": &self.tenant_id,
                "host_a": &self.host_id,
                "host_b": peer_host_id,
                "tick_range_start": tick_range.0,
                "tick_range_end": tick_range.1,
                "consistent": consistent,
                "divergence_count": divergence_count,
            }))
            .build()
            {
                Ok(event) => {
                    let _ = telemetry.log_event(event);
                }
                Err(e) => {
                    warn!(
                        tenant_id = %self.tenant_id,
                        host_id = %self.host_id,
                        peer_host_id = %peer_host_id,
                        tick_range_start = tick_range.0,
                        tick_range_end = tick_range.1,
                        error = %e,
                        "Failed to build cross-host consistency event"
                    );
                }
            }
        }

        if consistent {
            info!(
                tenant_id = %self.tenant_id,
                host_id = %self.host_id,
                peer_host_id = %peer_host_id,
                entries = our_entries.len(),
                "Cross-host consistency verified"
            );
        } else {
            warn!(
                tenant_id = %self.tenant_id,
                host_id = %self.host_id,
                peer_host_id = %peer_host_id,
                divergence_count = divergence_count,
                "Cross-host inconsistency detected"
            );
        }

        Ok(report)
    }

    /// Compute event hash
    fn compute_event_hash(&self, tick: u64, task_id: TaskId, event: &ExecutorEvent) -> B3Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&tick.to_le_bytes());
        hasher.update(task_id.as_bytes());

        // Hash event-specific data using canonical JSON (RFC 8785) for cross-host consistency
        let event_bytes = serde_jcs::to_vec(event).unwrap_or_default();
        hasher.update(&event_bytes);

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
    ///
    /// ## Issue C-3 Fix: Transaction Wrapping
    /// Wraps INSERT in explicit transaction for ACID guarantees and clarity.
    /// While the current code already prevents divergence (DB write failure
    /// prevents memory update via early return), explicit transactions provide
    /// better guarantees for future multi-statement operations.
    async fn store_entry(&self, entry: &TickLedgerEntry) -> Result<()> {
        let pool = self.db.pool_result()?;

        let task_id_hex = hex::encode(entry.task_id.as_bytes());
        let event_hash_hex = entry.event_hash.to_hex();
        let prev_hash_hex = entry.prev_entry_hash.as_ref().map(|h| h.to_hex());

        // Issue C-3: Wrap in transaction for ACID guarantees
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| AosError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Note: Federation fields (bundle_hash, prev_host_hash, federation_signature) exist
        // in the tick_ledger_entries schema (added in migration 0035_tick_ledger_federation.sql)
        // but are not currently populated. These fields are reserved for future multi-host
        // federation features where tick ledgers will be synchronized and verified across
        // multiple adapterOS instances. When federation is implemented, these fields will
        // enable cross-host consistency verification and tamper detection.
        sqlx::query(
            r#"
            INSERT INTO tick_ledger_entries
                (tick, tenant_id, host_id, task_id, event_type, event_hash, timestamp_us, prev_entry_hash)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(entry.tick as i64)
        .bind(&entry.tenant_id)
        .bind(&entry.host_id)
        .bind(&task_id_hex)
        .bind(&entry.event_type)
        .bind(&event_hash_hex)
        .bind(entry.timestamp_us as i64)
        .bind(&prev_hash_hex)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            // Transaction will auto-rollback on error
            AosError::Database(format!("Failed to store ledger entry: {}", e))
        })?;

        // Commit transaction
        tx.commit().await.map_err(|e| {
            AosError::Database(format!("Failed to commit ledger transaction: {}", e))
        })?;

        Ok(())
    }

    /// Get peer entries
    async fn get_peer_entries(
        &self,
        peer_host_id: &str,
        tick_range: (u64, u64),
    ) -> Result<Vec<TickLedgerEntry>> {
        let pool = self.db.pool_result()?;

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
    ///
    /// ## Phase 5 Enhancement: Multi-Entry Tick Detection
    /// Now uses multi-value maps to detect and report duplicate ticks (defense-in-depth).
    /// With the fetch_add fix (Phase 1), duplicates should never occur, but this provides
    /// additional safety and diagnostics if the invariant is violated.
    fn compute_divergences(
        &self,
        our_entries: &[TickLedgerEntry],
        peer_entries: &[TickLedgerEntry],
    ) -> Vec<DivergencePoint> {
        let mut divergences = Vec::new();

        // Phase 5: Create multi-value maps to handle duplicate ticks
        let mut our_map: std::collections::HashMap<u64, Vec<&TickLedgerEntry>> =
            std::collections::HashMap::new();
        for entry in our_entries {
            our_map.entry(entry.tick).or_default().push(entry);
        }

        let mut peer_map: std::collections::HashMap<u64, Vec<&TickLedgerEntry>> =
            std::collections::HashMap::new();
        for entry in peer_entries {
            peer_map.entry(entry.tick).or_default().push(entry);
        }

        // Find all unique ticks
        let mut all_ticks: Vec<u64> = our_map.keys().chain(peer_map.keys()).copied().collect();
        all_ticks.sort_unstable();
        all_ticks.dedup();

        // Compare entries at each tick
        for tick in all_ticks {
            let our_entries_at_tick = our_map.get(&tick);
            let peer_entries_at_tick = peer_map.get(&tick);

            match (our_entries_at_tick, peer_entries_at_tick) {
                (Some(our_list), Some(peer_list)) => {
                    let our_count = our_list.len();
                    let peer_count = peer_list.len();

                    // Phase 5: Detect multiple entries at same tick (should never happen with fetch_add)
                    if our_count > 1 || peer_count > 1 {
                        warn!(
                            tick = tick,
                            our_count = our_count,
                            peer_count = peer_count,
                            "CRITICAL: Multiple entries detected at same tick (violates uniqueness invariant)"
                        );
                    }

                    // Check if entry counts differ
                    if our_count != peer_count {
                        // Different number of entries at same tick is a divergence
                        let our_hash = if !our_list.is_empty() {
                            our_list[0].event_hash
                        } else {
                            B3Hash::new([0u8; 32])
                        };
                        let peer_hash = if !peer_list.is_empty() {
                            peer_list[0].event_hash
                        } else {
                            B3Hash::new([0u8; 32])
                        };

                        divergences.push(DivergencePoint {
                            tick,
                            hash_a: our_hash,
                            hash_b: peer_hash,
                            entries_a_count: our_count,
                            entries_b_count: peer_count,
                        });
                    } else {
                        // Same number of entries - compare all hashes
                        for (our_entry, peer_entry) in our_list.iter().zip(peer_list.iter()) {
                            if our_entry.event_hash != peer_entry.event_hash {
                                divergences.push(DivergencePoint {
                                    tick,
                                    hash_a: our_entry.event_hash,
                                    hash_b: peer_entry.event_hash,
                                    entries_a_count: our_count,
                                    entries_b_count: peer_count,
                                });
                                break; // Only report first hash mismatch per tick
                            }
                        }
                    }
                }
                (Some(our_list), None) => {
                    // We have entries, peer doesn't - divergence
                    let zero_hash = B3Hash::new([0u8; 32]);
                    // Edge case: guard against empty list (should not happen but be defensive)
                    let our_hash = our_list.first().map(|e| e.event_hash).unwrap_or(zero_hash);
                    let our_count = our_list.len();

                    divergences.push(DivergencePoint {
                        tick,
                        hash_a: our_hash,
                        hash_b: zero_hash,
                        entries_a_count: our_count,
                        entries_b_count: 0,
                    });
                }
                (None, Some(peer_list)) => {
                    // Peer has entries, we don't - divergence
                    let zero_hash = B3Hash::new([0u8; 32]);
                    // Edge case: guard against empty list (should not happen but be defensive)
                    let peer_hash = peer_list.first().map(|e| e.event_hash).unwrap_or(zero_hash);
                    let peer_count = peer_list.len();

                    divergences.push(DivergencePoint {
                        tick,
                        hash_a: zero_hash,
                        hash_b: peer_hash,
                        entries_a_count: 0,
                        entries_b_count: peer_count,
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
        let pool = self.db.pool_result()?;

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

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect("tempdir")
    }

    async fn setup_test_db() -> (Db, TempDir) {
        let temp_dir = new_test_tempdir();
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

        // record_tick now atomically increments tick internally
        ledger.record_tick(task_id, &event).await.unwrap();

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

    /// Test concurrent record_tick calls produce unique, sequential ticks
    /// Issue C-6: Validates atomic fetch_add prevents duplicate tick assignment
    #[tokio::test]
    async fn test_concurrent_record_tick_unique_ticks() {
        let (db, _temp) = setup_test_db().await;
        let ledger = Arc::new(GlobalTickLedger::new(
            Arc::new(db),
            "test-tenant".to_string(),
            "test-host".to_string(),
        ));

        // Spawn 50 threads, each recording 10 events
        let num_threads = 50;
        let events_per_thread = 10;
        let total_events = num_threads * events_per_thread;

        let mut handles = vec![];
        for thread_id in 0..num_threads {
            let ledger_clone = Arc::clone(&ledger);
            let handle = tokio::spawn(async move {
                for i in 0..events_per_thread {
                    let task_id = TaskId::from_bytes([thread_id as u8; 32]);
                    let event = ExecutorEvent::TaskSpawned {
                        task_id,
                        description: format!("thread-{} event-{}", thread_id, i),
                        tick: 0, // Ignored, will be assigned by record_tick
                        agent_id: None,
                        hash: [thread_id as u8; 32],
                    };
                    ledger_clone.record_tick(task_id, &event).await.unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Fetch all entries and verify uniqueness
        let entries = ledger.get_entries(0, total_events as u64).await.unwrap();
        assert_eq!(entries.len(), total_events, "All events should be recorded");

        // Collect all tick values
        let mut ticks: Vec<u64> = entries.iter().map(|e| e.tick).collect();
        ticks.sort_unstable();

        // Verify no duplicates (guard against empty ticks list)
        if ticks.len() > 1 {
            for i in 0..ticks.len() - 1 {
                assert_ne!(ticks[i], ticks[i + 1], "Duplicate tick found: {}", ticks[i]);
            }
        }

        // Verify sequential (0, 1, 2, ..., 499)
        for (i, &tick) in ticks.iter().enumerate() {
            assert_eq!(tick, i as u64, "Tick {} should equal {}", tick, i);
        }
    }

    /// Test Merkle chain integrity under concurrent writes
    /// Issue C-6: Validates that prev_entry_hash linkage remains valid
    #[tokio::test]
    async fn test_tick_ledger_merkle_chain_integrity() {
        let (db, _temp) = setup_test_db().await;
        let ledger = Arc::new(GlobalTickLedger::new(
            Arc::new(db),
            "test-tenant".to_string(),
            "test-host".to_string(),
        ));

        // Spawn 30 concurrent threads recording events
        let num_threads = 30;
        let events_per_thread = 10;

        let mut handles = vec![];
        for thread_id in 0..num_threads {
            let ledger_clone = Arc::clone(&ledger);
            let handle = tokio::spawn(async move {
                for i in 0..events_per_thread {
                    let task_id = TaskId::from_bytes([thread_id as u8; 32]);
                    let event = ExecutorEvent::TaskCompleted {
                        task_id,
                        tick: 0,
                        duration_ticks: i as u64,
                        agent_id: None,
                        hash: [(thread_id + i) as u8; 32],
                    };
                    ledger_clone.record_tick(task_id, &event).await.unwrap();
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Fetch all entries in tick order
        let entries = ledger
            .get_entries(0, (num_threads * events_per_thread) as u64)
            .await
            .unwrap();

        // Verify Merkle chain linkage
        // First entry should have None as prev_entry_hash
        assert_eq!(
            entries[0].prev_entry_hash, None,
            "First entry should have no previous hash"
        );

        // Each subsequent entry's prev_entry_hash should link to previous event_hash
        // (see record_tick line 284: stores event_hash, not entry_hash)
        for i in 1..entries.len() {
            let prev_entry = &entries[i - 1];
            let current_entry = &entries[i];

            // Current entry's prev_entry_hash should be the previous entry's event_hash
            assert_eq!(
                current_entry.prev_entry_hash,
                Some(prev_entry.event_hash),
                "Entry {} has broken Merkle chain link (expected {:?}, got {:?})",
                i,
                Some(prev_entry.event_hash),
                current_entry.prev_entry_hash
            );
        }
    }

    /// Test high-load scenario with no duplicate ticks
    /// Issue C-6: Stress test for race condition detection
    #[tokio::test]
    async fn test_no_duplicate_ticks_under_load() {
        let (db, _temp) = setup_test_db().await;
        let ledger = Arc::new(GlobalTickLedger::new(
            Arc::new(db),
            "test-tenant".to_string(),
            "test-host".to_string(),
        ));

        // High-frequency concurrent writes
        let num_threads = 100;
        let events_per_thread = 5;
        let total_events = num_threads * events_per_thread;

        let mut handles = vec![];
        for thread_id in 0..num_threads {
            let ledger_clone = Arc::clone(&ledger);
            let handle = tokio::spawn(async move {
                for i in 0..events_per_thread {
                    let task_id = TaskId::from_bytes([thread_id as u8; 32]);
                    let event = ExecutorEvent::TaskFailed {
                        task_id,
                        error: format!("test-error-{}", i),
                        tick: 0,
                        duration_ticks: i as u64,
                        agent_id: None,
                        hash: [i as u8; 32],
                    };
                    // No delay - hit the ledger as fast as possible
                    ledger_clone.record_tick(task_id, &event).await.unwrap();
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all events recorded
        let entries = ledger.get_entries(0, total_events as u64).await.unwrap();
        assert_eq!(
            entries.len(),
            total_events,
            "All {} events should be recorded",
            total_events
        );

        // Build set of all ticks to detect duplicates
        let mut tick_set = std::collections::HashSet::new();
        for entry in &entries {
            assert!(
                tick_set.insert(entry.tick),
                "Duplicate tick detected: {}",
                entry.tick
            );
        }

        assert_eq!(tick_set.len(), total_events, "All ticks should be unique");
    }

    /// Test empty ledger comparison does not panic
    /// Edge case: verify compute_divergences handles empty entry lists gracefully
    #[tokio::test]
    async fn test_empty_ledger_comparison() {
        let (db, _temp) = setup_test_db().await;
        let db = Arc::new(db);

        let ledger_a =
            GlobalTickLedger::new(db.clone(), "test-tenant".to_string(), "host-a".to_string());

        let _ledger_b =
            GlobalTickLedger::new(db.clone(), "test-tenant".to_string(), "host-b".to_string());

        // Don't record any entries - both ledgers are empty

        // This should not panic even with empty ledgers
        let report = ledger_a.verify_cross_host("host-b", (0, 10)).await.unwrap();

        // Empty ledgers should be consistent with each other
        assert!(report.consistent);
        assert_eq!(report.divergence_count, 0);
        assert!(report.divergences.is_empty());
    }

    /// Test single entry comparison (edge case for empty list guards)
    #[tokio::test]
    async fn test_single_entry_comparison() {
        let (db, _temp) = setup_test_db().await;
        let db = Arc::new(db);

        let ledger_a =
            GlobalTickLedger::new(db.clone(), "test-tenant".to_string(), "host-a".to_string());

        let _ledger_b =
            GlobalTickLedger::new(db.clone(), "test-tenant".to_string(), "host-b".to_string());

        // Record one entry on host-a, none on host-b
        let task_id = TaskId::from_bytes([1u8; 32]);
        let event = ExecutorEvent::TaskSpawned {
            task_id,
            description: "test task".to_string(),
            tick: 0,
            agent_id: None,
            hash: [0u8; 32],
        };

        ledger_a.record_tick(task_id, &event).await.unwrap();

        // This should detect divergence without panicking
        let report = ledger_a.verify_cross_host("host-b", (0, 10)).await.unwrap();

        assert!(!report.consistent);
        assert_eq!(report.divergence_count, 1);
    }
}
