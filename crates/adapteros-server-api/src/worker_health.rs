//! Worker Health Monitor for PRD-09: Worker Health, Hung Detection & Log Centralization
//!
//! Provides active and passive health monitoring for workers, including:
//! - Latency tracking with moving averages
//! - Degraded/crashed state detection
//! - Health-aware worker selection for routing
//! - Background polling for idle worker crash detection
//! - Model-state-aware routing for smarter request distribution

use crate::state::WorkerRuntimeInfo;
use crate::uds_client::UdsClient;
use adapteros_api_types::workers::WorkerModelLoadState;
use adapteros_db::workers::{WorkerIncidentType, WorkerWithBinding};
use adapteros_db::Db;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Health status for a worker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerHealthStatus {
    /// Worker is responding quickly and reliably
    Healthy,
    /// Worker is responding slowly (latency > threshold for consecutive requests)
    Degraded,
    /// Worker is not responding (connection failed or timeout)
    Crashed,
    /// Worker has never been contacted
    Unknown,
}

impl std::fmt::Display for WorkerHealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerHealthStatus::Healthy => write!(f, "healthy"),
            WorkerHealthStatus::Degraded => write!(f, "degraded"),
            WorkerHealthStatus::Crashed => write!(f, "crashed"),
            WorkerHealthStatus::Unknown => write!(f, "unknown"),
        }
    }
}

impl From<&str> for WorkerHealthStatus {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "healthy" => WorkerHealthStatus::Healthy,
            "degraded" => WorkerHealthStatus::Degraded,
            "crashed" => WorkerHealthStatus::Crashed,
            _ => WorkerHealthStatus::Unknown,
        }
    }
}

/// Configuration for health monitoring thresholds
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Latency threshold in milliseconds (responses above this are "slow")
    pub latency_threshold_ms: u64,
    /// Number of consecutive slow responses to trigger degraded status
    pub slow_response_count: usize,
    /// Number of consecutive fast responses to recover from degraded
    pub recovery_count: usize,
    /// Number of samples for moving average calculation
    pub moving_avg_window: usize,
    /// Interval between active health polls
    pub polling_interval: Duration,
    /// Timeout for health check requests
    pub polling_timeout: Duration,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            latency_threshold_ms: 5000,                // 5 seconds
            slow_response_count: 5,                    // 5 consecutive slow responses
            recovery_count: 5,                         // 5 consecutive fast responses to recover
            moving_avg_window: 10,                     // 10 samples for moving average
            polling_interval: Duration::from_secs(30), // Poll every 30 seconds
            polling_timeout: Duration::from_secs(3),   // 3 second timeout for health checks
        }
    }
}

/// Per-worker health metrics tracked in memory
#[derive(Debug, Clone)]
pub struct WorkerMetrics {
    /// Ring buffer of recent latencies in milliseconds
    pub recent_latencies: VecDeque<u64>,
    /// Calculated average latency
    pub avg_latency_ms: f64,
    /// Count of consecutive slow responses
    pub consecutive_slow: usize,
    /// Count of consecutive fast responses (for recovery tracking)
    pub consecutive_fast: usize,
    /// Count of consecutive failures (connection errors)
    pub consecutive_failures: usize,
    /// Timestamp of last successful response
    pub last_response_at: Option<Instant>,
    /// Current health status
    pub health_status: WorkerHealthStatus,
    /// Total requests processed (for statistics)
    pub total_requests: u64,
    /// Total failures (for statistics)
    pub total_failures: u64,
}

impl Default for WorkerMetrics {
    fn default() -> Self {
        Self {
            recent_latencies: VecDeque::with_capacity(10),
            avg_latency_ms: 0.0,
            consecutive_slow: 0,
            consecutive_fast: 0,
            consecutive_failures: 0,
            last_response_at: None,
            health_status: WorkerHealthStatus::Unknown,
            total_requests: 0,
            total_failures: 0,
        }
    }
}

/// Worker health monitor with background polling
pub struct WorkerHealthMonitor {
    /// Database handle for persisting health data
    db: Db,
    /// Configuration for thresholds and timing
    config: HealthConfig,
    /// Per-worker metrics stored in memory
    worker_metrics: DashMap<String, WorkerMetrics>,
    /// Shutdown signal for the polling task
    shutdown: CancellationToken,
}

