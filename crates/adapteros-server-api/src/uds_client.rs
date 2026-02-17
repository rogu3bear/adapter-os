//! Unix Domain Socket client for communicating with workers
//!
//! This module provides functionality to connect to worker UDS servers
//! and forward inference requests from the control plane.
//!
//! **Signal Protocol Support**: Extended to support receiving signals from
//! workers during inference via Server-Sent Events (SSE).
//!
//! Citation: docs/llm-interface-specification.md §5.1

use adapteros_core::{CircuitBreaker, CircuitBreakerConfig, StandardCircuitBreaker};
use serde::Deserialize;
use serde_json;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::warn;

// ============================================================================
// UDS Phase Timings
// ============================================================================

/// Timing breakdown for UDS inference operations.
///
/// Captures the duration of each phase of the UDS communication:
/// - connect: Time to establish the Unix socket connection
/// - write: Time to send the request over the socket
/// - read: Time to receive and read the response (includes inference time)
#[derive(Debug, Clone, Default)]
pub struct UdsPhaseTimings {
    /// Connect phase duration in seconds
    pub connect_secs: f64,
    /// Write phase duration in seconds
    pub write_secs: f64,
    /// Read phase duration in seconds (includes worker inference time)
    pub read_secs: f64,
}

impl UdsPhaseTimings {
    /// Total round-trip time in seconds
    pub fn total_secs(&self) -> f64 {
        self.connect_secs + self.write_secs + self.read_secs
    }

    /// Total round-trip time in milliseconds
    pub fn total_ms(&self) -> u64 {
        (self.total_secs() * 1000.0) as u64
    }
}

// ============================================================================
// Routing Guard - Ensures all inference goes through the router
// ============================================================================

// Use task-local storage instead of thread-local to survive async task migrations
tokio::task_local! {
    static ROUTING_GUARD: std::cell::Cell<bool>;
}

/// Mark that we're in a routed context (called by InferenceCore)
pub fn enter_routed_context() {
    // For task-local, we set via scope() in run_with_routing_context
    // This is kept for backward compatibility but is a no-op
    let _ = ROUTING_GUARD.try_with(|g| g.set(true));
}

/// Clear routed context after request completes
pub fn exit_routed_context() {
    // For task-local, context is cleared when scope exits
    // This is kept for backward compatibility but is a no-op
    let _ = ROUTING_GUARD.try_with(|g| g.set(false));
}

/// Check if currently in routed context
pub fn is_routed_context() -> bool {
    ROUTING_GUARD.try_with(|g| g.get()).unwrap_or(false)
}

/// Run a future within a routed context. This ensures the routing guard
/// is properly propagated across async task migrations.
pub async fn run_with_routing_context<F, T>(f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    ROUTING_GUARD.scope(std::cell::Cell::new(true), f).await
}

/// Send an inference request through the routing guard with a one-off client.
pub async fn infer_with_routing_context(
    uds_path: &Path,
    request: crate::types::WorkerInferRequest,
    authorization: Option<&str>,
    timeout: Duration,
    cancellation_token: Option<CancellationToken>,
) -> Result<crate::types::WorkerInferResponse, UdsClientError> {
    let client = UdsClient::new(timeout);
    run_with_routing_context(client.infer(uds_path, request, authorization, cancellation_token))
        .await
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
    #[error("Request cancelled: {0}")]
    Cancelled(String),
    /// Worker is at capacity but healthy - caller should retry with a different worker
    #[error("Worker overloaded (retry after {retry_after_ms}ms)")]
    WorkerOverloaded {
        retry_after_ms: u64,
        message: String,
    },
    /// Model cache budget exceeded in worker
    #[error("Model cache budget exceeded: needed {needed_mb} MB, freed {freed_mb} MB (pinned={pinned_count}, active={active_count}), max {max_mb} MB")]
    CacheBudgetExceeded {
        needed_mb: u64,
        freed_mb: u64,
        pinned_count: usize,
        active_count: usize,
        max_mb: u64,
        model_key: Option<String>,
    },
}

impl UdsClientError {
    /// Returns true if this error indicates the worker is healthy but at capacity
    pub fn is_backpressure(&self) -> bool {
        matches!(self, UdsClientError::WorkerOverloaded { .. })
    }
}

/// Circuit breaker configuration for UDS client
/// - failure_threshold: 3 consecutive failures to open circuit
/// - success_threshold: 2 consecutive successes to close circuit
/// - timeout_ms: 5000ms before transitioning to half-open
fn default_uds_circuit_breaker_config() -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout_ms: 5000,
        half_open_max_requests: 5,
    }
}

/// UDS client for communicating with workers
pub struct UdsClient {
    timeout: Duration,
    /// Circuit breaker to protect against cascading failures
    circuit_breaker: Arc<StandardCircuitBreaker>,
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

/// Token payload for streaming inference over UDS.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerStreamToken {
    pub text: String,
    #[serde(default)]
    pub token_id: Option<u32>,
}

