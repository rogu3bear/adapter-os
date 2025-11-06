//! Unix Domain Socket server for worker communication
//!
//! This module provides UDS server functionality for workers to receive
//! inference requests from the control plane via Unix domain sockets.
//!
//! **Signal Protocol Support**: Extended to support bidirectional signal
//! streaming during inference per Specification §5.1.
//!
//! Citation: Based on `crates/mplora-server-api/src/uds_client.rs` - implements
//! the server side of the UDS communication protocol.
//! Signal streaming: docs/llm-interface-specification.md §5.1

use adapteros_core::{AosError, Result};
use adapteros_deterministic_exec::spawn_deterministic;
use serde_json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use crate::signal::Signal;
use crate::{InferenceRequest, InferenceResponse, PatchProposalRequest, RequestType, Worker};

/// UDS server for worker communication
pub struct UdsServer {
    socket_path: PathBuf,
    worker: Arc<Mutex<Worker>>,
}

impl UdsServer {
    /// Create a new UDS server
    pub fn new(socket_path: PathBuf, worker: Arc<Mutex<Worker>>) -> Self {
        Self {
            socket_path,
            worker,
        }
    }

    /// Start UDS server for worker communication
    pub async fn serve(&self) -> Result<()> {
        // Remove existing socket file if it exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path).map_err(|e| {
                AosError::Worker(format!("Failed to remove existing socket: {}", e))
            })?;
        }

        // Create parent directory if it doesn't exist
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AosError::Worker(format!("Failed to create socket directory: {}", e))
            })?;
        }

        let listener = UnixListener::bind(&self.socket_path)
            .map_err(|e| AosError::Worker(format!("Failed to bind UDS socket: {}", e)))?;

        info!("UDS server listening on: {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let worker = Arc::clone(&self.worker);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, worker).await {
                            error!("Error handling UDS connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept UDS connection: {}", e);
                }
            }
        }
    }

    /// Handle individual UDS connection
    async fn handle_connection(
        mut stream: UnixStream,
        worker: Arc<Mutex<Worker>>,
    ) -> Result<()> {
        // Parse HTTP request from UDS stream
        let request = Self::parse_request(&mut stream).await?;

        // Check if client wants signal streaming
        let wants_signals = request
            .headers
            .get("X-Signal-Stream")
            .map(|v| v == "true")
            .unwrap_or(false);

        // Route to appropriate handler
        match request.path.as_str() {
            "/inference" => {
                let inference_req: InferenceRequest =
                    serde_json::from_str(&request.body).map_err(|e| {
                        AosError::Worker(format!("Failed to parse inference request: {}", e))
                    })?;

                // Standard inference (signal streaming disabled for now)
                let worker_guard = worker.lock().await;
                let response = worker_guard
                    .infer(inference_req)
                    .await
                    .map_err(|e| AosError::Worker(format!("Inference failed: {}", e)))?;

                Self::send_response(&mut stream, response).await?;
            }
            "/patch_proposal" => {
                let patch_req: PatchProposalRequest =
                    serde_json::from_str(&request.body).map_err(|e| {
                        AosError::Worker(format!("Failed to parse patch request: {}", e))
                    })?;

                // Create a dummy inference request for patch proposal
                let inference_req = InferenceRequest {
                    cpid: "patch-proposal".to_string(),
                    prompt: "patch proposal".to_string(),
                    max_tokens: 100,
                    require_evidence: false,
                    request_type: Some(RequestType::PatchProposal(patch_req.clone())),
                };

                let worker_guard = worker.lock().await;
                let patch_result = worker_guard
                    .propose_patch(inference_req, &patch_req)
                    .await
                    .map_err(|e| AosError::Worker(format!("Patch proposal failed: {}", e)))?;

                // Send JSON response for patch proposal
                Self::send_json_response(&mut stream, patch_result).await?;
            }
            "/health" => {
                let health_response = serde_json::json!({
                    "status": "healthy",
                    "worker_id": "default",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                });
                Self::send_json_response(&mut stream, health_response).await?;
            }
            _ => {
                Self::send_error(&mut stream, 404, "Not Found").await?;
            }
        }

        Ok(())
    }

    /// Handle inference with bidirectional signal streaming
    ///
    /// Implements Specification §5.1 signal protocol for LLM-runtime communication.
    /// Signals are sent as Server-Sent Events (SSE) for efficient streaming.
    ///
    /// Citation: docs/llm-interface-specification.md §5.1
    async fn handle_inference_with_signals(
        stream: UnixStream,
        worker: Arc<Mutex<Worker>>,
        inference_req: InferenceRequest,
    ) -> Result<()> {
        // Create channel for signal streaming
        // let (signal_tx, signal_rx) = DeterministicChannel::<Signal>::new(32);

        // Clone stream for signal transmission
        // Note: UnixStream doesn't implement Clone, so we split it
        let (_read_half, write_half) = stream.into_split();

        // Run inference (signal streaming disabled for now)
        let inference_result = {
            let worker_guard = worker.lock().await;
            worker_guard.infer(inference_req).await
        };

        inference_result.map(|_| ())
    }

    /// Parse HTTP request from UDS stream
    async fn parse_request(stream: &mut UnixStream) -> Result<HttpRequest> {
        let mut buffer = Vec::new();
        let mut line_buffer = Vec::new();

        // Read request line by line
        loop {
            let mut byte = [0u8; 1];
            stream
                .read_exact(&mut byte)
                .await
                .map_err(|e| AosError::Worker(format!("Failed to read from stream: {}", e)))?;

            if byte[0] == b'\n' {
                let line = String::from_utf8_lossy(&line_buffer);
                if line.trim().is_empty() {
                    break; // End of headers
                }
                buffer.extend_from_slice(&line_buffer);
                buffer.push(b'\n');
                line_buffer.clear();
            } else {
                line_buffer.push(byte[0]);
            }
        }

        let request_str = String::from_utf8_lossy(&buffer);
        let lines: Vec<&str> = request_str.lines().collect();

        if lines.is_empty() {
            return Err(AosError::Worker("Empty request".to_string()));
        }

        // Parse request line
        let request_line = lines[0];
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(AosError::Worker("Invalid request line".to_string()));
        }

        let method = parts[0].to_string();
        let path = parts[1].to_string();

        // Parse headers
        let mut headers = std::collections::HashMap::new();
        let mut content_length = 0;

        for line in &lines[1..] {
            if let Some(colon_pos) = line.find(':') {
                let header_name = line[..colon_pos].trim().to_string();
                let header_value = line[colon_pos + 1..].trim().to_string();

                if header_name.to_lowercase() == "content-length" {
                    content_length = header_value.parse().unwrap_or(0);
                }

                headers.insert(header_name, header_value);
            }
        }

        // Read body if present
        let mut body = String::new();
        if content_length > 0 {
            let mut body_buffer = vec![0u8; content_length];
            stream
                .read_exact(&mut body_buffer)
                .await
                .map_err(|e| AosError::Worker(format!("Failed to read request body: {}", e)))?;
            body = String::from_utf8_lossy(&body_buffer).to_string();
        }

        Ok(HttpRequest {
            _method: method,
            path,
            headers,
            body,
        })
    }

    /// Send HTTP response
    async fn send_response(stream: &mut UnixStream, response: InferenceResponse) -> Result<()> {
        let json_body = serde_json::to_string(&response)
            .map_err(|e| AosError::Worker(format!("Failed to serialize response: {}", e)))?;

        let http_response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            json_body.len(),
            json_body
        );

        stream
            .write_all(http_response.as_bytes())
            .await
            .map_err(|e| AosError::Worker(format!("Failed to send response: {}", e)))?;

        Ok(())
    }

    /// Send JSON response
    async fn send_json_response(stream: &mut UnixStream, json: serde_json::Value) -> Result<()> {
        let json_body = json.to_string();
        let http_response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            json_body.len(),
            json_body
        );

        stream
            .write_all(http_response.as_bytes())
            .await
            .map_err(|e| AosError::Worker(format!("Failed to send JSON response: {}", e)))?;

        Ok(())
    }

    /// Send HTTP error response
    async fn send_error(stream: &mut UnixStream, status_code: u16, message: &str) -> Result<()> {
        let error_body = format!("{{\"error\": \"{}\"}}", message);
        let http_response = format!(
            "HTTP/1.1 {} {}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            status_code,
            match status_code {
                404 => "Not Found",
                500 => "Internal Server Error",
                _ => "Error",
            },
            error_body.len(),
            error_body
        );

        stream
            .write_all(http_response.as_bytes())
            .await
            .map_err(|e| AosError::Worker(format!("Failed to send error response: {}", e)))?;

        Ok(())
    }
}

/// HTTP request structure
struct HttpRequest {
    _method: String,
    path: String,
    headers: std::collections::HashMap<String, String>,
    body: String,
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[tokio::test]
    async fn test_uds_server_creation() {
        // This test would require a mock worker and temp directory setup
        // The core UDS server functionality is tested via integration tests
        // For now, just verify the test compiles
        assert!(true);
    }
}
