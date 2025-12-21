//! Rate limiting per tenant for DDoS protection
//!
//! Implements sliding window rate limiting with tenant-specific quotas.
use adapteros_config::try_effective_config;
use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RateLimitBucket {
    pub tenant_id: String,
    pub requests_count: i64,
    pub window_start: String,
    pub window_size_seconds: i64,
    pub max_requests: i64,
    pub last_updated: String,
}

#[derive(Debug, Clone, Copy)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub current_count: i64,
    pub limit: i64,
    pub reset_at: i64, // Unix timestamp
}

/// Check and increment rate limit for a tenant
///
/// Returns `Ok(RateLimitResult)` with allowed=true if within limit,
/// or allowed=false if rate limit exceeded.
///
/// Rate limits are read from EffectiveConfig if available, with safe defaults.
pub async fn check_rate_limit(db: &Db, tenant_id: &str) -> Result<RateLimitResult> {
    let now = Utc::now();
    let window_size = 60; // 60 seconds (1 minute)

    // Read rate limit from config, with safe default
    let default_max = try_effective_config()
        .map(|cfg| cfg.rate_limits.requests_per_minute as i64)
        .unwrap_or(100); // Safe default: 100 rpm (not 1000)

    // Get or create bucket
    let bucket = sqlx::query_as::<_, RateLimitBucket>(
        "SELECT tenant_id, requests_count, window_start, window_size_seconds, max_requests, last_updated
         FROM rate_limit_buckets
         WHERE tenant_id = ?"
    )
    .bind(tenant_id)
    .fetch_optional(db.pool())
    .await?;

    if let Some(mut bucket) = bucket {
        let window_start = DateTime::parse_from_rfc3339(&bucket.window_start)
            .map_err(|e| AosError::Validation(format!("Invalid RFC3339 timestamp: {}", e)))?
            .with_timezone(&Utc);

        // Check if window has expired
        let window_duration = chrono::Duration::seconds(bucket.window_size_seconds);
        if now - window_start >= window_duration {
            // Reset window
            bucket.window_start = now.to_rfc3339();
            bucket.requests_count = 1;
            bucket.last_updated = now.to_rfc3339();

            sqlx::query(
                "UPDATE rate_limit_buckets
                 SET requests_count = ?, window_start = ?, last_updated = ?
                 WHERE tenant_id = ?",
            )
            .bind(bucket.requests_count)
            .bind(&bucket.window_start)
            .bind(&bucket.last_updated)
            .bind(tenant_id)
            .execute(db.pool())
            .await?;

            debug!(
                tenant_id = %tenant_id,
                count = %bucket.requests_count,
                limit = %bucket.max_requests,
                "Rate limit window reset"
            );

            return Ok(RateLimitResult {
                allowed: true,
                current_count: bucket.requests_count,
                limit: bucket.max_requests,
                reset_at: (window_start + window_duration).timestamp(),
            });
        }

        // Increment count
        bucket.requests_count += 1;
        bucket.last_updated = now.to_rfc3339();

        sqlx::query(
            "UPDATE rate_limit_buckets
             SET requests_count = ?, last_updated = ?
             WHERE tenant_id = ?",
        )
        .bind(bucket.requests_count)
        .bind(&bucket.last_updated)
        .bind(tenant_id)
        .execute(db.pool())
        .await?;

        let allowed = bucket.requests_count <= bucket.max_requests;

        if !allowed {
            warn!(
                tenant_id = %tenant_id,
                count = %bucket.requests_count,
                limit = %bucket.max_requests,
                "Rate limit exceeded"
            );
        }

        Ok(RateLimitResult {
            allowed,
            current_count: bucket.requests_count,
            limit: bucket.max_requests,
            reset_at: (window_start + window_duration).timestamp(),
        })
    } else {
        // Create new bucket
        let window_start = now.to_rfc3339();
        let last_updated = now.to_rfc3339();

        sqlx::query(
            "INSERT INTO rate_limit_buckets
             (tenant_id, requests_count, window_start, window_size_seconds, max_requests, last_updated)
             VALUES (?, 1, ?, ?, ?, ?)"
        )
        .bind(tenant_id)
        .bind(&window_start)
        .bind(window_size)
        .bind(default_max)
        .bind(&last_updated)
        .execute(db.pool())
        .await?;

        debug!(
            tenant_id = %tenant_id,
            max_requests = %default_max,
            "Created new rate limit bucket"
        );

        Ok(RateLimitResult {
            allowed: true,
            current_count: 1,
            limit: default_max,
            reset_at: (now + chrono::Duration::seconds(window_size)).timestamp(),
        })
    }
}

