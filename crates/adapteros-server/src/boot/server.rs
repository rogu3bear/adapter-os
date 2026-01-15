//! Server binding utilities for adapterOS control plane.
//!
//! This module provides abstractions for binding the server to either
//! TCP (development) or Unix Domain Socket (production) endpoints.
//!
//! # Binding Modes
//!
//! - **TCP (Development)**: Binds to a TCP address for local development.
//!   This mode is less secure but allows browser access.
//!
//! - **UDS (Production)**: Binds to a Unix Domain Socket per the Egress
//!   policy. This prevents network egress and is required for production.
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_server::boot::{BindMode, ServerConfig, bind_and_serve};
//!
//! let mode = if production {
//!     BindMode::Uds { socket_path: "/var/run/aos/cp.sock".to_string() }
//! } else {
//!     BindMode::Tcp { addr: "127.0.0.1:8080".parse().unwrap() }
//! };
//!
//! let config = ServerConfig {
//!     boot_state,
//!     shutdown_coordinator,
//!     drain_timeout,
//!     in_flight_requests,
//! };
//!
//! bind_and_serve(mode, app, config).await?;
//! ```

use crate::shutdown::{ShutdownCoordinator, ShutdownError};
use adapteros_boot::EXIT_CONFIG_ERROR;
use adapteros_server_api::boot_state::BootStateManager;
use axum::Router;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, instrument, warn};

/// Server binding mode.
#[derive(Debug, Clone)]
pub enum BindMode {
    /// TCP binding for development mode
    Tcp {
        /// Socket address to bind to
        addr: SocketAddr,
        /// Display address for logging (may differ from bind for 0.0.0.0)
        display_addr: SocketAddr,
    },
    /// Unix Domain Socket binding for production mode
    Uds {
        /// Path to the Unix socket
        socket_path: String,
    },
}

impl BindMode {
    /// Create a TCP bind mode with proper display address handling.
    ///
    /// If binding to 0.0.0.0 (all interfaces), the display address will
    /// show 127.0.0.1 for user-friendly logging.
    pub fn tcp(addr: SocketAddr) -> Self {
        use std::net::{IpAddr, Ipv4Addr};

        let display_addr = if addr.ip() == IpAddr::V4(Ipv4Addr::UNSPECIFIED) {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
        } else {
            addr
        };

        Self::Tcp { addr, display_addr }
    }

    /// Create a UDS bind mode.
    pub fn uds(socket_path: impl Into<String>) -> Self {
        Self::Uds {
            socket_path: socket_path.into(),
        }
    }
}

/// Configuration for server binding.
pub struct ServerBindConfig {
    /// Boot state manager for lifecycle transitions
    pub boot_state: BootStateManager,
    /// Shutdown coordinator for graceful shutdown
    pub shutdown_coordinator: ShutdownCoordinator,
    /// Timeout for draining in-flight requests
    pub drain_timeout: Duration,
    /// Counter for in-flight requests
    pub in_flight_requests: Arc<AtomicUsize>,
}

/// Result of server binding operation.
#[derive(Debug)]
pub enum BindError {
    /// Port is already in use
    PortInUse { port: u16, addr: SocketAddr },
    /// Socket file cannot be created
    SocketCreationFailed { path: String, reason: String },
    /// Generic IO error
    IoError(std::io::Error),
    /// Shutdown error
    ShutdownError(ShutdownError),
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PortInUse { port, addr } => {
                write!(
                    f,
                    "Port {} already in use at {}. Kill existing process: lsof -ti:{} | xargs kill",
                    port, addr, port
                )
            }
            Self::SocketCreationFailed { path, reason } => {
                write!(
                    f,
                    "Failed to create socket at {}: {}. Check permissions or remove stale socket",
                    path, reason
                )
            }
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::ShutdownError(e) => write!(f, "Shutdown error: {}", e),
        }
    }
}

impl std::error::Error for BindError {}

impl From<std::io::Error> for BindError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<ShutdownError> for BindError {
    fn from(e: ShutdownError) -> Self {
        Self::ShutdownError(e)
    }
}

/// Map a bind error to a process exit code.
pub fn bind_error_exit_code(err: &BindError) -> i32 {
    match err {
        BindError::PortInUse { .. } | BindError::SocketCreationFailed { .. } => EXIT_CONFIG_ERROR,
        BindError::IoError(e) if e.kind() == std::io::ErrorKind::AddrInUse => EXIT_CONFIG_ERROR,
        _ => 1,
    }
}

