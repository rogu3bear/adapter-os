//! API versioning and deprecation management
//!
//! Implements:
//! - Path-based versioning (/v1/, /v2/)
//! - Accept header negotiation (Accept: application/vnd.aos.v1+json)
//! - Deprecation warnings (X-API-Deprecation header)
//! - Migration path documentation
//!
//! Citations:
//! - API versioning best practices: REST API standards
//! - Deprecation headers: RFC 8594

use axum::{
    extract::Request,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

/// Supported API versions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiVersion {
    V1,
    V2,
}

impl ApiVersion {
    /// Parse version from path (e.g., "/v1/adapters" -> V1)
    pub fn from_path(path: &str) -> Option<Self> {
        if path.starts_with("/v1/") || path == "/v1" {
            Some(ApiVersion::V1)
        } else if path.starts_with("/v2/") || path == "/v2" {
            Some(ApiVersion::V2)
        } else {
            None
        }
    }

    /// Parse version from Accept header (e.g., "application/vnd.aos.v1+json" -> V1)
    pub fn from_accept_header(accept: &str) -> Option<Self> {
        if accept.contains("vnd.aos.v1") {
            Some(ApiVersion::V1)
        } else if accept.contains("vnd.aos.v2") {
            Some(ApiVersion::V2)
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

    /// Get content type for this version
    pub fn content_type(&self) -> &'static str {
        match self {
            ApiVersion::V1 => "application/vnd.aos.v1+json",
            ApiVersion::V2 => "application/vnd.aos.v2+json",
        }
    }

    /// Check if this version is deprecated
    pub fn is_deprecated(&self) -> bool {
        match self {
            ApiVersion::V1 => false, // V1 is current
            ApiVersion::V2 => false, // V2 is future
        }
    }

    /// Get deprecation info if deprecated
    pub fn deprecation_info(&self) -> Option<DeprecationInfo> {
        if !self.is_deprecated() {
            return None;
        }

        // Example: if V1 becomes deprecated in future
        match self {
            ApiVersion::V1 => None,
            ApiVersion::V2 => None,
        }
    }
}

/// Deprecation information for API versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeprecationInfo {
    /// Deprecation date (RFC3339)
    pub deprecated_at: String,
    /// Sunset date when API will be removed (RFC3339)
    pub sunset_at: String,
    /// Migration guide URL
    pub migration_guide: String,
    /// Replacement version
    pub replacement_version: ApiVersion,
}

/// API version negotiation result
#[derive(Debug)]
pub struct VersionNegotiation {
    /// Negotiated version
    pub version: ApiVersion,
    /// Source of version (path, header, default)
    pub source: VersionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionSource {
    Path,
    AcceptHeader,
    Default,
}

/// Negotiate API version from request
pub fn negotiate_version(path: &str, headers: &HeaderMap) -> VersionNegotiation {
    // Priority 1: Path-based versioning
    if let Some(version) = ApiVersion::from_path(path) {
        debug!(version = ?version, source = "path", "API version negotiated");
        return VersionNegotiation {
            version,
            source: VersionSource::Path,
        };
    }

    // Priority 2: Accept header negotiation
    if let Some(accept) = headers.get(header::ACCEPT) {
        if let Ok(accept_str) = accept.to_str() {
            if let Some(version) = ApiVersion::from_accept_header(accept_str) {
                debug!(version = ?version, source = "accept_header", "API version negotiated");
                return VersionNegotiation {
                    version,
                    source: VersionSource::AcceptHeader,
                };
            }
        }
    }

    // Default: V1
    debug!(version = ?ApiVersion::V1, source = "default", "API version negotiated");
    VersionNegotiation {
        version: ApiVersion::V1,
        source: VersionSource::Default,
    }
}

/// Add versioning headers to response
pub fn add_version_headers(mut response: Response, version: ApiVersion) -> Response {
    let headers = response.headers_mut();

    // Add Content-Type with version
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(version.content_type()),
    );

    // Add API version header
    headers.insert(
        "X-API-Version",
        HeaderValue::from_static(version.as_str()),
    );

