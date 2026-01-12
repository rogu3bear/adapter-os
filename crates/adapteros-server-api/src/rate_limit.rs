//! Per-tenant token-bucket rate limiting with clock injection
//!
//! Implements per-tenant rate limiting with token bucket algorithm.
//! Each tenant gets their own isolated token bucket with configurable rate and burst capacity.
//!
//! Key design principles (PRD Fortification):
//! - **Injected clock**: Uses `Clock` trait for deterministic testing
//! - **No global state**: State is owned by `AppState`
//! - **Fail closed**: Rejects requests when misconfigured
//! - **TTL eviction**: Stale buckets are evicted via background task
//!
//! # Usage
//!
//! ```rust,ignore
//! // Production: uses SystemClock from AppState
//! let state = AppState::new(...);
//! let rate_limiter = state.rate_limiter.clone();
//! rate_limiter.check("tenant-123")?;
//!
//! // Testing: inject MockClock
//! let mock_clock = Arc::new(MockClock::frozen_at(1000));
//! let rate_limiter = RateLimiterState::new(config, mock_clock);
//! ```

use crate::state::AppState;
use adapteros_core::Clock;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Rate limiter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiterConfig {
    /// Requests allowed per minute per tenant.
    pub requests_per_minute: u32,
    /// Additional burst capacity above the per-minute rate.
    pub burst_size: u32,
    /// Time-to-live for inactive buckets in seconds.
    #[serde(default = "default_bucket_ttl_secs")]
    pub bucket_ttl_secs: u64,
    /// Maximum number of buckets to track (per-tenant limit).
    #[serde(default = "default_max_buckets")]
    pub max_buckets: usize,
    /// Fail closed on misconfiguration (default: true).
    #[serde(default = "default_fail_closed")]
    pub fail_closed: bool,
}

fn default_bucket_ttl_secs() -> u64 {
    3600 // 1 hour
}

fn default_max_buckets() -> usize {
    10000
}

fn default_fail_closed() -> bool {
    true
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 120, // 2 per second
            burst_size: 20,
            bucket_ttl_secs: default_bucket_ttl_secs(),
            max_buckets: default_max_buckets(),
            fail_closed: default_fail_closed(),
        }
    }
}

/// Error returned when rate limit is exceeded or limiter is unhealthy.
#[derive(Debug, Clone)]
pub enum RateLimitError {
    /// Rate limit exceeded for this tenant.
    Exceeded {
        tenant_id: String,
        retry_after_ms: u64,
    },
    /// Rate limiter is unhealthy (fail-closed mode).
    ServiceUnavailable { reason: String },
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::Exceeded {
                tenant_id,
                retry_after_ms,
            } => {
                write!(
                    f,
                    "Rate limit exceeded for tenant {}, retry after {}ms",
                    tenant_id, retry_after_ms
                )
            }
            RateLimitError::ServiceUnavailable { reason } => {
                write!(f, "Rate limiter unavailable: {}", reason)
            }
        }
    }
}

impl std::error::Error for RateLimitError {}

/// Token bucket for a single tenant.
struct TokenBucket {
    /// Maximum tokens (burst capacity).
    capacity_fixed: u64,
    /// Current token count (fixed-point: tokens * 1000).
    tokens_fixed: AtomicU64,
    /// Tokens per minute refill rate.
    rate_per_minute: u32,
    /// Last refill timestamp (milliseconds since epoch).
    last_refill_ms: AtomicU64,
    /// Last access timestamp (milliseconds since epoch).
    last_access_ms: AtomicU64,
}

impl TokenBucket {
    fn new(rate_per_minute: u32, burst_size: u32, now_ms: u64) -> Self {
        let capacity = rate_per_minute.saturating_add(burst_size);
        let capacity_fixed = (capacity as u64).saturating_mul(1000);
        Self {
            capacity_fixed,
            tokens_fixed: AtomicU64::new(capacity_fixed),
            rate_per_minute,
            last_refill_ms: AtomicU64::new(now_ms),
            last_access_ms: AtomicU64::new(now_ms),
        }
    }

