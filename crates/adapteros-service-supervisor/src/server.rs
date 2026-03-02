//! HTTP server for the service supervisor

use crate::auth::AuthService;
use crate::error::{Result, SupervisorError};
use crate::supervisor::ServiceSupervisor;
use adapteros_telemetry::middleware::api_logger_middleware;
use axum::http::{header, HeaderValue, Method, Request};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

/// CORS configuration layer for the supervisor API
///
/// Configures Cross-Origin Resource Sharing based on runtime environment:
/// - If ALLOWED_ORIGINS is set: use those origins (production deployment)
/// - If AOS_PRODUCTION_MODE=true and UDS socket: no CORS needed (localhost only)
/// - Otherwise: allow localhost origins for development
fn supervisor_cors_layer() -> CorsLayer {
    use std::collections::HashSet;

    let origins: Vec<HeaderValue> = if let Ok(allowed) = std::env::var("ALLOWED_ORIGINS") {
        // Explicit origins from environment (highest priority)
        allowed
            .split(',')
            .map(|s| s.trim().to_string())
            .collect::<HashSet<_>>()
            .into_iter()
            .filter_map(|origin| origin.parse().ok())
            .collect()
    } else if std::env::var("AOS_PRODUCTION_MODE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
    {
        // Production mode: require explicit ALLOWED_ORIGINS
        warn!(
            "Supervisor CORS: AOS_PRODUCTION_MODE=true but ALLOWED_ORIGINS not set - CORS will block all origins"
        );
        Vec::new()
    } else {
        // Development mode: localhost origins (respects port env vars)
        let ui_port = std::env::var("AOS_UI_PORT").unwrap_or_else(|_| "18081".to_string());
        let server_port = std::env::var("AOS_SERVER_PORT").unwrap_or_else(|_| "18080".to_string());
        let metrics_port =
            std::env::var("AOS_PROMETHEUS_PORT").unwrap_or_else(|_| "18084".to_string());
        warn!(
            ui_port = %ui_port,
            server_port = %server_port,
            "Supervisor CORS: Using development localhost defaults. Set ALLOWED_ORIGINS or AOS_PRODUCTION_MODE=true for production"
        );
        [
            format!("http://localhost:{}", ui_port),
            format!("http://localhost:{}", server_port),
            format!("http://localhost:{}", metrics_port),
            format!("http://127.0.0.1:{}", ui_port),
            format!("http://127.0.0.1:{}", server_port),
            format!("http://127.0.0.1:{}", metrics_port),
        ]
        .into_iter()
        .filter_map(|o| o.parse().ok())
        .collect()
    };

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
        .allow_credentials(true)
        .max_age(std::time::Duration::from_secs(86400))
}

/// JWT authentication middleware for supervisor API
///
/// Validates Bearer token from Authorization header and injects Claims into request.
/// Health endpoint is exempted from authentication.
async fn auth_middleware_supervisor(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> std::result::Result<Response, (StatusCode, axum::Json<serde_json::Value>)> {
    // Skip auth for health endpoint (monitoring tools need unauthenticated access)
    if req.uri().path() == "/health" {
        return Ok(next.run(req).await);
    }

    // Extract Bearer token from Authorization header
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let token = token.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": "missing or invalid Authorization header",
                "hint": "Use 'Authorization: Bearer <token>'"
            })),
        )
    })?;

    // Validate token
    let claims = state.auth_service.validate_token(token).map_err(|e| {
        tracing::warn!(error = %e, "Token validation failed");
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": "invalid or expired token",
                "details": e.to_string()
            })),
        )
    })?;

    // Inject claims into request extensions
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

/// Application state for the HTTP server
#[derive(Clone)]
pub struct AppState {
    pub supervisor: Arc<ServiceSupervisor>,
    pub auth_service: Arc<AuthService>,
}

/// HTTP server for the service supervisor
pub struct SupervisorServer {
    app: Router,
    host: String,
    port: u16,
    /// Unix Domain Socket path for production mode
    uds_socket: Option<PathBuf>,
    /// Enable production mode (requires UDS)
    production_mode: bool,
}

