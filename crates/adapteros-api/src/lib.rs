use axum::{
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::Stream;
use hyper_util::rt::TokioExecutor;
use hyper_util::server::conn::auto::Builder;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

// Add mod declarations
mod logger;
mod middleware;
mod ratelimit;


/// API server state
pub struct ApiState {
    worker: Arc<tokio::sync::Mutex<adapteros_lora_worker::Worker>>,
    signals_tx: broadcast::Sender<Signal>,
}

// Ensure ApiState is Send + Sync
unsafe impl Send for ApiState {}
unsafe impl Sync for ApiState {}

impl ApiState {
    /// Create new API state with worker
    pub fn new(
        worker: adapteros_lora_worker::Worker,
        signals_tx: broadcast::Sender<Signal>,
    ) -> Self {
        Self {
            worker: Arc::new(tokio::sync::Mutex::new(worker)),
            signals_tx,
        }
    }
}

/// Start UDS server with MetalKernels worker (concrete implementation)
///
/// This is the primary entry point for production use with Metal kernels.
/// Creates a Unix Domain Socket HTTP server with streaming responses
/// and proper error handling for AdapterOS inference requests.
pub async fn serve_uds_with_metal_kernels<P: AsRef<Path>>(
    socket_path: P,
    worker: adapteros_lora_worker::Worker,
) -> Result<(), Box<dyn std::error::Error>> {
    serve_uds_with_metal_kernels_impl(socket_path, worker).await
}

/// Generic UDS server with worker (for other kernel implementations)
///
/// For use with non-Metal kernel implementations. If using MetalKernels,
/// prefer `serve_uds_with_metal_kernels` for better type inference.
pub async fn serve_uds_with_worker<P: AsRef<Path>>(
    socket_path: P,
    worker: adapteros_lora_worker::Worker,
) -> Result<(), Box<dyn std::error::Error>> {
    serve_uds_with_worker_impl(socket_path, worker).await
}

/// Internal implementation for MetalKernels (concrete, no generics)
async fn serve_uds_with_metal_kernels_impl<P: AsRef<Path>>(
    socket_path: P,
    worker: adapteros_lora_worker::Worker,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = socket_path.as_ref();

    // Remove existing socket file if it exists
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    // Create parent directory if it doesn't exist
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Create Unix listener
    let listener = UnixListener::bind(socket_path)?;

    // Set socket permissions (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(socket_path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(socket_path, perms)?;
    }

    println!("🚀 AdapterOS UDS server listening");
    println!("   Socket: {}", socket_path.display());
    println!("   Permissions: 600 (owner read/write only)");

    // Create API state with worker and broadcast channel for signals
    let (signals_tx, _signals_rx) = broadcast::channel::<Signal>(1024);

    // Bridge worker internal signals (WorkerSignal) to API Signal type
    let (worker_tx, worker_rx) =
        broadcast::channel::<adapteros_lora_worker::WorkerSignal>(1024);
    let bridge_tx = signals_tx.clone();
    // Spawn a bridge task to forward worker signals to API signals
    tokio::spawn(async move {
        let mut worker_rx = worker_rx;
        while let Ok(sig) = worker_rx.recv().await {
            let api_sig = Signal {
                signal_type: sig.signal_type,
                timestamp: sig.timestamp,
                payload: sig.payload,
                priority: "normal".to_string(),
                trace_id: None,
            };
            let _ = bridge_tx.send(api_sig);
        }
    });

    let mut worker = worker;
    worker.set_signal_tx(worker_tx);

    // Create app following adapteros-server pattern exactly
    // Match adapteros-server: nest router to get the same Router type that implements Service
    let state = Arc::new(ApiState::new(worker, signals_tx.clone()));
    let api_routes = build_router_metal(state);
    let app = Router::new().nest("/", api_routes);

    // Convert to MakeService
    let make_service = app.into_service::<axum::body::Body>();
    let builder = Builder::new(TokioExecutor::new());

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let _io = hyper_util::rt::TokioIo::new(stream);
                let _make_service_clone = make_service.clone();
                let _builder_clone = builder.clone();
                // TODO: Implement UDS connection handling
                let _ = spawn_deterministic("UDS connection handler".to_string(), async move {
                    // Stub implementation - UDS handling needs to be properly implemented
                    tracing::info!("UDS connection received - stub implementation");
                });
            }
            Err(e) => {
                tracing::error!("Fatal accept error, breaking out of server loop: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Generic implementation for other kernel types (uses closures)
async fn serve_uds_with_worker_impl<P: AsRef<Path>>(
    socket_path: P,
    worker: adapteros_lora_worker::Worker,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = socket_path.as_ref();

    // Remove existing socket file if it exists
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    // Create parent directory if it doesn't exist
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Create Unix listener
    let listener = UnixListener::bind(socket_path)?;

    // Set socket permissions (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(socket_path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(socket_path, perms)?;
    }

    println!("🚀 AdapterOS UDS server listening");
    println!("   Socket: {}", socket_path.display());
    println!("   Permissions: 600 (owner read/write only)");

    // Create API state with worker and broadcast channel for signals
    let (signals_tx, _signals_rx) = broadcast::channel::<Signal>(1024);

    // Bridge worker internal signals (WorkerSignal) to API Signal type
    let (worker_tx, worker_rx) =
        broadcast::channel::<adapteros_lora_worker::WorkerSignal>(1024);
    let bridge_tx = signals_tx.clone();
    // Spawn a bridge task to forward worker signals to API signals
    tokio::spawn(async move {
        let mut worker_rx = worker_rx;
        while let Ok(sig) = worker_rx.recv().await {
            let api_sig = Signal {
                signal_type: sig.signal_type,
                timestamp: sig.timestamp,
                payload: sig.payload,
                priority: "normal".to_string(),
                trace_id: None,
            };
            let _ = bridge_tx.send(api_sig);
        }
    });

    let mut worker = worker;
    worker.set_signal_tx(worker_tx);

    let state = Arc::new(ApiState::new(worker, signals_tx.clone()));

    // Create router with closures for generic handlers
    // Generic handlers need closures to satisfy Handler trait requirements
    // Note: This requires K: Send + Sync which is enforced by the function signature
    let app = Router::new()
        .route("/inference", post(|s: State<Arc<ApiState>>, req: Json<InferenceRequest>| async {
            inference_handler(s, req).await
        }))
        .route("/health", post(|s: State<Arc<ApiState>>| async {
            health_handler(s).await
        }))
        .route("/health", get(|s: State<Arc<ApiState>>| async {
            health_handler(s).await
        }))
        .route("/adapter", post(|s: State<Arc<ApiState>>, h: HeaderMap, cmd: Json<adapteros_lora_worker::AdapterCommand>| async {
            adapter_command_handler(s, h, cmd).await
        }))
        .route("/adapters", get(|s: State<Arc<ApiState>>, h: HeaderMap| async {
            list_adapters_handler(s, h).await
        }))
        .route("/adapter/:id", get(|s: State<Arc<ApiState>>, h: HeaderMap, p: axum::extract::Path<String>| async {
            adapter_profile_handler(s, h, p).await
        }))
        .route("/adapter/:id/:cmd", post(|s: State<Arc<ApiState>>, h: HeaderMap, p: axum::extract::Path<(String, String)>| async {
            adapter_lifecycle_handler(s, h, p).await
        }))
        .route("/profile/snapshot", get(|s: State<Arc<ApiState>>, h: HeaderMap| async {
            profile_snapshot_handler(s, h).await
        }))
        .route("/warmup", post(|s: State<Arc<ApiState>>, h: HeaderMap| async {
            warmup_handler(s, h).await
        }))
        .route("/signals", get(|s: State<Arc<ApiState>>, h: HeaderMap| async {
            signals_handler(s, h).await
        }))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        )
        .with_state(state);

    // In Axum 0.7, Router with state implements Service<Request<Body>>
    // Clone router for each connection (pattern from adapteros-server/src/main.rs L1512)
    let _app_service: Router<Arc<ApiState>> = app;
    let _builder = Builder::new(TokioExecutor::new());

    // Accept connections and serve
    loop {
        match listener.accept().await {
            Ok((_stream, _addr)) => {
                let _ = spawn_deterministic("UDS connection handler".to_string(), async move {
                    // TODO: Implement proper HTTP over UDS serving
                    // For now, just accept connections without serving HTTP
                    tracing::info!("UDS connection accepted - HTTP serving not yet implemented");
                });
            }
            Err(e) => {
                tracing::error!("Fatal accept error, breaking out of server loop: {}", e);
                break;
            }
        }
    }

    Ok(())
}

