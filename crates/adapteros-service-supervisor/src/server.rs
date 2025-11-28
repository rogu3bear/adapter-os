//! HTTP server for the service supervisor

use crate::auth::AuthService;
use crate::error::{Result, SupervisorError};
use crate::supervisor::ServiceSupervisor;
use adapteros_telemetry::middleware::api_logger_middleware;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

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
            .with_state(state)
            .layer(CorsLayer::permissive())
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
    _headers: HeaderMap,
) -> axum::response::Response {
    // For now, skip authentication for localhost communication
    // In production, this should validate JWT tokens
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
    Path(service_id): Path<String>,
    _headers: HeaderMap,
) -> axum::response::Response {
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
    _headers: HeaderMap,
    axum::Json(req): axum::Json<StartServiceRequest>,
) -> axum::response::Response {
    // Skip auth for now - in production this should validate JWT
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
    _headers: HeaderMap,
    axum::Json(req): axum::Json<StopServiceRequest>,
) -> axum::response::Response {
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
    _headers: HeaderMap,
    axum::Json(req): axum::Json<RestartServiceRequest>,
) -> axum::response::Response {
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
    _headers: HeaderMap,
) -> axum::response::Response {
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
    _headers: HeaderMap,
) -> axum::response::Response {
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
    Path(service_id): Path<String>,
    Query(params): Query<LogsQuery>,
    _headers: HeaderMap,
) -> axum::response::Response {
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

// Authentication functions removed for simplified implementation
// In production, JWT authentication should be added back