/// Paused event payload for human-in-the-loop review.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerStreamPaused {
    pub pause_id: String,
    pub inference_id: String,
    pub trigger_kind: String,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub text_so_far: Option<String>,
    pub token_count: usize,
}

/// Streaming events emitted by the worker over UDS.
#[derive(Debug)]
pub enum WorkerStreamEvent {
    Token(WorkerStreamToken),
    Complete(Box<crate::types::WorkerInferResponse>),
    Error(String),
    /// Inference paused for human review
    Paused(WorkerStreamPaused),
}

impl UdsClient {
    /// Create a new UDS client with default circuit breaker configuration
    pub fn new(timeout: Duration) -> Self {
        Self::with_circuit_breaker(
            timeout,
            Arc::new(StandardCircuitBreaker::new(
                "uds_client".to_string(),
                default_uds_circuit_breaker_config(),
            )),
        )
    }

    /// Create a new UDS client with a custom circuit breaker
    pub fn with_circuit_breaker(
        timeout: Duration,
        circuit_breaker: Arc<StandardCircuitBreaker>,
    ) -> Self {
        Self {
            timeout,
            circuit_breaker,
        }
    }

    /// Get a reference to the circuit breaker for monitoring
    pub fn circuit_breaker(&self) -> &Arc<StandardCircuitBreaker> {
        &self.circuit_breaker
    }

    /// Send an inference request to a worker via UDS
    ///
    /// This method is protected by a circuit breaker that will fail fast when
    /// the worker is experiencing repeated failures. The circuit breaker opens
    /// after 3 consecutive failures and re-closes after 2 consecutive successes
    /// following a 5 second timeout period.
    pub async fn infer(
        &self,
        uds_path: &Path,
        request: crate::types::WorkerInferRequest,
        authorization: Option<&str>,
        cancellation_token: Option<CancellationToken>,
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

        // Check circuit breaker state before attempting the operation
        let cb_state = self.circuit_breaker.state();
        if matches!(cb_state, adapteros_core::CircuitState::Open { .. }) {
            return Err(UdsClientError::WorkerNotAvailable(
                "Circuit breaker is open - worker is experiencing failures".to_string(),
            ));
        }

        // Execute the inference operation through the circuit breaker
        let result = self
            .infer_inner(uds_path, request, authorization, cancellation_token)
            .await;

        // Record the result with the circuit breaker
        match &result {
            Ok(_) => {
                // Record success with circuit breaker
                let _ = self.circuit_breaker.call(async { Ok::<(), _>(()) }).await;
            }
            Err(e) => {
                // Only count certain errors as failures for circuit breaker purposes
                // Connection failures and timeouts indicate worker issues
                // Routing bypass, cancellation, and serialization errors are not worker issues
                let is_worker_failure = matches!(
                    e,
                    UdsClientError::ConnectionFailed(_)
                        | UdsClientError::Timeout(_)
                        | UdsClientError::RequestFailed(_)
                        | UdsClientError::WorkerNotAvailable(_)
                );
                if is_worker_failure {
                    // Record failure with circuit breaker by calling with a failing future
                    let _ = self
                        .circuit_breaker
                        .call(async {
                            Err::<(), _>(adapteros_core::AosError::Worker(e.to_string()))
                        })
                        .await;
                }
            }
        }

        result
    }

    fn format_auth_header(authorization: Option<&str>) -> String {
        let Some(token) = authorization else {
            return String::new();
        };

        let token = token.trim();
        if token.starts_with("Bearer ") || token.starts_with("ApiKey ") {
            format!("Authorization: {}\r\n", token)
        } else {
            format!("Authorization: Bearer {}\r\n", token)
        }
    }

    fn spawn_cancel_request(
        uds_path: &Path,
        request_id: Option<&str>,
        cpid: Option<&str>,
        authorization: Option<&str>,
        reason: &str,
    ) {
        let Some(request_id) = request_id else {
            return;
        };

        let uds_path = uds_path.to_path_buf();
        let request_id = request_id.to_string();
        let cpid = cpid.map(|value| value.to_string());
        let authorization = authorization.map(|token| token.to_string());
        let reason = reason.to_string();

        tokio::spawn(async move {
            let client = UdsClient::new(Duration::from_secs(2));
            let _ = client
                .cancel_inference(
                    &uds_path,
                    &request_id,
                    cpid.as_deref(),
                    authorization.as_deref(),
                    Some(&reason),
                )
                .await;
        });
    }

