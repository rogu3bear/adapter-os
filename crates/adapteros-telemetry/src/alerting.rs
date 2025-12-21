use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

/// Severity of an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Comparison operator for alert rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertComparator {
    GreaterThan,
    LessThan,
}

/// Notification channel definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub channel_type: String,
    pub target: String,
}

/// Escalation policy for alerts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationPolicy {
    pub repeat_interval: Duration,
    pub channels: Vec<NotificationChannel>,
}

impl EscalationPolicy {
    pub fn notify_channels(&self) -> &[NotificationChannel] {
        &self.channels
    }
}

/// Alert rule definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub name: String,
    pub metric: String,
    pub comparator: AlertComparator,
    pub threshold: f64,
    pub severity: AlertSeverity,
    pub escalation: EscalationPolicy,
}

/// Record for triggered alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRecord {
    pub rule_name: String,
    pub metric: String,
    pub value: f64,
    pub severity: AlertSeverity,
    pub triggered_at: SystemTime,
    pub notifications: Vec<NotificationChannel>,
}

/// Production alerting engine with history retention.
#[derive(Debug)]
pub struct AlertingEngine {
    rules: Vec<AlertRule>,
    history: VecDeque<AlertRecord>,
    history_limit: usize,
}

impl AlertingEngine {
    pub fn new(history_limit: usize) -> Self {
        Self {
            rules: Vec::new(),
            history: VecDeque::with_capacity(history_limit),
            history_limit,
        }
    }

    pub fn register_rule(&mut self, rule: AlertRule) {
        self.rules.push(rule);
    }

    /// Evaluate a metric and return triggered alerts.
    pub fn evaluate_metric(&mut self, metric: &str, value: f64) -> Vec<AlertRecord> {
        let mut triggered = Vec::new();
        for rule in &self.rules {
            if rule.metric != metric {
                continue;
            }
            let condition = match rule.comparator {
                AlertComparator::GreaterThan => value > rule.threshold,
                AlertComparator::LessThan => value < rule.threshold,
            };
            if condition {
                let record = AlertRecord {
                    rule_name: rule.name.clone(),
                    metric: metric.to_string(),
                    value,
                    severity: rule.severity,
                    triggered_at: SystemTime::now(),
                    notifications: rule.escalation.channels.clone(),
                };
                triggered.push(record);
            }
        }
        for record in &triggered {
            self.push_history(record.clone());
        }
        triggered
    }

    /// Get recent alerts up to the configured history limit.
    pub fn recent_alerts(&self) -> impl Iterator<Item = &AlertRecord> {
        self.history.iter().rev()
    }

    fn push_history(&mut self, record: AlertRecord) {
        if self.history.len() == self.history_limit {
            self.history.pop_front();
        }
        self.history.push_back(record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_alert_and_records_history() {
        let mut engine = AlertingEngine::new(10);
        engine.register_rule(AlertRule {
            name: "latency".into(),
            metric: "latency_ms".into(),
            comparator: AlertComparator::GreaterThan,
            threshold: 500.0,
            severity: AlertSeverity::Critical,
            escalation: EscalationPolicy {
                repeat_interval: Duration::from_secs(60),
                channels: vec![NotificationChannel {
                    channel_type: "pagerduty".into(),
                    target: "incident-team".into(),
                }],
            },
        });

        let alerts = engine.evaluate_metric("latency_ms", 800.0);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Critical);
        assert_eq!(engine.recent_alerts().count(), 1);
    }
}
