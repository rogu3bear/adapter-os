#![allow(unused_variables)]

//! Alert evaluation engine
//!
//! Implements alert evaluation engine with threshold checking, escalation,
//! and cooldown logic. Integrates with database and telemetry systems.

use crate::anomaly::{AnomalyConfig, AnomalyDetector};
use crate::monitoring_types::*;
use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use adapteros_manifest::Policies;
use adapteros_policy::PolicyEngine;
use adapteros_telemetry::{SecurityEvent, TelemetryWriter};
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Alert evaluation engine
pub struct AlertEvaluator {
    db: Arc<Db>,
    telemetry_writer: TelemetryWriter,
    config: AlertingConfig,
    notification_sender: Arc<dyn NotificationSender + Send + Sync>,
    anomaly_detector: Option<AnomalyDetector>,
    policy_engine: Option<PolicyEngine>,
    /// Optional broadcast channel for real-time alert streaming
    ///
    /// # Citations
    /// - SSE stream handler: [source: crates/adapteros-server-api/src/handlers.rs L12929-12935]
    /// - Broadcast channel setup: [source: crates/adapteros-server-api/src/state.rs L473-475]
    alert_tx: Option<tokio::sync::broadcast::Sender<crate::monitoring_types::AlertResponse>>,
}

#[derive(Debug, Clone)]
pub struct AlertingConfig {
    pub evaluation_interval_secs: u64,
    pub max_concurrent_evaluations: usize,
    pub default_cooldown_secs: i64,
    pub escalation_check_interval_secs: u64,
    pub enable_escalation: bool,
    pub enable_notifications: bool,
}

impl Default for AlertingConfig {
    fn default() -> Self {
        Self {
            evaluation_interval_secs: 30,
            max_concurrent_evaluations: 10,
            default_cooldown_secs: 300, // 5 minutes
            escalation_check_interval_secs: 60,
            enable_escalation: true,
            enable_notifications: true,
        }
    }
}

/// Notification sender trait
#[async_trait::async_trait]
pub trait NotificationSender {
    async fn send_notification(&self, notification: NotificationRequest) -> Result<()>;
}

/// Notification request
#[derive(Debug, Clone)]
pub struct NotificationRequest {
    pub alert_id: String,
    pub notification_type: NotificationType,
    pub recipient: String,
    pub message: String,
    pub severity: AlertSeverity,
    pub escalation_level: i64,
}

impl AlertEvaluator {
    /// Create a new alert evaluator
    pub fn new(
        db: Arc<Db>,
        telemetry_writer: TelemetryWriter,
        config: AlertingConfig,
        notification_sender: Arc<dyn NotificationSender + Send + Sync>,
    ) -> Self {
        Self {
            db,
            telemetry_writer,
            config,
            notification_sender,
            anomaly_detector: None,
            policy_engine: None,
            alert_tx: None,
        }
    }

    /// Set the alert broadcast channel for real-time streaming
    ///
    /// # Citations
    /// - SSE stream handler: [source: crates/adapteros-server-api/src/handlers.rs L12929-12935]
    /// - Broadcast channel setup: [source: crates/adapteros-server-api/src/state.rs L473-475]
    pub fn with_alert_broadcast(
        mut self,
        alert_tx: Option<tokio::sync::broadcast::Sender<crate::monitoring_types::AlertResponse>>,
    ) -> Self {
        self.alert_tx = alert_tx;
        self
    }

    /// Create a new alert evaluator with anomaly detection
    pub fn new_with_anomaly_detection(
        db: Arc<Db>,
        telemetry_writer: TelemetryWriter,
        config: AlertingConfig,
        notification_sender: Arc<dyn NotificationSender + Send + Sync>,
        anomaly_config: AnomalyConfig,
    ) -> Self {
        let anomaly_detector =
            AnomalyDetector::new(db.clone(), telemetry_writer.clone(), anomaly_config);
        Self {
            db,
            telemetry_writer,
            config,
            notification_sender,
            anomaly_detector: Some(anomaly_detector),
            policy_engine: None,
            alert_tx: None,
        }
    }

    /// Create a new alert evaluator with policy engine
    pub fn new_with_policy_engine(
        db: Arc<Db>,
        telemetry_writer: TelemetryWriter,
        config: AlertingConfig,
        notification_sender: Arc<dyn NotificationSender + Send + Sync>,
        policies: Policies,
    ) -> Self {
        let policy_engine = PolicyEngine::new(policies);
        Self {
            db,
            telemetry_writer,
            config,
            notification_sender,
            anomaly_detector: None,
            policy_engine: Some(policy_engine),
            alert_tx: None,
        }
    }

    /// Create a new alert evaluator with both anomaly detection and policy engine
    pub fn new_with_full_features(
        db: Arc<Db>,
        telemetry_writer: TelemetryWriter,
        config: AlertingConfig,
        notification_sender: Arc<dyn NotificationSender + Send + Sync>,
        anomaly_config: AnomalyConfig,
        policies: Policies,
    ) -> Self {
        let anomaly_detector =
            AnomalyDetector::new(db.clone(), telemetry_writer.clone(), anomaly_config);
        let policy_engine = PolicyEngine::new(policies);
        Self {
            db,
            telemetry_writer,
            config,
            notification_sender,
            anomaly_detector: Some(anomaly_detector),
            policy_engine: Some(policy_engine),
            alert_tx: None,
        }
    }

    /// Start the alert evaluation service
    pub async fn start(&self) -> Result<()> {
        info!("Starting alert evaluation service");

        let evaluation_handle = {
            let evaluator = self.clone();
            tokio::spawn(async move {
                if let Err(e) = evaluator.evaluation_loop().await {
                    error!("Alert evaluation loop failed: {}", e);
                }
            })
        };

        let escalation_handle = {
            let evaluator = self.clone();
            tokio::spawn(async move {
                if let Err(e) = evaluator.escalation_loop().await {
                    error!("Alert escalation loop failed: {}", e);
                }
            })
        };

        // Wait for both tasks to complete (they run indefinitely)
        tokio::select! {
            _ = evaluation_handle => {},
            _ = escalation_handle => {},
        }

        Ok(())
    }

    /// Main evaluation loop
    async fn evaluation_loop(&self) -> Result<()> {
        let mut interval = interval(Duration::from_secs(self.config.evaluation_interval_secs));

        loop {
            interval.tick().await;

            if let Err(e) = self.evaluate_all_tenants().await {
                error!("Failed to evaluate alerts for all tenants: {}", e);
            }
        }
    }

    /// Escalation loop
    async fn escalation_loop(&self) -> Result<()> {
        let mut interval = interval(Duration::from_secs(
            self.config.escalation_check_interval_secs,
        ));

        loop {
            interval.tick().await;

            if self.config.enable_escalation {
                if let Err(e) = self.check_escalations().await {
                    error!("Failed to check escalations: {}", e);
                }
            }
        }
    }

    /// Evaluate alerts for all tenants
    pub async fn evaluate_all_tenants(&self) -> Result<()> {
        // Get all active tenants
        let tenants = self.get_active_tenants().await?;

        // Evaluate alerts for each tenant concurrently (with limit)
        let semaphore = Arc::new(tokio::sync::Semaphore::new(
            self.config.max_concurrent_evaluations,
        ));
        let mut handles = Vec::new();

        for tenant in tenants {
            let semaphore = semaphore.clone();
            let evaluator = self.clone();
            let tenant_id = tenant.id;

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                if let Err(e) = evaluator.evaluate_tenant_alerts(&tenant_id).await {
                    error!("Failed to evaluate alerts for tenant {}: {}", tenant_id, e);
                }
            });

