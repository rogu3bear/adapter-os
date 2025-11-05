//! AdapterOS API Types and Error Handling
//!
//! This crate provides:
//! - API request/response types
//! - Error handling for HTTP endpoints
//! - Serialization support for all API types
//! - Unix Domain Socket HTTP server implementation
//!
//! # Examples
//!
//! ```rust
//! use adapteros_api::{ApiError, LoginRequest, LoginResponse};
//! use axum::response::IntoResponse;
//!
//! // Handle API errors
//! let error = ApiError::NotFound("User not found".to_string());
//! let response = error.into_response();
//! ```
//!
//! References:
//! - Unix Domain Sockets: https://man7.org/linux/man-pages/man7/unix.7.html
//! - Hyper HTTP Server: https://docs.rs/hyper/latest/hyper/
//! - Axum Web Framework: https://docs.rs/axum/latest/axum/

use adapteros_core::AosError;
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_lora_worker::{InferenceRequest, InferenceResponse};
use axum::http::HeaderMap;
use axum::response::sse::{Event, Sse};
use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::Stream;
use futures_util::TryStreamExt;
use http_body_util::BodyExt;
use hyper_util::rt::TokioExecutor;
use hyper_util::server::conn::auto::Builder;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tower::Service;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub mod types;

use adapteros_lora_kernel_api::FusedKernels;

/// API server state
pub struct ApiState<K: FusedKernels> {
    worker: Arc<tokio::sync::Mutex<adapteros_lora_worker::Worker<K>>>,
    signals_tx: broadcast::Sender<Signal>,
}

// Ensure ApiState is Send + Sync when K is
unsafe impl<K: FusedKernels + Send + Sync> Send for ApiState<K> {}
unsafe impl<K: FusedKernels + Send + Sync> Sync for ApiState<K> {}

impl<K: FusedKernels> ApiState<K> {
    /// Create new API state with worker
    pub fn new(
        worker: adapteros_lora_worker::Worker<K>,
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
    worker: adapteros_lora_worker::Worker<adapteros_lora_kernel_mtl::MetalKernels>,
) -> Result<(), Box<dyn std::error::Error>> {
    serve_uds_with_metal_kernels_impl(socket_path, worker).await
}

/// Generic UDS server with worker (for other kernel implementations)
///
/// For use with non-Metal kernel implementations. If using MetalKernels,
/// prefer `serve_uds_with_metal_kernels` for better type inference.
pub async fn serve_uds_with_worker<K: FusedKernels + Send + Sync + 'static, P: AsRef<Path>>(
    socket_path: P,
    worker: adapteros_lora_worker::Worker<K>,
) -> Result<(), Box<dyn std::error::Error>> {
    serve_uds_with_worker_impl(socket_path, worker).await
}

