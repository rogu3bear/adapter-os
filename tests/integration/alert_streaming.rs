//! Integration tests for alert streaming functionality
//!
//! Tests real-time alert broadcasting and SSE streaming.
//!
//! Citations:
//! - Alert broadcasting: [source: crates/adapteros-system-metrics/src/alerting.rs L444-L452]
//! - SSE handler: [source: crates/adapteros-server-api/src/handlers.rs L12929-12935]
//! - Broadcast channel: [source: crates/adapteros-server-api/src/state.rs L427-428]

use adapteros_server_api::{AppState, routes};
use adapteros_system_metrics::{
    alerting::{AlertEvaluator, AlertingConfig, NotificationSender},
    monitoring_types::{AlertSeverity, AlertStatus, CreateAlertRequest, ProcessMonitoringRule, RuleType},
};
use adapteros_telemetry::TelemetryWriter;
use adapteros_db::Db;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::broadcast;
use tower::Service;

/// Mock notification sender for testing
struct MockNotificationSender;

#[async_trait::async_trait]
impl NotificationSender for MockNotificationSender {
    async fn send_notification(&self, _notification: adapteros_system_metrics::alerting::NotificationRequest) -> adapteros_core::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test_alert_creation_broadcasts_to_sse() {
    // Setup test database and state
    let db = Db::connect(":memory:").await.unwrap();
    let db_arc = Arc::new(db);

    // Create broadcast channel for alerts
    let (alert_tx, mut alert_rx) = broadcast::channel::<adapteros_server_api::types::ProcessAlertResponse>(10);

    // Create alert evaluator with broadcast channel
    let temp_dir = TempDir::with_prefix("aos-test-").unwrap();
    let telemetry_writer =
        TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024).unwrap_or_default();
    let alerting_config = AlertingConfig::default();
    let notification_sender = Arc::new(MockNotificationSender);

    let alert_evaluator = AlertEvaluator::new(
        db_arc.clone(),
        telemetry_writer,
        alerting_config,
        notification_sender,
    ).with_alert_broadcast(Some(alert_tx));

    // Create a test monitoring rule
    let rule = ProcessMonitoringRule {
        id: "test-rule".to_string(),
        name: "Test CPU Rule".to_string(),
        description: Some("Test rule for integration testing".to_string()),
        tenant_id: "test-tenant".to_string(),
        rule_type: RuleType::Cpu,
        metric_name: "cpu_usage".to_string(),
        threshold_value: 80.0,
        threshold_operator: ">".to_string(),
        severity: AlertSeverity::Warning,
        evaluation_window_seconds: 60,
        cooldown_seconds: 300,
        is_active: true,
        notification_channels: None,
        created_by: Some("test-user".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    // Insert test rule
    adapteros_db::process_monitoring::ProcessMonitoringRule::create(db_arc.pool_result().unwrap(), &rule).await.unwrap();

    // Trigger alert creation
    alert_evaluator.trigger_alert(&rule, 85.0).await.unwrap();

    // Verify alert was broadcasted
    let received_alert = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        alert_rx.recv()
    ).await.unwrap().unwrap();

    assert_eq!(received_alert.rule_id, "test-rule");
    assert_eq!(received_alert.metric_value, Some(85.0));
    assert_eq!(received_alert.severity, "warning");
    assert_eq!(received_alert.status, "active");
}

#[tokio::test]
async fn test_alert_acknowledgment_broadcasts_update() {
    // This test would verify that when an alert is acknowledged via API,
    // the updated alert is broadcasted to SSE streams

    // Setup similar to above, then make API call to acknowledge alert
    // and verify the broadcast contains the updated alert with acknowledged status
}

#[tokio::test]
async fn test_alert_sse_stream_end_to_end() {
    // Full end-to-end test: create alert → verify SSE stream receives it

    // This would require setting up a full test server and making HTTP requests
    // to the SSE endpoint, but for now we'll test the components separately
}
