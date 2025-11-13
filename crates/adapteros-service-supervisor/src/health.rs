//! Health check system for services

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info};

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HealthResult {
    Healthy,
    Unhealthy(String),
    Unknown,
}

/// Health check trait
#[async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check(&self) -> HealthResult;
}

/// Health monitor for tracking service health
pub struct HealthMonitor {
    checks: Arc<RwLock<HashMap<String, Box<dyn HealthCheck>>>>,
    statuses: Arc<RwLock<HashMap<String, HealthResult>>>,
    interval_seconds: u64,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(interval_seconds: u64) -> Self {
        Self {
            checks: Arc::new(RwLock::new(HashMap::new())),
            statuses: Arc::new(RwLock::new(HashMap::new())),
            interval_seconds,
        }
    }

    /// Register a health check
    pub async fn register_check(&self, id: String, check: Box<dyn HealthCheck>) {
        self.checks.write().await.insert(id.clone(), check);
        self.statuses.write().await.insert(id, HealthResult::Unknown);
    }

    /// Remove a health check
    pub async fn remove_check(&self, id: &str) {
        self.checks.write().await.remove(id);
        self.statuses.write().await.remove(id);
    }

    /// Get current health status for a service
    pub async fn get_status(&self, id: &str) -> Option<HealthResult> {
        self.statuses.read().await.get(id).cloned()
    }

    /// Get all health statuses
    pub async fn get_all_statuses(&self) -> HashMap<String, HealthResult> {
        self.statuses.read().await.clone()
    }

    /// Start the health monitoring loop
    pub fn start_monitoring(&self) {
        let checks = Arc::clone(&self.checks);
        let statuses = Arc::clone(&self.statuses);
        let interval_duration = Duration::from_secs(self.interval_seconds);

        tokio::spawn(async move {
            let mut interval = interval(interval_duration);

            loop {
                interval.tick().await;

                let check_ids: Vec<String> = checks.read().await.keys().cloned().collect();

                for id in check_ids {
                    if let Some(check) = checks.read().await.get(&id) {
                        let result = check.check().await;
                        let mut statuses_write = statuses.write().await;

                        let previous_result = statuses_write.get(&id).cloned();
                        statuses_write.insert(id.clone(), result.clone());

                        // Log status changes
                        match (&previous_result, &result) {
                            (Some(HealthResult::Healthy), HealthResult::Unhealthy(msg)) => {
                                error!("Service {} became unhealthy: {}", id, msg);
                            }
                            (Some(HealthResult::Unhealthy(_)), HealthResult::Healthy) => {
                                info!("Service {} recovered to healthy", id);
                            }
                            (Some(HealthResult::Unknown), HealthResult::Healthy) => {
                                info!("Service {} is now healthy", id);
                            }
                            _ => {
                                debug!("Service {} health: {:?}", id, result);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Check overall system health
    pub async fn system_health(&self) -> HealthResult {
        let statuses = self.statuses.read().await;

        if statuses.is_empty() {
            return HealthResult::Unknown;
        }

        let unhealthy_count = statuses.values()
            .filter(|result| matches!(result, HealthResult::Unhealthy(_)))
            .count();

        if unhealthy_count > 0 {
            HealthResult::Unhealthy(format!("{} services unhealthy", unhealthy_count))
        } else {
            HealthResult::Healthy
        }
    }
}

/// Health endpoint response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub services: HashMap<String, String>,
    pub overall: String,
}

impl HealthResponse {
    /// Create a health response from the monitor
    pub async fn from_monitor(monitor: &HealthMonitor) -> Self {
        let statuses = monitor.get_all_statuses().await;
        let system_health = monitor.system_health().await;

        let services = statuses.into_iter()
            .map(|(id, result)| {
                let status_str = match &result {
                    HealthResult::Healthy => "healthy".to_string(),
                    HealthResult::Unhealthy(msg) => format!("unhealthy: {}", msg),
                    HealthResult::Unknown => "unknown".to_string(),
                };
                (id, status_str)
            })
            .collect();

        let overall = match &system_health {
            HealthResult::Healthy => "healthy".to_string(),
            HealthResult::Unhealthy(msg) => format!("unhealthy: {}", msg),
            HealthResult::Unknown => "unknown".to_string(),
        };

        Self {
            status: match system_health {
                HealthResult::Healthy => "ok".to_string(),
                _ => "error".to_string(),
            },
            timestamp: chrono::Utc::now(),
            services,
            overall,
        }
    }
}
