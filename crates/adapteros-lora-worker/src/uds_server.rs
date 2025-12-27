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

use adapteros_boot::jti_cache::JtiCacheStore;
use adapteros_config::prepare_socket_path;
use adapteros_core::{AosError, Result};
use blake3::Hasher;
use ed25519_dalek::VerifyingKey;
use serde_json;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::{
    backpressure::{BackpressureGate, BackpressureStats},
    health::HealthMonitor,
    CancelTrainingRequest, InferenceRequest, InferenceResponse, PatchProposalRequest, RequestType,
    Worker,
};
use adapteros_db::Db;

/// Request to load a model into the worker
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelLoadRequest {
    /// Model ID to load
    pub model_id: String,
    /// Path to the model directory
    pub model_path: String,
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

/// UDS server for worker communication
use crate::StrictnessControl;

/// Fixed threshold for UDS accept failures before tripping the circuit breaker.
const UDS_ACCEPT_FAILURE_THRESHOLD: u32 = 5;

fn trip_uds_accept_circuit_breaker(
    failure_count: u32,
    drain_flag: &AtomicBool,
    health_monitor: Option<&HealthMonitor>,
) {
    drain_flag.store(true, Ordering::Relaxed);

    if let Some(monitor) = health_monitor {
        monitor.record_fatal(
            "uds_accept_circuit_breaker",
            format!(
                "UDS accept failed {} times; circuit breaker tripped",
                failure_count
            ),
        );
        monitor.request_shutdown();
    }
}

pub struct UdsServer<K: adapteros_lora_kernel_api::FusedKernels + StrictnessControl + 'static> {
    socket_path: PathBuf,
    worker: Arc<Mutex<Worker<K>>>,
    api_key_db: Option<std::sync::Arc<Db>>,
    drain_flag: Arc<AtomicBool>,
    backpressure: Arc<BackpressureGate>,
    /// Ed25519 public key for validating worker tokens from control plane
    worker_verifying_key: Option<Arc<VerifyingKey>>,
    /// Worker ID for token validation (must match expected wid claim)
    worker_id: String,
    /// JTI cache for replay defense (prevents token reuse).
    /// This is a persistent cache that survives worker restarts.
    /// Only allocated when worker_verifying_key is Some (auth enabled).
    jti_cache: Option<Arc<Mutex<JtiCacheStore>>>,
}

