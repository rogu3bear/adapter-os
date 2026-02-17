//! MLX Backend Monitoring and Alerting
//!
//! Provides comprehensive monitoring, alerting, and observability for the MLX
//! backend resilience system.

use crate::backend::{BackendHealth, MLXFFIBackend};
use adapteros_lora_kernel_api::FusedKernels;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// Monitoring configuration
#[derive(Debug, Clone)]
pub struct MonitoringConfig {
    /// Health check interval
    pub health_check_interval: Duration,
    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
    /// Metrics export settings
    pub metrics_enabled: bool,
}

/// Alert threshold configuration
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    /// Maximum consecutive failures before warning
    pub warning_failure_threshold: u32,
    /// Maximum consecutive failures before critical alert
    pub critical_failure_threshold: u32,
    /// Minimum success rate percentage before alert
    pub min_success_rate_percent: f32,
    /// Maximum recovery time in seconds
    pub max_recovery_time_secs: u64,
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub timestamp: Instant,
    pub backend_name: String,
    pub status: HealthStatus,
    pub health_score: f32,
    pub metrics: HealthMetrics,
    pub issues: Vec<String>,
}

/// Health status enum
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Down,
}

/// Health metrics
#[derive(Debug, Clone)]
pub struct HealthMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub success_rate: f32,
    pub current_failure_streak: u32,
    pub average_response_time_ms: f32,
    pub circuit_breaker_state: String,
    // MLX-specific metrics
    pub average_inference_time_ms: f32,
    pub peak_memory_usage_mb: f32,
    pub active_adapters: usize,
    pub cache_hit_rate: f32,
}

/// Alert types
#[derive(Debug, Clone, PartialEq)]
pub enum AlertType {
    CircuitBreakerOpened,
    BackendDown,
    RecoveryFailed,
    SuccessRateLow,
    RecoveryTimeExceeded,
}

/// Alert structure
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub backend_name: String,
    pub message: String,
    pub timestamp: Instant,
    pub context: std::collections::HashMap<String, String>,
}

/// Alert severity
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// MLX Backend Monitor
pub struct MLXMonitor {
    backend: Arc<MLXFFIBackend>,
    config: MonitoringConfig,
    last_health_check: Option<HealthCheckResult>,
    active_alerts: Vec<Alert>,
}

impl MLXMonitor {
    /// Create new monitor
    pub fn new(backend: Arc<MLXFFIBackend>, config: MonitoringConfig) -> Self {
        Self {
            backend,
            config,
            last_health_check: None,
            active_alerts: Vec::new(),
        }
    }

    /// Perform health check
    pub fn health_check(&mut self) -> HealthCheckResult {
        let start_time = Instant::now();
        let backend_name = "mlx".to_string();

        // Get current health status
        let health = self.backend.health_status();
        let _device_info = self.backend.device_name();

        // Calculate metrics
        let total_requests = health.total_requests;
        let successful_requests = health.successful_requests;
        let failed_requests = health.failed_requests;

        let success_rate = if total_requests > 0 {
            (successful_requests as f32 / total_requests as f32) * 100.0
        } else {
            100.0
        };

        // Determine health status
        let mut issues = Vec::new();
        let status = self.determine_health_status(&health, success_rate, &mut issues);

        // Calculate health score (0-100)
        let health_score = self.calculate_health_score(&health, success_rate);

        // Get MLX-specific metrics if available
        let mlx_metrics = if let Some(ref monitor) = &self.backend.monitor {
            if let Ok(mlx_monitor) = monitor.lock() {
                Some(mlx_monitor.performance_metrics())
            } else {
                None
            }
        } else {
            None
        };

        let metrics = HealthMetrics {
            total_requests,
            successful_requests,
            failed_requests,
            success_rate,
            current_failure_streak: health.current_failure_streak,
            average_response_time_ms: mlx_metrics
                .as_ref()
                .map(|m| m.average_latency_ms)
                .unwrap_or(0.0),
            circuit_breaker_state: if health.operational {
                "Operational"
            } else {
                "Non-operational"
            }
            .to_string(),
            average_inference_time_ms: mlx_metrics
                .as_ref()
                .map(|m| m.average_latency_ms)
                .unwrap_or(0.0),
            peak_memory_usage_mb: mlx_metrics
                .as_ref()
                .map(|m| m.peak_memory_usage_mb)
                .unwrap_or(0.0),
            active_adapters: self.backend.adapters.load().len(),
            cache_hit_rate: mlx_metrics
                .as_ref()
                .map(|m| m.cache_hit_rate)
                .unwrap_or(0.0),
        };

        let result = HealthCheckResult {
            timestamp: start_time,
            backend_name,
            status,
            health_score,
            metrics,
            issues,
        };

        self.last_health_check = Some(result.clone());
        self.check_alerts(&result);

        result
    }

