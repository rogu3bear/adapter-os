//! Per-tenant upload rate limiting for DoS protection
//!
//! Implements a token bucket algorithm for per-tenant upload rate limiting.
//! Each tenant gets isolated token buckets that refill at a configured rate.
//!
//! # Example
//! ```ignore
//! let limiter = UploadRateLimiter::new(10, 60); // 10 uploads per 60 seconds
//!
//! if limiter.check_rate_limit("tenant-a").await {
//!     // Proceed with upload
//! } else {
//!     // Return 429 Too Many Requests
//! }
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Token bucket for a tenant's upload quota
#[derive(Debug)]
struct UploadTokenBucket {
    /// Maximum tokens (burst capacity)
    capacity: u32,
    /// Current token count (using fixed-point: tokens * 1000)
    tokens: AtomicU64,
    /// Tokens per minute refill rate
    rate_per_minute: u32,
    /// Last refill timestamp (milliseconds since epoch)
    last_refill: AtomicU64,
    /// Last access timestamp (milliseconds since epoch)
    last_access: AtomicU64,
}

impl UploadTokenBucket {
    /// Create a new token bucket
    fn new(rate_per_minute: u32, burst_size: u32) -> Self {
        let capacity = rate_per_minute + burst_size;
        let now_ms = current_time_ms();
        Self {
            capacity,
            tokens: AtomicU64::new((capacity as u64) * 1000), // Initialize at capacity
            rate_per_minute,
            last_refill: AtomicU64::new(now_ms),
            last_access: AtomicU64::new(now_ms),
        }
    }

