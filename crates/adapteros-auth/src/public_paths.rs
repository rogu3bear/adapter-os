//! Public path allowlist for authentication bypass.
//!
//! These paths are explicitly allowed to be accessed without authentication.
//! All other paths require authentication.

/// List of public paths that don't require authentication.
///
/// These paths are matched as prefixes, so `/healthz` also matches `/healthz/live`.
///
/// SECURITY: Any route not in this list requires authentication. Adding routes
/// to this list should be done carefully and reviewed for security implications.
pub const PUBLIC_PATHS: &[&str] = &[
    // Health check endpoints (must be fast and unauthenticated for probes)
    "/healthz",
    "/readyz",
    "/livez",
    "/version",
    // System status (public portion)
    "/system/ready",
    // Metrics endpoint (Prometheus scraping - may have separate bearer token)
    "/metrics",
    "/v1/metrics",
    // Boot invariant status (for monitoring)
    "/v1/invariants",
    // Meta and status (public info)
    "/v1/meta",
    "/v1/status",
    "/v1/version",
    "/v1/search",
    // Auth endpoints that must work without existing auth
    "/v1/auth/login",
    "/v1/auth/register",
    "/v1/auth/refresh",
    "/v1/auth/config",
    "/v1/auth/health",
    "/v1/auth/bootstrap",
    "/v1/auth/dev-bypass", // Dev mode bypass (feature-gated, debug only)
    "/v1/dev/bootstrap",   // Dev mode bootstrap (feature-gated, debug only)
    // Anonymous telemetry (for pre-auth error reporting)
    "/v1/telemetry/client-errors/anonymous",
    // Admin lifecycle (typically localhost-only, may need separate auth)
    "/admin/lifecycle/request-shutdown",
    "/admin/lifecycle/request-maintenance",
    "/admin/lifecycle/safe-restart",
    // OpenAPI documentation
    "/swagger-ui",
    "/api-doc",
    "/api-docs",
    "/openapi.json",
    "/docs",
    // Static assets (served by frontend)
    "/static",
    "/assets",
    "/favicon.ico",
    // CORS preflight handled before auth middleware
    // OPTIONS requests bypass auth via CORS layer
];

/// Check if a path is in the public allowlist.
///
/// # Arguments
///
/// * `path` - The request path to check (without query string)
///
/// # Returns
///
/// `true` if the path is public, `false` if authentication is required.
///
/// # Examples
///
/// ```
/// use adapteros_auth::is_public_path;
///
/// assert!(is_public_path("/healthz"));
/// assert!(is_public_path("/healthz/live"));
/// assert!(is_public_path("/v1/auth/login"));
/// assert!(!is_public_path("/v1/adapters"));
/// assert!(!is_public_path("/v1/chat"));
/// ```
pub fn is_public_path(path: &str) -> bool {
    // Normalize path (remove trailing slash for consistency)
    let path = path.strip_suffix('/').unwrap_or(path);

    for public_path in PUBLIC_PATHS {
        // Exact match
        if path == *public_path {
            return true;
        }

        // Prefix match (path starts with public_path + "/")
        if let Some(remainder) = path.strip_prefix(public_path) {
            if remainder.is_empty() || remainder.starts_with('/') {
                return true;
            }
        }
    }

    false
}

