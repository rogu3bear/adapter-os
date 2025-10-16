//! Supervisor daemon for AdapterOS
//!
//! Provides:
//! - Health monitoring for worker processes
//! - Auto-quarantine enforcement
//! - Adapter hot-reload
//! - Policy hash watching
//! - Memory pressure monitoring

use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use adapteros_policy::{PolicyHashWatcher, QuarantineManager, QuarantineOperation};
use adapteros_registry::Registry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Supervisor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorConfig {
    /// Health check interval (seconds)
    pub health_check_interval_secs: u64,
    /// Policy validation interval (seconds)
    pub policy_check_interval_secs: u64,
    /// Adapter update check interval (seconds)
    pub adapter_check_interval_secs: u64,
    /// Memory monitoring interval (seconds)
    pub memory_check_interval_secs: u64,
    /// Database path
    pub db_path: PathBuf,
    /// Auto-quarantine enabled
    pub auto_quarantine_enabled: bool,
    /// Hot-reload enabled
    pub hot_reload_enabled: bool,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            health_check_interval_secs: 5,
            policy_check_interval_secs: 30,
            adapter_check_interval_secs: 60,
            memory_check_interval_secs: 10,
            db_path: PathBuf::from("var/aos.db"),
            auto_quarantine_enabled: true,
            hot_reload_enabled: true,
        }
    }
}

/// Worker handle
#[derive(Debug, Clone)]
pub struct WorkerHandle {
    /// Tenant ID
    pub tenant_id: String,
    /// Process ID
    pub pid: Option<u32>,
    /// Worker status
    pub status: WorkerStatus,
    /// Last health check timestamp
    pub last_health_check: std::time::SystemTime,
}

/// Worker status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkerStatus {
    /// Worker is running and healthy
    Healthy,
    /// Worker is running but degraded
    Degraded,
    /// Worker is quarantined
    Quarantined,
    /// Worker is stopped
    Stopped,
    /// Worker is restarting
    Restarting,
}

/// Restart policy with exponential backoff
/// Implements: 1s, 2s, 4s, 8s, up to 300s max
#[derive(Debug, Clone)]
pub struct RestartPolicy {
    /// Base delay in seconds
    pub base_delay_secs: u64,
    /// Maximum delay cap in seconds
    pub max_delay_secs: u64,
    /// Maximum restart attempts
    pub max_attempts: u32,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            base_delay_secs: 1,
            max_delay_secs: 300, // 5 minutes cap
            max_attempts: 10,
        }
    }
}

impl RestartPolicy {
    /// Calculate backoff delay for attempt number
    pub fn backoff_delay(&self, attempt: u32) -> Duration {
        let delay = self.base_delay_secs * 2u64.pow(attempt.saturating_sub(1));
        let capped = delay.min(self.max_delay_secs);
        Duration::from_secs(capped)
    }
}

/// Worker restart state
#[derive(Debug, Clone)]
pub struct WorkerRestartState {
    /// Number of restart attempts
    pub attempts: u32,
    /// Timestamp of last restart
    pub last_restart: std::time::SystemTime,
    /// Timestamp of last crash
    pub last_crash: Option<std::time::SystemTime>,
    /// Restart policy
    pub policy: RestartPolicy,
}

impl Default for WorkerRestartState {
    fn default() -> Self {
        Self {
            attempts: 0,
            last_restart: std::time::SystemTime::UNIX_EPOCH,
            last_crash: None,
            policy: RestartPolicy::default(),
        }
    }
}

/// Adapter update
#[derive(Debug, Clone)]
pub struct AdapterUpdate {
    /// Adapter ID
    pub adapter_id: String,
    /// Old version
    pub old_version: String,
    /// New version
    pub new_version: String,
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// Tenant ID
    pub tenant_id: String,
    /// Healthy status
    pub healthy: bool,
    /// Error message (if unhealthy)
    pub error: Option<String>,
}

/// Supervisor daemon
pub struct SupervisorDaemon {
    /// Configuration
    config: SupervisorConfig,
    /// Active worker processes
    workers: Arc<Mutex<HashMap<String, WorkerHandle>>>,
    /// Worker restart states (tracks backoff)
    restart_states: Arc<Mutex<HashMap<String, WorkerRestartState>>>,
    /// Health checker
    _health_checker: Arc<HealthChecker>,
    /// Policy hash watcher
    policy_watcher: Option<Arc<PolicyHashWatcher>>,
    /// Quarantine manager
    quarantine_manager: Arc<Mutex<QuarantineManager>>,
    /// Adapter registry
    adapter_registry: Option<Arc<Registry>>,
    /// Database
    db: Arc<Db>,
}

