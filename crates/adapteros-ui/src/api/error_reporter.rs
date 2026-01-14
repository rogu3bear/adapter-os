//! Client-side error reporting
//!
//! Reports UI errors to the server for persistent logging.

use super::{api_base_url, ApiError};
use crate::redact_sensitive_info;
use gloo_net::http::Request;
use serde::Serialize;

/// Client error report payload (matches server's ClientErrorReport)
#[derive(Debug, Clone, Serialize)]
pub struct ClientErrorReport {
    pub error_type: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    pub user_agent: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Report an API error to the server
///
/// This is a fire-and-forget function that sends error reports asynchronously.
/// Errors during reporting are silently ignored to avoid infinite error loops.
///
/// # Arguments
/// * `error` - The API error to report
/// * `page` - Optional current page/route path
/// * `is_authenticated` - Whether the user is authenticated (determines endpoint)
pub fn report_error(error: &ApiError, page: Option<&str>, is_authenticated: bool) {
    let report = build_report(error, page);
    let base_url = api_base_url();

    // Fire-and-forget async send
    wasm_bindgen_futures::spawn_local(async move {
        let endpoint = if is_authenticated {
            format!("{}/v1/telemetry/client-errors", base_url)
        } else {
            format!("{}/v1/telemetry/client-errors/anonymous", base_url)
        };

        // Send and ignore result - we don't want error reporting to cause more errors
        let _ = Request::post(&endpoint)
            .header("Content-Type", "application/json")
            .json(&report)
            .ok()
            .map(|req| req.send());
    });
}

/// Build a ClientErrorReport from an ApiError
fn build_report(error: &ApiError, page: Option<&str>) -> ClientErrorReport {
    let (error_type, message, http_status) = match error {
        ApiError::Aborted => ("Aborted".to_string(), "Request aborted".to_string(), None),
        ApiError::Network(msg) => ("Network".to_string(), msg.clone(), None),
        ApiError::Http { status, message } => ("Http".to_string(), message.clone(), Some(*status)),
        ApiError::Unauthorized => (
            "Unauthorized".to_string(),
            "Authentication required".to_string(),
            Some(401),
        ),
        ApiError::Forbidden(msg) => ("Forbidden".to_string(), msg.clone(), Some(403)),
        ApiError::NotFound(msg) => ("NotFound".to_string(), msg.clone(), Some(404)),
        ApiError::Validation(msg) => ("Validation".to_string(), msg.clone(), Some(400)),
        ApiError::Server(msg) => ("Server".to_string(), msg.clone(), Some(500)),
        ApiError::Serialization(msg) => ("Serialization".to_string(), msg.clone(), None),
        ApiError::RateLimited { retry_after } => {
            let msg = match retry_after {
                Some(ms) => format!("Rate limited, retry after {}ms", ms),
                None => "Rate limited".to_string(),
            };
            ("RateLimited".to_string(), msg, Some(429))
        }
        ApiError::Structured {
            error,
            code,
            failure_code,
            details,
        } => {
            return ClientErrorReport {
                error_type: "Structured".to_string(),
                message: redact_sensitive_info(error),
                code: Some(code.clone()),
                failure_code: failure_code.map(|fc| format!("{:?}", fc)),
                http_status: None,
                page: page.map(|s| s.to_string()),
                user_agent: get_user_agent(),
                timestamp: current_timestamp(),
                details: details.clone(),
            };
        }
    };

    ClientErrorReport {
        error_type,
        message: redact_sensitive_info(&message),
        code: error.code().map(|s| s.to_string()),
        failure_code: error.failure_code().map(|fc| format!("{:?}", fc)),
        http_status,
        page: page.map(|s| s.to_string()),
        user_agent: get_user_agent(),
        timestamp: current_timestamp(),
        details: None,
    }
}

/// Get the current user agent string
fn get_user_agent() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.navigator().user_agent().ok())
            .unwrap_or_else(|| "Unknown".to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "Unknown".to_string()
    }
}

/// Get the current timestamp in ISO 8601 format
fn current_timestamp() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::new_0().to_iso_string().into()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        chrono::Utc::now().to_rfc3339()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_report_network_error() {
        let error = ApiError::Network("Connection refused".to_string());
        let report = build_report(&error, Some("/dashboard"));

        assert_eq!(report.error_type, "Network");
        assert_eq!(report.message, "Connection refused");
        assert_eq!(report.page, Some("/dashboard".to_string()));
        assert!(report.http_status.is_none());
    }

    #[test]
    fn test_build_report_unauthorized() {
        let error = ApiError::Unauthorized;
        let report = build_report(&error, None);

        assert_eq!(report.error_type, "Unauthorized");
        assert_eq!(report.http_status, Some(401));
        assert_eq!(report.code, Some("UNAUTHORIZED".to_string()));
    }

    #[test]
    fn test_redaction_in_report() {
        let error = ApiError::Network("Connection failed with jwt=secret123 token".to_string());
        let report = build_report(&error, None);

        // Message should be redacted
        assert!(report.message.contains("[REDACTED]"));
        assert!(!report.message.contains("secret123"));
    }
}
