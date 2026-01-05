//! Stream types for UDS SSE communication
//!
//! These types define the wire format for streaming inference responses
//! from the worker to the control plane.

use serde::{Deserialize, Serialize};

/// A single generated token in a streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct StreamToken {
    /// The generated text fragment
    pub text: String,
    /// Optional token ID (vocabulary index)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<u32>,
}

/// SSE stream frame types for UDS communication.
///
/// Each variant corresponds to an SSE event type:
/// - `Token` → `event: token`
/// - `Complete` → `event: complete`
/// - `Error` → `event: error`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamFrame {
    /// A generated token
    Token(StreamToken),
    /// Generation complete with final response
    Complete {
        /// Final generated text
        text: String,
        /// Token count
        token_count: usize,
        /// Stop reason (if any)
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason: Option<String>,
    },
    /// Error during generation
    Error {
        /// Error message
        message: String,
        /// Error code (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    /// Generation paused for human review
    Paused {
        /// Unique pause ID for resume correlation
        pause_id: String,
        /// Inference request ID
        inference_id: String,
        /// Why the pause was triggered
        trigger_kind: String,
        /// Context for the reviewer
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        /// Generated text so far
        #[serde(skip_serializing_if = "Option::is_none")]
        text_so_far: Option<String>,
        /// Token count at pause point
        token_count: usize,
    },
}

/// Worker stream events for async channel communication.
///
/// This is the enum used internally for tokio channel communication,
/// wrapping the SSE frame types.
#[derive(Debug, Clone)]
pub enum WorkerStreamEvent {
    /// A single token was generated
    Token(StreamToken),
    /// Generation is complete
    Complete(Box<serde_json::Value>),
    /// An error occurred
    Error(String),
    /// Generation paused for human review
    Paused {
        /// Unique pause ID for resume correlation
        pause_id: String,
        /// Inference request ID
        inference_id: String,
        /// Why the pause was triggered
        trigger_kind: String,
        /// Context for the reviewer
        context: Option<String>,
        /// Generated text so far
        text_so_far: Option<String>,
        /// Token count at pause point
        token_count: usize,
    },
}

impl StreamToken {
    /// Create a new token with text only
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            token_id: None,
        }
    }

    /// Create a new token with text and token ID
    pub fn with_id(text: impl Into<String>, token_id: u32) -> Self {
        Self {
            text: text.into(),
            token_id: Some(token_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_token_serialization() {
        let token = StreamToken::with_id("Hello", 42);
        let json = serde_json::to_string(&token).unwrap();
        assert!(json.contains("\"text\":\"Hello\""));
        assert!(json.contains("\"token_id\":42"));
    }

    #[test]
    fn test_stream_token_no_id() {
        let token = StreamToken::new("World");
        let json = serde_json::to_string(&token).unwrap();
        assert!(json.contains("\"text\":\"World\""));
        assert!(!json.contains("token_id"));
    }

    #[test]
    fn test_stream_frame_token() {
        let frame = StreamFrame::Token(StreamToken::new("test"));
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"type\":\"token\""));
    }

    #[test]
    fn test_stream_frame_complete() {
        let frame = StreamFrame::Complete {
            text: "full response".to_string(),
            token_count: 10,
            stop_reason: Some("eos".to_string()),
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"type\":\"complete\""));
        assert!(json.contains("\"token_count\":10"));
    }

    #[test]
    fn test_stream_frame_error() {
        let frame = StreamFrame::Error {
            message: "timeout".to_string(),
            code: Some("TIMEOUT".to_string()),
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"code\":\"TIMEOUT\""));
    }
}