    // Add deprecation warning if deprecated
    if let Some(deprecation) = version.deprecation_info() {
        headers.insert(
            "Deprecation",
            HeaderValue::from_str(&deprecation.deprecated_at).unwrap_or_else(|_| {
                HeaderValue::from_static("true")
            }),
        );

        headers.insert(
            "Sunset",
            HeaderValue::from_str(&deprecation.sunset_at).unwrap_or_else(|_| {
                HeaderValue::from_static("2025-12-31")
            }),
        );

        headers.insert(
            "Link",
            HeaderValue::from_str(&format!(
                r#"<{}>; rel="deprecation""#,
                deprecation.migration_guide
            ))
            .unwrap_or_else(|_| {
                HeaderValue::from_static(r#"</docs/migration>; rel="deprecation""#)
            }),
        );

        warn!(
            version = ?version,
            sunset = %deprecation.sunset_at,
            "Deprecated API version used"
        );
    }

    response
}

/// Versioning middleware
pub async fn versioning_middleware(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let headers = request.headers();

    // Negotiate version
    let negotiation = negotiate_version(path, headers);

    // Process request
    let response = next.run(request).await;

    // Add version headers to response
    add_version_headers(response, negotiation.version)
}

/// API version info response
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiVersionInfo {
    /// Current version
    pub version: String,
    /// All supported versions
    pub supported_versions: Vec<String>,
    /// Deprecated versions
    pub deprecated_versions: Vec<DeprecatedVersionInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
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

/// Get API version information
pub fn get_version_info() -> ApiVersionInfo {
    ApiVersionInfo {
        version: "v1".to_string(),
        supported_versions: vec!["v1".to_string()],
        deprecated_versions: vec![],
    }
}

/// Migration guide for API versions
#[derive(Debug, Serialize, Deserialize)]
pub struct MigrationGuide {
    /// Source version
    pub from_version: String,
    /// Target version
    pub to_version: String,
    /// Breaking changes
    pub breaking_changes: Vec<BreakingChange>,
    /// Migration steps
    pub steps: Vec<MigrationStep>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BreakingChange {
    /// Endpoint affected
    pub endpoint: String,
    /// Description of change
    pub description: String,
    /// Required action
    pub action: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MigrationStep {
    /// Step number
    pub step: u32,
    /// Description
    pub description: String,
    /// Code example (optional)
    pub example: Option<String>,
}

/// Get migration guide between versions
pub fn get_migration_guide(from: ApiVersion, to: ApiVersion) -> MigrationGuide {
    MigrationGuide {
        from_version: from.as_str().to_string(),
        to_version: to.as_str().to_string(),
        breaking_changes: vec![],
        steps: vec![
            MigrationStep {
                step: 1,
                description: "Update API endpoints to use new version prefix".to_string(),
                example: Some("Change /v1/adapters to /v2/adapters".to_string()),
            },
            MigrationStep {
                step: 2,
                description: "Update Accept headers to request new version".to_string(),
                example: Some("Accept: application/vnd.aos.v2+json".to_string()),
            },
            MigrationStep {
                step: 3,
                description: "Test all endpoints with new version".to_string(),
                example: None,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_from_path() {
        assert_eq!(ApiVersion::from_path("/v1/adapters"), Some(ApiVersion::V1));
        assert_eq!(ApiVersion::from_path("/v2/adapters"), Some(ApiVersion::V2));
        assert_eq!(ApiVersion::from_path("/adapters"), None);
    }

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
        assert_eq!(
            ApiVersion::from_accept_header("application/json"),
            None
        );
    }

    #[test]
    fn test_negotiate_version_path() {
        let headers = HeaderMap::new();
        let result = negotiate_version("/v1/adapters", &headers);
        assert_eq!(result.version, ApiVersion::V1);
        assert_eq!(result.source, VersionSource::Path);
    }

    #[test]
    fn test_negotiate_version_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/vnd.aos.v2+json"),
        );
        let result = negotiate_version("/adapters", &headers);
        assert_eq!(result.version, ApiVersion::V2);
        assert_eq!(result.source, VersionSource::AcceptHeader);
    }

    #[test]
    fn test_negotiate_version_default() {
        let headers = HeaderMap::new();
        let result = negotiate_version("/adapters", &headers);
        assert_eq!(result.version, ApiVersion::V1);
        assert_eq!(result.source, VersionSource::Default);
    }
}
