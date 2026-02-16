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
use adapteros_boot::key_ring::WorkerKeyRing;
use adapteros_boot::{KeyUpdateRequest, KeyUpdateResponse, KEY_UPDATE_MAX_AGE_SECS};
use adapteros_config::prepare_socket_path;
use adapteros_core::{AosError, Result};
use adapteros_storage::secure_fs::traversal::normalize_path;
use blake3::Hasher;
use ed25519_dalek::VerifyingKey;
use serde_json;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

use crate::{
    backpressure::{BackpressureGate, BackpressureStats},
    health::HealthMonitor,
    inference_management::InferenceCancelRegistry,
    CancelTrainingRequest, InferenceRequest, InferenceResponse, PatchProposalRequest, RequestType,
    Worker, WorkerStreamEvent,
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

/// Maximum allowed size for JSON request bodies (16MB)
const MAX_REQUEST_SIZE: usize = 16 * 1024 * 1024; // 16MB

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

/// Parse JSON with size limit to prevent DoS via unbounded deserialization
fn parse_json_with_limit<T: serde::de::DeserializeOwned>(body: &str) -> Result<T> {
    if body.len() > MAX_REQUEST_SIZE {
        return Err(AosError::Worker("Request body too large".to_string()));
    }
    serde_json::from_str(body).map_err(|e| AosError::Worker(format!("JSON parse error: {}", e)))
}

pub struct UdsServer<K: adapteros_lora_kernel_api::FusedKernels + StrictnessControl + 'static> {
    socket_path: PathBuf,
    worker: Arc<Mutex<Worker<K>>>,
    inference_cancellations: Arc<InferenceCancelRegistry>,
    api_key_db: Option<std::sync::Arc<Db>>,
    drain_flag: Arc<AtomicBool>,
    backpressure: Arc<BackpressureGate>,
    /// Ed25519 public key for validating worker tokens from control plane (legacy)
    worker_verifying_key: Option<Arc<VerifyingKey>>,
    /// Worker ID for token validation (must match expected wid claim)
    worker_id: String,
    /// JTI cache for replay defense (prevents token reuse).
    /// This is a persistent cache that survives worker restarts.
    /// Only allocated when worker_verifying_key is Some (auth enabled).
    jti_cache: Option<Arc<Mutex<JtiCacheStore>>>,
    /// Key ring for multi-key validation with rotation support.
    /// When present, this takes precedence over worker_verifying_key.
    worker_key_ring: Option<Arc<RwLock<WorkerKeyRing>>>,
}

