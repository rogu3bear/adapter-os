//! API versioning middleware for AdapterOS
//!
//! Implements version negotiation and deprecation warnings:
//! - Accept header negotiation (application/vnd.aos.v1+json, application/vnd.aos.v2+json)
//! - Automatic version detection from path (/v1/, /v2/)
//! - Deprecation warnings for old endpoints (X-API-Deprecation header)
//! - Migration path documentation
//!
//! [source: crates/adapteros-server-api/src/middleware/versioning.rs]

use axum::{
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::debug;

/// Supported API versions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiVersion {
    V1,
    V2,
}

impl ApiVersion {
    /// Parse version from Accept header (e.g., "application/vnd.aos.v1+json")
    pub fn from_accept_header(accept: &str) -> Option<Self> {
        if accept.contains("application/vnd.aos.v2+json") {
            Some(ApiVersion::V2)
        } else if accept.contains("application/vnd.aos.v1+json") {
            Some(ApiVersion::V1)
        } else {
            None
        }
    }

    /// Parse version from path (e.g., "/v1/adapters")
    pub fn from_path(path: &str) -> Option<Self> {
        if path.starts_with("/v2/") {
            Some(ApiVersion::V2)
        } else if path.starts_with("/v1/") {
            Some(ApiVersion::V1)
        } else {
            None
        }
    }

    /// Get version string
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiVersion::V1 => "v1",
            ApiVersion::V2 => "v2",
        }
    }

    /// Check if this version is deprecated
    pub fn is_deprecated(&self) -> bool {
        match self {
            ApiVersion::V1 => false, // V1 is stable
            ApiVersion::V2 => false, // V2 is beta (not deprecated)
        }
    }

    /// Get deprecation date (if deprecated)
    pub fn deprecation_date(&self) -> Option<&'static str> {
        match self {
            ApiVersion::V1 => None, // Not deprecated
            ApiVersion::V2 => None,
        }
    }

    /// Get sunset date (when version will be removed)
    pub fn sunset_date(&self) -> Option<&'static str> {
        match self {
            ApiVersion::V1 => None, // No sunset planned
            ApiVersion::V2 => None,
        }
    }
}

/// Deprecation status for endpoints
#[derive(Debug, Clone)]
pub struct DeprecationInfo {
    /// Deprecation date (RFC3339)
    pub deprecated_at: String,
    /// Sunset date - when endpoint will be removed (RFC3339)
    pub sunset_at: Option<String>,
    /// Migration guide URL
    pub migration_url: Option<String>,
    /// Replacement endpoint
    pub replacement: Option<String>,
}

impl DeprecationInfo {
    /// Create deprecation info with sunset date
    pub fn new(deprecated_at: &str, sunset_at: Option<&str>) -> Self {
        Self {
            deprecated_at: deprecated_at.to_string(),
            sunset_at: sunset_at.map(|s| s.to_string()),
            migration_url: None,
            replacement: None,
        }
    }

    /// Add migration URL
    pub fn with_migration_url(mut self, url: &str) -> Self {
        self.migration_url = Some(url.to_string());
        self
    }

    /// Add replacement endpoint
    pub fn with_replacement(mut self, endpoint: &str) -> Self {
        self.replacement = Some(endpoint.to_string());
        self
    }

    /// Format as deprecation header value
    pub fn to_header_value(&self) -> String {
        let mut parts = vec![format!("deprecated_at=\"{}\"", self.deprecated_at)];

        if let Some(sunset) = &self.sunset_at {
            parts.push(format!("sunset_at=\"{}\"", sunset));
        }

        if let Some(url) = &self.migration_url {
            parts.push(format!("migration_url=\"{}\"", url));
        }

        if let Some(replacement) = &self.replacement {
            parts.push(format!("replacement=\"{}\"", replacement));
        }

        parts.join("; ")
    }
}

/// Check if endpoint is deprecated based on path and version
pub fn check_deprecation(path: &str, _version: ApiVersion) -> Option<DeprecationInfo> {
    // Example: /v1/repositories is deprecated, use /v1/code/repositories instead
    if path == "/v1/repositories" {
        return Some(
            DeprecationInfo::new("2025-01-01T00:00:00Z", Some("2025-07-01T00:00:00Z"))
                .with_replacement("/v1/code/repositories")
                .with_migration_url("https://docs.adapteros.com/api/migrations/repositories"),
        );
    }

    // No deprecation for this endpoint
    None
}