/// Pre-check TCP port availability before handing it to Axum.
pub fn precheck_tcp_port(addr: SocketAddr) -> Result<(), BindError> {
    match TcpListener::bind(addr) {
        Ok(listener) => drop(listener),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            return Err(BindError::PortInUse {
                port: addr.port(),
                addr,
            });
        }
        Err(e) => return Err(BindError::IoError(e)),
    }
    Ok(())
}

/// Bind and serve the application with graceful shutdown.
///
/// This function:
/// 1. Binds to the specified mode (TCP or UDS)
/// 2. Transitions boot state to ready
/// 3. Serves the application with graceful shutdown
/// 4. Handles coordinated shutdown of all components
///
/// # Arguments
///
/// * `mode` - The binding mode (TCP or UDS)
/// * `app` - The Axum router to serve
/// * `config` - Server configuration including boot state and shutdown coordinator
///
/// # Returns
///
/// Returns `Ok(())` on successful shutdown, or `Err` if binding fails.
///
#[instrument(skip_all)]
pub async fn bind_and_serve(
    mode: BindMode,
    app: Router,
    config: ServerBindConfig,
) -> Result<(), BindError> {
    let ServerBindConfig {
        boot_state,
        shutdown_coordinator,
        drain_timeout,
        in_flight_requests,
    } = config;

    match mode {
        BindMode::Tcp { addr, display_addr } => {
            info!(addr = %addr, "Starting control plane");
            info!(url = %format!("http://{}:{}/", display_addr.ip(), display_addr.port()), "UI available");
            info!(url = %format!("http://{}:{}/api/", display_addr.ip(), display_addr.port()), "API available");
            warn!("Development mode: TCP binding enabled. Set production_mode=true for UDS-only");

            precheck_tcp_port(addr)?;

            // Bind first, fail fast if port in use
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    return Err(BindError::PortInUse {
                        port: addr.port(),
                        addr,
                    });
                }
                Err(e) => return Err(e.into()),
            };

            serve_and_shutdown(
                ServeListener::Tcp(listener),
                app,
                boot_state,
                shutdown_coordinator,
                drain_timeout,
                in_flight_requests,
            )
            .await
        }
        BindMode::Uds { socket_path } => {
            info!(socket_path = %socket_path, "Starting control plane on UDS");
            info!("Production mode enabled - TCP binding disabled per Egress policy");

            // Remove existing socket file if present
            let _ = std::fs::remove_file(&socket_path);

            // Bind first, fail fast if socket cannot be created
            let listener = match tokio::net::UnixListener::bind(&socket_path) {
                Ok(l) => l,
                Err(e) => {
                    return Err(BindError::SocketCreationFailed {
                        path: socket_path.clone(),
                        reason: e.to_string(),
                    });
                }
            };

            serve_and_shutdown(
                ServeListener::Uds(listener),
                app,
                boot_state,
                shutdown_coordinator,
                drain_timeout,
                in_flight_requests,
            )
            .await
        }
    }
}

/// Internal enum to unify TCP and UDS listeners for serving.
enum ServeListener {
    Tcp(tokio::net::TcpListener),
    Uds(tokio::net::UnixListener),
}

/// Serve the application and handle shutdown.
///
/// This is the common serving logic used by both TCP and UDS modes.
async fn serve_and_shutdown(
    listener: ServeListener,
    app: Router,
    boot_state: BootStateManager,
    shutdown_coordinator: ShutdownCoordinator,
    drain_timeout: Duration,
    in_flight_requests: Arc<AtomicUsize>,
) -> Result<(), BindError> {
    // Mark ready - binding succeeded
    boot_state.ready().await;
    boot_state.fully_ready().await;

    // Serve with graceful shutdown
    match listener {
        ServeListener::Tcp(l) => {
            axum::serve(l, app)
                .with_graceful_shutdown(shutdown_signal_with_drain(
                    boot_state.clone(),
                    Arc::clone(&in_flight_requests),
                    drain_timeout,
                ))
                .await?;
        }
        ServeListener::Uds(l) => {
            axum::serve(l, app)
                .with_graceful_shutdown(shutdown_signal_with_drain(
                    boot_state.clone(),
                    Arc::clone(&in_flight_requests),
                    drain_timeout,
                ))
                .await?;
        }
    }

    // Server has shut down, now perform coordinated shutdown
    info!("Server shutdown complete, performing coordinated component shutdown");
    handle_coordinated_shutdown(shutdown_coordinator).await?;

    // Final MLX cleanup after all other components
    #[cfg(feature = "multi-backend")]
    {
        adapteros_lora_worker::mlx_runtime_shutdown();
        tracing::info!("MLX runtime shut down");
    }

    Ok(())
}

