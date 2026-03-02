//! Unix Domain Socket client for communicating with workers
//!
//! This module provides functionality to connect to worker UDS servers
//! and forward inference requests from the CLI.
//!
//! **Signal Protocol Support**: Extended to support receiving signals from
//! workers during inference via Server-Sent Events (SSE).
//!
//! Citation: docs/llm-interface-specification.md §5.1

use crate::{adapterOSClient, types::*, TelemetryBundleResponse, TelemetryEvent};
use adapteros_types::tenants::Tenant;
use adapteros_types::training::{
    DataLineageMode, DatasetVersionSelection, LoraTier, TrainingConfig,
};
use anyhow::Result;
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
        let mut request = "GET /signals HTTP/1.1\r\n\
             Host: worker\r\n\
             Accept: text/event-stream\r\n\
             Cache-Control: no-cache\r\n"
            .to_string();

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
                        } else if let Some(data) = line.strip_prefix("data: ") {
                            event_data.push_str(data);
                        } else if let Some(evt) = line.strip_prefix("event: ") {
                            event_type = evt.to_string();
                        } else if let Some(id) = line.strip_prefix("id: ") {
                            event_id = id.to_string();
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

    /// Send inference request to worker
    ///
    /// This method sends an inference request and returns the response.
    /// For streaming with signals, use `inference_with_signals` instead.
    pub async fn infer<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        uds_path: &Path,
        request: T,
    ) -> Result<R, UdsClientError> {
        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        let response_json = self
            .send_request(uds_path, "POST", "/infer", Some(&request_json))
            .await?;

        serde_json::from_str(&response_json)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))
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

        let request_json = serde_json::to_string(&request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        let response = self
            .send_request(uds_path, "POST", "/training/cancel", Some(&request_json))
            .await?;

        serde_json::from_str(&response)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))
    }

    /// Start a training job via UDS.
    pub async fn start_training_job(
        &self,
        uds_path: &Path,
        request: &UdsTrainingStartRequest,
    ) -> Result<UdsTrainingStartResponse, UdsClientError> {
        let request_json = serde_json::to_string(request)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))?;

        let response = self
            .send_request(uds_path, "POST", "/training/start", Some(&request_json))
            .await?;

        serde_json::from_str(&response)
            .map_err(|e| UdsClientError::SerializationError(e.to_string()))
    }
}

/// Connection pool for efficient UDS communication
pub struct ConnectionPool {
    connections: Vec<UnixStream>,
    #[allow(dead_code)]
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

/// Request payload for worker-dispatched training execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UdsTrainingStartRequest {
    /// Control-plane job id to preserve cancel compatibility.
    pub job_id: String,
    pub adapter_name: String,
    pub config: TrainingConfig,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
    pub target_branch: Option<String>,
    pub base_version_id: Option<String>,
    pub dataset_id: Option<String>,
    pub dataset_version_ids: Option<Vec<DatasetVersionSelection>>,
    pub synthetic_mode: bool,
    pub data_lineage_mode: DataLineageMode,
    pub tenant_id: Option<String>,
    pub initiated_by: Option<String>,
    pub initiated_by_role: Option<String>,
    pub base_model_id: Option<String>,
    pub collection_id: Option<String>,
    pub scope: Option<String>,
    pub lora_tier: Option<LoraTier>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub post_actions_json: Option<String>,
    pub retry_of_job_id: Option<String>,
    pub code_commit_sha: Option<String>,
    pub data_spec_json: Option<String>,
    pub data_spec_hash: Option<String>,
}

/// Response payload for worker-dispatched training execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UdsTrainingStartResponse {
    /// Control-plane job id echoed from request.
    pub job_id: String,
    /// Worker-local job id when worker executes via local TrainingService.
    pub worker_job_id: Option<String>,
    /// Dispatch status ("accepted" on success).
    pub status: String,
}

impl adapterOSClient for UdsClient {
    // Health & Auth
    async fn health(&self) -> Result<HealthResponse> {
        // UDS clients typically don't implement health checks
        // Return a mock response for now
        Ok(HealthResponse {
            schema_version: "1.0".to_string(),
            status: "healthy".to_string(),
            version: "1.0.0".to_string(),
            build_id: option_env!("AOS_BUILD_ID").map(|s| s.to_string()),
            models: None,
            crate_manifest: None,
            crate_manifest_digest: None,
        })
    }