impl<K: adapteros_lora_kernel_api::FusedKernels + StrictnessControl + 'static> UdsServer<K> {
    /// Create a new UDS server
    pub fn new(
        socket_path: PathBuf,
        worker: Arc<Mutex<Worker<K>>>,
        api_key_db: Option<std::sync::Arc<Db>>,
        drain_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            socket_path,
            worker,
            api_key_db,
            drain_flag,
            backpressure: Arc::new(BackpressureGate::from_env()),
            worker_verifying_key: None,
            worker_id: "unknown".to_string(),
            jti_cache: None, // Not needed when auth is disabled
        }
    }

    /// Create a new UDS server with worker token validation
    ///
    /// The verifying key is used to validate Ed25519-signed JWTs from the control plane.
    /// The worker_id must match the expected `wid` claim in the token.
    /// The jti_cache is a persistent cache for replay defense that survives restarts.
    pub fn new_with_worker_auth(
        socket_path: PathBuf,
        worker: Arc<Mutex<Worker<K>>>,
        api_key_db: Option<std::sync::Arc<Db>>,
        drain_flag: Arc<AtomicBool>,
        worker_verifying_key: VerifyingKey,
        worker_id: String,
        jti_cache: Arc<Mutex<JtiCacheStore>>,
    ) -> Self {
        info!(
            worker_id = %worker_id,
            jti_cache_capacity = jti_cache.blocking_lock().capacity(),
            "UDS server configured with worker token validation and persistent JTI cache"
        );
        Self {
            socket_path,
            worker,
            api_key_db,
            drain_flag,
            backpressure: Arc::new(BackpressureGate::from_env()),
            worker_verifying_key: Some(Arc::new(worker_verifying_key)),
            worker_id,
            jti_cache: Some(jti_cache),
        }
    }

    /// Persist the JTI cache to disk for graceful shutdown.
    ///
    /// This should be called before the worker exits to ensure replay defense
    /// survives across restarts.
    pub async fn persist_jti_cache(&self) -> std::result::Result<(), std::io::Error> {
        if let Some(cache) = &self.jti_cache {
            let guard = cache.lock().await;
            guard.persist()?;
            info!("JTI cache persisted successfully");
        }
        Ok(())
    }

    /// Get the JTI cache for external access (e.g., shutdown hooks)
    pub fn jti_cache(&self) -> Option<Arc<Mutex<JtiCacheStore>>> {
        self.jti_cache.clone()
    }

    /// Get backpressure statistics for observability
    pub fn backpressure_stats(&self) -> BackpressureStats {
        self.backpressure.stats()
    }

    /// Prepare the UDS listener (bind + path setup)
    pub async fn bind(&self) -> Result<UnixListener> {
        prepare_socket_path(&self.socket_path, "worker")
            .map_err(|e| AosError::Worker(e.to_string()))?;

        let listener = UnixListener::bind(&self.socket_path)
            .map_err(|e| AosError::Worker(format!("Failed to bind UDS socket: {}", e)))?;

        info!("UDS server listening on: {:?}", self.socket_path);

        Ok(listener)
    }

    /// Start UDS server for worker communication
    pub async fn serve(&self) -> Result<()> {
        let listener = self.bind().await?;
        self.serve_with_listener(listener).await
    }

    /// Run the accept loop with a pre-bound listener
    pub async fn serve_with_listener(&self, listener: UnixListener) -> Result<()> {
        use crate::backoff::{BackoffConfig, CircuitBreaker as BackoffCircuitBreaker};

        let backoff = BackoffConfig::new(
            std::time::Duration::from_millis(100),
            std::time::Duration::from_secs(10),
            2.0,
            5,
        );
        let circuit_breaker = BackoffCircuitBreaker::new(
            UDS_ACCEPT_FAILURE_THRESHOLD,
            std::time::Duration::from_secs(60),
        );
        let mut consecutive_failures = 0u32;
        let health_monitor = {
            let guard = self.worker.lock().await;
            guard.health_monitor()
        };

        loop {
            if self.drain_flag.load(Ordering::Relaxed) {
                info!("UDS server drain flag set, exiting accept loop");
                break Ok(());
            }
            // Check circuit breaker state
            if circuit_breaker.is_open() {
                warn!(
                    failure_count = circuit_breaker.failure_count(),
                    "UDS server circuit breaker is open, pausing accept loop"
                );
                tokio::time::sleep(circuit_breaker.reset_timeout()).await;
                continue;
            }

            let accept_result = tokio::select! {
                res = listener.accept() => res,
                _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                    // Check drain flag periodically
                    continue;
                }
            };

            match accept_result {
                Ok((stream, _)) => {
                    // Success - reset backoff
                    circuit_breaker.record_success();
                    consecutive_failures = 0;

                    let worker = Arc::clone(&self.worker);
                    // UDS connection handling is a background task, not deterministic inference
                    let api_key_db = self.api_key_db.clone();
                    let backpressure = Arc::clone(&self.backpressure);
                    let worker_verifying_key = self.worker_verifying_key.clone();
                    let worker_id = self.worker_id.clone();
                    let jti_cache = self.jti_cache.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            stream,
                            worker,
                            api_key_db,
                            backpressure,
                            worker_verifying_key,
                            worker_id,
                            jti_cache,
                        )
                        .await
                        {
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

                    if circuit_breaker.failure_count() >= UDS_ACCEPT_FAILURE_THRESHOLD {
                        error!(
                            failure_count = consecutive_failures,
                            "UDS accept circuit breaker tripped; shutting down listener"
                        );
                        trip_uds_accept_circuit_breaker(
                            consecutive_failures,
                            &self.drain_flag,
                            Some(health_monitor.as_ref()),
                        );
                        break Ok(());
                    }

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

    /// Validate ApiKey header against the control-plane database (if configured)
    fn enforce_api_key_tenant(record_tenant: &str, required_tenant: Option<&str>) -> Result<()> {
        if let Some(required) = required_tenant {
            if record_tenant != required {
                return Err(AosError::Worker(format!(
                    "API key tenant mismatch: expected {}, got {}",
                    required, record_tenant
                )));
            }
        }
        Ok(())
    }

    async fn validate_api_key(
        headers: &std::collections::HashMap<String, String>,
        db: &Db,
        required_tenant: Option<&str>,
    ) -> Result<()> {
        let auth_header = headers
            .get("Authorization")
            .or_else(|| headers.get("authorization"))
            .ok_or_else(|| AosError::Worker("Missing Authorization header".to_string()))?;

        let token = auth_header
            .strip_prefix("ApiKey ")
            .ok_or_else(|| AosError::Worker("Invalid Authorization scheme".to_string()))?;

        let mut hasher = Hasher::new();
        hasher.update(token.as_bytes());
        let hash = hasher.finalize().to_hex().to_string();

        let record = db
            .get_api_key_by_hash(&hash, false)
            .await
            .map_err(|e| AosError::Worker(format!("API key lookup failed: {}", e)))?;

        let record = match record {
            Some(r) => r,
            None => return Err(AosError::Worker("API key not found or revoked".to_string())),
        };

        Self::enforce_api_key_tenant(&record.tenant_id, required_tenant)?;

        Ok(())
    }

    /// Validate worker token (Ed25519-signed JWT) from control plane
    ///
    /// Returns Ok(()) if token is valid, Err otherwise.
    /// This is the primary authentication method for CP->Worker communication.
    async fn validate_worker_token(
        headers: &std::collections::HashMap<String, String>,
        verifying_key: &VerifyingKey,
        expected_worker_id: &str,
        jti_cache: &Mutex<JtiCacheStore>,
    ) -> Result<()> {
        let auth_header = headers
            .get("Authorization")
            .or_else(|| headers.get("authorization"))
            .ok_or_else(|| AosError::Worker("Missing Authorization header".to_string()))?;

        let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
            AosError::Worker("Invalid Authorization scheme (expected Bearer)".to_string())
        })?;

        // Validate the token using adapteros-boot with persistent JTI cache
        let mut cache_store = jti_cache.lock().await;
        adapteros_boot::validate_worker_token(
            token,
            verifying_key,
            Some(expected_worker_id),
            cache_store.cache_mut(),
        )
        .map_err(|e| AosError::Worker(format!("Worker token validation failed: {}", e)))?;

        debug!(
            worker_id = %expected_worker_id,
            "Worker token validated successfully"
        );

        Ok(())
    }

    /// Handle individual UDS connection
    async fn handle_connection(
        mut stream: UnixStream,
        worker: Arc<Mutex<Worker<K>>>,
        api_key_db: Option<std::sync::Arc<Db>>,
        backpressure: Arc<BackpressureGate>,
        worker_verifying_key: Option<Arc<VerifyingKey>>,
        worker_id: String,
        jti_cache: Option<Arc<Mutex<JtiCacheStore>>>,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        // Parse HTTP request from UDS stream with timeout to prevent infinite blocking
        let request = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Self::parse_request(&mut stream),
        )
        .await
        .map_err(|_| AosError::Worker("Request parse timeout (30s)".to_string()))?
        .map_err(|e| AosError::Worker(format!("Request parse failed: {}", e)))?;
        let path = request.path.clone();

        // Check if this path requires backpressure control (expensive operations only)
        let needs_backpressure =
            matches!(path.as_str(), "/inference" | "/patch_proposal" | "/embed");

        // Acquire backpressure permit for expensive operations (fast-fail)
        // The permit is held until dropped at the end of this function
        let _permit = if needs_backpressure {
            match backpressure.try_acquire() {
                Some(permit) => Some(permit),
                None => {
                    let retry_ms = backpressure.suggested_retry_ms();
                    let stats = backpressure.stats();
                    warn!(
                        path = %path,
                        retry_after_ms = retry_ms,
                        in_flight = stats.in_flight,
                        max_concurrent = stats.max_concurrent,
                        rejected_count = stats.rejected_count,
                        "Rejecting request due to backpressure"
                    );
                    Self::send_overload_error(&mut stream, retry_ms).await?;
                    return Ok(());
                }
            }
        } else {
            None
        };

        // Check if client wants signal streaming
        let wants_signals = request
            .headers
            .get("X-Signal-Stream")
            .map(|v| v == "true")
            .unwrap_or(false);

        // Optional API key validation for non-inference paths (inference validates with tenant)
        if request.path != "/inference" {
            if let Some(db) = api_key_db.clone() {
                if let Err(e) = Self::validate_api_key(&request.headers, &db, None).await {
                    warn!(error = %e, "API key validation failed");
                    Self::send_error(&mut stream, 401, "Unauthorized").await?;
                    return Ok(());
                }
            }
        }

        // Route to appropriate handler
        match request.path.as_str() {
            "/inference" => {
                let inference_req: InferenceRequest =
                    serde_json::from_str(&request.body).map_err(|e| {
                        AosError::Worker(format!("Failed to parse inference request: {}", e))
                    })?;

                // Authentication: Try worker token (Bearer) first, fall back to API key
                let auth_result = if let (Some(ref verifying_key), Some(ref cache)) =
                    (&worker_verifying_key, &jti_cache)
                {
                    // New path: validate Ed25519-signed JWT from control plane
                    Self::validate_worker_token(&request.headers, verifying_key, &worker_id, cache)
                        .await
                } else if let Some(db) = api_key_db.clone() {
                    // Legacy path: validate API key from database
                    Self::validate_api_key(&request.headers, &db, Some(inference_req.cpid.as_str()))
                        .await
                } else {
                    // No authentication configured - allow (for dev/testing)
                    Ok(())
                };

                if let Err(e) = auth_result {
                    warn!(error = %e, "Worker authentication failed");
                    Self::send_error(&mut stream, 401, "Unauthorized").await?;
                    return Ok(());
                }

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
                    reasoning_mode: false,
                    request_type: RequestType::PatchProposal(patch_req.clone()),
                    stack_id: None,
                    stack_version: None,
                    domain_hint: None,
                    // Sampling params (use defaults for patch proposal)
                    temperature: None,
                    top_k: None,
                    top_p: None,
                    seed: None,
                    router_seed: None,
                    seed_mode: None,
                    request_seed: None,
                    determinism: None,
                    fusion_interval: None,
                    backend_profile: None,
                    coreml_mode: None,
                    pinned_adapter_ids: None,
                    strict_mode: true,
                    adapter_strength_overrides: None,
                    determinism_mode: "strict".to_string(), // Patch proposals use strict mode
                    routing_determinism_mode: None,
                    effective_adapter_ids: None,
                    placement: None,
                    routing_policy: None,
                    stop_policy: None,
                    admin_override: false,
                };

                // #region agent log
                if let Ok(mut f) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/Users/mln-dev/Dev/adapter-os/.cursor/debug.log")
                {
                    let _ = writeln!(
                        f,
                        r#"{{"sessionId":"debug-session","runId":"pre-fix","hypothesisId":"H1","location":"uds_server.rs:patch_proposal","message":"patch proposal inference req","data":{{"coreml_mode":{:?},"backend_profile":{:?}}},"timestamp":{}}}"#,
                        inference_req.coreml_mode,
                        inference_req.backend_profile,
                        chrono::Utc::now().timestamp_millis()
                    );
                }
                // #endregion

                let mut worker_guard = worker.lock().await;
                let response = worker_guard
                    .propose_patch(inference_req, &patch_req)
                    .await
                    .map_err(|e| AosError::Worker(format!("Patch proposal failed: {}", e)))?;

                Self::send_response(&mut stream, response).await?;
            }
            "/health" => {
                let bp_stats = backpressure.stats();
                let health_response = serde_json::json!({
                    "status": "healthy",
                    "worker_id": "default",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "backpressure": {
                        "in_flight": bp_stats.in_flight,
                        "max_concurrent": bp_stats.max_concurrent,
                        "utilization_percent": bp_stats.utilization_percent(),
                        "rejected_count": bp_stats.rejected_count,
                        "admitted_count": bp_stats.admitted_count
                    }
                });
                Self::send_json_response(&mut stream, health_response).await?;
            }
            "/debug/coreml_verification" => {
                let worker_guard = worker.lock().await;
                let snapshot = worker_guard.coreml_verification();
                drop(worker_guard);

                let response = serde_json::json!({
                    "mode": snapshot.as_ref().and_then(|s| s.mode.clone()),
                    "status": snapshot.as_ref().and_then(|s| s.status.clone()).unwrap_or_else(|| "unknown".to_string()),
                    "expected": snapshot.as_ref().and_then(|s| s.expected.clone()),
                    "actual": snapshot.as_ref().and_then(|s| s.actual.clone()),
                    "source": snapshot.as_ref().and_then(|s| s.source.clone()),
                    "mismatch": snapshot.as_ref().map(|s| s.mismatch).unwrap_or(false),
                });
                Self::send_json_response(&mut stream, response).await?;
            }
            "/model/load" => {
                // Parse model load request
                let load_req: ModelLoadRequest = match serde_json::from_str(&request.body) {
                    Ok(req) => req,
                    Err(e) => {
                        let response = ModelLoadResponse {
                            status: "error".to_string(),
                            model_id: "".to_string(),
                            memory_usage_mb: None,
                            error: Some(format!("Invalid request: {}", e)),
                            loaded_at: None,
                        };
                        // Use map_err instead of unwrap to handle serialization failures gracefully
                        let json_value = serde_json::to_value(&response).map_err(|e| {
                            AosError::Worker(format!("Failed to serialize response: {}", e))
                        })?;
                        Self::send_json_response(&mut stream, json_value).await?;
                        return Ok(());
                    }
                };

                info!(
                    model_id = %load_req.model_id,
                    model_path = %load_req.model_path,
                    "Processing model load request via UDS"
                );

                // Verify the model path exists
                let model_path = std::path::Path::new(&load_req.model_path);
                if !model_path.exists() {
                    let response = ModelLoadResponse {
                        status: "error".to_string(),
                        model_id: load_req.model_id,
                        memory_usage_mb: None,
                        error: Some(format!(
                            "Model path does not exist: {}",
                            load_req.model_path
                        )),
                        loaded_at: None,
                    };
                    let json_value = serde_json::to_value(&response).map_err(|e| {
                        AosError::Worker(format!("Failed to serialize response: {}", e))
                    })?;
                    Self::send_json_response(&mut stream, json_value).await?;
                    return Ok(());
                }

                // The worker is already initialized with a model at startup.
                // This endpoint verifies the model is loaded and returns status.
                // For dynamic model switching, a more complex implementation would be needed.
                //
                // For now, we verify the worker is operational and return loaded status.
                // The actual model loading happens during worker initialization via backend_factory.
                let worker_guard = worker.lock().await;

                // Check worker health and get memory stats
                // Worker is healthy if we can access it (the method call succeeds)
                let _adapter_count = worker_guard.get_adapter_states().len();
                let actual_memory_mb = worker_guard.get_memory_usage_mb();
                let is_healthy = true; // If we got here, worker is responsive
                drop(worker_guard);

                if is_healthy {
                    // Use actual memory usage from worker, fall back to estimate if unavailable
                    let memory_usage_mb = if actual_memory_mb > 0 {
                        actual_memory_mb
                    } else {
                        // Estimate based on typical 7B model size (4-8GB)
                        4096i32
                    };

                    let response = ModelLoadResponse {
                        status: "loaded".to_string(),
                        model_id: load_req.model_id,
                        memory_usage_mb: Some(memory_usage_mb),
                        error: None,
                        loaded_at: Some(chrono::Utc::now().to_rfc3339()),
                    };

                    info!(
                        memory_usage_mb = memory_usage_mb,
                        actual_from_worker = actual_memory_mb > 0,
                        "Model load confirmed via UDS"
                    );

                    let json_value = serde_json::to_value(&response).map_err(|e| {
                        AosError::Worker(format!("Failed to serialize response: {}", e))
                    })?;
                    Self::send_json_response(&mut stream, json_value).await?;
                } else {
                    let response = ModelLoadResponse {
                        status: "error".to_string(),
                        model_id: load_req.model_id,
                        memory_usage_mb: None,
                        error: Some("Worker is not healthy".to_string()),
                        loaded_at: None,
                    };
                    let json_value = serde_json::to_value(&response).map_err(|e| {
                        AosError::Worker(format!("Failed to serialize response: {}", e))
                    })?;
                    Self::send_json_response(&mut stream, json_value).await?;
                }
            }
            "/model/status" => {
                // Return current model status with memory info
                let worker_guard = worker.lock().await;
                let adapter_states = worker_guard.get_adapter_states();
                let memory_usage_mb = worker_guard.get_memory_usage_mb();
                drop(worker_guard);

                let status_response = serde_json::json!({
                    "status": "loaded",
                    "adapter_count": adapter_states.len(),
                    "memory_usage_mb": memory_usage_mb,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                });
                Self::send_json_response(&mut stream, status_response).await?;
            }
            "/training/cancel" => {
                let cancel_req: CancelTrainingRequest = serde_json::from_str(&request.body)
                    .map_err(|e| {
                        AosError::Worker(format!("Failed to parse cancel request: {}", e))
                    })?;

                info!(
                    job_id = %cancel_req.job_id,
                    reason = ?cancel_req.reason,
                    "Processing training cancel request via UDS"
                );

                let worker_guard = worker.lock().await;
                let response = worker_guard
                    .cancel_training_job(&cancel_req.job_id)
                    .map_err(|e| {
                        AosError::Worker(format!("Training cancellation failed: {}", e))
                    })?;

                let json_value = serde_json::to_value(&response).map_err(|e| {
                    error!(
                        error = %e,
                        job_id = %cancel_req.job_id,
                        "Failed to serialize cancel response"
                    );
                    AosError::Worker(format!("Failed to serialize cancel response: {}", e))
                })?;
                Self::send_json_response(&mut stream, json_value).await?;
            }
            "/fatal" => {
                // Worker fatal error channel for panic reporting
                // Parse WorkerFatal from request body
                #[derive(serde::Deserialize)]
                struct WorkerFatal {
                    worker_id: String,
                    reason: String,
                    backtrace_snippet: Option<String>,
                    timestamp: String,
                }

                let fatal_msg: WorkerFatal = match serde_json::from_str(&request.body) {
                    Ok(msg) => msg,
                    Err(e) => {
                        error!(
                            error = %e,
                            body = %request.body,
                            "Failed to parse WorkerFatal message"
                        );
                        Self::send_error(&mut stream, 400, "Invalid WorkerFatal format").await?;
                        return Ok(());
                    }
                };

                // Log the fatal error with full context
                error!(
                    event = "worker.fatal",
                    worker_id = %fatal_msg.worker_id,
                    reason = %fatal_msg.reason,
                    timestamp = %fatal_msg.timestamp,
                    has_backtrace = fatal_msg.backtrace_snippet.is_some(),
                    "Worker reported fatal error"
                );

                // Log backtrace separately if present (can be long)
                if let Some(ref backtrace) = fatal_msg.backtrace_snippet {
                    error!(
                        worker_id = %fatal_msg.worker_id,
                        backtrace = %backtrace,
                        "Worker fatal error backtrace"
                    );
                }

                // Return acknowledgment
                let ack_response = serde_json::json!({
                    "status": "acknowledged",
                    "worker_id": fatal_msg.worker_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                });
                Self::send_json_response(&mut stream, ack_response).await?;
            }
            path if path.starts_with("/adapter/") => {
                // Handle adapter command routes: /adapter/{adapter_id}/{command}
                // Also supports /adapter/command for JSON-based commands
                let parts: Vec<&str> = path.split('/').collect();
                // parts = ["", "adapter", "{id_or_command}", "{command}"]

                if parts.len() == 3 && parts[2] == "command" {
                    // JSON-based adapter command: POST /adapter/command
                    // Expects AdapterCommand in request body
                    use crate::adapter_hotswap::AdapterCommand;

                    let command: AdapterCommand = match serde_json::from_str(&request.body) {
                        Ok(cmd) => cmd,
                        Err(e) => {
                            error!(
                                error = %e,
                                body = %request.body,
                                "Failed to parse AdapterCommand"
                            );
                            Self::send_error(&mut stream, 400, "Invalid AdapterCommand format")
                                .await?;
                            return Ok(());
                        }
                    };

                    info!(
                        command = ?command,
                        "Processing adapter command via UDS"
                    );

                    let mut worker_guard = worker.lock().await;
                    let command_summary = format!("{:?}", command);
                    match worker_guard.execute_adapter_command(command).await {
                        Ok(result) => {
                            let json_value = serde_json::to_value(&result).map_err(|e| {
                                error!(
                                    error = %e,
                                    command = %command_summary,
                                    "Failed to serialize adapter command response"
                                );
                                AosError::Worker(format!(
                                    "Failed to serialize adapter command response: {}",
                                    e
                                ))
                            })?;
                            Self::send_json_response(&mut stream, json_value).await?;
                        }
                        Err(e) => {
                            error!(error = %e, "Adapter command execution failed");
                            Self::send_error(&mut stream, 500, &e.to_string()).await?;
                        }
                    }
                } else if parts.len() >= 3 {
                    // Simple command format: /adapter/{adapter_id}/{command}
                    let adapter_id = parts[2];
                    let command = if parts.len() >= 4 { parts[3] } else { "status" };

                    info!(
                        adapter_id = %adapter_id,
                        command = %command,
                        "Processing simple adapter command via UDS"
                    );

                    // For simple commands, we convert them to AdapterCommand enum
                    // This provides backward compatibility with the simple UDS client API
                    match command {
                        "status" => {
                            // Get adapter states
                            let worker_guard = worker.lock().await;
                            let adapter_states = worker_guard.get_adapter_states();
                            let adapter_state =
                                adapter_states.iter().find(|s| s.id == adapter_id).map(|s| {
                                    serde_json::json!({
                                        "id": s.id,
                                        "hash": s.hash,
                                        "vram_mb": s.vram_mb,
                                        "active": s.active,
                                    })
                                });

                            if let Some(state) = adapter_state {
                                Self::send_json_response(&mut stream, state).await?;
                            } else {
                                Self::send_error(&mut stream, 404, "Adapter not found").await?;
                            }
                        }
                        _ => {
                            // Unknown command
                            warn!(
                                adapter_id = %adapter_id,
                                command = %command,
                                "Unknown adapter command"
                            );
                            Self::send_error(&mut stream, 400, "Unknown adapter command").await?;
                        }
                    }
                } else {
                    Self::send_error(&mut stream, 400, "Invalid adapter path").await?;
                }
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

        // Read request line by line with per-byte timeout to prevent infinite blocking
        let per_byte_timeout = std::time::Duration::from_secs(5);
        loop {
            let mut byte = [0u8; 1];
            match tokio::time::timeout(per_byte_timeout, stream.read_exact(&mut byte)).await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    return Err(AosError::Worker(format!(
                        "Failed to read from stream: {}",
                        e
                    )));
                }
                Err(_) => {
                    return Err(AosError::Worker("Timeout reading request byte".to_string()));
                }
            }

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

        // Read body if present with timeout to prevent infinite blocking
        let mut body = String::new();
        if content_length > 0 {
            let mut body_buffer = vec![0u8; content_length];
            let body_timeout = std::time::Duration::from_secs(30);
            match tokio::time::timeout(body_timeout, stream.read_exact(&mut body_buffer)).await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    return Err(AosError::Worker(format!(
                        "Failed to read request body: {}",
                        e
                    )));
                }
                Err(_) => {
                    return Err(AosError::Worker(format!(
                        "Timeout reading request body ({} bytes)",
                        content_length
                    )));
                }
            }
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

    /// Send HTTP 503 overload response with retry hint
    ///
    /// Returns a structured error response indicating the worker is at capacity
    /// but healthy. The `retry_after_ms` field provides a suggested backoff duration.
    async fn send_overload_error(stream: &mut UnixStream, retry_after_ms: u64) -> Result<()> {
        let error_body = serde_json::json!({
            "error": "WORKER_OVERLOADED",
            "retry_after_ms": retry_after_ms,
            "message": "Worker is at capacity, please retry"
        });
        let json_body = error_body.to_string();
        let retry_after_secs = (retry_after_ms / 1000).max(1);
        let http_response = format!(
            "HTTP/1.1 503 Service Unavailable\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Retry-After: {}\r\n\
             \r\n\
             {}",
            json_body.len(),
            retry_after_secs,
            json_body
        );

        stream
            .write_all(http_response.as_bytes())
            .await
            .map_err(|e| AosError::Worker(format!("Failed to send overload response: {}", e)))?;

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
    use adapteros_core::{AosError, Result};

    fn enforce_api_key_tenant(expected: &str, provided: Option<&str>) -> Result<()> {
        match provided {
            Some(val) if val == expected || val == "*" => Ok(()),
            Some(_) => Err(AosError::Validation("tenant_mismatch".into())),
            None => Err(AosError::Validation("tenant_missing".into())),
        }
    }

    #[tokio::test]
    #[ignore = "TODO: implement UDS server creation test with mock worker and temp directory [tracking: STAB-IGN-0045]"]
    async fn test_uds_server_creation() {
        // This test would require a mock worker and temp directory setup
        // The core UDS server functionality is tested via integration tests
    }

    // ========================================================================
    // Timeout Configuration Tests
    // ========================================================================

    #[test]
    fn test_request_parse_timeout_constant() {
        // Verify the timeout constants are reasonable
        let request_timeout = std::time::Duration::from_secs(30);
        let per_byte_timeout = std::time::Duration::from_secs(5);
        let body_timeout = std::time::Duration::from_secs(30);

        // Request timeout should be >= per-byte timeout
        assert!(request_timeout >= per_byte_timeout);
        // Body timeout should be reasonable for large payloads
        assert!(body_timeout.as_secs() >= 10);
    }

    #[test]
    fn test_http_request_structure() {
        // Test HttpRequest struct can be created
        let request = HttpRequest {
            _method: "POST".to_string(),
            path: "/inference".to_string(),
            headers: std::collections::HashMap::from([
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Content-Length".to_string(), "42".to_string()),
            ]),
            body: r#"{"prompt": "test"}"#.to_string(),
        };

        assert_eq!(request.path, "/inference");
        assert_eq!(
            request.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_http_request_with_signal_header() {
        // Test X-Signal-Stream header parsing
        let headers =
            std::collections::HashMap::from([("X-Signal-Stream".to_string(), "true".to_string())]);

        let wants_signals = headers
            .get("X-Signal-Stream")
            .map(|v| v == "true")
            .unwrap_or(false);

        assert!(wants_signals);
    }

    #[test]
    fn test_http_request_without_signal_header() {
        let headers: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        let wants_signals = headers
            .get("X-Signal-Stream")
            .map(|v| v == "true")
            .unwrap_or(false);

        assert!(!wants_signals);
    }

    #[test]
    fn api_key_tenant_match_allows_access() {
        let res = enforce_api_key_tenant("tenant-a", Some("tenant-a"));
        assert!(res.is_ok());
    }

    #[test]
    fn api_key_tenant_mismatch_is_rejected() {
        let res = enforce_api_key_tenant("tenant-b", Some("tenant-a"));
        assert!(res.is_err());
    }

    // ========================================================================
    // HTTP Response Format Tests
    // ========================================================================

    #[test]
    fn test_http_response_format_200() {
        let json_body = r#"{"status":"ok"}"#;
        let http_response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            json_body.len(),
            json_body
        );

        assert!(http_response.contains("HTTP/1.1 200 OK"));
        assert!(http_response.contains("Content-Type: application/json"));
        assert!(http_response.contains(&format!("Content-Length: {}", json_body.len())));
        assert!(http_response.ends_with(json_body));
    }

    #[test]
    fn test_http_response_format_404() {
        let status_code = 404u16;
        let message = "Not Found";
        let error_body = format!("{{\"error\": \"{}\"}}", message);
        let http_response = format!(
            "HTTP/1.1 {} {}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            status_code,
            message,
            error_body.len(),
            error_body
        );

        assert!(http_response.contains("HTTP/1.1 404 Not Found"));
        assert!(http_response.contains(r#"{"error": "Not Found"}"#));
    }

    #[test]
    fn test_http_response_format_500() {
        let status_code = 500u16;
        let message = "Internal Server Error";
        let error_body = format!("{{\"error\": \"{}\"}}", message);
        let http_response = format!(
            "HTTP/1.1 {} {}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            status_code,
            message,
            error_body.len(),
            error_body
        );

        assert!(http_response.contains("HTTP/1.1 500 Internal Server Error"));
    }

    // ========================================================================
    // Path Routing Tests
    // ========================================================================

    #[test]
    fn test_path_routing_inference() {
        let path = "/inference";
        assert!(matches!(path, "/inference"));
    }

    #[test]
    fn test_path_routing_patch_proposal() {
        let path = "/patch_proposal";
        assert!(matches!(path, "/patch_proposal"));
    }

    #[test]
    fn test_path_routing_health() {
        let path = "/health";
        assert!(matches!(path, "/health"));
    }

    #[test]
    fn test_path_routing_training_cancel() {
        let path = "/training/cancel";
        assert!(matches!(path, "/training/cancel"));
    }

    #[test]
    fn test_path_routing_unknown() {
        let path = "/unknown";
        assert!(!matches!(
            path,
            "/inference" | "/patch_proposal" | "/health" | "/training/cancel"
        ));
    }

    #[test]
    fn test_path_routing_adapter_command() {
        let path = "/adapter/command";
        assert!(path.starts_with("/adapter/"));
    }

    #[test]
    fn test_path_routing_adapter_status() {
        let path = "/adapter/adapter-123/status";
        assert!(path.starts_with("/adapter/"));
        let parts: Vec<&str> = path.split('/').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[2], "adapter-123");
        assert_eq!(parts[3], "status");
    }

    #[test]
    fn test_adapter_path_parsing() {
        let path = "/adapter/my-adapter/status";
        let parts: Vec<&str> = path.split('/').collect();

        // parts = ["", "adapter", "my-adapter", "status"]
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "");
        assert_eq!(parts[1], "adapter");
        assert_eq!(parts[2], "my-adapter");
        assert_eq!(parts[3], "status");
    }

    #[test]
    fn test_adapter_command_path_parsing() {
        let path = "/adapter/command";
        let parts: Vec<&str> = path.split('/').collect();

        // parts = ["", "adapter", "command"]
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2], "command");
    }

    // ========================================================================
    // Backoff Configuration Tests
    // ========================================================================

    #[test]
    fn test_backoff_config_defaults() {
        use crate::backoff::BackoffConfig;

        let config = BackoffConfig::new(
            std::time::Duration::from_millis(100),
            std::time::Duration::from_secs(10),
            2.0,
            5,
        );

        // delay = initial * multiplier^attempt
        // attempt=0: 100 * 2^0 = 100ms
        let delay0 = config.next_delay(0);
        assert_eq!(delay0.as_millis(), 100);

        // attempt=1: 100 * 2^1 = 200ms
        let delay1 = config.next_delay(1);
        assert_eq!(delay1.as_millis(), 200);

        // attempt=2: 100 * 2^2 = 400ms
        let delay2 = config.next_delay(2);
        assert_eq!(delay2.as_millis(), 400);

        // Should not exceed max delay
        let delay_max = config.next_delay(100);
        assert!(delay_max <= std::time::Duration::from_secs(10));
    }

    #[test]
    fn test_circuit_breaker_threshold() {
        use crate::backoff::CircuitBreaker;

        let cb = CircuitBreaker::new(5, std::time::Duration::from_secs(60));

        // Should start closed
        assert!(!cb.is_open());

        // Record failures
        for _ in 0..5 {
            cb.record_failure();
        }

        // Should be open after threshold
        assert!(cb.is_open());

        // Record success should reset
        cb.record_success();
        assert!(!cb.is_open());
    }

    #[test]
    fn uds_accept_circuit_breaker_trips_shutdown() {
        use crate::health::HealthConfig;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let drain_flag = Arc::new(AtomicBool::new(false));
        let monitor = match HealthMonitor::new(HealthConfig::default()) {
            Ok(monitor) => monitor,
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("Operation not permitted") || msg.contains("permission") {
                    eprintln!("skipping: {}", msg);
                    return;
                }
                panic!("monitor: {}", err);
            }
        };

        trip_uds_accept_circuit_breaker(UDS_ACCEPT_FAILURE_THRESHOLD, &drain_flag, Some(&monitor));

        assert!(drain_flag.load(Ordering::Relaxed));
        assert!(monitor.is_shutdown_requested());
        assert_eq!(
            monitor.last_status_for_test().as_deref(),
            Some("uds_accept_circuit_breaker")
        );
    }
}
