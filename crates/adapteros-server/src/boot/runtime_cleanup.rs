//! Boot-time cleanup of stale runtime state.
//!
//! Removes orphaned UDS sockets, the `system_ready` marker, and the
//! `training-worker.degraded` marker before any service attempts to bind.
//! All operations are best-effort: failures are logged and never block boot.

use std::path::Path;
use tracing::{info, warn};

/// Known UDS socket names that live under `var/run/`.
const KNOWN_SOCKETS: &[&str] = &[
    "aos-secd.sock",
    "training-worker.sock",
    "worker.sock",
    "metrics.sock",
    "action-logs.sock",
];

/// Socket-to-heartbeat associations. When a stale socket is removed its
/// companion heartbeat file is cleaned as well.
const SOCKET_HEARTBEAT_MAP: &[(&str, &str)] = &[("aos-secd.sock", "aos-secd.heartbeat")];

/// Remove stale runtime state from `var_dir`.
///
/// Called early in boot Phase 1 (before any service tries to bind sockets).
///
/// - Removes the `system_ready` marker so the health check must re-create it.
/// - Removes the `training-worker.degraded` marker to clear stale degraded state.
/// - Probes each known UDS socket: if nothing is listening the socket file
///   (and its associated heartbeat, if any) is removed.
pub fn clean_stale_runtime_state(var_dir: &Path) {
    let run_dir = var_dir.join("run");
    if !run_dir.exists() {
        return;
    }

    // Remove system_ready marker — health check will re-create it once ready.
    let system_ready = run_dir.join("system_ready");
    if system_ready.exists() {
        match std::fs::remove_file(&system_ready) {
            Ok(()) => info!(path = %system_ready.display(), "Removed stale system_ready marker"),
            Err(e) => {
                warn!(path = %system_ready.display(), error = %e, "Failed to remove system_ready marker")
            }
        }
    }

    // Remove degraded marker — will be re-set by the training worker if needed.
    let degraded = run_dir.join("training-worker.degraded");
    if degraded.exists() {
        match std::fs::remove_file(&degraded) {
            Ok(()) => {
                info!(path = %degraded.display(), "Removed stale training-worker.degraded marker")
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Race: removed between exists() and remove_file().
            }
            Err(e) => {
                warn!(path = %degraded.display(), error = %e, "Failed to remove training-worker.degraded marker")
            }
        }
    }

    // Probe each known socket. Remove stale ones and their associated heartbeat files.
    for &socket_name in KNOWN_SOCKETS {
        let socket_path = run_dir.join(socket_name);
        if !socket_path.exists() {
            continue;
        }

        if is_socket_live(&socket_path) {
            info!(socket = socket_name, "Socket is live, preserving");
            continue;
        }

        // Socket is stale — remove it.
        match std::fs::remove_file(&socket_path) {
            Ok(()) => info!(socket = socket_name, "Removed stale socket"),
            Err(e) => {
                warn!(socket = socket_name, error = %e, "Failed to remove stale socket");
                continue;
            }
        }

        // Clean associated heartbeat file, if any.
        for &(sock, heartbeat) in SOCKET_HEARTBEAT_MAP {
            if sock == socket_name {
                let heartbeat_path = run_dir.join(heartbeat);
                if heartbeat_path.exists() {
                    match std::fs::remove_file(&heartbeat_path) {
                        Ok(()) => info!(heartbeat = heartbeat, "Removed associated heartbeat file"),
                        Err(e) => {
                            warn!(heartbeat = heartbeat, error = %e, "Failed to remove heartbeat file")
                        }
                    }
                }
            }
        }
    }
}

/// Check whether a UDS socket has an active listener.
///
/// Returns `true` if `UnixStream::connect` succeeds (something is bound),
/// `false` on any error (connection refused, not found, permission denied, etc.).
fn is_socket_live(path: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use tempfile::TempDir;

    /// Helper: create the `run/` subdirectory inside a temp dir.
    fn make_run_dir(tmp: &TempDir) -> std::path::PathBuf {
        let run = tmp.path().join("run");
        std::fs::create_dir_all(&run).unwrap();
        run
    }

    #[test]
    fn test_stale_socket_cleanup() {
        let tmp = TempDir::new().unwrap();
        let run = make_run_dir(&tmp);

        // Create a plain file masquerading as a socket (no listener).
        let sock = run.join("worker.sock");
        std::fs::write(&sock, b"").unwrap();

        clean_stale_runtime_state(tmp.path());

        assert!(!sock.exists(), "Stale socket should have been removed");
    }

    #[test]
    fn test_live_socket_preserved() {
        let tmp = TempDir::new().unwrap();
        let run = make_run_dir(&tmp);

        let sock_path = run.join("worker.sock");
        let _listener = UnixListener::bind(&sock_path).unwrap();

        clean_stale_runtime_state(tmp.path());

        assert!(sock_path.exists(), "Live socket should be preserved");
    }

    #[test]
    fn test_system_ready_removed() {
        let tmp = TempDir::new().unwrap();
        let run = make_run_dir(&tmp);

        let marker = run.join("system_ready");
        std::fs::write(&marker, b"1").unwrap();

        clean_stale_runtime_state(tmp.path());

        assert!(!marker.exists(), "system_ready marker should be removed");
    }

    #[test]
    fn test_degraded_marker_removed() {
        let tmp = TempDir::new().unwrap();
        let run = make_run_dir(&tmp);

        let marker = run.join("training-worker.degraded");
        std::fs::write(&marker, b"degraded").unwrap();

        clean_stale_runtime_state(tmp.path());

        assert!(
            !marker.exists(),
            "training-worker.degraded marker should be removed"
        );
    }

    #[test]
    fn test_heartbeat_cleaned_with_socket() {
        let tmp = TempDir::new().unwrap();
        let run = make_run_dir(&tmp);

        // Create stale socket and its companion heartbeat.
        let sock = run.join("aos-secd.sock");
        std::fs::write(&sock, b"").unwrap();
        let hb = run.join("aos-secd.heartbeat");
        std::fs::write(&hb, b"").unwrap();

        clean_stale_runtime_state(tmp.path());

        assert!(!sock.exists(), "Stale socket should be removed");
        assert!(!hb.exists(), "Associated heartbeat should be removed");
    }

    #[test]
    fn test_nonexistent_run_dir() {
        let tmp = TempDir::new().unwrap();
        // Do NOT create run/ — verify no panic.
        clean_stale_runtime_state(tmp.path());
    }
}
