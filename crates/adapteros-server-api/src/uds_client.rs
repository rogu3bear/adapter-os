//! Unix Domain Socket client for communicating with workers
//!
//! This module provides functionality to connect to worker UDS servers
//! and forward inference requests from the control plane.
//!
//! **Signal Protocol Support**: Extended to support receiving signals from
//! workers during inference via Server-Sent Events (SSE).
//!
//! Citation: docs/llm-interface-specification.md §5.1

use serde::Deserialize;
use serde_json;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::warn;

// ============================================================================
// Routing Guard - Ensures all inference goes through the router
// ============================================================================

thread_local! {
    static ROUTING_GUARD: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

/// Mark that we're in a routed context (called by InferenceCore)
pub fn enter_routed_context() {
    ROUTING_GUARD.with(|g| g.set(true));
}

/// Clear routed context after request completes
pub fn exit_routed_context() {
    ROUTING_GUARD.with(|g| g.set(false));
}

/// Check if currently in routed context
pub fn is_routed_context() -> bool {
    ROUTING_GUARD.with(|g| g.get())
}

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
    #[error("Routing bypass detected: {0}")]
    RoutingBypass(String),
}

/// UDS client for communicating with workers
pub struct UdsClient {
    timeout: Duration,
}

/// CoreML verification snapshot returned by worker debug endpoint.
#[derive(Debug, Deserialize)]
pub struct WorkerCoremlVerification {
    pub mode: Option<String>,
    #[serde(default = "default_status")]
    pub status: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub source: Option<String>,
    #[serde(default)]
    pub mismatch: bool,
}