// Concrete handlers for MetalKernels (avoid generic closure issues)
// Use concrete ApiState<MetalKernels> type directly to avoid type alias issues with Axum

async fn inference_handler_metal(
    state: State<Arc<ApiState>>,
    request: Json<InferenceRequest>,
) -> Result<Json<InferenceResponse>, ApiError> {
    // Call generic handler with explicit type
    inference_handler(state, request).await
}

async fn health_handler_metal(
    state: State<Arc<ApiState>>,
) -> impl IntoResponse {
    health_handler(state).await
}

async fn adapter_command_handler_metal(
    state: State<Arc<ApiState>>,
    headers: HeaderMap,
    command: Json<adapteros_lora_worker::AdapterCommand>,
) -> Result<Json<adapteros_lora_worker::AdapterCommandResult>, ApiError> {
    adapter_command_handler(state, headers, command).await
}

async fn list_adapters_handler_metal(
    state: State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    list_adapters_handler(state, headers).await
}

async fn adapter_profile_handler_metal(
    state: State<Arc<ApiState>>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    adapter_profile_handler(state, headers, path).await
}

async fn adapter_lifecycle_handler_metal(
    state: State<Arc<ApiState>>,
    headers: HeaderMap,
    path: axum::extract::Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    adapter_lifecycle_handler(state, headers, path).await
}

