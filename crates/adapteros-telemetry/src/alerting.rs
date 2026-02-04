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

impl AlertRecord {
    /// Get severity as string for display.
    pub fn severity_str(&self) -> &'static str {
        match self.severity {
            AlertSeverity::Info => "INFO",
            AlertSeverity::Warning => "WARNING",
            AlertSeverity::Critical => "CRITICAL",
        }
    }
}

/// Error type for alert dispatch failures.
#[derive(Debug, thiserror::Error)]
pub enum AlertDispatchError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("Invalid webhook URL: {0}")]
    InvalidUrl(String),
}

/// Dispatcher for sending alert notifications to external services.
///
/// Supports Slack, PagerDuty, and generic webhook channels.
pub struct AlertDispatcher {
    client: reqwest::Client,
}

impl AlertDispatcher {
    /// Create a new AlertDispatcher with default timeout.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    /// Send alert to all notification channels in the record.
    pub async fn dispatch(&self, record: &AlertRecord) -> Result<(), AlertDispatchError> {
        for channel in &record.notifications {
            match channel.channel_type.as_str() {
                "slack" => self.send_slack(&channel.target, record).await?,
                "pagerduty" => self.send_pagerduty(&channel.target, record).await?,
                "webhook" => self.send_webhook(&channel.target, record).await?,
                other => {
                    tracing::warn!(channel_type = %other, "unknown notification channel type, skipping");
                }
            }
        }
        Ok(())
    }

    /// Send alert to Slack webhook.
    async fn send_slack(
        &self,
        webhook_url: &str,
        record: &AlertRecord,
    ) -> Result<(), AlertDispatchError> {
        let emoji = match record.severity {
            AlertSeverity::Critical => ":rotating_light:",
            AlertSeverity::Warning => ":warning:",
            AlertSeverity::Info => ":information_source:",
        };

        let payload = serde_json::json!({
            "text": format!(
                "{} *[{}]* {} | `{}` = {:.2}",
                emoji,
                record.severity_str(),
                record.rule_name,
                record.metric,
                record.value
            ),
            "blocks": [
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": format!(
                            "{} *{}*\n`{}` = *{:.2}* (threshold exceeded)",
                            emoji,
                            record.rule_name,
                            record.metric,
                            record.value
                        )
                    }
                }
            ]
        });

        self.client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;

        tracing::info!(
            channel = "slack",
            rule = %record.rule_name,
            "alert dispatched successfully"
        );
        Ok(())
    }

    /// Send alert to PagerDuty Events API v2.
    async fn send_pagerduty(
        &self,
        routing_key: &str,
        record: &AlertRecord,
    ) -> Result<(), AlertDispatchError> {
        let severity = match record.severity {
            AlertSeverity::Critical => "critical",
            AlertSeverity::Warning => "warning",
            AlertSeverity::Info => "info",
        };

        let payload = serde_json::json!({
            "routing_key": routing_key,
            "event_action": "trigger",
            "payload": {
                "summary": format!("{}: {} = {:.2}", record.rule_name, record.metric, record.value),
                "source": "adapteros",
                "severity": severity,
                "custom_details": {
                    "metric": record.metric,
                    "value": record.value,
                    "rule": record.rule_name
                }
            }
        });

        self.client
            .post("https://events.pagerduty.com/v2/enqueue")
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;

        tracing::info!(
            channel = "pagerduty",
            rule = %record.rule_name,
            "alert dispatched successfully"
        );
        Ok(())
    }

    /// Send alert to generic webhook endpoint.
    async fn send_webhook(
        &self,
        webhook_url: &str,
        record: &AlertRecord,
    ) -> Result<(), AlertDispatchError> {
        let payload = serde_json::json!({
            "alert": {
                "rule_name": record.rule_name,
                "metric": record.metric,
                "value": record.value,
                "severity": record.severity_str(),
                "triggered_at": record.triggered_at
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            }
        });

        self.client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;

        tracing::info!(
            channel = "webhook",
            rule = %record.rule_name,
            url = %webhook_url,
            "alert dispatched successfully"
        );
        Ok(())
    }
}

impl Default for AlertDispatcher {
    fn default() -> Self {
        Self::new()
    }
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
