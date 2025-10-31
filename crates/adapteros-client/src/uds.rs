//! Unix Domain Socket client for communicating with workers.
//!
//! The worker control plane speaks simple HTTP over Unix domain sockets.
//! This module provides a small async client that handles request formatting,
//! response parsing (including status/headers), and optional signal streaming
//! over Server-Sent Events (SSE).
//!
//! # Citations
//! - docs/llm-interface-specification.md §5.1 (signal protocol)
//! - crates/adapteros-lora-worker/src/uds_server.rs (server counterpart)

use crate::{types::*, AdapterOSClient};
use anyhow::Result;
use futures_util::stream::BoxStream;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

/// Error types for UDS client operations.
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

/// Parsed HTTP response returned by the worker.
#[derive(Debug)]
struct HttpResponse {
    status_code: u16,
    reason: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn is_success(&self) -> bool {
        (200..300).contains(&self.status_code)
    }

    fn header(&self, name: &str) -> Option<&str> {
        let key = name.to_ascii_lowercase();
        self.headers.get(&key).map(|s| s.as_str())
    }

    fn into_utf8(self) -> Result<String, UdsClientError> {
        String::from_utf8(self.body).map_err(|e| UdsClientError::SerializationError(e.to_string()))
    }

    fn body_bytes(&self) -> &[u8] {
        &self.body
    }
}

/// Streaming inference handle.
///
/// The worker streams signals as SSE events while the inference is running.
/// Final completion metadata (including the worker response payload) is sent
/// as the last `event: complete` message.  We expose the streaming channel
/// alongside a one-shot receiver that resolves once the completion event is
/// observed.
pub struct InferenceSession {
    /// Receiver for the final worker response (as raw JSON string).
    pub response: oneshot::Receiver<Result<String, UdsClientError>>,
    /// Stream of real-time worker signals.
    pub signals: BoxStream<'static, Result<Signal, UdsClientError>>,
}

/// UDS client for communicating with workers.
#[derive(Debug, Clone)]
pub struct UdsClient {
    timeout: Duration,
}

