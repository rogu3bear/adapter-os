//! Alert deduplication service
//! 【2025-11-07†refactor(server)†extract-alert-dedup】
//!
//! Consolidates duplicate alert checking logic from handlers.rs.
//! Prevents creating duplicate alerts by checking if an alert with the same rule_id already exists.

use adapteros_core::Result;
use adapteros_db::process_monitoring::{
    AlertFilters, AlertStatus, CreateAlertRequest, ProcessAlert,
};
use sqlx::SqlitePool;
use tracing::warn;

/// Check if an alert with the given rule_id already exists
/// 【2025-11-07†refactor(server)†extract-alert-dedup】
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `rule_id` - The rule ID to check for duplicates
/// * `tenant_id` - Optional tenant ID filter (defaults to "default" if None)
///
/// # Returns
/// `true` if an active alert with this rule_id exists, `false` otherwise
pub async fn check_alert_exists(
    pool: &SqlitePool,
    rule_id: &str,
    tenant_id: Option<&str>,
) -> Result<bool> {
    let tenant_id = tenant_id.unwrap_or("default");

    let existing_alerts = ProcessAlert::list(
        pool,
        AlertFilters {
            tenant_id: Some(tenant_id.to_string()),
            status: Some(AlertStatus::Active),
            ..Default::default()
        },
    )
    .await?;

    Ok(existing_alerts.iter().any(|alert| alert.rule_id == rule_id))
}

/// Create an alert if it doesn't already exist
/// 【2025-11-07†refactor(server)†extract-alert-dedup】
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `alert_request` - The alert creation request
/// * `tenant_id` - Optional tenant ID filter (defaults to "default" if None)
///
/// # Returns
/// `Ok(true)` if alert was created, `Ok(false)` if it already existed, `Err` on database error
pub async fn create_alert_if_not_exists(
    pool: &SqlitePool,
    alert_request: &CreateAlertRequest,
    tenant_id: Option<&str>,
) -> Result<bool> {
    let exists = check_alert_exists(pool, &alert_request.rule_id, tenant_id).await?;

    if exists {
        return Ok(false);
    }

    ProcessAlert::create(pool, alert_request.clone())
        .await
        .map(|_| true)
        .map_err(|e| {
            warn!(
                error = %e,
                rule_id = %alert_request.rule_id,
                "Failed to create alert"
            );
            e
        })
}
