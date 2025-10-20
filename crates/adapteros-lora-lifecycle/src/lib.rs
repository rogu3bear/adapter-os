//! Adapter lifecycle management for MPLoRA
//!
//! Orchestrates adapter state transitions:
//! - Promotion (Cold → Warm → Hot → Resident)
//! - Demotion (Hot → Warm → Cold → Unloaded)
//! - Hot-swap loading/unloading
//! - Memory pressure eviction

use adapteros_core::{AosError, Result};
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

pub mod activation_tracker;
pub mod aos_loader;
pub mod category_policies;
pub mod loader;
pub mod policy;
pub mod state;
pub mod ttl_manager;

pub use activation_tracker::ActivationTracker;
pub use aos_loader::{AosDirectLoader, AosMmapHandle, HotSwapResult};
pub use category_policies::{CategoryPolicy, CategoryPolicyManager};
pub use loader::{AdapterHandle, AdapterLoader};
pub use policy::{EvictionOrder, LifecyclePolicy};
pub use state::{AdapterState, AdapterStateRecord, EvictionPriority};
pub use ttl_manager::{EvictionAuditEntry, TtlManager, TtlRecord};

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
}

impl LifecycleManager {
    /// Create a new lifecycle manager
    pub fn new(
        adapter_names: Vec<String>,
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
            loader: Arc::new(RwLock::new(AdapterLoader::new(adapters_base_path))),
            telemetry,
            current_k: Arc::new(RwLock::new(initial_k)),
            category_policies: CategoryPolicyManager::new(),
            db: None,
            activation_tracker: Arc::new(RwLock::new(ActivationTracker::new(200))),
        }
    }

    /// Set database for persistence
    pub fn set_db(&mut self, db: Db) {
        self.db = Some(db);
    }

    /// Create a new lifecycle manager with database integration
    pub fn new_with_db(
        adapter_names: Vec<String>,
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
            loader: Arc::new(RwLock::new(AdapterLoader::new(adapters_base_path))),
            telemetry,
            current_k: Arc::new(RwLock::new(initial_k)),
            category_policies: CategoryPolicyManager::new(),
            db: Some(db),
            activation_tracker: Arc::new(RwLock::new(ActivationTracker::new(200))),
        }
    }

    /// Update rolling activation tracker window size (primarily for tests).
    pub fn set_activation_window(&self, window: usize) {
        let mut tracker = self.activation_tracker.write();
        *tracker = ActivationTracker::new(window);
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
                    let _ = sqlx::query(
                        "UPDATE adapters SET activation_pct = ?, updated_at = datetime('now') \
                         WHERE adapter_id = ?",
                    )
                    .bind(pct)
                    .bind(&adapter_id)
                    .execute(db_clone.pool())
                    .await
                    .map_err(|e| {
                        warn!("Failed to update activation_pct for {}: {}", adapter_id, e);
                    });
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
    pub fn pin_adapter(&self, adapter_id: u16) -> Result<()> {
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

            Ok(())
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter {} not found",
                adapter_id
            )))
        }
    }

    /// Unpin adapter
    pub fn unpin_adapter(&self, adapter_id: u16) -> Result<()> {
        let mut states = self.states.write();

        if let Some(record) = states.get_mut(&adapter_id) {
            record.unpin();
            info!("Unpinned adapter {}", record.adapter_id);
            Ok(())
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter {} not found",
                adapter_id
            )))
        }
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
                let _adapter = loader.load_adapter(record.adapter_idx, adapter_id)?;

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
                let _adapter = loader.load_adapter(record.adapter_idx, adapter_id)?;

                record.state = AdapterState::Cold;

                info!("Auto-reloaded adapter {}", adapter_id);
            }
        }

        Ok(())
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
    pub async fn check_memory_pressure(&self, total_memory: usize, threshold: f32) -> Result<()> {
        let memory_pressure = self.get_total_memory_usage() as f32 / total_memory as f32;

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
    use adapteros_manifest::Policies;

    fn test_policies() -> Policies {
        Policies::default()
    }

    #[test]
    fn test_lifecycle_basic() {
        let adapter_names = vec!["adapter_0".to_string(), "adapter_1".to_string()];
        let temp_dir = std::env::temp_dir().join("mplora_test_lifecycle");
        std::fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let manager =
            LifecycleManager::new(adapter_names, &test_policies(), temp_dir.clone(), None, 3);

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

        let manager =
            LifecycleManager::new(adapter_names, &test_policies(), temp_dir.clone(), None, 3);

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

        let manager =
            LifecycleManager::new(adapter_names, &test_policies(), temp_dir.clone(), None, 2);

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
