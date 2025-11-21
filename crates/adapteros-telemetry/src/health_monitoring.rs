use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

// Re-export canonical HealthStatus from adapteros-core
pub use adapteros_core::HealthStatus;

/// Health check definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub description: String,
    pub category: String,
}

/// Health check state tracked over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthState {
    pub status: HealthStatus,
    pub details: String,
    pub last_updated: SystemTime,
    pub uptime: Duration,
}

/// Aggregated health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub generated_at: SystemTime,
    pub summary_status: HealthStatus,
    pub checks: HashMap<String, HealthState>,
}

/// Monitor responsible for tracking health checks and uptime metrics.
#[derive(Debug, Default)]
pub struct HealthMonitor {
    checks: HashMap<String, HealthCheck>,
    states: HashMap<String, HealthState>,
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_check(&mut self, check: HealthCheck) {
        self.checks.entry(check.name.clone()).or_insert(check);
    }

    pub fn update_status(
        &mut self,
        name: &str,
        status: HealthStatus,
        details: impl Into<String>,
        uptime: Duration,
    ) {
        let state = HealthState {
            status,
            details: details.into(),
            last_updated: SystemTime::now(),
            uptime,
        };
        self.states.insert(name.to_string(), state);
    }

    /// Generate a point-in-time health report.
    pub fn generate_report(&self) -> HealthReport {
        let summary_status = self.states.values().map(|state| state.status).fold(
            HealthStatus::Healthy,
            |acc, status| match (acc, status) {
                (HealthStatus::Unhealthy, _) | (_, HealthStatus::Unhealthy) => {
                    HealthStatus::Unhealthy
                }
                (HealthStatus::Degraded, _) | (_, HealthStatus::Degraded) => HealthStatus::Degraded,
                _ => HealthStatus::Healthy,
            },
        );

        HealthReport {
            generated_at: SystemTime::now(),
            summary_status,
            checks: self.states.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_reflects_worst_status() {
        let mut monitor = HealthMonitor::new();
        monitor.register_check(HealthCheck {
            name: "db".into(),
            description: "Database connectivity".into(),
            category: "infra".into(),
        });
        monitor.update_status(
            "db",
            HealthStatus::Degraded,
            "replica lag",
            Duration::from_secs(120),
        );

        let report = monitor.generate_report();
        assert_eq!(report.summary_status, HealthStatus::Degraded);
        assert!(report.checks.contains_key("db"));
    }
}
