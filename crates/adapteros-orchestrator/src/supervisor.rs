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
use adapteros_registry::AdapterRegistry;
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
    /// Health checker
    _health_checker: Arc<HealthChecker>,
    /// Policy hash watcher
    policy_watcher: Option<Arc<PolicyHashWatcher>>,
    /// Quarantine manager
    quarantine_manager: Arc<Mutex<QuarantineManager>>,
    /// Adapter registry
    adapter_registry: Option<Arc<AdapterRegistry>>,
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

        let db = Arc::new(Db::connect(config.db_path.to_str().unwrap()).await.map_err(
            |e| AosError::Database(format!("Failed to connect to database: {}", e)),
        )?);

        let health_checker = Arc::new(HealthChecker::new(Duration::from_secs(
            config.health_check_interval_secs,
        )));

        Ok(Self {
            config,
            workers: Arc::new(Mutex::new(HashMap::new())),
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
    pub fn with_adapter_registry(mut self, registry: Arc<AdapterRegistry>) -> Self {
        self.adapter_registry = Some(registry);
        self
    }

    /// Run the supervisor daemon
    pub async fn run(&self) -> Result<()> {
        info!("Starting supervisor daemon");

        let mut health_interval = interval(Duration::from_secs(
            self.config.health_check_interval_secs,
        ));
        let mut policy_interval = interval(Duration::from_secs(
            self.config.policy_check_interval_secs,
        ));
        let mut adapter_interval = interval(Duration::from_secs(
            self.config.adapter_check_interval_secs,
        ));
        let mut memory_interval = interval(Duration::from_secs(
            self.config.memory_check_interval_secs,
        ));

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
                debug!("Worker {} (PID {}) status: {:?}", tenant_id, pid, worker.status);
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
        if let Some(ref registry) = self.adapter_registry {
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
}