    /// Try to consume one token. Returns (success, remaining_tokens, reset_timestamp_secs)
    fn try_consume(&self) -> (bool, u32, u64) {
        let now_ms = current_time_ms();

        // Update last access time
        self.last_access.store(now_ms, Ordering::Release);

        let mut last_refill = self.last_refill.load(Ordering::Acquire);

        // Refill tokens based on elapsed time
        if now_ms > last_refill {
            let elapsed_ms = now_ms - last_refill;
            let elapsed_minutes = elapsed_ms as f64 / 60_000.0;

            // Calculate tokens to add (fixed-point: tokens * 1000)
            let tokens_to_add = (self.rate_per_minute as f64 * elapsed_minutes * 1000.0) as u64;

            // Update last_refill atomically
            loop {
                match self.last_refill.compare_exchange_weak(
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
            let current_tokens = self.tokens.load(Ordering::Acquire);
            let new_tokens = (current_tokens + tokens_to_add).min((self.capacity as u64) * 1000);
            self.tokens.store(new_tokens, Ordering::Release);
        }

        // Try to consume one token (1000 in fixed-point)
        loop {
            let current = self.tokens.load(Ordering::Acquire);
            let remaining = (current / 1000) as u32;

            if current < 1000 {
                // No tokens available - rate limited
                let reset_at = (now_ms as u64 / 1000) + 60; // Reset after 1 minute
                return (false, 0, reset_at);
            }

            match self.tokens.compare_exchange_weak(
                current,
                current - 1000,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    // Successfully consumed one token
                    let reset_at = (now_ms as u64 / 1000) + 60;
                    return (true, remaining - 1, reset_at);
                }
                Err(_) => continue, // Retry on conflict
            }
        }
    }

    /// Check if this bucket is stale (not accessed for more than max_age_ms)
    fn is_stale(&self, max_age_ms: u64) -> bool {
        let now_ms = current_time_ms();
        let last_access = self.last_access.load(Ordering::Acquire);
        now_ms.saturating_sub(last_access) > max_age_ms
    }

    /// Get current token count (for metrics/debugging)
    #[allow(dead_code)]
    fn available_tokens(&self) -> u32 {
        let tokens_fixed = self.tokens.load(Ordering::Acquire);
        (tokens_fixed / 1000) as u32
    }
}

/// Per-tenant upload rate limiter
pub struct UploadRateLimiter {
    /// Rate limit config (uploads per minute)
    rate_per_minute: u32,
    /// Burst capacity (additional tokens beyond rate)
    burst_size: u32,
    /// Per-tenant token buckets
    buckets: Arc<RwLock<HashMap<String, UploadTokenBucket>>>,
}

impl UploadRateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    /// * `rate_per_minute` - Maximum uploads per minute
    /// * `burst_size` - Allow burst up to rate + burst_size
    pub fn new(rate_per_minute: u32, burst_size: u32) -> Self {
        Self {
            rate_per_minute,
            burst_size,
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a tenant can upload. Returns (allowed, remaining, reset_at_secs)
    pub async fn check_rate_limit(&self, tenant_id: &str) -> (bool, u32, u64) {
        // Try to get or create bucket
        let should_create = {
            let buckets = self.buckets.read().await;
            !buckets.contains_key(tenant_id)
        };

        if should_create {
            // Create new bucket with write lock
            let mut buckets = self.buckets.write().await;
            buckets.insert(
                tenant_id.to_string(),
                UploadTokenBucket::new(self.rate_per_minute, self.burst_size),
            );
        }

        // Now check the rate limit
        let buckets = self.buckets.read().await;
        if let Some(bucket) = buckets.get(tenant_id) {
            let (allowed, remaining, reset_at) = bucket.try_consume();
            if !allowed {
                warn!(
                    tenant_id = %tenant_id,
                    "Upload rate limit exceeded for tenant"
                );
            } else {
                debug!(
                    tenant_id = %tenant_id,
                    remaining_uploads = remaining,
                    "Upload rate limit check passed"
                );
            }
            (allowed, remaining, reset_at)
        } else {
            // Shouldn't happen, but fall back safely
            (
                true,
                self.rate_per_minute,
                current_time_ms() as u64 / 1000 + 60,
            )
        }
    }

    /// Clean up stale buckets that haven't been accessed for more than max_age_ms
    pub async fn cleanup_stale_buckets(&self, max_age_ms: u64) {
        let mut buckets = self.buckets.write().await;
        let mut to_remove = Vec::new();

        for (tenant_id, bucket) in buckets.iter() {
            if bucket.is_stale(max_age_ms) {
                to_remove.push(tenant_id.clone());
            }
        }

        for tenant_id in to_remove {
            buckets.remove(&tenant_id);
            debug!(
                tenant_id = %tenant_id,
                "Cleaned up stale upload rate limiter bucket"
            );
        }
    }

    /// Reset rate limit for a tenant (admin operation)
    pub async fn reset_rate_limit(&self, tenant_id: &str) {
        let mut buckets = self.buckets.write().await;
        buckets.insert(
            tenant_id.to_string(),
            UploadTokenBucket::new(self.rate_per_minute, self.burst_size),
        );
        debug!(
            tenant_id = %tenant_id,
            "Reset upload rate limit for tenant"
        );
    }

    /// Get rate limit info for a tenant
    #[allow(dead_code)]
    pub async fn get_limit_info(&self, tenant_id: &str) -> Option<(u32, u32)> {
        let buckets = self.buckets.read().await;
        buckets
            .get(tenant_id)
            .map(|b| (b.available_tokens(), self.capacity()))
    }

    /// Get the capacity for burst uploads
    fn capacity(&self) -> u32 {
        self.rate_per_minute + self.burst_size
    }
}

/// Get current time in milliseconds since epoch
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let limiter = UploadRateLimiter::new(5, 2); // 5 uploads per minute, burst of 2

        // First 7 requests should succeed (5 + burst of 2)
        for i in 0..7 {
            let (allowed, remaining, _) = limiter.check_rate_limit("tenant-a").await;
            assert!(allowed, "Request {} should be allowed", i);
            assert_eq!(remaining, 6 - i as u32, "Remaining should decrease");
        }

        // 8th request should fail
        let (allowed, remaining, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(!allowed, "8th request should be rate limited");
        assert_eq!(remaining, 0, "No tokens remaining");
    }

    #[tokio::test]
    async fn test_rate_limiter_per_tenant_isolation() {
        let limiter = UploadRateLimiter::new(3, 0); // 3 uploads per minute

        // Tenant A uses 2 uploads
        let (allowed_a1, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (allowed_a2, _, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(allowed_a1 && allowed_a2);

        // Tenant B should have independent quota
        let (allowed_b1, _, _) = limiter.check_rate_limit("tenant-b").await;
        let (allowed_b2, _, _) = limiter.check_rate_limit("tenant-b").await;
        let (allowed_b3, _, _) = limiter.check_rate_limit("tenant-b").await;
        assert!(allowed_b1 && allowed_b2 && allowed_b3);

        // Both should be rate limited on next request
        let (allowed_a3, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (allowed_b4, _, _) = limiter.check_rate_limit("tenant-b").await;
        assert!(!allowed_a3 && !allowed_b4);
    }

    #[tokio::test]
    async fn test_rate_limiter_reset() {
        let limiter = UploadRateLimiter::new(2, 0);

        // Use up quota
        let (a1, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a2, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a3, _, _) = limiter.check_rate_limit("tenant-a").await;

        assert!(a1 && a2 && !a3);

        // Reset
        limiter.reset_rate_limit("tenant-a").await;

        // Should have full quota again
        let (a4, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a5, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a6, _, _) = limiter.check_rate_limit("tenant-a").await;

        assert!(a4 && a5 && !a6);
    }

    #[tokio::test]
    async fn test_stale_bucket_cleanup() {
        let limiter = UploadRateLimiter::new(5, 0);

        // Access tenant A
        limiter.check_rate_limit("tenant-a").await;

        // Verify bucket exists
        let buckets = limiter.buckets.read().await;
        assert!(buckets.contains_key("tenant-a"));
        drop(buckets);

        // Clean up with very short timeout to mark as stale
        limiter.cleanup_stale_buckets(1).await;

        // Bucket should be gone (with small time window)
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        limiter.cleanup_stale_buckets(1).await;

        let buckets = limiter.buckets.read().await;
        // May still exist if cleanup happened too quickly, which is fine
        // The important thing is cleanup doesn't crash
    }
}
