use tower::Layer;
use tower::ServiceBuilder;
use tower_http::limit::{RequestBodyLimitLayer, TotalRequestLimitLayer};

// Simple RateLimitConfig
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    pub requests_per_minute: usize,
    pub body_limit: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            // Aligned with configs/cp.toml [rate_limits].requests_per_minute
            requests_per_minute: 300,
            body_limit: 1024 * 1024, // 1MB
        }
    }
}

/// Minimum concurrent request slots regardless of computed value.
/// Prevents the concurrency window from being unreasonably small at low rpm
/// configurations, which would serialize nearly all traffic.
const MIN_CONCURRENT_REQUESTS: usize = 20;

pub fn rate_limit_layer<S>(config: RateLimitConfig) -> impl Layer<S> + Clone + Send + 'static
where
    S: Send + Clone + 'static,
{
    // Previous formula: rpm / 60 → only ~2 concurrent slots at 100 rpm, far too
    // restrictive for bursty HTTP traffic.  Divide by 10 instead (≈ 6-second
    // window) and enforce a floor of MIN_CONCURRENT_REQUESTS so that even low-rpm
    // configs can serve a realistic number of in-flight requests.
    let concurrent = ((config.requests_per_minute as f64 / 10.0).ceil() as usize)
        .max(MIN_CONCURRENT_REQUESTS);
    ServiceBuilder::new()
        .layer(RequestBodyLimitLayer::new(config.body_limit))
        .layer(TotalRequestLimitLayer::new(concurrent as u64))
}
