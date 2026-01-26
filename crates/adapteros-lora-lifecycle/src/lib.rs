//! Adapter lifecycle management for DIR
//!
//! Orchestrates adapter state transitions:
//! - Promotion (Cold → Warm → Hot → Resident)
//! - Demotion (Hot → Warm → Cold → Unloaded)
//! - Hot-swap loading/unloading
//! - Memory pressure eviction

#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::type_complexity)]
// REMOVED: #![allow(clippy::await_holding_lock)] - Now explicitly scoped per method
#![allow(clippy::option_map_or_none)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::redundant_closure)]
#![allow(unused_must_use)]
#![allow(clippy::bind_instead_of_map)]
#![allow(clippy::unnecessary_map_or)]

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::{sqlx, Db, ProtectedDb};
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_model_hub::manifest::Policies;
use adapteros_model_hub::{ModelHubClient, ModelHubConfig};
use adapteros_telemetry::profiler::{AdapterMetrics, AdapterProfiler};
use adapteros_telemetry::TelemetryWriter;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};
use utoipa::ToSchema;

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// K reduction execution record for audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KReductionExecutionRecord {
    pub request_id: String,
    pub old_k: usize,
    pub new_k: usize,
    pub approved: bool,
    pub executed: bool,
    pub adapters_unloaded: Vec<u16>,
    pub failure_reason: Option<String>,
    pub timestamp: std::time::SystemTime,
}

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

/// Download progress event for model hub acquisition
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub phase: String, // "downloading", "verifying", "loading"
    pub progress_pct: u8,
    pub eta_seconds: Option<u64>,
    pub speed_mbps: Option<f64>,
}

/// Model acquisition state for tracking downloads
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AcquisitionState {
    /// Model not present locally
    NotAvailable,
    /// Download in progress
    Downloading { progress_pct: u8 },
    /// Verifying downloaded model
    Verifying,
    /// Model available locally
    Available,
    /// Download failed
    Failed { reason: String },
}

pub mod activation_tracker;
pub mod category_policies;
pub mod k_reduction_coordinator;
pub mod loader;
pub mod policy;
pub mod state;
pub mod ttl_manager;
pub mod workflow_executor;

pub use activation_tracker::ActivationTracker;
pub use category_policies::{CategoryPolicy, CategoryPolicyManager};
pub use k_reduction_coordinator::LifecycleKReductionCoordinator;
pub use loader::{AdapterHandle, AdapterLoader};
pub use policy::{EvictionOrder, LifecyclePolicy};
pub use state::{AdapterState, AdapterStateRecord, AllocationTier, EvictionPriority};
pub use ttl_manager::{EvictionAuditEntry, TtlManager, TtlRecord};
pub use workflow_executor::{
    AdapterExecutionBackend, AdapterExecutionResult, ExecutionStats, KernelAdapterBackend,
    MockAdapterBackend, RealBackendAdapterBackend, WorkflowContext, WorkflowExecutor,
    WorkflowResult, WorkflowType,
};