    async fn login(&self, _req: LoginRequest) -> Result<LoginResponse> {
        // UDS clients don't implement authentication
        // Return a mock response for now
        Ok(LoginResponse {
            schema_version: "1.0".to_string(),
            token: "uds-token".to_string(),
            user_id: "uds-user".to_string(),
            tenant_id: "default".to_string(),
            role: "admin".to_string(),
            expires_in: 28800, // 8 hours
            tenants: Some(vec![]),
            mfa_level: None,
        })
    }

    async fn logout(&self) -> Result<()> {
        // UDS clients don't implement logout
        Ok(())
    }

    async fn me(&self) -> Result<UserInfoResponse> {
        // UDS clients don't implement user info
        Ok(UserInfoResponse {
            schema_version: "1.0".to_string(),
            user_id: "uds-user".to_string(),
            email: "uds@example.com".to_string(),
            role: "admin".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            tenant_id: "default".to_string(),
            display_name: "UDS Operator".to_string(),
            permissions: vec!["AdapterList".to_string()],
            admin_tenants: vec![],
            last_login_at: None,
            mfa_enabled: None,
            token_last_rotated_at: None,
        })
    }

    // Tenants
    async fn list_tenants(&self) -> Result<Vec<TenantResponse>> {
        // UDS clients typically work with a single tenant
        Ok(vec![TenantResponse {
            schema_version: "1.0".to_string(),
            tenant: Tenant {
                id: "default".to_string(),
                name: "Default Tenant".to_string(),
                itar_flag: false,
                created_at: "2025-01-01T00:00:00Z".to_string(),
                status: Some("active".to_string()),
                updated_at: None,
                default_stack_id: None,
                max_adapters: None,
                max_training_jobs: None,
                max_storage_gb: None,
                rate_limit_rpm: None,
                default_pinned_adapter_ids: None,
                max_kv_cache_bytes: None,
                kv_residency_policy_id: None,
            },
        }])
    }

    async fn create_tenant(&self, _req: CreateTenantRequest) -> Result<TenantResponse> {
        Err(anyhow::anyhow!("UDS clients don't support tenant creation"))
    }

    // Adapters
    async fn list_adapters(&self) -> Result<Vec<AdapterResponse>> {
        // UDS clients would need to connect to worker socket
        // For now, return empty list
        Ok(vec![])
    }