    /// Try to consume one token using the given clock time.
    /// Returns Ok(()) if successful, Err(retry_after_ms) if rate limited.
    fn try_consume(&self, now_ms: u64) -> Result<(), u64> {
        // Update last access time
        self.last_access_ms.store(now_ms, Ordering::Release);

        let mut last_refill = self.last_refill_ms.load(Ordering::Acquire);

        // Refill tokens based on elapsed time
        if now_ms > last_refill {
            let elapsed_ms = now_ms - last_refill;
            let elapsed_minutes = elapsed_ms as f64 / 60_000.0;

            // Calculate tokens to add (fixed-point: tokens * 1000)
            let tokens_to_add = (self.rate_per_minute as f64 * elapsed_minutes * 1000.0) as u64;

            if tokens_to_add > 0 {
                // Update last_refill atomically
                loop {
                    match self.last_refill_ms.compare_exchange_weak(
                        last_refill,
                        now_ms,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => break,
                        Err(current) => last_refill = current,
                    }
                }

                // Refill tokens (cap at capacity)
                let current_tokens = self.tokens_fixed.load(Ordering::Acquire);
                let new_tokens = current_tokens
                    .saturating_add(tokens_to_add)
                    .min(self.capacity_fixed);
                self.tokens_fixed.store(new_tokens, Ordering::Release);
            }
        }

        // Try to consume one token (1000 in fixed-point)
        loop {
            let current = self.tokens_fixed.load(Ordering::Acquire);
            if current < 1000 {
                // Calculate retry-after time
                let tokens_needed = 1000 - current;
                let tokens_per_ms = self.rate_per_minute as f64 / 60_000.0 * 1000.0;
                let retry_after_ms = if tokens_per_ms > 0.0 {
                    (tokens_needed as f64 / tokens_per_ms).ceil() as u64
                } else {
                    60_000 // 1 minute default
                };
                return Err(retry_after_ms);
            }

            match self.tokens_fixed.compare_exchange_weak(
                current,
                current - 1000,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(_) => continue, // Retry on conflict
            }
        }
    }

    /// Get current token count (for metrics).
    fn available_tokens(&self) -> u32 {
        let tokens = self.tokens_fixed.load(Ordering::Acquire);
        (tokens / 1000) as u32
    }

    /// Check if this bucket is stale.
    fn is_stale(&self, now_ms: u64, max_age_ms: u64) -> bool {
        let last_access = self.last_access_ms.load(Ordering::Acquire);
        now_ms.saturating_sub(last_access) > max_age_ms
    }
}

/// Rate limiter state with injected clock.
///
/// This struct owns the per-tenant rate limiting state and uses an injected
/// clock for deterministic time handling.
pub struct RateLimiterState {
    buckets: DashMap<String, TokenBucket>,
    config: RateLimiterConfig,
    clock: Arc<dyn Clock>,
    /// Track if the rate limiter is healthy.
    healthy: std::sync::atomic::AtomicBool,
}