/// Check if a path requires tenant context.
///
/// Some public paths are public but still require tenant context
/// when a tenant header or token is provided.
#[allow(dead_code)]
pub fn requires_tenant_context(path: &str) -> bool {
    // Most endpoints require tenant context
    // Only health/metrics/docs are truly tenant-agnostic
    let tenant_agnostic = [
        "/healthz",
        "/readyz",
        "/livez",
        "/metrics",
        "/swagger-ui",
        "/api-doc",
        "/api-docs",
        "/openapi.json",
        "/static",
        "/assets",
        "/favicon.ico",
    ];

    !tenant_agnostic
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{}/", p)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_endpoints_are_public() {
        assert!(is_public_path("/healthz"));
        assert!(is_public_path("/healthz/"));
        assert!(is_public_path("/healthz/live"));
        assert!(is_public_path("/healthz/all"));
        assert!(is_public_path("/readyz"));
        assert!(is_public_path("/livez"));
        assert!(is_public_path("/version"));
    }

    #[test]
    fn test_auth_endpoints_are_public() {
        assert!(is_public_path("/v1/auth/login"));
        assert!(is_public_path("/v1/auth/register"));
        assert!(is_public_path("/v1/auth/refresh"));
        assert!(is_public_path("/v1/auth/config"));
        assert!(is_public_path("/v1/auth/health"));
        assert!(is_public_path("/v1/auth/bootstrap"));
        assert!(is_public_path("/v1/auth/dev-bypass"));
    }

    #[test]
    fn test_api_endpoints_are_protected() {
        assert!(!is_public_path("/v1/adapters"));
        assert!(!is_public_path("/v1/chat"));
        assert!(!is_public_path("/v1/tenants"));
        assert!(!is_public_path("/v1/users"));
        assert!(!is_public_path("/v1/training"));
        assert!(!is_public_path("/v1/models"));
        assert!(!is_public_path("/v1/workers"));
        assert!(!is_public_path("/v1/plans"));
    }

    #[test]
    fn test_partial_match_doesnt_work() {
        // "/health" should not match "/healthz"
        assert!(!is_public_path("/health"));

        // "/v1/auth" alone should not match (only specific auth endpoints)
        assert!(!is_public_path("/v1/auth"));
        assert!(!is_public_path("/v1/auth/"));

        // "/v1/auth/me" is protected (requires auth)
        assert!(!is_public_path("/v1/auth/me"));
        assert!(!is_public_path("/v1/auth/logout"));
        assert!(!is_public_path("/v1/auth/sessions"));
    }

    #[test]
    fn test_swagger_is_public() {
        assert!(is_public_path("/swagger-ui"));
        assert!(is_public_path("/swagger-ui/index.html"));
        assert!(is_public_path("/api-doc"));
        assert!(is_public_path("/api-docs"));
        assert!(is_public_path("/openapi.json"));
        assert!(is_public_path("/docs"));
    }

    #[test]
    fn test_metrics_is_public() {
        assert!(is_public_path("/metrics"));
        assert!(is_public_path("/v1/metrics"));
    }

    #[test]
    fn test_static_assets_are_public() {
        assert!(is_public_path("/static"));
        assert!(is_public_path("/static/main.js"));
        assert!(is_public_path("/assets/logo.png"));
        assert!(is_public_path("/favicon.ico"));
    }

    #[test]
    fn test_meta_status_endpoints_are_public() {
        assert!(is_public_path("/v1/meta"));
        assert!(is_public_path("/v1/status"));
        assert!(is_public_path("/v1/invariants"));
        assert!(is_public_path("/v1/version"));
        assert!(is_public_path("/v1/search"));
        assert!(is_public_path("/system/ready"));
    }

    #[test]
    fn test_admin_lifecycle_is_public() {
        // Note: These are public at the HTTP level but should have
        // localhost-only restrictions at the network level
        assert!(is_public_path("/admin/lifecycle/request-shutdown"));
        assert!(is_public_path("/admin/lifecycle/request-maintenance"));
        assert!(is_public_path("/admin/lifecycle/safe-restart"));
    }

    #[test]
    fn test_anonymous_telemetry_is_public() {
        assert!(is_public_path("/v1/telemetry/client-errors/anonymous"));
        // But regular telemetry requires auth
        assert!(!is_public_path("/v1/telemetry/client-errors"));
        assert!(!is_public_path("/v1/telemetry/events"));
    }

    #[test]
    fn test_tenant_context_requirements() {
        // Health/metrics don't need tenant
        assert!(!requires_tenant_context("/healthz"));
        assert!(!requires_tenant_context("/metrics"));
        assert!(!requires_tenant_context("/swagger-ui"));

        // Auth endpoints do need tenant context
        assert!(requires_tenant_context("/v1/auth/login"));

        // API endpoints need tenant context
        assert!(requires_tenant_context("/v1/adapters"));
    }
}