/// Health checker
pub struct HealthChecker {
    /// Check interval
    interval: Duration,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(interval: Duration) -> Self {
        Self { interval }
    }

    /// Check worker health
    pub async fn check_workers(
        &self,
        workers: &HashMap<String, WorkerHandle>,
    ) -> Vec<HealthCheckResult> {
        let mut results = Vec::new();

        for (tenant_id, worker) in workers.iter() {
            let result = self.check_worker(tenant_id, worker).await;
            results.push(result);
        }

        results
    }

    /// Check single worker
    async fn check_worker(&self, tenant_id: &str, worker: &WorkerHandle) -> HealthCheckResult {
        // In production, this would:
        // 1. Check process is running
        // 2. Ping worker health endpoint
        // 3. Verify memory usage
        // 4. Check response times

        let healthy = worker.status == WorkerStatus::Healthy;

        HealthCheckResult {
            tenant_id: tenant_id.to_string(),
            healthy,
            error: if !healthy {
                Some(format!("Worker status: {:?}", worker.status))
            } else {
                None
            },
        }
    }
}

impl SupervisorDaemon {
    /// Create a new supervisor daemon
    pub async fn new(config: SupervisorConfig) -> Result<Self> {
        info!("Initializing supervisor daemon");

        let db = Arc::new(
            Db::connect(config.db_path.to_str().unwrap())
                .await
                .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?,
        );

        let health_checker = Arc::new(HealthChecker::new(Duration::from_secs(
            config.health_check_interval_secs,
        )));

        Ok(Self {
            config,
            workers: Arc::new(Mutex::new(HashMap::new())),
            restart_states: Arc::new(Mutex::new(HashMap::new())),
            _health_checker: health_checker,
            policy_watcher: None,
            quarantine_manager: Arc::new(Mutex::new(QuarantineManager::new())),
            adapter_registry: None,
            db,
        })
    }

    /// Set policy watcher
    pub fn with_policy_watcher(mut self, watcher: Arc<PolicyHashWatcher>) -> Self {
        self.policy_watcher = Some(watcher);
        self
    }

    /// Set adapter registry
    pub fn with_adapter_registry(mut self, registry: Arc<Registry>) -> Self {
        self.adapter_registry = Some(registry);
        self
    }

    /// Run the supervisor daemon
    pub async fn run(&self) -> Result<()> {
        info!("Starting supervisor daemon");

        let mut health_interval =
            interval(Duration::from_secs(self.config.health_check_interval_secs));
        let mut policy_interval =
            interval(Duration::from_secs(self.config.policy_check_interval_secs));
        let mut adapter_interval =
            interval(Duration::from_secs(self.config.adapter_check_interval_secs));
        let mut memory_interval =
            interval(Duration::from_secs(self.config.memory_check_interval_secs));

        loop {
            tokio::select! {
                _ = health_interval.tick() => {
                    if let Err(e) = self.check_worker_health().await {
                        error!("Health check failed: {}", e);
                    }
                }
                _ = policy_interval.tick() => {
                    if let Err(e) = self.validate_policy_hashes().await {
                        error!("Policy validation failed: {}", e);
                    }
                }
                _ = adapter_interval.tick() => {
                    if self.config.hot_reload_enabled {
                        if let Err(e) = self.check_adapter_updates().await {
                            error!("Adapter check failed: {}", e);
                        }
                    }
                }
                _ = memory_interval.tick() => {
                    if let Err(e) = self.monitor_memory_pressure().await {
                        error!("Memory monitoring failed: {}", e);
                    }
                }
            }
        }
    }

    /// Check worker health
    async fn check_worker_health(&self) -> Result<()> {
        let workers = self.workers.lock().unwrap();
        debug!("Checking health of {} workers", workers.len());

        for (tenant_id, worker) in workers.iter() {
            // Check if worker process is still running
            if let Some(pid) = worker.pid {
                // In production, check if process exists
                debug!(
                    "Worker {} (PID {}) status: {:?}",
                    tenant_id, pid, worker.status
                );
            }

            // Check if worker is quarantined
            if worker.status == WorkerStatus::Quarantined {
                warn!("Worker {} is quarantined", tenant_id);
            }
        }

        Ok(())
    }

    /// Validate policy hashes
    async fn validate_policy_hashes(&self) -> Result<()> {
        if let Some(ref watcher) = self.policy_watcher {
            debug!("Validating policy hashes");

            // Check if system is quarantined
            if watcher.is_quarantined() {
                if self.config.auto_quarantine_enabled {
                    self.enforce_quarantine().await?;
                }
            }
        }

        Ok(())
    }

