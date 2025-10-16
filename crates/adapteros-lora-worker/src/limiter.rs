//! Resource limiting and exhaustion protection
//!
//! Implements resource limits and rate limiting to prevent runaway processes.
//! Aligns with Performance Ruleset #11 and Memory Ruleset #12 from policy enforcement.

use adapteros_core::{AosError, Result};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::warn;

/// Resource limits configuration
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_concurrent_requests: usize,
    pub max_tokens_per_second: usize,
    pub max_memory_per_request: u64,
    pub max_cpu_time_per_request: Duration,
    pub max_requests_per_minute: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_concurrent_requests: 10,
            max_tokens_per_second: 40,
            max_memory_per_request: 50 * 1024 * 1024, // 50MB
            max_cpu_time_per_request: Duration::from_secs(30),
            max_requests_per_minute: 100,
        }
    }
}

/// Resource limiter with rate limiting and quotas
pub struct ResourceLimiter {
    limits: ResourceLimits,
    request_semaphore: Semaphore,
    token_rate_limiter: TokenRateLimiter,
    memory_tracker: MemoryTracker,
    cpu_tracker: CpuTracker,
    request_rate_limiter: RequestRateLimiter,
}

impl ResourceLimiter {
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            request_semaphore: Semaphore::new(limits.max_concurrent_requests),
            token_rate_limiter: TokenRateLimiter::new(limits.max_tokens_per_second),
            memory_tracker: MemoryTracker::new(limits.max_memory_per_request),
            cpu_tracker: CpuTracker::new(limits.max_cpu_time_per_request),
            request_rate_limiter: RequestRateLimiter::new(limits.max_requests_per_minute),
            limits,
        }
    }

    pub async fn acquire_request(&self) -> Result<ResourceGuard> {
        // Check request rate limit first
        self.request_rate_limiter.check_rate()?;

        let permit = self
            .request_semaphore
            .acquire()
            .await
            .map_err(|_| AosError::Worker("Resource limiter closed".to_string()))?;

        // Check if we can handle another request
        if self.memory_tracker.would_exceed_limit() {
            drop(permit);
            return Err(AosError::MemoryPressure(
                "Memory limit would be exceeded".to_string(),
            ));
        }

        Ok(ResourceGuard {
            _permit: permit,
            start_time: Instant::now(),
            memory_tracker: &self.memory_tracker,
            cpu_tracker: &self.cpu_tracker,
        })
    }

    pub fn check_token_rate(&self) -> Result<()> {
        self.token_rate_limiter.check_rate()
    }

    pub fn get_concurrent_requests(&self) -> usize {
        self.limits.max_concurrent_requests - self.request_semaphore.available_permits()
    }

    pub fn get_memory_usage(&self) -> u64 {
        self.memory_tracker.get_current_usage()
    }

    pub fn get_cpu_time(&self) -> Duration {
        self.cpu_tracker.get_total_time()
    }
}

/// Guard that automatically releases resources when dropped
pub struct ResourceGuard<'a> {
    _permit: tokio::sync::SemaphorePermit<'a>,
    start_time: Instant,
    memory_tracker: &'a MemoryTracker,
    cpu_tracker: &'a CpuTracker,
}

impl<'a> Drop for ResourceGuard<'a> {
    fn drop(&mut self) {
        let duration = self.start_time.elapsed();
        self.memory_tracker.release();
        self.cpu_tracker.record_usage(duration);
    }
}

/// Token rate limiter using sliding window
struct TokenRateLimiter {
    max_tokens_per_second: usize,
    tokens: AtomicUsize,
    last_reset: AtomicU64,
}

impl TokenRateLimiter {
    fn new(max_tokens_per_second: usize) -> Self {
        Self {
            max_tokens_per_second,
            tokens: AtomicUsize::new(max_tokens_per_second),
            last_reset: AtomicU64::new(Instant::now().elapsed().as_secs()),
        }
    }

    fn check_rate(&self) -> Result<()> {
        let now = Instant::now().elapsed().as_secs();
        let last_reset = self.last_reset.load(Ordering::Relaxed);

        // Reset tokens if a second has passed
        if now > last_reset
            && self
                .last_reset
                .compare_exchange(last_reset, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                self.tokens
                    .store(self.max_tokens_per_second, Ordering::Relaxed);
            }

        // Try to consume a token
        let current_tokens = self.tokens.load(Ordering::Relaxed);
        if current_tokens == 0 {
            return Err(AosError::Worker("Token rate limit exceeded".to_string()));
        }

        self.tokens.fetch_sub(1, Ordering::Relaxed);
        Ok(())
    }
}

/// Request rate limiter using sliding window
struct RequestRateLimiter {
    max_requests_per_minute: usize,
    requests: AtomicUsize,
    last_reset: AtomicU64,
}

impl RequestRateLimiter {
    fn new(max_requests_per_minute: usize) -> Self {
        Self {
            max_requests_per_minute,
            requests: AtomicUsize::new(max_requests_per_minute),
            last_reset: AtomicU64::new(Instant::now().elapsed().as_secs() / 60),
        }
    }