/// Internal implementation for MetalKernels (concrete, no generics)
async fn serve_uds_with_metal_kernels_impl<P: AsRef<Path>>(
    socket_path: P,
    worker: adapteros_lora_worker::Worker<adapteros_lora_kernel_mtl::MetalKernels>,
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
    let make_service = app.into_make_service();
    let builder = Builder::new(TokioExecutor::new());

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let io = hyper_util::rt::TokioIo::new(stream);
                let make_service_clone = make_service.clone();
                let builder_clone = builder.clone();
                let _ = spawn_deterministic("UDS connection handler".to_string(), async move {
                    // Make a service for this connection
                    let svc = make_service_clone.make_service(()).await.map_err(|e| {
                        tracing::error!("MakeService failed: {}", e);
                        std::io::Error::new(std::io::ErrorKind::Other, "make service error")
                    })?;
                    
                    let hyper_svc = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                        let mut svc_clone = svc.clone();
                        async move {
                            // Convert Incoming to Body
                            let (parts, body) = req.into_parts();
                            let body_bytes = body.collect().await
                                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Body collection failed: {}", e)))?
                                .to_bytes();
                            let axum_body = Body::from(body_bytes.into_iter().collect::<Vec<u8>>());
                            let axum_req = hyper::Request::from_parts(parts, axum_body);
                            
                            svc_clone.call(axum_req).await.map_err(|e| {
                                tracing::error!("Service error: {}", e);
                                std::io::Error::new(std::io::ErrorKind::Other, "service error")
                            })
                        }
                    });
                    if let Err(err) = builder_clone.serve_connection(io, hyper_svc).await {
                        tracing::error!("Connection error: {}", err);
                    }
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
async fn serve_uds_with_worker_impl<K: FusedKernels + Send + Sync + 'static, P: AsRef<Path>>(
    socket_path: P,
    worker: adapteros_lora_worker::Worker<K>,
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
        .route("/inference", post(|s: State<Arc<ApiState<K>>>, req: Json<InferenceRequest>| async {
            inference_handler(s, req).await
        }))
        .route("/health", post(|s: State<Arc<ApiState<K>>>| async {
            health_handler(s).await
        }))
        .route("/health", get(|s: State<Arc<ApiState<K>>>| async {
            health_handler(s).await
        }))
        .route("/adapter", post(|s: State<Arc<ApiState<K>>>, h: HeaderMap, cmd: Json<adapteros_lora_worker::AdapterCommand>| async {
            adapter_command_handler(s, h, cmd).await
        }))
        .route("/adapters", get(|s: State<Arc<ApiState<K>>>, h: HeaderMap| async {
            list_adapters_handler(s, h).await
        }))
        .route("/adapter/:id", get(|s: State<Arc<ApiState<K>>>, h: HeaderMap, p: axum::extract::Path<String>| async {
            adapter_profile_handler(s, h, p).await
        }))
        .route("/adapter/:id/:cmd", post(|s: State<Arc<ApiState<K>>>, h: HeaderMap, p: axum::extract::Path<(String, String)>| async {
            adapter_lifecycle_handler(s, h, p).await
        }))
        .route("/profile/snapshot", get(|s: State<Arc<ApiState<K>>>, h: HeaderMap| async {
            profile_snapshot_handler(s, h).await
        }))
        .route("/warmup", post(|s: State<Arc<ApiState<K>>>, h: HeaderMap| async {
            warmup_handler(s, h).await
        }))
        .route("/signals", get(|s: State<Arc<ApiState<K>>>, h: HeaderMap| async {
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
    let app_service: Router<Arc<ApiState<K>>> = app;
    let builder = Builder::new(TokioExecutor::new());

    // Accept connections and serve
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let io = hyper_util::rt::TokioIo::new(stream);
                let svc = app_service.clone();
                let builder_clone = builder.clone();
                let _ = spawn_deterministic("UDS connection handler".to_string(), async move {
                    // Pattern from adapteros-server/src/main.rs L1562-L1571
                    // Router implements Service for hyper::Request<Incoming> when used with hyper_util
                    let hyper_svc = hyper::service::service_fn(move |req| {
                        let mut svc_clone = svc.clone();
                        async move {
                            Service::call(&mut svc_clone, req).await.map_err(|e| {
                                tracing::error!(error = %e, "UDS service call failed");
                                // Convert service errors to appropriate HTTP status codes
                                match e {
                                    // Handle specific error types if available
                                    _ => {
                                        tracing::error!(error = %e, "Unhandled service error in UDS handler");
                                        std::io::Error::new(std::io::ErrorKind::Other, format!("Service error: {}", e))
                                    }
                                }
                            })
                        }
                    });
                    if let Err(err) = builder_clone.serve_connection(io, hyper_svc).await {
                        tracing::error!("Connection error: {}", err);
                    }
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
type MetalApiState = ApiState<adapteros_lora_kernel_mtl::MetalKernels>;

async fn inference_handler_metal(
    state: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>,
    request: Json<InferenceRequest>,
) -> Result<Json<InferenceResponse>, ApiError> {
    // Call generic handler with explicit type
    inference_handler::<adapteros_lora_kernel_mtl::MetalKernels>(state, request).await
}

async fn health_handler_metal(
    state: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>,
) -> impl IntoResponse {
    health_handler::<adapteros_lora_kernel_mtl::MetalKernels>(state).await
}

async fn adapter_command_handler_metal(
    state: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>,
    headers: HeaderMap,
    command: Json<adapteros_lora_worker::AdapterCommand>,
) -> Result<Json<adapteros_lora_worker::AdapterCommandResult>, ApiError> {
    adapter_command_handler::<adapteros_lora_kernel_mtl::MetalKernels>(state, headers, command).await
}

async fn list_adapters_handler_metal(
    state: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    list_adapters_handler::<adapteros_lora_kernel_mtl::MetalKernels>(state, headers).await
}

async fn adapter_profile_handler_metal(
    state: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    adapter_profile_handler::<adapteros_lora_kernel_mtl::MetalKernels>(state, headers, path).await
}

async fn adapter_lifecycle_handler_metal(
    state: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>,
    headers: HeaderMap,
    path: axum::extract::Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    adapter_lifecycle_handler::<adapteros_lora_kernel_mtl::MetalKernels>(state, headers, path).await
}

async fn profile_snapshot_handler_metal(
    state: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    profile_snapshot_handler::<adapteros_lora_kernel_mtl::MetalKernels>(state, headers).await
}

async fn warmup_handler_metal(
    state: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    warmup_handler::<adapteros_lora_kernel_mtl::MetalKernels>(state, headers).await
}

async fn signals_handler_metal(
    state: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError> {
    signals_handler::<adapteros_lora_kernel_mtl::MetalKernels>(state, headers).await
}

/// Build router for MetalKernels (concrete type, no generic closure issues)
fn build_router_metal(state: Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>) -> Router<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>> {
    // Use closures for concrete handlers to ensure Handler trait satisfaction
    // Even though these are concrete, Axum's Handler trait works better with closures
    Router::new()
        .route("/inference", post(|s: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>, req: Json<InferenceRequest>| async move {
            inference_handler_metal(s, req).await
        }))
        .route("/health", post(|s: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>| async move {
            health_handler_metal(s).await
        }))
        .route("/health", get(|s: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>| async move {
            health_handler_metal(s).await
        }))
        .route("/adapter", post(|s: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>, h: HeaderMap, cmd: Json<adapteros_lora_worker::AdapterCommand>| async move {
            adapter_command_handler_metal(s, h, cmd).await
        }))
        .route("/adapters", get(|s: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>, h: HeaderMap| async move {
            list_adapters_handler_metal(s, h).await
        }))
        .route("/adapter/:id", get(|s: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>, h: HeaderMap, p: axum::extract::Path<String>| async move {
            adapter_profile_handler_metal(s, h, p).await
        }))
        .route("/adapter/:id/:cmd", post(|s: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>, h: HeaderMap, p: axum::extract::Path<(String, String)>| async move {
            adapter_lifecycle_handler_metal(s, h, p).await
        }))
        .route("/profile/snapshot", get(|s: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>, h: HeaderMap| async move {
            profile_snapshot_handler_metal(s, h).await
        }))
        .route("/warmup", post(|s: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>, h: HeaderMap| async move {
            warmup_handler_metal(s, h).await
        }))
        .route("/signals", get(|s: State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>, h: HeaderMap| async move {
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
async fn inference_handler<K: FusedKernels + Send + Sync>(
    State(state): State<Arc<ApiState<K>>>,
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
    let mut worker = state.worker.lock().await;
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
async fn health_handler<K: FusedKernels + Send + Sync>(
    State(state): State<Arc<ApiState<K>>>,
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
async fn adapter_command_handler<K: FusedKernels + Send + Sync>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
    Json(command): Json<adapteros_lora_worker::AdapterCommand>,
) -> Result<Json<adapteros_lora_worker::AdapterCommandResult>, ApiError> {
    admin_guard(&headers)?;
    // Forward command to worker
    let mut worker = state.worker.lock().await;
    let result = worker
        .execute_adapter_command(command)
        .map_err(|e| ApiError::WorkerError(e.to_string()))?;

    Ok(Json(result))
}

/// List adapters (CLI-compatible view)
async fn list_adapters_handler<K: FusedKernels + Send + Sync>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    let items = worker.list_adapter_states_view();
    Ok((StatusCode::OK, Json(items)))
}

/// Get adapter profile (CLI-compatible view)
async fn adapter_profile_handler<K: FusedKernels + Send + Sync>(
    State(state): State<Arc<ApiState<K>>>,
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
async fn adapter_lifecycle_handler<K: FusedKernels + Send + Sync>(
    State(state): State<Arc<ApiState<K>>>,
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
async fn profile_snapshot_handler<K: FusedKernels + Send + Sync>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    let snapshot = worker.profiling_snapshot_json();
    Ok((StatusCode::OK, Json(snapshot)))
}

/// Execute a warmup routine on the worker
async fn warmup_handler<K: FusedKernels + Send + Sync>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let mut worker = state.worker.lock().await;
    let report = worker
        .warmup()
        .await
        .map_err(|e| ApiError::WorkerError(e.to_string()))?;
    Ok((StatusCode::OK, Json(report)))
}

/// Stream signals as Server-Sent Events
async fn signals_handler<K: FusedKernels + Send + Sync>(
    State(state): State<Arc<ApiState<K>>>,
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

fn current_millis() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

// -----------------------------
// Admin gating and auth helpers
// -----------------------------

fn admin_enabled() -> bool {
    std::env::var("AOS_API_ENABLE_ADMIN")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn admin_guard(headers: &HeaderMap) -> Result<(), ApiError> {
    if !admin_enabled() {
        return Err(ApiError::Unauthorized);
    }

    if let Ok(token) = std::env::var("AOS_API_ADMIN_TOKEN") {
        // If token configured, require header X-Admin-Token to match
        if let Some(hv) = headers.get("X-Admin-Token") {
            if hv.to_str().unwrap_or("") == token {
                return Ok(());
            }
        }
        return Err(ApiError::Unauthorized);
    }

    Ok(())
}

/// API error response (matches aos-cp-api pattern)
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// API error type
#[derive(Debug)]
pub enum ApiError {
    Internal(String),
    BadRequest(String),
    NotFound(String),
    PolicyViolation(String),
    WorkerError(String),
    Unauthorized,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, details) = match self {
            ApiError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
                Some(msg),
            ),
            ApiError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "bad request".to_string(),
                Some(msg),
            ),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "not found".to_string(), Some(msg)),
            ApiError::PolicyViolation(msg) => (
                StatusCode::FORBIDDEN,
                "policy violation".to_string(),
                Some(msg),
            ),
            ApiError::WorkerError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "worker error".to_string(),
                Some(msg),
            ),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string(), None),
        };

        (status, Json(ErrorResponse { error, details })).into_response()
    }
}

// Conversion from AosError
impl From<AosError> for ApiError {
    fn from(err: AosError) -> Self {
        match err {
            AosError::PolicyViolation(msg) => ApiError::PolicyViolation(msg),
            AosError::Worker(msg) => ApiError::WorkerError(msg),
            AosError::InvalidCPID(msg) | AosError::InvalidHash(msg) => ApiError::BadRequest(msg),
            _ => ApiError::Internal(err.to_string()),
        }
    }
}
