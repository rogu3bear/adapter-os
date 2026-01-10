//! Error Alert Evaluator Service
//!
//! Evaluates client error events against configured alert rules and triggers
//! alerts when thresholds are exceeded.

use adapteros_db::client_errors::{ClientError, ErrorAlertRule};
use adapteros_db::Db;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Cooldown tracker to prevent alert spam
/// Maps rule_id -> last triggered timestamp
type CooldownMap = HashMap<String, chrono::DateTime<chrono::Utc>>;

/// Error alert evaluator that checks incoming errors against configured rules
pub struct ErrorAlertEvaluator {
    db: Arc<Db>,
    cooldowns: Arc<RwLock<CooldownMap>>,
}

impl ErrorAlertEvaluator {
    /// Create a new evaluator instance
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            cooldowns: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Evaluate an incoming error against all active rules for the tenant
    ///
    /// This should be called after persisting the error to the database.
    /// It checks if any alert thresholds have been exceeded and triggers alerts.
    pub async fn evaluate(&self, error: &ClientError) -> Result<Vec<String>, String> {
        let tenant_id = &error.tenant_id;

        // Fetch active alert rules for this tenant
        let rules = self
            .db
            .list_error_alert_rules(tenant_id)
            .await
            .map_err(|e| format!("Failed to fetch alert rules: {}", e))?;

        let active_rules: Vec<_> = rules.into_iter().filter(|r| r.is_active != 0).collect();

        if active_rules.is_empty() {
            return Ok(vec![]);
        }

        let mut triggered_alerts = Vec::new();
        let now = chrono::Utc::now();

        for rule in active_rules {
            // Check if error matches rule patterns
            if !self.matches_rule(error, &rule) {
                continue;
            }

            // Check cooldown
            if self
                .is_in_cooldown(&rule.id, rule.cooldown_minutes, now)
                .await
            {
                debug!(
                    rule_id = %rule.id,
                    rule_name = %rule.name,
                    "Alert rule in cooldown, skipping"
                );
                continue;
            }

            // Count errors in the threshold window
            let error_count = self
                .db
                .count_errors_in_window(
                    tenant_id,
                    rule.error_type_pattern.as_deref(),
                    rule.http_status_pattern.as_deref(),
                    rule.page_pattern.as_deref(),
                    rule.threshold_window_minutes as i64,
                )
                .await
                .unwrap_or(0);

            // Check if threshold exceeded
            if error_count >= rule.threshold_count as i64 {
                info!(
                    rule_id = %rule.id,
                    rule_name = %rule.name,
                    error_count = error_count,
                    threshold = rule.threshold_count,
                    "Alert threshold exceeded, triggering alert"
                );

                // Get sample error IDs (up to 5)
                let sample_ids = self
                    .get_sample_error_ids(tenant_id, &rule)
                    .await
                    .unwrap_or_default();

                // Insert alert history record
                match self
                    .db
                    .insert_error_alert_history(
                        &rule.id,
                        tenant_id,
                        error_count as i32,
                        Some(&sample_ids),
                    )
                    .await
                {
                    Ok(alert_id) => {
                        triggered_alerts.push(alert_id.clone());

                        // Set cooldown
                        self.set_cooldown(&rule.id, now).await;

                        info!(
                            alert_id = %alert_id,
                            rule_id = %rule.id,
                            rule_name = %rule.name,
                            severity = %rule.severity,
                            "Error alert triggered"
                        );
                    }
                    Err(e) => {
                        warn!(
                            rule_id = %rule.id,
                            error = %e,
                            "Failed to insert alert history"
                        );
                    }
                }
            }
        }

        Ok(triggered_alerts)
    }

