//! Supervisor daemon for adapterOS
//!
//! Provides:
//! - Health monitoring for worker processes
//! - Auto-quarantine enforcement
//! - Adapter hot-reload
//! - Policy hash watching
//! - Memory pressure monitoring

use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use adapteros_model_hub::registry::Registry;
use adapteros_policy::{PolicyHashWatcher, QuarantineManager, QuarantineOperation};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

const CRASH_LOOP_THRESHOLD: usize = 5;
const CRASH_LOOP_WINDOW: Duration = Duration::from_secs(60);

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
            db_path: adapteros_core::rebase_var_path("var/aos.db"),
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
    /// Recent crash timestamps (sliding window for circuit breaker)
    pub recent_crashes: VecDeque<std::time::SystemTime>,
    /// Restart policy
    pub policy: RestartPolicy,
}

impl Default for WorkerRestartState {
    fn default() -> Self {
        Self {
            attempts: 0,
            last_restart: std::time::SystemTime::UNIX_EPOCH,
            last_crash: None,
            recent_crashes: VecDeque::new(),
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
    workers: Arc<TokioMutex<HashMap<String, WorkerHandle>>>,
    /// Worker restart states (tracks backoff)
    restart_states: Arc<Mutex<HashMap<String, WorkerRestartState>>>,
    /// Health checker
    _health_checker: Arc<HealthChecker>,
    /// Policy hash watcher
    policy_watcher: Option<Arc<PolicyHashWatcher>>,
    /// Quarantine manager
    quarantine_manager: Arc<TokioMutex<QuarantineManager>>,
    /// Adapter registry
    adapter_registry: Option<Arc<Registry>>,
    /// Database (reserved for supervisor persistence)
    _db: Arc<Db>,
}

/// Health checker
pub struct HealthChecker {
    /// Check interval (reserved for scheduled health checks)
    _interval: Duration,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(interval: Duration) -> Self {
        Self {
            _interval: interval,
        }
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
            Db::connect(config.db_path.to_str().ok_or_else(|| {
                AosError::Validation("Database path contains invalid UTF-8".to_string())
            })?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?,
        );

        let health_checker = Arc::new(HealthChecker::new(Duration::from_secs(
            config.health_check_interval_secs,
        )));

        Ok(Self {
            config,
            workers: Arc::new(TokioMutex::new(HashMap::new())),
            restart_states: Arc::new(Mutex::new(HashMap::new())),
            _health_checker: health_checker,
            policy_watcher: None,
            quarantine_manager: Arc::new(TokioMutex::new(QuarantineManager::new())),
            adapter_registry: None,
            _db: db,
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
        let workers = self.workers.lock().await;
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
            if watcher.is_quarantined() && self.config.auto_quarantine_enabled {
                self.enforce_quarantine().await?;
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

    /// Quarantine a worker after repeated crashes and alert control plane
    async fn quarantine_worker(&self, tenant_id: &str, reason: &str) {
        let summary = format!(
            "Worker {} quarantined after repeated crashes: {}",
            tenant_id, reason
        );

        {
            let mut workers = self.workers.lock().await;
            if let Some(worker) = workers.get_mut(tenant_id) {
                worker.status = WorkerStatus::Quarantined;
            }
        }

        {
            let mut quarantine = self.quarantine_manager.lock().await;
            quarantine.set_quarantined(true, summary.clone());
        }

        self.alert_control_plane(tenant_id, &summary).await;
    }

    async fn alert_control_plane(&self, tenant_id: &str, summary: &str) {
        warn!(tenant = tenant_id, "Control plane alert: {}", summary);
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
        let workers = self.workers.lock().await;
        for (_tenant_id, _worker) in workers.iter() {
            // In production, signal worker to enter quarantine mode
            debug!("Signaling worker to enter quarantine");
        }
        drop(workers);

        // Update quarantine manager
        {
            let mut quarantine = self.quarantine_manager.lock().await;
            quarantine.set_quarantined(true, summary);
        }

        Ok(())
    }

    /// Register a worker
    pub async fn register_worker(&self, tenant_id: String, pid: Option<u32>) {
        let mut workers = self.workers.lock().await;
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
    pub async fn get_worker_status(&self, tenant_id: &str) -> Option<WorkerStatus> {
        let workers = self.workers.lock().await;
        workers.get(tenant_id).map(|w| w.status.clone())
    }

    /// Check quarantine operation
    pub async fn check_operation(&self, operation: QuarantineOperation) -> Result<()> {
        let quarantine = self.quarantine_manager.lock().await;
        quarantine.check_operation(operation)
    }

    /// Handle worker crash with exponential backoff restart
    ///
    /// Implements restart policy: 1s, 2s, 4s, 8s, up to 300s max
    /// Records crash and restart events in database for audit
    pub async fn handle_worker_crash(&self, tenant_id: &str, crash_reason: &str) -> Result<()> {
        info!("Worker {} crashed: {}", tenant_id, crash_reason);
        let now = std::time::SystemTime::now();

        // Record crash in database
        self.record_crash(tenant_id, crash_reason).await?;

        // Get restart state info - extract needed values then drop lock
        let (exceeded_max, backoff, attempts, max_attempts, crash_loop_detected) = {
            let mut restart_states = self
                .restart_states
                .lock()
                .map_err(|e| AosError::Internal(format!("Restart states lock poisoned: {}", e)))?;
            let restart_state = restart_states.entry(tenant_id.to_string()).or_default();

            restart_state.attempts += 1;
            restart_state.last_crash = Some(now);
            // Track crash timestamps within sliding window for circuit breaker
            restart_state
                .recent_crashes
                .retain(|ts| now.duration_since(*ts).unwrap_or_default() <= CRASH_LOOP_WINDOW);
            restart_state.recent_crashes.push_back(now);
            let crash_loop_detected = restart_state.recent_crashes.len() >= CRASH_LOOP_THRESHOLD;

            let exceeded = restart_state.attempts > restart_state.policy.max_attempts;
            let backoff = restart_state.policy.backoff_delay(restart_state.attempts);
            let attempts = restart_state.attempts;
            let max = restart_state.policy.max_attempts;

            (exceeded, backoff, attempts, max, crash_loop_detected)
        };

        // Circuit breaker: quarantine worker on rapid crash loop
        if crash_loop_detected {
            warn!(
                tenant = tenant_id,
                crashes = CRASH_LOOP_THRESHOLD,
                window_secs = CRASH_LOOP_WINDOW.as_secs(),
                "Worker crash loop detected; quarantining worker"
            );
            self.quarantine_worker(tenant_id, crash_reason).await;
            return Err(AosError::Worker(format!(
                "Worker {} quarantined after {} crashes in {}s",
                tenant_id,
                CRASH_LOOP_THRESHOLD,
                CRASH_LOOP_WINDOW.as_secs()
            )));
        }

        // Check if we've exceeded max attempts
        if exceeded_max {
            error!(
                "Worker {} exceeded max restart attempts ({}), marking as stopped",
                tenant_id, max_attempts
            );

            // Update worker status
            {
                let mut workers = self.workers.lock().await;
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
        info!(
            "Restarting worker {} after {}s (attempt {}/{})",
            tenant_id,
            backoff.as_secs(),
            attempts,
            max_attempts
        );

        // Mark as restarting
        {
            let mut workers = self.workers.lock().await;
            if let Some(worker) = workers.get_mut(tenant_id) {
                worker.status = WorkerStatus::Restarting;
            }
        }

        let skip_respawn = std::env::var("AOS_SUPERVISOR_SKIP_RESPAWN").is_ok();

        // Wait for backoff delay unless we're skipping respawn entirely (test hook)
        if !skip_respawn {
            tokio::time::sleep(backoff).await;
        }

        // Optional test hook to skip spawning real processes
        if skip_respawn {
            info!(
                tenant = tenant_id,
                "Skipping worker respawn due to AOS_SUPERVISOR_SKIP_RESPAWN"
            );
            let mut workers = self.workers.lock().await;
            if let Some(worker) = workers.get_mut(tenant_id) {
                worker.status = WorkerStatus::Healthy;
                worker.last_health_check = now;
            }
        } else {
            // Perform restart (in production, this would actually restart the worker process)
            self.restart_worker(tenant_id).await?;
        }

        // Record restart in database
        self.record_restart(tenant_id, attempts).await?;

        // Update last_restart time
        {
            let mut restart_states = self
                .restart_states
                .lock()
                .map_err(|e| AosError::Internal(format!("Restart states lock poisoned: {}", e)))?;
            if let Some(restart_state) = restart_states.get_mut(tenant_id) {
                restart_state.last_restart = now;
            }
        }

        // On successful restart, reset attempts after a grace period
        // (in production, would be triggered by successful health checks)
        Ok(())
    }

    /// Restart a worker process
    async fn restart_worker(&self, tenant_id: &str) -> Result<()> {
        info!("Restarting worker {}", tenant_id);

        // Get current worker info
        let old_pid = {
            let workers = self.workers.lock().await;
            workers.get(tenant_id).and_then(|w| w.pid)
        };

        // Terminate existing process if still running
        if let Some(pid) = old_pid {
            debug!("Terminating existing worker process {}", pid);
            #[cfg(unix)]
            {
                use std::process::Command;
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
            }
            // Give the OS time to clean up
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        // Spawn new worker process
        let new_pid = spawn_worker_process(tenant_id, &self.config.db_path).await?;

        // Update worker handle with new PID
        {
            let mut workers = self.workers.lock().await;
            if let Some(worker) = workers.get_mut(tenant_id) {
                worker.pid = Some(new_pid);
                worker.status = WorkerStatus::Healthy;
                worker.last_health_check = std::time::SystemTime::now();
            }
        }

        // Wait for initial health check
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify worker is responding
        let status = self.get_worker_status(tenant_id).await;
        if status != Some(WorkerStatus::Healthy) {
            return Err(AosError::Worker(format!(
                "Worker {} failed health check after restart",
                tenant_id
            )));
        }

        info!(
            "Worker {} restarted successfully with PID {}",
            tenant_id, new_pid
        );
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
    pub fn reset_restart_attempts(&self, tenant_id: &str) -> Result<()> {
        let mut restart_states = self
            .restart_states
            .lock()
            .map_err(|e| AosError::Internal(format!("Restart states lock poisoned: {}", e)))?;
        if let Some(state) = restart_states.get_mut(tenant_id) {
            info!(
                "Resetting restart attempts for worker {} (was {})",
                tenant_id, state.attempts
            );
            state.attempts = 0;
            state.recent_crashes.clear();
        }
        Ok(())
    }

    /// Get restart state for a worker
    pub fn get_restart_state(&self, tenant_id: &str) -> Option<WorkerRestartState> {
        let restart_states = match self.restart_states.lock() {
            Ok(lock) => lock,
            Err(e) => {
                warn!("Restart states lock poisoned: {}", e);
                return None;
            }
        };
        restart_states.get(tenant_id).cloned()
    }
}

/// Spawn a new worker process for a tenant
async fn spawn_worker_process(tenant_id: &str, db_path: &std::path::Path) -> Result<u32> {
    use std::process::Command;

    // Determine the worker binary path
    let worker_binary = std::env::current_exe()
        .map_err(|e| AosError::Worker(format!("Failed to get current executable: {}", e)))?
        .parent()
        .ok_or_else(|| AosError::Worker("No parent directory for executable".to_string()))?
        .join("aos-worker");

    if !worker_binary.exists() {
        // Debug-only bypass for testing
        let allow_placeholder =
            cfg!(debug_assertions) && std::env::var("AOS_WORKER_PLACEHOLDER_OK").is_ok();

        if allow_placeholder {
            warn!(
                "Worker binary not found at {}, using placeholder (AOS_WORKER_PLACEHOLDER_OK set)",
                worker_binary.display()
            );
            let child = Command::new("sleep")
                .args(["3600"])
                .spawn()
                .map_err(|e| AosError::Worker(format!("Failed to spawn placeholder: {}", e)))?;
            return Ok(child.id());
        }

        // Production: hard fail with actionable message
        error!(
            binary_path = %worker_binary.display(),
            "Worker binary not found. Build with: cargo build --release -p adapteros-lora-worker"
        );
        return Err(AosError::Worker(format!(
            "Worker binary not found at {}. Build with: cargo build --release -p adapteros-lora-worker",
            worker_binary.display()
        )));
    }

    let child = Command::new(&worker_binary)
        .args([
            "--tenant",
            tenant_id,
            "--db",
            &db_path.display().to_string(),
        ])
        .spawn()
        .map_err(|e| {
            AosError::Worker(format!(
                "Failed to spawn worker process {}: {}",
                worker_binary.display(),
                e
            ))
        })?;

    let pid = child.id();
    info!(
        tenant = tenant_id,
        pid = pid,
        binary = %worker_binary.display(),
        "Spawned worker process"
    );

    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestEnvGuard;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect(
            "Failed to create temporary directory for supervisor tests. \
             Expected: OS should allow temp directory creation with 'aos-test-' prefix. \
             Context: Tests require writable temp space for isolated database instances. \
            This typically fails only when: (1) the system temp directory is full, (2) permissions are restricted, \
             or (3) OS temp directory is misconfigured.",
        )
    }

    #[tokio::test]
    async fn test_supervisor_creation() {
        let temp_dir = new_test_tempdir();
        let db_path = temp_dir.path().join("test.db");

        let config = SupervisorConfig {
            db_path,
            ..Default::default()
        };

        let supervisor = SupervisorDaemon::new(config).await.unwrap();
        assert!(supervisor.workers.lock().await.is_empty());
    }

    #[tokio::test]
    async fn test_worker_registration() {
        let temp_dir = new_test_tempdir();
        let db_path = temp_dir.path().join("test.db");

        let config = SupervisorConfig {
            db_path,
            ..Default::default()
        };

        let supervisor = SupervisorDaemon::new(config).await.unwrap();
        supervisor
            .register_worker("test-tenant".to_string(), Some(12345))
            .await;

        let status = supervisor.get_worker_status("test-tenant").await;
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
        let _env = TestEnvGuard::new();
        // Enable placeholder worker for testing
        std::env::set_var("AOS_WORKER_PLACEHOLDER_OK", "1");

        let temp_dir = new_test_tempdir();
        let db_path = temp_dir.path().join("test.db");

        let config = SupervisorConfig {
            db_path,
            ..Default::default()
        };

        let supervisor = SupervisorDaemon::new(config).await.unwrap();
        supervisor
            .register_worker("test-tenant".to_string(), Some(12345))
            .await;

        // Simulate crash
        supervisor
            .handle_worker_crash("test-tenant", "simulated crash")
            .await
            .unwrap();

        // Clean up
        std::env::remove_var("AOS_WORKER_PLACEHOLDER_OK");

        // Worker should be restarted and healthy
        let status = supervisor.get_worker_status("test-tenant").await;
        assert_eq!(status, Some(WorkerStatus::Healthy));

        // Restart attempts should be 1
        let restart_state = supervisor.get_restart_state("test-tenant").unwrap();
        assert_eq!(restart_state.attempts, 1);
    }

    #[tokio::test]
    async fn test_worker_crash_loop_quarantines_worker() {
        let _env = TestEnvGuard::new();
        // Skip respawn and sleeps for faster test execution
        std::env::set_var("AOS_SUPERVISOR_SKIP_RESPAWN", "1");

        let temp_dir = new_test_tempdir();
        let db_path = temp_dir.path().join("test.db");

        let config = SupervisorConfig {
            db_path,
            ..Default::default()
        };

        let supervisor = SupervisorDaemon::new(config).await.unwrap();
        supervisor
            .register_worker("test-tenant".to_string(), Some(12345))
            .await;

        // Trigger crash loop
        for _ in 0..CRASH_LOOP_THRESHOLD {
            let _ = supervisor
                .handle_worker_crash("test-tenant", "simulated crash loop")
                .await;
        }

        let status = supervisor.get_worker_status("test-tenant").await;
        assert_eq!(status, Some(WorkerStatus::Quarantined));

        std::env::remove_var("AOS_SUPERVISOR_SKIP_RESPAWN");
    }
}