// Re-export MemoryPressureLevel from adapteros-memory for public API
pub use adapteros_memory::MemoryPressureLevel;

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
    db: Option<ProtectedDb>,
    /// Rolling activation tracker fed by router decisions
    activation_tracker: Arc<RwLock<ActivationTracker>>,
    /// Currently active stack (if any)
    active_stack: Arc<RwLock<Option<(String, Vec<String>)>>>, // (name, adapter_ids)
    /// K reduction coordinator for memory-lifecycle coordination
    k_reduction_coordinator: Arc<LifecycleKReductionCoordinator>,
    /// Channel receiver for K reduction requests from memory manager
    k_reduction_rx: Arc<
        parking_lot::Mutex<
            Option<tokio::sync::mpsc::UnboundedReceiver<adapteros_memory::KReductionRequest>>,
        >,
    >,
    /// K reduction decision history for audit trail
    k_reduction_history: Arc<parking_lot::RwLock<Vec<KReductionExecutionRecord>>>,
    /// Model acquisition states for tracking downloads
    acquisition_states: Arc<RwLock<HashMap<String, AcquisitionState>>>,
    /// Download progress event broadcaster
    download_progress_tx: Option<broadcast::Sender<DownloadProgress>>,
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

        // Create download progress channel with capacity for 100 events
        let (download_progress_tx, _) = broadcast::channel(100);

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
            k_reduction_coordinator: Arc::new(LifecycleKReductionCoordinator::new(
                initial_k, 2, 0.70,
            )),
            k_reduction_rx: Arc::new(parking_lot::Mutex::new(None)),
            k_reduction_history: Arc::new(parking_lot::RwLock::new(Vec::new())),
            acquisition_states: Arc::new(RwLock::new(HashMap::new())),
            download_progress_tx: Some(download_progress_tx),
        }
    }

    /// Bind expected base model identity for loader-level validation.
    pub fn set_expected_base_model(&self, model_id: &str, model_hash: B3Hash) {
        let mut loader = self.loader.write();
        loader.set_expected_base_model(model_id.to_string(), Some(model_hash));
    }

    /// Set database for persistence
    pub fn set_db(&mut self, db: ProtectedDb) {
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
        db: ProtectedDb,
    ) -> Self {
        let mut states = HashMap::new();
        for (idx, name) in adapter_names.iter().enumerate() {
            states.insert(
                idx as u16,
                AdapterStateRecord::new(name.clone(), idx as u16),
            );
        }

        // Create download progress channel with capacity for 100 events
        let (download_progress_tx, _) = broadcast::channel(100);

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
            k_reduction_coordinator: Arc::new(LifecycleKReductionCoordinator::new(
                initial_k, 2, 0.70,
            )),
            k_reduction_rx: Arc::new(parking_lot::Mutex::new(None)),
            k_reduction_history: Arc::new(parking_lot::RwLock::new(Vec::new())),
            acquisition_states: Arc::new(RwLock::new(HashMap::new())),
            download_progress_tx: Some(download_progress_tx),
        }
    }

    /// Get the K reduction coordinator
    pub fn get_k_reduction_coordinator(&self) -> Arc<LifecycleKReductionCoordinator> {
        Arc::clone(&self.k_reduction_coordinator)
    }

    /// Register a newly imported adapter with the lifecycle manager
    ///
    /// This method is called after an adapter is imported via the API to:
    /// 1. Register the adapter's expected hash with the loader
    /// 2. Add it to the states map so it can be managed
    /// 3. Optionally load it immediately
    ///
    /// # Arguments
    /// * `adapter_id` - String identifier for the adapter
    /// * `hash` - BLAKE3 hash of the adapter weights
    /// * `category` - Optional category (defaults to "code")
    /// * `load_immediately` - If true, promotes adapter to Cold state and loads it
    ///
    /// # Returns
    /// The adapter index assigned to this adapter
    pub fn register_adapter(
        &mut self,
        adapter_id: String,
        hash: B3Hash,
        category: Option<String>,
        load_immediately: bool,
    ) -> Result<u16> {
        // 1. Determine next available adapter index
        let next_idx = {
            let states = self.states.read();
            states.keys().max().map(|k| k + 1).unwrap_or(0)
        };

        // 2. Register hash with loader
        {
            let mut loader = self.loader.write();
            loader.register_hash(adapter_id.clone(), hash);
        }

        // 3. Create state record and add to states map
        {
            let mut states = self.states.write();
            let mut record = AdapterStateRecord::new(adapter_id.clone(), next_idx);
            record.category = category.unwrap_or_else(|| "code".to_string());
            states.insert(next_idx, record);
        }

        // 4. If load_immediately, promote and load the adapter
        if load_immediately {
            // Use synchronous record.promote() for immediate loading (no DB persistence in registration)
            // Full promote_adapter() with DB persistence happens later during lifecycle management
            {
                let mut states = self.states.write();
                if let Some(record) = states.get_mut(&next_idx) {
                    if !record.promote() {
                        warn!("Failed to promote adapter {} immediately", adapter_id);
                    }
                }
            }
            // Try to load via get_or_reload
            if let Err(e) = self.get_or_reload(&adapter_id) {
                warn!("Failed to load adapter {} immediately: {}", adapter_id, e);
                // Don't fail the registration, just warn
            }
        }

        info!(
            adapter_id = %adapter_id,
            adapter_idx = next_idx,
            load_immediately = load_immediately,
            "Registered new adapter with lifecycle manager"
        );

        Ok(next_idx)
    }

    /// Wire K reduction event receiver from memory manager
    ///
    /// This establishes the integration point with the memory manager's event bus.
    /// The memory manager sends K reduction requests through this channel when
    /// memory pressure exceeds thresholds.
    pub fn wire_k_reduction_channel(
        &self,
        rx: tokio::sync::mpsc::UnboundedReceiver<adapteros_memory::KReductionRequest>,
    ) {
        let mut channel = self.k_reduction_rx.lock();
        *channel = Some(rx);
        info!("Wired K reduction event channel to lifecycle manager");
    }

    /// Poll for K reduction requests and process them
    ///
    /// This should be called in a background loop to process incoming K reduction
    /// requests from the memory manager. Returns the number of requests processed.
    ///
    /// ## Locking Strategy:
    /// - Extracts all pending requests from the channel while holding the lock briefly
    /// - Releases the lock immediately before processing (which involves async operations)
    /// - This prevents deadlocks from holding a parking_lot::Mutex across await points
    /// - The unbounded channel ensures we won't lose requests between polls
    pub async fn poll_k_reduction_events(&self) -> Result<usize> {
        // Step 1: Collect all pending requests while holding the lock briefly
        // We use a local vector to avoid holding the lock during async processing
        let pending_requests = {
            let mut rx_guard = self.k_reduction_rx.lock();

            // Extract the channel reference (if it exists)
            let rx_channel = match rx_guard.as_mut() {
                Some(channel) => channel,
                None => return Ok(0), // Channel not wired yet
            };

            // Drain all pending requests using try_recv (non-blocking)
            let mut requests = Vec::new();
            while let Ok(request) = rx_channel.try_recv() {
                requests.push(request);
            }

            requests
            // Lock is dropped here automatically when rx_guard goes out of scope
        };

        // Step 2: Process all requests without holding any locks
        // This is safe because we've extracted the requests and dropped the lock
        let mut processed_count = 0;

        for request in pending_requests {
            processed_count += 1;

            // Evaluate the K reduction request
            let states_snapshot = {
                let states = self.states.read();
                states.clone()
            };

            let response = self
                .k_reduction_coordinator
                .evaluate_request(&request, &states_snapshot);

            // Log evaluation
            info!(
                request_id = %request.request_id,
                approved = response.approved,
                target_k = response.new_k,
                adapters_to_unload = response.adapters_to_unload.len(),
                "Evaluated K reduction request"
            );

            // If approved, execute the unload
            if response.approved {
                let execution_result = self.execute_k_reduction(&request, &response).await;

                // Record decision with execution status
                let executed = execution_result.is_ok();
                let failure_reason = execution_result.as_ref().err().map(|e| e.to_string());

                let mut history = self.k_reduction_history.write();
                history.push(KReductionExecutionRecord {
                    request_id: request.request_id.clone(),
                    old_k: request.current_k,
                    new_k: response.new_k,
                    approved: true,
                    executed,
                    adapters_unloaded: response.adapters_to_unload.clone(),
                    failure_reason,
                    timestamp: std::time::SystemTime::now(),
                });

                if let Err(e) = execution_result {
                    warn!(
                        request_id = %request.request_id,
                        error = %e,
                        "Failed to execute K reduction"
                    );
                }
            } else {
                // Record rejection
                let mut history = self.k_reduction_history.write();
                history.push(KReductionExecutionRecord {
                    request_id: request.request_id.clone(),
                    old_k: request.current_k,
                    new_k: request.current_k, // No change on rejection
                    approved: false,
                    executed: false,
                    adapters_unloaded: vec![],
                    failure_reason: Some(response.reason.clone()),
                    timestamp: std::time::SystemTime::now(),
                });

                warn!(
                    request_id = %request.request_id,
                    reason = %response.reason,
                    "K reduction request rejected"
                );
            }
        }

        Ok(processed_count)
    }

    /// Execute K reduction by unloading adapters with rollback capability
    ///
    /// FIX 5: K reduction rollback incomplete - Implement full rollback on failure
    /// This method unloads the specified adapters and updates the K value.
    /// If any unload fails, it performs FULL rollback: restore K value and reload adapters.
    async fn execute_k_reduction(
        &self,
        request: &adapteros_memory::KReductionRequest,
        response: &adapteros_memory::KReductionResponse,
    ) -> Result<()> {
        let mut successfully_unloaded = Vec::new();
        let old_k = *self.current_k.read(); // FIX 5: Save old K value for rollback

        // Step 1: Unload adapters in order
        for adapter_idx in &response.adapters_to_unload {
            match self.evict_adapter(*adapter_idx).await {
                Ok(()) => {
                    successfully_unloaded.push(*adapter_idx);
                    info!(
                        request_id = %request.request_id,
                        adapter_idx = adapter_idx,
                        "Successfully unloaded adapter during K reduction"
                    );
                }
                Err(e) => {
                    warn!(
                        request_id = %request.request_id,
                        adapter_idx = adapter_idx,
                        error = %e,
                        "Failed to unload adapter during K reduction, initiating FULL rollback"
                    );

                    // FIX 5: FULL rollback - reload adapters AND restore K value
                    self.rollback_k_reduction(
                        &successfully_unloaded,
                        old_k,
                        request.request_id.as_str(),
                    )
                    .await;

                    return Err(e);
                }
            }
        }

        // Step 2: Update K value (only if all unloads succeeded)
        {
            let mut k = self.current_k.write();
            let old_k = *k;
            *k = response.new_k;

            info!(
                request_id = %request.request_id,
                old_k = old_k,
                new_k = *k,
                "Updated K value following successful K reduction"
            );

            // Emit telemetry
            if let Some(ref telemetry) = self.telemetry {
                let _ = telemetry.log(
                    "k_reduction_executed",
                    serde_json::json!({
                        "request_id": request.request_id,
                        "old_k": old_k,
                        "new_k": *k,
                        "adapters_unloaded": successfully_unloaded.len(),
                        "pressure_level": request.pressure_level,
                        "memory_freed": response.estimated_freed,
                    }),
                );
            }
        }

        Ok(())
    }

    /// Rollback K reduction by attempting to reload unloaded adapters
    ///
    /// FIX 5: Full rollback implementation - restore K value AND reload adapters
    /// Called if adapter unload fails during K reduction to restore previous state.
    /// This is a best-effort operation; if reload also fails, we accept the partial state.
    async fn rollback_k_reduction(
        &self,
        unloaded_adapters: &[u16],
        old_k: usize,
        request_id: &str,
    ) {
        warn!(
            request_id = request_id,
            unloaded_count = unloaded_adapters.len(),
            old_k = old_k,
            "Initiating FULL rollback for K reduction (restore K + reload adapters)"
        );

        // FIX 5: Step 1 - Restore K value FIRST
        {
            let mut k = self.current_k.write();
            let attempted_k = *k;
            *k = old_k;
            info!(
                request_id = request_id,
                old_k = old_k,
                attempted_k = attempted_k,
                "Restored K value during rollback"
            );
        }

        let mut successfully_reloaded = Vec::new();

        // FIX 5: Step 2 - Attempt to reload each unloaded adapter in reverse order
        for adapter_idx in unloaded_adapters.iter().rev() {
            let adapter_id_str = {
                let states = self.states.read();
                states.get(adapter_idx).map(|r| r.adapter_id.clone())
            };

            if let Some(adapter_id) = adapter_id_str {
                match self.promote_adapter(*adapter_idx).await {
                    Ok(()) => {
                        successfully_reloaded.push(*adapter_idx);
                        info!(
                            request_id = request_id,
                            adapter_idx = adapter_idx,
                            adapter_id = %adapter_id,
                            "Successfully reloaded adapter during rollback"
                        );
                    }
                    Err(e) => {
                        warn!(
                            request_id = request_id,
                            adapter_idx = adapter_idx,
                            adapter_id = %adapter_id,
                            error = %e,
                            "Failed to reload adapter during rollback - accepting partial state"
                        );
                    }
                }
            }
        }

        // Emit rollback telemetry with K value restoration
        if let Some(ref telemetry) = self.telemetry {
            let _ = telemetry.log(
                "k_reduction_rollback",
                serde_json::json!({
                    "request_id": request_id,
                    "attempted_rollback": unloaded_adapters.len(),
                    "successfully_reloaded": successfully_reloaded.len(),
                    "k_restored": old_k,
                    "timestamp": std::time::SystemTime::now(),
                }),
            );
        }

        warn!(
            request_id = request_id,
            successfully_reloaded = successfully_reloaded.len(),
            failed_to_reload = unloaded_adapters.len() - successfully_reloaded.len(),
            k_restored = old_k,
            "Completed FULL K reduction rollback (K restored, partial adapter state accepted if reload failed)"
        );
    }

    /// Get K reduction execution history
    pub fn get_k_reduction_history(&self) -> Vec<KReductionExecutionRecord> {
        self.k_reduction_history.read().clone()
    }

    /// Clear K reduction execution history
    pub fn clear_k_reduction_history(&self) {
        self.k_reduction_history.write().clear();
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
            warn!(
                "Found {} orphaned adapters stuck in loading state",
                stale_adapters.len()
            );

            for (adapter_id, name, load_state) in stale_adapters {
                recovery_actions.push(format!(
                    "Adapter {} ({}) stuck in state '{}' - marking as unloaded",
                    name, adapter_id, load_state
                ));

                // Mark as unloaded in database
                sqlx::query(
                    "UPDATE adapters SET load_state = 'cold', current_state = 'unloaded', updated_at = datetime('now') WHERE adapter_id = ?",
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
        .map_err(|e| {
            AosError::Database(format!("Failed to query invalid activation_pct: {}", e))
        })?;

        if reset_count > 0 {
            warn!(
                "Found {} adapters with invalid activation_pct - resetting",
                reset_count
            );

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
            info!(
                "✓ Crash recovery complete - {} actions taken:",
                recovery_actions.len()
            );
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

    async fn ensure_sql_kv_ready(&self, adapter_id: &str) -> Result<()> {
        if let Some(ref db) = self.db {
            let status = db.check_adapter_consistency(adapter_id).await?;
            if !status.is_ready() {
                let reason = status
                    .message
                    .unwrap_or_else(|| "KV consistency check failed".to_string());
                return Err(AosError::Validation(format!(
                    "Adapter {} blocked: {}",
                    adapter_id, reason
                )));
            }
        }
        Ok(())
    }

    /// Pin adapter to resident state
    ///
    /// FIX 7: Pin+demote atomic operation - Make pin state change and database update atomic
    /// Persists pin to database via `pinned_adapters` table.
    /// Pinned adapters will not be evicted by TTL or memory pressure.
    pub async fn pin_adapter(
        &self,
        adapter_id: u16,
        tenant_id: &str,
        pinned_by: &str,
        pinned_until: Option<String>,
        reason: Option<String>,
    ) -> Result<()> {
        // FIX 7: Step 1 - Persist pin to database FIRST, before changing in-memory state
        // This ensures if DB write fails, we don't have inconsistent state
        let adapter_id_str = {
            let states = self.states.read();
            if let Some(record) = states.get(&adapter_id) {
                record.adapter_id.clone()
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter {} not found",
                    adapter_id
                )));
            }
        };

        self.ensure_sql_kv_ready(&adapter_id_str).await?;

        if let Some(ref db) = self.db {
            // Use tenant_id:adapter_id as stable pin ID
            let pin_id = format!("{}:{}", tenant_id, adapter_id_str);
            let pinned_until_sql = pinned_until.as_deref();
            let reason_sql = reason.as_deref();

            // FIX 7: Database write happens BEFORE state change
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

        // FIX 7: Step 2 - Update in-memory state AFTER successful database write
        // This ensures atomic operation: DB is source of truth, memory follows
        let (adapter_id_str, old_state, memory_bytes) = {
            let mut states = self.states.write();

            if let Some(record) = states.get_mut(&adapter_id) {
                let old_state = record.state;
                let memory_bytes = record.memory_bytes;
                record.pin();
                (record.adapter_id.clone(), old_state, memory_bytes)
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter {} not found",
                    adapter_id
                )));
            }
        };

        // Structured log for adapter state transition (PRD-INFRA-01)
        info!(
            adapter_id = %adapter_id_str,
            from_state = %old_state,
            to_state = "resident",
            reason = "manual_pin",
            memory_bytes = memory_bytes,
            event_type = "adapter_state_transition",
            "Adapter state transition: pinned to resident (after DB persistence)"
        );

        if let Some(ref telemetry) = self.telemetry {
            telemetry.log(
                "adapter_promoted",
                AdapterTransitionEvent {
                    adapter_id: adapter_id_str.clone(),
                    from_state: old_state.to_string(),
                    to_state: AdapterState::Resident.to_string(),
                    reason: "manual_pin".to_string(),
                },
            )?;
        }

        // Log behavior event for training data
        if let Some(ref db) = self.db {
            let _ = db
                .insert_behavior_event(
                    "pinned",
                    &adapter_id_str,
                    tenant_id,
                    &old_state.to_string(),
                    "resident",
                    old_state.priority_boost(),
                    memory_bytes as u64,
                    "manual_pin",
                    None,
                )
                .await;
        }

        Ok(())
    }

    /// Unpin adapter
    ///
    /// FIX 7: Pin+demote atomic operation - Remove pin from database FIRST, then update memory
    /// Removes pin from database. Adapter becomes eligible for eviction again.
    pub async fn unpin_adapter(&self, adapter_id: u16, tenant_id: &str) -> Result<()> {
        // FIX 7: Step 1 - Get adapter ID and remove pin from database FIRST
        let adapter_id_str = {
            let states = self.states.read();
            if let Some(record) = states.get(&adapter_id) {
                record.adapter_id.clone()
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter {} not found",
                    adapter_id
                )));
            }
        };

        self.ensure_sql_kv_ready(&adapter_id_str).await?;

        // Remove pin from database (single source of truth)
        if let Some(ref db) = self.db {
            sqlx::query("DELETE FROM pinned_adapters WHERE tenant_id = ? AND adapter_id = ?")
                .bind(tenant_id)
                .bind(&adapter_id_str)
                .execute(db.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to remove adapter pin: {}", e)))?;

            info!("✓ Removed pin for adapter {} from database", adapter_id_str);
        }

        // FIX 7: Step 2 - Update in-memory state AFTER successful database write
        {
            let mut states = self.states.write();

            if let Some(record) = states.get_mut(&adapter_id) {
                record.unpin();
                info!("Unpinned adapter {} (after DB removal)", record.adapter_id);
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter {} not found",
                    adapter_id
                )));
            }
        }

        Ok(())
    }

    /// Manually promote an adapter
    ///
    /// ATOMICITY: Persists state to database FIRST, then updates in-memory state.
    /// This follows the same pattern as pin_adapter() to ensure DB is source of truth.
    pub async fn promote_adapter(&self, adapter_id: u16) -> Result<()> {
        // Step 1: Extract data needed for DB update and validation (read lock only)
        let (adapter_id_str, old_state, new_state) = {
            let states = self.states.read();

            if let Some(record) = states.get(&adapter_id) {
                let old_state = record.state;

                // Check if promotion is possible (same logic as record.promote())
                if !old_state.can_promote(&record.category) {
                    return Err(AosError::Lifecycle(format!(
                        "Cannot promote adapter {} from {} (category: {})",
                        record.adapter_id, old_state, record.category
                    )));
                }

                let new_state = match old_state.promote() {
                    Some(s) => s,
                    None => {
                        return Err(AosError::Lifecycle(format!(
                            "Cannot promote adapter {} from {}",
                            record.adapter_id, old_state
                        )));
                    }
                };

                (record.adapter_id.clone(), old_state, new_state)
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter {} not found",
                    adapter_id
                )));
            }
        };

        self.ensure_sql_kv_ready(&adapter_id_str).await?;

        // Step 2: Persist to database FIRST (before changing in-memory state)
        if let Some(ref db) = self.db {
            let db = db.write(db.lifecycle_token());
            db.update_adapter_state_tx(&adapter_id_str, &new_state.to_string(), "manual_promotion")
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to persist adapter promotion: {}", e))
                })?;

            info!(
                "✓ Persisted promotion for adapter {} to database",
                adapter_id_str
            );
        }

        // Step 3: Update in-memory state AFTER successful database write
        let memory_bytes = {
            let mut states = self.states.write();

            if let Some(record) = states.get_mut(&adapter_id) {
                // Directly set the new state (validation already done above)
                record.state = new_state;
                let memory_bytes = record.memory_bytes;

                // Structured log for adapter state transition (PRD-INFRA-01)
                info!(
                    adapter_id = %record.adapter_id,
                    from_state = %old_state,
                    to_state = %record.state,
                    reason = "manual_promotion",
                    memory_bytes = memory_bytes,
                    event_type = "adapter_state_transition",
                    "Adapter state transition: promoted"
                );

                memory_bytes
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter {} not found",
                    adapter_id
                )));
            }
        };

        // Step 4: Log telemetry (non-critical, after state change)
        if let Some(ref telemetry) = self.telemetry {
            telemetry.log(
                "adapter_promoted",
                AdapterTransitionEvent {
                    adapter_id: adapter_id_str.clone(),
                    from_state: old_state.to_string(),
                    to_state: new_state.to_string(),
                    reason: "manual".to_string(),
                },
            )?;
        }

        Ok(())
    }

    /// Manually demote an adapter
    ///
    /// ATOMICITY: Persists state to database FIRST, then updates in-memory state.
    /// This follows the same pattern as pin_adapter() to ensure DB is source of truth.
    pub async fn demote_adapter(&self, adapter_id: u16) -> Result<()> {
        // Step 1: Extract data needed for DB update and validation (read lock only)
        let (adapter_id_str, old_state, new_state) = {
            let states = self.states.read();

            if let Some(record) = states.get(&adapter_id) {
                let old_state = record.state;

                // Check if demotion is possible (same validation as record.demote())
                if record.pinned {
                    return Err(AosError::Lifecycle(format!(
                        "Cannot demote pinned adapter {}",
                        record.adapter_id
                    )));
                }

                // Check if we should demote based on last activation time
                if let Some(last_activated) = record.last_activated {
                    let time_since_activation = last_activated
                        .elapsed()
                        .unwrap_or(std::time::Duration::from_secs(0));
                    if !old_state.should_demote(&record.category, time_since_activation) {
                        return Err(AosError::Lifecycle(format!(
                            "Cannot demote adapter {} from {} (not enough time since last activation)",
                            record.adapter_id, old_state
                        )));
                    }
                }

                let new_state = match old_state.demote() {
                    Some(s) => s,
                    None => {
                        return Err(AosError::Lifecycle(format!(
                            "Cannot demote adapter {} from {}",
                            record.adapter_id, old_state
                        )));
                    }
                };

                (record.adapter_id.clone(), old_state, new_state)
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter {} not found",
                    adapter_id
                )));
            }
        };

        self.ensure_sql_kv_ready(&adapter_id_str).await?;

        // Step 2: Persist to database FIRST (before changing in-memory state)
        if let Some(ref db) = self.db {
            let db = db.write(db.lifecycle_token());
            db.update_adapter_state_tx(&adapter_id_str, &new_state.to_string(), "manual_demotion")
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to persist adapter demotion: {}", e))
                })?;

            info!(
                "✓ Persisted demotion for adapter {} to database",
                adapter_id_str
            );
        }

        // Step 3: Update in-memory state AFTER successful database write
        let memory_bytes = {
            let mut states = self.states.write();

            if let Some(record) = states.get_mut(&adapter_id) {
                // Directly set the new state (validation already done above)
                record.state = new_state;
                let memory_bytes = record.memory_bytes;

                // Structured log for adapter state transition (PRD-INFRA-01)
                info!(
                    adapter_id = %record.adapter_id,
                    from_state = %old_state,
                    to_state = %record.state,
                    reason = "manual_demotion",
                    memory_bytes = memory_bytes,
                    event_type = "adapter_state_transition",
                    "Adapter state transition: demoted"
                );

                memory_bytes
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter {} not found",
                    adapter_id
                )));
            }
        };

        // Step 4: Log telemetry (non-critical, after state change)
        if let Some(ref telemetry) = self.telemetry {
            telemetry.log(
                "adapter_demoted",
                AdapterTransitionEvent {
                    adapter_id: adapter_id_str.clone(),
                    from_state: old_state.to_string(),
                    to_state: new_state.to_string(),
                    reason: "manual".to_string(),
                },
            )?;
        }

        Ok(())
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
                    let old_state = record.state;
                    let memory_freed = record.memory_bytes;
                    record.state = AdapterState::Unloaded;
                    // FIX 6: Reset memory_bytes = 0 after eviction (like evict_adapter does)
                    record.memory_bytes = 0;

                    // Structured log for adapter eviction (PRD-INFRA-01)
                    info!(
                        adapter_id = %record.adapter_id,
                        from_state = %old_state,
                        to_state = "unloaded",
                        reason = "memory_pressure",
                        memory_freed_bytes = memory_freed,
                        category = %record.category,
                        event_type = "adapter_eviction",
                        "Adapter evicted due to memory pressure"
                    );

                    if let Some(ref telemetry) = self.telemetry {
                        telemetry.log(
                            "adapter_evicted",
                            AdapterEvictionEvent {
                                adapter_id: record.adapter_id.clone(),
                                from_state: record.state.to_string(),
                                category: record.category.clone(),
                                memory_freed,
                            },
                        )?;
                    }

                    // Unload from memory
                    let mut loader = self.loader.write();
                    loader.unload_adapter(adapter_id)?;

                    // Note: DB behavior event logging skipped in sync context
                    // The telemetry log above captures the eviction event

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
        use std::time::Instant;

        // Generate correlation ID for this K-reduction operation
        let request_id = uuid::Uuid::new_v4().to_string();
        let start_time = Instant::now();
        let start_timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let mut k = self.current_k.write();
        let k_current = *k;

        // Phase 1: Request - emit K reduction request event
        if let Some(ref telemetry) = self.telemetry {
            let k_target = if k_current > 1 {
                k_current - 1
            } else {
                k_current
            };
            let is_valid = k_target < k_current && k_target >= 1;

            telemetry.log_k_reduction_request(
                adapteros_telemetry::events::KReductionRequestEvent {
                    timestamp_us: start_timestamp_us,
                    request_id: request_id.clone(),
                    k_current,
                    k_target,
                    pressure_level: 1.0, // Memory pressure triggered this
                    bytes_to_free: 0,    // Unknown at this point
                    headroom_pct: 0.0,   // Unknown at this point
                    reason: "memory_pressure".to_string(),
                    is_valid,
                },
            )?;
        }

        if *k > 1 {
            let old_k = *k;

            // Phase 2: Evaluation - emit evaluation event
            let eval_start = Instant::now();
            let approved = true; // We're proceeding with the reduction
            let eval_duration_us = eval_start.elapsed().as_micros() as u64;

            if let Some(ref telemetry) = self.telemetry {
                telemetry.log_k_reduction_evaluation(
                    adapteros_telemetry::events::KReductionEvaluationEvent {
                        timestamp_us: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_micros() as u64,
                        request_id: request_id.clone(),
                        evaluation_duration_us: eval_duration_us,
                        approved,
                        adapters_to_unload_count: 0, // No adapters unloaded in pure K reduction
                        estimated_freed: 0,
                        reason: "k_reduction_approved".to_string(),
                        lock_acquisition_time_us: 0, // Lock already acquired
                        timeout_occurred: false,
                    },
                )?;
            }

            // Phase 3: Execution - perform the actual K reduction
            let exec_start = Instant::now();
            *k -= 1;
            let exec_duration_us = exec_start.elapsed().as_micros() as u64;
            let new_k = *k;

            warn!(
                "Reduced K from {} to {} due to memory pressure",
                old_k, new_k
            );

            if let Some(ref telemetry) = self.telemetry {
                telemetry.log_k_reduction_execution(
                    adapteros_telemetry::events::KReductionExecutionEvent {
                        timestamp_us: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_micros() as u64,
                        request_id: request_id.clone(),
                        execution_duration_us: exec_duration_us,
                        success: true,
                        adapters_unloaded_count: 0,
                        actual_memory_freed: 0,
                        error: None,
                        k_final: new_k,
                        timeout_occurred: false,
                    },
                )?;
            }

            // Phase 4: Completion - emit final completion event
            let total_duration_us = start_time.elapsed().as_micros() as u64;

            if let Some(ref telemetry) = self.telemetry {
                telemetry.log_k_reduction_completion(
                    adapteros_telemetry::events::KReductionCompletionEvent {
                        timestamp_us: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_micros() as u64,
                        request_id: request_id.clone(),
                        total_duration_us,
                        success: true,
                        k_before: old_k,
                        k_after: new_k,
                        headroom_after_pct: 0.0, // Unknown at this point
                        prevented_hot_eviction: false,
                        deadlock_detected: false,
                        timeout_abort: false,
                    },
                )?;
            }

            Ok(())
        } else {
            // Failure case: K cannot be reduced below 1
            let total_duration_us = start_time.elapsed().as_micros() as u64;

            if let Some(ref telemetry) = self.telemetry {
                // Emit failure completion event
                telemetry.log_k_reduction_completion(
                    adapteros_telemetry::events::KReductionCompletionEvent {
                        timestamp_us: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_micros() as u64,
                        request_id: request_id.clone(),
                        total_duration_us,
                        success: false,
                        k_before: k_current,
                        k_after: k_current,
                        headroom_after_pct: 0.0,
                        prevented_hot_eviction: false,
                        deadlock_detected: false,
                        timeout_abort: false,
                    },
                )?;
            }

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

    /// Get adapter index by adapter ID
    pub fn get_adapter_idx(&self, adapter_id: &str) -> Option<u16> {
        let states = self.states.read();
        states
            .iter()
            .find(|(_, record)| record.adapter_id == adapter_id)
            .map(|(idx, _)| *idx)
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
        if let Some((mismatch_id, expected, actual, layer)) = match err {
            AosError::AdapterHashMismatch {
                adapter_id: mismatch_id,
                expected,
                actual,
            } => Some((mismatch_id, expected, actual, None)),
            AosError::AdapterLayerHashMismatch {
                adapter_id: mismatch_id,
                expected,
                actual,
                layer_id,
            } => Some((mismatch_id, expected, actual, Some(layer_id))),
            _ => None,
        } {
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
                        layer = ?layer,
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
    ///
    /// ATOMICITY: Persists state to database FIRST, then updates in-memory state.
    /// This follows the same pattern as pin_adapter() to ensure DB is source of truth.
    /// REMOVED: spawn_deterministic fire-and-forget pattern that could fail silently.
    pub async fn update_adapter_state(
        &self,
        adapter_id: u16,
        new_state: AdapterState,
        reason: &str,
    ) -> Result<()> {
        // Step 1: Extract adapter ID and old state (read lock only)
        let (adapter_id_str, old_state) = {
            let states = self.states.read();

            if let Some(record) = states.get(&adapter_id) {
                (record.adapter_id.clone(), record.state)
            } else {
                // Adapter not found - return early without error
                return Ok(());
            }
        };

        // Step 2: Persist to database FIRST (before changing in-memory state)
        if let Some(ref db) = self.db {
            let db = db.write(db.lifecycle_token());
            db.update_adapter_state_tx(&adapter_id_str, &new_state.to_string(), reason)
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to persist adapter state update: {}", e))
                })?;

            info!(
                "✓ Persisted state update for adapter {} to database",
                adapter_id_str
            );
        }

        // Step 3: Update in-memory state AFTER successful database write
        {
            let mut states = self.states.write();

            if let Some(record) = states.get_mut(&adapter_id) {
                record.state = new_state;
            } else {
                // Adapter was removed between read and write - this is OK
                return Ok(());
            }
        }

        // Step 4: Log telemetry and info (non-critical, after state change)
        if let Some(ref telemetry) = self.telemetry {
            telemetry.log(
                "adapter_state_transition",
                AdapterTransitionEvent {
                    adapter_id: adapter_id_str.clone(),
                    from_state: old_state.to_string(),
                    to_state: new_state.to_string(),
                    reason: reason.to_string(),
                },
            )?;
        }

        info!(
            "Updated adapter {} state: {} -> {} ({})",
            adapter_id_str, old_state, new_state, reason
        );

        Ok(())
    }

    /// Auto-promote adapter based on category policy
    pub async fn auto_promote_adapter(&self, adapter_id: u16) -> Result<()> {
        // Extract data while holding lock, then release before async operation
        let next_state_opt = {
            let states = self.states.read();

            states.get(&adapter_id).and_then(|record| {
                let category = &record.category;
                let current_state = record.state;

                if current_state.can_promote(category) {
                    current_state.promote()
                } else {
                    None
                }
            })
            // Lock is dropped here
        };

        // Perform async operation without holding any locks
        if let Some(next_state) = next_state_opt {
            self.update_adapter_state(adapter_id, next_state, "auto_promotion")
                .await?;
        }

        Ok(())
    }

    /// Auto-demote adapter based on category policy and inactivity
    pub async fn auto_demote_adapter(&self, adapter_id: u16) -> Result<()> {
        // Extract data while holding lock, then release before async operation
        let next_state_opt = {
            let states = self.states.read();

            states.get(&adapter_id).and_then(|record| {
                let category = &record.category;
                let current_state = record.state;

                // Check if we should demote based on last activation time
                record.last_activated.and_then(|last_activated| {
                    let time_since_activation = last_activated
                        .elapsed()
                        .unwrap_or(std::time::Duration::from_secs(0));

                    if current_state.should_demote(category, time_since_activation) {
                        current_state.demote()
                    } else {
                        None
                    }
                })
            })
            // Lock is dropped here
        };

        // Perform async operation without holding any locks
        if let Some(next_state) = next_state_opt {
            self.update_adapter_state(adapter_id, next_state, "auto_demotion")
                .await?;
        }

        Ok(())
    }

    /// Record adapter activation
    pub async fn record_adapter_activation(&self, adapter_id: u16) -> Result<()> {
        // Extract required data while holding lock, then release before async operations
        let (adapter_id_str, state, category, activation_count) = {
            let mut states = self.states.write();

            if let Some(record) = states.get_mut(&adapter_id) {
                record.record_activation();
                (
                    record.adapter_id.clone(),
                    record.state.to_string(),
                    record.category.clone(),
                    record.activation_count,
                )
            } else {
                return Ok(());
            }
        }; // LOCK RELEASED HERE

        // Async operations happen WITHOUT lock
        if let Some(ref db) = self.db {
            let db_clone = db.clone();
            let adapter_id_clone = adapter_id_str.clone();

            // Persist activation immediately (fallback when deterministic executor isn't running)
            if let Err(e) = db_clone
                .record_activation(&adapter_id_clone, None, 1.0, true)
                .await
            {
                warn!("Failed to record adapter activation in database: {}", e);
            }

            if let Err(e) = sqlx::query(
                "UPDATE adapters SET
                 activation_count = ?,
                 last_activated = datetime('now'),
                 updated_at = datetime('now')
                 WHERE adapter_id = ?",
            )
            .bind(activation_count as i64)
            .bind(&adapter_id_clone)
            .execute(db_clone.pool())
            .await
            {
                warn!(
                    "Failed to update adapter activation count in database: {}",
                    e
                );
            }
        }

        // Log activation (non-blocking)
        if let Some(ref telemetry) = self.telemetry {
            telemetry.log(
                "adapter_activated",
                AdapterActivationEvent {
                    adapter_id: adapter_id_str,
                    state,
                    category,
                    activation_count,
                },
            )?;
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
    pub async fn check_memory_pressure(
        &self,
        total_memory: usize,
        _pressure_level: MemoryPressureLevel,
    ) -> Result<()> {
        let memory_pressure = self.get_total_memory_usage() as f32 / total_memory as f32;

        // First, evict expired adapters
        if let Some(ref db) = self.db {
            if let Ok(expired_adapters) = db.find_expired_adapters().await {
                for expired in &expired_adapters {
                    if let Some(adapter_id) = self.get_adapter_id_by_name(&expired.name) {
                        self.evict_adapter(adapter_id).await?;
                    }
                }
            }
        }

        if memory_pressure > 0.95 {
            // critical
            // Evict Tier 0 if critical
            self.evict_by_tier(AllocationTier::Critical).await?;
        } else if memory_pressure > 0.85 {
            // high
            // Evict Tier 1
            self.evict_by_tier(AllocationTier::Extra).await?;
        }

        // Reduce K if still high
        if memory_pressure > 0.9 {
            self.reduce_k()?;
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
    ///
    /// FIX 1: Pinned adapter eviction race - Hold lock during entire pin check + eviction operation
    /// Don't release lock between checking pinned status and performing eviction.
    pub async fn evict_adapter(&self, adapter_id: u16) -> Result<()> {
        // FIX 1: Hold lock during ENTIRE operation to prevent race between pin check and eviction
        // Extract required data and perform state updates AND loader unload while holding lock
        let (adapter_id_str, old_state, category, memory_freed, tenant_id) = {
            let mut states = self.states.write();

            if let Some(record) = states.get_mut(&adapter_id) {
                // FIX 1: Check pinned status WHILE holding lock - no window for race
                if record.pinned {
                    return Err(AosError::Lifecycle(format!(
                        "Cannot evict pinned adapter: {}",
                        record.adapter_id
                    )));
                }

                let old_state = record.state;
                let memory_freed = record.memory_bytes;
                let adapter_id_str = record.adapter_id.clone();
                let category = record.category.clone();
                let tenant_id = record.scope.clone();

                // FIX 1: Unload from loader BEFORE changing state, while still holding states lock
                // This prevents race where adapter could be pinned after check but before unload
                {
                    let mut loader = self.loader.write();
                    if let Err(e) = loader.unload_adapter(adapter_id) {
                        // Allow eviction to continue when adapter was never loaded
                        if !e.to_string().contains("not loaded") {
                            return Err(e);
                        } else {
                            warn!(
                                adapter_id = %adapter_id_str,
                                "Adapter not loaded during eviction; continuing DB cleanup"
                            );
                        }
                    }
                } // LOADER LOCK RELEASED

                // FIX 2: Set state to Unloaded AFTER successful loader.unload()
                // If unload fails, state remains unchanged (error returns above)
                record.state = AdapterState::Unloaded;
                record.memory_bytes = 0;

                (adapter_id_str, old_state, category, memory_freed, tenant_id)
            } else {
                return Ok(());
            }
        }; // LOCK RELEASED HERE - but eviction is already complete

        // Async operations happen WITHOUT any locks
        if let Some(ref db) = self.db {
            let db_clone = db.clone();
            let adapter_id_clone = adapter_id_str.clone();

            // Persist eviction immediately (fallback when deterministic executor isn't running)
            if let Err(e) = db_clone
                .update_adapter_state(&tenant_id, &adapter_id_clone, "unloaded", "eviction")
                .await
            {
                warn!(
                    "Failed to update adapter state during eviction in database: {}",
                    e
                );
            }

            if let Err(e) = db_clone
                .update_adapter_memory(&tenant_id, &adapter_id_clone, 0)
                .await
            {
                warn!(
                    "Failed to update adapter memory during eviction in database: {}",
                    e
                );
            }
        }

        // Structured log for adapter eviction (PRD-INFRA-01)
        info!(
            adapter_id = %adapter_id_str,
            from_state = %old_state,
            to_state = "unloaded",
            reason = "lru_eviction",
            memory_freed_bytes = memory_freed,
            category = %category,
            event_type = "adapter_eviction",
            "Adapter evicted via LRU policy"
        );

        // Log eviction (non-blocking telemetry)
        if let Some(ref telemetry) = self.telemetry {
            telemetry.log(
                "adapter_evicted",
                AdapterEvictionEvent {
                    adapter_id: adapter_id_str.clone(),
                    from_state: old_state.to_string(),
                    category,
                    memory_freed,
                },
            )?;
        }

        // Log behavior event for training data
        if let Some(ref db) = self.db {
            let _ = db
                .insert_behavior_event(
                    "evicted",
                    &adapter_id_str,
                    "system", // Eviction is system-initiated
                    &old_state.to_string(),
                    "unloaded",
                    old_state.priority_boost(),
                    memory_freed as u64,
                    "lru_eviction",
                    None,
                )
                .await;
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
    ///
    /// DESIGN NOTE: This method updates in-memory routing state only (which stack is currently
    /// active for routing decisions). It does NOT persist to database because:
    /// 1. Active stack selection is ephemeral runtime state, not persistent configuration
    /// 2. The database tracks stack lifecycle_state (Draft/Active/Archived) separately via
    ///    Db::activate_stack() in stacks_kv.rs, which is about stack availability, not routing
    /// 3. The active_stack field tracks "which stack should the router use right now", which
    ///    is a worker-level decision that resets on restart
    ///
    /// If persistence of the currently-active stack for routing is needed in the future,
    /// it should be added to worker_manifest or a new worker_state table.
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

            self.ensure_sql_kv_ready(adapter_id).await?;

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

        // Update the active stack (in-memory only - see DESIGN NOTE above)
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
    /// ```ignore
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

            sqlx::query("UPDATE adapters SET last_heartbeat = ? WHERE id = ?")
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
                   AND load_state NOT IN ('unloaded', 'unloading')",
            )
            .bind(cutoff)
            .fetch_all(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to query stale adapters: {}", e)))?;

            let stale_ids: Vec<String> = stale.into_iter().map(|(id,)| id).collect();

            if !stale_ids.is_empty() {
                tracing::warn!(
                    count = stale_ids.len(),
                    threshold_seconds,
                    "Detected stale adapters"
                );
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

        // Emit telemetry event for stale detection
        if !stale_ids.is_empty() {
            if let Some(ref telemetry) = self.telemetry {
                let event = serde_json::json!({
                    "event_type": "heartbeat_stale_detected",
                    "stale_count": stale_ids.len(),
                    "threshold_seconds": threshold_seconds,
                    "adapter_ids": stale_ids,
                    "timestamp": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                });
                if let Err(e) = telemetry.log("heartbeat_stale_detected", &event) {
                    tracing::error!("Failed to write stale detection telemetry: {}", e);
                }
            }
        }

        for adapter_id in stale_ids {
            // Reset state to unloaded for stale adapters
            if let Some(ref db) = self.db {
                sqlx::query(
                    "UPDATE adapters
                     SET load_state = 'unloading', last_heartbeat = NULL
                     WHERE id = ?",
                )
                .bind(&adapter_id)
                .execute(db.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to reset stale adapter: {}", e)))?;

                tracing::info!(adapter_id = %adapter_id, "Recovered stale adapter");

                // Emit telemetry event for each recovery
                if let Some(ref telemetry) = self.telemetry {
                    let event = serde_json::json!({
                        "event_type": "heartbeat_recovery",
                        "adapter_id": adapter_id,
                        "threshold_seconds": threshold_seconds,
                        "timestamp": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                    });
                    if let Err(e) = telemetry.log("heartbeat_recovery", &event) {
                        tracing::error!("Failed to write heartbeat recovery telemetry: {}", e);
                    }
                }

                recovered.push(adapter_id);
            }
        }

        Ok(recovered)
    }

    pub fn get_eviction_candidates(&self, tier: AllocationTier) -> Vec<String> {
        let states = self.states.read();
        states
            .iter()
            .filter_map(|(id, record)| {
                if AllocationTier::from(record.state) == tier && !record.pinned {
                    self.get_adapter_id_by_name(&record.adapter_id)
                        .map(|_| record.adapter_id.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub async fn evict_by_tier(&self, tier: AllocationTier) -> Result<()> {
        let candidates = self.get_eviction_candidates(tier);
        for name in candidates {
            if let Some(id) = self.get_adapter_id_by_name(&name) {
                let _ = self.evict_adapter(id).await;
            }
        }
        Ok(())
    }

    // ===== Model Hub Integration Methods =====

    fn resolve_local_model_path(&self, model_id: &str, repo_id: Option<&str>) -> Option<PathBuf> {
        let direct_path = Path::new(model_id);
        if direct_path.exists() {
            return Some(direct_path.to_path_buf());
        }

        let repo = repo_id.unwrap_or(model_id);
        let cache_root = std::env::var("AOS_MODEL_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/model-cache"));

        let candidates = [
            cache_root.join("models").join(repo),
            cache_root.join(repo),
            PathBuf::from("var/models").join(repo),
        ];

        for candidate in candidates {
            if candidate.exists() {
                return Some(candidate);
            }
        }

        let loader = self.loader.read();
        let base_path = loader.adapters_base_path();
        let adapter_path = base_path.join(format!("{}.aos", model_id));
        if adapter_path.exists() {
            return Some(adapter_path);
        }

        None
    }

    fn build_model_hub_config(&self) -> ModelHubConfig {
        let cache_dir = std::env::var("AOS_MODEL_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/model-cache"));

        let max_concurrent_downloads = {
            let raw = std::env::var("AOS_MAX_CONCURRENT_DOWNLOADS").ok();
            let parsed = raw
                .as_deref()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(4);
            let clamped = parsed.clamp(1, 10);
            if clamped != parsed {
                warn!(
                    env = "AOS_MAX_CONCURRENT_DOWNLOADS",
                    raw = ?raw,
                    parsed,
                    clamped,
                    "Value out of bounds; clamping to safe range"
                );
            }
            clamped
        };

        let timeout_secs = {
            let raw = std::env::var("AOS_DOWNLOAD_TIMEOUT_SECS").ok();
            let parsed = raw
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300);
            let clamped = parsed.clamp(30, 3600);
            if clamped != parsed {
                warn!(
                    env = "AOS_DOWNLOAD_TIMEOUT_SECS",
                    raw = ?raw,
                    parsed,
                    clamped,
                    "Value out of bounds; clamping to safe range"
                );
            }
            clamped
        };

        ModelHubConfig {
            registry_url: std::env::var("AOS_HF_REGISTRY_URL")
                .unwrap_or_else(|_| "https://huggingface.co".to_string()),
            cache_dir,
            max_concurrent_downloads,
            timeout_secs,
            hf_token: std::env::var("HF_TOKEN").ok(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn persist_model_acquisition(
        &self,
        acquisition_id: &str,
        repo_id: &str,
        state: &AcquisitionState,
        progress_pct: Option<u8>,
        local_path: Option<&Path>,
        failure_reason: Option<&str>,
        mark_completed: bool,
    ) {
        let Some(ref db) = self.db else {
            return;
        };
        let Some(pool) = db.pool_opt() else {
            warn!("SQL pool unavailable; skipping model acquisition persistence");
            return;
        };

        let state_label = match state {
            AcquisitionState::NotAvailable => "not_cached",
            AcquisitionState::Downloading { .. } => "downloading",
            AcquisitionState::Verifying => "verifying",
            AcquisitionState::Available => "available",
            AcquisitionState::Failed { .. } => "failed",
        };

        let local_path_str = local_path.map(|p| p.to_string_lossy().to_string());
        let size_bytes = local_path.and_then(|p| std::fs::metadata(p).ok().map(|m| m.len() as i64));
        let completed_at = if mark_completed {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        };

        let result = sqlx::query(
            r#"
            INSERT INTO model_acquisitions (
                id,
                repo_id,
                revision,
                acquisition_state,
                download_progress_pct,
                local_path,
                size_bytes,
                failure_reason,
                download_started_at,
                download_completed_at,
                updated_at
            ) VALUES (?, ?, 'main', ?, ?, ?, ?, ?, datetime('now'), ?, datetime('now'))
            ON CONFLICT(id) DO UPDATE SET
                repo_id = excluded.repo_id,
                revision = excluded.revision,
                acquisition_state = excluded.acquisition_state,
                download_progress_pct = excluded.download_progress_pct,
                local_path = COALESCE(excluded.local_path, model_acquisitions.local_path),
                size_bytes = COALESCE(excluded.size_bytes, model_acquisitions.size_bytes),
                failure_reason = excluded.failure_reason,
                download_started_at = COALESCE(model_acquisitions.download_started_at, excluded.download_started_at),
                download_completed_at = CASE
                    WHEN excluded.download_completed_at IS NOT NULL THEN excluded.download_completed_at
                    ELSE model_acquisitions.download_completed_at
                END,
                updated_at = datetime('now')
            "#,
        )
        .bind(acquisition_id)
        .bind(repo_id)
        .bind(state_label)
        .bind(progress_pct.map(|v| v as i64))
        .bind(local_path_str)
        .bind(size_bytes)
        .bind(failure_reason)
        .bind(completed_at)
        .execute(pool)
        .await;

        if let Err(e) = result {
            let message = e.to_string();
            if message.contains("no such table") || message.contains("no such column") {
                warn!(
                    error = %e,
                    "Model acquisition persistence skipped (schema missing)"
                );
            } else {
                warn!(error = %e, "Failed to persist model acquisition state");
            }
        }
    }

    /// Ensure model is available (download if needed, then load)
    ///
    /// This method coordinates model acquisition from a remote hub:
    /// 1. Check if model exists locally
    /// 2. Download if needed (updating acquisition state)
    /// 3. Verify downloaded model
    /// 4. Load into lifecycle manager
    ///
    /// # Arguments
    /// * `model_id` - Unique identifier for the model/adapter
    /// * `repo_id` - Optional repository ID (e.g., "username/model-name" for HuggingFace)
    ///
    /// # Returns
    /// Path to the locally available model file
    pub async fn ensure_available(&self, model_id: &str, repo_id: Option<&str>) -> Result<PathBuf> {
        let repo = repo_id.unwrap_or(model_id);

        if let Some(local_path) = self.resolve_local_model_path(model_id, repo_id) {
            self.set_acquisition_state(model_id, AcquisitionState::Available);
            self.persist_model_acquisition(
                repo,
                repo,
                &AcquisitionState::Available,
                Some(100),
                Some(&local_path),
                None,
                true,
            )
            .await;
            return Ok(local_path);
        }

        if !env_truthy("AOS_HF_HUB_ENABLED") {
            let reason = format!(
                "Model '{}' not cached and hub downloads are disabled. Set AOS_HF_HUB_ENABLED=1 or provide a local path.",
                repo
            );
            self.set_acquisition_state(
                model_id,
                AcquisitionState::Failed {
                    reason: reason.clone(),
                },
            );
            self.persist_model_acquisition(
                repo,
                repo,
                &AcquisitionState::Failed {
                    reason: reason.clone(),
                },
                None,
                None,
                Some(&reason),
                false,
            )
            .await;
            return Err(AosError::Config(reason));
        }

        // Set acquisition state to downloading
        self.set_acquisition_state(model_id, AcquisitionState::Downloading { progress_pct: 0 });
        self.persist_model_acquisition(
            repo,
            repo,
            &AcquisitionState::Downloading { progress_pct: 0 },
            Some(0),
            None,
            None,
            false,
        )
        .await;

        info!(
            model_id = %model_id,
            repo_id = %repo,
            "Starting model download from hub"
        );

        self.update_download_progress(model_id, "downloading", 5, None, None);

        let client = ModelHubClient::new(self.build_model_hub_config()).map_err(|e| {
            AosError::Config(format!("Failed to initialize model hub client: {}", e))
        })?;

        let download_path = match client.download_model(repo).await {
            Ok(path) => path,
            Err(e) => {
                let reason = format!("Model download failed: {}", e);
                self.mark_acquisition_failed(model_id, &reason);
                self.persist_model_acquisition(
                    repo,
                    repo,
                    &AcquisitionState::Failed {
                        reason: reason.clone(),
                    },
                    None,
                    None,
                    Some(&reason),
                    false,
                )
                .await;
                return Err(AosError::Network(reason));
            }
        };

        self.update_download_progress(model_id, "downloading", 80, None, None);
        self.set_acquisition_state(model_id, AcquisitionState::Verifying);
        self.persist_model_acquisition(
            repo,
            repo,
            &AcquisitionState::Verifying,
            Some(90),
            Some(&download_path),
            None,
            false,
        )
        .await;

        let resolved_path = if download_path.exists() {
            download_path
        } else {
            let reason = format!(
                "Model path missing after download: {}",
                download_path.display()
            );
            self.mark_acquisition_failed(model_id, &reason);
            self.persist_model_acquisition(
                repo,
                repo,
                &AcquisitionState::Failed {
                    reason: reason.clone(),
                },
                None,
                None,
                Some(&reason),
                false,
            )
            .await;
            return Err(AosError::NotFound(reason));
        };

        self.mark_acquisition_complete(model_id, resolved_path.clone())?;
        self.persist_model_acquisition(
            repo,
            repo,
            &AcquisitionState::Available,
            Some(100),
            Some(&resolved_path),
            None,
            true,
        )
        .await;

        Ok(resolved_path)
    }

    /// Get acquisition state for a model
    ///
    /// Returns the current acquisition state (downloading, available, failed, etc.)
    pub fn get_acquisition_state(&self, model_id: &str) -> AcquisitionState {
        let states = self.acquisition_states.read();
        states
            .get(model_id)
            .cloned()
            .unwrap_or(AcquisitionState::NotAvailable)
    }

    /// Set acquisition state (called during download progress)
    ///
    /// Updates the acquisition state for a model and emits telemetry events
    pub fn set_acquisition_state(&self, model_id: &str, state: AcquisitionState) {
        {
            let mut states = self.acquisition_states.write();
            states.insert(model_id.to_string(), state.clone());
        }

        // Log state change
        info!(
            model_id = %model_id,
            state = ?state,
            event_type = "model_acquisition_state_change",
            "Model acquisition state changed"
        );

        // Emit telemetry
        if let Some(ref telemetry) = self.telemetry {
            let _ = telemetry.log(
                "model_acquisition_state_change",
                serde_json::json!({
                    "model_id": model_id,
                    "state": match &state {
                        AcquisitionState::NotAvailable => "not_available",
                        AcquisitionState::Downloading { .. } => "downloading",
                        AcquisitionState::Verifying => "verifying",
                        AcquisitionState::Available => "available",
                        AcquisitionState::Failed { .. } => "failed",
                    },
                    "details": state,
                }),
            );
        }

        // Emit download progress if in downloading state
        if let AcquisitionState::Downloading { progress_pct } = state {
            if let Some(ref tx) = self.download_progress_tx {
                let _ = tx.send(DownloadProgress {
                    model_id: model_id.to_string(),
                    phase: "downloading".to_string(),
                    progress_pct,
                    eta_seconds: None,
                    speed_mbps: None,
                });
            }
        }
    }

    /// Subscribe to download progress events
    ///
    /// Returns a broadcast receiver that will receive download progress updates
    /// for all models being acquired. Clients can filter by model_id.
    pub fn subscribe_progress(&self) -> Result<broadcast::Receiver<DownloadProgress>> {
        self.download_progress_tx
            .as_ref()
            .map(|tx| tx.subscribe())
            .ok_or_else(|| {
                AosError::Internal("Download progress channel not initialized".to_string())
            })
    }

    /// Check if model needs download
    ///
    /// Returns true if the model is not available locally and needs to be downloaded
    pub fn needs_download(&self, model_id: &str) -> bool {
        // Check acquisition state first
        let state = self.get_acquisition_state(model_id);
        match state {
            AcquisitionState::Available => false,
            AcquisitionState::Downloading { .. } => false, // Already downloading
            _ => self.resolve_local_model_path(model_id, None).is_none(),
        }
    }

    /// Update download progress with detailed metrics
    ///
    /// Called by external download implementations to report progress
    pub fn update_download_progress(
        &self,
        model_id: &str,
        phase: &str,
        progress_pct: u8,
        eta_seconds: Option<u64>,
        speed_mbps: Option<f64>,
    ) {
        // Update acquisition state if in downloading phase
        if phase == "downloading" {
            self.set_acquisition_state(model_id, AcquisitionState::Downloading { progress_pct });
        } else if phase == "verifying" {
            self.set_acquisition_state(model_id, AcquisitionState::Verifying);
        }

        // Emit progress event
        if let Some(ref tx) = self.download_progress_tx {
            let _ = tx.send(DownloadProgress {
                model_id: model_id.to_string(),
                phase: phase.to_string(),
                progress_pct,
                eta_seconds,
                speed_mbps,
            });
        }

        info!(
            model_id = %model_id,
            phase = %phase,
            progress_pct = progress_pct,
            eta_seconds = ?eta_seconds,
            speed_mbps = ?speed_mbps,
            "Download progress update"
        );
    }

    /// Mark model acquisition as complete
    ///
    /// Called after successful download and verification
    pub fn mark_acquisition_complete(&self, model_id: &str, local_path: PathBuf) -> Result<()> {
        // Verify file exists
        if !local_path.exists() {
            return Err(AosError::NotFound(format!(
                "Model file not found after download: {}",
                local_path.display()
            )));
        }

        // Update state to available
        self.set_acquisition_state(model_id, AcquisitionState::Available);

        // Emit completion event
        if let Some(ref tx) = self.download_progress_tx {
            let _ = tx.send(DownloadProgress {
                model_id: model_id.to_string(),
                phase: "complete".to_string(),
                progress_pct: 100,
                eta_seconds: Some(0),
                speed_mbps: None,
            });
        }

        info!(
            model_id = %model_id,
            path = %local_path.display(),
            "Model acquisition complete"
        );

        Ok(())
    }

    /// Mark model acquisition as failed
    ///
    /// Called when download or verification fails
    pub fn mark_acquisition_failed(&self, model_id: &str, reason: &str) {
        self.set_acquisition_state(
            model_id,
            AcquisitionState::Failed {
                reason: reason.to_string(),
            },
        );

        warn!(
            model_id = %model_id,
            reason = %reason,
            "Model acquisition failed"
        );
    }

    /// Get all models with their acquisition states
    ///
    /// Returns a map of model_id -> acquisition state for all tracked models
    pub fn get_all_acquisition_states(&self) -> HashMap<String, AcquisitionState> {
        self.acquisition_states.read().clone()
    }

    /// Clear acquisition state for a model
    ///
    /// Useful for retrying failed downloads
    pub fn clear_acquisition_state(&self, model_id: &str) {
        let mut states = self.acquisition_states.write();
        states.remove(model_id);
        info!(model_id = %model_id, "Cleared acquisition state");
    }
}

/// GPU integrity verification report
///
/// Returned by external verification code to indicate which adapters passed/failed
/// GPU buffer integrity checks.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

#[derive(Debug, Clone)]
pub struct BehaviorEvent {
    pub event_type: String,
    pub adapter_id: String,
    pub tenant_id: String,
    pub from_state: String,
    pub to_state: String,
    pub activation_pct: f32,
    pub memory_mb: u64,
    pub reason: String,
    pub metadata: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::B3Hash;
    use adapteros_model_hub::manifest::Policies;
    use std::collections::HashMap;
    use tempfile::{Builder as TempDirBuilder, TempDir};

    fn new_test_tempdir(prefix: &str) -> TempDir {
        let root = std::path::PathBuf::from("var/tmp");
        let _ = std::fs::create_dir_all(&root);
        TempDirBuilder::new()
            .prefix(prefix)
            .tempdir_in(&root)
            .expect("Test temp directory creation should succeed")
    }

    fn test_policies() -> Policies {
        Policies::default()
    }

    fn build_adapter_hashes(names: &[String]) -> HashMap<String, B3Hash> {
        names
            .iter()
            .map(|name| (name.clone(), B3Hash::hash(name.as_bytes())))
            .collect()
    }

    #[tokio::test]
    async fn test_lifecycle_basic() {
        let adapter_names = vec!["adapter_0".to_string(), "adapter_1".to_string()];
        let temp_dir = new_test_tempdir("mplora_test_lifecycle_");
        let temp_dir_path = temp_dir.path().to_path_buf();

        let adapter_hashes = build_adapter_hashes(&adapter_names);
        let manager = LifecycleManager::new(
            adapter_names.clone(),
            adapter_hashes,
            &test_policies(),
            temp_dir_path.clone(),
            None,
            3,
        );

        // Initial state should be unloaded
        assert_eq!(manager.get_state(0), Some(AdapterState::Unloaded));

        // Promote adapter
        manager
            .promote_adapter(0)
            .await
            .expect("Test adapter promotion should succeed");
        assert_eq!(manager.get_state(0), Some(AdapterState::Cold));

        // Demote adapter
        manager
            .demote_adapter(0)
            .await
            .expect("Test adapter demotion should succeed");
        assert_eq!(manager.get_state(0), Some(AdapterState::Unloaded));
    }

    #[tokio::test]
    async fn test_pinning() {
        let adapter_names = vec!["adapter_0".to_string()];
        let temp_dir = new_test_tempdir("mplora_test_pinning_");
        let temp_dir_path = temp_dir.path().to_path_buf();

        let adapter_hashes = build_adapter_hashes(&adapter_names);
        let manager = LifecycleManager::new(
            adapter_names.clone(),
            adapter_hashes,
            &test_policies(),
            temp_dir_path.clone(),
            None,
            3,
        );

        // Pin adapter
        manager
            .pin_adapter(0, "test_tenant", "test_user", None, None)
            .await
            .expect("Test adapter pinning should succeed");
        assert_eq!(manager.get_state(0), Some(AdapterState::Resident));

        // Cannot demote pinned adapter
        assert!(manager.demote_adapter(0).await.is_err());
        assert_eq!(manager.get_state(0), Some(AdapterState::Resident));

        // Unpin and then demote
        manager
            .unpin_adapter(0, "test_tenant")
            .await
            .expect("Test adapter unpinning should succeed");
        manager
            .demote_adapter(0)
            .await
            .expect("Test adapter demotion should succeed");
        assert_eq!(manager.get_state(0), Some(AdapterState::Hot));
    }

    #[tokio::test]
    async fn router_decision_updates_activation_and_eviction() {
        let adapter_names = vec!["adapter_a".to_string(), "adapter_b".to_string()];
        let temp_dir = new_test_tempdir("mplora_activation_tracker_");
        let temp_dir_path = temp_dir.path().to_path_buf();

        let adapter_hashes = build_adapter_hashes(&adapter_names);
        let manager = LifecycleManager::new(
            adapter_names.clone(),
            adapter_hashes,
            &test_policies(),
            temp_dir_path.clone(),
            None,
            2,
        );

        manager.set_activation_window(3);
        manager
            .promote_adapter(0)
            .await
            .expect("promotion should succeed");
        manager
            .promote_adapter(1)
            .await
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

        // Adapter 0 should fall below activation threshold and be demoted
        // (may be Cold or Unloaded depending on timing)
        let state0 = manager.get_state(0);
        assert!(
            state0 == Some(AdapterState::Cold) || state0 == Some(AdapterState::Unloaded),
            "Adapter 0 should be demoted, got {:?}",
            state0
        );
    }

    /// Test deadlock detection: concurrent operations should complete without hanging.
    /// The auto_promote_adapter/auto_demote_adapter methods now properly release locks
    /// before async operations, making them safe to use with tokio::spawn.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_no_deadlock_concurrent_operations() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc as StdArc;

        let adapter_names = vec![
            "adapter_0".to_string(),
            "adapter_1".to_string(),
            "adapter_2".to_string(),
        ];
        let temp_dir = new_test_tempdir("mplora_test_deadlock_concurrent_");
        let temp_dir_path = temp_dir.path().to_path_buf();

        let adapter_hashes = build_adapter_hashes(&adapter_names);
        let manager = Arc::new(LifecycleManager::new(
            adapter_names.clone(),
            adapter_hashes,
            &test_policies(),
            temp_dir_path.clone(),
            None,
            3,
        ));

        // Pre-promote adapters to warm state so they can be demoted
        for i in 0..3 {
            manager
                .promote_adapter(i)
                .await
                .expect("initial promotion should succeed");
        }

        let completed = StdArc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        // Spawn concurrent tasks that exercise async methods with proper lock scoping.
        // These methods now correctly release locks before await points, making them Send.
        for i in 0..3 {
            let mgr = Arc::clone(&manager);
            let done = StdArc::clone(&completed);

            handles.push(tokio::spawn(async move {
                // These async methods properly scope their locks
                let _ = mgr.auto_promote_adapter(i).await;
                let _ = mgr.auto_demote_adapter(i).await;
                done.fetch_add(1, Ordering::SeqCst);
            }));
        }

        // Wait with timeout to detect potential deadlocks
        let timeout_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            futures::future::join_all(handles),
        )
        .await;

        assert!(
            timeout_result.is_ok(),
            "Timeout occurred - possible deadlock in concurrent operations"
        );

        assert_eq!(
            completed.load(Ordering::SeqCst),
            3,
            "All concurrent operations should complete"
        );
    }

    /// Test that locks are properly scoped and released
    #[tokio::test]
    async fn test_lock_scope_explicit() {
        let adapter_names = vec!["adapter_0".to_string()];
        let temp_dir = new_test_tempdir("mplora_test_lock_scope_");
        let temp_dir_path = temp_dir.path().to_path_buf();

        let adapter_hashes = build_adapter_hashes(&adapter_names);
        let manager = LifecycleManager::new(
            adapter_names.clone(),
            adapter_hashes,
            &test_policies(),
            temp_dir_path.clone(),
            None,
            1,
        );

        // Test that update_adapter_state releases lock before async operations
        manager
            .promote_adapter(0)
            .await
            .expect("promotion should succeed");

        // This should not deadlock - lock is released before telemetry logging
        manager
            .update_adapter_state(0, AdapterState::Warm, "test")
            .await
            .expect("update should succeed");

        // Verify state was updated
        assert_eq!(manager.get_state(0), Some(AdapterState::Warm));
    }

    /// Test concurrent record_adapter_activation doesn't deadlock
    #[tokio::test]
    async fn test_concurrent_activation_recording() {
        let adapter_names = vec!["adapter_0".to_string(), "adapter_1".to_string()];
        let temp_dir = new_test_tempdir("mplora_test_activation_concurrent_");
        let temp_dir_path = temp_dir.path().to_path_buf();

        let adapter_hashes = build_adapter_hashes(&adapter_names);
        let manager = Arc::new(LifecycleManager::new(
            adapter_names.clone(),
            adapter_hashes,
            &test_policies(),
            temp_dir_path.clone(),
            None,
            2,
        ));

        // Pre-promote adapters
        manager
            .promote_adapter(0)
            .await
            .expect("promotion should succeed");
        manager
            .promote_adapter(1)
            .await
            .expect("promotion should succeed");

        // Spawn multiple concurrent activation records
        let mut handles = Vec::new();

        for _ in 0..10 {
            let manager_clone = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                manager_clone
                    .record_adapter_activation(0)
                    .await
                    .expect("activation record should succeed");
                manager_clone
                    .record_adapter_activation(1)
                    .await
                    .expect("activation record should succeed");
            }));
        }

        // All should complete without deadlock
        for handle in handles {
            handle.await.expect("task should complete without deadlock");
        }
    }

    /// Test evict_adapter doesn't deadlock with nested locks
    #[tokio::test]
    async fn test_evict_adapter_no_deadlock() {
        let adapter_names = vec!["adapter_0".to_string()];
        let temp_dir = new_test_tempdir("mplora_test_evict_deadlock_");
        let temp_dir_path = temp_dir.path().to_path_buf();

        let adapter_hashes = build_adapter_hashes(&adapter_names);
        let manager = LifecycleManager::new(
            adapter_names.clone(),
            adapter_hashes,
            &test_policies(),
            temp_dir_path.clone(),
            None,
            1,
        );

        // Promote so it can be evicted
        manager
            .promote_adapter(0)
            .await
            .expect("promotion should succeed");

        // Evict should complete without deadlock
        // (Adapter may not be loaded, so eviction might fail with NotLoaded)
        let evict_result = manager.evict_adapter(0).await;
        match evict_result {
            Ok(()) => {
                // Verify state if eviction succeeded
                assert_eq!(manager.get_state(0), Some(AdapterState::Unloaded));
            }
            Err(e) => {
                // If adapter wasn't loaded, that's OK for this test
                assert!(e.to_string().contains("not loaded"));
            }
        }
    }
}