impl RateLimiterState {
    /// Create a new rate limiter with the given config and clock.
    pub fn new(config: RateLimiterConfig, clock: Arc<dyn Clock>) -> Self {
        Self {
            buckets: DashMap::with_capacity(1000),
            config,
            clock,
            healthy: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Create a rate limiter from AppState's clock.
    pub fn from_app_state(config: RateLimiterConfig, state: &AppState) -> Self {
        Self::new(config, state.clock.clone())
    }

    /// Returns the rate limiter configuration.
    pub fn config(&self) -> &RateLimiterConfig {
        &self.config
    }

    /// Check if a request from the given tenant is allowed.
    ///
    /// Returns Ok(()) if allowed, Err(RateLimitError) if denied.
    pub fn check(&self, tenant_id: &str) -> Result<(), RateLimitError> {
        // Fail closed if unhealthy
        if self.config.fail_closed && !self.healthy.load(Ordering::Acquire) {
            return Err(RateLimitError::ServiceUnavailable {
                reason: "Rate limiter is unhealthy".to_string(),
            });
        }

        // Check bucket count limit
        if self.buckets.len() >= self.config.max_buckets {
            if self.config.fail_closed {
                return Err(RateLimitError::ServiceUnavailable {
                    reason: format!("Too many rate limit buckets ({})", self.buckets.len()),
                });
            }
            // In non-fail-closed mode, allow but log warning
            tracing::warn!(
                tenant_id = tenant_id,
                bucket_count = self.buckets.len(),
                "Rate limiter bucket limit reached, allowing request"
            );
        }

        let now_ms = self.clock.now_millis();

        // Get or create bucket for tenant
        let bucket = self
            .buckets
            .entry(tenant_id.to_string())
            .or_insert_with(|| {
                TokenBucket::new(
                    self.config.requests_per_minute,
                    self.config.burst_size,
                    now_ms,
                )
            });

        match bucket.try_consume(now_ms) {
            Ok(()) => Ok(()),
            Err(retry_after_ms) => Err(RateLimitError::Exceeded {
                tenant_id: tenant_id.to_string(),
                retry_after_ms,
            }),
        }
    }

    /// Evict stale buckets that haven't been accessed recently.
    ///
    /// Returns the number of buckets evicted.
    pub fn evict_stale(&self) -> usize {
        let now_ms = self.clock.now_millis();
        let max_age_ms = self.config.bucket_ttl_secs * 1000;

        let mut evicted = 0;
        self.buckets.retain(|tenant_id, bucket| {
            let stale = bucket.is_stale(now_ms, max_age_ms);
            if stale {
                tracing::debug!(tenant_id = tenant_id, "Evicting stale rate limiter bucket");
                evicted += 1;
            }
            !stale
        });

        if evicted > 0 {
            tracing::info!(
                evicted = evicted,
                remaining = self.buckets.len(),
                "Rate limiter eviction complete"
            );
        }

        evicted
    }

    /// Get the number of active buckets.
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Get metrics snapshot for observability.
    pub fn metrics(&self) -> RateLimiterMetrics {
        RateLimiterMetrics {
            bucket_count: self.buckets.len(),
            max_buckets: self.config.max_buckets,
            requests_per_minute: self.config.requests_per_minute,
            bucket_ttl_secs: self.config.bucket_ttl_secs,
            healthy: self.healthy.load(Ordering::Acquire),
        }
    }

    /// Mark the rate limiter as unhealthy.
    pub fn set_unhealthy(&self) {
        self.healthy.store(false, Ordering::Release);
        tracing::warn!("Rate limiter marked as unhealthy");
    }

    /// Mark the rate limiter as healthy.
    pub fn set_healthy(&self) {
        self.healthy.store(true, Ordering::Release);
        tracing::info!("Rate limiter marked as healthy");
    }

    /// Check if the rate limiter is healthy.
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Acquire)
    }

    /// Get available tokens for a tenant (for observability).
    pub fn tokens_for_tenant(&self, tenant_id: &str) -> Option<u32> {
        self.buckets.get(tenant_id).map(|b| b.available_tokens())
    }
}

/// Metrics snapshot for rate limiter observability.
#[derive(Debug, Clone, Serialize)]
pub struct RateLimiterMetrics {
    pub bucket_count: usize,
    pub max_buckets: usize,
    pub requests_per_minute: u32,
    pub bucket_ttl_secs: u64,
    pub healthy: bool,
}