fn default_status() -> String {
    "unknown".to_string()
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
        authorization: Option<&str>,
    ) -> Result<crate::types::WorkerInferResponse, UdsClientError> {
        // GUARD: Fail hard if not in routed context
        if !is_routed_context() {
            tracing::error!(
                kind = "ROUTING_BYPASS",
                "Inference call attempted without routing; this is a bug. Use InferenceCore::route_and_infer()"
            );
            return Err(UdsClientError::RoutingBypass(
                "Inference call attempted without routing. Use InferenceCore::route_and_infer()"
                    .into(),
            ));
        }

        // Connect to UDS
        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        // Serialize the request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        let auth_header = authorization
            .map(|token| format!("Authorization: ApiKey {}\r\n", token))
            .unwrap_or_default();

        // Create HTTP request
        let http_request = format!(
            "POST /inference HTTP/1.1\r\n\
             Host: worker\r\n\
             Content-Type: application/json\r\n\
             {}\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            auth_header,
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

        // Find JSON body (after double CRLF) - safe slicing to prevent panic
        let json_str = match response_str.find("\r\n\r\n") {
            Some(pos) => response_str.get(pos + 4..).unwrap_or_else(|| {
                warn!(
                    response_len = response_str.len(),
                    header_end_pos = pos,
                    "Malformed HTTP response: body offset exceeds response length"
                );
                ""
            }),
            None => {
                warn!(
                    response_preview = %response_str.chars().take(100).collect::<String>(),
                    "Malformed HTTP response: missing header/body separator (\\r\\n\\r\\n)"
                );
                ""
            }
        };

        // Parse JSON response
        let response: crate::types::WorkerInferResponse = serde_json::from_str(json_str)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        Ok(response)
    }

    /// Send an inference request to a worker via UDS with latency tracking
    ///
    /// Returns both the response and the round-trip latency in milliseconds.
    /// This is useful for monitoring worker performance and reporting metrics.
    pub async fn infer_timed(
        &self,
        uds_path: &Path,
        request: crate::types::WorkerInferRequest,
        authorization: Option<&str>,
    ) -> Result<(crate::types::WorkerInferResponse, u64), UdsClientError> {
        let start = Instant::now();
        let response = self.infer(uds_path, request, authorization).await?;
        let latency_ms = start.elapsed().as_millis() as u64;
        Ok((response, latency_ms))
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

    /// Fetch CoreML verification snapshot from worker debug endpoint.
    pub async fn coreml_verification_status(
        &self,
        uds_path: &Path,
    ) -> Result<WorkerCoremlVerification, UdsClientError> {
        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        let request = "GET /debug/coreml_verification HTTP/1.1\r\nHost: worker\r\n\r\n";
        tokio::time::timeout(self.timeout, stream.write_all(request.as_bytes()))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        let mut response_buffer = Vec::new();
        tokio::time::timeout(self.timeout, stream.read_to_end(&mut response_buffer))
            .await
            .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        let response_str = String::from_utf8_lossy(&response_buffer);
        let lines: Vec<&str> = response_str.lines().collect();
        if lines.is_empty() {
            return Err(UdsClientError::RequestFailed("Empty response".to_string()));
        }

        let status_line = lines[0];
        if !status_line.contains("200 OK") {
            return Err(UdsClientError::RequestFailed(format!(
                "Worker returned error: {}",
                status_line
            )));
        }

        let json_str = match response_str.find("\r\n\r\n") {
            Some(pos) => response_str.get(pos + 4..).unwrap_or(""),
            None => "",
        };

        serde_json::from_str::<WorkerCoremlVerification>(json_str)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))
    }

    /// Check if a worker is healthy via UDS with latency tracking
    ///
    /// Returns both the health status and the round-trip latency in milliseconds.
    /// This is useful for monitoring worker responsiveness and detecting degradation.
    pub async fn health_check_timed(&self, uds_path: &Path) -> Result<(bool, u64), UdsClientError> {
        let start = Instant::now();
        let is_healthy = self.health_check(uds_path).await?;
        let latency_ms = start.elapsed().as_millis() as u64;
        Ok((is_healthy, latency_ms))
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

        // Create HTTP request - send to /patch_proposal, not /inference
        let http_request = format!(
            "POST /patch_proposal HTTP/1.1\r\n\
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

        // Find JSON body (after double CRLF) - safe slicing to prevent panic
        let json_str = match response_str.find("\r\n\r\n") {
            Some(pos) => response_str.get(pos + 4..).unwrap_or_else(|| {
                warn!(
                    response_len = response_str.len(),
                    header_end_pos = pos,
                    "Malformed patch proposal response: body offset exceeds response length"
                );
                ""
            }),
            None => {
                warn!(
                    response_preview = %response_str.chars().take(100).collect::<String>(),
                    "Malformed patch proposal response: missing header/body separator (\\r\\n\\r\\n)"
                );
                ""
            }
        };

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
        // GUARD: Fail hard if not in routed context
        if !is_routed_context() {
            tracing::error!(
                kind = "ROUTING_BYPASS",
                "Inference call attempted without routing; this is a bug. Use InferenceCore::route_and_infer()"
            );
            return Err(UdsClientError::RoutingBypass(
                "Inference call attempted without routing. Use InferenceCore::route_and_infer()"
                    .into(),
            ));
        }

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

        // Read SSE stream with timeout protection
        let mut reader = BufReader::new(read_half);
        let mut response: Option<crate::types::WorkerInferResponse> = None;
        let mut line = String::new();

        // Overall SSE stream timeout (5 minutes max)
        let sse_timeout = Duration::from_secs(300);
        let per_line_timeout = Duration::from_secs(60);

        // Parse SSE headers with timeout
        let mut in_body = false;
        while !in_body {
            line.clear();
            match tokio::time::timeout(per_line_timeout, reader.read_line(&mut line)).await {
                Ok(Ok(_)) => {
                    if line.trim().is_empty() {
                        in_body = true;
                    }
                }
                Ok(Err(e)) => return Err(UdsClientError::RequestFailed(e.to_string())),
                Err(_) => {
                    return Err(UdsClientError::Timeout(
                        "SSE header read timeout".to_string(),
                    ))
                }
            }
        }

        // Process SSE events with overall timeout
        let mut event_type = String::new();
        let mut event_data = String::new();

        let sse_result = tokio::time::timeout(sse_timeout, async {
            loop {
                line.clear();
                let n = match tokio::time::timeout(per_line_timeout, reader.read_line(&mut line))
                    .await
                {
                    Ok(Ok(n)) => n,
                    Ok(Err(e)) => return Err(UdsClientError::RequestFailed(e.to_string())),
                    Err(_) => {
                        return Err(UdsClientError::Timeout("SSE line read timeout".to_string()))
                    }
                };

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
            Ok(())
        })
        .await;

        // Handle overall timeout
        match sse_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(UdsClientError::Timeout(
                    "SSE stream timeout (5 minutes)".to_string(),
                ))
            }
        }

        response.ok_or_else(|| UdsClientError::RequestFailed("No response received".to_string()))
    }

    pub async fn send_http_request(
        &self,
        uds_path: &Path,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, UdsClientError> {
        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        let request_json = if let Some(b) = body {
            serde_json::to_string(&b)
                .map_err(|e| UdsClientError::SerializationError(e.to_string()))?
        } else {
            "".to_string()
        };

        let http_request = if !request_json.is_empty() {
            format!(
                "{} {} HTTP/1.1\r\nHost: worker\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                method, path, request_json.len(), request_json
            )
        } else {
            format!("{} {} HTTP/1.1\r\nHost: worker\r\n\r\n", method, path)
        };

        tokio::time::timeout(self.timeout, stream.write_all(http_request.as_bytes()))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        let mut response_buffer = Vec::new();
        tokio::time::timeout(self.timeout, stream.read_to_end(&mut response_buffer))
            .await
            .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        let response_str = String::from_utf8_lossy(&response_buffer);
        let lines: Vec<&str> = response_str.lines().collect();

        if lines.is_empty() {
            return Err(UdsClientError::RequestFailed("Empty response".to_string()));
        }

        let status_line = lines[0];
        if !status_line.contains("200 OK") {
            return Err(UdsClientError::RequestFailed(format!(
                "Worker returned error: {}",
                status_line
            )));
        }

        // Safe slicing to prevent panic on malformed response
        let json_str = match response_str.find("\r\n\r\n") {
            Some(pos) => response_str.get(pos + 4..).unwrap_or_else(|| {
                warn!(
                    response_len = response_str.len(),
                    header_end_pos = pos,
                    "Malformed HTTP response in send_http_request: body offset exceeds response length"
                );
                ""
            }),
            None => {
                warn!(
                    response_preview = %response_str.chars().take(100).collect::<String>(),
                    "Malformed HTTP response in send_http_request: missing header/body separator (\\r\\n\\r\\n)"
                );
                ""
            }
        };

        serde_json::from_str(json_str)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))
    }

    /// Load a model via the worker UDS
    ///
    /// Sends a request to the worker to verify/load a model.
    /// Returns the model load status including memory usage.
    pub async fn load_model(
        &self,
        uds_path: &Path,
        model_id: &str,
        model_path: &str,
    ) -> Result<ModelLoadResponse, UdsClientError> {
        let request = serde_json::json!({
            "model_id": model_id,
            "model_path": model_path
        });

        let response = self
            .send_http_request(uds_path, "POST", "/model/load", Some(request))
            .await?;

        serde_json::from_value(response)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))
    }

    /// Get model status from worker via UDS
    ///
    /// Returns the current model status without triggering a load.
    pub async fn get_model_status(
        &self,
        uds_path: &Path,
    ) -> Result<serde_json::Value, UdsClientError> {
        self.send_http_request(uds_path, "GET", "/model/status", None)
            .await
    }

    /// Send an adapter command to worker via UDS
    pub async fn adapter_command(
        &self,
        uds_path: &Path,
        adapter_id: &str,
        command: &str,
    ) -> Result<(), UdsClientError> {
        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        let http_request = format!(
            "POST /adapter/{}/{} HTTP/1.1\r\nHost: worker\r\n\r\n",
            adapter_id, command
        );

        tokio::time::timeout(self.timeout, stream.write_all(http_request.as_bytes()))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        let mut response_buffer = Vec::new();
        tokio::time::timeout(self.timeout, stream.read_to_end(&mut response_buffer))
            .await
            .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        let response_str = String::from_utf8_lossy(&response_buffer);
        if !response_str.contains("200 OK") {
            return Err(UdsClientError::RequestFailed(format!(
                "Adapter command failed: {}",
                response_str.lines().next().unwrap_or("Unknown error")
            )));
        }

        Ok(())
    }

    /// Cancel a training job via UDS
    ///
    /// Sends a cancellation request to the worker and waits for confirmation.
    pub async fn cancel_training_job(
        &self,
        uds_path: &Path,
        job_id: &str,
        reason: Option<&str>,
    ) -> Result<CancelTrainingResponse, UdsClientError> {
        let request = serde_json::json!({
            "job_id": job_id,
            "reason": reason,
        });

        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        let http_request = format!(
            "POST /training/cancel HTTP/1.1\r\n\
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
        if !response_str.contains("200 OK") {
            return Err(UdsClientError::RequestFailed(format!(
                "Worker returned error: {}",
                response_str.lines().next().unwrap_or("Unknown error")
            )));
        }

        // Extract JSON body
        let json_str = match response_str.find("\r\n\r\n") {
            Some(pos) => response_str.get(pos + 4..).unwrap_or(""),
            None => "",
        };

        serde_json::from_str(json_str)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))
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