    /// Check if an error matches a rule's patterns
    fn matches_rule(&self, error: &ClientError, rule: &ErrorAlertRule) -> bool {
        // Check error_type pattern (exact match or empty for all)
        if let Some(ref pattern) = rule.error_type_pattern {
            if !pattern.is_empty() && error.error_type != *pattern {
                return false;
            }
        }

        // Check http_status pattern
        if let Some(ref pattern) = rule.http_status_pattern {
            if !pattern.is_empty() {
                if let Some(status) = error.http_status {
                    let status_str = status.to_string();
                    let matches = if pattern.ends_with("xx") || pattern.ends_with("XX") {
                        // Pattern like "4xx" or "5xx"
                        status_str.starts_with(&pattern[..1])
                    } else {
                        // Exact match
                        status_str == *pattern
                    };
                    if !matches {
                        return false;
                    }
                } else {
                    // No HTTP status on error but rule requires one
                    return false;
                }
            }
        }

        // Check page pattern (glob-style matching)
        if let Some(ref pattern) = rule.page_pattern {
            if !pattern.is_empty() {
                if let Some(ref page) = error.page {
                    if !Self::glob_match(page, pattern) {
                        return false;
                    }
                } else {
                    // No page on error but rule requires one
                    return false;
                }
            }
        }

        true
    }

    /// Simple glob matching (* matches any sequence, ? matches single char)
    fn glob_match(text: &str, pattern: &str) -> bool {
        let regex_pattern = pattern
            .replace('.', "\\.")
            .replace('*', ".*")
            .replace('?', ".");

        regex::Regex::new(&format!("^{}$", regex_pattern))
            .map(|re| re.is_match(text))
            .unwrap_or(false)
    }

    /// Check if a rule is in cooldown
    async fn is_in_cooldown(
        &self,
        rule_id: &str,
        cooldown_minutes: i32,
        now: chrono::DateTime<chrono::Utc>,
    ) -> bool {
        let cooldowns = self.cooldowns.read().await;
        if let Some(last_triggered) = cooldowns.get(rule_id) {
            let cooldown_duration = chrono::Duration::minutes(cooldown_minutes as i64);
            return now < *last_triggered + cooldown_duration;
        }
        false
    }

    /// Set cooldown for a rule
    async fn set_cooldown(&self, rule_id: &str, now: chrono::DateTime<chrono::Utc>) {
        let mut cooldowns = self.cooldowns.write().await;
        cooldowns.insert(rule_id.to_string(), now);
    }

    /// Get sample error IDs for an alert
    async fn get_sample_error_ids(
        &self,
        tenant_id: &str,
        rule: &ErrorAlertRule,
    ) -> Result<Vec<String>, String> {
        use adapteros_db::client_errors::ClientErrorQuery;

        let query = ClientErrorQuery {
            tenant_id: tenant_id.to_string(),
            error_type: rule.error_type_pattern.clone(),
            http_status: rule
                .http_status_pattern
                .as_ref()
                .and_then(|s| s.parse().ok()),
            page_pattern: rule.page_pattern.clone(),
            user_id: None, // Alert rules don't filter by user
            since: Some(
                (chrono::Utc::now()
                    - chrono::Duration::minutes(rule.threshold_window_minutes as i64))
                .to_rfc3339(),
            ),
            until: None,
            limit: Some(5),
            offset: None,
        };

        let errors = self
            .db
            .list_client_errors(&query)
            .await
            .map_err(|e| e.to_string())?;

        Ok(errors.into_iter().map(|e| e.id).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        // Basic patterns
        assert!(ErrorAlertEvaluator::glob_match("/api/users", "/api/*"));
        assert!(ErrorAlertEvaluator::glob_match("/api/users/123", "/api/**"));
        assert!(ErrorAlertEvaluator::glob_match("/chat", "/chat"));
        assert!(!ErrorAlertEvaluator::glob_match("/api/users", "/chat/*"));

        // Question mark
        assert!(ErrorAlertEvaluator::glob_match("/api/v1", "/api/v?"));
        assert!(!ErrorAlertEvaluator::glob_match("/api/v12", "/api/v?"));
    }
}
