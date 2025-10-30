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
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::warn;

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
        }
    }

    /// Try to consume one token. Returns true if successful, false if rate limited.
    fn try_consume(&self) -> bool {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let mut last_refill = self.last_refill.load(Ordering::Acquire);

        // Refill tokens based on elapsed time
        if now_ms > last_refill {
            let elapsed_ms = now_ms - last_refill;
            let elapsed_minutes = elapsed_ms as f64 / 60_000.0;

            // Calculate tokens to add (fixed-point: tokens * 1000)
            let tokens_to_add =
                (self.rate_per_minute as f64 * elapsed_minutes * 1000.0) as u64;

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
            let new_tokens = (current_tokens + tokens_to_add)
                .min((self.capacity as u64) * 1000);
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
    fn available_tokens(&self) -> u32 {
        let tokens_fixed = self.tokens.load(Ordering::Acquire);
        (tokens_fixed / 1000) as u32
    }
}

/// Per-tenant rate limiters
type TenantRateLimiters = Arc<Mutex<HashMap<String, TokenBucket>>>;

/// Per-tenant rate limiting middleware
pub async fn per_tenant_rate_limit_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Get rate limit config
    let rate_limits = {
        let config = state.config.read().map_err(|_| {
            (
                HttpStatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("config lock poisoned").with_code("INTERNAL_ERROR")),
            )
        })?;
        config.rate_limits.clone()
    };

    // If no rate limits configured, pass through
    let Some(rate_limits) = rate_limits else {
        return Ok(next.run(req).await);
    };

    // Get tenant ID from JWT claims (if authenticated)
    let tenant_id = req
        .extensions()
        .get::<Claims>()
        .map(|claims| claims.tenant_id.clone())
        .unwrap_or_else(|| "default".to_string());

    // Get or create rate limiter for this tenant
    // Use a static store for per-tenant token buckets
    static LIMITERS: tokio::sync::OnceCell<TenantRateLimiters> = tokio::sync::OnceCell::const_new();

    let limiters = LIMITERS
        .get_or_init(|| async { Arc::new(Mutex::new(HashMap::new())) })
        .await;

    let consumed = {
        let mut limiters_guard = limiters.lock().await;
        let bucket = limiters_guard
            .entry(tenant_id.clone())
            .or_insert_with(|| {
                TokenBucket::new(
                    rate_limits.requests_per_minute,
                    rate_limits.burst_size,
                )
            });
        bucket.try_consume()
    };

    if !consumed {
        warn!(
            tenant_id = %tenant_id,
            "Rate limit exceeded for tenant"
        );
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(
                ErrorResponse::new("rate limit exceeded")
                    .with_code("RATE_LIMIT_EXCEEDED")
                    .with_string_details(format!(
                        "Tenant '{}' has exceeded rate limit of {} requests/minute",
                        tenant_id, rate_limits.requests_per_minute
                    )),
            ),
        ));
    }

    Ok(next.run(req).await)
}