    fn check_rate(&self) -> Result<()> {
        let now_minutes = Instant::now().elapsed().as_secs() / 60;
        let last_reset = self.last_reset.load(Ordering::Relaxed);

        // Reset requests if a minute has passed
        if now_minutes > last_reset
            && self
                .last_reset
                .compare_exchange(
                    last_reset,
                    now_minutes,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                self.requests
                    .store(self.max_requests_per_minute, Ordering::Relaxed);
            }

        // Try to consume a request
        let current_requests = self.requests.load(Ordering::Relaxed);
        if current_requests == 0 {
            return Err(AosError::Worker("Request rate limit exceeded".to_string()));
        }

        self.requests.fetch_sub(1, Ordering::Relaxed);
        Ok(())
    }
}

/// Memory usage tracker
struct MemoryTracker {
    max_memory_per_request: u64,
    current_usage: AtomicU64,
    active_requests: AtomicUsize,
}

impl MemoryTracker {
    fn new(max_memory_per_request: u64) -> Self {
        Self {
            max_memory_per_request,
            current_usage: AtomicU64::new(0),
            active_requests: AtomicUsize::new(0),
        }
    }

    fn would_exceed_limit(&self) -> bool {
        let current = self.current_usage.load(Ordering::Relaxed);
        let active = self.active_requests.load(Ordering::Relaxed) as u64;
        let projected = current + (active * self.max_memory_per_request);

        projected > self.max_memory_per_request * 2 // Allow 2x for safety
    }

    fn release(&self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
        // In a real implementation, we'd track actual memory usage
        // For now, just decrement a counter
        self.current_usage.fetch_sub(1, Ordering::Relaxed);
    }

    fn get_current_usage(&self) -> u64 {
        self.current_usage.load(Ordering::Relaxed)
    }
}

/// CPU time tracker
struct CpuTracker {
    max_cpu_time_per_request: Duration,
    total_cpu_time: AtomicU64,
    request_count: AtomicUsize,
}

impl CpuTracker {
    fn new(max_cpu_time_per_request: Duration) -> Self {
        Self {
            max_cpu_time_per_request,
            total_cpu_time: AtomicU64::new(0),
            request_count: AtomicUsize::new(0),
        }
    }

    fn record_usage(&self, duration: Duration) {
        let duration_ms = duration.as_millis() as u64;
        self.total_cpu_time
            .fetch_add(duration_ms, Ordering::Relaxed);
        self.request_count.fetch_add(1, Ordering::Relaxed);

        if duration > self.max_cpu_time_per_request {
            warn!(
                "Request exceeded CPU time limit: {}ms > {}ms",
                duration_ms,
                self.max_cpu_time_per_request.as_millis()
            );
        }
    }

    fn get_total_time(&self) -> Duration {
        Duration::from_millis(self.total_cpu_time.load(Ordering::Relaxed))
    }

    fn _get_request_count(&self) -> usize {
        self.request_count.load(Ordering::Relaxed)
    }
}

/// Resource usage event for telemetry
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceUsageEvent {
    pub concurrent_requests: usize,
    pub memory_usage_bytes: u64,
    pub cpu_time_ms: u64,
    pub request_count: usize,
    pub timestamp: u64,
}

impl ResourceUsageEvent {
    pub fn from_limiter(limiter: &ResourceLimiter) -> Self {
        Self {
            concurrent_requests: limiter.get_concurrent_requests(),
            memory_usage_bytes: limiter.get_memory_usage(),
            cpu_time_ms: limiter.get_cpu_time().as_millis() as u64,
            request_count: 0, // Would track actual count
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::time::Duration; // unused

    #[tokio::test]
    async fn test_resource_limiter_creation() {
        let limits = ResourceLimits::default();
        let limiter = ResourceLimiter::new(limits);

        assert_eq!(limiter.get_concurrent_requests(), 0);
        assert_eq!(limiter.get_memory_usage(), 0);
    }

    #[tokio::test]
    async fn test_resource_guard() {
        let limits = ResourceLimits::default();
        let limiter = ResourceLimiter::new(limits);

        let guard = limiter
            .acquire_request()
            .await
            .expect("Test limiter acquire should succeed");
        assert_eq!(limiter.get_concurrent_requests(), 1);

        drop(guard);
        assert_eq!(limiter.get_concurrent_requests(), 0);
    }

    #[tokio::test]
    async fn test_token_rate_limiter() {
        let limiter = TokenRateLimiter::new(2);

        // First two tokens should succeed
        assert!(limiter.check_rate().is_ok());
        assert!(limiter.check_rate().is_ok());

        // Third token should fail
        assert!(limiter.check_rate().is_err());
    }

    #[tokio::test]
    async fn test_request_rate_limiter() {
        let limiter = RequestRateLimiter::new(2);

        // First two requests should succeed
        assert!(limiter.check_rate().is_ok());
        assert!(limiter.check_rate().is_ok());

        // Third request should fail
        assert!(limiter.check_rate().is_err());
    }
}
