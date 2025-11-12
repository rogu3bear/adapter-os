//! Per-tenant token-bucket rate limiting middleware
//!
//! Implements per-tenant rate limiting with token bucket algorithm for M1 production hardening.
//! Each tenant gets their own isolated token bucket with configurable rate and burst capacity.

use crate::auth::Claims;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Request, State},
    http::{StatusCode, StatusCode as HttpStatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::warn;
use tower_governor::{Governor, GovernorConfig, key_extractor::KeyExtractor};
use axum::http::Request;
use std::sync::Arc;
use crate::state::AppState;
use crate::auth::Claims;
use tower::Layer;
use tower::ServiceBuilder;
use futures::future::BoxFuture;
use tower::Service;

#[derive(Clone)]
struct TenantKeyExtractor {
    state: Arc<AppState>,
}

impl KeyExtractor for TenantKeyExtractor {
    type Key = String;
    type Fut = BoxFuture<'static, Self::Key>;
    fn extract(&self, req: &Request<Body>) -> Self::Fut {
        Box::pin(async move {
            if let Some(claims) = req.extensions().get::<Claims>() {
                claims.tenant_id.clone()
            } else {
                "anonymous".to_string()
            }
        })
    }
}

/// Token bucket implementation for rate limiting
struct TokenBucket {
    /// Maximum tokens (burst capacity)
    capacity: u32,
    /// Current token count
    tokens: AtomicU64, // Using u64 with fixed-point arithmetic (tokens * 1000)
    /// Tokens per minute refill rate
    rate_per_minute: u32,
    /// Last refill timestamp (milliseconds since epoch)
    last_refill: AtomicU64,
    /// Last access timestamp (milliseconds since epoch)
    last_access: AtomicU64,
}

impl TokenBucket {
    fn new(rate_per_minute: u32, burst_size: u32) -> Self {
        let capacity = rate_per_minute + burst_size;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            capacity,
            tokens: AtomicU64::new((capacity as u64) * 1000), // Initialize at capacity
            rate_per_minute,
            last_refill: AtomicU64::new(now_ms),
            last_access: AtomicU64::new(now_ms),
        }
    }

    /// Try to consume one token. Returns true if successful, false if rate limited.
    fn try_consume(&self) -> bool {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

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
            if current < 1000 {
                return false; // No tokens available
            }

            match self.tokens.compare_exchange_weak(
                current,
                current - 1000,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(_) => continue, // Retry on conflict
            }
        }
    }

    /// Get current token count (for metrics)
    #[allow(dead_code)]
    fn available_tokens(&self) -> u32 {
        let tokens_fixed = self.tokens.load(Ordering::Acquire);
        (tokens_fixed / 1000) as u32
    }

    /// Check if this bucket is stale (not accessed for more than the given duration)
    fn is_stale(&self, max_age_ms: u64) -> bool {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let last_access = self.last_access.load(Ordering::Acquire);
        now_ms.saturating_sub(last_access) > max_age_ms
    }
}

/// Per-tenant rate limiters
type TenantRateLimiters = Arc<Mutex<HashMap<String, TokenBucket>>>;

pub fn per_tenant_rate_limit_middleware(state: Arc<AppState>) -> impl Layer< S > + Clone + Send + 'static where S: Service<Request<Body>, Response = Response> + Send + Clone + 'static, S::Future: Send + 'static {
    let config = GovernorConfig::default()
        .per_second(100)
        .burst_size(100)
        .key_prefix("rate_limit");
    let key_extractor = TenantKeyExtractor { state: state.clone() };
    ServiceBuilder::new().layer(Governor::new(&config, key_extractor))
}

/// Clean up stale rate limiter buckets that haven't been accessed for more than max_age_ms
pub async fn cleanup_stale_rate_limiters(max_age_ms: u64) {
    static LIMITERS: tokio::sync::OnceCell<TenantRateLimiters> = tokio::sync::OnceCell::const_new();

    let limiters = match LIMITERS.get() {
        Some(l) => l,
        None => return, // No limiters initialized yet
    };

    let mut limiters_guard = limiters.lock().await;
    let mut to_remove = Vec::new();

    for (tenant_id, bucket) in limiters_guard.iter() {
        if bucket.is_stale(max_age_ms) {
            to_remove.push(tenant_id.clone());
        }
    }

    for tenant_id in to_remove {
        limiters_guard.remove(&tenant_id);
        tracing::debug!("Cleaned up stale rate limiter for tenant: {}", tenant_id);
    }
}