/// API versioning middleware
///
/// Extracts API version from path or Accept header and adds version
/// information to response headers. Also adds deprecation warnings if needed.
pub async fn versioning_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();

    // Extract version from path (primary method)
    let path_version = ApiVersion::from_path(&path);

    // Detect SSE-style requests early so we can force the correct content type
    // even when proxies strip or alter the Accept header (EventSource expects
    // `text/event-stream` and will abort on vendor JSON types).
    let path_is_stream = path.contains("/stream/");

    // Extract version from Accept header (secondary method)
    let accepts_event_stream = req
        .headers()
        .get(header::ACCEPT)
        .and_then(|h| h.to_str().ok())
        .map(|v| v.contains("text/event-stream"))
        .unwrap_or(false);
    let is_sse_request = accepts_event_stream || path_is_stream;
    let accept_version = req
        .headers()
        .get(header::ACCEPT)
        .and_then(|h| h.to_str().ok())
        .and_then(ApiVersion::from_accept_header);

    // Determine final version (path takes precedence)
    let version = path_version.or(accept_version).unwrap_or(ApiVersion::V1);

    debug!(
        path = %path,
        version = %version.as_str(),
        "API version detected"
    );

    // Process request
    let mut response = next.run(req).await;

    // Check status before borrowing headers
    let status = response.status();

    // Add version headers to response
    let headers = response.headers_mut();

    // Add API-Version header
    if let Ok(version_header) = HeaderValue::from_str(version.as_str()) {
        headers.insert("X-API-Version", version_header);
    }

    // Add deprecation warning if applicable
    if let Some(deprecation) = check_deprecation(&path, version) {
        if let Ok(deprecation_header) = HeaderValue::from_str(&deprecation.to_header_value()) {
            headers.insert("X-API-Deprecation", deprecation_header);
        }

        // Also add Sunset header (RFC 8594) if sunset date is set
        if let Some(sunset_at) = &deprecation.sunset_at {
            if let Ok(sunset_header) = HeaderValue::from_str(sunset_at) {
                headers.insert("Sunset", sunset_header);
            }
        }
    }

    let has_content_type = headers.get(header::CONTENT_TYPE).is_some();

    // Add or correct Content-Type
    if status == StatusCode::OK {
        if is_sse_request {
            // Ensure SSE endpoints always advertise the correct MIME type
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
        } else if !has_content_type {
            let content_type = format!("application/vnd.aos.{}+json", version.as_str());
            if let Ok(content_type_header) = HeaderValue::from_str(&content_type) {
                headers.insert(header::CONTENT_TYPE, content_type_header);
            }
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_from_accept_header() {
        assert_eq!(
            ApiVersion::from_accept_header("application/vnd.aos.v1+json"),
            Some(ApiVersion::V1)
        );
        assert_eq!(
            ApiVersion::from_accept_header("application/vnd.aos.v2+json"),
            Some(ApiVersion::V2)
        );
        assert_eq!(ApiVersion::from_accept_header("application/json"), None);
    }

    #[test]
    fn test_version_from_path() {
        assert_eq!(ApiVersion::from_path("/v1/adapters"), Some(ApiVersion::V1));
        assert_eq!(ApiVersion::from_path("/v2/adapters"), Some(ApiVersion::V2));
        assert_eq!(ApiVersion::from_path("/healthz"), None);
    }

    #[test]
    fn test_deprecation_info() {
        let info = DeprecationInfo::new("2025-01-01T00:00:00Z", Some("2025-07-01T00:00:00Z"))
            .with_replacement("/v2/new-endpoint")
            .with_migration_url("https://docs.example.com/migration");

        let header = info.to_header_value();
        assert!(header.contains("deprecated_at=\"2025-01-01T00:00:00Z\""));
        assert!(header.contains("sunset_at=\"2025-07-01T00:00:00Z\""));
        assert!(header.contains("replacement=\"/v2/new-endpoint\""));
        assert!(header.contains("migration_url=\"https://docs.example.com/migration\""));
    }

    #[test]
    fn test_check_deprecation() {
        // Deprecated endpoint
        let deprecation = check_deprecation("/v1/repositories", ApiVersion::V1);
        assert!(deprecation.is_some());
        let info = deprecation.unwrap();
        assert_eq!(info.replacement.as_deref(), Some("/v1/code/repositories"));

        // Non-deprecated endpoint
        assert!(check_deprecation("/v1/adapters", ApiVersion::V1).is_none());
    }

    #[tokio::test]
    async fn sets_event_stream_content_type_when_accepting_sse() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route(
                "/v1/stream/notifications",
                get(|| async { Response::new(Body::empty()) }),
            )
            .layer(axum::middleware::from_fn(versioning_middleware));

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/stream/notifications")
                    .header(header::ACCEPT, "text/event-stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.headers()
                .get(header::CONTENT_TYPE)
                .map(HeaderValue::as_bytes),
            Some(HeaderValue::from_static("text/event-stream").as_bytes())
        );
    }

    #[tokio::test]
    async fn sets_event_stream_content_type_for_stream_path_with_vendor_accept() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route(
                "/v1/stream/notifications",
                get(|| async { Response::new(Body::empty()) }),
            )
            .layer(axum::middleware::from_fn(versioning_middleware));

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/stream/notifications")
                    .header(header::ACCEPT, "application/vnd.aos.v1+json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.headers()
                .get(header::CONTENT_TYPE)
                .map(HeaderValue::as_bytes),
            Some(HeaderValue::from_static("text/event-stream").as_bytes())
        );
    }
}
