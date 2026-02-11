//! Worker UID validation middleware for Unix Domain Socket connections.
//!
//! This middleware validates that UDS connections originate from processes
//! running as the expected worker UID. It provides defense-in-depth security
//! for internal routes (`/v1/workers/*`) that bypass JWT authentication.
//!
//! # Configuration
//!
//! Set `AOS_WORKER_UID` environment variable to enable UID validation:
//! ```bash
//! export AOS_WORKER_UID=1000  # Only allow connections from UID 1000
//! ```
//!
//! If `AOS_WORKER_UID` is not set, validation is skipped (backwards compatible).
//!
//! # Security Model
//!
//! Internal routes assume the caller is a trusted worker process on the same host.
//! This middleware adds defense-in-depth by verifying the connecting process's UID
//! matches the expected worker UID, preventing:
//! - Compromised processes from registering fake workers
//! - Privilege escalation through spoofed worker status updates
//!
//! # Platform Support
//!
//! UCred validation is supported on:
//! - macOS (via `LOCAL_PEERCRED` socket option)
//! - Linux (via `SO_PEERCRED` socket option)
//!
//! [source: crates/adapteros-server-api/src/middleware/worker_uid.rs L1-45]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::OnceLock;
use tracing::{debug, info, warn};

use crate::auth::is_production_mode;
use crate::types::ErrorResponse;

/// Peer credentials extracted from a UDS connection.
///
/// This struct is injected into request extensions when serving over UDS.
/// The UID/GID/PID are extracted from the socket's peer credentials.
#[derive(Clone, Debug)]
pub struct UdsPeerCredentials {
    /// User ID of the connecting process
    pub uid: u32,
    /// Group ID of the connecting process
    pub gid: u32,
    /// Process ID of the connecting process (if available)
    pub pid: Option<i32>,
}

impl UdsPeerCredentials {
    /// Create new peer credentials from raw values.
    pub fn new(uid: u32, gid: u32, pid: Option<i32>) -> Self {
        Self { uid, gid, pid }
    }

    /// Create peer credentials from a tokio UnixStream.
    ///
    /// Returns `None` if credentials cannot be extracted.
    #[cfg(unix)]
    pub fn from_unix_stream(stream: &tokio::net::UnixStream) -> Option<Self> {
        match stream.peer_cred() {
            Ok(cred) => Some(Self {
                uid: cred.uid(),
                gid: cred.gid(),
                pid: cred.pid(),
            }),
            Err(e) => {
                warn!(error = %e, "Failed to extract peer credentials from UDS connection");
                None
            }
        }
    }
}

/// Cached expected worker UID from environment.
///
/// This is read once at first access and cached for performance.
static EXPECTED_WORKER_UID: OnceLock<Option<u32>> = OnceLock::new();

/// Get the expected worker UID from environment.
///
/// Returns `Some(uid)` if `AOS_WORKER_UID` is set and valid, `None` otherwise.
fn get_expected_worker_uid() -> Option<u32> {
    *EXPECTED_WORKER_UID.get_or_init(|| match std::env::var("AOS_WORKER_UID") {
        Ok(val) => match val.parse::<u32>() {
            Ok(uid) => {
                info!(expected_uid = uid, "Worker UID validation enabled");
                Some(uid)
            }
            Err(e) => {
                warn!(
                    value = %val,
                    error = %e,
                    "Invalid AOS_WORKER_UID value, worker UID validation disabled"
                );
                None
            }
        },
        Err(_) => {
            if is_production_mode() {
                warn!(
                    "AOS_WORKER_UID not set in production mode - internal routes will require UCred validation. \
                     Set AOS_WORKER_UID to the expected worker process UID."
                );
            } else {
                debug!("AOS_WORKER_UID not set, worker UID validation disabled (backwards compatible)");
            }
            None
        }
    })
}

/// Check if worker UID validation is enabled.
pub fn is_worker_uid_validation_enabled() -> bool {
    get_expected_worker_uid().is_some()
}

