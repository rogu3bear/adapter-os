//! Unix Domain Socket client for communicating with workers
//!
//! This module provides functionality to connect to worker UDS servers
//! and forward inference requests from the control plane.
//!
//! **Signal Protocol Support**: Extended to support receiving signals from
//! workers during inference via Server-Sent Events (SSE).
//!
//! Citation: docs/llm-interface-specification.md §5.1

use serde_json;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Error types for UDS client operations
#[derive(Debug, thiserror::Error)]
pub enum UdsClientError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Worker not available: {0}")]
    WorkerNotAvailable(String),
}

/// UDS client for communicating with workers
pub struct UdsClient {
    timeout: Duration,
}

impl UdsClient {
    /// Create a new UDS client
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Send an inference request to a worker via UDS
    pub async fn infer(
        &self,
        uds_path: &Path,
        request: crate::types::WorkerInferRequest,
    ) -> Result<crate::types::WorkerInferResponse, UdsClientError> {
        // Connect to UDS
        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        // Serialize the request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        // Create HTTP request
        let http_request = format!(
            "POST /inference HTTP/1.1\r\n\
             Host: worker\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            request_json.len(),
            request_json
        );

        // Send request
        tokio::time::timeout(self.timeout, stream.write_all(http_request.as_bytes()))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        // Read response
        let mut response_buffer = Vec::new();
        tokio::time::timeout(self.timeout, stream.read_to_end(&mut response_buffer))
            .await
            .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        // Parse HTTP response
        let response_str = String::from_utf8_lossy(&response_buffer);
        let lines: Vec<&str> = response_str.lines().collect();

        if lines.is_empty() {
            return Err(UdsClientError::RequestFailed("Empty response".to_string()));
        }

        // Check status line
        let status_line = lines[0];
        if !status_line.contains("200 OK") {
            return Err(UdsClientError::RequestFailed(format!(
                "Worker returned error: {}",
                status_line
            )));
        }

        // Find JSON body (after double CRLF)
        let body_start = response_str.find("\r\n\r\n").unwrap_or(0) + 4;
        let json_str = &response_str[body_start..];

        // Parse JSON response
        let response: crate::types::WorkerInferResponse = serde_json::from_str(json_str)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        Ok(response)
    }

    /// Check if a worker is healthy via UDS
    pub async fn health_check(&self, uds_path: &Path) -> Result<bool, UdsClientError> {
        // Connect to UDS
        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        // Send health check request
        let health_request = "GET /health HTTP/1.1\r\nHost: worker\r\n\r\n";

        tokio::time::timeout(self.timeout, stream.write_all(health_request.as_bytes()))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        // Read response
        let mut response_buffer = Vec::new();
        tokio::time::timeout(self.timeout, stream.read_to_end(&mut response_buffer))
            .await
            .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        // Check if response contains 200 OK
        let response_str = String::from_utf8_lossy(&response_buffer);
        Ok(response_str.contains("200 OK"))
    }

    /// Send a patch proposal request to a worker via UDS
    pub async fn propose_patch(
        &self,
        uds_path: &Path,
        request: crate::types::PatchProposalInferRequest,
    ) -> Result<crate::types::PatchProposalInferResponse, UdsClientError> {
        // Connect to UDS
        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        // Serialize the request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        // Create HTTP request
        let http_request = format!(
            "POST /inference HTTP/1.1\r\n\
             Host: worker\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            request_json.len(),
            request_json
        );

        // Send request
        tokio::time::timeout(self.timeout, stream.write_all(http_request.as_bytes()))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        // Read response
        let mut response_buffer = Vec::new();
        tokio::time::timeout(self.timeout, stream.read_to_end(&mut response_buffer))
            .await
            .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        // Parse HTTP response
        let response_str = String::from_utf8_lossy(&response_buffer);
        let lines: Vec<&str> = response_str.lines().collect();

        if lines.is_empty() {
            return Err(UdsClientError::RequestFailed("Empty response".to_string()));
        }

        // Check status line
        let status_line = lines[0];
        if !status_line.contains("200 OK") {
            return Err(UdsClientError::RequestFailed(format!(
                "Worker returned error: {}",
                status_line
            )));
        }

        // Find JSON body (after double CRLF)
        let body_start = response_str.find("\r\n\r\n").unwrap_or(0) + 4;
        let json_str = &response_str[body_start..];

        // Parse JSON response
        let response: crate::types::PatchProposalInferResponse = serde_json::from_str(json_str)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        Ok(response)
    }