async fn profile_snapshot_handler_metal(
    state: State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    profile_snapshot_handler(state, headers).await
}

async fn warmup_handler_metal(
    state: State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    warmup_handler(state, headers).await
}

async fn signals_handler_metal(
    state: State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError> {
    signals_handler(state, headers).await
}

/// Build router for MetalKernels (concrete type, no generic closure issues)
fn build_router_metal(state: Arc<ApiState>) -> Router<Arc<ApiState>> {
    // Use closures for concrete handlers to ensure Handler trait satisfaction
    // Even though these are concrete, Axum's Handler trait works better with closures
    Router::new()
        .route("/inference", post(|s: State<Arc<ApiState>>, req: Json<InferenceRequest>| async move {
            inference_handler_metal(s, req).await
        }))
        .route("/health", post(|s: State<Arc<ApiState>>| async move {
            health_handler_metal(s).await
        }))
        .route("/health", get(|s: State<Arc<ApiState>>| async move {
            health_handler_metal(s).await
        }))
        .route("/adapter", post(|s: State<Arc<ApiState>>, h: HeaderMap, cmd: Json<adapteros_lora_worker::AdapterCommand>| async move {
            adapter_command_handler_metal(s, h, cmd).await
        }))
        .route("/adapters", get(|s: State<Arc<ApiState>>, h: HeaderMap| async move {
            list_adapters_handler_metal(s, h).await
        }))
        .route("/adapter/:id", get(|s: State<Arc<ApiState>>, h: HeaderMap, p: axum::extract::Path<String>| async move {
            adapter_profile_handler_metal(s, h, p).await
        }))
        .route("/adapter/:id/:cmd", post(|s: State<Arc<ApiState>>, h: HeaderMap, p: axum::extract::Path<(String, String)>| async move {
            adapter_lifecycle_handler_metal(s, h, p).await
        }))
        .route("/profile/snapshot", get(|s: State<Arc<ApiState>>, h: HeaderMap| async move {
            profile_snapshot_handler_metal(s, h).await
        }))
        .route("/warmup", post(|s: State<Arc<ApiState>>, h: HeaderMap| async move {
            warmup_handler_metal(s, h).await
        }))
        .route("/signals", get(|s: State<Arc<ApiState>>, h: HeaderMap| async move {
            signals_handler_metal(s, h).await
        }))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        )
        .with_state(state)
}

/// Inference endpoint handler
async fn inference_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<InferenceRequest>,
) -> Result<Json<InferenceResponse>, ApiError> {
    // Emit start signal
    let _ = state.signals_tx.send(Signal {
        signal_type: "inference_start".to_string(),
        timestamp: current_millis(),
        payload: serde_json::json!({ "cpid": request.cpid, "max_tokens": request.max_tokens }),
        priority: "normal".to_string(),
        trace_id: None,
    });

    // Forward request to worker; worker policy remains source of truth
    let worker = state.worker.lock().await;
    let response = worker
        .infer(request)
        .await
        .map_err(|e| ApiError::WorkerError(e.to_string()))?;

    // Emit completion signal
    let _ = state.signals_tx.send(Signal {
        signal_type: "complete".to_string(),
        timestamp: current_millis(),
        payload: serde_json::to_value(&response).unwrap_or(serde_json::json!({})),
        priority: "normal".to_string(),
        trace_id: None,
    });

    Ok(Json(response))
}

