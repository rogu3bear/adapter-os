//! API versioning middleware for adapterOS
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
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use tracing::{debug, warn};

/// API version information response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiVersionInfo {
    /// Current version
    pub version: String,
    /// All supported versions
    pub supported_versions: Vec<String>,
    /// Deprecated versions
    pub deprecated_versions: Vec<DeprecatedVersionInfo>,
}

/// Information about a deprecated API version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeprecatedVersionInfo {
    /// Version identifier
    pub version: String,
    /// Deprecation date
    pub deprecated_at: String,
    /// Sunset date
    pub sunset_at: String,
    /// Migration guide URL
    pub migration_guide: String,
}

/// Get current API version information
pub fn get_version_info() -> ApiVersionInfo {
    ApiVersionInfo {
        version: "v1".to_string(),
        supported_versions: vec!["v1".to_string()],
        deprecated_versions: vec![],
    }
}

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

/// Schema version for compatibility tracking
///
/// Represents a semantic version with major, minor, and optional patch components.
/// Used to track API schema compatibility between client and server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SchemaVersion {
    /// Create a new schema version
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse a schema version from a string (e.g., "1.0.0" or "1.0")
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.trim().split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }

        let major = parts.first()?.parse().ok()?;

        // If a part exists, it must parse successfully (don't silently default to 0)
        let minor = match parts.get(1) {
            Some(s) => s.parse().ok()?,
            None => 0,
        };
        let patch = match parts.get(2) {
            Some(s) => s.parse().ok()?,
            None => 0,
        };

        Some(Self {
            major,
            minor,
            patch,
        })
    }

    /// Get the current server schema version
    pub fn current() -> Self {
        Self::parse(adapteros_api_types::API_SCHEMA_VERSION)
            .expect(
                "Failed to parse API_SCHEMA_VERSION constant as valid semver: \
                 expected API_SCHEMA_VERSION to be a compile-time constant in format 'major.minor.patch', \
                 but parsing failed. This indicates adapteros_api_types::API_SCHEMA_VERSION is malformed. \
                 This is a critical configuration error that should never occur in production."
            )
    }

    /// Check if this version is compatible with the server version
    ///
    /// Compatibility rules:
    /// - Major version must match (breaking changes)
    /// - Client minor version can be <= server minor (server has new features)
    /// - Patch differences are always compatible
    pub fn is_compatible_with(&self, server: &SchemaVersion) -> bool {
        // Major version must match
        if self.major != server.major {
            return false;
        }
        // Client minor version must be <= server minor
        // (client can be older than server, but not newer)
        self.minor <= server.minor
    }

    /// Check compatibility and return detailed result
    pub fn check_compatibility(&self, server: &SchemaVersion) -> VersionCompatibility {
        if self.major != server.major {
            VersionCompatibility::Incompatible {
                reason: format!(
                    "Major version mismatch: client {} vs server {}",
                    self.major, server.major
                ),
            }
        } else if self.minor > server.minor {
            VersionCompatibility::ClientNewer {
                client_minor: self.minor,
                server_minor: server.minor,
            }
        } else if self.minor < server.minor {
            VersionCompatibility::ServerNewer {
                client_minor: self.minor,
                server_minor: server.minor,
            }
        } else {
            VersionCompatibility::Compatible
        }
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Ord for SchemaVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => self.patch.cmp(&other.patch),
                ord => ord,
            },
            ord => ord,
        }
    }
}

impl PartialOrd for SchemaVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Result of checking version compatibility
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionCompatibility {
    /// Versions are fully compatible
    Compatible,
    /// Client is newer than server (client may use unavailable features)
    ClientNewer {
        client_minor: u32,
        server_minor: u32,
    },
    /// Server is newer than client (backward compatible)
    ServerNewer {
        client_minor: u32,
        server_minor: u32,
    },
    /// Versions are incompatible (major version mismatch)
    Incompatible { reason: String },
}

impl VersionCompatibility {
    /// Check if this compatibility result allows the request to proceed
    pub fn is_ok(&self) -> bool {
        matches!(
            self,
            VersionCompatibility::Compatible | VersionCompatibility::ServerNewer { .. }
        )
    }

    /// Check if there's a version mismatch (even if compatible)
    pub fn has_mismatch(&self) -> bool {
        !matches!(self, VersionCompatibility::Compatible)
    }
}

