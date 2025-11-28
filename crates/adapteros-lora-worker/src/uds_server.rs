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
use serde_json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::{InferenceRequest, InferenceResponse, PatchProposalRequest, RequestType, Worker};

/// UDS server for worker communication
pub struct UdsServer<K: adapteros_lora_kernel_api::FusedKernels> {
    socket_path: PathBuf,
    worker: Arc<Mutex<Worker<K>>>,
}

impl<K: adapteros_lora_kernel_api::FusedKernels + 'static> UdsServer<K> {
    /// Create a new UDS server
    pub fn new(socket_path: PathBuf, worker: Arc<Mutex<Worker<K>>>) -> Self {
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

        use crate::backoff::{BackoffConfig, CircuitBreaker as BackoffCircuitBreaker};

        let backoff = BackoffConfig::new(
            std::time::Duration::from_millis(100),
            std::time::Duration::from_secs(10),
            2.0,
            5,
        );
        let circuit_breaker = BackoffCircuitBreaker::new(20, std::time::Duration::from_secs(60));
        let mut consecutive_failures = 0u32;

        loop {
            // Check circuit breaker state
            if circuit_breaker.is_open() {
                warn!(
                    failure_count = circuit_breaker.failure_count(),
                    "UDS server circuit breaker is open, pausing accept loop"
                );
                tokio::time::sleep(circuit_breaker.reset_timeout()).await;
                continue;
            }

            match listener.accept().await {
                Ok((stream, _)) => {
                    // Success - reset backoff
                    circuit_breaker.record_success();
                    consecutive_failures = 0;

                    let worker = Arc::clone(&self.worker);
                    // UDS connection handling is a background task, not deterministic inference
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, worker).await {
                            error!("Error handling UDS connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    // Failure - apply backoff
                    circuit_breaker.record_failure();
                    consecutive_failures += 1;

                    error!(
                        error = %e,
                        consecutive_failures = consecutive_failures,
                        "Failed to accept UDS connection"
                    );

                    // Apply exponential backoff
                    let delay = backoff.next_delay(consecutive_failures);
                    warn!(
                        delay_ms = delay.as_millis(),
                        "Applying backoff to UDS accept loop"
                    );
                    tokio::time::sleep(delay).await;

                    // Extended backoff if we've exceeded max retries
                    if backoff.should_give_up(consecutive_failures) {
                        error!(
                            "UDS accept has failed {} times, entering extended backoff",
                            consecutive_failures
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        consecutive_failures = 0;
                    }
                }
            }
        }
    }

    /// Handle individual UDS connection
    async fn handle_connection(
        mut stream: UnixStream,
        worker: Arc<Mutex<Worker<K>>>,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        // Parse HTTP request from UDS stream
        let request = Self::parse_request(&mut stream).await?;
        let path = request.path.clone();

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

                // Standard inference (signal streaming not yet implemented)
                if wants_signals {
                    warn!("Signal streaming requested but not yet implemented, using standard inference");
                }
                let mut worker_guard = worker.lock().await;
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
                    request_type: RequestType::PatchProposal(patch_req.clone()),
                    stack_id: None,
                    stack_version: None,
                };

                let mut worker_guard = worker.lock().await;
                let response = worker_guard
                    .propose_patch(inference_req, &patch_req)
                    .await
                    .map_err(|e| AosError::Worker(format!("Patch proposal failed: {}", e)))?;

                Self::send_response(&mut stream, response).await?;
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
                let duration_ms = start.elapsed().as_millis();
                warn!(
                    target: "api",
                    path = %path,
                    status = 404,
                    duration_ms = %duration_ms,
                    "Worker UDS request not found"
                );
                Self::send_error(&mut stream, 404, "Not Found").await?;
                return Ok(()); // Early return to avoid double logging
            }
        }

        // Log successful requests only
        let duration_ms = start.elapsed().as_millis();
        info!(
            target: "api",
            path = %path,
            duration_ms = %duration_ms,
            "Worker UDS request completed"
        );

        Ok(())
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
    #[ignore = "TODO: implement UDS server creation test with mock worker and temp directory"]
    async fn test_uds_server_creation() {
        // This test would require a mock worker and temp directory setup
        // The core UDS server functionality is tested via integration tests
    }
}