/// Middleware to validate the connecting process's UID for internal routes.
///
/// This middleware:
/// 1. In production mode (AOS_PRODUCTION_MODE=1): UCred validation is MANDATORY
///    - Requests without peer credentials are rejected
///    - If AOS_WORKER_UID is set, UID must match; otherwise any authenticated UDS connection is allowed
/// 2. In development mode: Backwards compatible - allows requests without validation if AOS_WORKER_UID is not set
/// 3. Extracts `UdsPeerCredentials` from request extensions
/// 4. Validates the peer UID matches the expected worker UID (when configured)
/// 5. Rejects with 403 Forbidden if validation fails
///
/// # Example
///
/// ```rust,ignore
/// use axum::{Router, middleware};
/// use adapteros_server_api::middleware::worker_uid::worker_uid_middleware;
///
/// let internal_routes = Router::new()
///     .route("/v1/workers/register", post(register_handler))
///     .layer(middleware::from_fn(worker_uid_middleware));
/// ```
pub async fn worker_uid_middleware(req: Request<Body>, next: Next) -> Response {
    let production_mode = is_production_mode();
    let expected_uid = get_expected_worker_uid();

    // Extract peer credentials from request extensions
    let peer_creds = req.extensions().get::<UdsPeerCredentials>().cloned();

    match (production_mode, expected_uid, peer_creds) {
        // Production mode: UCred validation is MANDATORY
        (true, _, None) => {
            warn!(
                path = %req.uri().path(),
                "SECURITY: Internal route called without UCred in production mode. \
                 Internal routes require UDS connections with peer credentials in production. \
                 Ensure workers connect via Unix Domain Socket."
            );
            production_no_credentials_response()
        }

        // Production mode with expected UID: validate UID matches
        (true, Some(uid), Some(creds)) => {
            if creds.uid == uid {
                debug!(
                    peer_uid = creds.uid,
                    peer_pid = ?creds.pid,
                    path = %req.uri().path(),
                    "Worker UID validation passed (production mode)"
                );
                next.run(req).await
            } else {
                warn!(
                    peer_uid = creds.uid,
                    expected_uid = uid,
                    peer_pid = ?creds.pid,
                    path = %req.uri().path(),
                    "Worker UID validation failed: UID mismatch (production mode)"
                );
                uid_mismatch_response(creds.uid, uid)
            }
        }

        // Production mode without expected UID: accept any UDS connection with credentials
        (true, None, Some(creds)) => {
            debug!(
                peer_uid = creds.uid,
                peer_pid = ?creds.pid,
                path = %req.uri().path(),
                "Internal route allowed via UDS with credentials (production mode, no UID filter)"
            );
            next.run(req).await
        }

        // Development mode with expected UID and credentials: validate
        (false, Some(uid), Some(creds)) => {
            if creds.uid == uid {
                debug!(
                    peer_uid = creds.uid,
                    peer_pid = ?creds.pid,
                    path = %req.uri().path(),
                    "Worker UID validation passed"
                );
                next.run(req).await
            } else {
                warn!(
                    peer_uid = creds.uid,
                    expected_uid = uid,
                    peer_pid = ?creds.pid,
                    path = %req.uri().path(),
                    "Worker UID validation failed: UID mismatch"
                );
                uid_mismatch_response(creds.uid, uid)
            }
        }

        // Development mode with expected UID but no credentials
        (false, Some(_), None) => {
            warn!(
                path = %req.uri().path(),
                "Worker UID validation failed: no peer credentials available. \
                 Request may have arrived over TCP instead of UDS, or credentials \
                 were not injected."
            );
            no_credentials_response()
        }

        // Development mode without expected UID: backwards compatible - allow all
        (false, None, _) => {
            debug!(
                path = %req.uri().path(),
                "Internal route allowed (development mode, no UID validation configured)"
            );
            next.run(req).await
        }
    }
}

/// Generate a 403 Forbidden response for UID mismatch.
fn uid_mismatch_response(actual_uid: u32, expected_uid: u32) -> Response {
    let body = ErrorResponse::new("worker uid validation failed")
        .with_code("WORKER_UID_MISMATCH")
        .with_string_details(format!(
            "Peer UID {} does not match expected worker UID {}",
            actual_uid, expected_uid
        ));

    (StatusCode::FORBIDDEN, Json(body)).into_response()
}

/// Generate a 403 Forbidden response for missing credentials.
fn no_credentials_response() -> Response {
    let body = ErrorResponse::new("worker uid validation failed")
        .with_code("PEER_CREDENTIALS_MISSING")
        .with_string_details(
            "Peer credentials not available. Internal routes require UDS connections \
             with AOS_WORKER_UID validation enabled."
                .to_string(),
        );

    (StatusCode::FORBIDDEN, Json(body)).into_response()
}

/// Generate a 403 Forbidden response for missing credentials in production mode.
fn production_no_credentials_response() -> Response {
    let body = ErrorResponse::new("internal route requires UDS connection in production")
        .with_code("PRODUCTION_UCRED_REQUIRED")
        .with_string_details(
            "Production mode (AOS_PRODUCTION_MODE=1) requires internal routes to be accessed \
             via Unix Domain Socket with peer credentials. TCP connections to internal routes \
             are not permitted in production. Ensure workers connect via UDS."
                .to_string(),
        );

    (StatusCode::FORBIDDEN, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get, Router};
    use std::env;
    use tower::ServiceExt;

    // Helper to reset the OnceLock for testing
    // Note: In real tests, we'd need to use a different approach since OnceLock
    // can't be reset. These tests demonstrate the logic.

    #[allow(dead_code)]
    fn test_app() -> Router {
        Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(worker_uid_middleware))
    }

    #[tokio::test]
    async fn test_no_validation_without_env_var() {
        // When AOS_WORKER_UID is not set, requests should pass
        // Note: This test relies on the env var not being set in the test environment

        // We can't easily test this since OnceLock is initialized once.
        // In a real scenario, we'd use a different pattern for testability.
    }

    #[test]
    fn test_peer_credentials_creation() {
        let creds = UdsPeerCredentials::new(1000, 1000, Some(12345));
        assert_eq!(creds.uid, 1000);
        assert_eq!(creds.gid, 1000);
        assert_eq!(creds.pid, Some(12345));
    }

    #[test]
    fn test_peer_credentials_without_pid() {
        let creds = UdsPeerCredentials::new(0, 0, None);
        assert_eq!(creds.uid, 0);
        assert_eq!(creds.pid, None);
    }
}
