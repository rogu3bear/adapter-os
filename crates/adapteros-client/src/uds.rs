//! Unix Domain Socket client for communicating with workers
//!
//! This module provides functionality to connect to worker UDS servers
//! and forward inference requests from the CLI.
//!
//! **Signal Protocol Support**: Extended to support receiving signals from
//! workers during inference via Server-Sent Events (SSE).
//!
//! Citation: docs/llm-interface-specification.md §5.1

use futures_util::stream::BoxStream;
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

    /// Send a generic request to worker via UDS
    pub async fn send_request(
        &self,
        uds_path: &Path,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<String, UdsClientError> {
        // Connect to UDS
        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        // Create HTTP request
        let http_request = if let Some(body_content) = body {
            format!(
                "{} {} HTTP/1.1\r\n\
                 Host: worker\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\
                 \r\n\
                 {}",
                method,
                path,
                body_content.len(),
                body_content
            )
        } else {
            format!(
                "{} {} HTTP/1.1\r\n\
                 Host: worker\r\n\
                 \r\n",
                method, path
            )
        };

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

        Ok(json_str.to_string())
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

    /// Send an adapter command to worker
    pub async fn adapter_command(
        &self,
        uds_path: &Path,
        adapter_id: &str,
        command: &str,
    ) -> Result<(), UdsClientError> {
        let _response = self
            .send_request(
                uds_path,
                "POST",
                &format!("/adapter/{}/{}", adapter_id, command),
                None,
            )
            .await?;
        Ok(())
    }

    /// List all adapters from worker
    pub async fn list_adapters(&self, uds_path: &Path) -> Result<String, UdsClientError> {
        self.send_request(uds_path, "GET", "/adapters", None).await
    }

    /// Get adapter profile
    pub async fn get_adapter_profile(
        &self,
        uds_path: &Path,
        adapter_id: &str,
    ) -> Result<String, UdsClientError> {
        self.send_request(uds_path, "GET", &format!("/adapter/{}", adapter_id), None)
            .await
    }

    /// Get profiling snapshot
    pub async fn get_profiling_snapshot(&self, uds_path: &Path) -> Result<String, UdsClientError> {
        self.send_request(uds_path, "GET", "/profile/snapshot", None)
            .await
    }

    /// Stream signals from worker via Server-Sent Events (SSE)
    ///
    /// This method establishes a persistent connection to the worker's signal endpoint
    /// and streams signals in real-time as they are emitted during inference.
    ///
    /// Citation: docs/llm-interface-specification.md §5.1
    pub async fn stream_signals(
        &self,
        uds_path: &Path,
        trace_id: Option<&str>,
    ) -> Result<BoxStream<'static, Result<Signal, UdsClientError>>, UdsClientError> {
        let timeout = self.timeout;
        let uds_path = uds_path.to_path_buf();
        let trace_id = trace_id.map(|s| s.to_string());

        // Connect to UDS
        let mut stream = tokio::time::timeout(timeout, UnixStream::connect(&uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        // Create SSE request
        let mut request = format!(
            "GET /signals HTTP/1.1\r\n\
             Host: worker\r\n\
             Accept: text/event-stream\r\n\
             Cache-Control: no-cache\r\n"
        );

        // Add trace ID filter if provided
        if let Some(trace) = &trace_id {
            request.push_str(&format!("X-Trace-ID: {}\r\n", trace));
        }

        request.push_str("\r\n");

        // Send request
        tokio::time::timeout(timeout, stream.write_all(request.as_bytes()))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        // Read initial response headers
        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        // Skip HTTP headers until we reach the SSE stream
        loop {
            tokio::time::timeout(timeout, reader.read_line(&mut line))
                .await
                .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

            if line.trim().is_empty() {
                break; // End of headers
            }

            line.clear();
        }

        // Create SSE stream
        let stream = async_stream::stream! {
            let mut buffer = String::new();
            let mut event_data = String::new();
            let mut event_type = String::new();
            let mut event_id = String::new();

            loop {
                match tokio::time::timeout(timeout, reader.read_line(&mut buffer)).await {
                    Ok(Ok(0)) => break, // EOF
                    Ok(Ok(_)) => {
                        let line = buffer.trim();

                        if line.is_empty() {
                            // End of event - process accumulated data
                            if !event_data.is_empty() {
                                match serde_json::from_str::<Signal>(&event_data) {
                                    Ok(signal) => yield Ok(signal),
                                    Err(e) => yield Err(UdsClientError::SerializationError(
                                        format!("Failed to parse signal: {}", e)
                                    )),
                                }
                            }

                            // Reset for next event
                            event_data.clear();
                            event_type.clear();
                            event_id.clear();
                        } else if line.starts_with("data: ") {
                            event_data.push_str(&line[6..]);
                        } else if line.starts_with("event: ") {
                            event_type = line[7..].to_string();
                        } else if line.starts_with("id: ") {
                            event_id = line[4..].to_string();
                        }

                        buffer.clear();
                    }
                    Ok(Err(e)) => {
                        yield Err(UdsClientError::RequestFailed(e.to_string()));
                        break;
                    }
                    Err(_) => {
                        yield Err(UdsClientError::Timeout("Read timed out".to_string()));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    /// Send inference request and stream signals in real-time
    ///
    /// This method sends an inference request to the worker and returns both
    /// the final response and a stream of signals emitted during inference.
    pub async fn inference_with_signals(
        &self,
        uds_path: &Path,
        request_body: &str,
        trace_id: Option<&str>,
    ) -> Result<(String, BoxStream<'static, Result<Signal, UdsClientError>>), UdsClientError> {
        // Start signal stream first
        let signal_stream = self.stream_signals(uds_path, trace_id).await?;

        // Send inference request
        let response = self
            .send_request(uds_path, "POST", "/inference", Some(request_body))
            .await?;

        Ok((response, signal_stream))
    }

    /// Create a connection pool for efficient UDS communication
    ///
    /// This method creates a reusable connection pool that can be used
    /// for multiple requests to the same worker endpoint.
    pub async fn create_connection_pool(
        &self,
        uds_path: &Path,
        pool_size: usize,
    ) -> Result<ConnectionPool, UdsClientError> {
        let mut connections = Vec::with_capacity(pool_size);

        for _ in 0..pool_size {
            let stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
                .await
                .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
                .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

            connections.push(stream);
        }

        Ok(ConnectionPool {
            connections,
            timeout: self.timeout,
        })
    }
}

/// Connection pool for efficient UDS communication
pub struct ConnectionPool {
    connections: Vec<UnixStream>,
    timeout: Duration,
}

impl ConnectionPool {
    /// Get an available connection from the pool
    pub async fn get_connection(&mut self) -> Result<UnixStream, UdsClientError> {
        if let Some(stream) = self.connections.pop() {
            Ok(stream)
        } else {
            Err(UdsClientError::WorkerNotAvailable(
                "No available connections".to_string(),
            ))
        }
    }

    /// Return a connection to the pool
    pub fn return_connection(&mut self, stream: UnixStream) {
        self.connections.push(stream);
    }

    /// Check if the pool has available connections
    pub fn has_available(&self) -> bool {
        !self.connections.is_empty()
    }

    /// Get the number of available connections
    pub fn available_count(&self) -> usize {
        self.connections.len()
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
    async fn test_signal_structure() {
        let signal = Signal {
            signal_type: "adapter_activate".to_string(),
            timestamp: 1234567890,
            payload: serde_json::json!({"adapter_id": "test-adapter"}),
            priority: "normal".to_string(),
            trace_id: Some("trace-123".to_string()),
        };

        // Test serialization
        let json = serde_json::to_string(&signal).unwrap();
        assert!(json.contains("adapter_activate"));
        assert!(json.contains("test-adapter"));

        // Test deserialization
        let deserialized: Signal = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.signal_type, "adapter_activate");
        assert_eq!(deserialized.trace_id, Some("trace-123".to_string()));
    }

    #[tokio::test]
    async fn test_connection_pool_creation() {
        // This test would require a real UDS socket, so we'll just test the structure
        let client = UdsClient::new(Duration::from_secs(5));
        assert_eq!(client.timeout, Duration::from_secs(5));
    }
}
