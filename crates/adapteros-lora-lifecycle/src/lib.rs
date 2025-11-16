//! Adapter lifecycle management for MPLoRA
//!
//! Orchestrates adapter state transitions:
//! - Promotion (Cold → Warm → Hot → Resident)
//! - Demotion (Hot → Warm → Cold → Unloaded)
//! - Hot-swap loading/unloading
//! - Memory pressure eviction

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::{sqlx, Db};
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_manifest::Policies;
use adapteros_profiler::{AdapterMetrics, AdapterProfiler};
use adapteros_telemetry::TelemetryWriter;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// Telemetry event for adapter state transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterTransitionEvent {
    pub adapter_id: String,
    pub from_state: String,
    pub to_state: String,
    pub reason: String,
}

/// Telemetry event for adapter activations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterActivationEvent {
    pub adapter_id: String,
    pub state: String,
    pub category: String,
    pub activation_count: u64,
}

/// Telemetry event for adapter evictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterEvictionEvent {
    pub adapter_id: String,
    pub from_state: String,
    pub category: String,
    pub memory_freed: usize,
}

/// Telemetry event emitted when adapter load hash validation fails
#[derive(Debug, Clone, Serialize)]
pub struct AdapterLoadHashMismatchEvent {
    pub adapter_id: String,
    pub adapter_idx: u16,
    pub expected_hash: String,
    pub actual_hash: String,
}

/// Telemetry event for GPU buffer integrity verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuIntegrityVerificationEvent {
    pub adapter_id: String,
    pub adapter_idx: u16,
    pub verified: bool,
    pub buffer_bytes: u64,
    pub checkpoint_hash: String,
    pub memory_footprint_within_tolerance: bool,
    pub z_score: Option<f64>,
    pub baseline_mean: Option<f64>,
    pub timestamp: u64,
}

/// Telemetry event for GPU integrity violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuIntegrityViolationEvent {
    pub adapter_id: String,
    pub adapter_idx: u16,
    pub violation_type: String, // "fingerprint_mismatch", "memory_anomaly", "verification_error"
    pub details: String,
    pub buffer_bytes: Option<u64>,
    pub z_score: Option<f64>,
    pub timestamp: u64,
}

pub mod activation_tracker;
pub mod category_policies;
pub mod loader;
pub mod policy;
pub mod state;
pub mod ttl_manager;
pub mod workflow_executor;

pub use activation_tracker::ActivationTracker;
pub use category_policies::{CategoryPolicy, CategoryPolicyManager};
pub use loader::{AdapterHandle, AdapterLoader};
pub use policy::{EvictionOrder, LifecyclePolicy};
pub use state::{AdapterState, AdapterStateRecord, EvictionPriority};
pub use ttl_manager::{EvictionAuditEntry, TtlManager, TtlRecord};
pub use workflow_executor::{
    AdapterExecutionBackend, AdapterExecutionResult, ExecutionStats, KernelAdapterBackend,
    MockAdapterBackend, WorkflowContext, WorkflowExecutor, WorkflowResult, WorkflowType,
};

/// Enhanced lifecycle manager for adapters with category-aware state management
pub struct LifecycleManager {
    /// Adapter states
    states: Arc<RwLock<HashMap<u16, AdapterStateRecord>>>,
    /// Lifecycle policy
    policy: LifecyclePolicy,
    /// Adapter loader
    loader: Arc<RwLock<AdapterLoader>>,
    /// Telemetry writer
    telemetry: Option<TelemetryWriter>,
    /// Current K value for router
    current_k: Arc<RwLock<usize>>,
    /// Category-specific policies
    category_policies: CategoryPolicyManager,
    /// Database connection for persistence
    db: Option<Db>,
    /// Rolling activation tracker fed by router decisions
    activation_tracker: Arc<RwLock<ActivationTracker>>,
    /// Currently active stack (if any)
    active_stack: Arc<RwLock<Option<(String, Vec<String>)>>>, // (name, adapter_ids)
}

impl LifecycleManager {
    /// Create a new lifecycle manager
    pub fn new(
        adapter_names: Vec<String>,
        adapter_hashes: HashMap<String, B3Hash>,
        policies: &Policies,
        adapters_base_path: PathBuf,
        telemetry: Option<TelemetryWriter>,
        initial_k: usize,
    ) -> Self {
        let mut states = HashMap::new();
        for (idx, name) in adapter_names.iter().enumerate() {
            states.insert(
                idx as u16,
                AdapterStateRecord::new(name.clone(), idx as u16),
            );
        }

        Self {
            states: Arc::new(RwLock::new(states)),
            policy: LifecyclePolicy::from_manifest(policies),
            loader: Arc::new(RwLock::new(AdapterLoader::new(
                adapters_base_path,
                adapter_hashes,
            ))),
            telemetry,
            current_k: Arc::new(RwLock::new(initial_k)),
            category_policies: CategoryPolicyManager::new(),
            db: None,
            activation_tracker: Arc::new(RwLock::new(ActivationTracker::new(200))),
            active_stack: Arc::new(RwLock::new(None)),
        }
    }

    /// Set database for persistence
    pub fn set_db(&mut self, db: Db) {
        self.db = Some(db);
    }

    /// Create a new lifecycle manager with database integration
    pub fn new_with_db(
        adapter_names: Vec<String>,
        adapter_hashes: HashMap<String, B3Hash>,
        policies: &Policies,
        adapters_base_path: PathBuf,
        telemetry: Option<TelemetryWriter>,
        initial_k: usize,
        db: Db,
    ) -> Self {
        let mut states = HashMap::new();
        for (idx, name) in adapter_names.iter().enumerate() {
            states.insert(
                idx as u16,
                AdapterStateRecord::new(name.clone(), idx as u16),
            );
        }

        Self {
            states: Arc::new(RwLock::new(states)),
            policy: LifecyclePolicy::from_manifest(policies),
            loader: Arc::new(RwLock::new(AdapterLoader::new(
                adapters_base_path,
                adapter_hashes,
            ))),
            telemetry,
            current_k: Arc::new(RwLock::new(initial_k)),
            category_policies: CategoryPolicyManager::new(),
            db: Some(db),
            activation_tracker: Arc::new(RwLock::new(ActivationTracker::new(200))),
            active_stack: Arc::new(RwLock::new(None)),
        }
    }