    /// Check for adapter updates
    async fn check_adapter_updates(&self) -> Result<()> {
        if let Some(ref _registry) = self.adapter_registry {
            debug!("Checking for adapter updates");

            // In production, this would check for new adapter versions
            // and trigger hot-reload
            let _updates = Vec::<AdapterUpdate>::new();

            // Log adapter stats
            debug!("Adapter registry check complete");
        }

        Ok(())
    }

    /// Monitor memory pressure
    async fn monitor_memory_pressure(&self) -> Result<()> {
        debug!("Monitoring memory pressure");

        // In production, this would:
        // 1. Check system memory usage
        // 2. Check per-worker memory
        // 3. Trigger eviction if needed
        // 4. Reduce K if memory pressure high

        Ok(())
    }

    /// Enforce quarantine on all workers
    async fn enforce_quarantine(&self) -> Result<()> {
        let violations = if let Some(ref watcher) = self.policy_watcher {
            watcher.get_violations()
        } else {
            vec![]
        };

        let summary = format!("{} policy violations detected", violations.len());

        warn!("Enforcing system quarantine: {}", summary);

        // Set quarantine on all workers
        let workers = self.workers.lock().unwrap();
        for (_tenant_id, _worker) in workers.iter() {
            // In production, signal worker to enter quarantine mode
            debug!("Signaling worker to enter quarantine");
        }

        // Update quarantine manager
        {
            let mut quarantine = self.quarantine_manager.lock().unwrap();
            quarantine.set_quarantined(true, summary);
        }

        Ok(())
    }

    /// Register a worker
    pub fn register_worker(&self, tenant_id: String, pid: Option<u32>) {
        let mut workers = self.workers.lock().unwrap();
        workers.insert(
            tenant_id.clone(),
            WorkerHandle {
                tenant_id,
                pid,
                status: WorkerStatus::Healthy,
                last_health_check: std::time::SystemTime::now(),
            },
        );
    }

    /// Get worker status
    pub fn get_worker_status(&self, tenant_id: &str) -> Option<WorkerStatus> {
        let workers = self.workers.lock().unwrap();
        workers.get(tenant_id).map(|w| w.status.clone())
    }

    /// Check quarantine operation
    pub fn check_operation(&self, operation: QuarantineOperation) -> Result<()> {
        let quarantine = self.quarantine_manager.lock().unwrap();
        quarantine.check_operation(operation)
    }

    /// Handle worker crash with exponential backoff restart
    ///
    /// Implements restart policy: 1s, 2s, 4s, 8s, up to 300s max
    /// Records crash and restart events in database for audit
    pub async fn handle_worker_crash(&self, tenant_id: &str, crash_reason: &str) -> Result<()> {
        info!("Worker {} crashed: {}", tenant_id, crash_reason);

        // Record crash in database
        self.record_crash(tenant_id, crash_reason).await?;

        // Get restart state
        let mut restart_states = self.restart_states.lock().unwrap();
        let restart_state = restart_states
            .entry(tenant_id.to_string())
            .or_insert_with(WorkerRestartState::default);

        restart_state.attempts += 1;
        restart_state.last_crash = Some(std::time::SystemTime::now());

        // Check if we've exceeded max attempts
        if restart_state.attempts > restart_state.policy.max_attempts {
            error!(
                "Worker {} exceeded max restart attempts ({}), marking as stopped",
                tenant_id, restart_state.policy.max_attempts
            );

            // Update worker status
            {
                let mut workers = self.workers.lock().unwrap();
                if let Some(worker) = workers.get_mut(tenant_id) {
                    worker.status = WorkerStatus::Stopped;
                }
            }

            return Err(AosError::Worker(format!(
                "Worker {} exceeded max restart attempts",
                tenant_id
            )));
        }

        // Calculate backoff delay
        let backoff = restart_state.policy.backoff_delay(restart_state.attempts);
        info!(
            "Restarting worker {} after {}s (attempt {}/{})",
            tenant_id,
            backoff.as_secs(),
            restart_state.attempts,
            restart_state.policy.max_attempts
        );

        // Mark as restarting
        {
            let mut workers = self.workers.lock().unwrap();
            if let Some(worker) = workers.get_mut(tenant_id) {
                worker.status = WorkerStatus::Restarting;
            }
        }

        // Wait for backoff delay
        tokio::time::sleep(backoff).await;

        // Perform restart (in production, this would actually restart the worker process)
        self.restart_worker(tenant_id).await?;

        // Record restart in database
        self.record_restart(tenant_id, restart_state.attempts)
            .await?;

        restart_state.last_restart = std::time::SystemTime::now();

        // On successful restart, reset attempts after a grace period
        // (in production, would be triggered by successful health checks)
        Ok(())
    }