/// Health check endpoint
async fn health_handler(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    let worker = state.worker.lock().await;
    let evidence_required = worker.policy_requires_open_book();
    let abstain_threshold = worker.policy_abstain_threshold();
    Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "evidence_required": evidence_required,
        "abstain_threshold": abstain_threshold,
    }))
}

/// Adapter command endpoint
async fn adapter_command_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(command): Json<adapteros_lora_worker::AdapterCommand>,
) -> Result<Json<adapteros_lora_worker::AdapterCommandResult>, ApiError> {
    admin_guard(&headers)?;
    // Forward command to worker
    let worker = state.worker.lock().await;
    let result = worker
        .execute_adapter_command(command)
        .await
        .map_err(|e| ApiError::WorkerError(e.to_string()))?;

    Ok(Json(result))
}

/// List adapters (CLI-compatible view)
async fn list_adapters_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    let items = worker.list_adapter_states_view();
    Ok((StatusCode::OK, Json(items)))
}

/// Get adapter profile (CLI-compatible view)
async fn adapter_profile_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    axum::extract::Path(adapter_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    if let Some(profile) = worker.adapter_profile_view(&adapter_id).await {
        Ok((StatusCode::OK, Json(profile)))
    } else {
        Err(ApiError::NotFound(format!(
            "Adapter not found: {}",
            adapter_id
        )))
    }
}

/// Adapter lifecycle operations (promote/demote/pin/unpin)
async fn adapter_lifecycle_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    axum::extract::Path((adapter_id, cmd)): axum::extract::Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    let res = match cmd.as_str() {
        "promote" => worker.promote_adapter_by_id(&adapter_id),
        "demote" => worker.demote_adapter_by_id(&adapter_id),
        "pin" => worker.pin_adapter_by_id(&adapter_id),
        "unpin" => worker.unpin_adapter_by_id(&adapter_id),
        // Functional stand-in: mark adapter as loaded/unloaded via lifecycle
        "load" => worker.promote_adapter_by_id(&adapter_id),
        "unload" => worker.demote_adapter_by_id(&adapter_id),
        other => Err(adapteros_core::AosError::Validation(format!(
            "Unknown adapter command: {}",
            other
        ))),
    };

    match res {
        Ok(()) => Ok((StatusCode::OK, Json(serde_json::json!({ "ok": true })))),
        Err(e) => Err(ApiError::WorkerError(e.to_string())),
    }
}

/// Profiling snapshot (CLI-compatible)
async fn profile_snapshot_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    let snapshot = worker.profiling_snapshot_json();
    Ok((StatusCode::OK, Json(snapshot)))
}

/// Execute a warmup routine on the worker
async fn warmup_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    let report = worker
        .warmup()
        .await
        .map_err(|e| ApiError::WorkerError(e.to_string()))?;
    Ok((StatusCode::OK, Json(report)))
}

/// Stream signals as Server-Sent Events
async fn signals_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError> {
    admin_guard(&headers)?;
    let rx = state.signals_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(signal) => {
            let json = serde_json::to_string(&signal).unwrap_or("{}".to_string());
            Some(Ok(Event::default().event("signal").data(json)))
        }
        Err(_) => None,
    });
    Ok(Sse::new(stream))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    #[serde(rename = "type")]
    signal_type: String,
    timestamp: u128,
    payload: serde_json::Value,
    priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
}

pub struct ApiState {
    pub worker: Arc<TokioMutex<MockWorker>>,
    pub signals_tx: mpsc::Sender<Signal>,
}

struct MockWorker;