    /// Update rolling activation tracker window size (primarily for tests).
    pub fn set_activation_window(&self, window: usize) {
        let mut tracker = self.activation_tracker.write();
        *tracker = ActivationTracker::new(window);
    }

    /// Recover from system crash or unexpected shutdown
    ///
    /// Scans for orphaned adapters and inconsistent state, then cleans up:
    /// 1. Marks adapters stuck in loading state as unloaded
    /// 2. Verifies GPU buffer integrity (if applicable)
    /// 3. Reconciles in-memory state with database
    /// 4. Emits telemetry events for detected issues
    ///
    /// Should be called on server startup before handling requests.
    pub async fn recover_from_crash(&self) -> Result<()> {
        use chrono::Utc;
        use tracing::{error, info, warn};

        info!("Starting crash recovery scan...");

        let db = match &self.db {
            Some(db) => db,
            None => {
                warn!("No database connection - skipping crash recovery");
                return Ok(());
            }
        };

        let mut recovery_actions = Vec::new();

        // 1. Find adapters stuck in "loading" state (orphaned from crash)
        let stale_adapters: Vec<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT adapter_id, name, load_state
            FROM adapters
            WHERE load_state = 'loading'
              AND last_loaded_at < datetime('now', '-5 minutes')
            "#,
        )
        .fetch_all(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to query stale adapters: {}", e)))?;

        if !stale_adapters.is_empty() {
            warn!("Found {} orphaned adapters stuck in loading state", stale_adapters.len());

            for (adapter_id, name, load_state) in stale_adapters {
                recovery_actions.push(format!(
                    "Adapter {} ({}) stuck in state '{}' - marking as unloaded",
                    name, adapter_id, load_state
                ));

                // Mark as unloaded in database
                sqlx::query(
                    "UPDATE adapters SET load_state = 'unloaded', updated_at = datetime('now') WHERE adapter_id = ?",
                )
                .bind(&adapter_id)
                .execute(db.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to update adapter state: {}", e)))?;

                // Emit telemetry event
                if let Some(ref telemetry) = self.telemetry {
                    let event = serde_json::json!({
                        "adapter_id": adapter_id,
                        "adapter_name": name,
                        "stuck_state": load_state,
                        "recovered_at": Utc::now().to_rfc3339(),
                        "recovery_action": "marked_unloaded"
                    });

                    if let Err(e) = telemetry.log("adapter_crash_detected", &event) {
                        error!("Failed to write crash recovery telemetry: {}", e);
                    }
                }

                info!("✓ Recovered adapter: {} ({})", name, adapter_id);
            }
        }

        // 2. Check for adapters with last_heartbeat too old (if heartbeat column exists)
        // Note: This will be implemented in Phase 2.1 when heartbeat mechanism is added

        // 3. Verify adapter count consistency
        let db_adapter_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM adapters")
            .fetch_one(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to count adapters: {}", e)))?;

        let states_count = self.states.read().len();
        if db_adapter_count as usize != states_count {
            warn!(
                "Adapter count mismatch: DB has {}, in-memory has {}",
                db_adapter_count, states_count
            );
            recovery_actions.push(format!(
                "Count mismatch: {} in DB vs {} in memory (may indicate stale data)",
                db_adapter_count, states_count
            ));
        }

        // 4. Clean up stale activation percentages (reset if needed)
        let reset_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM adapters WHERE activation_pct > 1.0 OR activation_pct < 0.0",
        )
        .fetch_one(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to query invalid activation_pct: {}", e)))?;

        if reset_count > 0 {
            warn!("Found {} adapters with invalid activation_pct - resetting", reset_count);

            sqlx::query("UPDATE adapters SET activation_pct = 0.0 WHERE activation_pct > 1.0 OR activation_pct < 0.0")
                .execute(db.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to reset activation_pct: {}", e)))?;

            recovery_actions.push(format!(
                "Reset {} adapters with invalid activation percentages",
                reset_count
            ));
        }

        // Summary
        if recovery_actions.is_empty() {
            info!("✓ Crash recovery complete - no issues detected");
        } else {
            info!("✓ Crash recovery complete - {} actions taken:", recovery_actions.len());
            for action in &recovery_actions {
                info!("  - {}", action);
            }

            // Emit summary telemetry event
            if let Some(ref telemetry) = self.telemetry {
                let event = serde_json::json!({
                    "actions_taken": recovery_actions.len(),
                    "recovery_actions": recovery_actions,
                    "completed_at": Utc::now().to_rfc3339()
                });

                if let Err(e) = telemetry.log("crash_recovery_completed", &event) {
                    error!("Failed to write crash recovery summary telemetry: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Record router selection results to update activation percentages.
    pub async fn record_router_decision(&self, selected: &[u16]) -> Result<()> {
        let changed = {
            let mut tracker = self.activation_tracker.write();
            tracker.record_decision(selected)
        };

        if changed.is_empty() {
            return Ok(());
        }

        let mut updates = Vec::new();
        {
            let states = self.states.read();
            for (adapter_idx, pct) in &changed {
                if let Some(record) = states.get(adapter_idx) {
                    updates.push((
                        *adapter_idx,
                        record.adapter_id.clone(),
                        record.state,
                        record.pinned,
                        *pct,
                    ));
                }
            }
        }

        if let Some(ref db) = self.db {
            for (_, adapter_id, _, _, pct) in updates.iter().cloned() {
                let db_clone = db.clone();
                spawn_deterministic("Activation pct update".to_string(), async move {
                    if let Err(e) = sqlx::query(
                        "UPDATE adapters SET activation_pct = ?, updated_at = datetime('now') \
                         WHERE adapter_id = ?",
                    )
                    .bind(pct)
                    .bind(&adapter_id)
                    .execute(db_clone.pool())
                    .await
                    {
                        warn!("Failed to update activation_pct for {}: {}", adapter_id, e);
                    }
                });
            }
        }

        for (adapter_idx, adapter_id, state, pinned, pct) in updates {
            if pct < self.policy.min_activation_pct && state.is_loaded() && !pinned {
                if let Err(e) = self.evict_adapter(adapter_idx).await {
                    warn!(
                        "Failed to evict low-activation adapter {} ({}): {}",
                        adapter_id, adapter_idx, e
                    );
                }
            }
        }

        Ok(())
    }

    /// Fetch activation percentage tracked for an adapter.
    pub fn activation_pct(&self, adapter_idx: u16) -> f32 {
        let tracker = self.activation_tracker.read();
        tracker.activation_pct(adapter_idx)
    }

    /// Get current state of an adapter
    pub fn get_state(&self, adapter_id: u16) -> Option<AdapterState> {
        let states = self.states.read();
        states.get(&adapter_id).map(|r| r.state)
    }

    /// Get all adapter states
    pub fn get_all_states(&self) -> Vec<AdapterStateRecord> {
        let states = self.states.read();
        states.values().cloned().collect()
    }

    /// Pin adapter to resident state
    ///
    /// Persists pin to database via `pinned_adapters` table.
    /// Pinned adapters will not be evicted by TTL or memory pressure.
    pub async fn pin_adapter(&self, adapter_id: u16, tenant_id: &str, pinned_by: &str, pinned_until: Option<String>, reason: Option<String>) -> Result<()> {
        let adapter_id_str = {
            let mut states = self.states.write();

            if let Some(record) = states.get_mut(&adapter_id) {
                let old_state = record.state;
                record.pin();

                info!("Pinned adapter {} to resident state", record.adapter_id);

                if let Some(ref telemetry) = self.telemetry {
                    telemetry.log(
                        "adapter_promoted",
                        AdapterTransitionEvent {
                            adapter_id: record.adapter_id.clone(),
                            from_state: old_state.to_string(),
                            to_state: AdapterState::Resident.to_string(),
                            reason: "manual_pin".to_string(),
                        },
                    )?;
                }

                record.adapter_id.clone()
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter {} not found",
                    adapter_id
                )));
            }
        };

        // Persist pin to database (single source of truth)
        if let Some(ref db) = self.db {
            // Use tenant_id:adapter_id as stable pin ID
            let pin_id = format!("{}:{}", tenant_id, adapter_id_str);
            let pinned_until_sql = pinned_until.as_deref();
            let reason_sql = reason.as_deref();

            sqlx::query(
                r#"
                INSERT INTO pinned_adapters (id, tenant_id, adapter_id, pinned_until, reason, pinned_by)
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(tenant_id, adapter_id) DO UPDATE SET
                    pinned_until = excluded.pinned_until,
                    reason = excluded.reason,
                    pinned_by = excluded.pinned_by,
                    updated_at = datetime('now')
                "#
            )
            .bind(&pin_id)
            .bind(tenant_id)
            .bind(&adapter_id_str)
            .bind(pinned_until_sql)
            .bind(reason_sql)
            .bind(pinned_by)
            .execute(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to persist adapter pin: {}", e)))?;

            info!("✓ Persisted pin for adapter {} to database", adapter_id_str);
        }

        Ok(())
    }

    /// Unpin adapter
    ///
    /// Removes pin from database. Adapter becomes eligible for eviction again.
    pub async fn unpin_adapter(&self, adapter_id: u16, tenant_id: &str) -> Result<()> {
        let adapter_id_str = {
            let mut states = self.states.write();

            if let Some(record) = states.get_mut(&adapter_id) {
                record.unpin();
                info!("Unpinned adapter {}", record.adapter_id);
                record.adapter_id.clone()
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter {} not found",
                    adapter_id
                )));
            }
        };

        // Remove pin from database (single source of truth)
        if let Some(ref db) = self.db {
            sqlx::query(
                "DELETE FROM pinned_adapters WHERE tenant_id = ? AND adapter_id = ?"
            )
            .bind(tenant_id)
            .bind(&adapter_id_str)
            .execute(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to remove adapter pin: {}", e)))?;

            info!("✓ Removed pin for adapter {} from database", adapter_id_str);
        }

        Ok(())
    }

    /// Manually promote an adapter
    pub fn promote_adapter(&self, adapter_id: u16) -> Result<()> {
        let mut states = self.states.write();

        if let Some(record) = states.get_mut(&adapter_id) {
            let old_state = record.state;

            if record.promote() {
                info!(
                    "Promoted adapter {} from {} to {}",
                    record.adapter_id, old_state, record.state
                );

                if let Some(ref telemetry) = self.telemetry {
                    telemetry.log(
                        "adapter_promoted",
                        AdapterTransitionEvent {
                            adapter_id: record.adapter_id.clone(),
                            from_state: old_state.to_string(),
                            to_state: record.state.to_string(),
                            reason: "manual".to_string(),
                        },
                    )?;
                }

                Ok(())
            } else {
                Err(AosError::Lifecycle(format!(
                    "Cannot promote adapter {} from {}",
                    record.adapter_id, old_state
                )))
            }
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter {} not found",
                adapter_id
            )))
        }
    }

    /// Manually demote an adapter
    pub fn demote_adapter(&self, adapter_id: u16) -> Result<()> {
        let mut states = self.states.write();

        if let Some(record) = states.get_mut(&adapter_id) {
            let old_state = record.state;

            if record.demote() {
                info!(
                    "Demoted adapter {} from {} to {}",
                    record.adapter_id, old_state, record.state
                );

                if let Some(ref telemetry) = self.telemetry {
                    telemetry.log(
                        "adapter_demoted",
                        AdapterTransitionEvent {
                            adapter_id: record.adapter_id.clone(),
                            from_state: old_state.to_string(),
                            to_state: record.state.to_string(),
                            reason: "manual".to_string(),
                        },
                    )?;
                }

                Ok(())
            } else {
                Err(AosError::Lifecycle(format!(
                    "Cannot demote adapter {} from {}",
                    record.adapter_id, old_state
                )))
            }
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter {} not found",
                adapter_id
            )))
        }
    }

    /// Evaluate state transitions based on profiler metrics
    pub fn evaluate_transitions(&self, profiler: &AdapterProfiler) -> Result<()> {
        let metrics = profiler.get_all_metrics();
        let mut states = self.states.write();

        for metric in &metrics {
            // Find adapter by name
            let adapter_id = states
                .iter()
                .find(|(_, r)| r.adapter_id == metric.adapter_id)
                .map(|(id, _)| *id);

            if let Some(adapter_id) = adapter_id {
                if let Some(record) = states.get_mut(&adapter_id) {
                    // Skip pinned adapters
                    if record.pinned {
                        continue;
                    }

                    let old_state = record.state;

                    // Check for promotion
                    if self.policy.should_promote(metric) {
                        if record.promote() {
                            info!(
                                "Auto-promoted adapter {} from {} to {}",
                                record.adapter_id, old_state, record.state
                            );

                            if let Some(ref telemetry) = self.telemetry {
                                telemetry.log(
                                    "adapter_promoted",
                                    AdapterTransitionEvent {
                                        adapter_id: record.adapter_id.clone(),
                                        from_state: old_state.to_string(),
                                        to_state: record.state.to_string(),
                                        reason: "high_activation".to_string(),
                                    },
                                )?;
                            }
                        }
                    }
                    // Check for demotion
                    else if self.policy.should_demote(metric) && record.demote() {
                        info!(
                            "Auto-demoted adapter {} from {} to {}",
                            record.adapter_id, old_state, record.state
                        );

                        if let Some(ref telemetry) = self.telemetry {
                            telemetry.log(
                                "adapter_demoted",
                                AdapterTransitionEvent {
                                    adapter_id: record.adapter_id.clone(),
                                    from_state: old_state.to_string(),
                                    to_state: record.state.to_string(),
                                    reason: "low_activation".to_string(),
                                },
                            )?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle memory pressure by evicting adapters
    pub fn handle_memory_pressure(&self, profiler: &AdapterProfiler) -> Result<()> {
        warn!("Handling memory pressure");

        let metrics = profiler.get_all_metrics();
        let mut states = self.states.write();

        // Sort adapters by eviction priority (cold, low activation first)
        let mut candidates: Vec<(u16, &AdapterMetrics)> = states
            .iter()
            .filter_map(|(id, record)| {
                if record.pinned {
                    return None; // Never evict pinned
                }
                metrics
                    .iter()
                    .find(|m| m.adapter_id == record.adapter_id)
                    .map(|m| (*id, m))
            })
            .collect();

        // Sort by activation percentage (lowest first)
        candidates.sort_by(|a, b| {
            a.1.activation_pct
                .partial_cmp(&b.1.activation_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Try evicting cold adapters first
        for (adapter_id, metric) in candidates {
            if let Some(record) = states.get_mut(&adapter_id) {
                if record.state == AdapterState::Cold || self.policy.should_evict(metric) {
                    let _old_state = record.state;
                    record.state = AdapterState::Unloaded;

                    info!(
                        "Evicted adapter {} due to memory pressure",
                        record.adapter_id
                    );

                    if let Some(ref telemetry) = self.telemetry {
                        telemetry.log(
                            "adapter_evicted",
                            AdapterEvictionEvent {
                                adapter_id: record.adapter_id.clone(),
                                from_state: record.state.to_string(),
                                category: record.category.clone(),
                                memory_freed: record.memory_bytes,
                            },
                        )?;
                    }

                    // Unload from memory
                    let mut loader = self.loader.write();
                    loader.unload_adapter(adapter_id)?;

                    return Ok(()); // Evicted one, check if enough
                }
            }
        }

        // If still under pressure, reduce K
        self.reduce_k()?;

        Ok(())
    }

    /// Reduce K value for router
    fn reduce_k(&self) -> Result<()> {
        let mut k = self.current_k.write();

        if *k > 1 {
            let old_k = *k;
            *k -= 1;

            warn!("Reduced K from {} to {} due to memory pressure", old_k, *k);

            if let Some(ref telemetry) = self.telemetry {
                telemetry.log(
                    "k_reduced",
                    KReductionEvent {
                        old_k,
                        new_k: *k,
                        reason: "memory_pressure".to_string(),
                    },
                )?;
            }

            Ok(())
        } else {
            Err(AosError::MemoryPressure(
                "Cannot reduce K below 1".to_string(),
            ))
        }
    }

    /// Warm up cache by preloading specified adapters
    pub fn warmup_cache(&mut self, adapter_ids: &[String]) -> Result<()> {
        info!("Warming up cache with {} adapters", adapter_ids.len());

        for adapter_id in adapter_ids {
            if let Err(e) = self.preload_adapter(adapter_id) {
                warn!("Failed to preload adapter {}: {}", adapter_id, e);
                // Continue with other adapters
            }
        }

        Ok(())
    }

    /// Preload a specific adapter into cache
    fn preload_adapter(&mut self, adapter_id: &str) -> Result<()> {
        let mut states = self.states.write();

        // Find record by adapter_id
        if let Some(record) = states.values_mut().find(|r| r.adapter_id == adapter_id) {
            if record.state == AdapterState::Unloaded {
                let mut loader = self.loader.write();
                let handle = match loader.load_adapter(record.adapter_idx, adapter_id) {
                    Ok(handle) => handle,
                    Err(err) => {
                        self.report_adapter_hash_mismatch(adapter_id, record.adapter_idx, &err);
                        return Err(err);
                    }
                };

                record.state = AdapterState::Cold;

                info!("Preloaded adapter {}", adapter_id);
            }
        }

        Ok(())
    }

    /// Get or reload adapter with automatic reload on cache miss
    pub fn get_or_reload(&mut self, adapter_id: &str) -> Result<()> {
        let mut states = self.states.write();

        // Find record by adapter_id
        if let Some(record) = states.values_mut().find(|r| r.adapter_id == adapter_id) {
            if record.state == AdapterState::Unloaded {
                let mut loader = self.loader.write();
                let handle = match loader.load_adapter(record.adapter_idx, adapter_id) {
                    Ok(handle) => handle,
                    Err(err) => {
                        self.report_adapter_hash_mismatch(adapter_id, record.adapter_idx, &err);
                        return Err(err);
                    }
                };

                record.state = AdapterState::Cold;

                info!("Auto-reloaded adapter {}", adapter_id);
            }
        }

        Ok(())
    }

    fn report_adapter_hash_mismatch(&self, adapter_id: &str, adapter_idx: u16, err: &AosError) {
        if let AosError::AdapterHashMismatch {
            adapter_id: mismatch_id,
            expected,
            actual,
        } = err
        {
            if let Some(ref telemetry) = self.telemetry {
                let event = AdapterLoadHashMismatchEvent {
                    adapter_id: mismatch_id.clone(),
                    adapter_idx,
                    expected_hash: expected.to_hex(),
                    actual_hash: actual.to_hex(),
                };

                if let Err(log_err) = telemetry.log("adapter_load_failed_hash_mismatch", event) {
                    warn!(
                        adapter_id = adapter_id,
                        error = %log_err,
                        "Failed to log adapter hash mismatch telemetry"
                    );
                }
            }
        }
    }

    /// Get current K value
    pub fn current_k(&self) -> usize {
        *self.current_k.read()
    }

    /// Update adapter state with category awareness
    pub async fn update_adapter_state(
        &self,
        adapter_id: u16,
        new_state: AdapterState,
        reason: &str,
    ) -> Result<()> {
        let mut states = self.states.write();

        if let Some(record) = states.get_mut(&adapter_id) {
            let old_state = record.state;
            record.state = new_state;

            // Update database if available
            if let Some(ref db) = self.db {
                let adapter_id_str = record.adapter_id.clone();
                let state_str = new_state.to_string();
                let reason_str = reason.to_string();
                let db_clone = db.clone();

                // Spawn async task to update database without blocking
                let _ = spawn_deterministic("Adapter state update".to_string(), async move {
                    if let Err(e) = db_clone
                        .update_adapter_state(&adapter_id_str, &state_str, &reason_str)
                        .await
                    {
                        warn!("Failed to update adapter state in database: {}", e);
                    }
                });
            }

            // Log transition
            if let Some(ref telemetry) = self.telemetry {
                telemetry.log(
                    "adapter_state_transition",
                    AdapterTransitionEvent {
                        adapter_id: record.adapter_id.clone(),
                        from_state: old_state.to_string(),
                        to_state: new_state.to_string(),
                        reason: reason.to_string(),
                    },
                )?;
            }

            info!(
                "Updated adapter {} state: {} -> {} ({})",
                record.adapter_id, old_state, new_state, reason
            );
        }

        Ok(())
    }

    /// Auto-promote adapter based on category policy
    pub async fn auto_promote_adapter(&self, adapter_id: u16) -> Result<()> {
        let states = self.states.read();

        if let Some(record) = states.get(&adapter_id) {
            let category = &record.category;
            let current_state = record.state;

            if current_state.can_promote(category) {
                if let Some(next_state) = current_state.promote() {
                    drop(states); // Release read lock before write
                    self.update_adapter_state(adapter_id, next_state, "auto_promotion")
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Auto-demote adapter based on category policy and inactivity
    pub async fn auto_demote_adapter(&self, adapter_id: u16) -> Result<()> {
        let states = self.states.read();

        if let Some(record) = states.get(&adapter_id) {
            let category = &record.category;
            let current_state = record.state;

            // Check if we should demote based on last activation time
            if let Some(last_activated) = record.last_activated {
                let time_since_activation = last_activated
                    .elapsed()
                    .unwrap_or(std::time::Duration::from_secs(0));
                if current_state.should_demote(category, time_since_activation) {
                    if let Some(next_state) = current_state.demote() {
                        drop(states); // Release read lock before write
                        self.update_adapter_state(adapter_id, next_state, "auto_demotion")
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Record adapter activation
    pub async fn record_adapter_activation(&self, adapter_id: u16) -> Result<()> {
        let mut states = self.states.write();

        if let Some(record) = states.get_mut(&adapter_id) {
            record.record_activation();

            // Update database if available
            if let Some(ref db) = self.db {
                let adapter_id_str = record.adapter_id.clone();
                let activation_count = record.activation_count;
                let db_clone = db.clone();

                // Spawn async task to update database without blocking
                let _ = spawn_deterministic("Adapter activation update".to_string(), async move {
                    // Record activation event
                    if let Err(e) = db_clone
                        .record_activation(&adapter_id_str, None, 1.0, true)
                        .await
                    {
                        warn!("Failed to record adapter activation in database: {}", e);
                    }

                    // Update activation count and last_activated timestamp
                    if let Err(e) = sqlx::query(
                        "UPDATE adapters SET 
                         activation_count = ?, 
                         last_activated = datetime('now'),
                         updated_at = datetime('now')
                         WHERE adapter_id = ?",
                    )
                    .bind(activation_count as i64)
                    .bind(&adapter_id_str)
                    .execute(db_clone.pool())
                    .await
                    {
                        warn!(
                            "Failed to update adapter activation count in database: {}",
                            e
                        );
                    }
                });
            }

            // Log activation
            if let Some(ref telemetry) = self.telemetry {
                telemetry.log(
                    "adapter_activated",
                    AdapterActivationEvent {
                        adapter_id: record.adapter_id.clone(),
                        state: record.state.to_string(),
                        category: record.category.clone(),
                        activation_count: record.activation_count,
                    },
                )?;
            }
        }

        Ok(())
    }

    /// Get adapters by category
    pub fn get_adapters_by_category(&self, category: &str) -> Vec<AdapterStateRecord> {
        let states = self.states.read();
        states
            .values()
            .filter(|record| record.category == category)
            .cloned()
            .collect()
    }

    /// Get adapters by state
    pub fn get_adapters_by_state(&self, state: AdapterState) -> Vec<AdapterStateRecord> {
        let states = self.states.read();
        states
            .values()
            .filter(|record| record.state == state)
            .cloned()
            .collect()
    }

    /// Get memory usage by category
    pub fn get_memory_usage_by_category(&self) -> HashMap<String, usize> {
        let states = self.states.read();
        let mut usage = HashMap::new();

        for record in states.values() {
            let entry = usage.entry(record.category.clone()).or_insert(0);
            *entry += record.memory_bytes;
        }

        usage
    }

    /// Check memory pressure and evict adapters if needed
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 2.2
    /// Enhanced to prioritize eviction of expired adapters
    pub async fn check_memory_pressure(&self, total_memory: usize, threshold: f32) -> Result<()> {
        let memory_pressure = self.get_total_memory_usage() as f32 / total_memory as f32;

        // First, check for and evict expired adapters regardless of memory pressure
        // This ensures expired adapters don't linger in memory
        if let Some(ref db) = self.db {
            if let Ok(expired_adapters) = db.find_expired_adapters().await {
                if !expired_adapters.is_empty() {
                    info!(
                        count = expired_adapters.len(),
                        "Found expired adapters during memory check, evicting immediately"
                    );

                    for expired in &expired_adapters {
                        if let Some(adapter_id) = self.get_adapter_id_by_name(&expired.name) {
                            info!(
                                adapter_id = %expired.name,
                                expired_at = ?expired.expires_at,
                                "Evicting expired adapter"
                            );
                            let _ = self.evict_adapter(adapter_id).await;
                        }
                    }
                }
            }
        }

        if memory_pressure > threshold {
            info!(
                "Memory pressure detected: {:.2} (threshold: {:.2})",
                memory_pressure, threshold
            );

            // Get adapters sorted by eviction priority
            let states = self.states.read();
            let mut eviction_candidates: Vec<_> = states
                .values()
                .filter(|record| record.should_evict(memory_pressure))
                .collect();

            eviction_candidates.sort_by(|a, b| {
                b.eviction_priority()
                    .numeric_value()
                    .cmp(&a.eviction_priority().numeric_value())
            });

            // Evict adapters starting with highest priority
            for record in eviction_candidates {
                if let Some(adapter_id) = self.get_adapter_id_by_name(&record.adapter_id) {
                    self.evict_adapter(adapter_id).await?;

                    // Check if memory pressure is resolved
                    let new_pressure = self.get_total_memory_usage() as f32 / total_memory as f32;
                    if new_pressure <= threshold {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Get total memory usage across all adapters
    pub fn get_total_memory_usage(&self) -> usize {
        let states = self.states.read();
        states.values().map(|record| record.memory_bytes).sum()
    }

    /// Get adapter ID by name
    fn get_adapter_id_by_name(&self, adapter_name: &str) -> Option<u16> {
        let states = self.states.read();
        states
            .iter()
            .find(|(_, record)| record.adapter_id == adapter_name)
            .map(|(id, _)| *id)
    }

    /// Evict an adapter (unload from memory)
    pub async fn evict_adapter(&self, adapter_id: u16) -> Result<()> {
        let mut states = self.states.write();

        if let Some(record) = states.get_mut(&adapter_id) {
            if record.pinned {
                return Err(AosError::Lifecycle(format!(
                    "Cannot evict pinned adapter: {}",
                    record.adapter_id
                )));
            }

            let old_state = record.state;
            let memory_freed = record.memory_bytes;
            record.state = AdapterState::Unloaded;
            record.memory_bytes = 0;

            // Unload from loader
            let mut loader = self.loader.write();
            loader.unload_adapter(adapter_id)?;

            // Update database if available
            if let Some(ref db) = self.db {
                let adapter_id_str = record.adapter_id.clone();
                let db_clone = db.clone();

                // Spawn async task to update database without blocking
                let _ = spawn_deterministic("Adapter eviction update".to_string(), async move {
                    // Update adapter state to unloaded and reset memory
                    if let Err(e) = db_clone
                        .update_adapter_state(&adapter_id_str, "unloaded", "eviction")
                        .await
                    {
                        warn!(
                            "Failed to update adapter state during eviction in database: {}",
                            e
                        );
                    }

                    // Update memory usage to 0
                    if let Err(e) = db_clone.update_adapter_memory(&adapter_id_str, 0).await {
                        warn!(
                            "Failed to update adapter memory during eviction in database: {}",
                            e
                        );
                    }
                });
            }

            // Log eviction
            if let Some(ref telemetry) = self.telemetry {
                telemetry.log(
                    "adapter_evicted",
                    AdapterEvictionEvent {
                        adapter_id: record.adapter_id.clone(),
                        from_state: old_state.to_string(),
                        category: record.category.clone(),
                        memory_freed,
                    },
                )?;
            }

            info!(
                "Evicted adapter {} ({} -> unloaded)",
                record.adapter_id, old_state
            );
        }

        Ok(())
    }

    /// Get category policy manager
    pub fn get_category_policies(&self) -> &CategoryPolicyManager {
        &self.category_policies
    }

    /// Update category policy
    pub fn update_category_policy(&mut self, category: String, policy: CategoryPolicy) {
        self.category_policies.update_policy(category, policy);
    }

    /// Get available adapters for routing
    pub fn get_available_adapters(&self) -> Vec<u16> {
        let states = self.states.read();
        states
            .iter()
            .filter(|(_, record)| record.state.is_available())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get state-based priority boosts for routing
    pub fn get_priority_boosts(&self) -> HashMap<u16, f32> {
        let states = self.states.read();
        states
            .iter()
            .map(|(id, record)| (*id, record.state.priority_boost()))
            .collect()
    }

    /// Load and activate an adapter stack
    pub async fn activate_stack(&self, stack_name: String, adapter_ids: Vec<String>) -> Result<()> {
        use tracing::{debug, info};

        info!(
            "Activating adapter stack '{}' with {} adapters",
            stack_name,
            adapter_ids.len()
        );

        // Ensure all adapters in the stack are loaded
        for adapter_id in &adapter_ids {
            debug!("Checking if adapter {} is loaded", adapter_id);

            // Find the adapter index by ID
            let adapter_idx = {
                let states = self.states.read();
                states
                    .iter()
                    .find(|(_, record)| record.adapter_id == *adapter_id)
                    .map(|(idx, _)| *idx)
            };

            if let Some(idx) = adapter_idx {
                // Check if adapter is loaded
                let is_loaded = {
                    let states = self.states.read();
                    states
                        .get(&idx)
                        .map_or(false, |record| record.state.is_loaded())
                };

                if !is_loaded {
                    info!(
                        "Adapter {} needs to be loaded for stack {}",
                        adapter_id, stack_name
                    );
                    // In a real implementation, we would load the adapter here
                    // For now, we just log that it needs loading
                }
            } else {
                return Err(AosError::NotFound(format!(
                    "Adapter {} not found in lifecycle manager",
                    adapter_id
                ))
                .into());
            }
        }

        // Update the active stack
        {
            let mut active_stack = self.active_stack.write();
            *active_stack = Some((stack_name.clone(), adapter_ids.clone()));
        }

        // Log telemetry event
        if let Some(ref telemetry) = self.telemetry {
            telemetry.log(
                "lifecycle.stack_activated",
                serde_json::json!({
                    "stack_name": stack_name,
                    "adapter_count": adapter_ids.len(),
                    "adapter_ids": adapter_ids,
                }),
            )?;
        }

        info!("Stack '{}' activated successfully", stack_name);
        Ok(())
    }

    /// Deactivate the current stack
    pub async fn deactivate_stack(&self) -> Result<()> {
        let stack_info = {
            let mut active_stack = self.active_stack.write();
            active_stack.take()
        };

        if let Some((name, _)) = stack_info {
            // Log telemetry event
            if let Some(ref telemetry) = self.telemetry {
                telemetry.log(
                    "lifecycle.stack_deactivated",
                    serde_json::json!({
                        "stack_name": name,
                    }),
                )?;
            }

            info!("Stack '{}' deactivated", name);
        }

        Ok(())
    }

    /// Get the currently active stack
    pub fn get_active_stack(&self) -> Option<(String, Vec<String>)> {
        let active_stack = self.active_stack.read();
        active_stack.clone()
    }

    /// Load a stack from database and activate it
    pub async fn load_and_activate_stack(&self, stack_id: &str) -> Result<()> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| AosError::Database("Database not configured".to_string()))?;

        // Query the stack from database
        let row = sqlx::query_as::<_, (String, String, Option<String>)>(
            r#"
            SELECT name, adapter_ids_json, workflow_type
            FROM adapter_stacks
            WHERE id = ?
            "#,
        )
        .bind(stack_id)
        .fetch_optional(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch stack: {}", e)))?
        .ok_or_else(|| AosError::NotFound(format!("Stack {} not found", stack_id)))?;

        let adapter_ids: Vec<String> =
            serde_json::from_str(&row.1).map_err(|e| AosError::Serialization(e))?;

        self.activate_stack(row.0, adapter_ids).await
    }

    /// Execute the current stack's workflow
    pub async fn execute_stack_workflow(&self, context: WorkflowContext) -> Result<WorkflowResult> {
        let stack_info = {
            let active_stack = self.active_stack.read();
            active_stack.clone()
        };

        let (stack_name, adapter_ids) =
            stack_info.ok_or_else(|| AosError::Lifecycle("No active stack".to_string()))?;

        info!("Executing workflow for stack '{}'", stack_name);

        // Get workflow type from database if available
        let workflow_type = if let Some(ref db) = self.db {
            let row = sqlx::query_scalar::<_, Option<String>>(
                "SELECT workflow_type FROM adapter_stacks WHERE name = ?",
            )
            .bind(&stack_name)
            .fetch_optional(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to fetch workflow type: {}", e)))?;

            match row.and_then(|wt| wt) {
                Some(wt) => match wt.as_str() {
                    "parallel" => WorkflowType::Parallel,
                    "upstream_downstream" => WorkflowType::UpstreamDownstream,
                    "sequential" => WorkflowType::Sequential,
                    _ => WorkflowType::Parallel, // Default
                },
                None => WorkflowType::Parallel, // Default if not specified
            }
        } else {
            WorkflowType::Parallel // Default if no database
        };

        // Create and execute workflow
        // Note: Uses MockAdapterBackend for workflow coordination/testing.
        // For real kernel execution with LoRA transformations, use Worker::execute_workflow()
        // which creates KernelAdapterBackend with shared kernel access.
        let backend = Arc::new(MockAdapterBackend);
        let executor = WorkflowExecutor::new(workflow_type, adapter_ids, backend);
        let result = executor.execute(context).await?;

        // Log telemetry
        if let Some(ref telemetry) = self.telemetry {
            telemetry.log(
                "lifecycle.workflow_executed",
                serde_json::json!({
                    "stack_name": stack_name,
                    "adapters_executed": result.stats.adapters_executed,
                    "total_time_ms": result.stats.total_time_ms,
                    "phases": result.stats.phases.len(),
                }),
            )?;
        }

        Ok(result)
    }

    // ===== GPU Integrity Verification API =====

    /// Get current adapter state for GPU verification
    ///
    /// Returns list of (adapter_id, adapter_name, state) for adapters that should
    /// have GPU buffers loaded. Used by external GPU verification code.
    pub fn get_loaded_adapters(&self) -> Vec<(u16, String, AdapterState)> {
        let states = self.states.read();
        states
            .iter()
            .filter_map(|(id, record)| {
                // Only adapters in Cold, Warm, Hot, or Resident states have GPU buffers
                if !matches!(record.state, AdapterState::Unloaded) {
                    Some((*id, record.adapter_id.clone(), record.state))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Mark adapter state as verified with GPU
    ///
    /// Called by external GPU verification code after successful buffer verification.
    /// This is an integration point for cross-layer integrity checks.
    ///
    /// # Arguments
    /// * `adapter_id` - Adapter that was verified
    /// * `gpu_fingerprint_hash` - BLAKE3 hash of GPU buffer fingerprint
    ///
    /// # Usage
    /// ```no_run
    /// // In Worker or orchestrator layer with both lifecycle and GPU access:
    /// let (buffer_size, first, last, mid) = kernels.verify_adapter_buffers(adapter_id)?;
    /// let fingerprint = GpuBufferFingerprint::new(buffer_size, &first, &last, &mid);
    /// lifecycle.mark_gpu_verified(adapter_id, fingerprint.checkpoint_hash)?;
    /// ```
    pub fn mark_gpu_verified(&self, adapter_id: u16, _gpu_fingerprint_hash: B3Hash) -> Result<()> {
        // For now, just log verification (future: store verification timestamp in state record)
        let states = self.states.read();
        if let Some(record) = states.get(&adapter_id) {
            info!(
                "GPU verification passed for adapter {} ({})",
                adapter_id, record.adapter_id
            );
        }
        Ok(())
    }

    /// Update heartbeat for an adapter
    ///
    /// Updates last_heartbeat timestamp in database to indicate adapter is alive
    pub async fn heartbeat_adapter(&self, adapter_id: &str) -> Result<()> {
        if let Some(ref db) = self.db {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| AosError::Internal(format!("System time error: {}", e)))?
                .as_secs() as i64;

            sqlx::query(
                "UPDATE adapters SET last_heartbeat = ? WHERE id = ?"
            )
            .bind(now)
            .bind(adapter_id)
            .execute(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update heartbeat: {}", e)))?;

            tracing::trace!(adapter_id = %adapter_id, timestamp = now, "Updated adapter heartbeat");
        }
        Ok(())
    }

    /// Check for stale adapters (no heartbeat in threshold seconds)
    ///
    /// Returns list of adapter IDs that haven't sent heartbeat recently
    pub async fn check_stale_adapters(&self, threshold_seconds: i64) -> Result<Vec<String>> {
        if let Some(ref db) = self.db {
            let cutoff = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| AosError::Internal(format!("System time error: {}", e)))?
                .as_secs() as i64
                - threshold_seconds;

            let stale: Vec<(String,)> = sqlx::query_as(
                "SELECT id FROM adapters
                 WHERE last_heartbeat IS NOT NULL
                   AND last_heartbeat < ?
                   AND load_state != 'unloaded'"
            )
            .bind(cutoff)
            .fetch_all(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to query stale adapters: {}", e)))?;

            let stale_ids: Vec<String> = stale.into_iter().map(|(id,)| id).collect();

            if !stale_ids.is_empty() {
                tracing::warn!(count = stale_ids.len(), threshold_seconds, "Detected stale adapters");
            }

            return Ok(stale_ids);
        }
        Ok(vec![])
    }

    /// Auto-recover stale adapters by resetting their state
    ///
    /// Called periodically to detect and recover adapters that stopped sending heartbeats
    pub async fn recover_stale_adapters(&self, threshold_seconds: i64) -> Result<Vec<String>> {
        let stale_ids = self.check_stale_adapters(threshold_seconds).await?;
        let mut recovered = Vec::new();

        for adapter_id in stale_ids {
            // Reset state to unloaded for stale adapters
            if let Some(ref db) = self.db {
                sqlx::query(
                    "UPDATE adapters
                     SET load_state = 'unloaded', last_heartbeat = NULL
                     WHERE id = ?"
                )
                .bind(&adapter_id)
                .execute(db.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to reset stale adapter: {}", e)))?;

                tracing::info!(adapter_id = %adapter_id, "Recovered stale adapter");
                recovered.push(adapter_id);
            }
        }

        Ok(recovered)
    }
}

/// GPU integrity verification report
///
/// Returned by external verification code to indicate which adapters passed/failed
/// GPU buffer integrity checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuIntegrityReport {
    /// Adapters that passed verification
    pub verified: Vec<(u16, String)>,
    /// Adapters that failed verification (id, name, reason)
    pub failed: Vec<(u16, String, String)>,
    /// Adapters that were skipped (not in GPU)
    pub skipped: Vec<(u16, String)>,
    /// Total adapters checked
    pub total_checked: usize,
    /// Verification timestamp
    pub timestamp: u64,
}

/// K reduction event for telemetry
#[derive(Debug, Clone, serde::Serialize)]
pub struct KReductionEvent {
    pub old_k: usize,
    pub new_k: usize,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::B3Hash;
    use adapteros_manifest::Policies;
    use std::collections::HashMap;

    fn test_policies() -> Policies {
        Policies::default()
    }

    fn build_adapter_hashes(names: &[String]) -> HashMap<String, B3Hash> {
        names
            .iter()
            .map(|name| (name.clone(), B3Hash::hash(name.as_bytes())))
            .collect()
    }

    #[test]
    fn test_lifecycle_basic() {
        let adapter_names = vec!["adapter_0".to_string(), "adapter_1".to_string()];
        let temp_dir = std::env::temp_dir().join("mplora_test_lifecycle");
        std::fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let adapter_hashes = build_adapter_hashes(&adapter_names);
        let manager = LifecycleManager::new(
            adapter_names.clone(),
            adapter_hashes,
            &test_policies(),
            temp_dir.clone(),
            None,
            3,
        );

        // Initial state should be unloaded
        assert_eq!(manager.get_state(0), Some(AdapterState::Unloaded));

        // Promote adapter
        manager
            .promote_adapter(0)
            .expect("Test adapter promotion should succeed");
        assert_eq!(manager.get_state(0), Some(AdapterState::Cold));

        // Demote adapter
        manager
            .demote_adapter(0)
            .expect("Test adapter demotion should succeed");
        assert_eq!(manager.get_state(0), Some(AdapterState::Unloaded));

        std::fs::remove_dir_all(temp_dir).expect("Test cleanup should succeed");
    }

    #[test]
    fn test_pinning() {
        let adapter_names = vec!["adapter_0".to_string()];
        let temp_dir = std::env::temp_dir().join("mplora_test_pinning");
        std::fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let adapter_hashes = build_adapter_hashes(&adapter_names);
        let manager = LifecycleManager::new(
            adapter_names.clone(),
            adapter_hashes,
            &test_policies(),
            temp_dir.clone(),
            None,
            3,
        );

        // Pin adapter
        manager
            .pin_adapter(0)
            .expect("Test adapter pinning should succeed");
        assert_eq!(manager.get_state(0), Some(AdapterState::Resident));

        // Cannot demote pinned adapter
        assert!(manager.demote_adapter(0).is_err());
        assert_eq!(manager.get_state(0), Some(AdapterState::Resident));

        // Unpin and then demote
        manager
            .unpin_adapter(0)
            .expect("Test adapter unpinning should succeed");
        manager
            .demote_adapter(0)
            .expect("Test adapter demotion should succeed");
        assert_eq!(manager.get_state(0), Some(AdapterState::Hot));

        std::fs::remove_dir_all(temp_dir).expect("Test cleanup should succeed");
    }

    #[tokio::test]
    async fn router_decision_updates_activation_and_eviction() {
        let adapter_names = vec!["adapter_a".to_string(), "adapter_b".to_string()];
        let temp_dir = std::env::temp_dir().join("mplora_activation_tracker");
        std::fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let adapter_hashes = build_adapter_hashes(&adapter_names);
        let manager = LifecycleManager::new(
            adapter_names.clone(),
            adapter_hashes,
            &test_policies(),
            temp_dir.clone(),
            None,
            2,
        );

        manager.set_activation_window(3);
        manager
            .promote_adapter(0)
            .expect("promotion should succeed");
        manager
            .promote_adapter(1)
            .expect("promotion should succeed");

        manager
            .record_router_decision(&[0])
            .await
            .expect("record should succeed");
        assert!((manager.activation_pct(0) - 100.0).abs() < 1e-3);

        manager
            .record_router_decision(&[1])
            .await
            .expect("record should succeed");
        manager
            .record_router_decision(&[1])
            .await
            .expect("record should succeed");

        // Adapter 0 should fall below activation threshold and be evicted
        assert_eq!(manager.get_state(0), Some(AdapterState::Unloaded));

        std::fs::remove_dir_all(temp_dir).expect("Test cleanup should succeed");
    }
}
