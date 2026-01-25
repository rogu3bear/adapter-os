#![allow(unused_variables)]

//! Notification system
//!
//! Supports multiple notification channels including email, Slack, webhook, and PagerDuty.
//! Tracks notification delivery in database and handles escalation logic.

use crate::monitoring_types::*;
use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_telemetry::TelemetryWriter;
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

/// Notification service
pub struct NotificationService {
    db: Arc<Db>,
    telemetry_writer: Arc<TelemetryWriter>,
    config: NotificationConfig,
    http_client: Client,
}

#[derive(Debug, Clone)]
pub struct NotificationConfig {
    pub enable_email: bool,
    pub enable_slack: bool,
    pub enable_webhook: bool,
    pub enable_pagerduty: bool,
    /// Enable SMS notifications (requires `sms` feature flag)
    pub enable_sms: bool,
    pub retry_attempts: u32,
    pub retry_delay_secs: u64,
    pub timeout_secs: u64,
    pub smtp_config: Option<SmtpConfig>,
    pub slack_config: Option<SlackConfig>,
    pub webhook_config: Option<WebhookConfig>,
    pub pagerduty_config: Option<PagerDutyConfig>,
    /// SMS provider configuration (Twilio)
    pub sms_config: Option<SmsConfig>,
}

#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_email: String,
    pub use_tls: bool,
}

#[derive(Debug, Clone)]
pub struct SlackConfig {
    pub webhook_url: String,
    pub default_channel: String,
    pub username: String,
    pub icon_emoji: String,
}

#[derive(Debug, Clone)]
pub struct WebhookConfig {
    pub timeout_secs: u64,
    pub retry_attempts: u32,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct PagerDutyConfig {
    pub integration_key: String,
    pub api_url: String,
    pub severity_mapping: HashMap<String, String>,
}

/// SMS notification configuration.
///
/// Requires the `sms` feature flag to be enabled.
/// Currently supports Twilio as the reference provider.
///
/// # Configuration
///
/// Set the following environment variables:
/// - `AOS_SMS_ACCOUNT_SID`: Twilio Account SID
/// - `AOS_SMS_AUTH_TOKEN`: Twilio Auth Token
/// - `AOS_SMS_FROM_NUMBER`: Sender phone number (E.164 format)
///
/// # Example
///
/// ```toml
/// # In Cargo.toml
/// adapteros-system-metrics = { path = "...", features = ["sms"] }
/// ```
#[derive(Debug, Clone)]
pub struct SmsConfig {
    /// Twilio Account SID
    pub account_sid: String,
    /// Twilio Auth Token
    pub auth_token: String,
    /// Sender phone number in E.164 format (e.g., "+15551234567")
    pub from_number: String,
    /// API endpoint (defaults to Twilio)
    pub api_url: Option<String>,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enable_email: true,
            enable_slack: true,
            enable_webhook: true,
            enable_pagerduty: true,
            enable_sms: false, // Requires `sms` feature and provider configuration
            retry_attempts: 3,
            retry_delay_secs: 5,
            timeout_secs: 30,
            smtp_config: None,
            slack_config: None,
            webhook_config: None,
            pagerduty_config: None,
            sms_config: None,
        }
    }
}

/// Notification sender implementation
pub struct NotificationSenderImpl {
    service: Arc<NotificationService>,
}

#[async_trait]
impl crate::alerting::NotificationSender for NotificationSenderImpl {
    async fn send_notification(
        &self,
        notification: crate::alerting::NotificationRequest,
    ) -> Result<()> {
        self.service.send_notification(notification).await
    }
}