            handles.push(handle);
        }

        // Wait for all evaluations to complete
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Alert evaluation task failed: {}", e);
            }
        }

        Ok(())
    }

    /// Evaluate alerts for a specific tenant
    async fn evaluate_tenant_alerts(&self, tenant_id: &str) -> Result<()> {
        // Get active monitoring rules for this tenant
        let rules = ProcessMonitoringRule::list(
            self.db.pool(),
            Some(tenant_id),
            Some(true), // Only active rules
        )
        .await?;

        debug!("Evaluating {} rules for tenant {}", rules.len(), tenant_id);

        for rule in rules {
            if let Err(e) = self.evaluate_rule(&rule).await {
                error!("Failed to evaluate rule {}: {}", rule.id, e);
                // Continue with other rules
            }
        }

        Ok(())
    }

    /// Evaluate a single monitoring rule
    async fn evaluate_rule(&self, rule: &ProcessMonitoringRule) -> Result<()> {
        // Calculate metric value over evaluation window
        let metric_value = self.calculate_window_metric(rule).await?;

        // Check for drift using baseline comparison
        if let Some(drift) = self.detect_drift(rule, metric_value).await? {
            self.trigger_drift_alert(rule, drift).await?;
        }

        // Check for anomalies using integrated anomaly detector
        if let Err(e) = self.check_anomalies(rule, metric_value).await {
            warn!("Failed to check anomalies for rule {}: {}", rule.id, e);
        }

        // Check performance budgets using policy engine
        if let Err(e) = self.check_performance_budgets(rule, metric_value).await {
            warn!(
                "Failed to check performance budgets for rule {}: {}",
                rule.id, e
            );
        }

        // Check memory headroom using policy engine
        if let Err(e) = self.check_memory_headroom(rule).await {
            warn!(
                "Failed to check memory headroom for rule {}: {}",
                rule.id, e
            );
        }

        // Check for security event correlation
        if let Err(e) = self.correlate_security_events(rule).await {
            warn!(
                "Failed to correlate security events for rule {}: {}",
                rule.id, e
            );
        }

        // Check compliance validation
        if let Err(e) = self.validate_compliance(rule).await {
            warn!("Failed to validate compliance for rule {}: {}", rule.id, e);
        }

        // Check patch validation pipeline
        if let Err(e) = self.validate_patch_pipeline(rule).await {
            warn!(
                "Failed to validate patch pipeline for rule {}: {}",
                rule.id, e
            );
        }

        // Log comprehensive evaluation to telemetry
        if let Err(e) = self.log_evaluation_summary(rule, metric_value).await {
            warn!("Failed to log evaluation summary to telemetry: {}", e);
        }

        // Monitor performance metrics
        if let Err(e) = self.monitor_performance_metrics(rule, metric_value).await {
            warn!("Failed to monitor performance metrics: {}", e);
        }

        // Check if threshold is violated
        if self.check_threshold(metric_value, rule.threshold_value, &rule.threshold_operator) {
            // Check cooldown period
            if self.is_in_cooldown(&rule.id, rule.cooldown_seconds).await? {
                debug!("Rule {} is in cooldown, skipping alert", rule.id);
                return Ok(());
            }

            // Trigger alert
            self.trigger_alert(rule, metric_value).await?;
        }

        Ok(())
    }

    /// Calculate metric value over evaluation window
    async fn calculate_window_metric(&self, rule: &ProcessMonitoringRule) -> Result<f64> {
        let end_time = chrono::Utc::now();
        let start_time = end_time - chrono::Duration::seconds(rule.evaluation_window_seconds);

        let window = TimeWindow {
            start: start_time,
            end: end_time,
            aggregation: AggregationType::Avg, // Default to average
        };

        let aggregation = ProcessHealthMetric::aggregate(
            self.db.pool(),
            window,
            &rule.metric_name,
            Some(&rule.tenant_id),
        )
        .await?;

        Ok(aggregation.avg_value)
    }

    /// Check if metric violates threshold
    fn check_threshold(&self, value: f64, threshold: f64, operator: &ThresholdOperator) -> bool {
        match operator {
            ThresholdOperator::Gt => value > threshold,
            ThresholdOperator::Lt => value < threshold,
            ThresholdOperator::Eq => (value - threshold).abs() < f64::EPSILON,
            ThresholdOperator::Gte => value >= threshold,
            ThresholdOperator::Lte => value <= threshold,
        }
    }

    /// Check if rule is in cooldown period
    async fn is_in_cooldown(&self, rule_id: &str, cooldown_seconds: i64) -> Result<bool> {
        let cutoff_time = chrono::Utc::now() - chrono::Duration::seconds(cooldown_seconds);

        let recent_alert = sqlx::query(
            "SELECT created_at FROM process_alerts 
             WHERE rule_id = ? AND status IN ('active', 'acknowledged') 
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(rule_id)
        .fetch_optional(self.db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to check cooldown: {}", e))
        })?;

        if let Some(alert) = recent_alert {
            let created_at_str: Option<String> = alert
                .try_get::<Option<String>, _>("created_at")
                .ok()
                .flatten();
            let alert_time = match created_at_str {
                Some(dt_str) => chrono::DateTime::parse_from_rfc3339(&dt_str),
                None => return Ok(false), // No created_at means we can't check cooldown
            }
            .map_err(|e| adapteros_core::AosError::Database(format!("Invalid alert time: {}", e)))?
            .with_timezone(&chrono::Utc);

            Ok(alert_time > cutoff_time)
        } else {
            Ok(false)
        }
    }

    /// Trigger an alert
    async fn trigger_alert(&self, rule: &ProcessMonitoringRule, metric_value: f64) -> Result<()> {
        // Get active workers for this tenant
        let workers = self.get_active_workers_for_tenant(&rule.tenant_id).await?;

        for worker in workers {
            let alert_request = CreateAlertRequest {
                rule_id: rule.id.clone(),
                worker_id: worker.id.clone(),
                tenant_id: rule.tenant_id.clone(),
                alert_type: rule.rule_type.to_string(),
                severity: rule.severity.clone(),
                title: format!("{} threshold exceeded", rule.name),
                message: format!(
                    "Metric '{}' value {} {} threshold {}",
                    rule.metric_name, metric_value, rule.threshold_operator, rule.threshold_value
                ),
                metric_value: Some(metric_value),
                threshold_value: Some(rule.threshold_value),
                status: AlertStatus::Active,
            };

            let alert_id = ProcessAlert::create(self.db.pool(), alert_request.clone()).await?;

            // Broadcast alert to SSE stream if channel is available
            // Citation: [source: crates/adapteros-server-api/src/handlers.rs L4598-L4603] - Alert update broadcasting pattern
            if let Some(alert_tx) = &self.alert_tx {
                // Fetch the created alert to broadcast
                if let Ok(Some(created_alert)) =
                    ProcessAlert::get_by_id(self.db.pool(), &alert_id).await
                {
                    let alert_response = created_alert.into();
                    let _ = alert_tx.send(alert_response);
                }
            }

            // Send notification if enabled
            if self.config.enable_notifications {
                if let Err(e) = self
                    .send_alert_notification(&alert_id, rule, &worker.id)
                    .await
                {
                    error!("Failed to send notification for alert {}: {}", alert_id, e);
                }
            }

            // Log alert to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "alert.triggered",
                serde_json::json!({
                    "alert_id": alert_id,
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "worker_id": worker.id,
                    "tenant_id": rule.tenant_id,
                    "metric_name": rule.metric_name,
                    "metric_value": metric_value,
                    "threshold_value": rule.threshold_value,
                    "severity": rule.severity.to_string(),
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log alert to telemetry: {}", e);
            }

            info!(
                "Alert triggered: {} for worker {} (metric: {} = {})",
                rule.name, worker.id, rule.metric_name, metric_value
            );
        }

        Ok(())
    }

    /// Send notification for an alert
    async fn send_alert_notification(
        &self,
        alert_id: &str,
        rule: &ProcessMonitoringRule,
        worker_id: &str,
    ) -> Result<()> {
        // Parse notification channels from rule
        let channels = if let Some(channels) = &rule.notification_channels {
            channels.as_object().cloned().unwrap_or_default()
        } else {
            serde_json::Map::new()
        };

        // Send to each configured channel
        for (channel_type, config) in channels {
            if let Some(recipients) = config.get("recipients").and_then(|r| r.as_array()) {
                for recipient in recipients {
                    if let Some(recipient_str) = recipient.as_str() {
                        let notification = NotificationRequest {
                            alert_id: alert_id.to_string(),
                            notification_type: NotificationType::from_string(channel_type.clone()),
                            recipient: recipient_str.to_string(),
                            message: format!(
                                "Alert: {} - {} threshold exceeded on worker {}",
                                rule.name, rule.metric_name, worker_id
                            ),
                            severity: rule.severity.clone(),
                            escalation_level: 0,
                        };

                        if let Err(e) = self
                            .notification_sender
                            .send_notification(notification)
                            .await
                        {
                            error!("Failed to send notification to {}: {}", recipient_str, e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check for alerts that need escalation
    async fn check_escalations(&self) -> Result<()> {
        // Get active alerts that might need escalation
        let alerts = ProcessAlert::list(
            self.db.pool(),
            AlertFilters {
                tenant_id: None,
                worker_id: None,
                status: Some(AlertStatus::Active),
                severity: None,
                start_time: None,
                end_time: None,
                limit: Some(100),
            },
        )
        .await?;

        for alert in alerts {
            if let Err(e) = self.check_alert_escalation(&alert).await {
                error!("Failed to check escalation for alert {}: {}", alert.id, e);
            }
        }

        Ok(())
    }

    /// Check if a specific alert needs escalation
    async fn check_alert_escalation(&self, alert: &ProcessAlert) -> Result<()> {
        // Get the rule for this alert
        let rules = ProcessMonitoringRule::list(self.db.pool(), None, Some(true)).await?;

        let rule = rules.iter().find(|r| r.id == alert.rule_id);
        if rule.is_none() {
            warn!("Rule {} not found for alert {}", alert.rule_id, alert.id);
            return Ok(());
        }
        let rule = rule.unwrap();

        // Parse escalation rules
        let escalation_rules = if let Some(escalation) = &rule.escalation_rules {
            escalation.as_object().cloned().unwrap_or_default()
        } else {
            serde_json::Map::new()
        };

        // Check if escalation is needed based on duration
        let alert_duration = chrono::Utc::now() - alert.created_at;
        let current_level = alert.escalation_level;

        for (level_str, level_config) in escalation_rules {
            if let Ok(level) = level_str.parse::<i64>() {
                if level > current_level {
                    if let Some(duration_secs) = level_config
                        .get("duration_seconds")
                        .and_then(|d| d.as_i64())
                    {
                        if alert_duration.num_seconds() >= duration_secs {
                            // Escalate this alert
                            self.escalate_alert(alert, level, &level_config).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Escalate an alert to the next level
    async fn escalate_alert(
        &self,
        alert: &ProcessAlert,
        new_level: i64,
        level_config: &serde_json::Value,
    ) -> Result<()> {
        // Update escalation level in database
        sqlx::query(
            "UPDATE process_alerts SET escalation_level = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"
        )
        .bind(new_level)
        .bind(&alert.id)
        .execute(self.db.pool())
        .await
        .map_err(|e| adapteros_core::AosError::Database(format!("Failed to escalate alert: {}", e)))?;

        // Send escalation notification
        if let Some(recipients) = level_config.get("recipients").and_then(|r| r.as_array()) {
            for recipient in recipients {
                if let Some(recipient_str) = recipient.as_str() {
                    let notification = NotificationRequest {
                        alert_id: alert.id.clone(),
                        notification_type: NotificationType::from_string(
                            level_config
                                .get("channel")
                                .and_then(|c| c.as_str())
                                .unwrap_or("email")
                                .to_string(),
                        ),
                        recipient: recipient_str.to_string(),
                        message: format!(
                            "ESCALATED Alert: {} - Level {} escalation",
                            alert.title, new_level
                        ),
                        severity: alert.severity.clone(),
                        escalation_level: new_level,
                    };

                    if let Err(e) = self
                        .notification_sender
                        .send_notification(notification)
                        .await
                    {
                        error!("Failed to send escalation notification: {}", e);
                    }
                }
            }
        }

        // Log escalation to telemetry
        if let Err(e) = self.telemetry_writer.log(
            "alert.escalated",
            serde_json::json!({
                "alert_id": alert.id,
                "rule_id": alert.rule_id,
                "worker_id": alert.worker_id,
                "tenant_id": alert.tenant_id,
                "old_level": alert.escalation_level,
                "new_level": new_level,
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        ) {
            warn!("Failed to log escalation to telemetry: {}", e);
        }

        info!("Alert {} escalated to level {}", alert.id, new_level);

        Ok(())
    }

    /// Get active tenants
    async fn get_active_tenants(&self) -> Result<Vec<TenantInfo>> {
        let rows = sqlx::query("SELECT id FROM tenants")
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| {
                adapteros_core::AosError::Database(format!("Failed to get tenants: {}", e))
            })?;

        let tenants = rows
            .into_iter()
            .map(|row| TenantInfo { id: row.get("id") })
            .collect();

        Ok(tenants)
    }

    /// Get active workers for a tenant
    async fn get_active_workers_for_tenant(&self, tenant_id: &str) -> Result<Vec<WorkerInfo>> {
        // Mock implementation for testing - return a mock worker
        Ok(vec![WorkerInfo {
            id: "test-worker-1".to_string(),
            tenant_id: tenant_id.to_string(),
        }])
    }

    /// Detect drift using baseline comparison
    async fn detect_drift(
        &self,
        rule: &ProcessMonitoringRule,
        current_value: f64,
    ) -> Result<Option<DriftDetection>> {
        // Get baseline from historical data (7 days)
        let baseline = self.calculate_baseline(rule, 7).await?;

        // Calculate drift metrics
        let z_score = if baseline.std_dev > 0.0 {
            (current_value - baseline.mean).abs() / baseline.std_dev
        } else {
            0.0
        };

        let drift_percentage = if baseline.mean > 0.0 {
            ((current_value - baseline.mean) / baseline.mean).abs() * 100.0
        } else {
            0.0
        };

        // Check if drift exceeds thresholds
        if z_score > 2.0 || drift_percentage > 20.0 {
            Ok(Some(DriftDetection {
                current_value,
                baseline_mean: baseline.mean,
                baseline_std_dev: baseline.std_dev,
                z_score,
                drift_percentage,
                confidence_score: (z_score / 3.0).min(1.0),
                severity: if z_score > 3.0 || drift_percentage > 50.0 {
                    AlertSeverity::Critical
                } else if z_score > 2.5 || drift_percentage > 30.0 {
                    AlertSeverity::Error
                } else {
                    AlertSeverity::Warning
                },
            }))
        } else {
            Ok(None)
        }
    }

    /// Calculate baseline from historical data
    async fn calculate_baseline(
        &self,
        rule: &ProcessMonitoringRule,
        days: u32,
    ) -> Result<BaselineStats> {
        let end_time = chrono::Utc::now();
        let start_time = end_time - chrono::Duration::days(days as i64);

        let window = TimeWindow {
            start: start_time,
            end: end_time,
            aggregation: AggregationType::Avg,
        };

        // Get historical metrics for baseline calculation
        let metrics = ProcessHealthMetric::query(
            self.db.pool(),
            MetricFilters {
                worker_id: None,
                tenant_id: Some(rule.tenant_id.clone()),
                metric_name: Some(rule.metric_name.clone()),
                start_time: Some(start_time),
                end_time: Some(end_time),
                limit: Some(1000),
            },
        )
        .await?;

        if metrics.is_empty() {
            return Err(adapteros_core::AosError::Validation(
                "Insufficient historical data for baseline".to_string(),
            ));
        }

        let values: Vec<f64> = metrics.iter().map(|m| m.metric_value).collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;

        let variance =
            values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
        let std_dev = variance.sqrt();

        Ok(BaselineStats {
            mean,
            std_dev,
            sample_count: values.len(),
            min_value: values.iter().cloned().fold(f64::INFINITY, f64::min),
            max_value: values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        })
    }

    /// Trigger drift alert
    async fn trigger_drift_alert(
        &self,
        rule: &ProcessMonitoringRule,
        drift: DriftDetection,
    ) -> Result<()> {
        // Get active workers for this tenant
        let workers = self.get_active_workers_for_tenant(&rule.tenant_id).await?;

        for worker in workers {
            let alert_request = CreateAlertRequest {
                rule_id: rule.id.clone(),
                worker_id: worker.id.clone(),
                tenant_id: rule.tenant_id.clone(),
                alert_type: "drift_detection".to_string(),
                severity: drift.severity.clone(),
                title: format!("Drift detected in {}", rule.name),
                message: format!(
                    "Metric '{}' shows drift: current value {} vs baseline {} (Z-score: {:.2}, drift: {:.1}%)",
                    rule.metric_name,
                    drift.current_value,
                    drift.baseline_mean,
                    drift.z_score,
                    drift.drift_percentage
                ),
                metric_value: Some(drift.current_value),
                threshold_value: Some(drift.baseline_mean),
                status: AlertStatus::Active,
            };

            let alert_id = ProcessAlert::create(self.db.pool(), alert_request.clone()).await?;

            // Log drift alert to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "alert.drift_detected",
                serde_json::json!({
                    "alert_id": alert_id,
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "worker_id": worker.id,
                    "tenant_id": rule.tenant_id,
                    "metric_name": rule.metric_name,
                    "current_value": drift.current_value,
                    "baseline_mean": drift.baseline_mean,
                    "z_score": drift.z_score,
                    "drift_percentage": drift.drift_percentage,
                    "confidence_score": drift.confidence_score,
                    "severity": drift.severity.to_string(),
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log drift alert to telemetry: {}", e);
            }

            info!(
                "Drift alert triggered: {} for worker {} (metric: {} = {}, Z-score: {:.2})",
                rule.name, worker.id, rule.metric_name, drift.current_value, drift.z_score
            );
        }

        Ok(())
    }

    /// Check for anomalies using integrated anomaly detector
    async fn check_anomalies(&self, rule: &ProcessMonitoringRule, metric_value: f64) -> Result<()> {
        if let Some(ref detector) = self.anomaly_detector {
            // Get recent metrics for anomaly detection
            // Note: This would require public access to anomaly detector methods
            // For now, we'll skip the actual anomaly detection in this implementation
            // let recent_metrics = detector.get_recent_worker_metrics(&rule.worker_id, &rule.tenant_id).await?;

            // if recent_metrics.len() >= detector.config.min_samples_for_baseline {
            //     // Calculate baseline for anomaly detection
            //     let baseline = detector.calculate_baseline(&rule.worker_id, &rule.metric_name, detector.config.baseline_window_days).await?;
            //
            //     // Detect anomalies
            //     let anomalies = detector.detect_anomalies(
            //         &rule.worker_id,
            //         &rule.tenant_id,
            //         &rule.metric_name,
            //         metric_value,
            //         &baseline,
            //     ).await?;
            //
            //     // Trigger alerts for detected anomalies
            //     for anomaly in anomalies {
            //         if anomaly.confidence_score >= detector.config.confidence_threshold {
            //             self.trigger_anomaly_alert(rule, &anomaly).await?;
            //         }
            //     }
            // }
        }

        Ok(())
    }

    /// Trigger anomaly alert
    #[allow(dead_code)]
    async fn trigger_anomaly_alert(
        &self,
        rule: &ProcessMonitoringRule,
        anomaly: &crate::anomaly::DetectedAnomaly,
    ) -> Result<()> {
        let alert_request = CreateAlertRequest {
            rule_id: rule.id.clone(),
            worker_id: anomaly.worker_id.clone(),
            tenant_id: anomaly.tenant_id.clone(),
            alert_type: "anomaly_detection".to_string(),
            severity: anomaly.severity.clone(),
            title: format!("Anomaly detected in {}", rule.name),
            message: format!(
                "Anomaly detected in metric '{}': {} (confidence: {:.2}, method: {})",
                anomaly.metric_name,
                anomaly.description,
                anomaly.confidence_score,
                anomaly.detection_method
            ),
            metric_value: Some(anomaly.detected_value),
            threshold_value: anomaly.expected_range_max,
            status: AlertStatus::Active,
        };

        let alert_id = ProcessAlert::create(self.db.pool(), alert_request).await?;

        // Log anomaly alert to telemetry
        if let Err(e) = self.telemetry_writer.log(
            "alert.anomaly_detected",
            serde_json::json!({
                "alert_id": alert_id,
                "rule_id": rule.id,
                "rule_name": rule.name,
                "worker_id": anomaly.worker_id,
                "tenant_id": anomaly.tenant_id,
                "metric_name": anomaly.metric_name,
                "detected_value": anomaly.detected_value,
                "expected_range_min": anomaly.expected_range_min,
                "expected_range_max": anomaly.expected_range_max,
                "confidence_score": anomaly.confidence_score,
                "severity": anomaly.severity.to_string(),
                "detection_method": anomaly.detection_method,
                "anomaly_type": anomaly.anomaly_type,
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        ) {
            warn!("Failed to log anomaly alert to telemetry: {}", e);
        }

        info!(
            "Anomaly alert triggered: {} for worker {} (metric: {} = {}, method: {})",
            rule.name,
            anomaly.worker_id,
            anomaly.metric_name,
            anomaly.detected_value,
            anomaly.detection_method
        );

        Ok(())
    }

    /// Check performance budgets using policy engine
    async fn check_performance_budgets(
        &self,
        rule: &ProcessMonitoringRule,
        metric_value: f64,
    ) -> Result<()> {
        if let Some(ref policy_engine) = self.policy_engine {
            // Calculate performance metrics for this rule
            let latency_p95 = self.calculate_latency_p95(rule).await?;
            let router_overhead = self.calculate_router_overhead(rule).await?;
            let throughput = self.calculate_throughput(rule).await?;

            // Check latency budget (p95 < 24ms per Performance Ruleset #11)
            if latency_p95 > 24.0 {
                self.trigger_performance_violation(rule, "latency", latency_p95, 24.0)
                    .await?;
            }

            // Check router overhead (≤ 8% per Performance Ruleset #11)
            if router_overhead > 8.0 {
                self.trigger_performance_violation(rule, "router_overhead", router_overhead, 8.0)
                    .await?;
            }

            // Check throughput minimum (≥ 40 tokens/s per Performance Ruleset #11)
            if throughput < 40.0 {
                self.trigger_performance_violation(rule, "throughput", throughput, 40.0)
                    .await?;
            }

            // Log performance metrics to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "performance.budget_check",
                serde_json::json!({
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "tenant_id": rule.tenant_id,
                    "metric_name": rule.metric_name,
                    "metric_value": metric_value,
                    "latency_p95_ms": latency_p95,
                    "router_overhead_pct": router_overhead,
                    "throughput_tokens_per_s": throughput,
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log performance budget check to telemetry: {}", e);
            }
        }

        Ok(())
    }

    /// Calculate latency p95 for performance budget checking
    async fn calculate_latency_p95(&self, rule: &ProcessMonitoringRule) -> Result<f64> {
        // Mock implementation for testing - return a reasonable default
        Ok(20.0) // 20ms latency
    }

    /// Calculate router overhead for performance budget checking
    async fn calculate_router_overhead(&self, rule: &ProcessMonitoringRule) -> Result<f64> {
        // Mock implementation for testing - return a reasonable default
        Ok(5.0) // 5% router overhead
    }

    /// Calculate throughput for performance budget checking
    async fn calculate_throughput(&self, rule: &ProcessMonitoringRule) -> Result<f64> {
        // Mock implementation for testing - return a reasonable default
        Ok(50.0) // 50 tokens/s throughput
    }

    /// Trigger performance budget violation alert
    async fn trigger_performance_violation(
        &self,
        rule: &ProcessMonitoringRule,
        violation_type: &str,
        current_value: f64,
        threshold: f64,
    ) -> Result<()> {
        // Get active workers for this tenant
        let workers = self.get_active_workers_for_tenant(&rule.tenant_id).await?;

        for worker in workers {
            let alert_request = CreateAlertRequest {
                rule_id: rule.id.clone(),
                worker_id: worker.id.clone(),
                tenant_id: rule.tenant_id.clone(),
                alert_type: "performance_budget_violation".to_string(),
                severity: if current_value > threshold * 2.0 {
                    AlertSeverity::Critical
                } else if current_value > threshold * 1.5 {
                    AlertSeverity::Error
                } else {
                    AlertSeverity::Warning
                },
                title: format!("Performance budget violation: {}", violation_type),
                message: format!(
                    "Performance budget violation in '{}': {} = {:.2} exceeds threshold {:.2}",
                    violation_type, rule.metric_name, current_value, threshold
                ),
                metric_value: Some(current_value),
                threshold_value: Some(threshold),
                status: AlertStatus::Active,
            };

            let alert_id = ProcessAlert::create(self.db.pool(), alert_request.clone()).await?;

            // Log performance violation to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "alert.performance_violation",
                serde_json::json!({
                    "alert_id": alert_id,
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "worker_id": worker.id,
                    "tenant_id": rule.tenant_id,
                    "violation_type": violation_type,
                    "current_value": current_value,
                    "threshold": threshold,
                    "severity": alert_request.severity.to_string(),
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log performance violation to telemetry: {}", e);
            }

            info!(
                "Performance budget violation: {} for worker {} ({} = {:.2} > {:.2})",
                violation_type, worker.id, rule.metric_name, current_value, threshold
            );
        }

        Ok(())
    }

    /// Check memory headroom using policy engine
    async fn check_memory_headroom(&self, rule: &ProcessMonitoringRule) -> Result<()> {
        if let Some(ref policy_engine) = self.policy_engine {
            // Get current memory usage for this tenant
            let memory_usage = self.get_current_memory_usage(&rule.tenant_id).await?;
            let memory_headroom = 100.0 - memory_usage;

            // Check memory headroom (≥ 15% per Memory Ruleset #12)
            if memory_headroom < 15.0 {
                self.trigger_memory_headroom_violation(rule, memory_headroom, 15.0)
                    .await?;

                // Apply eviction order policy (ephemeral → cold → warm)
                self.apply_eviction_order(&rule.tenant_id).await?;
            }

            // Log memory metrics to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "memory.headroom_check",
                serde_json::json!({
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "tenant_id": rule.tenant_id,
                    "memory_usage_pct": memory_usage,
                    "memory_headroom_pct": memory_headroom,
                    "threshold_pct": 15.0,
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log memory headroom check to telemetry: {}", e);
            }
        }

        Ok(())
    }

    /// Get current memory usage for a tenant
    async fn get_current_memory_usage(&self, tenant_id: &str) -> Result<f64> {
        // Mock implementation for testing - return a reasonable default
        Ok(80.0) // 80% memory usage
    }

    /// Trigger memory headroom violation alert
    async fn trigger_memory_headroom_violation(
        &self,
        rule: &ProcessMonitoringRule,
        current_headroom: f64,
        threshold: f64,
    ) -> Result<()> {
        // Get active workers for this tenant
        let workers = self.get_active_workers_for_tenant(&rule.tenant_id).await?;

        for worker in workers {
            let alert_request = CreateAlertRequest {
                rule_id: rule.id.clone(),
                worker_id: worker.id.clone(),
                tenant_id: rule.tenant_id.clone(),
                alert_type: "memory_headroom_violation".to_string(),
                severity: if current_headroom < threshold * 0.5 {
                    AlertSeverity::Critical
                } else if current_headroom < threshold * 0.75 {
                    AlertSeverity::Error
                } else {
                    AlertSeverity::Warning
                },
                title: "Memory headroom violation".to_string(),
                message: format!(
                    "Memory headroom {}% below threshold {}% for tenant {}",
                    current_headroom, threshold, rule.tenant_id
                ),
                metric_value: Some(current_headroom),
                threshold_value: Some(threshold),
                status: AlertStatus::Active,
            };

            let alert_id = ProcessAlert::create(self.db.pool(), alert_request.clone()).await?;

            // Log memory violation to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "alert.memory_violation",
                serde_json::json!({
                    "alert_id": alert_id,
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "worker_id": worker.id,
                    "tenant_id": rule.tenant_id,
                    "current_headroom_pct": current_headroom,
                    "threshold_pct": threshold,
                    "severity": alert_request.severity.to_string(),
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log memory violation to telemetry: {}", e);
            }

            info!(
                "Memory headroom violation: {}% < {}% for tenant {}",
                current_headroom, threshold, rule.tenant_id
            );
        }

        Ok(())
    }

    /// Apply eviction order policy (ephemeral → cold → warm)
    async fn apply_eviction_order(&self, tenant_id: &str) -> Result<()> {
        // Get active adapters for this tenant
        let adapters = self.get_active_adapters_for_tenant(tenant_id).await?;

        // Sort by eviction priority (ephemeral first, then by LRU)
        let mut sorted_adapters = adapters;
        sorted_adapters.sort_by(|a, b| {
            // Ephemeral adapters first
            match (&a.category, &b.category) {
                (AdapterCategory::Ephemeral, AdapterCategory::Ephemeral) => {
                    // Then by last accessed time (LRU)
                    a.last_accessed.cmp(&b.last_accessed)
                }
                (AdapterCategory::Ephemeral, _) => std::cmp::Ordering::Less,
                (_, AdapterCategory::Ephemeral) => std::cmp::Ordering::Greater,
                _ => a.last_accessed.cmp(&b.last_accessed),
            }
        });

        // Evict adapters until headroom is restored
        let mut evicted_count = 0;
        for adapter in sorted_adapters {
            if evicted_count >= 3 {
                // Limit evictions per cycle
                break;
            }

            // Evict adapter
            if let Err(e) = self.evict_adapter(&adapter.id, tenant_id).await {
                warn!("Failed to evict adapter {}: {}", adapter.id, e);
                continue;
            }

            evicted_count += 1;
            info!("Evicted adapter {} due to memory pressure", adapter.id);

            // Check if headroom is restored
            let current_headroom = 100.0 - self.get_current_memory_usage(tenant_id).await?;
            if current_headroom >= 15.0 {
                break;
            }
        }

        // Log eviction actions to telemetry
        if let Err(e) = self.telemetry_writer.log(
            "memory.eviction_actions",
            serde_json::json!({
                "tenant_id": tenant_id,
                "evicted_count": evicted_count,
                "reason": "memory_headroom_violation",
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        ) {
            warn!("Failed to log eviction actions to telemetry: {}", e);
        }

        Ok(())
    }

    /// Get active adapters for a tenant
    async fn get_active_adapters_for_tenant(&self, tenant_id: &str) -> Result<Vec<AdapterInfo>> {
        // Get all active adapters from the database
        // Note: Current schema doesn't have tenant_id in adapters table
        // TODO: Add tenant filtering once schema supports it
        let adapters = self
            .db
            .list_adapters_by_state("active")
            .await
            .map_err(|e| {
                adapteros_core::AosError::Database(format!("Failed to get active adapters: {}", e))
            })?;

        // Convert to AdapterInfo format
        let adapter_infos: Vec<AdapterInfo> = adapters
            .into_iter()
            .map(|adapter| {
                // Parse category
                let category = match adapter.category.as_str() {
                    "code" => AdapterCategory::Code,
                    "framework" => AdapterCategory::Framework,
                    "codebase" => AdapterCategory::Codebase,
                    "ephemeral" => AdapterCategory::Ephemeral,
                    _ => AdapterCategory::Code, // Default fallback
                };

                // Parse last_activated timestamp
                let last_accessed = adapter
                    .last_activated
                    .and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                AdapterInfo {
                    id: adapter.adapter_id,
                    category,
                    last_accessed,
                }
            })
            .collect();

        info!(tenant_id = %tenant_id, count = adapter_infos.len(), "Retrieved active adapters for tenant");
        Ok(adapter_infos)
    }

    /// Evict an adapter
    async fn evict_adapter(&self, adapter_id: &str, tenant_id: &str) -> Result<()> {
        // Update adapter state to evicted
        self.db
            .update_adapter_state(adapter_id, "evicted", "Memory pressure eviction")
            .await
            .map_err(|e| {
                adapteros_core::AosError::Database(format!("Failed to evict adapter: {}", e))
            })?;

        // Log eviction to telemetry
        if let Err(e) = self.telemetry_writer.log(
            "adapter.evicted",
            serde_json::json!({
                "adapter_id": adapter_id,
                "tenant_id": tenant_id,
                "reason": "memory_headroom_violation",
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        ) {
            warn!("Failed to log adapter eviction to telemetry: {}", e);
        }

        Ok(())
    }

    /// Correlate security events with alert evaluation
    async fn correlate_security_events(&self, rule: &ProcessMonitoringRule) -> Result<()> {
        if let Some(ref policy_engine) = self.policy_engine {
            // Get recent security events for this tenant
            let security_events = self.get_recent_security_events(&rule.tenant_id).await?;

            // Check for policy violations
            for event in &security_events {
                if self.is_security_violation(event) {
                    self.trigger_security_correlation_alert(rule, event).await?;
                }
            }

            // Log security correlation to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "security.correlation_check",
                serde_json::json!({
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "tenant_id": rule.tenant_id,
                    "events_checked": security_events.len(),
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log security correlation to telemetry: {}", e);
            }
        }

        Ok(())
    }

    /// Get recent security events for a tenant
    async fn get_recent_security_events(&self, tenant_id: &str) -> Result<Vec<SecurityEvent>> {
        let end_time = chrono::Utc::now();
        let start_time = end_time - chrono::Duration::minutes(10); // 10-minute window

        // Query security events from telemetry database
        let telemetry_records = self.db.get_telemetry_by_event_type("security", 100)
            .await
            .map_err(|e| {
                tracing::error!(tenant_id = %tenant_id, error = %e, "Failed to query security events from telemetry");
                AosError::Database(format!("Failed to query security events: {}", e))
            })?;

        let mut security_events = Vec::new();

        for record in telemetry_records {
            // Filter by tenant and time window
            if record.tenant_id != tenant_id {
                continue;
            }

            // Parse timestamp and check if it's within the window
            if let Ok(record_time) = chrono::DateTime::parse_from_rfc3339(&record.timestamp) {
                let record_time_utc = record_time.with_timezone(&chrono::Utc);
                if record_time_utc < start_time || record_time_utc > end_time {
                    continue;
                }
            } else {
                // Skip records with invalid timestamps
                continue;
            }

            // Parse the event data from JSON
            match serde_json::from_str::<SecurityEvent>(&record.event_data) {
                Ok(security_event) => {
                    security_events.push(security_event);
                }
                Err(e) => {
                    tracing::warn!(
                        tenant_id = %tenant_id,
                        event_id = %record.id,
                        error = %e,
                        "Failed to parse security event data"
                    );
                }
            }
        }

        tracing::info!(
            tenant_id = %tenant_id,
            event_count = security_events.len(),
            time_window_minutes = 10,
            "Retrieved security events from telemetry"
        );

        Ok(security_events)
    }

    /// Check if a security event represents a policy violation
    fn is_security_violation(&self, event: &SecurityEvent) -> bool {
        match event {
            SecurityEvent::PolicyViolation {
                policy,
                violation_type,
                ..
            } => {
                // Check for critical policy violations
                matches!(
                    policy.as_str(),
                    "egress" | "determinism" | "evidence" | "isolation"
                ) || matches!(
                    violation_type.as_str(),
                    "network_access"
                        | "kernel_mismatch"
                        | "insufficient_evidence"
                        | "cross_tenant_access"
                )
            }
            _ => false,
        }
    }

    /// Trigger security correlation alert
    async fn trigger_security_correlation_alert(
        &self,
        rule: &ProcessMonitoringRule,
        event: &SecurityEvent,
    ) -> Result<()> {
        // Get active workers for this tenant
        let workers = self.get_active_workers_for_tenant(&rule.tenant_id).await?;

        for worker in workers {
            let alert_request = CreateAlertRequest {
                rule_id: rule.id.clone(),
                worker_id: worker.id.clone(),
                tenant_id: rule.tenant_id.clone(),
                alert_type: "security_correlation".to_string(),
                severity: AlertSeverity::Critical, // Security events are always critical
                title: format!("Security event correlation: {}", rule.name),
                message: format!(
                    "Security event correlated with alert rule '{}': {:?}",
                    rule.name, event
                ),
                metric_value: None,
                threshold_value: None,
                status: AlertStatus::Active,
            };

            let alert_id = ProcessAlert::create(self.db.pool(), alert_request.clone()).await?;

            // Log security correlation alert to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "alert.security_correlation",
                serde_json::json!({
                    "alert_id": alert_id,
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "worker_id": worker.id,
                    "tenant_id": rule.tenant_id,
                    "security_event": format!("{:?}", event),
                    "severity": "critical",
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!(
                    "Failed to log security correlation alert to telemetry: {}",
                    e
                );
            }

            info!(
                "Security correlation alert triggered: {} for worker {} (event: {:?})",
                rule.name, worker.id, event
            );
        }

        Ok(())
    }

    /// Validate compliance requirements
    async fn validate_compliance(&self, rule: &ProcessMonitoringRule) -> Result<()> {
        if let Some(ref policy_engine) = self.policy_engine {
            // Check control matrix mapping (Compliance Ruleset #16)
            if !self.validate_control_matrix(&rule.tenant_id).await? {
                self.trigger_compliance_violation(
                    rule,
                    "control_matrix_mapping",
                    "Control matrix cross-links do not resolve to existing evidence",
                )
                .await?;
            }

            // Validate ITAR isolation (Compliance Ruleset #16)
            if !self.validate_itar_isolation(&rule.tenant_id).await? {
                self.trigger_compliance_violation(
                    rule,
                    "itar_isolation",
                    "ITAR isolation validation failed",
                )
                .await?;
            }

            // Check evidence links (Compliance Ruleset #16)
            if !self.validate_evidence_links(rule).await? {
                self.trigger_compliance_violation(
                    rule,
                    "evidence_links",
                    "Evidence links required but not provided",
                )
                .await?;
            }

            // Log compliance validation to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "compliance.validation_check",
                serde_json::json!({
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "tenant_id": rule.tenant_id,
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log compliance validation to telemetry: {}", e);
            }
        }

        Ok(())
    }

    /// Validate control matrix mapping
    async fn validate_control_matrix(&self, tenant_id: &str) -> Result<bool> {
        // Check if control matrix cross-links resolve to existing evidence (Compliance Ruleset #16)
        if let Some(ref policy_engine) = self.policy_engine {
            // Get compliance policy configuration
            if let Some(compliance_config) = policy_engine
                .pack_manager()
                .get_config(&adapteros_policy::PolicyPackId::Compliance)
            {
                // Extract compliance-specific configuration
                let compliance_data = &compliance_config.config;
                if let Some(compliance_obj) = compliance_data.get("compliance") {
                    // Parse control matrix configuration
                    if let Some(control_matrix) = compliance_obj.get("control_matrix") {
                        if let Some(controls_array) = control_matrix.as_array() {
                            let mut all_valid = true;

                            for control in controls_array {
                                if let Some(control_obj) = control.as_object() {
                                    // Validate required evidence fields
                                    let evidence_file = control_obj.get("evidence_file");
                                    let evidence_hash = control_obj.get("evidence_hash");
                                    let verification_status =
                                        control_obj.get("verification_status");

                                    // Check evidence file exists
                                    if let Some(evidence_path) =
                                        evidence_file.and_then(|v| v.as_str())
                                    {
                                        if !std::path::Path::new(evidence_path).exists() {
                                            tracing::warn!(
                                                tenant_id = %tenant_id,
                                                control_id = ?control_obj.get("control_id"),
                                                evidence_path = %evidence_path,
                                                "Control matrix evidence file does not exist"
                                            );
                                            all_valid = false;
                                        }
                                    } else {
                                        tracing::warn!(
                                            tenant_id = %tenant_id,
                                            control_id = ?control_obj.get("control_id"),
                                            "Control matrix entry missing evidence_file"
                                        );
                                        all_valid = false;
                                    }

                                    // Check evidence hash is present
                                    if evidence_hash.and_then(|v| v.as_str()).is_none() {
                                        tracing::warn!(
                                            tenant_id = %tenant_id,
                                            control_id = ?control_obj.get("control_id"),
                                            "Control matrix entry missing evidence_hash"
                                        );
                                        all_valid = false;
                                    }

                                    // Check verification status
                                    if let Some(status) =
                                        verification_status.and_then(|v| v.as_str())
                                    {
                                        if status == "failed" || status == "expired" {
                                            tracing::warn!(
                                                tenant_id = %tenant_id,
                                                control_id = ?control_obj.get("control_id"),
                                                verification_status = %status,
                                                "Control matrix entry has invalid verification status"
                                            );
                                            all_valid = false;
                                        }
                                    } else {
                                        tracing::warn!(
                                            tenant_id = %tenant_id,
                                            control_id = ?control_obj.get("control_id"),
                                            "Control matrix entry missing verification_status"
                                        );
                                        all_valid = false;
                                    }
                                }
                            }

                            tracing::info!(
                                tenant_id = %tenant_id,
                                controls_validated = controls_array.len(),
                                all_valid = all_valid,
                                "Completed control matrix validation"
                            );

                            return Ok(all_valid);
                        }
                    }
                }
            }
        }

        // If no policy engine or compliance config available, assume valid for backward compatibility
        tracing::debug!(
            tenant_id = %tenant_id,
            "No policy engine or compliance configuration available, assuming control matrix valid"
        );
        Ok(true)
    }

    /// Validate ITAR isolation
    async fn validate_itar_isolation(&self, tenant_id: &str) -> Result<bool> {
        // Check ITAR isolation via adversarial suite per CP
        // This would run ITAR isolation tests
        // For now, return true as placeholder
        Ok(true)
    }

    /// Validate evidence links
    async fn validate_evidence_links(&self, rule: &ProcessMonitoringRule) -> Result<bool> {
        // Check if evidence links are required and provided
        // This would validate against the evidence ruleset
        // For now, return true as placeholder
        Ok(true)
    }

    /// Trigger compliance violation alert
    async fn trigger_compliance_violation(
        &self,
        rule: &ProcessMonitoringRule,
        violation_type: &str,
        description: &str,
    ) -> Result<()> {
        // Get active workers for this tenant
        let workers = self.get_active_workers_for_tenant(&rule.tenant_id).await?;

        for worker in workers {
            let alert_request = CreateAlertRequest {
                rule_id: rule.id.clone(),
                worker_id: worker.id.clone(),
                tenant_id: rule.tenant_id.clone(),
                alert_type: "compliance_violation".to_string(),
                severity: AlertSeverity::Critical, // Compliance violations are always critical
                title: format!("Compliance violation: {}", violation_type),
                message: format!(
                    "Compliance violation in rule '{}': {}",
                    rule.name, description
                ),
                metric_value: None,
                threshold_value: None,
                status: AlertStatus::Active,
            };

            let alert_id = ProcessAlert::create(self.db.pool(), alert_request.clone()).await?;

            // Log compliance violation to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "alert.compliance_violation",
                serde_json::json!({
                    "alert_id": alert_id,
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "worker_id": worker.id,
                    "tenant_id": rule.tenant_id,
                    "violation_type": violation_type,
                    "description": description,
                    "severity": "critical",
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log compliance violation to telemetry: {}", e);
            }

            info!(
                "Compliance violation alert triggered: {} for worker {} ({}: {})",
                rule.name, worker.id, violation_type, description
            );
        }

        Ok(())
    }

    /// Validate patch pipeline requirements
    async fn validate_patch_pipeline(&self, rule: &ProcessMonitoringRule) -> Result<()> {
        if let Some(ref policy_engine) = self.policy_engine {
            // Check for recent patch validation failures
            let validation_failures = self
                .get_recent_patch_validation_failures(&rule.tenant_id)
                .await?;

            if !validation_failures.is_empty() {
                self.trigger_patch_validation_alert(rule, &validation_failures.join(", "))
                    .await?;
            }

            // Check patch validation metrics
            let validation_metrics = self.get_patch_validation_metrics(&rule.tenant_id).await?;

            // Check validation success rate (should be > 95%)
            if validation_metrics.success_rate < 0.95 {
                self.trigger_patch_validation_alert(
                    rule,
                    &format!(
                        "Low validation success rate: {:.1}%",
                        validation_metrics.success_rate * 100.0
                    ),
                )
                .await?;
            }

            // Check average validation time (should be < 5 seconds)
            if validation_metrics.avg_validation_time_ms > 5000.0 {
                self.trigger_patch_validation_alert(
                    rule,
                    &format!(
                        "High validation time: {:.1}ms",
                        validation_metrics.avg_validation_time_ms
                    ),
                )
                .await?;
            }

            // Log patch validation metrics to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "patch.validation_check",
                serde_json::json!({
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "tenant_id": rule.tenant_id,
                    "success_rate": validation_metrics.success_rate,
                    "avg_validation_time_ms": validation_metrics.avg_validation_time_ms,
                    "total_validations": validation_metrics.total_validations,
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log patch validation to telemetry: {}", e);
            }
        }

        Ok(())
    }

    /// Get recent patch validation failures
    async fn get_recent_patch_validation_failures(&self, tenant_id: &str) -> Result<Vec<String>> {
        // Note: This would require the patch_validations table to exist
        // For now, return empty vector as placeholder
        // let end_time = chrono::Utc::now();
        // let start_time = end_time - chrono::Duration::minutes(10); // 10-minute window
        //
        // // Query patch validation failures from database
        // let rows = sqlx::query!(
        //     "SELECT error_message FROM patch_validations WHERE tenant_id = ? AND status = 'failed' AND created_at > ?",
        //     tenant_id,
        //     start_time
        // )
        // .fetch_all(self.db.pool())
        // .await
        // .map_err(|e| adapteros_core::AosError::Database(format!("Failed to get patch validation failures: {}", e)))?;
        //
        // Ok(rows.into_iter().map(|row| row.error_message.unwrap_or_default()).collect())

        Ok(Vec::new())
    }

    /// Get patch validation metrics
    async fn get_patch_validation_metrics(
        &self,
        tenant_id: &str,
    ) -> Result<PatchValidationMetrics> {
        let end_time = chrono::Utc::now();
        let start_time = end_time - chrono::Duration::hours(1); // 1-hour window

        // Query patch proposals from database as proxy for validation metrics
        // TODO: Add tenant filtering when patch_proposals table includes tenant_id
        let patch_proposals = self.db.list_patch_proposals(None)
            .await
            .map_err(|e| {
                tracing::error!(tenant_id = %tenant_id, error = %e, "Failed to query patch proposals for validation metrics");
                AosError::Database(format!("Failed to query patch proposals: {}", e))
            })?;

        // Filter by time window (created_at within last hour)
        let recent_proposals: Vec<_> = patch_proposals
            .into_iter()
            .filter(|proposal| {
                // Parse created_at timestamp and check if it's within the window
                if let Ok(proposal_time) =
                    chrono::DateTime::parse_from_rfc3339(&proposal.created_at)
                {
                    let proposal_time_utc = proposal_time.with_timezone(&chrono::Utc);
                    proposal_time_utc >= start_time && proposal_time_utc <= end_time
                } else {
                    false
                }
            })
            .collect();

        let total_validations = recent_proposals.len();

        // Count successful validations based on status
        let successful_validations = recent_proposals
            .iter()
            .filter(|proposal| {
                // Consider "completed" or "applied" as successful validations
                proposal.status == "completed" || proposal.status == "applied"
            })
            .count();

        // Calculate success rate
        let success_rate = if total_validations > 0 {
            successful_validations as f64 / total_validations as f64
        } else {
            1.0 // Default to 100% success if no validations
        };

        // Parse validation results to extract timing information
        let mut total_validation_time = 0.0;
        let mut timed_validations = 0;

        for proposal in &recent_proposals {
            // Try to parse validation_result_json for timing data
            if let Ok(validation_result) =
                serde_json::from_str::<serde_json::Value>(&proposal.validation_result_json)
            {
                if let Some(validation_time) = validation_result
                    .get("validation_time_ms")
                    .and_then(|v| v.as_f64())
                {
                    total_validation_time += validation_time;
                    timed_validations += 1;
                }
            }
        }

        let avg_validation_time_ms = if timed_validations > 0 {
            total_validation_time / timed_validations as f64
        } else {
            0.0 // Default to 0 if no timing data available
        };

        tracing::info!(
            tenant_id = %tenant_id,
            total_validations = total_validations,
            successful_validations = successful_validations,
            success_rate = success_rate,
            avg_validation_time_ms = avg_validation_time_ms,
            time_window_hours = 1,
            "Calculated patch validation metrics from database"
        );

        Ok(PatchValidationMetrics {
            success_rate,
            avg_validation_time_ms,
            total_validations,
        })
    }

    /// Trigger patch validation alert
    async fn trigger_patch_validation_alert(
        &self,
        rule: &ProcessMonitoringRule,
        details: &str,
    ) -> Result<()> {
        // Get active workers for this tenant
        let workers = self.get_active_workers_for_tenant(&rule.tenant_id).await?;

        for worker in workers {
            let alert_request = CreateAlertRequest {
                rule_id: rule.id.clone(),
                worker_id: worker.id.clone(),
                tenant_id: rule.tenant_id.clone(),
                alert_type: "patch_validation_failure".to_string(),
                severity: AlertSeverity::Error,
                title: format!("Patch validation failure: {}", rule.name),
                message: format!(
                    "Patch validation failure in rule '{}': {}",
                    rule.name, details
                ),
                metric_value: None,
                threshold_value: None,
                status: AlertStatus::Active,
            };

            let alert_id = ProcessAlert::create(self.db.pool(), alert_request.clone()).await?;

            // Log patch validation alert to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "alert.patch_validation_failure",
                serde_json::json!({
                    "alert_id": alert_id,
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "worker_id": worker.id,
                    "tenant_id": rule.tenant_id,
                    "details": details,
                    "severity": "error",
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!("Failed to log patch validation alert to telemetry: {}", e);
            }

            info!(
                "Patch validation alert triggered: {} for worker {} (details: {})",
                rule.name, worker.id, details
            );
        }

        Ok(())
    }

    /// Log comprehensive evaluation summary to telemetry
    async fn log_evaluation_summary(
        &self,
        rule: &ProcessMonitoringRule,
        metric_value: f64,
    ) -> Result<()> {
        // Collect evaluation metrics
        let evaluation_metrics = self.collect_evaluation_metrics(rule, metric_value).await?;

        // Log comprehensive evaluation summary
        if let Err(e) = self.telemetry_writer.log(
            "alert.evaluation_summary",
            serde_json::json!({
                "rule_id": rule.id,
                "rule_name": rule.name,
                "tenant_id": rule.tenant_id,
                "rule_id": rule.id,
                "metric_name": rule.metric_name,
                "metric_value": metric_value,
                "threshold_value": rule.threshold_value,
                "threshold_operator": rule.threshold_operator.to_string(),
                "evaluation_window_seconds": rule.evaluation_window_seconds,
                "cooldown_seconds": rule.cooldown_seconds,
                "is_active": rule.is_active,
                "evaluation_metrics": evaluation_metrics,
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        ) {
            warn!("Failed to log evaluation summary to telemetry: {}", e);
        }

        Ok(())
    }

    /// Collect comprehensive evaluation metrics
    async fn collect_evaluation_metrics(
        &self,
        rule: &ProcessMonitoringRule,
        metric_value: f64,
    ) -> Result<EvaluationMetrics> {
        let mut metrics = EvaluationMetrics {
            drift_detected: false,
            anomaly_detected: false,
            performance_violation: false,
            memory_violation: false,
            security_violation: false,
            compliance_violation: false,
            patch_validation_failure: false,
            total_checks: 0,
            successful_checks: 0,
            failed_checks: 0,
            evaluation_duration_ms: 0,
        };

        let start_time = SystemTime::now();

        // Check drift detection
        if let Ok(Some(_)) = self.detect_drift(rule, metric_value).await {
            metrics.drift_detected = true;
        }

        // Check anomaly detection
        if let Ok(()) = self.check_anomalies(rule, metric_value).await {
            // Anomaly detection succeeded, check if any were found
            // This would require checking the anomaly detector state
        }

        // Check performance budgets
        if let Ok(()) = self.check_performance_budgets(rule, metric_value).await {
            // Performance budget check succeeded
        }

        // Check memory headroom
        if let Ok(()) = self.check_memory_headroom(rule).await {
            // Memory headroom check succeeded
        }

        // Check security event correlation
        if let Ok(()) = self.correlate_security_events(rule).await {
            // Security correlation check succeeded
        }

        // Check compliance validation
        if let Ok(()) = self.validate_compliance(rule).await {
            // Compliance validation succeeded
        }

        // Check patch validation pipeline
        if let Ok(()) = self.validate_patch_pipeline(rule).await {
            // Patch validation succeeded
        }

        let end_time = SystemTime::now();
        metrics.evaluation_duration_ms = end_time
            .duration_since(start_time)
            .unwrap_or_default()
            .as_millis() as u64;

        Ok(metrics)
    }

    /// Monitor performance metrics for alert evaluation
    async fn monitor_performance_metrics(
        &self,
        rule: &ProcessMonitoringRule,
        metric_value: f64,
    ) -> Result<()> {
        let start_time = SystemTime::now();

        // Collect performance metrics
        let performance_metrics = self.collect_performance_metrics(rule).await?;

        // Check for performance degradation
        if performance_metrics.evaluation_time_ms > 1000.0 {
            self.trigger_performance_degradation_alert(
                rule,
                "evaluation_time",
                performance_metrics.evaluation_time_ms,
                1000.0,
            )
            .await?;
        }

        if performance_metrics.memory_usage_mb > 1000.0 {
            self.trigger_performance_degradation_alert(
                rule,
                "memory_usage",
                performance_metrics.memory_usage_mb,
                1000.0,
            )
            .await?;
        }

        if performance_metrics.cpu_usage_pct > 80.0 {
            self.trigger_performance_degradation_alert(
                rule,
                "cpu_usage",
                performance_metrics.cpu_usage_pct,
                80.0,
            )
            .await?;
        }

        // Log performance metrics to telemetry
        if let Err(e) = self.telemetry_writer.log(
            "performance.monitoring",
            serde_json::json!({
                "rule_id": rule.id,
                "rule_name": rule.name,
                "tenant_id": rule.tenant_id,
                "metric_name": rule.metric_name,
                "metric_value": metric_value,
                "performance_metrics": performance_metrics,
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        ) {
            warn!("Failed to log performance metrics to telemetry: {}", e);
        }

        Ok(())
    }

    /// Collect performance metrics for monitoring
    async fn collect_performance_metrics(
        &self,
        rule: &ProcessMonitoringRule,
    ) -> Result<PerformanceMetrics> {
        let start_time = SystemTime::now();

        // Simulate performance metric collection
        // In a real implementation, this would collect actual system metrics
        let evaluation_time_ms = 50.0; // Simulated evaluation time
        let memory_usage_mb = 512.0; // Simulated memory usage
        let cpu_usage_pct = 25.0; // Simulated CPU usage
        let disk_io_mb_s = 10.0; // Simulated disk I/O
        let network_io_mb_s = 5.0; // Simulated network I/O

        let end_time = SystemTime::now();
        let actual_evaluation_time = end_time
            .duration_since(start_time)
            .unwrap_or_default()
            .as_millis() as f64;

        Ok(PerformanceMetrics {
            evaluation_time_ms: actual_evaluation_time,
            memory_usage_mb,
            cpu_usage_pct,
            disk_io_mb_s,
            network_io_mb_s,
            active_connections: 10,
            queue_depth: 5,
            error_rate: 0.01,
            throughput_ops_per_s: 100.0,
        })
    }

    /// Trigger performance degradation alert
    async fn trigger_performance_degradation_alert(
        &self,
        rule: &ProcessMonitoringRule,
        metric_type: &str,
        current_value: f64,
        threshold: f64,
    ) -> Result<()> {
        // Get active workers for this tenant
        let workers = self.get_active_workers_for_tenant(&rule.tenant_id).await?;

        for worker in workers {
            let alert_request = CreateAlertRequest {
                rule_id: rule.id.clone(),
                worker_id: worker.id.clone(),
                tenant_id: rule.tenant_id.clone(),
                alert_type: "performance_degradation".to_string(),
                severity: if current_value > threshold * 2.0 {
                    AlertSeverity::Critical
                } else if current_value > threshold * 1.5 {
                    AlertSeverity::Error
                } else {
                    AlertSeverity::Warning
                },
                title: format!("Performance degradation: {}", metric_type),
                message: format!(
                    "Performance degradation detected in '{}': {} = {:.2} exceeds threshold {:.2}",
                    metric_type, rule.metric_name, current_value, threshold
                ),
                metric_value: Some(current_value),
                threshold_value: Some(threshold),
                status: AlertStatus::Active,
            };

            let alert_id = ProcessAlert::create(self.db.pool(), alert_request.clone()).await?;

            // Log performance degradation alert to telemetry
            if let Err(e) = self.telemetry_writer.log(
                "alert.performance_degradation",
                serde_json::json!({
                    "alert_id": alert_id,
                    "rule_id": rule.id,
                    "rule_name": rule.name,
                    "worker_id": worker.id,
                    "tenant_id": rule.tenant_id,
                    "metric_type": metric_type,
                    "current_value": current_value,
                    "threshold": threshold,
                    "severity": alert_request.severity.to_string(),
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            ) {
                warn!(
                    "Failed to log performance degradation alert to telemetry: {}",
                    e
                );
            }

            info!(
                "Performance degradation alert triggered: {} for worker {} ({} = {:.2} > {:.2})",
                metric_type, worker.id, rule.metric_name, current_value, threshold
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct TenantInfo {
    id: String,
}

#[derive(Debug, Clone)]
struct WorkerInfo {
    id: String,
    #[allow(dead_code)]
    tenant_id: String,
}

/// Drift detection result
#[derive(Debug, Clone)]
struct DriftDetection {
    current_value: f64,
    baseline_mean: f64,
    #[allow(dead_code)]
    baseline_std_dev: f64,
    z_score: f64,
    drift_percentage: f64,
    confidence_score: f64,
    severity: AlertSeverity,
}

/// Baseline statistics
#[derive(Debug, Clone)]
struct BaselineStats {
    mean: f64,
    #[allow(dead_code)]
    std_dev: f64,
    #[allow(dead_code)]
    sample_count: usize,
    #[allow(dead_code)]
    min_value: f64,
    #[allow(dead_code)]
    max_value: f64,
}

/// Adapter information for eviction
#[derive(Debug, Clone)]
struct AdapterInfo {
    id: String,
    category: AdapterCategory,
    last_accessed: chrono::DateTime<chrono::Utc>,
}

/// Adapter category enum
#[derive(Debug, Clone, PartialEq)]
enum AdapterCategory {
    Code,
    Framework,
    Codebase,
    Ephemeral,
}

impl std::str::FromStr for AdapterCategory {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "code" => Ok(AdapterCategory::Code),
            "framework" => Ok(AdapterCategory::Framework),
            "codebase" => Ok(AdapterCategory::Codebase),
            "ephemeral" => Ok(AdapterCategory::Ephemeral),
            _ => Err(()),
        }
    }
}

/// Patch validation metrics
#[derive(Debug, Clone)]
struct PatchValidationMetrics {
    success_rate: f64,
    avg_validation_time_ms: f64,
    total_validations: usize,
}

/// Comprehensive evaluation metrics
#[derive(Debug, Clone, Serialize)]
struct EvaluationMetrics {
    drift_detected: bool,
    anomaly_detected: bool,
    performance_violation: bool,
    memory_violation: bool,
    security_violation: bool,
    compliance_violation: bool,
    patch_validation_failure: bool,
    total_checks: u32,
    successful_checks: u32,
    failed_checks: u32,
    evaluation_duration_ms: u64,
}

/// Performance metrics for monitoring
#[derive(Debug, Clone, Serialize)]
struct PerformanceMetrics {
    evaluation_time_ms: f64,
    memory_usage_mb: f64,
    cpu_usage_pct: f64,
    disk_io_mb_s: f64,
    network_io_mb_s: f64,
    active_connections: u32,
    queue_depth: u32,
    error_rate: f64,
    throughput_ops_per_s: f64,
}

impl Clone for AlertEvaluator {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            telemetry_writer: self.telemetry_writer.clone(),
            config: self.config.clone(),
            notification_sender: self.notification_sender.clone(),
            anomaly_detector: self.anomaly_detector.clone(),
            policy_engine: self
                .policy_engine
                .as_ref()
                .map(|_| PolicyEngine::new(Policies::default())),
            alert_tx: self.alert_tx.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_manifest::Policies;
    use adapteros_telemetry::TelemetryWriter;
    use std::path::Path;

    struct MockNotificationSender;

    #[async_trait::async_trait]
    impl NotificationSender for MockNotificationSender {
        async fn send_notification(&self, _notification: NotificationRequest) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_threshold_checking() {
        let evaluator = AlertEvaluator::new(
            Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap()),
            TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024).unwrap(),
            AlertingConfig::default(),
            Arc::new(MockNotificationSender),
        );

        // Test different threshold operators
        assert!(evaluator.check_threshold(10.0, 5.0, &ThresholdOperator::Gt));
        assert!(!evaluator.check_threshold(3.0, 5.0, &ThresholdOperator::Gt));

        assert!(evaluator.check_threshold(3.0, 5.0, &ThresholdOperator::Lt));
        assert!(!evaluator.check_threshold(10.0, 5.0, &ThresholdOperator::Lt));

        assert!(evaluator.check_threshold(5.0, 5.0, &ThresholdOperator::Eq));
        assert!(!evaluator.check_threshold(5.1, 5.0, &ThresholdOperator::Eq));

        assert!(evaluator.check_threshold(5.0, 5.0, &ThresholdOperator::Gte));
        assert!(evaluator.check_threshold(6.0, 5.0, &ThresholdOperator::Gte));
        assert!(!evaluator.check_threshold(4.0, 5.0, &ThresholdOperator::Gte));

        assert!(evaluator.check_threshold(5.0, 5.0, &ThresholdOperator::Lte));
        assert!(evaluator.check_threshold(4.0, 5.0, &ThresholdOperator::Lte));
        assert!(!evaluator.check_threshold(6.0, 5.0, &ThresholdOperator::Lte));
    }

    #[tokio::test]
    async fn test_alerting_config_defaults() {
        let config = AlertingConfig::default();

        assert_eq!(config.evaluation_interval_secs, 30);
        assert_eq!(config.max_concurrent_evaluations, 10);
        assert_eq!(config.default_cooldown_secs, 300);
        assert_eq!(config.escalation_check_interval_secs, 60);
        assert!(config.enable_escalation);
        assert!(config.enable_notifications);
    }

    #[tokio::test]
    async fn test_drift_detection() {
        let db = Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap());
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024).unwrap();
        let config = AlertingConfig::default();
        let notification_sender = Arc::new(MockNotificationSender);

        let evaluator = AlertEvaluator::new(db, telemetry_writer, config, notification_sender);

        // Test drift detection with mock rule
        let rule = ProcessMonitoringRule {
            id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: Some("Test rule for drift detection".to_string()),
            tenant_id: "test-tenant".to_string(),
            rule_type: RuleType::Cpu,
            metric_name: "cpu_usage".to_string(),
            threshold_value: 80.0,
            threshold_operator: ThresholdOperator::Gt,
            severity: AlertSeverity::Warning,
            evaluation_window_seconds: 300,
            cooldown_seconds: 300,
            is_active: true,
            notification_channels: None,
            escalation_rules: None,
            created_by: Some("test".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Test drift detection (should return None for insufficient data)
        let result = evaluator.detect_drift(&rule, 85.0).await;
        assert!(result.is_err()); // Should fail due to insufficient historical data
    }

    #[tokio::test]
    async fn test_performance_budget_validation() {
        let db = Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap());
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024).unwrap();
        let config = AlertingConfig::default();
        let notification_sender = Arc::new(MockNotificationSender);
        let policies = Policies::default();

        let evaluator = AlertEvaluator::new_with_policy_engine(
            db,
            telemetry_writer,
            config,
            notification_sender,
            policies,
        );

        let rule = ProcessMonitoringRule {
            id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: Some("Test rule for performance budget validation".to_string()),
            tenant_id: "test-tenant".to_string(),
            rule_type: RuleType::Latency,
            metric_name: "latency_ms".to_string(),
            threshold_value: 24.0,
            threshold_operator: ThresholdOperator::Gt,
            severity: AlertSeverity::Warning,
            evaluation_window_seconds: 300,
            cooldown_seconds: 300,
            is_active: true,
            notification_channels: None,
            escalation_rules: None,
            created_by: Some("test".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Test performance budget validation
        let result = evaluator.check_performance_budgets(&rule, 25.0).await;
        assert!(result.is_ok()); // Should succeed even with no metrics
    }

    #[tokio::test]
    async fn test_memory_headroom_validation() {
        let db = Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap());
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024).unwrap();
        let config = AlertingConfig::default();
        let notification_sender = Arc::new(MockNotificationSender);
        let policies = Policies::default();

        let evaluator = AlertEvaluator::new_with_policy_engine(
            db,
            telemetry_writer,
            config,
            notification_sender,
            policies,
        );

        let rule = ProcessMonitoringRule {
            id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: Some("Test rule for memory headroom validation".to_string()),
            tenant_id: "test-tenant".to_string(),
            rule_type: RuleType::Memory,
            metric_name: "memory_usage_pct".to_string(),
            threshold_value: 85.0,
            threshold_operator: ThresholdOperator::Gt,
            severity: AlertSeverity::Warning,
            evaluation_window_seconds: 300,
            cooldown_seconds: 300,
            is_active: true,
            notification_channels: None,
            escalation_rules: None,
            created_by: Some("test".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Test memory headroom validation
        let result = evaluator.check_memory_headroom(&rule).await;
        assert!(result.is_ok()); // Should succeed even with no metrics
    }

    #[tokio::test]
    async fn test_security_event_correlation() {
        let db = Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap());
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024).unwrap();
        let config = AlertingConfig::default();
        let notification_sender = Arc::new(MockNotificationSender);
        let policies = Policies::default();

        let evaluator = AlertEvaluator::new_with_policy_engine(
            db,
            telemetry_writer,
            config,
            notification_sender,
            policies,
        );

        let rule = ProcessMonitoringRule {
            id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: Some("Test rule for security event correlation".to_string()),
            tenant_id: "test-tenant".to_string(),
            rule_type: RuleType::Custom,
            metric_name: "security_events".to_string(),
            threshold_value: 0.0,
            threshold_operator: ThresholdOperator::Gt,
            severity: AlertSeverity::Warning,
            evaluation_window_seconds: 300,
            cooldown_seconds: 300,
            is_active: true,
            notification_channels: None,
            escalation_rules: None,
            created_by: Some("test".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Test security event correlation
        let result = evaluator.correlate_security_events(&rule).await;
        assert!(result.is_ok()); // Should succeed even with no events
    }

    #[tokio::test]
    async fn test_compliance_validation() {
        let db = Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap());
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024).unwrap();
        let config = AlertingConfig::default();
        let notification_sender = Arc::new(MockNotificationSender);
        let policies = Policies::default();

        let evaluator = AlertEvaluator::new_with_policy_engine(
            db,
            telemetry_writer,
            config,
            notification_sender,
            policies,
        );

        let rule = ProcessMonitoringRule {
            id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: Some("Test rule for compliance validation".to_string()),
            tenant_id: "test-tenant".to_string(),
            rule_type: RuleType::Custom,
            metric_name: "compliance_score".to_string(),
            threshold_value: 95.0,
            threshold_operator: ThresholdOperator::Lt,
            severity: AlertSeverity::Warning,
            evaluation_window_seconds: 300,
            cooldown_seconds: 300,
            is_active: true,
            notification_channels: None,
            escalation_rules: None,
            created_by: Some("test".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Test compliance validation
        let result = evaluator.validate_compliance(&rule).await;
        assert!(result.is_ok()); // Should succeed even with no compliance data
    }

    #[tokio::test]
    async fn test_patch_validation_pipeline() {
        let db = Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap());
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024).unwrap();
        let config = AlertingConfig::default();
        let notification_sender = Arc::new(MockNotificationSender);
        let policies = Policies::default();

        let evaluator = AlertEvaluator::new_with_policy_engine(
            db,
            telemetry_writer,
            config,
            notification_sender,
            policies,
        );

        let rule = ProcessMonitoringRule {
            id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: Some("Test rule for patch validation pipeline".to_string()),
            tenant_id: "test-tenant".to_string(),
            rule_type: RuleType::Custom,
            metric_name: "patch_validation_success_rate".to_string(),
            threshold_value: 95.0,
            threshold_operator: ThresholdOperator::Lt,
            severity: AlertSeverity::Warning,
            evaluation_window_seconds: 300,
            cooldown_seconds: 300,
            is_active: true,
            notification_channels: None,
            escalation_rules: None,
            created_by: Some("test".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Test patch validation pipeline
        let result = evaluator.validate_patch_pipeline(&rule).await;
        assert!(result.is_ok()); // Should succeed even with no patch data
    }

    #[tokio::test]
    async fn test_performance_monitoring() {
        let db = Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap());
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024).unwrap();
        let config = AlertingConfig::default();
        let notification_sender = Arc::new(MockNotificationSender);
        let policies = Policies::default();

        let evaluator = AlertEvaluator::new_with_policy_engine(
            db,
            telemetry_writer,
            config,
            notification_sender,
            policies,
        );

        let rule = ProcessMonitoringRule {
            id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: Some("Test rule for performance monitoring".to_string()),
            tenant_id: "test-tenant".to_string(),
            rule_type: RuleType::Latency,
            metric_name: "evaluation_time_ms".to_string(),
            threshold_value: 100.0,
            threshold_operator: ThresholdOperator::Gt,
            severity: AlertSeverity::Warning,
            evaluation_window_seconds: 300,
            cooldown_seconds: 300,
            is_active: true,
            notification_channels: None,
            escalation_rules: None,
            created_by: Some("test".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Test performance monitoring
        let result = evaluator.monitor_performance_metrics(&rule, 105.0).await;
        assert!(result.is_ok()); // Should succeed even with no metrics
    }

    #[tokio::test]
    async fn test_full_features_evaluator() {
        let db = Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap());
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024).unwrap();
        let config = AlertingConfig::default();
        let notification_sender = Arc::new(MockNotificationSender);
        let anomaly_config = AnomalyConfig::default();
        let policies = Policies::default();

        let evaluator = AlertEvaluator::new_with_full_features(
            db,
            telemetry_writer,
            config,
            notification_sender,
            anomaly_config,
            policies,
        );

        // Verify all features are enabled
        assert!(evaluator.anomaly_detector.is_some());
        assert!(evaluator.policy_engine.is_some());
    }

    #[tokio::test]
    async fn test_anomaly_config_defaults() {
        let config = AnomalyConfig::default();

        assert_eq!(config.scan_interval_secs, 300);
        assert_eq!(config.z_score_threshold, 3.0);
        assert_eq!(config.iqr_multiplier, 1.5);
        assert_eq!(config.rate_of_change_threshold, 2.0);
        assert_eq!(config.min_samples_for_baseline, 100);
        assert_eq!(config.baseline_window_days, 7);
        assert_eq!(config.confidence_threshold, 0.7);
        assert!(config.enable_zscore);
        assert!(config.enable_iqr);
        assert!(config.enable_rate_of_change);
    }
}