/// Background task for rate limiter eviction.
///
/// Call this from the background task spawner with a reasonable interval (e.g., 60 seconds).
pub async fn evict_stale_buckets(rate_limiter: Arc<RateLimiterState>) -> usize {
    rate_limiter.evict_stale()
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::MockClock;
    use std::time::Duration;

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let clock = Arc::new(MockClock::frozen_at(1000));
        let config = RateLimiterConfig {
            requests_per_minute: 60,
            burst_size: 10,
            ..Default::default()
        };
        let limiter = RateLimiterState::new(config, clock);

        // Should allow up to capacity (60 + 10 = 70) requests
        for i in 0..70 {
            assert!(
                limiter.check("tenant-1").is_ok(),
                "Request {} should succeed",
                i
            );
        }

        // 71st request should be rate limited
        assert!(limiter.check("tenant-1").is_err());
    }

    #[test]
    fn test_rate_limiter_refills_over_time() {
        let clock = Arc::new(MockClock::frozen_at(0));
        let config = RateLimiterConfig {
            requests_per_minute: 60, // 1 per second
            burst_size: 0,
            ..Default::default()
        };
        let limiter = RateLimiterState::new(config, clock.clone());

        // Consume all 60 tokens
        for _ in 0..60 {
            assert!(limiter.check("tenant-1").is_ok());
        }
        assert!(limiter.check("tenant-1").is_err());

        // Advance time by 1 minute - should refill
        clock.advance(Duration::from_secs(60));

        // Should allow 60 more requests
        for _ in 0..60 {
            assert!(limiter.check("tenant-1").is_ok());
        }
    }

    #[test]
    fn test_rate_limiter_per_tenant_isolation() {
        let clock = Arc::new(MockClock::frozen_at(0));
        let config = RateLimiterConfig {
            requests_per_minute: 10,
            burst_size: 0,
            ..Default::default()
        };
        let limiter = RateLimiterState::new(config, clock);

        // Exhaust tenant-1's quota
        for _ in 0..10 {
            assert!(limiter.check("tenant-1").is_ok());
        }
        assert!(limiter.check("tenant-1").is_err());

        // tenant-2 should still have full quota
        for _ in 0..10 {
            assert!(limiter.check("tenant-2").is_ok());
        }
    }

    #[test]
    fn test_eviction_removes_stale_buckets() {
        let clock = Arc::new(MockClock::frozen_at(0));
        let config = RateLimiterConfig {
            requests_per_minute: 60,
            burst_size: 0,
            bucket_ttl_secs: 60, // 1 minute TTL
            ..Default::default()
        };
        let limiter = RateLimiterState::new(config, clock.clone());

        // Create some buckets
        limiter.check("tenant-1").ok();
        limiter.check("tenant-2").ok();
        assert_eq!(limiter.bucket_count(), 2);

        // Advance time past TTL
        clock.advance(Duration::from_secs(120));

        // Evict stale buckets
        let evicted = limiter.evict_stale();
        assert_eq!(evicted, 2);
        assert_eq!(limiter.bucket_count(), 0);
    }

    #[test]
    fn test_fail_closed_when_unhealthy() {
        let clock = Arc::new(MockClock::frozen_at(0));
        let config = RateLimiterConfig {
            fail_closed: true,
            ..Default::default()
        };
        let limiter = RateLimiterState::new(config, clock);

        // Should work when healthy
        assert!(limiter.check("tenant-1").is_ok());

        // Mark unhealthy
        limiter.set_unhealthy();

        // Should fail closed
        match limiter.check("tenant-1") {
            Err(RateLimitError::ServiceUnavailable { .. }) => {}
            other => panic!("Expected ServiceUnavailable, got {:?}", other),
        }

        // Mark healthy again
        limiter.set_healthy();
        assert!(limiter.check("tenant-1").is_ok());
    }

    #[test]
    fn test_deterministic_with_mock_clock() {
        let clock1 = Arc::new(MockClock::frozen_at(1000));
        let clock2 = Arc::new(MockClock::frozen_at(1000));

        let config = RateLimiterConfig {
            requests_per_minute: 10,
            burst_size: 0,
            ..Default::default()
        };

        let limiter1 = RateLimiterState::new(config.clone(), clock1.clone());
        let limiter2 = RateLimiterState::new(config, clock2.clone());

        // Both should have identical behavior
        for _ in 0..10 {
            assert!(limiter1.check("tenant-1").is_ok());
            assert!(limiter2.check("tenant-1").is_ok());
        }

        // Both should be rate limited
        assert!(limiter1.check("tenant-1").is_err());
        assert!(limiter2.check("tenant-1").is_err());

        // Advance both clocks by same amount
        clock1.advance(Duration::from_secs(30));
        clock2.advance(Duration::from_secs(30));

        // Both should now have 5 tokens (30 seconds = 5 tokens at 10/min)
        for _ in 0..5 {
            assert!(limiter1.check("tenant-1").is_ok());
            assert!(limiter2.check("tenant-1").is_ok());
        }
        assert!(limiter1.check("tenant-1").is_err());
        assert!(limiter2.check("tenant-1").is_err());
    }
}
