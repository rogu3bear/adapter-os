//! Model Server Implementation
//!
//! gRPC server for handling forward pass requests from workers.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};

use crate::activation_tracker::ActivationTracker;
use crate::adapter_cache::AdapterCache;
use crate::config::ModelServerConfig;
use crate::forward::{ForwardExecutor, ForwardPassRequest};
use crate::kv_cache::KvCacheManager;
use crate::proto;

/// RAII guard that decrements an atomic counter on drop.
struct ActiveRequestGuard(Arc<AtomicU64>);

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelServerStartupStatus {
    pub phase: String,
    pub attempt: u32,
    pub ready: bool,
    pub deterministic_seed: String,
    pub replay_gate_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Model Server state
pub struct ModelServer {
    /// Configuration
    config: ModelServerConfig,

    /// Forward pass executor
    executor: Arc<RwLock<ForwardExecutor>>,

    /// KV cache manager
    kv_cache: Arc<KvCacheManager>,

    /// Adapter cache
    adapter_cache: Arc<AdapterCache>,

    /// Activation tracker for hybrid strategy
    activation_tracker: Arc<ActivationTracker>,

    /// Server start time
    started_at: Instant,

    /// Request counter
    request_count: AtomicU64,

    /// Currently in-flight requests
    active_requests: Arc<AtomicU64>,

    /// Drain flag
    draining: AtomicBool,

    /// Startup gate status (deterministic boot + replay readiness)
    startup_status: Arc<RwLock<ModelServerStartupStatus>>,
}

impl ModelServer {
    /// Create a new model server
    pub fn new(config: ModelServerConfig) -> Self {
        let kv_cache = Arc::new(KvCacheManager::new(
            config.kv_cache_max_bytes,
            4096, // Default hidden_size, updated after model load
            32,   // Default num_layers, updated after model load
        ));

        let adapter_cache = Arc::new(AdapterCache::new(
            config.max_hot_adapters,
            None, // No memory limit for adapters
        ));

        let activation_tracker = Arc::new(ActivationTracker::new(config.hot_adapter_threshold));

        let executor = Arc::new(RwLock::new(ForwardExecutor::new(
            kv_cache.clone(),
            adapter_cache.clone(),
            32000, // Default vocab_size, updated after model load
            4096,  // Default hidden_size
            32,    // Default num_layers
        )));
        let deterministic_seed = adapteros_core::B3Hash::hash(
            config
                .model_path
                .as_ref()
                .map(|path| path.to_string_lossy().as_bytes().to_vec())
                .unwrap_or_default()
                .as_slice(),
        )
        .to_hex();
        let startup_status = Arc::new(RwLock::new(ModelServerStartupStatus {
            phase: "created".to_string(),
            attempt: 0,
            ready: false,
            deterministic_seed,
            replay_gate_ready: false,
            last_error: None,
        }));

        Self {
            config,
            executor,
            kv_cache,
            adapter_cache,
            activation_tracker,
            started_at: Instant::now(),
            request_count: AtomicU64::new(0),
            active_requests: Arc::new(AtomicU64::new(0)),
            draining: AtomicBool::new(false),
            startup_status,
        }
    }

    /// Load the model
    pub async fn load_model(&self, model_path: &std::path::Path) -> Result<(), Status> {
        self.load_model_with_recovery(model_path, 1).await
    }

    /// Load the model with retry, deterministic startup gating, and audit status.
    pub async fn load_model_with_recovery(
        &self,
        model_path: &std::path::Path,
        max_attempts: u32,
    ) -> Result<(), Status> {
        if !model_path.exists() {
            self.set_startup_phase(
                "load_failed",
                1,
                Some(format!(
                    "Model path does not exist: {}",
                    model_path.display()
                )),
            )
            .await;
            return Err(Status::invalid_argument(format!(
                "Model path does not exist: {}",
                model_path.display()
            )));
        }

        let attempts = max_attempts.max(1);
        for attempt in 1..=attempts {
            self.set_startup_phase("loading_model", attempt, None).await;
            let load_result = {
                let mut executor = self.executor.write().await;
                executor.load_model(model_path)
            };
            match load_result {
                Ok(()) => {
                    let mut status = self.startup_status.write().await;
                    status.phase = "model_loaded".to_string();
                    status.attempt = attempt;
                    status.ready = true;
                    status.replay_gate_ready = true;
                    status.last_error = None;
                    info!(
                        attempts = attempt,
                        model_path = %model_path.display(),
                        deterministic_seed = %status.deterministic_seed,
                        "Model server startup completed"
                    );
                    return Ok(());
                }
                Err(error) => {
                    let message = format!("Failed to load model: {}", error);
                    self.set_startup_phase("load_retry", attempt, Some(message.clone()))
                        .await;
                    if attempt < attempts {
                        let backoff = Duration::from_millis(250u64.saturating_mul(attempt as u64));
                        warn!(
                            attempt = attempt,
                            max_attempts = attempts,
                            backoff_ms = backoff.as_millis() as u64,
                            error = %error,
                            "Model load failed during startup, retrying"
                        );
                        tokio::time::sleep(backoff).await;
                        continue;
                    }
                    self.set_startup_phase("load_failed", attempt, Some(message.clone()))
                        .await;
                    return Err(Status::internal(message));
                }
            }
        }

        Err(Status::internal(
            "Model startup retry loop exited without a terminal result",
        ))
    }

    /// Start the gRPC server
    pub async fn serve(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let status = self.startup_status().await;
        if !status.ready || !status.replay_gate_ready {
            return Err(format!(
                "Model Server startup gate not ready (phase={}, ready={}, replay_ready={})",
                status.phase, status.ready, status.replay_gate_ready
            )
            .into());
        }

        let socket_path = &self.config.socket_path;

        // Clean up stale socket file from a previous run
        if socket_path.exists() {
            std::fs::remove_file(socket_path)?;
        }
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        info!(
            socket_path = %socket_path.display(),
            "Starting Model Server (UDS)"
        );

        let uds = tokio::net::UnixListener::bind(socket_path)?;
        let uds_stream = tokio_stream::wrappers::UnixListenerStream::new(uds);

        let service = proto::model_server_server::ModelServerServer::new(ModelServerService::new(
            self.clone(),
        ));

        tonic::transport::Server::builder()
            .add_service(service)
            .serve_with_incoming(uds_stream)
            .await?;

        Ok(())
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Get request count
    pub fn request_count(&self) -> u64 {
        self.request_count.load(Ordering::Relaxed)
    }

    /// Get active (in-flight) request count
    pub fn active_requests(&self) -> u64 {
        self.active_requests.load(Ordering::Relaxed)
    }

    /// Acquire an active-request guard (increments counter; decrements on drop)
    fn enter_request(&self) -> ActiveRequestGuard {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
        ActiveRequestGuard(Arc::clone(&self.active_requests))
    }

    /// Check if draining
    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Relaxed)
    }

    /// Start drain
    pub fn start_drain(&self) {
        self.draining.store(true, Ordering::Relaxed);
    }

    /// Snapshot startup status for health/readiness reporting.
    pub async fn startup_status(&self) -> ModelServerStartupStatus {
        self.startup_status.read().await.clone()
    }

    async fn set_startup_phase(&self, phase: &str, attempt: u32, last_error: Option<String>) {
        let mut status = self.startup_status.write().await;
        status.phase = phase.to_string();
        status.attempt = attempt;
        status.last_error = last_error;
        status.ready = phase == "model_loaded";
        status.replay_gate_ready = phase == "model_loaded";
    }
}