    /// Internal implementation of infer without circuit breaker wrapping
    async fn infer_inner(
        &self,
        uds_path: &Path,
        request: crate::types::WorkerInferRequest,
        authorization: Option<&str>,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<crate::types::WorkerInferResponse, UdsClientError> {
        let cancel = cancellation_token.as_ref();
        let request_id = request.request_id.clone();
        let request_cpid = request.cpid.clone();

        // Connect to UDS
        let connect = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path));
        let stream_result = if let Some(token) = cancel {
            tokio::select! {
                _ = token.cancelled() => {
                    Self::spawn_cancel_request(
                        uds_path,
                        request_id.as_deref(),
                        Some(request_cpid.as_str()),
                        authorization,
                        "Cancelled before connecting to worker",
                    );
                    return Err(UdsClientError::Cancelled(
                        "Cancelled before connecting to worker".to_string()
                    ));
                }
                res = connect => res,
            }
        } else {
            connect.await
        };
        let mut stream = stream_result
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        // Serialize the request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        // Worker authentication: Use Bearer format for Ed25519-signed JWTs
        // The worker validates these tokens using the control plane's public key
        let auth_header = Self::format_auth_header(authorization);

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
        let write_future =
            tokio::time::timeout(self.timeout, stream.write_all(http_request.as_bytes()));
        if let Some(token) = cancel {
            tokio::select! {
                _ = token.cancelled() => {
                    Self::spawn_cancel_request(
                        uds_path,
                        request_id.as_deref(),
                        Some(request_cpid.as_str()),
                        authorization,
                        "Cancelled while sending request to worker",
                    );
                    return Err(UdsClientError::Cancelled(
                        "Cancelled while sending request to worker".to_string()
                    ));
                }
                result = write_future => {
                    result
                        .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
                        .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?
                }
            }
        } else {
            write_future
                .await
                .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
        }

        // Read response
        let mut response_buffer = Vec::new();
        let read_future =
            tokio::time::timeout(self.timeout, stream.read_to_end(&mut response_buffer));
        if let Some(token) = cancel {
            tokio::select! {
                _ = token.cancelled() => {
                    Self::spawn_cancel_request(
                        uds_path,
                        request_id.as_deref(),
                        Some(request_cpid.as_str()),
                        authorization,
                        "Cancelled while waiting for worker response",
                    );
                    return Err(UdsClientError::Cancelled(
                        "Cancelled while waiting for worker response".to_string()
                    ));
                }
                result = read_future => {
                    let _ = result
                        .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
                        .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
                }
            }
        } else {
            let _ = read_future
                .await
                .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
        }

        // Parse HTTP response
        let response_str = String::from_utf8_lossy(&response_buffer);
        let lines: Vec<&str> = response_str.lines().collect();

        if lines.is_empty() {
            return Err(UdsClientError::RequestFailed("Empty response".to_string()));
        }