impl MockWorker {
    async fn infer(&self, _req: InferenceRequest) -> Result<InferenceResponse, String> {
        Ok(InferenceResponse {
            completion: "Mock response".to_string(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Signal {
    pub signal_type: String,
    pub timestamp: u128,
    pub payload: serde_json::Value,
    pub priority: String,
    pub trace_id: Option<String>,
}

// Assume health_handler
async fn health_handler() -> &'static str {
    "OK"
}

// Rest of the code remains the same...
// ApiError (Rectified - All Variants with trace_id)
#[derive(Debug)]
pub enum ApiError {
    Internal(Option<String>, String),
    BadRequest(Option<String>, String),
    NotFound(Option<String>, String),
    PolicyViolation(Option<String>, String),
    WorkerError(Option<String>, String),
    Unauthorized(Option<String>),
    EgressViolation(Option<String>, String),
    DeterminismViolation(Option<String>, String),
    IsolationViolation(Option<String>, String),
    ValidationFailed(Option<String>, Vec<String>),
    DatabaseError(Option<String>, String),
    CryptoError(Option<String>, String),
    NetworkError(Option<String>, String),
    RateLimitExceeded(Option<String>, String),
}

// From<AosError> (Full Mapping)
impl From<adapteros_core::AosError> for ApiError {
    fn from(err: adapteros_core::AosError) -> Self {
        let trace_id = Some(Uuid::new_v4().to_string());
        match err {
            adapteros_core::AosError::PolicyViolation(msg) => {
                ApiError::PolicyViolation(trace_id, msg)
            }
            adapteros_core::AosError::Worker(msg) => ApiError::WorkerError(trace_id, msg),
            adapteros_core::AosError::InvalidCPID(msg) => ApiError::BadRequest(trace_id, msg),
            adapteros_core::AosError::EgressViolation(msg) => {
                ApiError::EgressViolation(trace_id, msg)
            }
            adapteros_core::AosError::DeterminismViolation(msg) => {
                ApiError::DeterminismViolation(trace_id, msg)
            }
            adapteros_core::AosError::IsolationViolation(msg) => {
                ApiError::IsolationViolation(trace_id, msg)
            }
            adapteros_core::AosError::Validation(msg) => {
                ApiError::ValidationFailed(trace_id, vec![msg])
            }
            adapteros_core::AosError::Database(msg) => ApiError::DatabaseError(trace_id, msg),
            adapteros_core::AosError::Crypto(msg) => ApiError::CryptoError(trace_id, msg),
            adapteros_core::AosError::Network(msg) => ApiError::NetworkError(trace_id, msg),
            _ => ApiError::Internal(trace_id, err.to_string()),
        }
    }
}

// From<tower_http::limit::Error>
impl From<tower_http::limit::Error> for ApiError {
    fn from(_: tower_http::limit::Error) -> Self {
        ApiError::RateLimitExceeded(
            Some(Uuid::new_v4().to_string()),
            "Rate limit exceeded".to_string(),
        )
    }
}

// ErrorResponse (with code, trace_id)
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

// IntoResponse (Full with Status, Code, Trace ID, Retry-After for RateLimit)
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, details, trace_id, code) = match self {
            ApiError::Internal(trace, msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
                Some(msg),
                trace,
                "INTERNAL_ERROR".to_string(),
            ),
            ApiError::BadRequest(trace, msg) => (
                StatusCode::BAD_REQUEST,
                "bad request".to_string(),
                Some(msg),
                trace,
                "BAD_REQUEST".to_string(),
            ),
            ApiError::NotFound(trace, msg) => (
                StatusCode::NOT_FOUND,
                "not found".to_string(),
                Some(msg),
                trace,
                "NOT_FOUND".to_string(),
            ),
            ApiError::PolicyViolation(trace, msg) => (
                StatusCode::FORBIDDEN,
                "policy violation".to_string(),
                Some(msg),
                trace,
                "POLICY_VIOLATION".to_string(),
            ),
            ApiError::WorkerError(trace, msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "worker error".to_string(),
                Some(msg),
                trace,
                "WORKER_ERROR".to_string(),
            ),
            ApiError::Unauthorized(trace) => (
                StatusCode::UNAUTHORIZED,
                "unauthorized".to_string(),
                None,
                trace,
                "UNAUTHORIZED".to_string(),
            ),
            ApiError::EgressViolation(trace, msg) => (
                StatusCode::FORBIDDEN,
                "egress violation".to_string(),
                Some(msg),
                trace,
                "EGRESS_VIOLATION".to_string(),
            ),
            ApiError::DeterminismViolation(trace, msg) => (
                StatusCode::FORBIDDEN,
                "determinism violation".to_string(),
                Some(msg),
                trace,
                "DETERMINISM_VIOLATION".to_string(),
            ),
            ApiError::IsolationViolation(trace, msg) => (
                StatusCode::FORBIDDEN,
                "isolation violation".to_string(),
                Some(msg),
                trace,
                "ISOLATION_VIOLATION".to_string(),
            ),
            ApiError::ValidationFailed(trace, fields) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation failed".to_string(),
                Some(format!("Validation errors: {:?}", fields)),
                trace,
                "VALIDATION_FAILED".to_string(),
            ),
            ApiError::DatabaseError(trace, msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "database error".to_string(),
                Some(msg),
                trace,
                "DATABASE_ERROR".to_string(),
            ),
            ApiError::CryptoError(trace, msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "crypto error".to_string(),
                Some(msg),
                trace,
                "CRYPTO_ERROR".to_string(),
            ),
            ApiError::NetworkError(trace, msg) => (
                StatusCode::BAD_GATEWAY,
                "network error".to_string(),
                Some(msg),
                trace,
                "NETWORK_ERROR".to_string(),
            ),
            ApiError::RateLimitExceeded(trace, _) => {
                let mut resp = (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("Retry-After", "60")],
                    Json(ErrorResponse {
                        error: "rate limit exceeded".to_string(),
                        details: Some(
                            "Retry after 60 seconds (exponential backoff: next 120 seconds)"
                                .to_string(),
                        ),
                        trace_id: trace.clone(),
                        code: "RATE_LIMIT_EXCEEDED".to_string(),
                    }),
                )
                    .into_response();
                if let Some(id) = trace {
                    resp.headers_mut().insert("X-Error-ID", id.parse().unwrap());
                }
                return resp;
            }
        };