/// gRPC service implementation
pub struct ModelServerService {
    server: Arc<ModelServer>,
}

impl ModelServerService {
    pub fn new(server: Arc<ModelServer>) -> Self {
        Self { server }
    }
}

#[tonic::async_trait]
impl proto::model_server_server::ModelServer for ModelServerService {
    async fn forward(
        &self,
        request: Request<proto::ForwardRequest>,
    ) -> Result<Response<proto::ForwardResponse>, Status> {
        let startup = self.server.startup_status().await;
        if self.server.is_draining() || !startup.ready || !startup.replay_gate_ready {
            return Err(Status::unavailable(
                "Model server not ready (draining or startup gate incomplete)",
            ));
        }

        self.server.request_count.fetch_add(1, Ordering::Relaxed);
        let _active_guard = self.server.enter_request();
        let req = request.into_inner();

        // Convert to internal request
        let forward_request = ForwardPassRequest {
            session_id: req.session_id,
            input_ids: req.input_ids,
            position: req.position,
            max_seq_len: req.max_seq_len,
            adapter_ids: req.adapter_ids,
            adapter_gates_q15: req
                .adapter_gates_q15
                .into_iter()
                .map(|g| g as i16)
                .collect(),
            include_hidden_states: req.include_hidden_states,
            manifest_seed: if req.manifest_seed.is_empty() {
                None
            } else {
                Some(req.manifest_seed)
            },
        };

        // Track adapter activations
        if !forward_request.adapter_ids.is_empty() {
            self.server
                .activation_tracker
                .record_request(&forward_request.adapter_ids);
        }

        // Execute forward pass
        let executor = self.server.executor.read().await;
        let result = executor.forward(forward_request).map_err(|e| {
            error!(error = %e, "Forward pass failed");
            Status::internal(format!("Forward pass failed: {}", e))
        })?;

        Ok(Response::new(proto::ForwardResponse {
            logits: result.logits,
            position: result.position,
            hidden_states: result.hidden_states.unwrap_or_default(),
            kv_cache_hit: result.kv_cache_hit,
            cached_tokens: result.cached_tokens,
            forward_latency_ms: result.latency_ms,
        }))
    }