/// Update rate limit for a tenant (admin operation)
pub async fn update_rate_limit(db: &Db, tenant_id: &str, max_requests: i64) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO rate_limit_buckets
         (tenant_id, requests_count, window_start, window_size_seconds, max_requests, last_updated)
         VALUES (?, 0, ?, 60, ?, ?)
         ON CONFLICT(tenant_id) DO UPDATE SET
         max_requests = excluded.max_requests,
         last_updated = excluded.last_updated",
    )
    .bind(tenant_id)
    .bind(&now)
    .bind(max_requests)
    .bind(&now)
    .execute(db.pool())
    .await?;

    debug!(
        tenant_id = %tenant_id,
        max_requests = %max_requests,
        "Updated rate limit"
    );

    Ok(())
}

/// Get current rate limit status for a tenant
pub async fn get_rate_limit_status(db: &Db, tenant_id: &str) -> Result<Option<RateLimitResult>> {
    let bucket = sqlx::query_as::<_, RateLimitBucket>(
        "SELECT tenant_id, requests_count, window_start, window_size_seconds, max_requests, last_updated
         FROM rate_limit_buckets
         WHERE tenant_id = ?"
    )
    .bind(tenant_id)
    .fetch_optional(db.pool())
    .await?;

    if let Some(bucket) = bucket {
        let window_start = DateTime::parse_from_rfc3339(&bucket.window_start)
            .map_err(|e| AosError::Validation(format!("Invalid RFC3339 timestamp: {}", e)))?
            .with_timezone(&Utc);
        let window_duration = chrono::Duration::seconds(bucket.window_size_seconds);
        let reset_at = (window_start + window_duration).timestamp();

        Ok(Some(RateLimitResult {
            allowed: bucket.requests_count <= bucket.max_requests,
            current_count: bucket.requests_count,
            limit: bucket.max_requests,
            reset_at,
        }))
    } else {
        Ok(None)
    }
}

/// Reset rate limit for a tenant (admin emergency operation)
pub async fn reset_rate_limit(db: &Db, tenant_id: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE rate_limit_buckets
         SET requests_count = 0,
             window_start = ?,
             last_updated = ?
         WHERE tenant_id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(tenant_id)
    .execute(db.pool())
    .await?;

    debug!(tenant_id = %tenant_id, "Reset rate limit");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn init_test_schema(db: &Db) {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS rate_limit_buckets (
                tenant_id TEXT PRIMARY KEY,
                requests_count INTEGER NOT NULL DEFAULT 0,
                window_start TEXT NOT NULL DEFAULT (datetime('now')),
                window_size_seconds INTEGER NOT NULL DEFAULT 60,
                max_requests INTEGER NOT NULL DEFAULT 1000,
                last_updated TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(db.pool())
        .await
        .expect("Failed to create rate_limit_buckets table");
    }

    #[tokio::test]
    async fn test_rate_limit_basic() {
        let db = Db::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");
        init_test_schema(&db).await;

        // First request should succeed
        let result = check_rate_limit(&db, "tenant-a")
            .await
            .expect("Failed to check rate limit");
        assert!(result.allowed);
        assert_eq!(result.current_count, 1);
    }

    #[tokio::test]
    async fn test_rate_limit_exceeded() {
        let db = Db::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");
        init_test_schema(&db).await;

        // Set low limit
        update_rate_limit(&db, "tenant-b", 2)
            .await
            .expect("Failed to update rate limit");

        // First two requests succeed
        let r1 = check_rate_limit(&db, "tenant-b")
            .await
            .expect("Failed to check rate limit r1");
        assert!(r1.allowed);

        let r2 = check_rate_limit(&db, "tenant-b")
            .await
            .expect("Failed to check rate limit r2");
        assert!(r2.allowed);

        // Third request should be denied
        let r3 = check_rate_limit(&db, "tenant-b")
            .await
            .expect("Failed to check rate limit r3");
        assert!(!r3.allowed);
        assert_eq!(r3.current_count, 3);
        assert_eq!(r3.limit, 2);
    }

    #[tokio::test]
    async fn test_rate_limit_reset() {
        let db = Db::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");
        init_test_schema(&db).await;

        update_rate_limit(&db, "tenant-c", 5)
            .await
            .expect("Failed to update rate limit for tenant-c");

        // Make some requests
        check_rate_limit(&db, "tenant-c")
            .await
            .expect("Failed to check rate limit for tenant-c");
        check_rate_limit(&db, "tenant-c")
            .await
            .expect("Failed to check rate limit for tenant-c");

        // Reset
        reset_rate_limit(&db, "tenant-c")
            .await
            .expect("Failed to reset rate limit");

        // Count should be reset
        let status = get_rate_limit_status(&db, "tenant-c")
            .await
            .expect("Failed to get rate limit status")
            .expect("Rate limit status should exist");
        assert_eq!(status.current_count, 0);
    }
}