    /// Determine health status based on metrics
    fn determine_health_status(
        &self,
        health: &BackendHealth,
        success_rate: f32,
        issues: &mut Vec<String>,
    ) -> HealthStatus {
        // Check for critical issues
        if !health.operational {
            issues.push("Backend marked non-operational".to_string());
            return HealthStatus::Down;
        }

        if health.current_failure_streak >= self.config.alert_thresholds.critical_failure_threshold
        {
            issues.push(format!(
                "{} consecutive failures",
                health.current_failure_streak
            ));
            return HealthStatus::Critical;
        }

        if success_rate < self.config.alert_thresholds.min_success_rate_percent {
            issues.push(format!(
                "Success rate {:.1}% below threshold {:.1}%",
                success_rate, self.config.alert_thresholds.min_success_rate_percent
            ));
            return HealthStatus::Critical;
        }

        // Check for warnings
        if health.current_failure_streak >= self.config.alert_thresholds.warning_failure_threshold {
            issues.push(format!(
                "{} consecutive failures (warning threshold)",
                health.current_failure_streak
            ));
            return HealthStatus::Warning;
        }

        if health.stub_fallback_active {
            issues.push("Operating in stub fallback mode".to_string());
            return HealthStatus::Warning;
        }

        HealthStatus::Healthy
    }

    /// Calculate health score (0-100)
    fn calculate_health_score(&self, health: &BackendHealth, success_rate: f32) -> f32 {
        let mut score = 100.0;

        // Deduct for failures
        score -= (health.current_failure_streak as f32) * 5.0;

        // Deduct for low success rate
        if success_rate < 95.0 {
            score -= (100.0 - success_rate) * 2.0;
        }

        // Deduct for stub fallback
        if health.stub_fallback_active {
            score -= 20.0;
        }

        // Deduct for circuit breaker open
        if !health.operational {
            score -= 50.0;
        }

        score.clamp(0.0, 100.0)
    }