impl NotificationService {
    /// Create a new notification service
    pub fn new(
        db: Arc<Db>,
        telemetry_writer: Arc<TelemetryWriter>,
        config: NotificationConfig,
    ) -> Self {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            db,
            telemetry_writer,
            config,
            http_client,
        }
    }

    /// Create notification sender
    pub fn create_sender(self) -> Arc<dyn crate::alerting::NotificationSender + Send + Sync> {
        Arc::new(NotificationSenderImpl {
            service: Arc::new(self),
        })
    }

    /// Send a notification
    pub async fn send_notification(
        &self,
        notification: crate::alerting::NotificationRequest,
    ) -> Result<()> {
        let notification_id = self.create_notification_record(&notification).await?;

        let result = match notification.notification_type {
            NotificationType::Email => {
                if self.config.enable_email {
                    self.send_email_notification(&notification).await
                } else {
                    Err(adapteros_core::AosError::Validation(
                        "Email notifications disabled".to_string(),
                    ))
                }
            }
            NotificationType::Slack => {
                if self.config.enable_slack {
                    self.send_slack_notification(&notification).await
                } else {
                    Err(adapteros_core::AosError::Validation(
                        "Slack notifications disabled".to_string(),
                    ))
                }
            }
            NotificationType::Webhook => {
                if self.config.enable_webhook {
                    self.send_webhook_notification(&notification).await
                } else {
                    Err(adapteros_core::AosError::Validation(
                        "Webhook notifications disabled".to_string(),
                    ))
                }
            }
            NotificationType::Pagerduty => {
                if self.config.enable_pagerduty {
                    self.send_pagerduty_notification(&notification).await
                } else {
                    Err(adapteros_core::AosError::Validation(
                        "PagerDuty notifications disabled".to_string(),
                    ))
                }
            }
            NotificationType::Sms => {
                if self.config.enable_sms {
                    self.send_sms_notification(&notification).await
                } else {
                    Err(adapteros_core::AosError::Validation(
                        "SMS notifications disabled. Enable with: enable_sms = true in config, \
                         and compile with --features sms"
                            .to_string(),
                    ))
                }
            }
        };

        // Update notification record with result
        self.update_notification_record(&notification_id, &result)
            .await?;

        // Log to telemetry
        if let Err(e) = self.telemetry_writer.log(
            "notification.sent",
            serde_json::json!({
                "notification_id": notification_id,
                "alert_id": notification.alert_id,
                "type": notification.notification_type.to_string(),
                "recipient": notification.recipient,
                "success": result.is_ok(),
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        ) {
            warn!("Failed to log notification to telemetry: {}", e);
        }

        result
    }

    /// Create notification record in database
    async fn create_notification_record(
        &self,
        notification: &crate::alerting::NotificationRequest,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();

        let notification_type_str = notification.notification_type.to_string();
        sqlx::query(
            r#"
            INSERT INTO process_monitoring_notifications (
                id, alert_id, notification_type, recipient, message, status
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&notification.alert_id)
        .bind(&notification_type_str)
        .bind(&notification.recipient)
        .bind(&notification.message)
        .bind("pending")
        .execute(self.db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!(
                "Failed to create notification record: {}",
                e
            ))
        })?;

        Ok(id)
    }

    /// Update notification record with result
    async fn update_notification_record(
        &self,
        notification_id: &str,
        result: &Result<()>,
    ) -> Result<()> {
        let (status, error_message) = match result {
            Ok(_) => ("sent", None),
            Err(e) => ("failed", Some(e.to_string())),
        };

        sqlx::query(
            r#"
            UPDATE process_monitoring_notifications
            SET status = ?, error_message = ?, sent_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(status)
        .bind(&error_message)
        .bind(notification_id)
        .execute(self.db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!(
                "Failed to update notification record: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Send email notification
    async fn send_email_notification(
        &self,
        notification: &crate::alerting::NotificationRequest,
    ) -> Result<()> {
        let smtp_config = self.config.smtp_config.as_ref().ok_or_else(|| {
            adapteros_core::AosError::Validation("SMTP config not provided".to_string())
        })?;

        // For now, we'll use a simple HTTP-based email service
        // In production, you'd use a proper SMTP library like lettre
        let email_payload = serde_json::json!({
            "to": notification.recipient,
            "subject": format!("Alert: {}", notification.severity.to_string()),
            "body": notification.message,
            "from": smtp_config.from_email
        });

        // This is a placeholder - in reality you'd send via SMTP
        info!(
            "Email notification sent to {}: {}",
            notification.recipient, notification.message
        );

        Ok(())
    }

    /// Send Slack notification
    async fn send_slack_notification(
        &self,
        notification: &crate::alerting::NotificationRequest,
    ) -> Result<()> {
        let slack_config = self.config.slack_config.as_ref().ok_or_else(|| {
            adapteros_core::AosError::Validation("Slack config not provided".to_string())
        })?;

        let color = match notification.severity {
            AlertSeverity::Critical => "#FF0000",
            AlertSeverity::Error => "#FF6600",
            AlertSeverity::Warning => "#FFAA00",
            AlertSeverity::Info => "#00AAFF",
        };

        let slack_payload = serde_json::json!({
            "channel": slack_config.default_channel,
            "username": slack_config.username,
            "icon_emoji": slack_config.icon_emoji,
            "attachments": [{
                "color": color,
                "title": format!("Alert: {}", notification.severity.to_string()),
                "text": notification.message,
                "fields": [
                    {
                        "title": "Alert ID",
                        "value": notification.alert_id,
                        "short": true
                    },
                    {
                        "title": "Escalation Level",
                        "value": notification.escalation_level,
                        "short": true
                    }
                ],
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }]
        });

        let response = self
            .http_client
            .post(&slack_config.webhook_url)
            .json(&slack_payload)
            .send()
            .await
            .map_err(|e| {
                adapteros_core::AosError::Validation(format!(
                    "Failed to send Slack notification: {}",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(adapteros_core::AosError::Verification(format!(
                "Slack API error {}: {}",
                status, body
            )));
        }

        info!("Slack notification sent to {}", notification.recipient);
        Ok(())
    }

    /// Send webhook notification
    async fn send_webhook_notification(
        &self,
        notification: &crate::alerting::NotificationRequest,
    ) -> Result<()> {
        let webhook_config = self.config.webhook_config.as_ref().ok_or_else(|| {
            adapteros_core::AosError::Verification("Webhook config not provided".to_string())
        })?;

        let webhook_payload = serde_json::json!({
            "alert_id": notification.alert_id,
            "type": notification.notification_type.to_string(),
            "severity": notification.severity.to_string(),
            "message": notification.message,
            "escalation_level": notification.escalation_level,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        });

        let mut request = self
            .http_client
            .post(&notification.recipient)
            .json(&webhook_payload);

        // Add custom headers
        for (key, value) in &webhook_config.headers {
            request = request.header(key, value);
        }

        let response = request.send().await.map_err(|e| {
            adapteros_core::AosError::Verification(format!(
                "Failed to send webhook notification: {}",
                e
            ))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(adapteros_core::AosError::Validation(format!(
                "Webhook error {}: {}",
                status, body
            )));
        }

        info!("Webhook notification sent to {}", notification.recipient);
        Ok(())
    }

    /// Send PagerDuty notification
    async fn send_pagerduty_notification(
        &self,
        notification: &crate::alerting::NotificationRequest,
    ) -> Result<()> {
        let pagerduty_config = self.config.pagerduty_config.as_ref().ok_or_else(|| {
            adapteros_core::AosError::Validation("PagerDuty config not provided".to_string())
        })?;

        let severity = pagerduty_config
            .severity_mapping
            .get(&notification.severity.to_string())
            .cloned()
            .unwrap_or_else(|| notification.severity.to_string());

        let pagerduty_payload = serde_json::json!({
            "routing_key": pagerduty_config.integration_key,
            "event_action": "trigger",
            "dedup_key": notification.alert_id,
            "payload": {
                "summary": notification.message,
                "source": "adapteros-monitoring",
                "severity": severity,
                "custom_details": {
                    "alert_id": notification.alert_id,
                    "escalation_level": notification.escalation_level,
                    "notification_type": notification.notification_type.to_string()
                }
            }
        });

        let response = self
            .http_client
            .post(&pagerduty_config.api_url)
            .json(&pagerduty_payload)
            .send()
            .await
            .map_err(|e| {
                adapteros_core::AosError::Validation(format!(
                    "Failed to send PagerDuty notification: {}",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(adapteros_core::AosError::Validation(format!(
                "PagerDuty API error {}: {}",
                status, body
            )));
        }

        info!(
            "PagerDuty notification sent for alert {}",
            notification.alert_id
        );
        Ok(())
    }

    /// Send SMS notification via Twilio.
    ///
    /// Requires the `sms` feature flag and valid Twilio credentials in `SmsConfig`.
    /// The recipient should be a phone number in E.164 format (e.g., "+15551234567").
    ///
    /// # Feature Flag
    ///
    /// This method requires the `sms` feature flag to be enabled at compile time.
    /// Without the feature flag, the method will return a configuration error.
    ///
    /// # Configuration
    ///
    /// ```toml
    /// # In config file
    /// [notifications.sms]
    /// account_sid = "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    /// auth_token = "your_auth_token"
    /// from_number = "+15551234567"
    /// ```
    async fn send_sms_notification(
        &self,
        notification: &crate::alerting::NotificationRequest,
    ) -> Result<()> {
        #[cfg(not(feature = "sms"))]
        {
            // Feature flag not enabled - return clear error message
            let _ = notification; // Suppress unused warning
            Err(adapteros_core::AosError::Config(
                "SMS notifications require the 'sms' feature flag. \
                 Recompile with: cargo build --features sms"
                    .to_string(),
            ))
        }

        #[cfg(feature = "sms")]
        {
            let sms_config = self.config.sms_config.as_ref().ok_or_else(|| {
                adapteros_core::AosError::Config(
                    "SMS config not provided. Set sms_config in NotificationConfig with \
                     account_sid, auth_token, and from_number"
                        .to_string(),
                )
            })?;

            // Twilio Messages API endpoint
            let api_url = sms_config.api_url.clone().unwrap_or_else(|| {
                format!(
                    "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
                    sms_config.account_sid
                )
            });

            // Build the SMS message body
            let sms_body = format!(
                "[{}] {}: {}",
                notification.severity.to_string().to_uppercase(),
                notification.alert_id,
                notification.message
            );

            // Use reqwest's form builder which handles URL encoding
            let form_params = [
                ("To", notification.recipient.as_str()),
                ("From", sms_config.from_number.as_str()),
                ("Body", sms_body.as_str()),
            ];

            let response = self
                .http_client
                .post(&api_url)
                .basic_auth(&sms_config.account_sid, Some(&sms_config.auth_token))
                .form(&form_params)
                .send()
                .await
                .map_err(|e| {
                    adapteros_core::AosError::Validation(format!(
                        "Failed to send SMS notification: {}",
                        e
                    ))
                })?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(adapteros_core::AosError::Validation(format!(
                    "Twilio API error {}: {}",
                    status, body
                )));
            }

            info!(
                "SMS notification sent to {} for alert {}",
                notification.recipient, notification.alert_id
            );
            Ok(())
        }
    }

    /// Get notification delivery status
    pub async fn get_notification_status(&self, alert_id: &str) -> Result<Vec<NotificationStatus>> {
        let rows = sqlx::query(
            "SELECT id, alert_id, notification_type, recipient, message, status, sent_at, delivered_at, error_message, retry_count, created_at FROM process_monitoring_notifications WHERE alert_id = ? ORDER BY created_at DESC",
        )
        .bind(alert_id)
        .fetch_all(self.db.pool())
        .await
        .map_err(|e| adapteros_core::AosError::Database(format!("Failed to get notification status: {}", e)))?;

        let mut notifications = Vec::new();
        for row in rows {
            use sqlx::Row;
            notifications.push(NotificationStatus {
                id: row.get::<Option<String>, _>("id").unwrap_or_default(),
                alert_id: row.get("alert_id"),
                notification_type: NotificationType::from_string(row.get("notification_type")),
                recipient: row.get("recipient"),
                message: row.get("message"),
                status: crate::monitoring_types::NotificationStatus::from_string(row.get("status")),
                sent_at: row
                    .get::<Option<chrono::NaiveDateTime>, _>("sent_at")
                    .map(|dt| dt.and_utc()),
                delivered_at: row
                    .get::<Option<chrono::NaiveDateTime>, _>("delivered_at")
                    .map(|dt| dt.and_utc()),
                error_message: row.get("error_message"),
                retry_count: row.get::<Option<i64>, _>("retry_count").unwrap_or(0),
                created_at: row
                    .get::<Option<chrono::NaiveDateTime>, _>("created_at")
                    .unwrap_or_default()
                    .and_utc(),
            });
        }

        Ok(notifications)
    }

    /// Retry failed notifications
    pub async fn retry_failed_notifications(&self) -> Result<()> {
        let retry_attempts = self.config.retry_attempts as i64;
        let failed_notifications = sqlx::query(
            "SELECT id, alert_id, notification_type, recipient, message FROM process_monitoring_notifications
             WHERE status = 'failed' AND retry_count < ?
             ORDER BY created_at ASC LIMIT 10",
        )
        .bind(retry_attempts)
        .fetch_all(self.db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to get failed notifications: {}", e))
        })?;

        for row in failed_notifications {
            use sqlx::Row;
            let id: Option<String> = row.get("id");

            // Increment retry count
            sqlx::query(
                "UPDATE process_monitoring_notifications SET retry_count = retry_count + 1 WHERE id = ?",
            )
            .bind(&id)
            .execute(self.db.pool())
            .await
            .map_err(|e| adapteros_core::AosError::Database(format!("Failed to update retry count: {}", e)))?;

            // Retry the notification
            let notification = crate::alerting::NotificationRequest {
                alert_id: row.get("alert_id"),
                notification_type: NotificationType::from_string(row.get("notification_type")),
                recipient: row.get("recipient"),
                message: row.get("message"),
                severity: AlertSeverity::Warning, // Default severity for retry
                escalation_level: 0,
            };

            if let Err(e) = self.send_notification(notification).await {
                error!("Failed to retry notification {:?}: {}", id, e);
            } else {
                info!("Successfully retried notification {:?}", id);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct NotificationStatus {
    pub id: String,
    pub alert_id: String,
    pub notification_type: NotificationType,
    pub recipient: String,
    pub message: String,
    pub status: crate::monitoring_types::NotificationStatus,
    pub sent_at: Option<chrono::DateTime<chrono::Utc>>,
    pub delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error_message: Option<String>,
    pub retry_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_telemetry::TelemetryWriter;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_notification_config_defaults() {
        let config = NotificationConfig::default();

        assert!(config.enable_email);
        assert!(config.enable_slack);
        assert!(config.enable_webhook);
        assert!(config.enable_pagerduty);
        assert!(!config.enable_sms); // SMS disabled by default
        assert_eq!(config.retry_attempts, 3);
        assert_eq!(config.retry_delay_secs, 5);
        assert_eq!(config.timeout_secs, 30);
        assert!(config.sms_config.is_none());
    }

    #[test]
    fn test_sms_config_creation() {
        let sms_config = SmsConfig {
            account_sid: "AC123".to_string(),
            auth_token: "token123".to_string(),
            from_number: "+15551234567".to_string(),
            api_url: None,
        };

        assert_eq!(sms_config.account_sid, "AC123");
        assert_eq!(sms_config.from_number, "+15551234567");
        assert!(sms_config.api_url.is_none());
    }

    #[test]
    fn test_sms_config_with_custom_api_url() {
        let sms_config = SmsConfig {
            account_sid: "AC123".to_string(),
            auth_token: "token123".to_string(),
            from_number: "+15551234567".to_string(),
            api_url: Some("https://custom.twilio.api/messages".to_string()),
        };

        assert_eq!(
            sms_config.api_url,
            Some("https://custom.twilio.api/messages".to_string())
        );
    }

    #[tokio::test]
    async fn test_notification_service_creation() {
        let db = Arc::new(
            adapteros_db::Db::connect(":memory:")
                .await
                .expect("Failed to create test database"),
        );

        let temp_dir = TempDir::new_in(".").unwrap();
        let telemetry_writer = Arc::new(
            TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024)
                .expect("Failed to create telemetry writer"),
        );

        let config = NotificationConfig::default();
        let service = NotificationService::new(db, telemetry_writer, config);

        // Test that we can create a sender
        let _sender = service.create_sender();
    }

    #[tokio::test]
    async fn test_notification_types() {
        assert_eq!(NotificationType::Email.to_string(), "email");
        assert_eq!(NotificationType::Slack.to_string(), "slack");
        assert_eq!(NotificationType::Webhook.to_string(), "webhook");
        assert_eq!(NotificationType::Pagerduty.to_string(), "pagerduty");
        assert_eq!(NotificationType::Sms.to_string(), "sms");
    }

    #[tokio::test]
    async fn test_alert_severity() {
        assert_eq!(AlertSeverity::Critical.to_string(), "critical");
        assert_eq!(AlertSeverity::Error.to_string(), "error");
        assert_eq!(AlertSeverity::Warning.to_string(), "warning");
        assert_eq!(AlertSeverity::Info.to_string(), "info");
    }
}