impl WorkerHealthMonitor {
    /// Create a new health monitor with the given database and config
    pub fn new(db: Db, config: HealthConfig) -> Self {
        Self {
            db,
            config,
            worker_metrics: DashMap::new(),
            shutdown: CancellationToken::new(),
        }
    }

    /// Create a new health monitor with default configuration
    pub fn with_defaults(db: Db) -> Self {
        Self::new(db, HealthConfig::default())
    }

    /// Get the shutdown token for this monitor
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Start the background health polling task
    ///
    /// Returns a JoinHandle that can be awaited for shutdown
    pub fn start_polling(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let monitor = self.clone();
        tokio::spawn(async move {
            monitor.run_polling_loop().await;
        })
    }

    /// Main polling loop that runs in the background
    pub async fn run_polling_loop(&self) {
        info!(
            interval_secs = self.config.polling_interval.as_secs(),
            "Starting worker health polling loop"
        );

        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.config.polling_interval) => {
                    if let Err(e) = self.poll_all_workers().await {
                        error!(error = %e, "Error polling workers");
                    }
                }
                _ = self.shutdown.cancelled() => {
                    info!("Worker health polling loop shutting down");
                    break;
                }
            }
        }
    }

    /// Poll all workers and update their health status
    async fn poll_all_workers(&self) -> Result<(), adapteros_core::AosError> {
        let workers = self.db.list_all_workers().await?;

        // A single UDS path maps to a single live socket. If the DB accumulates
        // multiple worker IDs for the same socket across restarts, we must not
        // "heal" all of them by polling the same socket.
        //
        // Pick the newest non-terminal worker per UDS path and only poll that one.
        let mut by_uds: HashMap<String, adapteros_db::models::Worker> = HashMap::new();
        for worker in workers {
            if worker.status == "stopped" || worker.status == "error" {
                continue;
            }

            by_uds
                .entry(worker.uds_path.clone())
                .and_modify(|current| {
                    if worker.started_at > current.started_at {
                        *current = worker.clone();
                    }
                })
                .or_insert(worker);
        }

        debug!(count = by_uds.len(), "Polling workers for health");

        for worker in by_uds.values() {
            let uds_path = PathBuf::from(&worker.uds_path);
            let result = self.health_check(&uds_path).await;

            match result {
                Ok(latency_ms) => {
                    self.record_response(&worker.id, latency_ms).await;
                }
                Err(e) => {
                    debug!(worker_id = %worker.id, error = %e, "Health check failed");
                    self.record_failure(&worker.id, &e.to_string()).await;
                }
            }
        }

        // Sync metrics to database periodically
        if let Err(e) = self.sync_to_db().await {
            warn!(error = %e, "Failed to sync health metrics to database");
        }

        Ok(())
    }

    /// Perform a health check on a worker and return latency in ms
    async fn health_check(
        &self,
        uds_path: &Path,
    ) -> Result<u64, crate::uds_client::UdsClientError> {
        let client = UdsClient::new(self.config.polling_timeout);
        let start = Instant::now();

        client.health_check(uds_path).await?;

        Ok(start.elapsed().as_millis() as u64)
    }

    /// Record a successful response and update metrics
    pub async fn record_response(&self, worker_id: &str, latency_ms: u64) {
        let is_slow = latency_ms >= self.config.latency_threshold_ms;

        let mut metrics = self
            .worker_metrics
            .entry(worker_id.to_string())
            .or_default();
        let metrics = metrics.value_mut();

        // Update latency ring buffer
        if metrics.recent_latencies.len() >= self.config.moving_avg_window {
            metrics.recent_latencies.pop_front();
        }
        metrics.recent_latencies.push_back(latency_ms);

        // Recalculate moving average
        if !metrics.recent_latencies.is_empty() {
            let sum: u64 = metrics.recent_latencies.iter().sum();
            metrics.avg_latency_ms = sum as f64 / metrics.recent_latencies.len() as f64;
        }

        // Update consecutive counts
        if is_slow {
            metrics.consecutive_slow += 1;
            metrics.consecutive_fast = 0;
        } else {
            metrics.consecutive_fast += 1;
            metrics.consecutive_slow = 0;
        }

        // Reset failure count on successful response
        metrics.consecutive_failures = 0;
        metrics.last_response_at = Some(Instant::now());
        metrics.total_requests += 1;

        // Determine new health status
        let old_status = metrics.health_status;
        metrics.health_status = self.calculate_health_status(metrics);

        // Log status transitions
        if old_status != metrics.health_status {
            match metrics.health_status {
                WorkerHealthStatus::Degraded => {
                    warn!(
                        worker_id = %worker_id,
                        avg_latency_ms = metrics.avg_latency_ms,
                        consecutive_slow = metrics.consecutive_slow,
                        "Worker marked as degraded"
                    );
                    // Create incident for degradation
                    self.create_incident_async(
                        worker_id,
                        WorkerIncidentType::Degraded,
                        &format!(
                            "Worker marked degraded: {} consecutive slow responses (avg {}ms)",
                            metrics.consecutive_slow, metrics.avg_latency_ms as u64
                        ),
                        Some(latency_ms as f64),
                    );
                }
                WorkerHealthStatus::Healthy => {
                    info!(
                        worker_id = %worker_id,
                        consecutive_fast = metrics.consecutive_fast,
                        "Worker recovered to healthy"
                    );
                    // Create incident for recovery
                    self.create_incident_async(
                        worker_id,
                        WorkerIncidentType::Recovered,
                        &format!(
                            "Worker recovered: {} consecutive fast responses",
                            metrics.consecutive_fast
                        ),
                        Some(latency_ms as f64),
                    );
                }
                _ => {}
            }
        }

        debug!(
            worker_id = %worker_id,
            latency_ms = latency_ms,
            avg_latency_ms = metrics.avg_latency_ms,
            health_status = %metrics.health_status,
            "Recorded response"
        );
    }

    /// Record a failed request (connection error, timeout)
    pub async fn record_failure(&self, worker_id: &str, error: &str) {
        let mut metrics = self
            .worker_metrics
            .entry(worker_id.to_string())
            .or_default();
        let metrics = metrics.value_mut();

        metrics.consecutive_failures += 1;
        metrics.consecutive_slow = 0;
        metrics.consecutive_fast = 0;
        metrics.total_failures += 1;

        let old_status = metrics.health_status;
        metrics.health_status = self.calculate_health_status(metrics);

        // Log crash detection
        if old_status != WorkerHealthStatus::Crashed
            && metrics.health_status == WorkerHealthStatus::Crashed
        {
            error!(
                worker_id = %worker_id,
                consecutive_failures = metrics.consecutive_failures,
                error = %error,
                "Worker marked as crashed"
            );
            // Create incident for crash
            self.create_incident_async(
                worker_id,
                WorkerIncidentType::Crash,
                &format!(
                    "Worker crashed: {} consecutive failures. Last error: {}",
                    metrics.consecutive_failures, error
                ),
                None,
            );
        }
    }

    /// Calculate health status based on current metrics
    fn calculate_health_status(&self, metrics: &WorkerMetrics) -> WorkerHealthStatus {
        // Crashed: 3+ consecutive failures
        if metrics.consecutive_failures >= 3 {
            // Allow recovery after a time window (60 seconds)
            // This enables re-probing of crashed workers that may have restarted
            if let Some(last_response) = metrics.last_response_at {
                let recovery_window = Duration::from_secs(60);
                if last_response.elapsed() > recovery_window {
                    // After recovery window, demote to Unknown to allow re-probing
                    // Next successful response will transition to Healthy
                    return WorkerHealthStatus::Unknown;
                }
            }
            return WorkerHealthStatus::Crashed;
        }

        // Degraded: 5+ consecutive slow responses
        if metrics.consecutive_slow >= self.config.slow_response_count {
            return WorkerHealthStatus::Degraded;
        }

        // Recovery from degraded: 5+ consecutive fast responses
        if metrics.health_status == WorkerHealthStatus::Degraded
            && metrics.consecutive_fast >= self.config.recovery_count
        {
            return WorkerHealthStatus::Healthy;
        }

        // If previously degraded but not recovered yet, stay degraded
        if metrics.health_status == WorkerHealthStatus::Degraded {
            return WorkerHealthStatus::Degraded;
        }

        // If we have recent data, we're healthy
        if metrics.last_response_at.is_some() {
            return WorkerHealthStatus::Healthy;
        }

        WorkerHealthStatus::Unknown
    }

    /// Get the best worker for routing from a list of workers
    ///
    /// Selection logic:
    /// 1. Filter out crashed workers
    /// 2. Prefer healthy over degraded
    /// 3. Among same status, pick lowest average latency
    pub fn get_best_worker<'a>(
        &self,
        workers: &'a [adapteros_db::models::Worker],
    ) -> Option<&'a adapteros_db::models::Worker> {
        if workers.is_empty() {
            return None;
        }

        // Get health status and latency for each worker
        let mut candidates: Vec<(&adapteros_db::models::Worker, WorkerHealthStatus, f64)> = workers
            .iter()
            .map(|w| {
                let (status, latency) = self
                    .worker_metrics
                    .get(&w.id)
                    .map(|m| (m.health_status, m.avg_latency_ms))
                    .unwrap_or((WorkerHealthStatus::Unknown, 0.0));
                (w, status, latency)
            })
            .collect();

        // Filter out crashed workers
        candidates.retain(|(_, status, _)| *status != WorkerHealthStatus::Crashed);

        if candidates.is_empty() {
            // All workers crashed, return None
            return None;
        }

        // Sort by: healthy first, then by lowest latency
        candidates.sort_by(|(wa, status_a, latency_a), (wb, status_b, latency_b)| {
            // Healthy < Degraded < Unknown
            let priority_a = match status_a {
                WorkerHealthStatus::Healthy => 0,
                WorkerHealthStatus::Degraded => 1,
                WorkerHealthStatus::Unknown => 2,
                WorkerHealthStatus::Crashed => 3,
            };
            let priority_b = match status_b {
                WorkerHealthStatus::Healthy => 0,
                WorkerHealthStatus::Degraded => 1,
                WorkerHealthStatus::Unknown => 2,
                WorkerHealthStatus::Crashed => 3,
            };

            priority_a.cmp(&priority_b).then_with(|| {
                latency_a
                    .partial_cmp(latency_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| wa.id.cmp(&wb.id))
            })
        });

        candidates.first().map(|(w, _, _)| *w)
    }

    /// Get the best worker from a list of WorkerWithBinding
    ///
    /// Similar to `get_best_worker` but accepts workers with manifest binding info.
    /// Selection criteria:
    /// 1. Filter out crashed workers
    /// 2. Prefer healthy over degraded
    /// 3. Among same status, pick lowest average latency
    pub fn get_best_worker_with_binding<'a>(
        &self,
        workers: &'a [WorkerWithBinding],
    ) -> Option<&'a WorkerWithBinding> {
        if workers.is_empty() {
            return None;
        }

        // Get health status and latency for each worker
        let mut candidates: Vec<(&WorkerWithBinding, WorkerHealthStatus, f64)> = workers
            .iter()
            .map(|w| {
                let (status, latency) = self
                    .worker_metrics
                    .get(&w.id)
                    .map(|m| (m.health_status, m.avg_latency_ms))
                    .unwrap_or((WorkerHealthStatus::Unknown, 0.0));
                (w, status, latency)
            })
            .collect();

        // Filter out crashed workers
        candidates.retain(|(_, status, _)| *status != WorkerHealthStatus::Crashed);

        if candidates.is_empty() {
            // All workers crashed, return None
            return None;
        }

        // Sort by: healthy first, then by lowest latency
        candidates.sort_by(|(wa, status_a, latency_a), (wb, status_b, latency_b)| {
            // Healthy < Degraded < Unknown
            let priority_a = match status_a {
                WorkerHealthStatus::Healthy => 0,
                WorkerHealthStatus::Degraded => 1,
                WorkerHealthStatus::Unknown => 2,
                WorkerHealthStatus::Crashed => 3,
            };
            let priority_b = match status_b {
                WorkerHealthStatus::Healthy => 0,
                WorkerHealthStatus::Degraded => 1,
                WorkerHealthStatus::Unknown => 2,
                WorkerHealthStatus::Crashed => 3,
            };

            priority_a.cmp(&priority_b).then_with(|| {
                latency_a
                    .partial_cmp(latency_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| wa.id.cmp(&wb.id))
            })
        });

        candidates.first().map(|(w, _, _)| *w)
    }

    /// Get the best worker considering model state for smarter routing
    ///
    /// This method factors in model load state to avoid routing to workers that are:
    /// - Currently loading a model (would queue the request)
    /// - Near memory pressure (might trigger OOM or slowdown)
    ///
    /// Selection criteria (in priority order):
    /// 1. Filter out crashed workers and workers mid-load
    /// 2. Prefer workers with the requested model already loaded
    /// 3. Prefer healthy over degraded
    /// 4. Penalize workers near memory pressure (>80% cache utilization)
    /// 5. Among same status, pick lowest average latency
    pub fn get_best_worker_for_model<'a>(
        &self,
        workers: &'a [WorkerWithBinding],
        target_model_hash: Option<&str>,
        worker_runtime: &DashMap<String, WorkerRuntimeInfo>,
    ) -> Option<&'a WorkerWithBinding> {
        if workers.is_empty() {
            return None;
        }

        // Build candidate list with extended scoring
        let mut candidates: Vec<WorkerCandidate<'a>> = workers
            .iter()
            .map(|w| {
                let (health_status, latency) = self
                    .worker_metrics
                    .get(&w.id)
                    .map(|m| (m.health_status, m.avg_latency_ms))
                    .unwrap_or((WorkerHealthStatus::Unknown, 0.0));

                let runtime = worker_runtime.get(&w.id);
                let (model_load_state, loaded_model_hash, memory_pressure) =
                    if let Some(ref rt) = runtime {
                        let pressure = rt
                            .cache_stats
                            .as_ref()
                            .and_then(|cs| {
                                match (cs.used_mb, cs.max_mb) {
                                    (Some(used), Some(max)) if max > 0 => {
                                        Some(used as f32 / max as f32)
                                    }
                                    _ => cs.memory_bytes.and_then(|bytes| {
                                        // Assume 16GB max if not specified (common for Apple Silicon)
                                        let max_bytes = 16 * 1024 * 1024 * 1024u64;
                                        Some(bytes as f32 / max_bytes as f32)
                                    }),
                                }
                            })
                            .unwrap_or(0.0);
                        (
                            rt.model_load_state.clone(),
                            rt.loaded_model_hash.clone(),
                            pressure,
                        )
                    } else {
                        (None, None, 0.0)
                    };

                WorkerCandidate {
                    worker: w,
                    health_status,
                    latency,
                    model_load_state,
                    loaded_model_hash,
                    memory_pressure,
                }
            })
            .collect();

        // Filter out crashed workers and workers currently loading
        candidates.retain(|c| {
            c.health_status != WorkerHealthStatus::Crashed
                && !matches!(c.model_load_state, Some(WorkerModelLoadState::Loading))
        });

        if candidates.is_empty() {
            return None;
        }

        // Sort by model affinity, health, memory pressure, then latency
        candidates.sort_by(|a, b| {
            // 1. Prefer workers with target model already loaded
            let model_affinity_a = if let Some(target) = target_model_hash {
                a.loaded_model_hash
                    .as_ref()
                    .map(|h| h == target)
                    .unwrap_or(false)
            } else {
                false
            };
            let model_affinity_b = if let Some(target) = target_model_hash {
                b.loaded_model_hash
                    .as_ref()
                    .map(|h| h == target)
                    .unwrap_or(false)
            } else {
                false
            };

            // Model affinity is highest priority (true < false in our ordering)
            model_affinity_b.cmp(&model_affinity_a).then_with(|| {
                // 2. Health status priority
                let health_priority_a = match a.health_status {
                    WorkerHealthStatus::Healthy => 0,
                    WorkerHealthStatus::Degraded => 1,
                    WorkerHealthStatus::Unknown => 2,
                    WorkerHealthStatus::Crashed => 3,
                };
                let health_priority_b = match b.health_status {
                    WorkerHealthStatus::Healthy => 0,
                    WorkerHealthStatus::Degraded => 1,
                    WorkerHealthStatus::Unknown => 2,
                    WorkerHealthStatus::Crashed => 3,
                };

                health_priority_a.cmp(&health_priority_b).then_with(|| {
                    // 3. Memory pressure (lower is better, penalize >80%)
                    let pressure_penalty_a = if a.memory_pressure > 0.8 { 1 } else { 0 };
                    let pressure_penalty_b = if b.memory_pressure > 0.8 { 1 } else { 0 };

                    pressure_penalty_a.cmp(&pressure_penalty_b).then_with(|| {
                        // 4. Latency (lower is better)
                        a.latency
                            .partial_cmp(&b.latency)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| {
                                // 5. Deterministic tie-breaker
                                a.worker.id.cmp(&b.worker.id)
                            })
                    })
                })
            })
        });

        candidates.first().map(|c| c.worker)
    }

    /// Check if a worker has the specified model loaded and ready
    pub fn has_model_loaded(
        &self,
        worker_id: &str,
        model_hash: &str,
        worker_runtime: &DashMap<String, WorkerRuntimeInfo>,
    ) -> bool {
        worker_runtime.get(worker_id).map_or(false, |rt| {
            matches!(rt.model_load_state, Some(WorkerModelLoadState::Loaded))
                && rt
                    .loaded_model_hash
                    .as_ref()
                    .map_or(false, |h| h == model_hash)
        })
    }

    /// Get memory pressure level for a worker (0.0 = no pressure, 1.0 = fully utilized)
    pub fn get_memory_pressure(
        &self,
        worker_id: &str,
        worker_runtime: &DashMap<String, WorkerRuntimeInfo>,
    ) -> f32 {
        worker_runtime
            .get(worker_id)
            .and_then(|rt| {
                rt.cache_stats
                    .as_ref()
                    .and_then(|cs| match (cs.used_mb, cs.max_mb) {
                        (Some(used), Some(max)) if max > 0 => Some(used as f32 / max as f32),
                        _ => None,
                    })
            })
            .unwrap_or(0.0)
    }

    /// Get health metrics for a specific worker
    pub fn get_worker_metrics(&self, worker_id: &str) -> Option<WorkerMetrics> {
        self.worker_metrics.get(worker_id).map(|m| m.clone())
    }

    /// Get health status for a specific worker
    pub fn get_worker_health(&self, worker_id: &str) -> WorkerHealthStatus {
        self.worker_metrics
            .get(worker_id)
            .map(|m| m.health_status)
            .unwrap_or(WorkerHealthStatus::Unknown)
    }

    /// Get health summary for all tracked workers
    pub fn get_health_summary(&self) -> Vec<WorkerHealthSummary> {
        self.worker_metrics
            .iter()
            .map(|entry| WorkerHealthSummary {
                worker_id: entry.key().clone(),
                health_status: entry.value().health_status,
                avg_latency_ms: entry.value().avg_latency_ms,
                total_requests: entry.value().total_requests,
                total_failures: entry.value().total_failures,
                consecutive_slow: entry.value().consecutive_slow,
                consecutive_failures: entry.value().consecutive_failures,
            })
            .collect()
    }

    /// Sync health metrics to database
    async fn sync_to_db(&self) -> Result<(), adapteros_core::AosError> {
        for entry in self.worker_metrics.iter() {
            let worker_id = entry.key();
            let metrics = entry.value();

            self.db
                .update_worker_health_metrics(
                    worker_id,
                    &metrics.health_status.to_string(),
                    metrics.avg_latency_ms,
                    metrics.recent_latencies.len() as i32,
                    metrics.consecutive_slow as i32,
                    metrics.consecutive_failures as i32,
                )
                .await?;
        }

        Ok(())
    }

    /// Create an incident asynchronously with retry logic
    fn create_incident_async(
        &self,
        worker_id: &str,
        incident_type: WorkerIncidentType,
        reason: &str,
        latency_ms: Option<f64>,
    ) {
        let db = self.db.clone();
        let worker_id = worker_id.to_string();
        let reason = reason.to_string();

        tokio::spawn(async move {
            const MAX_ATTEMPTS: u32 = 3;
            let mut attempts = 0u32;

            while attempts < MAX_ATTEMPTS {
                attempts += 1;

                // Get tenant_id from worker
                match db.get_worker(&worker_id).await {
                    Ok(Some(worker)) => {
                        match db
                            .insert_worker_incident(
                                &worker_id,
                                &worker.tenant_id,
                                incident_type,
                                &reason,
                                None, // backtrace
                                latency_ms,
                            )
                            .await
                        {
                            Ok(_) => return, // Success
                            Err(e) => {
                                if attempts < MAX_ATTEMPTS {
                                    warn!(
                                        attempt = attempts,
                                        error = %e,
                                        worker_id = %worker_id,
                                        "Failed to insert worker incident, retrying"
                                    );
                                    tokio::time::sleep(Duration::from_millis(
                                        100 * 2u64.pow(attempts),
                                    ))
                                    .await;
                                } else {
                                    error!(
                                        error = %e,
                                        worker_id = %worker_id,
                                        "Exhausted retries for incident creation"
                                    );
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        warn!(worker_id = %worker_id, "Worker not found for incident creation");
                        return;
                    }
                    Err(e) => {
                        if attempts < MAX_ATTEMPTS {
                            debug!(
                                attempt = attempts,
                                error = %e,
                                worker_id = %worker_id,
                                "Failed to get worker for incident, retrying"
                            );
                            tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(attempts)))
                                .await;
                        } else {
                            error!(
                                error = %e,
                                worker_id = %worker_id,
                                "Exhausted retries getting worker for incident"
                            );
                        }
                    }
                }
            }
        });
    }
}