    /// Check for alerts based on health check result
    fn check_alerts(&mut self, result: &HealthCheckResult) {
        let mut new_alerts = Vec::new();

        // Circuit breaker opened
        if matches!(result.status, HealthStatus::Critical | HealthStatus::Down)
            && !self.has_active_alert(AlertType::CircuitBreakerOpened)
        {
            new_alerts.push(Alert {
                alert_type: AlertType::CircuitBreakerOpened,
                severity: AlertSeverity::Critical,
                backend_name: result.backend_name.clone(),
                message: "MLX backend circuit breaker opened".to_string(),
                timestamp: Instant::now(),
                context: [
                    (
                        "failure_streak".to_string(),
                        result.metrics.current_failure_streak.to_string(),
                    ),
                    (
                        "success_rate".to_string(),
                        format!("{:.1}", result.metrics.success_rate),
                    ),
                ]
                .into(),
            });
        }

        // Backend down
        if matches!(result.status, HealthStatus::Down)
            && !self.has_active_alert(AlertType::BackendDown)
        {
            new_alerts.push(Alert {
                alert_type: AlertType::BackendDown,
                severity: AlertSeverity::Critical,
                backend_name: result.backend_name.clone(),
                message: "MLX backend is down".to_string(),
                timestamp: Instant::now(),
                context: [
                    (
                        "total_requests".to_string(),
                        result.metrics.total_requests.to_string(),
                    ),
                    (
                        "failed_requests".to_string(),
                        result.metrics.failed_requests.to_string(),
                    ),
                ]
                .into(),
            });
        }

        // Low success rate
        if result.metrics.success_rate < self.config.alert_thresholds.min_success_rate_percent
            && !self.has_active_alert(AlertType::SuccessRateLow)
        {
            new_alerts.push(Alert {
                alert_type: AlertType::SuccessRateLow,
                severity: AlertSeverity::Warning,
                backend_name: result.backend_name.clone(),
                message: format!(
                    "MLX backend success rate {:.1}% below threshold",
                    result.metrics.success_rate
                ),
                timestamp: Instant::now(),
                context: [
                    (
                        "success_rate".to_string(),
                        format!("{:.1}", result.metrics.success_rate),
                    ),
                    (
                        "threshold".to_string(),
                        self.config
                            .alert_thresholds
                            .min_success_rate_percent
                            .to_string(),
                    ),
                ]
                .into(),
            });
        }

        // Recovery alerts
        if let Some(last_check) = &self.last_health_check {
            if matches!(
                last_check.status,
                HealthStatus::Critical | HealthStatus::Down
            ) && matches!(result.status, HealthStatus::Healthy | HealthStatus::Warning)
            {
                new_alerts.push(Alert {
                    alert_type: AlertType::RecoveryFailed, // Actually recovered
                    severity: AlertSeverity::Info,
                    backend_name: result.backend_name.clone(),
                    message: "MLX backend recovered".to_string(),
                    timestamp: Instant::now(),
                    context: [
                        (
                            "recovery_time_ms".to_string(),
                            result
                                .timestamp
                                .duration_since(last_check.timestamp)
                                .as_millis()
                                .to_string(),
                        ),
                        (
                            "health_score".to_string(),
                            format!("{:.1}", result.health_score),
                        ),
                    ]
                    .into(),
                });
            }
        }

        // Add new alerts
        for alert in new_alerts {
            self.send_alert(&alert);
            self.active_alerts.push(alert);
        }

        // Clean up resolved alerts
        let resolved_alerts: Vec<_> = self
            .active_alerts
            .iter()
            .filter(|alert| self.is_alert_resolved(alert, result))
            .cloned()
            .collect();
        self.active_alerts
            .retain(|alert| !resolved_alerts.contains(alert));
    }

    /// Check if alert type is already active
    fn has_active_alert(&self, alert_type: AlertType) -> bool {
        self.active_alerts
            .iter()
            .any(|a| std::mem::discriminant(&a.alert_type) == std::mem::discriminant(&alert_type))
    }

    /// Check if alert is resolved
    fn is_alert_resolved(&self, alert: &Alert, result: &HealthCheckResult) -> bool {
        match alert.alert_type {
            AlertType::CircuitBreakerOpened => {
                matches!(result.status, HealthStatus::Healthy | HealthStatus::Warning)
            }
            AlertType::BackendDown => {
                matches!(
                    result.status,
                    HealthStatus::Healthy | HealthStatus::Warning | HealthStatus::Critical
                )
            }
            AlertType::SuccessRateLow => {
                result.metrics.success_rate >= self.config.alert_thresholds.min_success_rate_percent
            }
            _ => false,
        }
    }