impl SupervisorServer {
    /// Create a new server
    pub fn new(supervisor: Arc<ServiceSupervisor>, config: &crate::config::ServerConfig) -> Self {
        let auth_service = supervisor.auth_service();
        let state = AppState {
            supervisor,
            auth_service,
        };

        let app = Router::new()
            .route("/health", get(health_handler))
            .route("/v1/services", get(list_services_handler))
            .route("/v1/services/:service_id", get(get_service_handler))
            .route("/v1/services/start", post(start_service_handler))
            .route("/v1/services/stop", post(stop_service_handler))
            .route("/v1/services/restart", post(restart_service_handler))
            .route(
                "/v1/services/essential/start",
                post(start_essential_handler),
            )
            .route("/v1/services/essential/stop", post(stop_essential_handler))
            .route(
                "/v1/services/:service_id/logs",
                get(get_service_logs_handler),
            )
            .with_state(state.clone())
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware_supervisor,
            ))
            .layer(supervisor_cors_layer())
            .layer(middleware::from_fn(api_logger_middleware));

        Self {
            app,
            host: config.host.clone(),
            port: config.port,
            uds_socket: config.uds_socket.clone(),
            production_mode: config.production_mode,
        }
    }

    /// Start the server
    pub async fn serve(self) -> Result<()> {
        // Production mode requires UDS socket (egress policy compliance)
        if self.production_mode {
            let uds_path = self.uds_socket.as_ref().ok_or_else(|| {
                SupervisorError::Configuration(
                    "Production mode requires uds_socket to be configured".to_string(),
                )
            })?;

            // Remove existing socket file if it exists
            if uds_path.exists() {
                std::fs::remove_file(uds_path).map_err(|e| {
                    SupervisorError::Internal(format!(
                        "Failed to remove existing socket {}: {}",
                        uds_path.display(),
                        e
                    ))
                })?;
            }

            // Ensure parent directory exists
            if let Some(parent) = uds_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    SupervisorError::Internal(format!(
                        "Failed to create socket directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }

            info!("Starting supervisor server on UDS: {}", uds_path.display());

            let listener = tokio::net::UnixListener::bind(uds_path).map_err(|e| {
                SupervisorError::Internal(format!(
                    "Failed to bind UDS {}: {}",
                    uds_path.display(),
                    e
                ))
            })?;

            axum::serve(listener, self.app).await?;
        } else {
            // Development mode: TCP listener
            let addr = format!("{}:{}", self.host, self.port);
            info!("Starting supervisor server on {}", addr);

            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, self.app).await?;
        }

        Ok(())
    }
}

/// Health check endpoint
async fn health_handler(State(state): State<AppState>) -> axum::response::Response {
    match state.supervisor.get_health_status().await {
        Ok(health) => axum::Json(health).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// List all services
async fn list_services_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::auth::Claims>,
) -> axum::response::Response {
    // Check permission
    if !state.auth_service.has_permission(&claims, "services.read") {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "insufficient permissions",
                "required": "services.read"
            })),
        )
            .into_response();
    }

    let services = state.supervisor.get_services().await;

    #[derive(Serialize)]
    struct ServicesResponse {
        services: Vec<crate::service::ServiceStatus>,
    }

    axum::Json(ServicesResponse { services }).into_response()
}

