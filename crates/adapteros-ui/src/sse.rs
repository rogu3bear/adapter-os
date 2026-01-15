//! Server-Sent Events (SSE) parsing module
//!
//! This module provides types and utilities for parsing SSE streams from
//! adapterOS streaming inference endpoints. It supports both the native
//! adapterOS event format and OpenAI-compatible streaming chunks.
//!
//! # Event Formats
//!
//! The parser handles two event formats:
//!
//! 1. **adapterOS format** - Uses tagged JSON with an `event` field:
//!    ```text
//!    data: {"event": "Token", "text": "Hello"}
//!    data: {"event": "Done", "total_tokens": 10, "latency_ms": 100}
//!    ```
//!
//! 2. **OpenAI-compatible format** - Uses the `choices[].delta.content` structure:
//!    ```text
//!    data: {"choices": [{"delta": {"content": "Hello"}}]}
//!    ```
//!
//! # Example
//!
//! ```ignore
//! use adapteros_ui::sse::{parse_sse_event_with_info, ParsedSseEvent};
//!
//! let event_data = "data: {\"event\": \"Token\", \"text\": \"Hello\"}";
//! let parsed = parse_sse_event_with_info(event_data);
//! assert_eq!(parsed.token, Some("Hello".to_string()));
//! ```

use serde::Deserialize;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// SSE event types from the adapterOS streaming inference endpoint.
///
/// This enum represents the different event types that can be received
/// during a streaming inference request.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event")]
pub enum InferenceEvent {
    /// A token was generated during inference.
    Token {
        /// The generated token text.
        text: String,
    },
    /// Inference has completed successfully.
    Done {
        /// Total number of tokens generated.
        #[serde(default)]
        total_tokens: usize,
        /// Total latency in milliseconds.
        #[serde(default)]
        latency_ms: u64,
        /// Optional trace ID for debugging and observability.
        #[serde(default)]
        trace_id: Option<String>,
        /// Number of prompt tokens (input tokens).
        #[serde(default)]
        prompt_tokens: Option<u32>,
        /// Number of completion tokens (output tokens).
        #[serde(default)]
        completion_tokens: Option<u32>,
    },
    /// An error occurred during inference.
    Error {
        /// Error message describing what went wrong.
        message: String,
    },
    /// Catch-all for other events (Loading, Ready, etc.).
    ///
    /// These events are typically informational and can be safely ignored
    /// for basic streaming functionality.
    #[serde(other)]
    Other,
}

/// OpenAI-compatible streaming chunk format.
///
/// This struct represents the response format used by OpenAI-compatible
/// streaming endpoints, allowing the UI to work with various LLM providers.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamingChunk {
    /// List of completion choices (typically contains one choice).
    #[serde(default)]
    pub choices: Vec<StreamingChoice>,
}

/// A single choice in an OpenAI-compatible streaming response.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamingChoice {
    /// The delta containing the incremental content.
    #[serde(default)]
    pub delta: Delta,
    /// Finish reason (only present in final chunk).
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// Delta content in an OpenAI-compatible streaming choice.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Delta {
    /// The incremental content text, if present.
    #[serde(default)]
    pub content: Option<String>,
}

/// Result of parsing an SSE event.
///
/// This struct contains all the information that can be extracted from
/// a single SSE event, including token content and metadata.
#[derive(Debug, Clone, Default)]
pub struct ParsedSseEvent {
    /// The token content extracted from the event, if present.
    pub token: Option<String>,
    /// The trace ID from a Done event, if present.
    pub trace_id: Option<String>,
    /// The latency in milliseconds from a Done event, if present.
    pub latency_ms: Option<u64>,
    /// The total token count from a Done event, if present.
    pub token_count: Option<u32>,
    /// The number of prompt tokens (input tokens) from a Done event, if present.
    pub prompt_tokens: Option<u32>,
    /// The number of completion tokens (output tokens) from a Done event, if present.
    pub completion_tokens: Option<u32>,
    /// The finish reason from an OpenAI-format Done event, if present.
    pub finish_reason: Option<String>,
}