    /// Send alert (would integrate with actual alerting system)
    fn send_alert(&self, alert: &Alert) {
        let severity_str = match alert.severity {
            AlertSeverity::Info => "INFO",
            AlertSeverity::Warning => "WARN",
            AlertSeverity::Error => "ERROR",
            AlertSeverity::Critical => "CRIT",
        };

        match alert.severity {
            AlertSeverity::Info => info!(
                backend = %alert.backend_name,
                alert_type = ?alert.alert_type,
                severity = %severity_str,
                "MLX alert: {}", alert.message
            ),
            AlertSeverity::Warning => warn!(
                backend = %alert.backend_name,
                alert_type = ?alert.alert_type,
                severity = %severity_str,
                "MLX alert: {}", alert.message
            ),
            AlertSeverity::Error | AlertSeverity::Critical => error!(
                backend = %alert.backend_name,
                alert_type = ?alert.alert_type,
                severity = %severity_str,
                "MLX alert: {}", alert.message
            ),
        };

        // In real implementation, would send to PagerDuty, Slack, etc.
        // For now, just log
    }

    /// Get current alerts
    pub fn active_alerts(&self) -> &[Alert] {
        &self.active_alerts
    }

    /// Export metrics (would integrate with Prometheus, etc.)
    pub fn export_metrics(&self) -> String {
        if !self.config.metrics_enabled {
            return String::new();
        }

        let health = self.backend.health_status();

        format!(
            "# HELP mlx_backend_requests_total Total requests processed\n\
             # TYPE mlx_backend_requests_total counter\n\
             mlx_backend_requests_total {}\n\
             \n\
             # HELP mlx_backend_requests_successful Successful requests\n\
             # TYPE mlx_backend_requests_successful counter\n\
             mlx_backend_requests_successful {}\n\
             \n\
             # HELP mlx_backend_success_rate Current success rate percentage\n\
             # TYPE mlx_backend_success_rate gauge\n\
             mlx_backend_success_rate {:.2}\n\
             \n\
             # HELP mlx_backend_health_score Overall health score (0-100)\n\
             # TYPE mlx_backend_health_score gauge\n\
             mlx_backend_health_score {:.1}\n",
            health.total_requests,
            health.successful_requests,
            if health.total_requests > 0 {
                (health.successful_requests as f32 / health.total_requests as f32) * 100.0
            } else {
                100.0
            },
            self.calculate_health_score(
                &health,
                if health.total_requests > 0 {
                    (health.successful_requests as f32 / health.total_requests as f32) * 100.0
                } else {
                    100.0
                }
            )
        )
    }

    /// Get current performance metrics from the backend
    pub fn performance_metrics(&self) -> crate::backend::PerformanceMetrics {
        self.backend.performance_metrics.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_monitor_creation() {
        let backend = Arc::new(MLXFFIBackend::new(crate::MLXFFIModel {
            model: std::ptr::null_mut(),
            config: crate::ModelConfig {
                hidden_size: 4096,
                num_hidden_layers: 32,
                num_attention_heads: 32,
                num_key_value_heads: 8,
                intermediate_size: 11008,
                vocab_size: 32000,
                max_position_embeddings: 32768,
                rope_theta: 10000.0,
            },
            inference_lock: parking_lot::Mutex::new(()),
            health: Arc::new(std::sync::Mutex::new(crate::ModelHealth {
                operational: true,
                consecutive_failures: 0,
                last_success: None,
                last_failure: None,
                circuit_breaker: crate::CircuitBreakerState::Closed,
            })),
            model_path: std::path::PathBuf::new(),
            tokenizer: None,
            kv_cache: None,
        }));

        let config = MonitoringConfig {
            health_check_interval: Duration::from_secs(60),
            alert_thresholds: AlertThresholds {
                warning_failure_threshold: 2,
                critical_failure_threshold: 5,
                min_success_rate_percent: 95.0,
                max_recovery_time_secs: 300,
            },
            metrics_enabled: true,
        };

        let monitor = MLXMonitor::new(backend, config);
        assert_eq!(monitor.active_alerts().len(), 0);
    }
}
