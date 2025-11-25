use std::path::Path;
use tokio::net::UnixListener;
use hyper_util::rt::TokioIo;
use hyper_util::server::conn::auto::Builder;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tokio::sync::broadcast;
use std::os::unix::fs::PermissionsExt;
use tracing::{info, error};
use axum::Router;
use adapteros_api::ApiState;
use std::sync::Arc;
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_core::{AosError, Result};

// Citation: 【2025-11-12†uds_handler†dispatch】
// Extracted from adapteros-server UDS pattern to avoid duplication [source: crates/adapteros-server/src/main.rs L1661-L1698]

/// Handles UDS connections with the given router service
pub async fn handle_uds_connections<P: AsRef<Path>>(
    socket_path: P,
    app: Router<Arc<ApiState>>,
    signals_tx: broadcast::Sender<adapteros_api::Signal>,
) -> Result<()> {
    let socket_path = socket_path.as_ref();
    if socket_path.exists() {
        std::fs::remove_file(socket_path)
            .map_err(|e| AosError::Io(format!("Failed to remove existing socket: {}", e)))?;
    }
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AosError::Io(format!("Failed to create socket directory: {}", e)))?;
    }
    let listener = UnixListener::bind(socket_path)
        .map_err(|e| AosError::Io(format!("Failed to bind Unix socket: {}", e)))?;
    #[cfg(unix)]
    {
        let mut perms = std::fs::metadata(socket_path)
            .map_err(|e| AosError::Io(format!("Failed to get socket metadata: {}", e)))?
            .permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(socket_path, perms)
            .map_err(|e| AosError::Io(format!("Failed to set socket permissions: {}", e)))?;
    }
    info!(socket_path = %socket_path.display(), "UDS server bound");

    let make_service = app.into_make_service();
    let builder = Builder::new(tokio::runtime::Handle::current().into());

    let mut shutdown_rx = signals_tx.subscribe();
    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("UDS shutdown signal received");
                break Ok(());
            }
            accept_res = listener.accept() => {
                match accept_res {
                    Ok((stream, _)) => {
                        let io = TokioIo::new(stream.compat());
                        let make_service_clone = make_service.clone();
                        let builder_clone = builder.clone();
                        let _ = spawn_deterministic("UDS conn".to_string(), async move {
                            if let Err(e) = builder_clone.serve_connection(io, make_service_clone).await {
                                error!(error = %e, "UDS connection error");
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "UDS accept error");
                        break Err(AosError::Io(format!("UDS accept error: {}", e)));
                    }
                }
            }
        }
    }
}