    /// Send an inference request with signal streaming support
    ///
    /// Enables bidirectional communication per Specification §5.1.
    /// Signals are received as Server-Sent Events (SSE) and passed to the callback.
    ///
    /// Citation: docs/llm-interface-specification.md §5.1
    pub async fn infer_with_signals<F>(
        &self,
        uds_path: &Path,
        request: crate::types::WorkerInferRequest,
        mut signal_callback: F,
    ) -> Result<crate::types::WorkerInferResponse, UdsClientError>
    where
        F: FnMut(Signal) + Send,
    {
        // Connect to UDS
        let stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        let (read_half, mut write_half) = stream.into_split();

        // Serialize the request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        // Create HTTP request with signal streaming header
        let http_request = format!(
            "POST /inference HTTP/1.1\r\n\
             Host: worker\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             X-Signal-Stream: true\r\n\
             \r\n\
             {}",
            request_json.len(),
            request_json
        );

        // Send request
        tokio::time::timeout(self.timeout, write_half.write_all(http_request.as_bytes()))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        // Read SSE stream
        let mut reader = BufReader::new(read_half);
        let mut response: Option<crate::types::WorkerInferResponse> = None;
        let mut line = String::new();

        // Parse SSE headers
        let mut in_body = false;
        while !in_body {
            line.clear();
            reader
                .read_line(&mut line)
                .await
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

            if line.trim().is_empty() {
                in_body = true;
            }
        }

        // Process SSE events
        let mut event_type = String::new();
        let mut event_data = String::new();

        loop {
            line.clear();
            let n = reader
                .read_line(&mut line)
                .await
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

            if n == 0 {
                break; // End of stream
            }

            let line_trimmed = line.trim();

            if line_trimmed.is_empty() {
                // Event boundary - process accumulated event
                if !event_type.is_empty() && !event_data.is_empty() {
                    match event_type.as_str() {
                        "signal" => {
                            // Parse and emit signal
                            if let Ok(signal) = serde_json::from_str::<Signal>(&event_data) {
                                signal_callback(signal);
                            }
                        }
                        "complete" => {
                            // Inference complete - response should be in final data
                            if let Ok(resp) = serde_json::from_str::<
                                crate::types::WorkerInferResponse,
                            >(&event_data)
                            {
                                response = Some(resp);
                            }
                            break;
                        }
                        _ => {}
                    }
                }

                // Reset for next event
                event_type.clear();
                event_data.clear();
            } else if let Some(stripped) = line_trimmed.strip_prefix("event:") {
                event_type = stripped.trim().to_string();
            } else if let Some(stripped) = line_trimmed.strip_prefix("data:") {
                if !event_data.is_empty() {
                    event_data.push('\n');
                }
                event_data.push_str(stripped.trim());
            }
        }

        response.ok_or_else(|| UdsClientError::RequestFailed("No response received".to_string()))
    }
}

/// Signal type for client consumption
///
/// Simplified signal structure for client-side processing.
/// Full signal definition is in mplora-worker/src/signal.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Signal {
    #[serde(rename = "type")]
    pub signal_type: String,
    pub timestamp: u128,
    pub payload: serde_json::Value,
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

impl Default for UdsClient {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_uds_client_creation() {
        let client = UdsClient::new(Duration::from_secs(5));
        assert_eq!(client.timeout, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_uds_client_default() {
        let client = UdsClient::default();
        assert_eq!(client.timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_infer_request_serialization() {
        let request = crate::types::InferRequest {
            prompt: "Test prompt".to_string(),
            max_tokens: Some(100),
            temperature: None,
            top_k: None,
            top_p: None,
            seed: None,
            require_evidence: None,
        };

        let serialized = serde_json::to_vec(&request).expect("Test request should serialize");
        let deserialized: crate::types::InferRequest =
            serde_json::from_slice(&serialized).expect("Test request should deserialize");

        assert_eq!(request.prompt, deserialized.prompt);
        assert_eq!(request.max_tokens, deserialized.max_tokens);
    }

    #[tokio::test]
    async fn test_patch_proposal_request_serialization() {
        let request = crate::types::PatchProposalInferRequest {
            cpid: "patch-proposal".to_string(),
            prompt: "Add error handling".to_string(),
            max_tokens: 2000,
            require_evidence: true,
            request_type: crate::types::PatchProposalRequestType {
                repo_id: "test-repo".to_string(),
                commit_sha: Some("abc123".to_string()),
                target_files: vec!["src/main.rs".to_string()],
                description: "Add error handling".to_string(),
            },
        };

        let serialized = serde_json::to_vec(&request).expect("Test request should serialize");
        let deserialized: crate::types::PatchProposalInferRequest =
            serde_json::from_slice(&serialized).expect("Test request should deserialize");

        assert_eq!(request.prompt, deserialized.prompt);
        assert_eq!(request.max_tokens, deserialized.max_tokens);
        assert_eq!(request.cpid, deserialized.cpid);
    }
}