/// Header name for client schema version
pub const CLIENT_SCHEMA_VERSION_HEADER: &str = "X-Client-Schema-Version";

/// Header name for server schema version
pub const SERVER_SCHEMA_VERSION_HEADER: &str = "X-Schema-Version";

/// Header name for version mismatch warning
pub const VERSION_MISMATCH_HEADER: &str = "X-Version-Mismatch";

/// Extract client schema version from request headers
pub fn extract_client_schema_version(req: &Request) -> Option<SchemaVersion> {
    req.headers()
        .get(CLIENT_SCHEMA_VERSION_HEADER)
        .and_then(|h| h.to_str().ok())
        .and_then(SchemaVersion::parse)
}

/// Check version compatibility and return appropriate headers
pub fn check_version_compatibility(
    client_version: Option<SchemaVersion>,
) -> (VersionCompatibility, Option<String>) {
    let server_version = SchemaVersion::current();

    match client_version {
        Some(client) => {
            let compatibility = client.check_compatibility(&server_version);
            let mismatch_header = match &compatibility {
                VersionCompatibility::Compatible => None,
                VersionCompatibility::ServerNewer {
                    client_minor,
                    server_minor,
                } => Some(format!(
                    "server_newer; client_minor={}; server_minor={}",
                    client_minor, server_minor
                )),
                VersionCompatibility::ClientNewer {
                    client_minor,
                    server_minor,
                } => Some(format!(
                    "client_newer; client_minor={}; server_minor={}",
                    client_minor, server_minor
                )),
                VersionCompatibility::Incompatible { reason } => {
                    Some(format!("incompatible; reason=\"{}\"", reason))
                }
            };
            (compatibility, mismatch_header)
        }
        None => (VersionCompatibility::Compatible, None),
    }
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
/// Additionally checks client schema version for compatibility.
pub async fn versioning_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();

    // Extract version from path (primary method)
    let path_version = ApiVersion::from_path(&path);

    // Extract client schema version for compatibility checking
    let client_schema_version = extract_client_schema_version(&req);

    // Check version compatibility
    let (compatibility, mismatch_header) = check_version_compatibility(client_schema_version);

    // Log version mismatch warnings
    if let Some(client_ver) = client_schema_version {
        match &compatibility {
            VersionCompatibility::Incompatible { reason } => {
                warn!(
                    client_version = %client_ver,
                    server_version = %SchemaVersion::current(),
                    reason = %reason,
                    "Incompatible client schema version"
                );
            }
            VersionCompatibility::ClientNewer {
                client_minor,
                server_minor,
            } => {
                warn!(
                    client_version = %client_ver,
                    server_version = %SchemaVersion::current(),
                    client_minor = %client_minor,
                    server_minor = %server_minor,
                    "Client schema version is newer than server"
                );
            }
            VersionCompatibility::ServerNewer { .. } => {
                debug!(
                    client_version = %client_ver,
                    server_version = %SchemaVersion::current(),
                    "Client schema version is older than server (backward compatible)"
                );
            }
            VersionCompatibility::Compatible => {}
        }
    }

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

    // Add Schema-Version header for client compatibility detection
    if let Ok(schema_header) = HeaderValue::from_str(adapteros_api_types::API_SCHEMA_VERSION) {
        headers.insert(SERVER_SCHEMA_VERSION_HEADER, schema_header);
    }

    // Add version mismatch header if applicable
    if let Some(mismatch_value) = mismatch_header {
        if let Ok(mismatch_header) = HeaderValue::from_str(&mismatch_value) {
            headers.insert(VERSION_MISMATCH_HEADER, mismatch_header);
        }
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

    #[tokio::test]
    async fn sets_schema_version_header() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route("/v1/test", get(|| async { Response::new(Body::empty()) }))
            .layer(axum::middleware::from_fn(versioning_middleware));

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Verify X-Schema-Version header is set
        assert_eq!(
            resp.headers()
                .get("X-Schema-Version")
                .map(|v| v.to_str().unwrap()),
            Some(adapteros_api_types::API_SCHEMA_VERSION)
        );

        // Verify X-API-Version header is also set
        assert_eq!(
            resp.headers()
                .get("X-API-Version")
                .map(|v| v.to_str().unwrap()),
            Some("v1")
        );
    }

    // =========================================================================
    // Schema Version Parsing Tests
    // =========================================================================

    #[test]
    fn test_schema_version_parse_full_semver() {
        let v = SchemaVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_schema_version_parse_major_minor() {
        let v = SchemaVersion::parse("1.2").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_schema_version_parse_major_only() {
        let v = SchemaVersion::parse("1").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_schema_version_parse_with_whitespace() {
        let v = SchemaVersion::parse("  1.2.3  ").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_schema_version_parse_invalid() {
        assert!(SchemaVersion::parse("").is_none());
        assert!(SchemaVersion::parse("abc").is_none());
        assert!(SchemaVersion::parse("1.2.3.4").is_none());
        assert!(SchemaVersion::parse("1.a.3").is_none());
        assert!(SchemaVersion::parse("-1.2.3").is_none());
    }

    #[test]
    fn test_schema_version_display() {
        let v = SchemaVersion::new(1, 2, 3);
        assert_eq!(v.to_string(), "1.2.3");
    }

    #[test]
    fn test_schema_version_current() {
        let current = SchemaVersion::current();
        // Verify it matches the API_SCHEMA_VERSION constant
        let expected = SchemaVersion::parse(adapteros_api_types::API_SCHEMA_VERSION).unwrap();
        assert_eq!(current, expected);
    }

    // =========================================================================
    // Schema Version Ordering Tests
    // =========================================================================

    #[test]
    fn test_schema_version_ordering() {
        let v1_0_0 = SchemaVersion::new(1, 0, 0);
        let v1_0_1 = SchemaVersion::new(1, 0, 1);
        let v1_1_0 = SchemaVersion::new(1, 1, 0);
        let v2_0_0 = SchemaVersion::new(2, 0, 0);

        assert!(v1_0_0 < v1_0_1);
        assert!(v1_0_1 < v1_1_0);
        assert!(v1_1_0 < v2_0_0);
        assert!(v1_0_0 < v2_0_0);

        assert!(v2_0_0 > v1_1_0);
        assert!(v1_1_0 > v1_0_1);
    }

    #[test]
    fn test_schema_version_equality() {
        let v1 = SchemaVersion::new(1, 2, 3);
        let v2 = SchemaVersion::parse("1.2.3").unwrap();
        assert_eq!(v1, v2);
    }

    // =========================================================================
    // Version Compatibility Tests
    // =========================================================================

    #[test]
    fn test_compatibility_same_version() {
        let v1 = SchemaVersion::new(1, 0, 0);
        let v2 = SchemaVersion::new(1, 0, 0);
        assert!(v1.is_compatible_with(&v2));
        assert_eq!(
            v1.check_compatibility(&v2),
            VersionCompatibility::Compatible
        );
    }

    #[test]
    fn test_compatibility_patch_difference() {
        let client = SchemaVersion::new(1, 0, 0);
        let server = SchemaVersion::new(1, 0, 5);
        assert!(client.is_compatible_with(&server));
        assert_eq!(
            client.check_compatibility(&server),
            VersionCompatibility::Compatible
        );
    }

    #[test]
    fn test_compatibility_server_newer_minor() {
        let client = SchemaVersion::new(1, 0, 0);
        let server = SchemaVersion::new(1, 2, 0);
        assert!(client.is_compatible_with(&server));
        assert_eq!(
            client.check_compatibility(&server),
            VersionCompatibility::ServerNewer {
                client_minor: 0,
                server_minor: 2
            }
        );
    }

    #[test]
    fn test_compatibility_client_newer_minor() {
        let client = SchemaVersion::new(1, 3, 0);
        let server = SchemaVersion::new(1, 1, 0);
        // Client is newer, which means it may use features not available on server
        assert!(!client.is_compatible_with(&server));
        assert_eq!(
            client.check_compatibility(&server),
            VersionCompatibility::ClientNewer {
                client_minor: 3,
                server_minor: 1
            }
        );
    }

    #[test]
    fn test_compatibility_major_mismatch() {
        let client = SchemaVersion::new(2, 0, 0);
        let server = SchemaVersion::new(1, 5, 0);
        assert!(!client.is_compatible_with(&server));
        match client.check_compatibility(&server) {
            VersionCompatibility::Incompatible { reason } => {
                assert!(reason.contains("Major version mismatch"));
                assert!(reason.contains("client 2"));
                assert!(reason.contains("server 1"));
            }
            other => panic!("Expected Incompatible, got {:?}", other),
        }
    }

    #[test]
    fn test_compatibility_is_ok() {
        assert!(VersionCompatibility::Compatible.is_ok());
        assert!(VersionCompatibility::ServerNewer {
            client_minor: 0,
            server_minor: 1
        }
        .is_ok());
        assert!(!VersionCompatibility::ClientNewer {
            client_minor: 2,
            server_minor: 1
        }
        .is_ok());
        assert!(!VersionCompatibility::Incompatible {
            reason: "test".to_string()
        }
        .is_ok());
    }

    #[test]
    fn test_compatibility_has_mismatch() {
        assert!(!VersionCompatibility::Compatible.has_mismatch());
        assert!(VersionCompatibility::ServerNewer {
            client_minor: 0,
            server_minor: 1
        }
        .has_mismatch());
        assert!(VersionCompatibility::ClientNewer {
            client_minor: 2,
            server_minor: 1
        }
        .has_mismatch());
        assert!(VersionCompatibility::Incompatible {
            reason: "test".to_string()
        }
        .has_mismatch());
    }

    // =========================================================================
    // Version Compatibility Check Function Tests
    // =========================================================================

    #[test]
    fn test_check_version_compatibility_no_client_version() {
        let (compat, header) = check_version_compatibility(None);
        assert_eq!(compat, VersionCompatibility::Compatible);
        assert!(header.is_none());
    }

    #[test]
    fn test_check_version_compatibility_matching_version() {
        let server = SchemaVersion::current();
        let (compat, header) = check_version_compatibility(Some(server));
        assert_eq!(compat, VersionCompatibility::Compatible);
        assert!(header.is_none());
    }

    #[test]
    fn test_check_version_compatibility_server_newer() {
        // Create a client version with lower minor than server
        let server = SchemaVersion::current();
        if server.minor > 0 {
            let client = SchemaVersion::new(server.major, server.minor - 1, 0);
            let (compat, header) = check_version_compatibility(Some(client));
            assert!(matches!(compat, VersionCompatibility::ServerNewer { .. }));
            assert!(header.is_some());
            let header_val = header.unwrap();
            assert!(header_val.contains("server_newer"));
        }
    }

    #[test]
    fn test_check_version_compatibility_client_newer() {
        let server = SchemaVersion::current();
        let client = SchemaVersion::new(server.major, server.minor + 1, 0);
        let (compat, header) = check_version_compatibility(Some(client));
        assert!(matches!(compat, VersionCompatibility::ClientNewer { .. }));
        assert!(header.is_some());
        let header_val = header.unwrap();
        assert!(header_val.contains("client_newer"));
    }

    #[test]
    fn test_check_version_compatibility_incompatible() {
        let server = SchemaVersion::current();
        let client = SchemaVersion::new(server.major + 1, 0, 0);
        let (compat, header) = check_version_compatibility(Some(client));
        assert!(matches!(compat, VersionCompatibility::Incompatible { .. }));
        assert!(header.is_some());
        let header_val = header.unwrap();
        assert!(header_val.contains("incompatible"));
    }

    // =========================================================================
    // X-Client-Schema-Version Header Parsing Tests
    // =========================================================================

    #[tokio::test]
    async fn parses_client_schema_version_header() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route("/v1/test", get(|| async { Response::new(Body::empty()) }))
            .layer(axum::middleware::from_fn(versioning_middleware));

        // Request with matching client version
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/test")
                    .header(CLIENT_SCHEMA_VERSION_HEADER, "1.0.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should have schema version header in response
        assert_eq!(
            resp.headers()
                .get(SERVER_SCHEMA_VERSION_HEADER)
                .map(|v| v.to_str().unwrap()),
            Some(adapteros_api_types::API_SCHEMA_VERSION)
        );
    }

    #[tokio::test]
    async fn sets_mismatch_header_for_client_newer() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route("/v1/test", get(|| async { Response::new(Body::empty()) }))
            .layer(axum::middleware::from_fn(versioning_middleware));

        // Create a client version that's newer than server
        let server = SchemaVersion::current();
        let client_version = format!("{}.{}.0", server.major, server.minor + 5);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/test")
                    .header(CLIENT_SCHEMA_VERSION_HEADER, &client_version)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should have version mismatch header
        let mismatch_header = resp
            .headers()
            .get(VERSION_MISMATCH_HEADER)
            .map(|v| v.to_str().unwrap());
        assert!(mismatch_header.is_some());
        assert!(mismatch_header.unwrap().contains("client_newer"));
    }

    #[tokio::test]
    async fn sets_mismatch_header_for_incompatible_major() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route("/v1/test", get(|| async { Response::new(Body::empty()) }))
            .layer(axum::middleware::from_fn(versioning_middleware));

        // Create a client version with different major version
        let server = SchemaVersion::current();
        let client_version = format!("{}.0.0", server.major + 1);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/test")
                    .header(CLIENT_SCHEMA_VERSION_HEADER, &client_version)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should have version mismatch header indicating incompatibility
        let mismatch_header = resp
            .headers()
            .get(VERSION_MISMATCH_HEADER)
            .map(|v| v.to_str().unwrap());
        assert!(mismatch_header.is_some());
        assert!(mismatch_header.unwrap().contains("incompatible"));
    }

    #[tokio::test]
    async fn no_mismatch_header_for_compatible_versions() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route("/v1/test", get(|| async { Response::new(Body::empty()) }))
            .layer(axum::middleware::from_fn(versioning_middleware));

        // Use the exact server version
        let server = SchemaVersion::current();

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/test")
                    .header(CLIENT_SCHEMA_VERSION_HEADER, server.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should NOT have version mismatch header
        assert!(resp.headers().get(VERSION_MISMATCH_HEADER).is_none());
    }

    #[tokio::test]
    async fn handles_invalid_client_schema_version() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route("/v1/test", get(|| async { Response::new(Body::empty()) }))
            .layer(axum::middleware::from_fn(versioning_middleware));

        // Invalid version string should be treated as no version
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/test")
                    .header(CLIENT_SCHEMA_VERSION_HEADER, "invalid-version")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should still work, no mismatch header
        assert!(resp.headers().get(VERSION_MISMATCH_HEADER).is_none());
        // But schema version header should still be present
        assert!(resp.headers().get(SERVER_SCHEMA_VERSION_HEADER).is_some());
    }

    #[tokio::test]
    async fn handles_missing_client_schema_version() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route("/v1/test", get(|| async { Response::new(Body::empty()) }))
            .layer(axum::middleware::from_fn(versioning_middleware));

        // No client version header
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should work normally, no mismatch header
        assert!(resp.headers().get(VERSION_MISMATCH_HEADER).is_none());
        // Schema version header should be present
        assert!(resp.headers().get(SERVER_SCHEMA_VERSION_HEADER).is_some());
    }

    // =========================================================================
    // Integration Tests for Multiple Headers
    // =========================================================================

    #[tokio::test]
    async fn sets_all_version_headers_together() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route("/v2/test", get(|| async { Response::new(Body::empty()) }))
            .layer(axum::middleware::from_fn(versioning_middleware));

        let server = SchemaVersion::current();
        let client_version = format!("{}.{}.0", server.major, server.minor + 1);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v2/test")
                    .header(header::ACCEPT, "application/vnd.aos.v2+json")
                    .header(CLIENT_SCHEMA_VERSION_HEADER, &client_version)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check all headers are present
        assert_eq!(
            resp.headers()
                .get("X-API-Version")
                .map(|v| v.to_str().unwrap()),
            Some("v2")
        );
        assert_eq!(
            resp.headers()
                .get(SERVER_SCHEMA_VERSION_HEADER)
                .map(|v| v.to_str().unwrap()),
            Some(adapteros_api_types::API_SCHEMA_VERSION)
        );
        // Mismatch header should be present since client is newer
        assert!(resp.headers().get(VERSION_MISMATCH_HEADER).is_some());
    }

    #[tokio::test]
    async fn sets_server_newer_mismatch_header() {
        use axum::{body::Body, http::Request, response::Response, routing::get, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route("/v1/test", get(|| async { Response::new(Body::empty()) }))
            .layer(axum::middleware::from_fn(versioning_middleware));

        // Use major.0.0 which should be older if server minor > 0
        let server = SchemaVersion::current();
        if server.minor > 0 {
            let client_version = format!("{}.0.0", server.major);

            let resp = app
                .oneshot(
                    Request::builder()
                        .uri("/v1/test")
                        .header(CLIENT_SCHEMA_VERSION_HEADER, &client_version)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            // Should have server_newer mismatch header
            let mismatch_header = resp
                .headers()
                .get(VERSION_MISMATCH_HEADER)
                .map(|v| v.to_str().unwrap());
            assert!(mismatch_header.is_some());
            assert!(mismatch_header.unwrap().contains("server_newer"));
        }
    }
}
