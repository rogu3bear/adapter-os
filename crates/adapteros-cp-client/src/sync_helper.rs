//! Synchronous helper for panic hook context
//!
//! This module provides a blocking HTTP client for use in panic handlers,
//! where async code cannot be used. Uses `ureq` for simple blocking requests.

use std::time::Duration;

use tracing::error;

use adapteros_api_types::workers::WorkerFatalRequest;

/// Report a fatal error to the control plane synchronously
///
/// This function is designed for use in panic hooks where async code
/// is not available. It uses a blocking HTTP client with a short timeout.
///
/// # Arguments
///
/// * `cp_url` - Base URL of the control plane (e.g., "http://127.0.0.1:8080")
/// * `fatal` - The fatal error request containing worker ID, reason, and backtrace
/// * `timeout` - Maximum time to wait for the request
///
/// # Returns
///
/// * `Ok(())` if the request was sent successfully (regardless of response)
/// * `Err(message)` if the request failed to send
///
/// # Example
///
/// ```ignore
/// use adapteros_cp_client::sync_helper::report_fatal_sync;
/// use adapteros_api_types::workers::WorkerFatalRequest;
/// use std::time::Duration;
///
/// let fatal = WorkerFatalRequest {
///     worker_id: "worker-123".to_string(),
///     reason: "PANIC: index out of bounds".to_string(),
///     backtrace_snippet: Some("at src/main.rs:42".to_string()),
///     timestamp: chrono::Utc::now().to_rfc3339(),
/// };
///
/// let _ = report_fatal_sync("http://127.0.0.1:8080", fatal, Duration::from_secs(3));
/// ```
pub fn report_fatal_sync(
    cp_url: &str,
    fatal: WorkerFatalRequest,
    timeout: Duration,
) -> std::result::Result<(), String> {
    let url = format!("{}/v1/workers/fatal", cp_url);

    // Serialize the request
    let body = serde_json::to_string(&fatal).map_err(|e| format!("Failed to serialize: {}", e))?;

    // Create agent with timeout
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .new_agent();

    // Send request (best effort)
    match agent
        .post(&url)
        .header("Content-Type", "application/json")
        .send(body.as_bytes())
    {
        Ok(response) => {
            if response.status().is_success() {
                Ok(())
            } else {
                // Even non-2xx is "sent" - the CP received it
                Ok(())
            }
        }
        Err(e) => {
            error!(
                worker_id = %fatal.worker_id,
                error = %e,
                "Failed to report fatal error to CP"
            );
            Err(format!("HTTP error: {}", e))
        }
    }
}

/// Create a WorkerFatalRequest from panic information
///
/// Helper function to build a fatal request from panic details.
pub fn create_fatal_request(
    worker_id: &str,
    location: &str,
    message: &str,
    backtrace: Option<String>,
) -> WorkerFatalRequest {
    WorkerFatalRequest {
        worker_id: worker_id.to_string(),
        reason: format!("PANIC at {}: {}", location, message),
        backtrace_snippet: backtrace,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_fatal_request() {
        let req = create_fatal_request(
            "worker-123",
            "src/main.rs:42:5",
            "index out of bounds",
            Some("backtrace here".to_string()),
        );

        assert_eq!(req.worker_id, "worker-123");
        assert!(req.reason.contains("PANIC"));
        assert!(req.reason.contains("src/main.rs:42:5"));
        assert!(req.reason.contains("index out of bounds"));
        assert_eq!(req.backtrace_snippet, Some("backtrace here".to_string()));
        assert!(!req.timestamp.is_empty());
    }
}
