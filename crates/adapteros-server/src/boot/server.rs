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
//!     shutdown_rx,
//! };
//!
//! bind_and_serve(mode, app, config).await?;
//! ```

use crate::shutdown::{shutdown_signal_with_drain, ShutdownCoordinator, ShutdownError};
use adapteros_boot::EXIT_CONFIG_ERROR;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::middleware::UdsPeerCredentials;
use axum::Router;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info, instrument, warn};

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
    /// Shutdown signal receiver (in-process trigger)
    pub shutdown_rx: broadcast::Receiver<()>,
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
        shutdown_rx,
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
                shutdown_rx,
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
                shutdown_rx,
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
    shutdown_rx: broadcast::Receiver<()>,
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
                    shutdown_rx,
                ))
                .await?;
        }
        ServeListener::Uds(l) => {
            // Use custom UDS serve with peer credential injection
            serve_uds_with_peer_credentials(
                l,
                app,
                boot_state.clone(),
                in_flight_requests,
                drain_timeout,
                shutdown_rx,
            )
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

/// Serve UDS connections with peer credential injection.
///
/// This custom serve loop:
/// 1. Accepts each UDS connection
/// 2. Extracts peer credentials (UID/GID/PID) from the socket
/// 3. Wraps the service to inject credentials into each request's extensions
/// 4. Handles graceful shutdown
///
/// This enables the `worker_uid_middleware` to validate that connecting
/// processes are running as the expected worker UID.
#[cfg(unix)]
async fn serve_uds_with_peer_credentials(
    listener: tokio::net::UnixListener,
    app: Router,
    boot_state: BootStateManager,
    in_flight_requests: Arc<AtomicUsize>,
    drain_timeout: Duration,
    shutdown_rx: broadcast::Receiver<()>,
) -> Result<(), std::io::Error> {
    use hyper::server::conn::http1;
    use hyper_util::rt::TokioIo;
    use std::convert::Infallible;
    use tower::Service;

    let shutdown_signal =
        shutdown_signal_with_drain(boot_state, in_flight_requests, drain_timeout, shutdown_rx);
    tokio::pin!(shutdown_signal);

    loop {
        tokio::select! {
            biased;

            _ = &mut shutdown_signal => {
                info!("UDS server shutting down");
                break;
            }

            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        // Extract peer credentials from the Unix socket
                        let peer_creds = UdsPeerCredentials::from_unix_stream(&stream);

                        if let Some(ref creds) = peer_creds {
                            debug!(
                                peer_uid = creds.uid,
                                peer_gid = creds.gid,
                                peer_pid = ?creds.pid,
                                "UDS connection accepted with peer credentials"
                            );
                        } else {
                            warn!("UDS connection accepted but peer credentials unavailable");
                        }

                        let app = app.clone();
                        let io = TokioIo::new(stream);

                        // Spawn a task to handle this connection
                        tokio::spawn(async move {
                            // Create a service that injects peer credentials into requests
                            let service = hyper::service::service_fn(move |mut req: hyper::Request<hyper::body::Incoming>| {
                                // Inject peer credentials into request extensions
                                if let Some(ref creds) = peer_creds {
                                    req.extensions_mut().insert(creds.clone());
                                }

                                // Clone app for this request
                                let mut app = app.clone();

                                async move {
                                    // Call the axum router
                                    let response = app.call(req).await.map_err(|e| {
                                        // This shouldn't happen as Router's error type is Infallible
                                        error!("Router error: {}", e);
                                        std::io::Error::other("router error")
                                    })?;

                                    Ok::<_, std::io::Error>(response)
                                }
                            });

                            // Serve the connection
                            if let Err(e) = http1::Builder::new()
                                .serve_connection(io, service)
                                .await
                            {
                                // Don't log normal connection closes
                                if !e.to_string().contains("connection closed") {
                                    debug!(error = %e, "UDS connection error");
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "UDS accept error");
                        // Don't break on transient errors
                        if e.kind() != std::io::ErrorKind::WouldBlock {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
            }
        }
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