    type ForwardStreamStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::ForwardToken, Status>>;

    /// Streaming forward pass - returns single forward result as a stream.
    ///
    /// Note: Token-by-token generation is handled at the Worker layer, not here.
    /// The Model Server's job is single forward passes with KV cache management.
    /// This endpoint exists for API completeness but delegates to `forward()`.
    async fn forward_stream(
        &self,
        request: Request<proto::ForwardRequest>,
    ) -> Result<Response<Self::ForwardStreamStream>, Status> {
        // Delegate to single forward pass - generation loop is Worker's responsibility
        let response = self.forward(request).await?.into_inner();

        let (tx, rx) = tokio::sync::mpsc::channel(1);

        // Return forward result wrapped as final token
        tx.send(Ok(proto::ForwardToken {
            token_id: 0,
            text: String::new(),
            index: 0,
            is_final: true,
            final_response: Some(response),
        }))
        .await
        .map_err(|e| Status::internal(format!("Failed to send token: {}", e)))?;

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn health(
        &self,
        _request: Request<proto::HealthRequest>,
    ) -> Result<Response<proto::HealthResponse>, Status> {
        let startup = self.server.startup_status().await;
        let status = if self.server.is_draining() || !startup.ready || !startup.replay_gate_ready {
            proto::health_response::Status::Unhealthy
        } else {
            proto::health_response::Status::Healthy
        };

        Ok(Response::new(proto::HealthResponse {
            status: status.into(),
            message: if startup.ready {
                String::new()
            } else {
                startup
                    .last_error
                    .unwrap_or_else(|| format!("startup phase '{}'", startup.phase))
            },
            model_id: self.server.config.model_id.clone().unwrap_or_default(),
            uptime_seconds: self.server.uptime_secs(),
        }))
    }

    async fn status(
        &self,
        _request: Request<proto::StatusRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let kv_stats = self.server.kv_cache.stats();
        let adapter_stats = self.server.adapter_cache.stats();
        let activation_stats = self.server.activation_tracker.all_stats();

        // Get model memory from executor
        let model_memory_bytes = {
            let executor = self.server.executor.read().await;
            executor.model_memory_bytes()
        };

        Ok(Response::new(proto::StatusResponse {
            model_id: self.server.config.model_id.clone().unwrap_or_default(),
            model_path: self
                .server
                .config
                .model_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            model_memory_bytes,
            active_sessions: kv_stats.active_sessions as u32,
            kv_cache_bytes_used: kv_stats.used_bytes,
            kv_cache_bytes_total: kv_stats.max_bytes,
            loaded_adapters: adapter_stats.cached_adapters as u32,
            adapter_memory_bytes: adapter_stats.memory_bytes,
            adapter_stats: activation_stats
                .into_iter()
                .map(|s| proto::AdapterActivationStats {
                    adapter_id: s.adapter_id,
                    adapter_name: s.adapter_name,
                    activation_count: s.activation_count,
                    activation_rate: s.activation_rate,
                    is_hot: s.is_hot,
                })
                .collect(),
        }))
    }

    async fn warmup(
        &self,
        request: Request<proto::WarmupRequest>,
    ) -> Result<Response<proto::WarmupResponse>, Status> {
        let req = request.into_inner();

        // Create or update KV cache for the session
        let entry = self
            .server
            .kv_cache
            .get_or_create(&req.session_id, req.max_seq_len);

        let cached_tokens = entry.read().cached_tokens;

        Ok(Response::new(proto::WarmupResponse {
            success: true,
            cached_tokens,
            error: String::new(),
        }))
    }

    async fn drain(
        &self,
        request: Request<proto::DrainRequest>,
    ) -> Result<Response<proto::DrainResponse>, Status> {
        let req = request.into_inner();

        info!(grace_period_secs = req.grace_period_secs, "Starting drain");

        self.server.start_drain();

        let active_requests = self.server.active_requests() as u32;

        Ok(Response::new(proto::DrainResponse {
            accepted: true,
            active_requests,
        }))
    }

    async fn load_adapter(
        &self,
        request: Request<proto::LoadAdapterRequest>,
    ) -> Result<Response<proto::LoadAdapterResponse>, Status> {
        let req = request.into_inner();

        // Parse adapter weights from SafeTensors format
        let (lora_a, lora_b, scale) = parse_adapter_weights(&req.adapter_weights).map_err(|e| {
            error!(
                adapter_id = req.adapter_id,
                error = %e,
                "Failed to parse adapter weights"
            );
            Status::invalid_argument(format!("Failed to parse adapter weights: {}", e))
        })?;

        let result = self.server.adapter_cache.load(
            req.adapter_id,
            req.adapter_name.clone(),
            lora_a,
            lora_b,
            scale,
        );

        match result {
            Ok(_) => {
                // Register with activation tracker
                self.server
                    .activation_tracker
                    .register_adapter(req.adapter_id, req.adapter_name);

                if req.promote_to_hot {
                    // Force hot status - adapter is already loaded, just verify it exists
                    if let Some(_stats) = self.server.activation_tracker.get_stats(req.adapter_id) {
                        // Adapter is tracked, hot status is determined by being in cache
                    }
                }

                let stats = self.server.adapter_cache.stats();
                Ok(Response::new(proto::LoadAdapterResponse {
                    success: true,
                    is_hot: self.server.adapter_cache.contains(req.adapter_id),
                    memory_bytes: stats.memory_bytes,
                    error: String::new(),
                }))
            }
            Err(e) => Ok(Response::new(proto::LoadAdapterResponse {
                success: false,
                is_hot: false,
                memory_bytes: 0,
                error: e,
            })),
        }
    }

    async fn unload_adapter(
        &self,
        request: Request<proto::UnloadAdapterRequest>,
    ) -> Result<Response<proto::UnloadAdapterResponse>, Status> {
        let req = request.into_inner();

        let freed = self.server.adapter_cache.unload(req.adapter_id);
        self.server
            .activation_tracker
            .unregister_adapter(req.adapter_id);

        Ok(Response::new(proto::UnloadAdapterResponse {
            success: freed.is_some(),
            freed_bytes: freed.unwrap_or(0),
            error: String::new(),
        }))
    }

    async fn list_adapters(
        &self,
        _request: Request<proto::ListAdaptersRequest>,
    ) -> Result<Response<proto::ListAdaptersResponse>, Status> {
        let stats = self.server.activation_tracker.all_stats();

        Ok(Response::new(proto::ListAdaptersResponse {
            adapters: stats
                .into_iter()
                .map(|s| {
                    // Look up adapter in cache to get memory usage
                    let memory_bytes = self
                        .server
                        .adapter_cache
                        .get(s.adapter_id)
                        .map(|a| a.memory_bytes)
                        .unwrap_or(0);

                    proto::AdapterInfo {
                        adapter_id: s.adapter_id,
                        adapter_name: s.adapter_name,
                        memory_bytes,
                        is_hot: s.is_hot,
                        activation_count: s.activation_count,
                        activation_rate: s.activation_rate,
                    }
                })
                .collect(),
        }))
    }
}

/// Parse adapter weights from SafeTensors format or raw f32 bytes
///
/// Returns (lora_a, lora_b, scale)
///
/// Expected tensor format (matching AOS adapters):
/// - lora_a: [rank, hidden_dim] - down projection
/// - lora_b: [hidden_dim, rank] - up projection
///
/// Supports:
/// 1. SafeTensors: Uses safetensors crate for proper parsing
/// 2. Raw f32: Direct bytes where first half is lora_a, second half is lora_b
fn parse_adapter_weights(data: &[u8]) -> Result<(Vec<f32>, Vec<f32>, f32), String> {
    use safetensors::SafeTensors;

    if data.is_empty() {
        return Err("Empty adapter weights".to_string());
    }

    // Try SafeTensors format first
    match SafeTensors::deserialize(data) {
        Ok(tensors) => {
            // Extract lora_a tensor (try multiple naming conventions)
            let lora_a_tensor = tensors
                .tensor("lora_a")
                .or_else(|_| tensors.tensor("lora.a"))
                .map_err(|_| "Missing lora_a tensor in SafeTensors")?;

            // Extract lora_b tensor
            let lora_b_tensor = tensors
                .tensor("lora_b")
                .or_else(|_| tensors.tensor("lora.b"))
                .map_err(|_| "Missing lora_b tensor in SafeTensors")?;

            // Convert to f32
            let lora_a = tensor_to_f32_vec(&lora_a_tensor)?;
            let lora_b = tensor_to_f32_vec(&lora_b_tensor)?;

            // Log shape info for debugging
            debug!(
                lora_a_shape = ?lora_a_tensor.shape(),
                lora_b_shape = ?lora_b_tensor.shape(),
                lora_a_len = lora_a.len(),
                lora_b_len = lora_b.len(),
                "Parsed SafeTensors adapter weights"
            );

            Ok((lora_a, lora_b, 1.0))
        }
        Err(_) => {
            // Fallback to raw f32 format
            parse_raw_f32_weights(data)
        }
    }
}

/// Convert safetensors tensor data to f32 vec, handling F16, BF16, and F32 dtypes
///
/// Mirrors the implementation in adapteros-aos/src/single_file/loader.rs
fn tensor_to_f32_vec(tensor: &safetensors::tensor::TensorView<'_>) -> Result<Vec<f32>, String> {
    use safetensors::Dtype;

    match tensor.dtype() {
        Dtype::F16 => Ok(tensor
            .data()
            .chunks(2)
            .map(|chunk| {
                let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                half::f16::from_bits(bits).to_f32()
            })
            .collect()),
        Dtype::F32 => Ok(tensor
            .data()
            .chunks(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()),
        Dtype::BF16 => Ok(tensor
            .data()
            .chunks(2)
            .map(|chunk| {
                let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                half::bf16::from_bits(bits).to_f32()
            })
            .collect()),
        other => Err(format!("Unsupported tensor dtype: {:?}", other)),
    }
}

/// Parse raw f32 bytes (fallback format)
fn parse_raw_f32_weights(data: &[u8]) -> Result<(Vec<f32>, Vec<f32>, f32), String> {
    if !data.len().is_multiple_of(4) {
        return Err(format!(
            "Invalid raw weights length: {} (not multiple of 4)",
            data.len()
        ));
    }

    let floats: Vec<f32> = data
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    if floats.is_empty() {
        return Err("Empty weights after parsing".to_string());
    }

    // Split in half: first half is lora_a, second half is lora_b
    let mid = floats.len() / 2;
    let lora_a = floats[..mid].to_vec();
    let lora_b = floats[mid..].to_vec();

    debug!(
        lora_a_len = lora_a.len(),
        lora_b_len = lora_b.len(),
        "Parsed raw f32 adapter weights"
    );

    Ok((lora_a, lora_b, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_server_creation() {
        let config = ModelServerConfig::default();
        let server = ModelServer::new(config);

        assert!(!server.is_draining());
        assert_eq!(server.request_count(), 0);
    }

    #[test]
    fn test_drain() {
        let config = ModelServerConfig::default();
        let server = ModelServer::new(config);

        assert!(!server.is_draining());
        server.start_drain();
        assert!(server.is_draining());
    }

    #[test]
    fn test_parse_raw_f32_weights() {
        // 8 floats: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
        let data: Vec<u8> = (1..=8u32)
            .flat_map(|i| (i as f32).to_le_bytes().to_vec())
            .collect();

        let (lora_a, lora_b, scale) = parse_adapter_weights(&data).unwrap();

        assert_eq!(lora_a.len(), 4);
        assert_eq!(lora_b.len(), 4);
        assert!((scale - 1.0).abs() < 1e-6);
        assert!((lora_a[0] - 1.0).abs() < 1e-6);
        assert!((lora_b[0] - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_empty_weights() {
        let result = parse_adapter_weights(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_length_weights() {
        // 5 bytes - not divisible by 4
        let result = parse_adapter_weights(&[1, 2, 3, 4, 5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_safetensors_format() {
        use safetensors::serialize;
        use safetensors::tensor::TensorView;

        // Create test tensors matching AOS format:
        // lora_a: [rank=2, hidden_dim=4] = 8 elements
        // lora_b: [hidden_dim=4, rank=2] = 8 elements
        let lora_a: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let lora_b: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];

        // Convert to bytes
        let lora_a_bytes: Vec<u8> = lora_a.iter().flat_map(|f| f.to_le_bytes()).collect();
        let lora_b_bytes: Vec<u8> = lora_b.iter().flat_map(|f| f.to_le_bytes()).collect();

        // Create tensor views
        let tensors = vec![
            (
                "lora_a",
                TensorView::new(safetensors::Dtype::F32, vec![2, 4], &lora_a_bytes).unwrap(),
            ),
            (
                "lora_b",
                TensorView::new(safetensors::Dtype::F32, vec![4, 2], &lora_b_bytes).unwrap(),
            ),
        ];

        // Serialize to safetensors format
        let serialized = serialize(tensors, None).unwrap();

        // Parse it back
        let (parsed_a, parsed_b, scale) = parse_adapter_weights(&serialized).unwrap();

        assert_eq!(parsed_a.len(), 8);
        assert_eq!(parsed_b.len(), 8);
        assert!((scale - 1.0).abs() < 1e-6);
        assert!((parsed_a[0] - 1.0).abs() < 1e-6);
        assert!((parsed_b[0] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn test_parse_safetensors_alternate_names() {
        use safetensors::serialize;
        use safetensors::tensor::TensorView;

        // Test with "lora.a" / "lora.b" naming (alternate convention)
        let lora_a: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        let lora_b: Vec<f32> = vec![5.0, 6.0, 7.0, 8.0];

        let lora_a_bytes: Vec<u8> = lora_a.iter().flat_map(|f| f.to_le_bytes()).collect();
        let lora_b_bytes: Vec<u8> = lora_b.iter().flat_map(|f| f.to_le_bytes()).collect();

        let tensors = vec![
            (
                "lora.a",
                TensorView::new(safetensors::Dtype::F32, vec![2, 2], &lora_a_bytes).unwrap(),
            ),
            (
                "lora.b",
                TensorView::new(safetensors::Dtype::F32, vec![2, 2], &lora_b_bytes).unwrap(),
            ),
        ];

        let serialized = serialize(tensors, None).unwrap();
        let (parsed_a, parsed_b, _) = parse_adapter_weights(&serialized).unwrap();

        assert_eq!(parsed_a.len(), 4);
        assert_eq!(parsed_b.len(), 4);
    }
}