/// Summary of worker health for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHealthSummary {
    pub worker_id: String,
    pub health_status: WorkerHealthStatus,
    pub avg_latency_ms: f64,
    pub total_requests: u64,
    pub total_failures: u64,
    pub consecutive_slow: usize,
    pub consecutive_failures: usize,
}

/// Internal struct for model-aware worker selection scoring
struct WorkerCandidate<'a> {
    worker: &'a WorkerWithBinding,
    health_status: WorkerHealthStatus,
    latency: f64,
    model_load_state: Option<WorkerModelLoadState>,
    loaded_model_hash: Option<String>,
    memory_pressure: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn worker_selection_ties_break_by_id() {
        let db = adapteros_db::Db::new_in_memory()
            .await
            .expect("in-memory db");
        let monitor = WorkerHealthMonitor::with_defaults(db);

        monitor.worker_metrics.insert(
            "worker-a".to_string(),
            WorkerMetrics {
                avg_latency_ms: 10.0,
                health_status: WorkerHealthStatus::Healthy,
                ..Default::default()
            },
        );
        monitor.worker_metrics.insert(
            "worker-b".to_string(),
            WorkerMetrics {
                avg_latency_ms: 10.0,
                health_status: WorkerHealthStatus::Healthy,
                ..Default::default()
            },
        );

        let workers = vec![
            WorkerWithBinding {
                id: "worker-b".to_string(),
                tenant_id: "tenant".to_string(),
                node_id: "node".to_string(),
                plan_id: "plan".to_string(),
                uds_path: "/var/run/aos/tenant/worker-b.sock".to_string(),
                pid: None,
                status: "serving".to_string(),
                started_at: "now".to_string(),
                last_seen_at: None,
                manifest_hash_b3: Some("hash".to_string()),
                backend: None,
                model_hash_b3: None,
                capabilities_json: None,
                schema_version: Some("1.0".to_string()),
                api_version: None,
                registered_at: None,
                health_status: None,
            },
            WorkerWithBinding {
                id: "worker-a".to_string(),
                tenant_id: "tenant".to_string(),
                node_id: "node".to_string(),
                plan_id: "plan".to_string(),
                uds_path: "/var/run/aos/tenant/worker-a.sock".to_string(),
                pid: None,
                status: "serving".to_string(),
                started_at: "now".to_string(),
                last_seen_at: None,
                manifest_hash_b3: Some("hash".to_string()),
                backend: None,
                model_hash_b3: None,
                capabilities_json: None,
                schema_version: Some("1.0".to_string()),
                api_version: None,
                registered_at: None,
                health_status: None,
            },
        ];

        let selected = monitor
            .get_best_worker_with_binding(&workers)
            .expect("selected worker");
        assert_eq!(
            selected.id, "worker-a",
            "Lower lexicographic id should win when latency and health tie"
        );
    }
}