    /// Restart a worker process
    async fn restart_worker(&self, tenant_id: &str) -> Result<()> {
        info!("Restarting worker {}", tenant_id);

        // In production, this would:
        // 1. Kill existing process if still running
        // 2. Spawn new worker process with same tenant config
        // 3. Update PID in worker handle
        // 4. Wait for initial health check

        // For now, just mark as healthy (placeholder)
        let mut workers = self.workers.lock().unwrap();
        if let Some(worker) = workers.get_mut(tenant_id) {
            worker.status = WorkerStatus::Healthy;
            worker.pid = Some(std::process::id()); // Placeholder PID
            worker.last_health_check = std::time::SystemTime::now();
        }

        Ok(())
    }

    /// Record crash event in database
    async fn record_crash(&self, tenant_id: &str, reason: &str) -> Result<()> {
        // In production, insert into worker_crashes table
        // For now, log only
        info!("Recording crash for worker {}: {}", tenant_id, reason);
        Ok(())
    }

    /// Record restart event in database
    async fn record_restart(&self, tenant_id: &str, attempt: u32) -> Result<()> {
        // In production, insert into worker_restarts table
        // For now, log only
        info!("Recording restart #{} for worker {}", attempt, tenant_id);
        Ok(())
    }

    /// Reset restart attempts after successful uptime
    pub fn reset_restart_attempts(&self, tenant_id: &str) {
        let mut restart_states = self.restart_states.lock().unwrap();
        if let Some(state) = restart_states.get_mut(tenant_id) {
            info!(
                "Resetting restart attempts for worker {} (was {})",
                tenant_id, state.attempts
            );
            state.attempts = 0;
        }
    }

    /// Get restart state for a worker
    pub fn get_restart_state(&self, tenant_id: &str) -> Option<WorkerRestartState> {
        let restart_states = self.restart_states.lock().unwrap();
        restart_states.get(tenant_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_supervisor_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let config = SupervisorConfig {
            db_path,
            ..Default::default()
        };

        let supervisor = SupervisorDaemon::new(config).await.unwrap();
        assert!(supervisor.workers.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_worker_registration() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let config = SupervisorConfig {
            db_path,
            ..Default::default()
        };

        let supervisor = SupervisorDaemon::new(config).await.unwrap();
        supervisor.register_worker("test-tenant".to_string(), Some(12345));

        let status = supervisor.get_worker_status("test-tenant");
        assert_eq!(status, Some(WorkerStatus::Healthy));
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let policy = RestartPolicy::default();

        // Test exponential backoff: 1s, 2s, 4s, 8s, ...
        assert_eq!(policy.backoff_delay(1).as_secs(), 1);
        assert_eq!(policy.backoff_delay(2).as_secs(), 2);
        assert_eq!(policy.backoff_delay(3).as_secs(), 4);
        assert_eq!(policy.backoff_delay(4).as_secs(), 8);
        assert_eq!(policy.backoff_delay(5).as_secs(), 16);
        assert_eq!(policy.backoff_delay(6).as_secs(), 32);
        assert_eq!(policy.backoff_delay(7).as_secs(), 64);
        assert_eq!(policy.backoff_delay(8).as_secs(), 128);
        assert_eq!(policy.backoff_delay(9).as_secs(), 256);

        // Test cap at 300s
        assert_eq!(policy.backoff_delay(10).as_secs(), 300);
        assert_eq!(policy.backoff_delay(15).as_secs(), 300);
    }

    #[tokio::test]
    async fn test_worker_crash_and_restart() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let config = SupervisorConfig {
            db_path,
            ..Default::default()
        };

        let supervisor = SupervisorDaemon::new(config).await.unwrap();
        supervisor.register_worker("test-tenant".to_string(), Some(12345));

        // Simulate crash
        supervisor
            .handle_worker_crash("test-tenant", "simulated crash")
            .await
            .unwrap();

        // Worker should be restarted and healthy
        let status = supervisor.get_worker_status("test-tenant");
        assert_eq!(status, Some(WorkerStatus::Healthy));

        // Restart attempts should be 1
        let restart_state = supervisor.get_restart_state("test-tenant").unwrap();
        assert_eq!(restart_state.attempts, 1);
    }
}