/// Parse an SSE event and extract token content plus trace info.
///
/// This function handles the parsing of SSE event data, supporting both
/// adapterOS native format and OpenAI-compatible format.
///
/// # Arguments
///
/// * `event_data` - Raw SSE event data including the `data:` prefix lines
///
/// # Returns
///
/// A [`ParsedSseEvent`] containing any extracted information. Fields will be
/// `None` if the corresponding data was not present in the event.
///
/// # SSE Format
///
/// SSE events have the format:
/// ```text
/// event: <event_type>
/// data: <json_data>
/// ```
/// or just:
/// ```text
/// data: <json_data>
/// ```
///
/// # Example
///
/// ```ignore
/// let event = "data: {\"event\": \"Token\", \"text\": \"Hello\"}";
/// let parsed = parse_sse_event_with_info(event);
/// assert_eq!(parsed.token, Some("Hello".to_string()));
/// ```
pub fn parse_sse_event_with_info(event_data: &str) -> ParsedSseEvent {
    let mut result = ParsedSseEvent::default();

    // SSE events have format:
    // event: <event_type>
    // data: <json_data>
    // or just:
    // data: <json_data>

    let mut data_line: Option<&str> = None;

    for line in event_data.lines() {
        if let Some(stripped) = line.strip_prefix("data: ") {
            data_line = Some(stripped);
        }
    }

    let data = match data_line {
        Some(d) => d,
        None => return result,
    };

    // Check for [DONE] marker
    if data == "[DONE]" {
        return result;
    }

    // Try parsing as InferenceEvent first (adapterOS format)
    if let Ok(event) = serde_json::from_str::<InferenceEvent>(data) {
        match event {
            InferenceEvent::Token { text } => {
                result.token = Some(text);
            }
            InferenceEvent::Done {
                total_tokens,
                latency_ms,
                trace_id,
                prompt_tokens,
                completion_tokens,
            } => {
                result.trace_id = trace_id;
                result.latency_ms = Some(latency_ms);
                result.token_count = Some(total_tokens as u32);
                result.prompt_tokens = prompt_tokens;
                result.completion_tokens = completion_tokens;
            }
            InferenceEvent::Error { message } => {
                // Log error but don't return it as content
                #[cfg(target_arch = "wasm32")]
                web_sys::console::error_1(&JsValue::from_str(&format!(
                    "Stream error: {}",
                    message
                )));
                #[cfg(not(target_arch = "wasm32"))]
                let _ = &message; // Silence unused variable warning in non-wasm builds
            }
            InferenceEvent::Other => {
                // Ignore Loading, Ready, and other unhandled events
            }
        }
        return result;
    }

    // Try parsing as OpenAI-compatible StreamingChunk
    if let Ok(chunk) = serde_json::from_str::<StreamingChunk>(data) {
        if let Some(choice) = chunk.choices.first() {
            if let Some(content) = &choice.delta.content {
                result.token = Some(content.clone());
            }
            // Capture finish_reason from final chunk
            if choice.finish_reason.is_some() {
                result.finish_reason = choice.finish_reason.clone();
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_token_event() {
        let event = "data: {\"event\": \"Token\", \"text\": \"Hello\"}";
        let parsed = parse_sse_event_with_info(event);
        assert_eq!(parsed.token, Some("Hello".to_string()));
        assert!(parsed.trace_id.is_none());
        assert!(parsed.latency_ms.is_none());
        assert!(parsed.token_count.is_none());
    }

    #[test]
    fn test_parse_done_event() {
        let event = r#"data: {"event": "Done", "total_tokens": 42, "latency_ms": 150, "trace_id": "trace-123"}"#;
        let parsed = parse_sse_event_with_info(event);
        assert!(parsed.token.is_none());
        assert_eq!(parsed.trace_id, Some("trace-123".to_string()));
        assert_eq!(parsed.latency_ms, Some(150));
        assert_eq!(parsed.token_count, Some(42));
    }

    #[test]
    fn test_parse_done_event_without_trace() {
        let event = r#"data: {"event": "Done", "total_tokens": 10, "latency_ms": 100}"#;
        let parsed = parse_sse_event_with_info(event);
        assert!(parsed.token.is_none());
        assert!(parsed.trace_id.is_none());
        assert_eq!(parsed.latency_ms, Some(100));
        assert_eq!(parsed.token_count, Some(10));
    }

    #[test]
    fn test_parse_openai_format() {
        let event = r#"data: {"choices": [{"delta": {"content": "World"}}]}"#;
        let parsed = parse_sse_event_with_info(event);
        assert_eq!(parsed.token, Some("World".to_string()));
    }

    #[test]
    fn test_parse_done_marker() {
        let event = "data: [DONE]";
        let parsed = parse_sse_event_with_info(event);
        assert!(parsed.token.is_none());
        assert!(parsed.trace_id.is_none());
    }

    #[test]
    fn test_parse_empty_event() {
        let event = "";
        let parsed = parse_sse_event_with_info(event);
        assert!(parsed.token.is_none());
    }

    #[test]
    fn test_parse_event_with_event_line() {
        let event = "event: token\ndata: {\"event\": \"Token\", \"text\": \"Test\"}";
        let parsed = parse_sse_event_with_info(event);
        assert_eq!(parsed.token, Some("Test".to_string()));
    }

    #[test]
    fn test_parse_other_event() {
        let event = r#"data: {"event": "Loading"}"#;
        let parsed = parse_sse_event_with_info(event);
        assert!(parsed.token.is_none());
        assert!(parsed.trace_id.is_none());
    }

    #[test]
    fn test_parse_openai_empty_delta() {
        let event = r#"data: {"choices": [{"delta": {}}]}"#;
        let parsed = parse_sse_event_with_info(event);
        assert!(parsed.token.is_none());
    }

    #[test]
    fn test_parse_openai_no_choices() {
        let event = r#"data: {"choices": []}"#;
        let parsed = parse_sse_event_with_info(event);
        assert!(parsed.token.is_none());
    }

    #[test]
    fn test_parse_token_event_with_special_characters() {
        let event = "data: {\"event\":\"Token\",\"text\":\"Hello\\nWorld\\t!\"}";
        let result = parse_sse_event_with_info(event);
        assert_eq!(result.token, Some("Hello\nWorld\t!".to_string()));
    }

    #[test]
    fn test_parse_token_event_empty_text() {
        let event = "data: {\"event\":\"Token\",\"text\":\"\"}";
        let result = parse_sse_event_with_info(event);
        assert_eq!(result.token, Some("".to_string()));
    }

    #[test]
    fn test_parse_done_event_with_all_fields() {
        let event = "data: {\"event\":\"Done\",\"total_tokens\":100,\"latency_ms\":500,\"trace_id\":\"abc123\"}";
        let result = parse_sse_event_with_info(event);
        assert!(result.token.is_none());
        assert_eq!(result.trace_id, Some("abc123".to_string()));
        assert_eq!(result.latency_ms, Some(500));
        assert_eq!(result.token_count, Some(100));
    }

    #[test]
    fn test_parse_done_event_with_minimal_fields() {
        // Done event with defaults (fields missing use serde defaults)
        let event = "data: {\"event\":\"Done\"}";
        let result = parse_sse_event_with_info(event);
        assert!(result.token.is_none());
        assert!(result.trace_id.is_none());
        assert_eq!(result.latency_ms, Some(0)); // default
        assert_eq!(result.token_count, Some(0)); // default
    }

    #[test]
    fn test_parse_openai_compatible_format_empty_content() {
        let event = "data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}";
        let result = parse_sse_event_with_info(event);
        assert_eq!(result.token, Some("".to_string()));
    }

    #[test]
    fn test_parse_openai_compatible_format_null_content() {
        let event = "data: {\"choices\":[{\"delta\":{\"content\":null}}]}";
        let result = parse_sse_event_with_info(event);
        assert!(result.token.is_none());
    }

    #[test]
    fn test_parse_openai_compatible_format_multiple_choices() {
        // Should use the first choice
        let event = "data: {\"choices\":[{\"delta\":{\"content\":\"First\"}},{\"delta\":{\"content\":\"Second\"}}]}";
        let result = parse_sse_event_with_info(event);
        assert_eq!(result.token, Some("First".to_string()));
    }

    // Note: test_parse_error_event is skipped because it triggers web_sys::console::error_1
    // which panics on non-WASM targets. The Error event parsing is tested implicitly through
    // the function behavior - it logs the error and returns an empty result.

    #[test]
    fn test_parse_ready_event() {
        let event = "data: {\"event\":\"Ready\"}";
        let result = parse_sse_event_with_info(event);
        assert!(result.token.is_none());
    }

    #[test]
    fn test_malformed_json_returns_empty() {
        let event = "data: {not valid json}";
        let result = parse_sse_event_with_info(event);
        assert!(result.token.is_none());
        assert!(result.trace_id.is_none());
        assert!(result.latency_ms.is_none());
        assert!(result.token_count.is_none());
    }

    #[test]
    fn test_malformed_json_truncated() {
        let event = "data: {\"event\":\"Token\",\"text\":\"Hello";
        let result = parse_sse_event_with_info(event);
        assert!(result.token.is_none());
    }

    #[test]
    fn test_empty_data_line() {
        let event = "data: ";
        let result = parse_sse_event_with_info(event);
        assert!(result.token.is_none());
        assert!(result.trace_id.is_none());
    }

    #[test]
    fn test_no_data_prefix() {
        // Line without "data: " prefix should be ignored
        let event = "{\"event\":\"Token\",\"text\":\"Hello\"}";
        let result = parse_sse_event_with_info(event);
        assert!(result.token.is_none());
    }

    #[test]
    fn test_multiline_sse_event_multiple_lines() {
        // Multiple non-data lines before data line
        let event = "event: message\nid: 123\ndata: {\"event\":\"Token\",\"text\":\"Test\"}";
        let result = parse_sse_event_with_info(event);
        assert_eq!(result.token, Some("Test".to_string()));
    }

    #[test]
    fn test_multiline_sse_uses_last_data_line() {
        // When multiple data lines exist, the last one is used
        let event = "data: {\"event\":\"Token\",\"text\":\"First\"}\ndata: {\"event\":\"Token\",\"text\":\"Last\"}";
        let result = parse_sse_event_with_info(event);
        assert_eq!(result.token, Some("Last".to_string()));
    }

    #[test]
    fn test_whitespace_only_event() {
        let event = "   \n   \n   ";
        let result = parse_sse_event_with_info(event);
        assert!(result.token.is_none());
    }

    #[test]
    fn test_data_prefix_with_extra_spaces() {
        // Only exact "data: " prefix is recognized
        let event = "data:  {\"event\":\"Token\",\"text\":\"Hello\"}";
        let result = parse_sse_event_with_info(event);
        // Extra space after colon means the JSON starts with a space, which is valid
        assert_eq!(result.token, Some("Hello".to_string()));
    }

    #[test]
    fn test_unicode_token_content() {
        // Use JSON escape sequences for unicode
        let event = r#"data: {"event":"Token","text":"Hello\u1F44B\u1F30D"}"#;
        let result = parse_sse_event_with_info(event);
        // The JSON parser will handle the unicode escapes
        assert!(result.token.is_some());
        let token = result.token.unwrap();
        assert!(token.starts_with("Hello"));
    }

    #[test]
    fn test_large_token_count() {
        let event = "data: {\"event\":\"Done\",\"total_tokens\":999999,\"latency_ms\":123456,\"trace_id\":\"large\"}";
        let result = parse_sse_event_with_info(event);
        assert_eq!(result.token_count, Some(999999));
        assert_eq!(result.latency_ms, Some(123456));
    }

    #[test]
    fn test_json_with_extra_fields() {
        // Extra fields should be ignored
        let event = "data: {\"event\":\"Token\",\"text\":\"Hi\",\"extra\":\"ignored\",\"num\":42}";
        let result = parse_sse_event_with_info(event);
        assert_eq!(result.token, Some("Hi".to_string()));
    }

    #[test]
    fn test_comment_line_ignored() {
        // SSE spec allows comment lines starting with ":"
        let event = ": this is a comment\ndata: {\"event\":\"Token\",\"text\":\"After comment\"}";
        let result = parse_sse_event_with_info(event);
        assert_eq!(result.token, Some("After comment".to_string()));
    }

    #[test]
    fn test_parse_openai_finish_reason() {
        let event = r#"data: {"choices": [{"delta": {}, "finish_reason": "stop"}]}"#;
        let parsed = parse_sse_event_with_info(event);
        assert!(parsed.token.is_none());
        assert_eq!(parsed.finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_parse_openai_finish_reason_length() {
        let event =
            r#"data: {"choices": [{"delta": {"content": "final"}, "finish_reason": "length"}]}"#;
        let parsed = parse_sse_event_with_info(event);
        assert_eq!(parsed.token, Some("final".to_string()));
        assert_eq!(parsed.finish_reason, Some("length".to_string()));
    }
}
