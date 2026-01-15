//! Response compression middleware for adapterOS
//!
//! Implements transparent compression of API responses:
//! - Supports gzip and brotli compression
//! - Automatically selects best compression based on Accept-Encoding
//! - Skips compression for already-compressed content types
//! - Configurable compression levels
//!
//! Uses tower-http's CompressionLayer for implementation.
//!
//! [source: crates/adapteros-server-api/src/middleware/compression.rs]

use axum::{extract::Request, http::header, middleware::Next, response::Response};
use tower_http::compression::{CompressionLayer, CompressionLevel};

/// Content types that should not be compressed (already compressed)
const SKIP_COMPRESSION_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "video/",
    "audio/",
    "application/zip",
    "application/gzip",
    "application/x-gzip",
    "application/x-brotli",
];

/// Minimum size to compress (bytes)
const MIN_COMPRESS_SIZE: u16 = 1024; // 1KB

/// Compression configuration
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    /// Compression level (0-9, higher = better compression but slower)
    pub level: CompressionLevel,
    /// Minimum size to compress
    pub min_size: u16,
    /// Content types to skip
    pub skip_types: Vec<String>,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            level: CompressionLevel::Default,
            min_size: MIN_COMPRESS_SIZE,
            skip_types: SKIP_COMPRESSION_TYPES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

/// Create compression layer with default configuration
pub fn create_compression_layer() -> CompressionLayer {
    // Use default compression layer - tower-http handles content type filtering automatically
    // and we can configure size limits via the predicate if needed
    CompressionLayer::new().quality(CompressionLevel::Default)
}

/// Compression middleware (simple wrapper that adds compression info to logs)
///
/// Note: Actual compression is handled by tower-http's CompressionLayer.
/// This middleware just adds observability.
pub async fn compression_middleware(req: Request, next: Next) -> Response {
    // Extract accept-encoding before moving req
    let accept_encoding = req
        .headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_default();

    // Process request
    let response = next.run(req).await;

    // Log compression info if Content-Encoding was added
    if let Some(encoding) = response.headers().get(header::CONTENT_ENCODING) {
        tracing::debug!(
            accept_encoding = %accept_encoding,
            content_encoding = ?encoding,
            "Response compressed"
        );
    }

    response
}

/// Calculate compression savings
pub fn compression_savings(original_size: usize, compressed_size: usize) -> f64 {
    if original_size == 0 {
        return 0.0;
    }
    ((original_size - compressed_size) as f64 / original_size as f64) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_savings() {
        assert_eq!(compression_savings(1000, 500), 50.0);
        assert_eq!(compression_savings(1000, 100), 90.0);
        assert_eq!(compression_savings(1000, 1000), 0.0);
        assert_eq!(compression_savings(0, 0), 0.0);
    }

    #[test]
    fn test_default_config() {
        let config = CompressionConfig::default();
        assert_eq!(config.min_size, MIN_COMPRESS_SIZE);
        assert!(!config.skip_types.is_empty());
    }
}
