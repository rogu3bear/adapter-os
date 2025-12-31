//! Streaming response handling for large HTTP responses

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::error::{WebBrowseError, WebBrowseResult};

/// Configuration for streaming response handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    /// Threshold in bytes to switch to streaming mode
    pub streaming_threshold_bytes: u64,

    /// Chunk size for streaming reads
    pub chunk_size_bytes: usize,

    /// Maximum content size before truncation
    pub max_content_bytes: u64,

    /// Whether to allow truncation of large responses
    pub allow_truncation: bool,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            streaming_threshold_bytes: 50 * 1024, // 50KB
            chunk_size_bytes: 8 * 1024,           // 8KB chunks
            max_content_bytes: 500 * 1024,        // 500KB max
            allow_truncation: true,
        }
    }
}

/// Result of streaming a response
#[derive(Debug, Clone)]
pub struct StreamedContent {
    /// The content (possibly truncated)
    pub content: String,
    /// Original size in bytes
    pub original_size_bytes: u64,
    /// Whether content was truncated
    pub was_truncated: bool,
    /// Content-Type header value
    pub content_type: Option<String>,
}

/// Stream response body with optional truncation for large responses
///
/// This function handles responses in a memory-efficient way by:
/// 1. Reading content in chunks to avoid large allocations
/// 2. Truncating content if it exceeds the configured limit
/// 3. Gracefully handling encoding issues with lossy UTF-8 conversion
pub async fn stream_response_body(
    response: reqwest::Response,
    config: &StreamingConfig,
) -> WebBrowseResult<StreamedContent> {
    let content_length = response.content_length();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    // Determine if we should stream based on content length
    let should_stream = content_length
        .map(|len| len > config.streaming_threshold_bytes)
        .unwrap_or(true); // Stream if unknown size

    if should_stream {
        stream_chunked(response, config, content_length, content_type).await
    } else {
        // Small response: read directly
        read_full_response(response, content_type).await
    }
}

/// Stream response in chunks with truncation support
async fn stream_chunked(
    response: reqwest::Response,
    config: &StreamingConfig,
    content_length: Option<u64>,
    content_type: Option<String>,
) -> WebBrowseResult<StreamedContent> {
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::with_capacity(config.chunk_size_bytes * 8);
    let mut total_bytes = 0u64;
    let mut was_truncated = false;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e: reqwest::Error| {
            WebBrowseError::NetworkError(format!("Failed to read chunk: {}", e))
        })?;

        total_bytes += chunk.len() as u64;

        if buffer.len() + chunk.len() <= config.max_content_bytes as usize {
            buffer.extend_from_slice(&chunk);
        } else if config.allow_truncation {
            // Truncate: take only what fits
            let remaining = (config.max_content_bytes as usize).saturating_sub(buffer.len());
            if remaining > 0 {
                buffer.extend_from_slice(&chunk[..remaining.min(chunk.len())]);
            }
            was_truncated = true;
            // Continue reading to get accurate total_bytes, but don't store
        } else {
            return Err(WebBrowseError::ContentTooLarge {
                size_kb: total_bytes / 1024,
                limit_kb: config.max_content_bytes / 1024,
            });
        }
    }

    let content = String::from_utf8_lossy(&buffer).to_string();

    Ok(StreamedContent {
        content,
        original_size_bytes: content_length.unwrap_or(total_bytes),
        was_truncated,
        content_type,
    })
}

/// Read full response for small content
async fn read_full_response(
    response: reqwest::Response,
    content_type: Option<String>,
) -> WebBrowseResult<StreamedContent> {
    let bytes = response
        .bytes()
        .await
        .map_err(|e| WebBrowseError::ParseError(format!("Failed to read response: {}", e)))?;

    Ok(StreamedContent {
        content: String::from_utf8_lossy(&bytes).to_string(),
        original_size_bytes: bytes.len() as u64,
        was_truncated: false,
        content_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = StreamingConfig::default();
        assert_eq!(config.streaming_threshold_bytes, 50 * 1024);
        assert_eq!(config.chunk_size_bytes, 8 * 1024);
        assert_eq!(config.max_content_bytes, 500 * 1024);
        assert!(config.allow_truncation);
    }

    #[test]
    fn test_streaming_config_custom() {
        let config = StreamingConfig {
            streaming_threshold_bytes: 100 * 1024,
            chunk_size_bytes: 16 * 1024,
            max_content_bytes: 1024 * 1024,
            allow_truncation: false,
        };

        assert_eq!(config.streaming_threshold_bytes, 100 * 1024);
        assert!(!config.allow_truncation);
    }
}
