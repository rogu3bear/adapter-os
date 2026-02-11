//! Client-side diagnostic bundle for error surfacing
//!
//! Provides a structured diagnostic bundle that can be copied to clipboard
//! for error reporting and debugging. Includes E-code, trace ID, timestamp,
//! and context information.

use super::ApiError;
use serde::Serialize;

/// Diagnostic bundle for error reporting.
///
/// Contains all information needed to debug an error:
/// - Error code (E-codes from the error registry)
/// - Trace ID (for correlation with server logs)
/// - Timestamp (ISO 8601)
/// - Page context
/// - Error details
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticBundle {
    /// Schema version for bundle format
    pub schema_version: &'static str,
    /// Error code (e.g., "E-1001", "UNAUTHORIZED")
    pub error_code: String,
    /// Human-readable error message
    pub message: String,
    /// Trace ID for server-side correlation (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Preferred debug id for support (err-... when present, else req-...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_id: Option<String>,
    /// Request ID for correlation (req-...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// UI session id for correlation (ses-...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// AdapterOS diagnostic trace id (trc-...) when present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diag_trace_id: Option<String>,
    /// W3C trace id (32-hex) when present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otel_trace_id: Option<String>,
    /// Stable fingerprint for bucketing/dedupe
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// ISO 8601 timestamp when the error occurred
    pub timestamp: String,
    /// Current page/route when error occurred
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// HTTP status code (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    /// Error type classification
    pub error_type: String,
    /// Whether the error is retryable
    pub retryable: bool,
    /// UI build version
    pub ui_version: String,
    /// UI build ID (set via AOS_BUILD_ID at compile time)
    pub ui_build_id: String,
    /// User agent
    pub user_agent: String,
    /// Additional context details (JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl DiagnosticBundle {
    /// Create a diagnostic bundle from an API error.
    pub fn from_error(error: &ApiError, page: Option<&str>) -> Self {
        let (error_type, http_status) = match error {
            ApiError::Aborted => ("Aborted".to_string(), None),
            ApiError::Network(_) => ("Network".to_string(), None),
            ApiError::Http { status, .. } => ("Http".to_string(), Some(*status)),
            ApiError::Unauthorized => ("Unauthorized".to_string(), Some(401)),
            ApiError::Forbidden(_) => ("Forbidden".to_string(), Some(403)),
            ApiError::NotFound(_) => ("NotFound".to_string(), Some(404)),
            ApiError::Validation(_) => ("Validation".to_string(), Some(400)),
            ApiError::Server(_) => ("Server".to_string(), Some(500)),
            ApiError::Serialization(_) => ("Serialization".to_string(), None),
            ApiError::RateLimited { .. } => ("RateLimited".to_string(), Some(429)),
            ApiError::Structured { .. } => ("Structured".to_string(), None),
        };

        let mut debug_id = None;
        let mut request_id = None;
        let mut session_id = None;
        let mut diag_trace_id = None;
        let mut otel_trace_id = None;
        let mut fingerprint = None;

        let details = match error {
            ApiError::Structured {
                request_id: rid,
                error_id,
                session_id: sid,
                diag_trace_id: dtid,
                otel_trace_id: otid,
                fingerprint: fp,
                details,
                ..
            } => {
                debug_id = error_id.clone().or_else(|| rid.clone());
                request_id = rid.clone();
                session_id = sid.clone();
                diag_trace_id = dtid.clone();
                otel_trace_id = otid.clone();
                fingerprint = fp.clone();
                details.clone()
            }
            _ => None,
        };

        let trace_id = otel_trace_id.clone();

        Self {
            schema_version: "1.0.0",
            error_code: error.code().unwrap_or("UNKNOWN").to_string(),
            message: error.to_string(),
            trace_id,
            debug_id,
            request_id,
            session_id,
            diag_trace_id,
            otel_trace_id,
            fingerprint,
            timestamp: current_timestamp(),
            page: page.map(|s| s.to_string()),
            http_status,
            error_type,
            retryable: error.is_retryable(),
            ui_version: super::ui_build_version().to_string(),
            ui_build_id: option_env!("AOS_BUILD_ID").unwrap_or("unknown").to_string(),
            user_agent: get_user_agent(),
            details,
        }
    }

    /// Convert to pretty-printed JSON string.
    pub fn to_json_string(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| {
            // Fallback to simple format if JSON serialization fails
            format!(
                "Error: {} ({})\nTimestamp: {}\nPage: {}",
                self.message,
                self.error_code,
                self.timestamp,
                self.page.as_deref().unwrap_or("unknown")
            )
        })
    }

    /// Convert to compact single-line format for logging.
    pub fn to_compact_string(&self) -> String {
        let trace = self
            .trace_id
            .as_ref()
            .map(|t| format!(" trace={}", t))
            .unwrap_or_default();
        format!(
            "[{}] {} - {}{} ({})",
            self.error_code, self.error_type, self.message, trace, self.timestamp
        )
    }
}

/// Get current timestamp in ISO 8601 format.
fn current_timestamp() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::new_0().to_iso_string().into()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::utils::now_utc().to_rfc3339()
    }
}

/// Get the user agent string.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_from_network_error() {
        let error = ApiError::Network("Connection refused".to_string());
        let bundle = DiagnosticBundle::from_error(&error, Some("/dashboard"));

        assert_eq!(bundle.error_code, "UNKNOWN");
        assert_eq!(bundle.error_type, "Network");
        assert_eq!(bundle.page, Some("/dashboard".to_string()));
        assert!(bundle.retryable);
        assert!(bundle.http_status.is_none());
    }

    #[test]
    fn test_bundle_from_unauthorized() {
        let error = ApiError::Unauthorized;
        let bundle = DiagnosticBundle::from_error(&error, None);

        assert_eq!(bundle.error_code, "UNAUTHORIZED");
        assert_eq!(bundle.http_status, Some(401));
        assert!(!bundle.retryable);
    }

    #[test]
    fn test_bundle_to_json() {
        let error = ApiError::Server("Internal error".to_string());
        let bundle = DiagnosticBundle::from_error(&error, Some("/api/test"));
        let json = bundle.to_json_string();

        assert!(json.contains("SERVER_ERROR"));
        assert!(json.contains("Internal error"));
        assert!(json.contains("/api/test"));
    }

    #[test]
    fn test_bundle_compact_string() {
        let error = ApiError::NotFound("Resource not found".to_string());
        let bundle = DiagnosticBundle::from_error(&error, None);
        let compact = bundle.to_compact_string();

        assert!(compact.contains("NOT_FOUND"));
        assert!(compact.contains("NotFound"));
    }
}