impl UdsClient {
    /// Create a new UDS client with the specified timeout.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Return the configured timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Establish a Unix domain socket connection with timeout.
    async fn connect(&self, uds_path: &Path) -> Result<UnixStream, UdsClientError> {
        tokio::time::timeout(self.timeout, UnixStream::connect(uds_path))
            .await
            .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
            .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))
    }

    /// Send an HTTP request and read the complete response.
    async fn send_http_request_internal(
        &self,
        uds_path: &Path,
        method: &str,
        path: &str,
        headers: Vec<(String, String)>,
        body: Option<&[u8]>,
    ) -> Result<HttpResponse, UdsClientError> {
        let mut stream = self.connect(uds_path).await?;

        let body_bytes = body.unwrap_or(&[]);
        let mut request = String::new();
        request.push_str(&format!("{} {} HTTP/1.1\r\n", method, path));

        // Track explicit headers to avoid duplicates.
        let mut header_map = HashMap::new();

        // Default headers that we always send.
        header_map.insert("host".to_string(), "worker".to_string());
        header_map.insert("connection".to_string(), "close".to_string());

        for (name, value) in headers {
            header_map.insert(name.to_ascii_lowercase(), value);
        }

        if !body_bytes.is_empty() {
            header_map
                .entry("content-type".to_string())
                .or_insert_with(|| "application/json".to_string());
            header_map.insert("content-length".to_string(), body_bytes.len().to_string());
        }

        for (name, value) in &header_map {
            request.push_str(&format!("{}: {}\r\n", name, value));
        }
        request.push_str("\r\n");

        // Write request head.
        tokio::time::timeout(self.timeout, stream.write_all(request.as_bytes()))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        // Write request body (if present).
        if !body_bytes.is_empty() {
            tokio::time::timeout(self.timeout, stream.write_all(body_bytes))
                .await
                .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
        }

        self.read_http_response(&mut stream).await
    }

    /// Read and parse an HTTP response from the provided stream.
    async fn read_http_response(
        &self,
        stream: &mut UnixStream,
    ) -> Result<HttpResponse, UdsClientError> {
        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();

        tokio::time::timeout(self.timeout, reader.read_line(&mut status_line))
            .await
            .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        if status_line.trim().is_empty() {
            return Err(UdsClientError::RequestFailed(
                "Empty response from worker".to_string(),
            ));
        }

        let (status_code, reason) = parse_status_line(&status_line)?;
        let mut headers = HashMap::new();

        loop {
            let mut line = String::new();
            tokio::time::timeout(self.timeout, reader.read_line(&mut line))
                .await
                .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

            let trimmed = line.trim_end_matches(&['\r', '\n'][..]);
            if trimmed.is_empty() {
                break;
            }

            if let Some((name, value)) = trimmed.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
            }
        }

        let mut body = Vec::new();
        if let Some(length) = headers.get("content-length") {
            let expected = length.parse::<usize>().map_err(|e| {
                UdsClientError::RequestFailed(format!("Invalid Content-Length: {}", e))
            })?;
            body.resize(expected, 0);
            tokio::time::timeout(self.timeout, reader.read_exact(&mut body))
                .await
                .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
        } else {
            tokio::time::timeout(self.timeout, reader.read_to_end(&mut body))
                .await
                .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
        }

        Ok(HttpResponse {
            status_code,
            reason,
            headers,
            body,
        })
    }

    /// Send a generic request to a worker. Returns the raw response body as UTF-8.
    pub async fn send_request(
        &self,
        uds_path: &Path,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<String, UdsClientError> {
        let response = self
            .send_http_request_internal(
                uds_path,
                method,
                path,
                Vec::new(),
                body.map(|s| s.as_bytes()),
            )
            .await?;

        if !response.is_success() {
            let body_text = String::from_utf8_lossy(response.body_bytes());
            return Err(UdsClientError::RequestFailed(format!(
                "Worker returned {} {}: {}",
                response.status_code, response.reason, body_text
            )));
        }

        response.into_utf8()
    }

    /// Send a request and deserialize the JSON body into the requested type.
    pub async fn send_json<T: serde::de::DeserializeOwned>(
        &self,
        uds_path: &Path,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<T, UdsClientError> {
        let raw = self.send_request(uds_path, method, path, body).await?;
        serde_json::from_str(&raw).map_err(|e| UdsClientError::SerializationError(e.to_string()))
    }

    /// Check if a worker is healthy via UDS.
    pub async fn health_check(&self, uds_path: &Path) -> Result<bool, UdsClientError> {
        let response = self
            .send_http_request_internal(uds_path, "GET", "/health", Vec::new(), None)
            .await?;

        if !response.is_success() {
            return Ok(false);
        }

        let body = response.into_utf8()?;
        let value: Value = serde_json::from_str(&body)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;
        Ok(value
            .get("status")
            .and_then(|s| s.as_str())
            .map(|s| s.eq_ignore_ascii_case("healthy") || s.eq_ignore_ascii_case("ok"))
            .unwrap_or(false))
    }

    /// Send an adapter command to worker.
    pub async fn adapter_command(
        &self,
        uds_path: &Path,
        adapter_id: &str,
        command: &str,
    ) -> Result<(), UdsClientError> {
        self.send_request(
            uds_path,
            "POST",
            &format!("/adapter/{}/{}", adapter_id, command),
            None,
        )
        .await
        .map(|_| ())
    }

    /// List all adapters from worker (raw JSON string).
    pub async fn list_adapters(&self, uds_path: &Path) -> Result<String, UdsClientError> {
        self.send_request(uds_path, "GET", "/adapters", None).await
    }

    /// Get adapter profile (raw JSON string).
    pub async fn get_adapter_profile(
        &self,
        uds_path: &Path,
        adapter_id: &str,
    ) -> Result<String, UdsClientError> {
        self.send_request(uds_path, "GET", &format!("/adapter/{}", adapter_id), None)
            .await
    }

    /// Get profiling snapshot (raw JSON string).
    pub async fn get_profiling_snapshot(&self, uds_path: &Path) -> Result<String, UdsClientError> {
        self.send_request(uds_path, "GET", "/profile/snapshot", None)
            .await
    }

    /// Start an inference request with signal streaming support.
    ///
    /// The returned [`InferenceSession`] exposes a `signals` stream for
    /// real-time updates and a one-shot `response` receiver that resolves
    /// once the worker sends the terminal completion event.
    pub async fn inference_with_signals(
        &self,
        uds_path: &Path,
        request_body: &str,
        trace_id: Option<&str>,
    ) -> Result<InferenceSession, UdsClientError> {
        let mut stream = self.connect(uds_path).await?;

        // Prepare headers required for SSE streaming.
        let mut headers = vec![
            ("accept".to_string(), "text/event-stream".to_string()),
            ("cache-control".to_string(), "no-cache".to_string()),
            ("connection".to_string(), "keep-alive".to_string()),
            ("x-signal-stream".to_string(), "true".to_string()),
        ];
        if let Some(trace) = trace_id {
            headers.push(("x-trace-id".to_string(), trace.to_string()));
        }

        // Write request with streaming headers.
        let body_bytes = request_body.as_bytes();
        let mut request = String::new();
        request.push_str("POST /inference HTTP/1.1\r\n");
        request.push_str("Host: worker\r\n");
        request.push_str("Content-Type: application/json\r\n");
        request.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
        for (name, value) in &headers {
            request.push_str(&format!("{}: {}\r\n", name, value));
        }
        request.push_str("\r\n");

        tokio::time::timeout(self.timeout, stream.write_all(request.as_bytes()))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;
        tokio::time::timeout(self.timeout, stream.write_all(body_bytes))
            .await
            .map_err(|_| UdsClientError::Timeout("Write timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        tokio::time::timeout(self.timeout, reader.read_line(&mut status_line))
            .await
            .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
            .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

        let (status_code, reason) = parse_status_line(&status_line)?;
        if status_code != 200 {
            return Err(UdsClientError::RequestFailed(format!(
                "Worker returned {} {}",
                status_code, reason
            )));
        }

        // Consume header lines until blank separator.
        loop {
            let mut line = String::new();
            tokio::time::timeout(self.timeout, reader.read_line(&mut line))
                .await
                .map_err(|_| UdsClientError::Timeout("Read timed out".to_string()))?
                .map_err(|e| UdsClientError::RequestFailed(e.to_string()))?;

            if line.trim().is_empty() {
                break;
            }
        }

        let (signal_tx, signal_rx) = mpsc::channel::<Result<Signal, UdsClientError>>(64);
        let (response_tx, response_rx) = oneshot::channel::<Result<String, UdsClientError>>();
        let timeout = self.timeout;

        tokio::spawn(async move {
            let mut reader = reader;
            let mut line_buf = String::new();
            let mut event_type: Option<String> = None;
            let mut data_buf = String::new();
            let mut completion_result: Option<Result<String, UdsClientError>> = None;

            loop {
                line_buf.clear();
                let read = tokio::time::timeout(timeout, reader.read_line(&mut line_buf)).await;

                match read {
                    Ok(Ok(0)) => break, // EOF
                    Ok(Ok(_)) => {
                        let trimmed = line_buf.trim_end_matches(&['\r', '\n'][..]);
                        if trimmed.is_empty() {
                            if let Some(ref ty) = event_type {
                                match ty.as_str() {
                                    "signal" => {
                                        if !data_buf.is_empty() {
                                            let result = serde_json::from_str::<Signal>(&data_buf)
                                                .map_err(|e| {
                                                    UdsClientError::SerializationError(
                                                        e.to_string(),
                                                    )
                                                });
                                            let _ = signal_tx.send(result).await;
                                        }
                                    }
                                    "complete" => {
                                        if completion_result.is_none() {
                                            completion_result = Some(Ok(data_buf.clone()));
                                            break; // Exit the loop after receiving completion
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            event_type = None;
                            data_buf.clear();
                        } else if let Some(rest) = trimmed.strip_prefix("event:") {
                            event_type = Some(rest.trim().to_string());
                        } else if let Some(rest) = trimmed.strip_prefix("data:") {
                            if !data_buf.is_empty() {
                                data_buf.push('\n');
                            }
                            data_buf.push_str(rest.trim_start());
                        }
                    }
                    Ok(Err(e)) => {
                        let _ = signal_tx
                            .send(Err(UdsClientError::RequestFailed(e.to_string())))
                            .await;
                        completion_result = Some(Err(UdsClientError::RequestFailed(e.to_string())));
                        break;
                    }
                    Err(_) => {
                        let _ = signal_tx
                            .send(Err(UdsClientError::Timeout("Read timed out".to_string())))
                            .await;
                        completion_result =
                            Some(Err(UdsClientError::Timeout("Read timed out".to_string())));
                        break;
                    }
                }
            }

            // Send the completion result (success or error)
            let result = completion_result.unwrap_or_else(|| {
                Err(UdsClientError::RequestFailed(
                    "Inference stream closed before completion event".to_string(),
                ))
            });
            let _ = response_tx.send(result);
        });

        let signal_stream = ReceiverStream::new(signal_rx).map(|item| item);

        Ok(InferenceSession {
            response: response_rx,
            signals: Box::pin(signal_stream),
        })
    }

    /// Create a connection pool for efficient UDS communication.
    pub async fn create_connection_pool(
        &self,
        uds_path: &Path,
        pool_size: usize,
    ) -> Result<ConnectionPool, UdsClientError> {
        ConnectionPool::new(uds_path, pool_size, self.timeout).await
    }
}

/// Connection pool for efficient UDS communication.
pub struct ConnectionPool {
    socket_path: PathBuf,
    timeout: Duration,
    max_size: usize,
    connections: Vec<UnixStream>,
}

impl ConnectionPool {
    /// Establish a new pool with the requested number of eager connections.
    pub async fn new(
        socket_path: &Path,
        pool_size: usize,
        timeout: Duration,
    ) -> Result<Self, UdsClientError> {
        let mut connections = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            let stream = tokio::time::timeout(timeout, UnixStream::connect(socket_path))
                .await
                .map_err(|_| UdsClientError::Timeout("Connection timed out".to_string()))?
                .map_err(|e| UdsClientError::ConnectionFailed(e.to_string()))?;
            connections.push(stream);
        }

        Ok(Self {
            socket_path: socket_path.to_path_buf(),
            timeout,
            max_size: pool_size,
            connections,
        })
    }

    /// Acquire an available connection from the pool.
    pub fn get_connection(&mut self) -> Result<UnixStream, UdsClientError> {
        self.connections.pop().ok_or_else(|| {
            UdsClientError::WorkerNotAvailable("No available connections".to_string())
        })
    }

    /// Return a connection back to the pool.
    pub fn return_connection(&mut self, stream: UnixStream) {
        if self.connections.len() < self.max_size {
            self.connections.push(stream);
        }
    }

    /// Check if the pool has idle connections.
    pub fn has_available(&self) -> bool {
        !self.connections.is_empty()
    }

    /// Number of idle connections.
    pub fn available_count(&self) -> usize {
        self.connections.len()
    }

    /// Maximum size requested at construction.
    pub fn size(&self) -> usize {
        self.max_size
    }

    /// Socket path backing this pool.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Timeout configured for pooled connections.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// Signal type for client consumption.
///
/// Simplified signal structure for client-side processing.
/// The full worker definition lives in `mplora-worker/src/signal.rs`.
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

fn parse_status_line(line: &str) -> Result<(u16, String), UdsClientError> {
    let mut parts = line.split_whitespace();
    let http_version = parts
        .next()
        .ok_or_else(|| UdsClientError::RequestFailed("Invalid status line".to_string()))?;
    if !http_version.starts_with("HTTP/") {
        return Err(UdsClientError::RequestFailed(format!(
            "Unexpected status line: {}",
            line.trim()
        )));
    }
    let status_code = parts
        .next()
        .ok_or_else(|| UdsClientError::RequestFailed("Missing status code".to_string()))?;
    let status_code: u16 = status_code
        .parse()
        .map_err(|e| UdsClientError::RequestFailed(format!("Invalid status code: {}", e)))?;
    let reason = parts.collect::<Vec<_>>().join(" ");
    Ok((
        status_code,
        if reason.is_empty() {
            "OK".to_string()
        } else {
            reason
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn client_holds_timeout() {
        let client = UdsClient::new(Duration::from_secs(5));
        assert_eq!(client.timeout(), Duration::from_secs(5));
    }

    #[tokio::test]
    async fn connection_pool_metadata() {
        let pool = ConnectionPool {
            socket_path: PathBuf::from("/tmp/test.sock"),
            timeout: Duration::from_secs(2),
            max_size: 3,
            connections: Vec::new(),
        };

        assert_eq!(pool.size(), 3);
        assert_eq!(pool.timeout(), Duration::from_secs(2));
        assert_eq!(pool.socket_path(), Path::new("/tmp/test.sock"));
    }
}

impl AdapterOSClient for UdsClient {
    // Health & Auth
    fn health(&self) -> impl std::future::Future<Output = Result<HealthResponse>> + Send {
        async {
            Ok(HealthResponse {
                status: "uds".to_string(),
                version: "unavailable".to_string(),
            })
        }
    }

    fn login(
        &self,
        _req: LoginRequest,
    ) -> impl std::future::Future<Output = Result<LoginResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "UDS client does not implement authentication"
            ))
        }
    }

    fn logout(&self) -> impl std::future::Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }

    fn me(&self) -> impl std::future::Future<Output = Result<UserInfoResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "UDS client does not implement user information"
            ))
        }
    }

    // Tenants
    fn list_tenants(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<TenantResponse>>> + Send {
        async { Ok(Vec::new()) }
    }

    fn create_tenant(
        &self,
        _req: CreateTenantRequest,
    ) -> impl std::future::Future<Output = Result<TenantResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "UDS client does not manage control-plane tenants"
            ))
        }
    }

    // Adapters
    fn list_adapters(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<AdapterResponse>>> + Send {
        async {
            Err(anyhow::anyhow!(
                "UDS client does not expose structured adapter listings"
            ))
        }
    }

    fn register_adapter(
        &self,
        _req: RegisterAdapterRequest,
    ) -> impl std::future::Future<Output = Result<AdapterResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "UDS client does not register adapters with control plane"
            ))
        }
    }

    fn evict_adapter(
        &self,
        _adapter_id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send {
        async {
            Err(anyhow::anyhow!(
                "UDS client does not manage adapter eviction via control plane"
            ))
        }
    }

    fn pin_adapter(
        &self,
        _adapter_id: &str,
        _pinned: bool,
    ) -> impl std::future::Future<Output = Result<()>> + Send {
        async {
            Err(anyhow::anyhow!(
                "UDS client does not manage adapter pinning via control plane"
            ))
        }
    }

    // Memory Management
    fn get_memory_usage(
        &self,
    ) -> impl std::future::Future<Output = Result<MemoryUsageResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "UDS client does not expose control-plane memory usage"
            ))
        }
    }

    // Training
    fn start_adapter_training(
        &self,
        _req: StartTrainingRequest,
    ) -> impl std::future::Future<Output = Result<TrainingSessionResponse>> + Send {
        async { Err(anyhow::anyhow!("Training via UDS client is unsupported")) }
    }

    fn get_training_session(
        &self,
        _session_id: &str,
    ) -> impl std::future::Future<Output = Result<TrainingSessionResponse>> + Send {
        async { Err(anyhow::anyhow!("Training via UDS client is unsupported")) }
    }

    fn list_training_sessions(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<TrainingSessionResponse>>> + Send {
        async { Err(anyhow::anyhow!("Training via UDS client is unsupported")) }
    }

    // Telemetry
    fn get_telemetry_events(
        &self,
        _filters: TelemetryFilters,
    ) -> impl std::future::Future<Output = Result<Vec<TelemetryEvent>>> + Send {
        async { Err(anyhow::anyhow!("Telemetry via UDS client is unsupported")) }
    }

    // Nodes
    fn list_nodes(&self) -> impl std::future::Future<Output = Result<Vec<NodeResponse>>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Node management via UDS client is unsupported"
            ))
        }
    }

    fn register_node(
        &self,
        _req: RegisterNodeRequest,
    ) -> impl std::future::Future<Output = Result<NodeResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Node registration via UDS client is unsupported"
            ))
        }
    }

    // Plans
    fn list_plans(
        &self,
        _tenant_id: Option<String>,
    ) -> impl std::future::Future<Output = Result<Vec<PlanResponse>>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Plan management via UDS client is unsupported"
            ))
        }
    }

    fn build_plan(
        &self,
        _req: BuildPlanRequest,
    ) -> impl std::future::Future<Output = Result<JobResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Plan building via UDS client is unsupported"
            ))
        }
    }

    // Workers
    fn list_workers(
        &self,
        _tenant_id: Option<String>,
    ) -> impl std::future::Future<Output = Result<Vec<WorkerResponse>>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Worker management via UDS client is unsupported"
            ))
        }
    }

    fn spawn_worker(
        &self,
        _req: SpawnWorkerRequest,
    ) -> impl std::future::Future<Output = Result<()>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Worker spawning via UDS client is unsupported"
            ))
        }
    }

    // CP Operations
    fn promote_cp(
        &self,
        _req: PromoteCPRequest,
    ) -> impl std::future::Future<Output = Result<PromotionResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Control-plane promotion via UDS client is unsupported"
            ))
        }
    }

    fn promotion_gates(
        &self,
        _cpid: String,
    ) -> impl std::future::Future<Output = Result<PromotionGatesResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Control-plane promotion via UDS client is unsupported"
            ))
        }
    }

    fn rollback_cp(
        &self,
        _req: RollbackCPRequest,
    ) -> impl std::future::Future<Output = Result<RollbackResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Control-plane rollback via UDS client is unsupported"
            ))
        }
    }

    // Jobs
    fn list_jobs(
        &self,
        _tenant_id: Option<String>,
    ) -> impl std::future::Future<Output = Result<Vec<JobResponse>>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Job management via UDS client is unsupported"
            ))
        }
    }

    // Models
    fn import_model(
        &self,
        _req: ImportModelRequest,
    ) -> impl std::future::Future<Output = Result<()>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Model import via UDS client is unsupported"
            ))
        }
    }

    // Policies
    fn list_policies(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<PolicyPackResponse>>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Policy management via UDS client is unsupported"
            ))
        }
    }

    fn get_policy(
        &self,
        _cpid: String,
    ) -> impl std::future::Future<Output = Result<PolicyPackResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Policy management via UDS client is unsupported"
            ))
        }
    }

    fn validate_policy(
        &self,
        _req: ValidatePolicyRequest,
    ) -> impl std::future::Future<Output = Result<PolicyValidationResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Policy management via UDS client is unsupported"
            ))
        }
    }

    fn apply_policy(
        &self,
        _req: ApplyPolicyRequest,
    ) -> impl std::future::Future<Output = Result<PolicyPackResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Policy management via UDS client is unsupported"
            ))
        }
    }

    // Telemetry Bundles
    fn list_telemetry_bundles(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<TelemetryBundleResponse>>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Telemetry bundles via UDS client are unsupported"
            ))
        }
    }

    // Code Intelligence
    fn register_repo(
        &self,
        _req: RegisterRepoRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Code intelligence via UDS client is unsupported"
            ))
        }
    }

    fn scan_repo(
        &self,
        _req: ScanRepoRequest,
    ) -> impl std::future::Future<Output = Result<JobResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Code intelligence via UDS client is unsupported"
            ))
        }
    }

    fn list_repos(&self) -> impl std::future::Future<Output = Result<Vec<RepoResponse>>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Code intelligence via UDS client is unsupported"
            ))
        }
    }

    fn list_adapters_by_tenant(
        &self,
        _tenant_id: String,
    ) -> impl std::future::Future<Output = Result<ListAdaptersResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Code intelligence via UDS client is unsupported"
            ))
        }
    }

    fn get_adapter_activations(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<ActivationData>>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Code intelligence via UDS client is unsupported"
            ))
        }
    }

    fn create_commit_delta(
        &self,
        _req: CommitDeltaRequest,
    ) -> impl std::future::Future<Output = Result<CommitDeltaResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Code intelligence via UDS client is unsupported"
            ))
        }
    }

    fn get_commit_details(
        &self,
        _repo_id: String,
        _commit: String,
    ) -> impl std::future::Future<Output = Result<CommitDetailsResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Code intelligence via UDS client is unsupported"
            ))
        }
    }

    // Routing Inspector
    fn extract_router_features(
        &self,
        _req: RouterFeaturesRequest,
    ) -> impl std::future::Future<Output = Result<RouterFeaturesResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Routing inspector via UDS client is unsupported"
            ))
        }
    }

    fn score_adapters(
        &self,
        _req: ScoreAdaptersRequest,
    ) -> impl std::future::Future<Output = Result<ScoreAdaptersResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Routing inspector via UDS client is unsupported"
            ))
        }
    }

    // Patch Lab
    fn propose_patch(
        &self,
        _req: ProposePatchRequest,
    ) -> impl std::future::Future<Output = Result<ProposePatchResponse>> + Send {
        async { Err(anyhow::anyhow!("Patch lab via UDS client is unsupported")) }
    }

    fn validate_patch(
        &self,
        _req: ValidatePatchRequest,
    ) -> impl std::future::Future<Output = Result<ValidatePatchResponse>> + Send {
        async { Err(anyhow::anyhow!("Patch lab via UDS client is unsupported")) }
    }

    fn apply_patch(
        &self,
        _req: ApplyPatchRequest,
    ) -> impl std::future::Future<Output = Result<ApplyPatchResponse>> + Send {
        async { Err(anyhow::anyhow!("Patch lab via UDS client is unsupported")) }
    }

    // Code Policy
    fn get_code_policy(
        &self,
    ) -> impl std::future::Future<Output = Result<GetCodePolicyResponse>> + Send {
        async { Err(anyhow::anyhow!("Code policy via UDS client is unsupported")) }
    }

    fn update_code_policy(
        &self,
        _req: UpdateCodePolicyRequest,
    ) -> impl std::future::Future<Output = Result<()>> + Send {
        async { Err(anyhow::anyhow!("Code policy via UDS client is unsupported")) }
    }

    // Metrics Dashboard
    fn get_code_metrics(
        &self,
        _req: CodeMetricsRequest,
    ) -> impl std::future::Future<Output = Result<CodeMetricsResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Metrics dashboard via UDS client is unsupported"
            ))
        }
    }

    fn compare_metrics(
        &self,
        _req: CompareMetricsRequest,
    ) -> impl std::future::Future<Output = Result<CompareMetricsResponse>> + Send {
        async {
            Err(anyhow::anyhow!(
                "Metrics dashboard via UDS client is unsupported"
            ))
        }
    }
}
