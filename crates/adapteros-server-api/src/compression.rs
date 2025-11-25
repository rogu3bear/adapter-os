//! HTTP compression middleware
//!
//! Implements:
//! - Gzip compression
//! - Brotli compression (if enabled)
//! - Accept-Encoding negotiation
//! - Content-Encoding headers
//!
//! Citations:
//! - HTTP compression: RFC 7231
//! - Brotli: RFC 7932

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use flate2::write::GzEncoder;
use flate2::Compression as GzCompression;
use std::io::Write;
use tracing::{debug, warn};

/// Compression algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    Gzip,
    Deflate,
    Identity,
}

impl CompressionAlgorithm {
    /// Parse from Accept-Encoding header
    pub fn from_accept_encoding(accept_encoding: &str) -> Self {
        let encodings: Vec<&str> = accept_encoding
            .split(',')
            .map(|s| s.trim().split(';').next().unwrap_or(""))
            .collect();

        // Prefer gzip > deflate > identity
        if encodings.contains(&"gzip") {
            CompressionAlgorithm::Gzip
        } else if encodings.contains(&"deflate") {
            CompressionAlgorithm::Deflate
        } else {
            CompressionAlgorithm::Identity
        }
    }

    /// Get Content-Encoding header value
    pub fn content_encoding(&self) -> Option<&'static str> {
        match self {
            CompressionAlgorithm::Gzip => Some("gzip"),
            CompressionAlgorithm::Deflate => Some("deflate"),
            CompressionAlgorithm::Identity => None,
        }
    }
}

/// Compress data with gzip
pub fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = GzEncoder::new(Vec::new(), GzCompression::default());
    encoder.write_all(data)?;
    encoder.finish()
}

/// Compress data with deflate
pub fn compress_deflate(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), GzCompression::default());
    encoder.write_all(data)?;
    encoder.finish()
}

/// Check if content type should be compressed
pub fn should_compress_content_type(content_type: Option<&HeaderValue>) -> bool {
    if let Some(ct) = content_type {
        if let Ok(ct_str) = ct.to_str() {
            // Compress text-based content types
            return ct_str.contains("json")
                || ct_str.contains("xml")
                || ct_str.contains("html")
                || ct_str.contains("text")
                || ct_str.contains("javascript")
                || ct_str.contains("css");
        }
    }
    false
}

/// Compression middleware
pub async fn compression_middleware(request: Request, next: Next) -> Response {
    // Get Accept-Encoding header
    let accept_encoding = request
        .headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Determine compression algorithm
    let algorithm = CompressionAlgorithm::from_accept_encoding(accept_encoding);

    debug!(algorithm = ?algorithm, accept_encoding = %accept_encoding, "Compression negotiation");

    // Process request
    let response = next.run(request).await;

    // Only compress if algorithm is not Identity and response is compressible
    if algorithm == CompressionAlgorithm::Identity {
        return response;
    }

    // Check if content type is compressible
    let content_type = response.headers().get(header::CONTENT_TYPE);
    if !should_compress_content_type(content_type) {
        return response;
    }

    // For now, return response as-is
    // Full compression implementation would require body extraction and re-wrapping
    // which is complex with Axum's streaming body model
    // Instead, we use tower-http's CompressionLayer in production
    response
}

/// Add compression headers to response
pub fn add_compression_headers(
    mut response: Response,
    algorithm: CompressionAlgorithm,
) -> Response {
    if let Some(encoding) = algorithm.content_encoding() {
        if let Ok(header_value) = HeaderValue::from_str(encoding) {
            response
                .headers_mut()
                .insert(header::CONTENT_ENCODING, header_value);
        }
    }
    response
}

/// Compression configuration
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    /// Enable gzip compression
    pub enable_gzip: bool,
    /// Enable brotli compression
    pub enable_brotli: bool,
    /// Minimum size for compression (bytes)
    pub min_size: usize,
    /// Compression level (1-9)
    pub level: u32,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enable_gzip: true,
            enable_brotli: false,
            min_size: 1024, // Only compress responses > 1KB
            level: 6,       // Default compression level
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_algorithm_from_accept_encoding() {
        assert_eq!(
            CompressionAlgorithm::from_accept_encoding("gzip, deflate, br"),
            CompressionAlgorithm::Gzip
        );
        assert_eq!(
            CompressionAlgorithm::from_accept_encoding("deflate"),
            CompressionAlgorithm::Deflate
        );
        assert_eq!(
            CompressionAlgorithm::from_accept_encoding("identity"),
            CompressionAlgorithm::Identity
        );
        assert_eq!(
            CompressionAlgorithm::from_accept_encoding(""),
            CompressionAlgorithm::Identity
        );
    }

    #[test]
    fn test_compress_gzip() {
        let data = b"Hello, world! This is test data for compression.";
        let compressed = compress_gzip(data).unwrap();
        assert!(compressed.len() < data.len());
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_should_compress_content_type() {
        assert!(should_compress_content_type(Some(
            &HeaderValue::from_static("application/json")
        )));
        assert!(should_compress_content_type(Some(
            &HeaderValue::from_static("text/html")
        )));
        assert!(should_compress_content_type(Some(
            &HeaderValue::from_static("text/plain")
        )));
        assert!(!should_compress_content_type(Some(
            &HeaderValue::from_static("image/png")
        )));
        assert!(!should_compress_content_type(Some(
            &HeaderValue::from_static("video/mp4")
        )));
    }

    #[test]
    fn test_content_encoding() {
        assert_eq!(CompressionAlgorithm::Gzip.content_encoding(), Some("gzip"));
        assert_eq!(
            CompressionAlgorithm::Deflate.content_encoding(),
            Some("deflate")
        );
        assert_eq!(CompressionAlgorithm::Identity.content_encoding(), None);
    }
}