        // Check status line
        let status_line = lines[0];
        if !status_line.contains("200 OK") {
            // Check for 503 backpressure response (worker healthy but at capacity)
            if status_line.contains("503") {
                // Try to parse the JSON body for retry_after_ms
                let bp_json_str = response_str
                    .find("\r\n\r\n")
                    .and_then(|pos| response_str.get(pos + 4..))
                    .unwrap_or("{}");

                #[derive(Deserialize)]
                struct OverloadResponse {
                    #[serde(default)]
                    retry_after_ms: u64,
                    #[serde(default)]
                    message: String,
                }

                let overload: OverloadResponse =
                    serde_json::from_str(bp_json_str).unwrap_or(OverloadResponse {
                        retry_after_ms: 100,
                        message: "Worker overloaded".to_string(),
                    });

                return Err(UdsClientError::WorkerOverloaded {
                    retry_after_ms: overload.retry_after_ms,
                    message: overload.message,
                });
            }

            // Try to parse the JSON body for structured error details
            let error_json_str = response_str
                .find("\r\n\r\n")
                .and_then(|pos| response_str.get(pos + 4..))
                .unwrap_or("{}");

            // Check for cache budget exceeded error by parsing error body
            #[derive(Deserialize)]
            struct WorkerErrorResponse {
                #[serde(default)]
                error: String,
            }

            if let Ok(err_response) = serde_json::from_str::<WorkerErrorResponse>(error_json_str) {
                // Check if error message indicates cache budget exceeded
                if err_response.error.contains("cache budget exceeded")
                    || err_response.error.contains("Model cache budget exceeded")
                {
                    // Try to parse structured fields from the error message
                    if let Some((needed_mb, freed_mb, pinned_count, active_count, max_mb)) =
                        parse_cache_budget_error(&err_response.error)
                    {
                        return Err(UdsClientError::CacheBudgetExceeded {
                            needed_mb,
                            freed_mb,
                            pinned_count,
                            active_count,
                            max_mb,
                            model_key: None,
                        });
                    }
                }
            }

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

    /// Internal implementation of infer that captures phase timings.
    ///
    /// This method wraps the I/O operations with timing to produce
    /// `UdsPhaseTimings` for observability and debugging.
    async fn infer_inner_with_timings(
        &self,
        uds_path: &Path,
        request: crate::types::WorkerInferRequest,
        authorization: Option<&str>,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<(crate::types::WorkerInferResponse, UdsPhaseTimings), UdsClientError> {
        let cancel = cancellation_token.as_ref();
        let request_id = request.request_id.clone();
        let request_cpid = request.cpid.clone();

        let mut timings = UdsPhaseTimings::default();

        // ---- Connect phase ----
        let connect_start = Instant::now();
        let connect = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path));
        let stream_result = if let Some(token) = cancel {
            tokio::select! {
                _ = token.cancelled() => {
                    Self::spawn_cancel_request(
                        uds_path,
                        request_id.as_deref(),
                        Some(request_cpid.as_str()),
                        authorization,
                        "Cancelled before connecting to worker",
                    );
                    return Err(UdsClientError::Cancelled(
                        "Cancelled before connecting to worker".to_string()
                    ));
                }
                res = connect => res,
            }
        } else {
            connect.await
        };
        let mut stream = stream_result
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;
        timings.connect_secs = connect_start.elapsed().as_secs_f64();

        // Serialize the request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        // Worker authentication: Use Bearer format for Ed25519-signed JWTs
        // The worker validates these tokens using the control plane's public key
        let auth_header = Self::format_auth_header(authorization);

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

        // ---- Write phase ----
        let write_start = Instant::now();
        let write_future =
            tokio::time::timeout(self.timeout, stream.write_all(http_request.as_bytes()));
        if let Some(token) = cancel {
            tokio::select! {
                _ = token.cancelled() => {
                    Self::spawn_cancel_request(
                        uds_path,
                        request_id.as_deref(),
                        Some(request_cpid.as_str()),
                        authorization,
                        "Cancelled while sending request to worker",
                    );
                    return Err(UdsClientError::Cancelled(
                        "Cancelled while sending request to worker".to_string()
                    ));
                }
                result = write_future => {
                    result
                        .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
                        .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?
                }
            }
        } else {
            write_future
                .await
                .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
        }
        timings.write_secs = write_start.elapsed().as_secs_f64();

        // ---- Read phase ----
        let read_start = Instant::now();
        let mut response_buffer = Vec::new();
        let read_future =
            tokio::time::timeout(self.timeout, stream.read_to_end(&mut response_buffer));
        if let Some(token) = cancel {
            tokio::select! {
                _ = token.cancelled() => {
                    Self::spawn_cancel_request(
                        uds_path,
                        request_id.as_deref(),
                        Some(request_cpid.as_str()),
                        authorization,
                        "Cancelled while waiting for worker response",
                    );
                    return Err(UdsClientError::Cancelled(
                        "Cancelled while waiting for worker response".to_string()
                    ));
                }
                result = read_future => {
                    let _ = result
                        .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
                        .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
                }
            }
        } else {
            let _ = read_future
                .await
                .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
        }
        timings.read_secs = read_start.elapsed().as_secs_f64();

        // Parse HTTP response
        let response_str = String::from_utf8_lossy(&response_buffer);
        let lines: Vec<&str> = response_str.lines().collect();

        if lines.is_empty() {
            return Err(UdsClientError::RequestFailed("Empty response".to_string()));
        }

        // Check status line
        let status_line = lines[0];
        if !status_line.contains("200 OK") {
            // Check for 503 backpressure response (worker healthy but at capacity)
            if status_line.contains("503") {
                // Try to parse the JSON body for retry_after_ms
                let bp_json_str = response_str
                    .find("\r\n\r\n")
                    .and_then(|pos| response_str.get(pos + 4..))
                    .unwrap_or("{}");

                #[derive(Deserialize)]
                struct OverloadResponse {
                    #[serde(default)]
                    retry_after_ms: u64,
                    #[serde(default)]
                    message: String,
                }

                let overload: OverloadResponse =
                    serde_json::from_str(bp_json_str).unwrap_or(OverloadResponse {
                        retry_after_ms: 100,
                        message: "Worker overloaded".to_string(),
                    });

                return Err(UdsClientError::WorkerOverloaded {
                    retry_after_ms: overload.retry_after_ms,
                    message: overload.message,
                });
            }

            // Try to parse the JSON body for structured error details
            let error_json_str = response_str
                .find("\r\n\r\n")
                .and_then(|pos| response_str.get(pos + 4..))
                .unwrap_or("{}");

            // Check for cache budget exceeded error by parsing error body
            #[derive(Deserialize)]
            struct WorkerErrorResponse {
                #[serde(default)]
                error: String,
            }

            if let Ok(err_response) = serde_json::from_str::<WorkerErrorResponse>(error_json_str) {
                // Check if error message indicates cache budget exceeded
                if err_response.error.contains("cache budget exceeded")
                    || err_response.error.contains("Model cache budget exceeded")
                {
                    // Try to parse structured fields from the error message
                    if let Some((needed_mb, freed_mb, pinned_count, active_count, max_mb)) =
                        parse_cache_budget_error(&err_response.error)
                    {
                        return Err(UdsClientError::CacheBudgetExceeded {
                            needed_mb,
                            freed_mb,
                            pinned_count,
                            active_count,
                            max_mb,
                            model_key: None,
                        });
                    }
                }
            }

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

        Ok((response, timings))
    }

    /// Send an inference request and stream tokens via SSE.
    pub async fn infer_stream(
        &self,
        uds_path: &Path,
        request: crate::types::WorkerInferRequest,
        authorization: Option<&str>,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<ReceiverStream<Result<WorkerStreamEvent, UdsClientError>>, UdsClientError> {
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

        // Check circuit breaker state before attempting the operation
        let cb_state = self.circuit_breaker.state();
        if matches!(cb_state, adapteros_core::CircuitState::Open { .. }) {
            return Err(UdsClientError::WorkerNotAvailable(
                "Circuit breaker is open - worker is experiencing failures".to_string(),
            ));
        }

        let cancel = cancellation_token.clone();
        let request_id = request.request_id.clone();
        let request_cpid = request.cpid.clone();
        let cancel_authorization = authorization.map(|token| token.to_string());

        // Connect to UDS
        let connect = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path));
        let stream_result = if let Some(token) = cancel.as_ref() {
            tokio::select! {
                _ = token.cancelled() => {
                    Self::spawn_cancel_request(
                        uds_path,
                        request_id.as_deref(),
                        Some(request_cpid.as_str()),
                        authorization,
                        "Cancelled before connecting to worker",
                    );
                    return Err(UdsClientError::Cancelled(
                        "Cancelled before connecting to worker".to_string()
                    ));
                }
                res = connect => res,
            }
        } else {
            connect.await
        };

        let stream = stream_result
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        let (read_half, mut write_half) = stream.into_split();

        // Serialize the request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        // Worker authentication: Use Bearer format for Ed25519-signed JWTs
        let auth_header = Self::format_auth_header(authorization);

        // Create HTTP request with SSE headers
        let http_request = format!(
            "POST /inference HTTP/1.1\r\n\
             Host: worker\r\n\
             Content-Type: application/json\r\n\
             {}\
             Accept: text/event-stream\r\n\
             X-Stream: true\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            auth_header,
            request_json.len(),
            request_json
        );

        // Send request
        let write_future =
            tokio::time::timeout(self.timeout, write_half.write_all(http_request.as_bytes()));
        if let Some(token) = cancel.as_ref() {
            tokio::select! {
                _ = token.cancelled() => {
                    Self::spawn_cancel_request(
                        uds_path,
                        request_id.as_deref(),
                        Some(request_cpid.as_str()),
                        authorization,
                        "Cancelled while sending request to worker",
                    );
                    return Err(UdsClientError::Cancelled(
                        "Cancelled while sending request to worker".to_string()
                    ));
                }
                result = write_future => {
                    result
                        .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
                        .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?
                }
            }
        } else {
            write_future
                .await
                .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
        }

        let (tx, rx) = mpsc::channel::<Result<WorkerStreamEvent, UdsClientError>>(64);
        let timeout = self.timeout;
        let uds_path = uds_path.to_path_buf();
        let cancel_request_id = request_id.clone();
        let cancel_cpid = request_cpid.clone();
        let cancel_authorization = cancel_authorization.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();

            // Read status line
            line.clear();
            let status = match tokio::time::timeout(timeout, reader.read_line(&mut line)).await {
                Ok(Ok(0)) => {
                    let _ = tx
                        .send(Err(UdsClientError::RequestFailed(
                            "Empty response".to_string(),
                        )))
                        .await;
                    return;
                }
                Ok(Ok(_)) => line.clone(),
                Ok(Err(e)) => {
                    let _ = tx
                        .send(Err(UdsClientError::RequestFailed(e.to_string())))
                        .await;
                    return;
                }
                Err(_) => {
                    let _ = tx
                        .send(Err(UdsClientError::Timeout("Read timed out".to_string())))
                        .await;
                    return;
                }
            };

            if !status.contains("200 OK") {
                let _ = tx
                    .send(Err(UdsClientError::RequestFailed(format!(
                        "Worker returned error: {}",
                        status.trim()
                    ))))
                    .await;
                return;
            }

            // Skip headers
            loop {
                line.clear();
                match tokio::time::timeout(timeout, reader.read_line(&mut line)).await {
                    Ok(Ok(0)) => break,
                    Ok(Ok(_)) => {
                        if line.trim().is_empty() {
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        let _ = tx
                            .send(Err(UdsClientError::RequestFailed(e.to_string())))
                            .await;
                        return;
                    }
                    Err(_) => {
                        let _ = tx
                            .send(Err(UdsClientError::Timeout(
                                "Header read timed out".to_string(),
                            )))
                            .await;
                        return;
                    }
                }
            }

            let mut event_type = String::new();
            let mut event_data = String::new();

            loop {
                line.clear();
                let read_line = tokio::time::timeout(timeout, reader.read_line(&mut line));
                let read_result = if let Some(token) = cancel.as_ref() {
                    tokio::select! {
                        _ = token.cancelled() => {
                            UdsClient::spawn_cancel_request(
                                &uds_path,
                                cancel_request_id.as_deref(),
                                Some(cancel_cpid.as_str()),
                                cancel_authorization.as_deref(),
                                "Cancelled while streaming",
                            );
                            let _ = tx
                                .send(Err(UdsClientError::Cancelled(
                                    "Cancelled while streaming".to_string()
                                )))
                                .await;
                            return;
                        }
                        res = read_line => res,
                    }
                } else {
                    read_line.await
                };

                let n = match read_result {
                    Ok(Ok(n)) => n,
                    Ok(Err(e)) => {
                        let _ = tx
                            .send(Err(UdsClientError::RequestFailed(e.to_string())))
                            .await;
                        return;
                    }
                    Err(_) => {
                        let _ = tx
                            .send(Err(UdsClientError::Timeout(
                                "SSE line read timed out".to_string(),
                            )))
                            .await;
                        return;
                    }
                };

                if n == 0 {
                    break;
                }

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    if event_type.is_empty() {
                        event_data.clear();
                        continue;
                    }

                    let result = match event_type.as_str() {
                        "token" => serde_json::from_str::<WorkerStreamToken>(&event_data)
                            .map(WorkerStreamEvent::Token)
                            .map_err(|e| UdsClientError::SerializationError(e.to_string())),
                        "complete" => {
                            serde_json::from_str::<crate::types::WorkerInferResponse>(&event_data)
                                .map(|v| WorkerStreamEvent::Complete(Box::new(v)))
                                .map_err(|e| UdsClientError::SerializationError(e.to_string()))
                        }
                        "error" => {
                            let message = serde_json::from_str::<serde_json::Value>(&event_data)
                                .ok()
                                .and_then(|value| {
                                    value
                                        .get("error")
                                        .and_then(|v| v.as_str())
                                        .map(|v| v.to_string())
                                })
                                .unwrap_or_else(|| event_data.clone());
                            Ok(WorkerStreamEvent::Error(message))
                        }
                        "paused" => serde_json::from_str::<WorkerStreamPaused>(&event_data)
                            .map(WorkerStreamEvent::Paused)
                            .map_err(|e| UdsClientError::SerializationError(e.to_string())),
                        _ => Ok(WorkerStreamEvent::Error(format!(
                            "Unknown event type: {}",
                            event_type
                        ))),
                    };

                    if tx.send(result).await.is_err() {
                        return;
                    }

                    event_type.clear();
                    event_data.clear();
                } else if let Some(stripped) = trimmed.strip_prefix("event:") {
                    event_type = stripped.trim().to_string();
                } else if let Some(stripped) = trimmed.strip_prefix("data:") {
                    if !event_data.is_empty() {
                        event_data.push('\n');
                    }
                    event_data.push_str(stripped.trim());
                }
            }
        });

        Ok(ReceiverStream::new(rx))
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
        cancellation_token: Option<CancellationToken>,
    ) -> Result<(crate::types::WorkerInferResponse, u64), UdsClientError> {
        let start = Instant::now();
        let response = self
            .infer(uds_path, request, authorization, cancellation_token)
            .await?;
        let latency_ms = start.elapsed().as_millis() as u64;
        Ok((response, latency_ms))
    }

    /// Send an inference request to a worker via UDS with detailed phase timings.
    ///
    /// Returns the response along with `UdsPhaseTimings` which captures the
    /// duration of each I/O phase (connect, write, read). This is useful for
    /// diagnosing performance issues in the UDS communication path.
    ///
    /// This method is protected by:
    /// - Routing guard (must be called via InferenceCore)
    /// - Circuit breaker (fails fast when worker is unhealthy)
    pub async fn infer_with_phase_timings(
        &self,
        uds_path: &Path,
        request: crate::types::WorkerInferRequest,
        authorization: Option<&str>,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<(crate::types::WorkerInferResponse, UdsPhaseTimings), UdsClientError> {
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

        // Check circuit breaker state before attempting the operation
        let cb_state = self.circuit_breaker.state();
        if matches!(cb_state, adapteros_core::CircuitState::Open { .. }) {
            return Err(UdsClientError::WorkerNotAvailable(
                "Circuit breaker is open - worker is experiencing failures".to_string(),
            ));
        }

        // Execute the inference operation through the circuit breaker
        let result = self
            .infer_inner_with_timings(uds_path, request, authorization, cancellation_token)
            .await;

        // Record the result with the circuit breaker
        match &result {
            Ok(_) => {
                // Record success with circuit breaker
                let _ = self.circuit_breaker.call(async { Ok::<(), _>(()) }).await;
            }
            Err(e) => {
                // Only count certain errors as failures for circuit breaker purposes
                // Connection failures and timeouts indicate worker issues
                // Routing bypass, cancellation, and serialization errors are not worker issues
                let is_worker_failure = matches!(
                    e,
                    UdsClientError::ConnectionFailed(_)
                        | UdsClientError::Timeout(_)
                        | UdsClientError::RequestFailed(_)
                        | UdsClientError::WorkerNotAvailable(_)
                );
                if is_worker_failure {
                    // Record failure with circuit breaker by calling with a failing future
                    let _ = self
                        .circuit_breaker
                        .call(async {
                            Err::<(), _>(adapteros_core::AosError::Worker(e.to_string()))
                        })
                        .await;
                }
            }
        }

        result
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
            // Check for 503 backpressure response (worker healthy but at capacity)
            if status_line.contains("503") {
                let bp_json_str = response_str
                    .find("\r\n\r\n")
                    .and_then(|pos| response_str.get(pos + 4..))
                    .unwrap_or("{}");

                #[derive(Deserialize)]
                struct OverloadResponse {
                    #[serde(default)]
                    retry_after_ms: u64,
                    #[serde(default)]
                    message: String,
                }

                let overload: OverloadResponse =
                    serde_json::from_str(bp_json_str).unwrap_or(OverloadResponse {
                        retry_after_ms: 100,
                        message: "Worker overloaded".to_string(),
                    });

                return Err(UdsClientError::WorkerOverloaded {
                    retry_after_ms: overload.retry_after_ms,
                    message: overload.message,
                });
            }

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
    fn parse_sse_error_message(event_data: &str) -> String {
        serde_json::from_str::<serde_json::Value>(event_data)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string())
            })
            .unwrap_or_else(|| event_data.to_string())
    }

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
             Accept: text/event-stream\r\n\
             X-Stream: true\r\n\
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

        // Parse HTTP status line with timeout
        line.clear();
        let status_line =
            match tokio::time::timeout(per_line_timeout, reader.read_line(&mut line)).await {
                Ok(Ok(0)) => {
                    return Err(UdsClientError::RequestFailed("Empty response".to_string()));
                }
                Ok(Ok(_)) => line.clone(),
                Ok(Err(e)) => return Err(UdsClientError::RequestFailed(e.to_string())),
                Err(_) => {
                    return Err(UdsClientError::Timeout(
                        "SSE status line read timeout".to_string(),
                    ))
                }
            };

        if !status_line.contains("200 OK") {
            return Err(UdsClientError::RequestFailed(format!(
                "Worker returned error: {}",
                status_line.trim()
            )));
        }

        // Parse SSE headers with timeout
        loop {
            line.clear();
            match tokio::time::timeout(per_line_timeout, reader.read_line(&mut line)).await {
                Ok(Ok(_)) => {
                    if line.trim().is_empty() {
                        break;
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

        let mut process_event = |event_type: &str,
                                 event_data: &str|
         -> Result<bool, UdsClientError> {
            match event_type {
                "signal" => {
                    let signal: Signal = serde_json::from_str(event_data)
                        .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;
                    signal_callback(signal);
                    Ok(false)
                }
                "complete" => {
                    let resp: crate::types::WorkerInferResponse = serde_json::from_str(event_data)
                        .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;
                    response = Some(resp);
                    Ok(true)
                }
                "error" => Err(UdsClientError::RequestFailed(
                    Self::parse_sse_error_message(event_data),
                )),
                _ => Ok(false),
            }
        };

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
                    if !event_type.is_empty() && !event_data.is_empty() {
                        process_event(&event_type, &event_data)?;
                    }
                    break; // End of stream
                }

                let line_trimmed = line.trim();

                if line_trimmed.is_empty() {
                    // Event boundary - process accumulated event
                    if !event_type.is_empty()
                        && !event_data.is_empty()
                        && process_event(&event_type, &event_data)?
                    {
                        break;
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

    /// Cancel an active inference request via UDS
    ///
    /// Sends a cancellation request to the worker for the specified request_id.
    /// The worker should abort the inference loop and return early.
    pub async fn cancel_inference(
        &self,
        uds_path: &Path,
        request_id: &str,
        cpid: Option<&str>,
        authorization: Option<&str>,
        reason: Option<&str>,
    ) -> Result<(), UdsClientError> {
        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        let request_json = if reason.is_some() || cpid.is_some() {
            serde_json::to_string(&serde_json::json!({
                "reason": reason,
                "cpid": cpid,
            }))
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?
        } else {
            String::new()
        };

        let auth_header = Self::format_auth_header(authorization);

        let http_request = if request_json.is_empty() {
            format!(
                "POST /inference/cancel/{} HTTP/1.1\r\nHost: worker\r\n{}\r\n",
                request_id, auth_header
            )
        } else {
            format!(
                "POST /inference/cancel/{} HTTP/1.1\r\n\
                 Host: worker\r\n\
                 Content-Type: application/json\r\n\
                 {}\
                 Content-Length: {}\r\n\
                 \r\n\
                 {}",
                request_id,
                auth_header,
                request_json.len(),
                request_json
            )
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
        if !response_str.contains("200 OK") {
            return Err(UdsClientError::RequestFailed(format!(
                "Inference cancellation failed: {}",
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

    /// Send a maintenance mode signal to a worker via UDS
    ///
    /// This signals the worker to enter maintenance/drain mode. The worker will:
    /// 1. Stop accepting new requests
    /// 2. Complete any in-flight requests
    /// 3. Gracefully shut down
    ///
    /// # Arguments
    /// * `uds_path` - Path to the worker's Unix domain socket
    /// * `mode` - Maintenance mode: "drain" (graceful) or "maintenance"
    /// * `reason` - Optional reason for maintenance (for audit logging)
    ///
    /// # Returns
    /// `Ok(MaintenanceSignalResponse)` on success, or an error if the signal failed
    pub async fn signal_maintenance(
        &self,
        uds_path: &Path,
        mode: &str,
        reason: Option<&str>,
    ) -> Result<MaintenanceSignalResponse, UdsClientError> {
        let request = serde_json::json!({
            "mode": mode,
            "reason": reason,
        });

        let mut stream = tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;

        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        let http_request = format!(
            "POST /maintenance HTTP/1.1\r\n\
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

/// Response from maintenance signal operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MaintenanceSignalResponse {
    /// Status of the operation: "accepted" or "error"
    pub status: String,
    /// Maintenance mode: "drain" or "maintenance"
    pub mode: String,
    /// Reason for maintenance
    pub reason: String,
    /// Whether the drain flag was set
    pub drain_flag_set: bool,
    /// Timestamp of when maintenance was signaled
    pub timestamp: String,
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

/// Parse cache budget exceeded error message to extract structured fields.
///
/// Expected format: "Model cache budget exceeded: needed X MB, freed Y MB (pinned=N, active=M), max Z MB"
fn parse_cache_budget_error(msg: &str) -> Option<(u64, u64, usize, usize, u64)> {
    // Look for the key numbers in the message
    // Example: "Model cache budget exceeded: needed 8192 MB, freed 2048 MB (pinned=3, active=2), max 4096 MB"

    let needed_mb = msg.find("needed ").and_then(|i| {
        let start = i + 7;
        let rest = &msg[start..];
        rest.split_whitespace()
            .next()
            .and_then(|s| s.parse::<u64>().ok())
    })?;

    let freed_mb = msg.find("freed ").and_then(|i| {
        let start = i + 6;
        let rest = &msg[start..];
        rest.split_whitespace()
            .next()
            .and_then(|s| s.parse::<u64>().ok())
    })?;

    let pinned_count = msg.find("pinned=").and_then(|i| {
        let start = i + 7;
        let rest = &msg[start..];
        rest.split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|s| s.parse::<usize>().ok())
    })?;

    let active_count = msg.find("active=").and_then(|i| {
        let start = i + 7;
        let rest = &msg[start..];
        rest.split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|s| s.parse::<usize>().ok())
    })?;

    let max_mb = msg.find("max ").and_then(|i| {
        let start = i + 4;
        let rest = &msg[start..];
        rest.split_whitespace()
            .next()
            .and_then(|s| s.parse::<u64>().ok())
    })?;

    Some((needed_mb, freed_mb, pinned_count, active_count, max_mb))
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
            request_id: Some("req-123".to_string()),
            run_envelope: None,
            require_evidence: true,
            admin_override: false,
            reasoning_mode: false,
            stop_policy: None,
            stack_id: Some("stack-42".to_string()),
            stack_version: Some(7),
            policy_id: Some("policy-1".to_string()),
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
            adapter_stable_ids: None,
            routing_policy: None,
            placement: None,
            adapter_strength_overrides: Some(
                [("adapter-a".to_string(), 0.8_f32)].into_iter().collect(),
            ),
            policy_mask_digest_b3: None,
            utf8_healing: true,
            fim_prefix: None,
            fim_suffix: None,
        };

        let serialized =
            serde_json::to_vec(&request).expect("WorkerInferRequest should serialize to JSON");
        let deserialized: crate::types::WorkerInferRequest =
            serde_json::from_slice(&serialized).expect("WorkerInferRequest should deserialize");

        assert_eq!(request.cpid, deserialized.cpid);
        assert_eq!(request.prompt, deserialized.prompt);
        assert_eq!(request.max_tokens, deserialized.max_tokens);
        assert_eq!(request.request_id, deserialized.request_id);
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