/// Get a specific service
async fn get_service_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::auth::Claims>,
    Path(service_id): Path<String>,
) -> axum::response::Response {
    // Check permission
    if !state.auth_service.has_permission(&claims, "services.read") {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "insufficient permissions",
                "required": "services.read"
            })),
        )
            .into_response();
    }

    match state.supervisor.get_service(&service_id).await {
        Ok(service) => axum::Json(service).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Start service request
#[derive(Deserialize)]
struct StartServiceRequest {
    service_id: String,
}

/// Start a service
async fn start_service_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::auth::Claims>,
    axum::Json(req): axum::Json<StartServiceRequest>,
) -> axum::response::Response {
    // Check permission
    if !state.auth_service.has_permission(&claims, "services.start") {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "insufficient permissions",
                "required": "services.start"
            })),
        )
            .into_response();
    }

    match state.supervisor.start_service(&req.service_id).await {
        Ok(message) => {
            #[derive(Serialize)]
            struct StartResponse {
                success: bool,
                message: String,
            }
            axum::Json(StartResponse {
                success: true,
                message,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Stop service request
#[derive(Deserialize)]
struct StopServiceRequest {
    service_id: String,
}

/// Stop a service
async fn stop_service_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::auth::Claims>,
    axum::Json(req): axum::Json<StopServiceRequest>,
) -> axum::response::Response {
    // Check permission
    if !state.auth_service.has_permission(&claims, "services.stop") {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "insufficient permissions",
                "required": "services.stop"
            })),
        )
            .into_response();
    }

    match state.supervisor.stop_service(&req.service_id).await {
        Ok(message) => {
            #[derive(Serialize)]
            struct StopResponse {
                success: bool,
                message: String,
            }
            axum::Json(StopResponse {
                success: true,
                message,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Restart service request
#[derive(Deserialize)]
struct RestartServiceRequest {
    service_id: String,
}

/// Restart a service
async fn restart_service_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::auth::Claims>,
    axum::Json(req): axum::Json<RestartServiceRequest>,
) -> axum::response::Response {
    // Check permission
    if !state
        .auth_service
        .has_permission(&claims, "services.restart")
    {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "insufficient permissions",
                "required": "services.restart"
            })),
        )
            .into_response();
    }

    match state.supervisor.restart_service(&req.service_id).await {
        Ok(message) => {
            #[derive(Serialize)]
            struct RestartResponse {
                success: bool,
                message: String,
            }
            axum::Json(RestartResponse {
                success: true,
                message,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Start all essential services
async fn start_essential_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::auth::Claims>,
) -> axum::response::Response {
    // Check permission - requires both permission AND admin role
    if !state.auth_service.has_permission(&claims, "services.start") || claims.role != "admin" {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "insufficient permissions",
                "required": "services.start and admin role"
            })),
        )
            .into_response();
    }

    match state.supervisor.start_essential_services().await {
        Ok(results) => {
            #[derive(Serialize)]
            struct EssentialResponse {
                success: bool,
                results: Vec<String>,
            }
            axum::Json(EssentialResponse {
                success: true,
                results,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Stop all essential services
async fn stop_essential_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::auth::Claims>,
) -> axum::response::Response {
    // Check permission - requires both permission AND admin role
    if !state.auth_service.has_permission(&claims, "services.stop") || claims.role != "admin" {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "insufficient permissions",
                "required": "services.stop and admin role"
            })),
        )
            .into_response();
    }

    match state.supervisor.stop_essential_services().await {
        Ok(results) => {
            #[derive(Serialize)]
            struct EssentialResponse {
                success: bool,
                results: Vec<String>,
            }
            axum::Json(EssentialResponse {
                success: true,
                results,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Query parameters for log retrieval
#[derive(Deserialize)]
struct LogsQuery {
    #[serde(default = "default_log_lines")]
    lines: usize,
}

fn default_log_lines() -> usize {
    100
}

/// Get service logs
async fn get_service_logs_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::auth::Claims>,
    Path(service_id): Path<String>,
    Query(params): Query<LogsQuery>,
) -> axum::response::Response {
    // Check permission
    if !state.auth_service.has_permission(&claims, "services.read") {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "insufficient permissions",
                "required": "services.read"
            })),
        )
            .into_response();
    }

    match state
        .supervisor
        .get_service_logs(&service_id, params.lines)
        .await
    {
        Ok(logs) => {
            #[derive(Serialize)]
            struct LogsResponse {
                service_id: String,
                logs: Vec<String>,
            }

            axum::Json(LogsResponse { service_id, logs }).into_response()
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}
