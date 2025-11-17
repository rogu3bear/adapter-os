use crate::{
    alerting::{AlertComparator, AlertSeverity},
    health_monitoring::HealthStatus,
    unified_events::{
        EventType, LogLevel, TelemetryEvent as UnifiedTelemetryEvent, TelemetryEventBuilder,
    },
    TelemetryWriter,
};
use adapteros_core::{identity::IdentityEnvelope, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};

const HEALTH_CHECK_EVENT: &str = "monitoring.health_check";
const POLICY_VIOLATION_EVENT: &str = "monitoring.policy_violation_alert";

pub trait TelemetrySink: Send + Sync + Clone {
    fn log_event(&self, event: UnifiedTelemetryEvent) -> Result<()>;
}

impl TelemetrySink for TelemetryWriter {
    fn log_event(&self, event: UnifiedTelemetryEvent) -> Result<()> {
        TelemetryWriter::log_event(self, event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthCheckEventPayload {
    pub check_name: String,
    pub status: HealthStatus,
    pub summary: String,
    pub details: Option<String>,
    pub latency_ms: Option<f64>,
    pub observed_value: Option<f64>,
    pub expected_range: Option<ThresholdRange>,
    pub tags: HashMap<String, String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThresholdRange {
    pub minimum: Option<f64>,
    pub warning: Option<f64>,
    pub critical: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceAlertPayload {
    pub metric: String,
    pub value: f64,
    pub threshold: f64,
    pub comparator: AlertComparator,
    pub severity: AlertSeverity,
    pub unit: String,
    pub evaluation_window_secs: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyViolationAlertPayload {
    pub policy: String,
    pub violation_type: String,
    pub description: String,
    pub severity: AlertSeverity,
    pub impacted_tenant: Option<String>,
    pub remediation: Option<String>,
    pub evidence_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryPressureAlertPayload {
    pub severity: AlertSeverity,
    pub pressure_percent: f64,
    pub memory_total_bytes: u64,
    pub memory_used_bytes: u64,
    pub processes: Vec<MemoryProcessSample>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryProcessSample {
    pub process_name: String,
    pub pid: u32,
    pub rss_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct MonitoringTelemetry<S: TelemetrySink> {
    sink: S,
    component: Option<String>,
}

impl<S: TelemetrySink> MonitoringTelemetry<S> {
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            component: None,
        }
    }

    pub fn with_component(mut self, component: impl Into<String>) -> Self {
        self.component = Some(component.into());
        self
    }

    pub fn log_health_check(&self, payload: HealthCheckEventPayload) -> Result<()> {
        self.log_with_metadata(
            EventType::Custom(HEALTH_CHECK_EVENT.to_string()),
            LogLevel::Info,
            format!(
                "Health check '{}' reported {:?}",
                payload.check_name, payload.status
            ),
            payload,
        )
    }

    pub fn log_performance_alert(&self, payload: PerformanceAlertPayload) -> Result<()> {
        self.log_with_metadata(
            EventType::PerformanceAlert,
            level_for(payload.severity),
            format!("Performance alert for '{}'", payload.metric),
            payload,
        )
    }

    pub fn log_policy_violation_alert(&self, payload: PolicyViolationAlertPayload) -> Result<()> {
        self.log_with_metadata(
            EventType::Custom(POLICY_VIOLATION_EVENT.to_string()),
            level_for(payload.severity),
            format!("Policy violation '{}' detected", payload.policy),
            payload,
        )
    }

    pub fn log_memory_pressure_alert(&self, payload: MemoryPressureAlertPayload) -> Result<()> {
        self.log_with_metadata(
            EventType::MemoryPressure,
            level_for(payload.severity),
            format!("Memory pressure at {:.2}%", payload.pressure_percent),
            payload,
        )
    }

    fn log_with_metadata<T: Serialize>(
        &self,
        event_type: EventType,
        level: LogLevel,
        message: String,
        metadata: T,
    ) -> Result<()> {
        use adapteros_core::{Domain, Purpose};

        let identity = IdentityEnvelope::new(
            "system".to_string(),
            Domain::Telemetry,
            Purpose::Maintenance,
            IdentityEnvelope::default_revision(),
        );
        let mut builder = TelemetryEventBuilder::new(event_type, level, message, identity);
        if let Some(component) = &self.component {
            builder = builder.component(component.clone());
        }
        self.sink
            .log_event(builder.metadata(serde_json::to_value(metadata)?).build())
    }
}

fn level_for(severity: AlertSeverity) -> LogLevel {
    match severity {
        AlertSeverity::Info => LogLevel::Info,
        AlertSeverity::Warning => LogLevel::Warn,
        AlertSeverity::Critical => LogLevel::Critical,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceThreshold {
    pub metric: String,
    pub warning_threshold: f64,
    pub critical_threshold: f64,
    pub comparator: AlertComparator,
    pub unit: String,
}

#[derive(Debug, Clone)]
pub struct PerformanceThresholdMonitor<S: TelemetrySink> {
    telemetry: MonitoringTelemetry<S>,
    thresholds: HashMap<String, PerformanceThreshold>,
    evaluation_window: Duration,
}

impl<S: TelemetrySink> PerformanceThresholdMonitor<S> {
    pub fn new(telemetry: MonitoringTelemetry<S>, evaluation_window: Duration) -> Self {
        Self {
            telemetry,
            thresholds: HashMap::new(),
            evaluation_window,
        }
    }

    pub fn register_threshold(&mut self, threshold: PerformanceThreshold) {
        self.thresholds.insert(threshold.metric.clone(), threshold);
    }

    pub fn evaluate(&self, metric: &str, value: f64) -> Result<Option<AlertSeverity>> {
        let Some(threshold) = self.thresholds.get(metric) else {
            return Ok(None);
        };
        let severity = match threshold.comparator {
            AlertComparator::GreaterThan => severity_for_greater(
                value,
                threshold.warning_threshold,
                threshold.critical_threshold,
            ),
            AlertComparator::LessThan => severity_for_less(
                value,
                threshold.warning_threshold,
                threshold.critical_threshold,
            ),
        };
        if let Some(severity) = severity {
            let payload = PerformanceAlertPayload {
                metric: threshold.metric.clone(),
                value,
                threshold: if severity == AlertSeverity::Critical {
                    threshold.critical_threshold
                } else {
                    threshold.warning_threshold
                },
                comparator: threshold.comparator,
                severity,
                unit: threshold.unit.clone(),
                evaluation_window_secs: self.evaluation_window.as_secs_f64(),
                timestamp: Utc::now(),
            };
            self.telemetry.log_performance_alert(payload)?;
        }
        Ok(severity)
    }
}

fn severity_for_greater(value: f64, warning: f64, critical: f64) -> Option<AlertSeverity> {
    if value >= critical {
        Some(AlertSeverity::Critical)
    } else if value >= warning {
        Some(AlertSeverity::Warning)
    } else {
        None
    }
}

fn severity_for_less(value: f64, warning: f64, critical: f64) -> Option<AlertSeverity> {
    if value <= critical {
        Some(AlertSeverity::Critical)
    } else if value <= warning {
        Some(AlertSeverity::Warning)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use std::sync::Mutex;

    #[derive(Debug, Clone, Default)]
    struct TestSink(Arc<Mutex<Vec<UnifiedTelemetryEvent>>>);

    impl TelemetrySink for TestSink {
        fn log_event(&self, event: UnifiedTelemetryEvent) -> Result<()> {
            self.0.lock().unwrap().push(event);
            Ok(())
        }
    }

    fn metadata<T: DeserializeOwned>(event: &UnifiedTelemetryEvent) -> T {
        serde_json::from_value(event.metadata.clone().unwrap()).unwrap()
    }

    #[test]
    fn health_check_round_trip() -> Result<()> {
        let sink = TestSink::default();
        let telemetry = MonitoringTelemetry::new(sink.clone()).with_component("scheduler");
        let payload = HealthCheckEventPayload {
            check_name: "db".into(),
            status: HealthStatus::Healthy,
            summary: "ok".into(),
            details: Some("replica lag < 10ms".into()),
            latency_ms: Some(7.0),
            observed_value: Some(0.07),
            expected_range: Some(ThresholdRange {
                minimum: Some(0.0),
                warning: Some(0.5),
                critical: Some(0.8),
            }),
            tags: HashMap::from([(String::from("region"), String::from("us-east-1"))]),
            timestamp: Utc::now(),
        };
        telemetry.log_health_check(payload.clone())?;
        let events = sink.0.lock().unwrap().clone();
        assert_eq!(events[0].event_type, HEALTH_CHECK_EVENT);
        assert_eq!(events[0].component.as_deref(), Some("scheduler"));
        let parsed: HealthCheckEventPayload = metadata(&events[0]);
        assert_eq!(parsed.check_name, payload.check_name);
        assert_eq!(parsed.status, payload.status);
        assert_eq!(parsed.tags.get("region"), Some(&"us-east-1".to_string()));
        Ok(())
    }

    #[test]
    fn performance_monitor_covers_thresholds_and_missing_metric() -> Result<()> {
        let sink = TestSink::default();
        let telemetry = MonitoringTelemetry::new(sink.clone());
        let mut monitor = PerformanceThresholdMonitor::new(telemetry, Duration::from_secs(30));
        monitor.register_threshold(PerformanceThreshold {
            metric: "latency".into(),
            warning_threshold: 400.0,
            critical_threshold: 700.0,
            comparator: AlertComparator::GreaterThan,
            unit: "ms".into(),
        });
        monitor.register_threshold(PerformanceThreshold {
            metric: "throughput".into(),
            warning_threshold: 120.0,
            critical_threshold: 80.0,
            comparator: AlertComparator::LessThan,
            unit: "tps".into(),
        });
        assert_eq!(
            monitor.evaluate("latency", 720.0)?,
            Some(AlertSeverity::Critical)
        );
        assert_eq!(
            monitor.evaluate("throughput", 90.0)?,
            Some(AlertSeverity::Warning)
        );
        assert!(monitor.evaluate("cpu", 0.5)?.is_none());
        let events = sink.0.lock().unwrap().clone();
        assert_eq!(events.len(), 2);
        assert_eq!(
            metadata::<PerformanceAlertPayload>(&events[0]).severity,
            AlertSeverity::Critical
        );
        assert_eq!(
            metadata::<PerformanceAlertPayload>(&events[1]).severity,
            AlertSeverity::Warning
        );
        Ok(())
    }

    #[test]
    fn policy_and_memory_alerts_are_emitted() -> Result<()> {
        let sink = TestSink::default();
        let telemetry = MonitoringTelemetry::new(sink.clone());
        let policy_payload = PolicyViolationAlertPayload {
            policy: "egress".into(),
            violation_type: "unauthorized_destination".into(),
            description: "attempted to contact external host".into(),
            severity: AlertSeverity::Critical,
            impacted_tenant: Some("tenant-01".into()),
            remediation: Some("block address".into()),
            evidence_id: Some("evidence-7".into()),
            timestamp: Utc::now(),
        };
        telemetry.log_policy_violation_alert(policy_payload.clone())?;
        let memory_payload = MemoryPressureAlertPayload {
            severity: AlertSeverity::Warning,
            pressure_percent: 82.5,
            memory_total_bytes: 64 * 1024 * 1024 * 1024,
            memory_used_bytes: 52 * 1024 * 1024 * 1024,
            processes: vec![MemoryProcessSample {
                process_name: "adapteros".into(),
                pid: 4242,
                rss_bytes: 12 * 1024 * 1024 * 1024,
            }],
            timestamp: Utc::now(),
        };
        telemetry.log_memory_pressure_alert(memory_payload.clone())?;
        let events = sink.0.lock().unwrap().clone();
        assert_eq!(events[0].event_type, POLICY_VIOLATION_EVENT);
        assert_eq!(events[1].event_type, EventType::MemoryPressure.as_str());
        assert_eq!(events[0].level, LogLevel::Critical);
        assert_eq!(events[1].level, LogLevel::Warn);
        assert_eq!(
            metadata::<PolicyViolationAlertPayload>(&events[0]),
            policy_payload
        );
        assert_eq!(
            metadata::<MemoryPressureAlertPayload>(&events[1]),
            memory_payload
        );
        let sample_json = serde_json::to_string(&memory_payload.processes[0])?;
        assert_eq!(
            serde_json::from_str::<MemoryProcessSample>(&sample_json)?,
            memory_payload.processes[0]
        );
        Ok(())
    }
}