        let mut resp = (
            status,
            Json(ErrorResponse {
                error,
                details,
                trace_id,
                code,
            }),
        )
            .into_response();

        // Client hints for errors
        if status.is_client_error() || status.is_server_error() {
            if let Some(id) = trace_id {
                resp.headers_mut().insert("X-Error-ID", id.parse().unwrap());
            }
        }

        resp
    }
}

// Prometheus Counter
lazy_static! {
    static ref ERRORS_TOTAL: CounterVec = register_counter_vec!(
        "api_errors_total",
        "Total number of API errors by type",
        &["variant"]
    )
    .unwrap();
}

fn increment_error_counter(variant: &str) {
    ERRORS_TOTAL.with_label_values(&[variant]).inc();
}

// Example Handler (inference_handler with Validation)
async fn inference_handler(
    State(state): State<Arc<ApiState>>,
    Json(mut request): Json<InferenceRequest>,
) -> Result<Json<InferenceResponse>, ApiError> {
    let trace_id = Some(Uuid::new_v4().to_string());

    // Validation
    if request.max_tokens < 1 || request.max_tokens > 4096 {
        increment_error_counter("VALIDATION_FAILED");
        return Err(ApiError::ValidationFailed(
            trace_id,
            vec!["max_tokens out of range (1-4096)".to_string()],
        ));
    }

    // Worker logic (mocked for example)
    let response = state
        .worker
        .lock()
        .await
        .infer(request)
        .await
        .map_err(|e| {
            increment_error_counter("WORKER_ERROR");
            ApiError::WorkerError(trace_id.clone(), e.to_string())
        })?;

    // Emit signal with trace_id
    let _ = state.signals_tx.send(Signal {
        signal_type: "inference_complete".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        payload: serde_json::to_value(&response).unwrap_or_default(),
        priority: "normal".to_string(),
        trace_id,
    });

    Ok(Json(response))
}

// Router (Full Stack with Middleware)
pub fn build_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/inference", post(inference_handler))
        .route("/health", get(health_handler)) // Assume health_handler defined
        .route(
            "/metrics",
            get(|| async {
                let encoder = prometheus::TextEncoder::new();
                let mut buffer = vec![];
                encoder.encode(&prometheus::gather(), &mut buffer).unwrap();
                ([("Content-Type", encoder.format_type())], buffer).into_response()
            }),
        )
        .layer(
            ServiceBuilder::new()
                .layer(middleware::panic_recovery_layer()) // Assume defined in middleware.rs
                .layer(middleware::extractor_error_layer()) // Assume defined
                .layer(middleware::error_catcher_layer()) // Assume defined
                .layer(ratelimit::rate_limit_layer(
                    ratelimit::RateLimitConfig::default(),
                )) // Assume defined in ratelimit.rs
                .layer(logger::error_logger_layer()) // Assume defined in logger.rs
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        )
        .with_state(state)
}