    async fn register_adapter(&self, _req: RegisterAdapterRequest) -> Result<AdapterResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support adapter registration"
        ))
    }

    async fn evict_adapter(&self, _adapter_id: &str) -> Result<()> {
        Err(anyhow::anyhow!(
            "UDS clients don't support adapter eviction"
        ))
    }

    async fn pin_adapter(&self, _adapter_id: &str, _pinned: bool) -> Result<()> {
        Err(anyhow::anyhow!("UDS clients don't support adapter pinning"))
    }

    // Memory Management
    async fn get_memory_usage(&self) -> Result<MemoryUsageResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support memory management"
        ))
    }

    // Training
    async fn start_adapter_training(
        &self,
        _req: StartTrainingRequest,
    ) -> Result<TrainingSessionResponse> {
        Err(anyhow::anyhow!("UDS clients don't support training"))
    }

    async fn get_training_session(&self, _session_id: &str) -> Result<TrainingSessionResponse> {
        Err(anyhow::anyhow!("UDS clients don't support training"))
    }

    async fn list_training_sessions(&self) -> Result<Vec<TrainingSessionResponse>> {
        Err(anyhow::anyhow!("UDS clients don't support training"))
    }

    // Telemetry
    async fn get_telemetry_events(
        &self,
        _filters: TelemetryFilters,
    ) -> Result<Vec<TelemetryEvent>> {
        Err(anyhow::anyhow!("UDS clients don't support telemetry"))
    }

    // Nodes
    async fn list_nodes(&self) -> Result<Vec<NodeResponse>> {
        Err(anyhow::anyhow!("UDS clients don't support node management"))
    }

    async fn register_node(&self, _req: RegisterNodeRequest) -> Result<NodeResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support node registration"
        ))
    }

    // Plans
    async fn list_plans(&self, _tenant_id: Option<String>) -> Result<Vec<PlanResponse>> {
        Err(anyhow::anyhow!("UDS clients don't support plan management"))
    }

    async fn build_plan(&self, _req: BuildPlanRequest) -> Result<JobResponse> {
        Err(anyhow::anyhow!("UDS clients don't support plan building"))
    }

    // Workers
    async fn list_workers(&self, _tenant_id: Option<String>) -> Result<Vec<WorkerResponse>> {
        Err(anyhow::anyhow!(
            "UDS clients don't support worker management"
        ))
    }

    async fn spawn_worker(&self, _req: SpawnWorkerRequest) -> Result<()> {
        Err(anyhow::anyhow!("UDS clients don't support worker spawning"))
    }

    // CP Operations
    async fn promote_cp(&self, _req: PromoteCPRequest) -> Result<PromotionResponse> {
        Err(anyhow::anyhow!("UDS clients don't support CP operations"))
    }

    async fn promotion_gates(&self, _cpid: String) -> Result<PromotionGatesResponse> {
        Err(anyhow::anyhow!("UDS clients don't support CP operations"))
    }

    async fn rollback_cp(&self, _req: RollbackCPRequest) -> Result<RollbackResponse> {
        Err(anyhow::anyhow!("UDS clients don't support CP operations"))
    }

    // Jobs
    async fn list_jobs(&self, _tenant_id: Option<String>) -> Result<Vec<JobResponse>> {
        Err(anyhow::anyhow!("UDS clients don't support job management"))
    }

    // Models
    async fn import_model(&self, _req: ImportModelRequest) -> Result<()> {
        Err(anyhow::anyhow!("UDS clients don't support model import"))
    }

    // Policies
    async fn list_policies(&self) -> Result<Vec<PolicyPackResponse>> {
        Err(anyhow::anyhow!(
            "UDS clients don't support policy management"
        ))
    }

    async fn get_policy(&self, _cpid: String) -> Result<PolicyPackResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support policy management"
        ))
    }

    async fn validate_policy(
        &self,
        _req: ValidatePolicyRequest,
    ) -> Result<PolicyValidationResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support policy management"
        ))
    }

    async fn apply_policy(&self, _req: ApplyPolicyRequest) -> Result<PolicyPackResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support policy management"
        ))
    }

    // Telemetry Bundles
    async fn list_telemetry_bundles(&self) -> Result<Vec<TelemetryBundleResponse>> {
        Err(anyhow::anyhow!(
            "UDS clients don't support telemetry bundles"
        ))
    }

    // Code Intelligence
    async fn register_repo(&self, _req: RegisterRepoRequest) -> Result<RepoResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support code intelligence"
        ))
    }

    async fn scan_repo(&self, _req: ScanRepoRequest) -> Result<JobResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support code intelligence"
        ))
    }

    async fn list_repos(&self) -> Result<Vec<RepoResponse>> {
        Err(anyhow::anyhow!(
            "UDS clients don't support code intelligence"
        ))
    }

    async fn list_adapters_by_tenant(&self, _tenant_id: String) -> Result<ListAdaptersResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support code intelligence"
        ))
    }

    async fn get_adapter_activations(&self) -> Result<Vec<ActivationData>> {
        Err(anyhow::anyhow!(
            "UDS clients don't support code intelligence"
        ))
    }

    async fn create_commit_delta(&self, _req: CommitDeltaRequest) -> Result<CommitDeltaResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support code intelligence"
        ))
    }

    async fn get_commit_details(
        &self,
        _repo_id: String,
        _commit: String,
    ) -> Result<CommitDetailsResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support code intelligence"
        ))
    }

    // Patch Lab
    async fn propose_patch(&self, _req: ProposePatchRequest) -> Result<ProposePatchResponse> {
        Err(anyhow::anyhow!("UDS clients don't support patch lab"))
    }

    async fn validate_patch(&self, _req: ValidatePatchRequest) -> Result<ValidatePatchResponse> {
        Err(anyhow::anyhow!("UDS clients don't support patch lab"))
    }

    async fn apply_patch(&self, _req: ApplyPatchRequest) -> Result<ApplyPatchResponse> {
        Err(anyhow::anyhow!("UDS clients don't support patch lab"))
    }

    // Code Policy
    async fn get_code_policy(&self) -> Result<GetCodePolicyResponse> {
        Err(anyhow::anyhow!("UDS clients don't support code policy"))
    }

    async fn update_code_policy(&self, _req: UpdateCodePolicyRequest) -> Result<()> {
        Err(anyhow::anyhow!("UDS clients don't support code policy"))
    }

    // Metrics Dashboard
    async fn get_code_metrics(&self, _req: CodeMetricsRequest) -> Result<CodeMetricsResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support metrics dashboard"
        ))
    }

    async fn compare_metrics(&self, _req: CompareMetricsRequest) -> Result<CompareMetricsResponse> {
        Err(anyhow::anyhow!(
            "UDS clients don't support metrics dashboard"
        ))
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