/// Response from model load operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelLoadResponse {
    /// Status of the operation: "loaded", "already_loaded", "error"
    pub status: String,
    /// Model ID that was loaded
    pub model_id: String,
    /// Estimated memory usage in MB
    pub memory_usage_mb: Option<i32>,
    /// Error message if status is "error"
    pub error: Option<String>,
    /// Timestamp of when model was loaded
    pub loaded_at: Option<String>,
}

/// Response from training job cancellation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CancelTrainingResponse {
    /// Job ID that was cancelled
    pub job_id: String,
    /// Current status: "cancelled", "stopping", "error"
    pub status: String,
    /// Number of tokens processed before cancellation
    pub tokens_processed: Option<u64>,
    /// Final loss value if available
    pub final_loss: Option<f32>,
    /// Epoch number where training was stopped
    pub stopped_at_epoch: Option<u32>,
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
        let request = crate::types::WorkerInferRequest {
            cpid: "cp-123".to_string(),
            prompt: "Hello worker".to_string(),
            max_tokens: 128,
            require_evidence: true,
            stack_id: Some("stack-42".to_string()),
            stack_version: Some(7),
            domain_hint: Some("aerospace".to_string()),
            temperature: 0.7,
            top_k: Some(5),
            top_p: Some(0.9),
            seed: Some(4242),
            router_seed: Some("router-seed".to_string()),
            seed_mode: None,
            request_seed: None,
            determinism: None,
            backend_profile: None,
            coreml_mode: None,
            determinism_mode: Some("strict".to_string()),
            routing_determinism_mode: Some(
                adapteros_types::adapters::metadata::RoutingDeterminismMode::Deterministic,
            ),
            pinned_adapter_ids: Some(vec!["adapter-a".to_string(), "adapter-b".to_string()]),
            strict_mode: Some(true),
            effective_adapter_ids: Some(vec!["eff-1".to_string(), "eff-2".to_string()]),
            routing_policy: None,
            placement: None,
            adapter_strength_overrides: Some(
                [("adapter-a".to_string(), 0.8_f32)].into_iter().collect(),
            ),
        };

        let serialized =
            serde_json::to_vec(&request).expect("WorkerInferRequest should serialize to JSON");
        let deserialized: crate::types::WorkerInferRequest =
            serde_json::from_slice(&serialized).expect("WorkerInferRequest should deserialize");

        assert_eq!(request.cpid, deserialized.cpid);
        assert_eq!(request.prompt, deserialized.prompt);
        assert_eq!(request.max_tokens, deserialized.max_tokens);
        assert_eq!(request.require_evidence, deserialized.require_evidence);
        assert_eq!(request.stack_id, deserialized.stack_id);
        assert_eq!(request.stack_version, deserialized.stack_version);
        assert_eq!(request.temperature, deserialized.temperature);
        assert_eq!(request.top_k, deserialized.top_k);
        assert_eq!(request.top_p, deserialized.top_p);
        assert_eq!(request.seed, deserialized.seed);
        assert_eq!(request.router_seed, deserialized.router_seed);
        assert_eq!(request.seed_mode, deserialized.seed_mode);
        assert_eq!(request.request_seed, deserialized.request_seed);
        assert_eq!(request.backend_profile, deserialized.backend_profile);
        assert_eq!(request.determinism_mode, deserialized.determinism_mode);
        assert_eq!(request.domain_hint, deserialized.domain_hint);
        assert_eq!(request.pinned_adapter_ids, deserialized.pinned_adapter_ids);
        assert_eq!(request.strict_mode, deserialized.strict_mode);
        assert_eq!(
            request.effective_adapter_ids,
            deserialized.effective_adapter_ids
        );
        assert!(deserialized.routing_policy.is_none());
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

    // ========================================================================
    // Safe String Slicing Tests
    // ========================================================================

    /// Helper function to extract JSON body from HTTP response (mirrors production logic)
    fn extract_json_body(response_str: &str) -> &str {
        match response_str.find("\r\n\r\n") {
            Some(pos) => response_str.get(pos + 4..).unwrap_or(""),
            None => "",
        }
    }

    #[test]
    fn test_safe_slicing_normal_response() {
        let response =
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"status\":\"ok\"}";
        let json = extract_json_body(response);
        assert_eq!(json, "{\"status\":\"ok\"}");
    }

    #[test]
    fn test_safe_slicing_empty_body() {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n";
        let json = extract_json_body(response);
        assert_eq!(json, "");
    }

    #[test]
    fn test_safe_slicing_missing_separator() {
        // No \r\n\r\n separator - should return empty string, not panic
        let response = "HTTP/1.1 200 OK\nContent-Type: application/json\n{\"status\":\"ok\"}";
        let json = extract_json_body(response);
        assert_eq!(json, "");
    }

    #[test]
    fn test_safe_slicing_truncated_response() {
        // Response ends right at separator - no body
        let response = "HTTP/1.1 200 OK\r\n\r\n";
        let json = extract_json_body(response);
        assert_eq!(json, "");
    }

    #[test]
    fn test_safe_slicing_only_headers() {
        // Headers only, no separator at all
        let response = "HTTP/1.1 200 OK";
        let json = extract_json_body(response);
        assert_eq!(json, "");
    }

    #[test]
    fn test_safe_slicing_empty_string() {
        let response = "";
        let json = extract_json_body(response);
        assert_eq!(json, "");
    }

    #[test]
    fn test_safe_slicing_unicode_body() {
        let response = "HTTP/1.1 200 OK\r\n\r\n{\"message\":\"你好世界\"}";
        let json = extract_json_body(response);
        assert_eq!(json, "{\"message\":\"你好世界\"}");
    }

    #[test]
    fn test_safe_slicing_multiline_body() {
        let response = "HTTP/1.1 200 OK\r\n\r\n{\n  \"status\": \"ok\",\n  \"data\": [1,2,3]\n}";
        let json = extract_json_body(response);
        assert_eq!(json, "{\n  \"status\": \"ok\",\n  \"data\": [1,2,3]\n}");
    }

    #[test]
    fn test_safe_slicing_partial_separator() {
        // Only \r\n once, not twice
        let response = "HTTP/1.1 200 OK\r\n{\"status\":\"ok\"}";
        let json = extract_json_body(response);
        assert_eq!(json, "");
    }

    #[test]
    fn test_safe_slicing_separator_at_end() {
        // Edge case: separator is the last 4 bytes
        let response = "X\r\n\r\n";
        let json = extract_json_body(response);
        assert_eq!(json, "");
    }

    #[test]
    fn test_safe_slicing_single_char_body() {
        let response = "HTTP/1.1 200 OK\r\n\r\nX";
        let json = extract_json_body(response);
        assert_eq!(json, "X");
    }

    // ========================================================================
    // Model Loading Tests
    // ========================================================================

    #[test]
    fn test_model_load_response_serialization() {
        let response = ModelLoadResponse {
            status: "loaded".to_string(),
            model_id: "test-model-123".to_string(),
            memory_usage_mb: Some(4096),
            error: None,
            loaded_at: Some("2025-12-01T00:00:00Z".to_string()),
        };

        let serialized = serde_json::to_string(&response).expect("Should serialize");
        let deserialized: ModelLoadResponse =
            serde_json::from_str(&serialized).expect("Should deserialize");

        assert_eq!(deserialized.status, "loaded");
        assert_eq!(deserialized.model_id, "test-model-123");
        assert_eq!(deserialized.memory_usage_mb, Some(4096));
        assert!(deserialized.error.is_none());
        assert!(deserialized.loaded_at.is_some());
    }

    #[test]
    fn test_model_load_response_with_error() {
        let response = ModelLoadResponse {
            status: "error".to_string(),
            model_id: "test-model".to_string(),
            memory_usage_mb: None,
            error: Some("Model path does not exist".to_string()),
            loaded_at: None,
        };

        let serialized = serde_json::to_string(&response).expect("Should serialize");
        let deserialized: ModelLoadResponse =
            serde_json::from_str(&serialized).expect("Should deserialize");

        assert_eq!(deserialized.status, "error");
        assert!(deserialized.memory_usage_mb.is_none());
        assert_eq!(
            deserialized.error,
            Some("Model path does not exist".to_string())
        );
    }

    #[tokio::test]
    async fn test_load_model_request_format() {
        // Test that the request JSON is properly formatted
        let request = serde_json::json!({
            "model_id": "test-model",
            "model_path": "/path/to/model"
        });

        // Verify it can be serialized/deserialized
        let serialized = serde_json::to_string(&request).expect("Should serialize");
        assert!(serialized.contains("model_id"));
        assert!(serialized.contains("model_path"));
    }
}
