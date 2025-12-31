//! Idempotency types for request deduplication.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Header name for idempotency key
pub const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";

/// How long to cache idempotent responses (24 hours)
pub const IDEMPOTENCY_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Maximum body size to cache (1MB)
pub const MAX_CACHED_BODY_SIZE: usize = 1024 * 1024;

/// Newtype wrapper for idempotency keys
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(pub String);

impl IdempotencyKey {
    /// Create a new idempotency key
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Get the key as a string reference
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Cached response data for idempotent requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResponse {
    /// HTTP status code
    pub status_code: u16,
    /// Response headers (key-value pairs)
    pub headers: Vec<(String, String)>,
    /// Response body bytes
    pub body: Vec<u8>,
    /// Unix timestamp when this response was created
    pub created_at: i64,
}

impl CachedResponse {
    /// Check if this cached response has expired
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        let ttl_secs = IDEMPOTENCY_TTL.as_secs() as i64;
        now - self.created_at >= ttl_secs
    }
}

/// Status of an idempotency key in the store
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyStatus {
    /// Key not found - this is a new request
    New,
    /// Request is currently being processed
    InProgress,
    /// Request completed and response is cached
    Completed,
}