/// Handle coordinated shutdown of all components.
async fn handle_coordinated_shutdown(
    shutdown_coordinator: ShutdownCoordinator,
) -> Result<(), BindError> {
    match shutdown_coordinator.shutdown().await {
        Ok(()) => {
            info!("All components shut down successfully");
            Ok(())
        }
        Err(e) => {
            match &e {
                ShutdownError::CriticalFailure { component } => {
                    error!(
                        "Critical shutdown failure in {} - system integrity compromised",
                        component
                    );
                    std::process::exit(1);
                }
                ShutdownError::PartialFailure { failed_count } => {
                    warn!(
                        failed_count = failed_count,
                        "Partial shutdown failure - components failed but system integrity maintained"
                    );
                    // Don't return error - partial failures are acceptable
                    Ok(())
                }
                _ => {
                    error!(error = %e, "Shutdown error");
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Shutdown signal handler with request draining.
///
/// Waits for SIGINT/SIGTERM, transitions boot state to draining,
/// and waits for in-flight requests to complete (with timeout).
async fn shutdown_signal_with_drain(
    boot_state: BootStateManager,
    in_flight_requests: Arc<AtomicUsize>,
    drain_timeout: Duration,
) {
    use std::sync::atomic::Ordering;
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, initiating graceful shutdown");
        },
        _ = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown");
        },
    }

    // Transition to draining state
    boot_state.drain().await;

    // Wait for in-flight requests to complete (with timeout)
    let start = std::time::Instant::now();
    loop {
        let in_flight = in_flight_requests.load(Ordering::Acquire);
        if in_flight == 0 {
            info!("All in-flight requests completed");
            break;
        }

        if start.elapsed() > drain_timeout {
            warn!(
                in_flight = in_flight,
                timeout_secs = drain_timeout.as_secs(),
                "Drain timeout reached with requests still in flight"
            );
            break;
        }

        info!(
            in_flight = in_flight,
            "Waiting for in-flight requests to complete"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Mark shutdown complete
    boot_state.stop().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_bind_mode_tcp_localhost() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080);
        let mode = BindMode::tcp(addr);

        match mode {
            BindMode::Tcp {
                addr: a,
                display_addr: d,
            } => {
                assert_eq!(a, addr);
                assert_eq!(d, addr); // Same for localhost
            }
            _ => panic!("Expected TCP mode"),
        }
    }

    #[test]
    fn test_bind_mode_tcp_unspecified() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080);
        let mode = BindMode::tcp(addr);

        match mode {
            BindMode::Tcp {
                addr: a,
                display_addr: d,
            } => {
                assert_eq!(a, addr);
                assert_eq!(d.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST)); // Display should be localhost
                assert_eq!(d.port(), 8080);
            }
            _ => panic!("Expected TCP mode"),
        }
    }

    #[test]
    fn test_bind_mode_uds() {
        let mode = BindMode::uds("/var/run/aos/cp.sock");

        match mode {
            BindMode::Uds { socket_path } => {
                assert_eq!(socket_path, "/var/run/aos/cp.sock");
            }
            _ => panic!("Expected UDS mode"),
        }
    }

    #[test]
    fn test_bind_error_display() {
        let err = BindError::PortInUse {
            port: 8080,
            addr: "127.0.0.1:8080".parse().unwrap(),
        };
        assert!(err.to_string().contains("8080"));
        assert!(err.to_string().contains("lsof"));

        let err = BindError::SocketCreationFailed {
            path: "/var/run/test.sock".to_string(),
            reason: "permission denied".to_string(),
        };
        assert!(err.to_string().contains("/var/run/test.sock"));
        assert!(err.to_string().contains("permission denied"));
    }
}
