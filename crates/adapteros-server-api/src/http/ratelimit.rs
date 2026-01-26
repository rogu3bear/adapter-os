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
            requests_per_minute: 100,
            body_limit: 1024 * 1024, // 1MB
        }
    }
}

pub fn rate_limit_layer<S>(config: RateLimitConfig) -> impl Layer<S> + Clone + Send + 'static
where
    S: Send + Clone + 'static,
{
    let requests_per_second = ((config.requests_per_minute as f64 / 60.0).ceil() as usize).max(1);
    ServiceBuilder::new()
        .layer(RequestBodyLimitLayer::new(config.body_limit))
        .layer(TotalRequestLimitLayer::new(requests_per_second as u64))
}