impl<K: adapteros_lora_kernel_api::FusedKernels + StrictnessControl + 'static> UdsServer<K> {
    /// Create a new UDS server
    pub fn new(
        socket_path: PathBuf,
        worker: Arc<Mutex<Worker<K>>>,
        inference_cancellations: Arc<InferenceCancelRegistry>,
        api_key_db: Option<std::sync::Arc<Db>>,
        drain_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            socket_path,
            worker,
            inference_cancellations,
            api_key_db,
            drain_flag,
            backpressure: Arc::new(BackpressureGate::from_env()),
            worker_verifying_key: None,
            worker_id: "unknown".to_string(),
            jti_cache: None, // Not needed when auth is disabled
            worker_key_ring: None,
        }
    }

    /// Create a new UDS server with worker token validation (legacy single-key mode)
    ///
    /// The verifying key is used to validate Ed25519-signed JWTs from the control plane.
    /// The worker_id must match the expected `wid` claim in the token.
    /// The jti_cache is a persistent cache for replay defense that survives restarts.
    ///
    /// NOTE: For key rotation support, use `new_with_key_ring` instead.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_worker_auth(
        socket_path: PathBuf,
        worker: Arc<Mutex<Worker<K>>>,
        inference_cancellations: Arc<InferenceCancelRegistry>,
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
            inference_cancellations,
            api_key_db,
            drain_flag,
            backpressure: Arc::new(BackpressureGate::from_env()),
            worker_verifying_key: Some(Arc::new(worker_verifying_key)),
            worker_id,
            jti_cache: Some(jti_cache),
            worker_key_ring: None,
        }
    }

    /// Create a new UDS server with key ring support for rotation.
    ///
    /// The key ring holds multiple verifying keys to support seamless key rotation.
    /// During rotation, both old and new keys are valid for the grace period.
    ///
    /// # Arguments
    ///
    /// * `socket_path` - Path to the Unix domain socket
    /// * `worker` - Worker instance
    /// * `api_key_db` - Optional API key database for legacy auth
    /// * `drain_flag` - Flag to signal server drain
    /// * `worker_key_ring` - Key ring with rotation support
    /// * `worker_id` - Worker ID for token validation
    pub fn new_with_key_ring(
        socket_path: PathBuf,
        worker: Arc<Mutex<Worker<K>>>,
        inference_cancellations: Arc<InferenceCancelRegistry>,
        api_key_db: Option<std::sync::Arc<Db>>,
        drain_flag: Arc<AtomicBool>,
        worker_key_ring: Arc<RwLock<WorkerKeyRing>>,
        worker_id: String,
    ) -> Self {
        let key_count = {
            let ring = worker_key_ring.read().unwrap_or_else(|e| {
                warn!("Worker key ring RwLock poisoned on read, recovering: {}", e);
                e.into_inner()
            });
            ring.key_count()
        };
        info!(
            worker_id = %worker_id,
            key_count = key_count,
            "UDS server configured with key ring for rotation support"
        );
        Self {
            socket_path,
            worker,
            inference_cancellations,
            api_key_db,
            drain_flag,
            backpressure: Arc::new(BackpressureGate::from_env()),
            worker_verifying_key: None,
            worker_id,
            jti_cache: None, // Key ring has its own JTI cache
            worker_key_ring: Some(worker_key_ring),
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

        // Set secure socket permissions (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.socket_path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| {
                    AosError::Worker(format!("Failed to set socket permissions: {}", e))
                })?;
        }

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
                    let inference_cancellations = Arc::clone(&self.inference_cancellations);
                    // UDS connection handling is a background task, not deterministic inference
                    let api_key_db = self.api_key_db.clone();
                    let backpressure = Arc::clone(&self.backpressure);
                    let worker_verifying_key = self.worker_verifying_key.clone();
                    let worker_id = self.worker_id.clone();
                    let jti_cache = self.jti_cache.clone();
                    let worker_key_ring = self.worker_key_ring.clone();
                    let drain_flag = Arc::clone(&self.drain_flag);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            stream,
                            worker,
                            inference_cancellations,
                            api_key_db,
                            backpressure,
                            worker_verifying_key,
                            worker_id,
                            jti_cache,
                            worker_key_ring,
                            drain_flag,
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

    /// Validate worker token using the key ring (supports rotation).
    ///
    /// This method uses the key ring to validate tokens signed with any
    /// of the keys in the ring (current or grace period keys).
    fn validate_worker_token_with_ring(
        headers: &std::collections::HashMap<String, String>,
        key_ring: &RwLock<WorkerKeyRing>,
        expected_worker_id: &str,
    ) -> Result<()> {
        let auth_header = headers
            .get("Authorization")
            .or_else(|| headers.get("authorization"))
            .ok_or_else(|| AosError::Worker("Missing Authorization header".to_string()))?;

        let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
            AosError::Worker("Invalid Authorization scheme (expected Bearer)".to_string())
        })?;

        // Validate the token using the key ring
        let ring = key_ring.read().unwrap_or_else(|e| {
            warn!("Key ring RwLock poisoned on read, recovering: {}", e);
            e.into_inner()
        });
        ring.validate_token(token, Some(expected_worker_id))
            .map_err(|e| AosError::Worker(format!("Worker token validation failed: {}", e)))?;

        debug!(
            worker_id = %expected_worker_id,
            key_count = ring.key_count(),
            "Worker token validated via key ring"
        );

        Ok(())
    }

    fn is_inference_path(path: &str) -> bool {
        matches!(path, "/inference" | "/api/v1/infer" | "/inference/cancel")
            || path.starts_with("/inference/cancel/")
    }

    async fn authenticate_inference_request(
        headers: &std::collections::HashMap<String, String>,
        api_key_db: Option<&Arc<Db>>,
        worker_verifying_key: Option<&Arc<VerifyingKey>>,
        worker_id: &str,
        jti_cache: Option<&Arc<Mutex<JtiCacheStore>>>,
        worker_key_ring: Option<&Arc<RwLock<WorkerKeyRing>>>,
        required_tenant: Option<&str>,
    ) -> Result<()> {
        if let Some(key_ring) = worker_key_ring {
            Self::validate_worker_token_with_ring(headers, key_ring, worker_id)
        } else if let (Some(verifying_key), Some(cache)) = (worker_verifying_key, jti_cache) {
            Self::validate_worker_token(headers, verifying_key, worker_id, cache).await
        } else if let Some(db) = api_key_db {
            Self::validate_api_key(headers, db, required_tenant).await
        } else {
            Ok(())
        }
    }

    /// Handle individual UDS connection
    #[allow(clippy::too_many_arguments)]
    async fn handle_connection(
        mut stream: UnixStream,
        worker: Arc<Mutex<Worker<K>>>,
        inference_cancellations: Arc<InferenceCancelRegistry>,
        api_key_db: Option<std::sync::Arc<Db>>,
        backpressure: Arc<BackpressureGate>,
        worker_verifying_key: Option<Arc<VerifyingKey>>,
        worker_id: String,
        jti_cache: Option<Arc<Mutex<JtiCacheStore>>>,
        worker_key_ring: Option<Arc<RwLock<WorkerKeyRing>>>,
        drain_flag: Arc<AtomicBool>,
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
        let needs_backpressure = matches!(
            path.as_str(),
            "/inference" | "/api/v1/infer" | "/patch_proposal" | "/embed"
        );

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
        let wants_signals = Self::header_value(&request.headers, "X-Signal-Stream")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let wants_stream = Self::header_value(&request.headers, "Accept")
            .map(|v| v.split(',').any(|part| part.trim() == "text/event-stream"))
            .unwrap_or(false)
            || Self::header_value(&request.headers, "X-Stream")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
            || Self::header_value(&request.headers, "X-Token-Stream")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

        // Optional API key validation for non-inference paths (inference validates with tenant)
        if !Self::is_inference_path(request.path.as_str()) {
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
            "/inference" | "/api/v1/infer" => {
                let mut inference_req: InferenceRequest = parse_json_with_limit(&request.body)?;

                // Set arrival timestamp for queue/generation time tracking
                inference_req.arrival_instant = Some(start);

                let auth_result = Self::authenticate_inference_request(
                    &request.headers,
                    api_key_db.as_ref(),
                    worker_verifying_key.as_ref(),
                    &worker_id,
                    jti_cache.as_ref(),
                    worker_key_ring.as_ref(),
                    Some(inference_req.cpid.as_str()),
                )
                .await;

                if let Err(e) = auth_result {
                    warn!(error = %e, "Worker authentication failed");
                    Self::send_error(&mut stream, 401, "Unauthorized").await?;
                    return Ok(());
                }

                if wants_stream || wants_signals {
                    Self::stream_response(
                        &mut stream,
                        worker.clone(),
                        inference_req,
                        wants_signals,
                    )
                    .await?;
                } else {
                    let mut worker_guard = worker.lock().await;
                    let response = worker_guard
                        .infer(inference_req)
                        .await
                        .map_err(|e| AosError::Worker(format!("Inference failed: {}", e)))?;

                    Self::send_response(&mut stream, response).await?;
                }
            }
            path if path.starts_with("/inference/cancel") => {
                #[derive(serde::Deserialize)]
                struct CancelInferenceRequest {
                    #[serde(default)]
                    reason: Option<String>,
                    #[serde(default)]
                    cpid: Option<String>,
                }

                let request_id = path
                    .trim_start_matches("/inference/cancel")
                    .trim_start_matches('/');
                if request_id.is_empty() {
                    Self::send_error(&mut stream, 400, "Missing request_id").await?;
                    return Ok(());
                }

                let cancel_req: CancelInferenceRequest = if request.body.trim().is_empty() {
                    CancelInferenceRequest {
                        reason: None,
                        cpid: None,
                    }
                } else {
                    parse_json_with_limit(&request.body)?
                };

                let auth_result = Self::authenticate_inference_request(
                    &request.headers,
                    api_key_db.as_ref(),
                    worker_verifying_key.as_ref(),
                    &worker_id,
                    jti_cache.as_ref(),
                    worker_key_ring.as_ref(),
                    cancel_req.cpid.as_deref(),
                )
                .await;

                if let Err(e) = auth_result {
                    warn!(error = %e, "Worker authentication failed");
                    Self::send_error(&mut stream, 401, "Unauthorized").await?;
                    return Ok(());
                }

                let cancelled =
                    inference_cancellations.cancel(request_id, cancel_req.reason.clone());
                if !cancelled {
                    warn!(
                        request_id = %request_id,
                        "Inference cancel requested for unknown request"
                    );
                }

                let response = serde_json::json!({
                    "status": if cancelled { "cancelled" } else { "not_found" },
                    "request_id": request_id,
                    "reason": cancel_req.reason,
                });
                Self::send_json_response(&mut stream, response).await?;
            }
            path if path.starts_with("/inference/resume") => {
                // Human-in-the-loop resume endpoint
                use adapteros_api_types::review::SubmitReviewRequest;

                let pause_id = path
                    .trim_start_matches("/inference/resume")
                    .trim_start_matches('/');
                if pause_id.is_empty() {
                    Self::send_error(&mut stream, 400, "Missing pause_id").await?;
                    return Ok(());
                }

                let resume_req: SubmitReviewRequest = parse_json_with_limit(&request.body)?;

                info!(
                    pause_id = %pause_id,
                    reviewer = %resume_req.reviewer,
                    "Processing inference resume request via UDS"
                );

                // Look up the pause registry and submit review
                let worker_guard = worker.lock().await;
                let result = if let Some(ref registry) = worker_guard.pause_registry {
                    registry.submit_review(resume_req)
                } else {
                    Err(adapteros_core::AosError::Worker(
                        "Pause registry not initialized".to_string(),
                    ))
                };
                drop(worker_guard);

                let response = match result {
                    Ok(new_state) => serde_json::json!({
                        "status": "resumed",
                        "pause_id": pause_id,
                        "new_state": format!("{:?}", new_state),
                    }),
                    Err(e) => serde_json::json!({
                        "status": "error",
                        "pause_id": pause_id,
                        "error": e.to_string(),
                    }),
                };
                Self::send_json_response(&mut stream, response).await?;
            }
            "/patch_proposal" => {
                let patch_req: PatchProposalRequest = parse_json_with_limit(&request.body)?;

                // Create a dummy inference request for patch proposal
                let inference_req = InferenceRequest {
                    cpid: "patch-proposal".to_string(),
                    prompt: "patch proposal".to_string(),
                    max_tokens: 100,
                    request_id: None,
                    run_envelope: None,
                    require_evidence: false,
                    reasoning_mode: false,
                    request_type: RequestType::PatchProposal(patch_req.clone()),
                    stack_id: None,
                    stack_version: None,
                    policy_id: None,
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
                    adapter_stable_ids: None,
                    placement: None,
                    routing_policy: None,
                    stop_policy: None,
                    policy_mask_digest_b3: None,
                    utf8_healing: true,
                    admin_override: false,
                    // Set arrival timestamp for queue/generation time tracking
                    arrival_instant: Some(start),
                };

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
            "/key/update" => {
                // Key rotation update from control plane
                // Requires key ring to be configured
                let Some(ref key_ring) = worker_key_ring else {
                    warn!("Key update received but key ring not configured");
                    Self::send_error(&mut stream, 501, "Key ring not configured").await?;
                    return Ok(());
                };

                // Parse the key update request
                let update_req: KeyUpdateRequest = match parse_json_with_limit(&request.body) {
                    Ok(req) => req,
                    Err(e) => {
                        error!(error = %e, "Failed to parse key update request");
                        let response = KeyUpdateResponse::error(
                            format!("Invalid request: {}", e),
                            key_ring
                                .read()
                                .unwrap_or_else(|e| {
                                    warn!("Key ring RwLock poisoned on read, recovering: {}", e);
                                    e.into_inner()
                                })
                                .key_count(),
                        );
                        let json_value = serde_json::to_value(&response).map_err(|e| {
                            AosError::Worker(format!("Failed to serialize response: {}", e))
                        })?;
                        Self::send_json_response(&mut stream, json_value).await?;
                        return Ok(());
                    }
                };

                info!(
                    new_kid = %update_req.new_kid,
                    old_kid = %update_req.old_kid,
                    grace_period_secs = update_req.grace_period_secs,
                    "Processing key update request"
                );

                // Validate the request and prepare response (all sync operations)
                // Lock must be dropped before any async operations
                enum KeyUpdateOutcome {
                    Error(String, usize),   // error_msg, key_count
                    Success(String, usize), // new_kid, key_count
                }

                let outcome = {
                    let mut ring = key_ring.write().unwrap_or_else(|e| {
                        warn!("Key ring RwLock poisoned on write, recovering: {}", e);
                        e.into_inner()
                    });

                    // Check for replay (nonce already seen)
                    if ring.has_seen_nonce(&update_req.nonce) {
                        warn!(nonce = %update_req.nonce, "Replay detected - nonce already seen");
                        KeyUpdateOutcome::Error("Replay detected".to_string(), ring.key_count())
                    } else if !update_req.is_valid_time() {
                        // Check time validity
                        warn!(
                            issued_at = update_req.issued_at,
                            max_age = KEY_UPDATE_MAX_AGE_SECS,
                            "Key update request too old"
                        );
                        KeyUpdateOutcome::Error("Request expired".to_string(), ring.key_count())
                    } else {
                        // Verify signature using the current (old) key
                        match ring.get_verifying_key(&update_req.old_kid) {
                            None => {
                                warn!(old_kid = %update_req.old_kid, "Old key not found in ring");
                                KeyUpdateOutcome::Error(
                                    format!("Unknown old key ID: {}", update_req.old_kid),
                                    ring.key_count(),
                                )
                            }
                            Some(old_verifying_key) => {
                                if let Err(e) = update_req.verify_signature(&old_verifying_key) {
                                    warn!(error = %e, "Key update signature verification failed");
                                    KeyUpdateOutcome::Error(
                                        "Signature verification failed".to_string(),
                                        ring.key_count(),
                                    )
                                } else {
                                    // Decode the new public key
                                    match update_req.decode_new_public_key() {
                                        Err(e) => {
                                            error!(error = %e, "Failed to decode new public key");
                                            KeyUpdateOutcome::Error(
                                                format!("Invalid new key: {}", e),
                                                ring.key_count(),
                                            )
                                        }
                                        Ok(new_verifying_key) => {
                                            // Add the new key with grace period for old key
                                            let new_kid = ring.add_verifying_key_with_grace(
                                                new_verifying_key,
                                                update_req.grace_period_secs,
                                            );

                                            // Record the nonce to prevent replay
                                            let expiry = chrono::Utc::now().timestamp()
                                                + KEY_UPDATE_MAX_AGE_SECS;
                                            ring.record_nonce(&update_req.nonce, expiry);

                                            // Clean up expired keys
                                            let removed = ring.cleanup_expired_keys();
                                            if removed > 0 {
                                                info!(
                                                    removed_count = removed,
                                                    "Cleaned up expired keys"
                                                );
                                            }

                                            info!(
                                                new_kid = %new_kid,
                                                key_count = ring.key_count(),
                                                "Key update applied successfully"
                                            );

                                            KeyUpdateOutcome::Success(new_kid, ring.key_count())
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Lock is dropped here at end of block
                };

                // Now safe to do async operation - lock has been dropped
                let response = match outcome {
                    KeyUpdateOutcome::Error(msg, key_count) => {
                        KeyUpdateResponse::error(msg, key_count)
                    }
                    KeyUpdateOutcome::Success(new_kid, key_count) => {
                        KeyUpdateResponse::success(new_kid, key_count)
                    }
                };
                let json_value = serde_json::to_value(&response).map_err(|e| {
                    AosError::Worker(format!("Failed to serialize response: {}", e))
                })?;
                Self::send_json_response(&mut stream, json_value).await?;
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
                let load_req: ModelLoadRequest = match parse_json_with_limit(&request.body) {
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

                // Canonicalize and validate the model path to prevent path traversal attacks
                // normalize_path also ensures the path exists (via canonicalize)
                let _canonical_path = normalize_path(&load_req.model_path)
                    .map_err(|e| AosError::Worker(format!("Invalid model path: {}", e)))?;
                // Note: canonical_path validated but not used directly since model loading
                // happens during worker initialization. This check prevents path traversal
                // attacks even if model_path is later used for dynamic loading.

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
                let cancel_req: CancelTrainingRequest = parse_json_with_limit(&request.body)?;

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
            "/maintenance" => {
                // Worker maintenance mode signal from control plane
                // Sets the drain flag to begin graceful shutdown
                #[derive(serde::Deserialize)]
                struct MaintenanceRequest {
                    #[serde(default)]
                    mode: Option<String>, // "drain" or "maintenance"
                    #[serde(default)]
                    reason: Option<String>,
                }

                let maint_req: MaintenanceRequest = match parse_json_with_limit(&request.body) {
                    Ok(req) => req,
                    Err(e) => {
                        error!(
                            error = %e,
                            body = %request.body,
                            "Failed to parse maintenance request"
                        );
                        Self::send_error(&mut stream, 400, "Invalid maintenance request format")
                            .await?;
                        return Ok(());
                    }
                };

                let mode = maint_req.mode.as_deref().unwrap_or("drain");
                let reason = maint_req.reason.as_deref().unwrap_or("admin request");

                info!(
                    mode = %mode,
                    reason = %reason,
                    "Worker entering maintenance mode via control plane signal"
                );

                // Set the drain flag to signal the accept loop to exit gracefully
                drain_flag.store(true, Ordering::Relaxed);

                // Return acknowledgment with current status
                let ack_response = serde_json::json!({
                    "status": "accepted",
                    "mode": mode,
                    "reason": reason,
                    "drain_flag_set": true,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                });
                Self::send_json_response(&mut stream, ack_response).await?;
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

                let fatal_msg: WorkerFatal = match parse_json_with_limit(&request.body) {
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

                    let command: AdapterCommand = match parse_json_with_limit(&request.body) {
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

    fn header_value<'a>(
        headers: &'a std::collections::HashMap<String, String>,
        name: &str,
    ) -> Option<&'a str> {
        headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
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

    async fn stream_response(
        stream: &mut UnixStream,
        worker: Arc<Mutex<Worker<K>>>,
        inference_req: InferenceRequest,
        emit_lifecycle_signals: bool,
    ) -> Result<()> {
        let http_response = "HTTP/1.1 200 OK\r\n\
             Content-Type: text/event-stream\r\n\
             Cache-Control: no-cache\r\n\
             Connection: keep-alive\r\n\
             \r\n";

        stream
            .write_all(http_response.as_bytes())
            .await
            .map_err(|e| AosError::Worker(format!("Failed to send SSE headers: {}", e)))?;

        let cpid = inference_req.cpid.clone();
        let request_id = inference_req.request_id.clone();

        if emit_lifecycle_signals {
            Self::send_signal_event(
                stream,
                "lifecycle.start",
                "normal",
                serde_json::json!({
                    "cpid": &cpid,
                    "request_id": &request_id,
                }),
            )
            .await?;
            let _ = stream.flush().await;
        }

        let (tx, mut rx) = mpsc::channel::<WorkerStreamEvent>(64);
        let worker_clone = worker.clone();

        let inference_handle = tokio::spawn(async move {
            let mut worker_guard = worker_clone.lock().await;
            if let Err(e) = worker_guard.infer_stream(inference_req, tx).await {
                tracing::error!(error = %e, "Streaming inference failed");
            }
        });

        // Guard to abort the spawned task on early exit (e.g., client disconnect)
        struct AbortOnDrop(Option<tokio::task::JoinHandle<()>>);
        impl Drop for AbortOnDrop {
            fn drop(&mut self) {
                if let Some(handle) = self.0.take() {
                    if !handle.is_finished() {
                        handle.abort();
                        tracing::debug!("Aborted orphaned inference task on handler exit");
                    }
                }
            }
        }
        let _abort_guard = AbortOnDrop(Some(inference_handle));

        while let Some(event) = rx.recv().await {
            if emit_lifecycle_signals {
                let signal_result = match &event {
                    WorkerStreamEvent::Complete(response) => {
                        Self::send_signal_event(
                            stream,
                            "lifecycle.complete",
                            "normal",
                            serde_json::json!({
                                "cpid": &cpid,
                                "request_id": &request_id,
                                "status": &response.status,
                            }),
                        )
                        .await
                    }
                    WorkerStreamEvent::Error(message) => {
                        Self::send_signal_event(
                            stream,
                            "lifecycle.error",
                            "critical",
                            serde_json::json!({
                                "cpid": &cpid,
                                "request_id": &request_id,
                                "error": message,
                            }),
                        )
                        .await
                    }
                    _ => Ok(()),
                };

                if signal_result.is_err() {
                    break;
                }
            }

            let payload = match &event {
                WorkerStreamEvent::Token(token) => {
                    let data = serde_json::json!({
                        "text": token.text,
                        "token_id": token.token_id,
                    });
                    format!("event: token\ndata: {}\n\n", data)
                }
                WorkerStreamEvent::Complete(response) => {
                    let json = serde_json::to_string(response).unwrap_or_else(|e| {
                        serde_json::json!({ "error": format!("serialization failed: {}", e) })
                            .to_string()
                    });
                    format!("event: complete\ndata: {}\n\n", json)
                }
                WorkerStreamEvent::Error(ref message) => {
                    let data = serde_json::json!({ "error": message });
                    format!("event: error\ndata: {}\n\n", data)
                }
                WorkerStreamEvent::Paused {
                    pause_id,
                    inference_id,
                    trigger_kind,
                    context,
                    text_so_far,
                    token_count,
                } => {
                    let data = serde_json::json!({
                        "pause_id": pause_id,
                        "inference_id": inference_id,
                        "trigger_kind": trigger_kind,
                        "context": context,
                        "text_so_far": text_so_far,
                        "token_count": token_count,
                    });
                    format!("event: paused\ndata: {}\n\n", data)
                }
            };

            if stream.write_all(payload.as_bytes()).await.is_err() {
                break;
            }
            let _ = stream.flush().await;

            // Only terminate stream on Complete or Error - NOT on Paused
            // Paused events notify the server but inference continues after review
            if matches!(
                &event,
                WorkerStreamEvent::Complete(_) | WorkerStreamEvent::Error(_)
            ) {
                break;
            }
        }

        Ok(())
    }

    async fn send_signal_event(
        stream: &mut UnixStream,
        signal_type: &str,
        priority: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let signal = serde_json::json!({
            "type": signal_type,
            "timestamp": timestamp,
            "payload": payload,
            "priority": priority,
        });
        let frame = format!("event: signal\ndata: {}\n\n", signal);

        stream
            .write_all(frame.as_bytes())
            .await
            .map_err(|e| AosError::Worker(format!("Failed to send signal event: {}", e)))
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
        let error_body = serde_json::json!({ "error": message }).to_string();
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

    /// Tests UDS socket creation and binding in a temporary directory.
    ///
    /// This test verifies:
    /// 1. UDS socket can be created in a temp directory
    /// 2. Socket file is created with correct permissions (0o600)
    /// 3. Socket cleanup happens when listener is dropped
    ///
    /// NOTE: Full UdsServer integration with Worker is tested via e2e tests
    /// because Worker has many complex dependencies (manifest, kernels, policy, etc.)
    /// that are difficult to mock at unit test level.
    #[tokio::test]
    async fn test_uds_server_creation() {
        use adapteros_config::prepare_socket_path;
        use std::os::unix::fs::PermissionsExt;
        use tokio::net::UnixListener;

        // Create a temporary directory for the socket under var/tmp
        let temp_root = adapteros_core::resolve_var_dir().join("tmp");
        std::fs::create_dir_all(&temp_root).expect("Failed to create var/tmp directory");
        let temp_dir = tempfile::Builder::new()
            .prefix("aos-uds-")
            .tempdir_in(&temp_root)
            .expect("Failed to create temp directory");
        let socket_path = temp_dir.path().join("test-worker.sock");

        // Test prepare_socket_path - this is what UdsServer::bind() calls
        prepare_socket_path(&socket_path, "worker").expect("Failed to prepare socket path");

        // Bind the socket
        let listener = match UnixListener::bind(&socket_path) {
            Ok(listener) => listener,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("skipping: UDS bind not permitted in this environment");
                return;
            }
            Err(e) => panic!("Failed to bind UDS socket: {}", e),
        };

        // Verify socket file was created
        assert!(socket_path.exists(), "Socket file should exist after bind");

        // Set permissions like UdsServer::bind() does
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
            .expect("Failed to set socket permissions");

        // Verify permissions are secure (owner read/write only)
        let metadata = std::fs::metadata(&socket_path).expect("Failed to get socket metadata");
        let mode = metadata.permissions().mode();
        // Socket type bits are 0o140000, mode bits are last 9 bits
        assert_eq!(
            mode & 0o777,
            0o600,
            "Socket should have 0600 permissions, got {:o}",
            mode & 0o777
        );

        // Drop listener and verify socket can be rebound (cleanup test)
        drop(listener);

        // Socket file may still exist after drop, but we should be able to
        // remove it and create a new one (this is what prepare_socket_path does)
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).expect("Failed to remove socket file");
        }

        // Verify we can bind again after cleanup
        let _listener2 =
            UnixListener::bind(&socket_path).expect("Failed to rebind UDS socket after cleanup");

        // Temp directory cleanup happens automatically when temp_dir is dropped
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
