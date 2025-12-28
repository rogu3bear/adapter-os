//! OpenAPI coverage validation
//!
//! Ensures all routes registered in routes.rs have corresponding
//! utoipa annotations for OpenAPI documentation.
//!
//! This prevents API drift where backend endpoints exist but are
//! not documented in the OpenAPI spec.

use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Extract handler function references from the ApiDoc paths(...) macro.
/// These are the handlers documented in OpenAPI.
fn extract_documented_handlers(routes_content: &str) -> Result<HashSet<String>> {
    let mut handlers = HashSet::new();

    // Find the paths(...) macro content - it spans multiple lines
    // Pattern: paths( ... ) where content includes handler references
    let paths_re = Regex::new(r"(?s)paths\(\s*(.*?)\s*\),")?;

    if let Some(cap) = paths_re.captures(routes_content) {
        let paths_content = &cap[1];

        // Extract handler references like:
        // - handlers::health
        // - handlers::auth::auth_login
        // - crate::health::check_all_health
        let handler_re = Regex::new(r"(handlers::[a-zA-Z_][a-zA-Z0-9_:]*|crate::[a-zA-Z_][a-zA-Z0-9_:]*)")?;

        for cap in handler_re.captures_iter(paths_content) {
            let handler = cap[1].to_string();
            // Normalize: remove module paths to get base handler name
            handlers.insert(normalize_handler(&handler));
        }
    }

    Ok(handlers)
}

/// Extract handler functions from .route() registrations.
/// These are the handlers actually served by the router.
fn extract_registered_handlers(routes_content: &str) -> Result<HashSet<String>> {
    let mut handlers = HashSet::new();

    // Match patterns like:
    // .route("/healthz", get(handlers::health))
    // .route("/v1/auth/login", post(auth::auth_login))
    // .route("/v1/boot/report", get(handlers::health::get_boot_report))
    let route_re = Regex::new(
        r"\.route\([^,]+,\s*(?:get|post|put|delete|patch)\(([a-zA-Z_][a-zA-Z0-9_:]*)\)"
    )?;

    for cap in route_re.captures_iter(routes_content) {
        let handler = cap[1].to_string();
        handlers.insert(normalize_handler(&handler));
    }

    Ok(handlers)
}

/// Normalize handler reference to a consistent format for comparison.
/// Strips common prefixes and extracts the function name.
fn normalize_handler(handler: &str) -> String {
    // For comparison, we use the full path as-is since handlers::auth::login
    // and auth::login could be different depending on imports
    handler.to_string()
}

/// Check if a handler is in a known exception list (internal handlers, etc.)
fn is_exception(handler: &str) -> bool {
    // These are internal handlers that don't need OpenAPI documentation
    let exceptions = [
        "handlers::metrics_handler",  // Prometheus metrics endpoint
        "handlers::inference_ready",  // Internal readiness check
        "handlers::meta",             // Internal metadata
        "handlers::receive_worker_fatal", // Internal worker communication
    ];

    exceptions.iter().any(|e| handler.contains(e) || e.contains(handler))
}

pub fn run() -> Result<()> {
    println!("🔍 Checking OpenAPI route coverage...\n");

    let routes_path = Path::new("crates/adapteros-server-api/src/routes.rs");

    if !routes_path.exists() {
        anyhow::bail!(
            "routes.rs not found at {}. Run from project root.",
            routes_path.display()
        );
    }

    let routes_content = fs::read_to_string(routes_path)
        .context("Failed to read routes.rs")?;

    let documented = extract_documented_handlers(&routes_content)?;
    let registered = extract_registered_handlers(&routes_content)?;

    println!("📋 Documented handlers in ApiDoc paths(): {}", documented.len());
    println!("📋 Registered route handlers: {}", registered.len());

    // Find handlers registered but not documented (excluding exceptions)
    let missing: Vec<_> = registered
        .difference(&documented)
        .filter(|h| !is_exception(h))
        .collect();

    // Find handlers documented but not registered (might be stale)
    let stale: Vec<_> = documented
        .difference(&registered)
        .filter(|h| !is_exception(h))
        .collect();

    let mut has_issues = false;

    if !missing.is_empty() {
        println!("\n❌ Found {} routes without OpenAPI documentation:", missing.len());
        for handler in &missing {
            println!("  - {}", handler);
        }
        println!("\n💡 To fix: Add #[utoipa::path(...)] annotations to these handlers");
        println!("   and include them in the ApiDoc paths(...) macro in routes.rs.");
        has_issues = true;
    }

    if !stale.is_empty() {
        println!("\n⚠️  Found {} documented handlers not in routes (possibly stale):", stale.len());
        for handler in &stale {
            println!("  - {}", handler);
        }
        println!("\n💡 These handlers are in paths() but may not be registered as routes.");
        println!("   This could indicate stale documentation or handlers in other routers.");
    }

    if !has_issues {
        println!("\n✅ All registered routes have OpenAPI documentation!");
        Ok(())
    } else {
        anyhow::bail!("{} routes missing OpenAPI documentation", missing.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_documented_handlers() {
        let content = r#"
#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health,
        handlers::auth::auth_login,
        crate::health::check_all_health,
    ),
    components(schemas(...))
)]
pub struct ApiDoc;
"#;
        let handlers = extract_documented_handlers(content).unwrap();
        assert!(handlers.contains("handlers::health"));
        assert!(handlers.contains("handlers::auth::auth_login"));
        assert!(handlers.contains("crate::health::check_all_health"));
    }

    #[test]
    fn test_extract_registered_handlers() {
        let content = r#"
Router::new()
    .route("/healthz", get(handlers::health))
    .route("/v1/auth/login", post(auth::auth_login))
    .route("/v1/boot/report", get(handlers::health::get_boot_report))
"#;
        let handlers = extract_registered_handlers(content).unwrap();
        assert!(handlers.contains("handlers::health"));
        assert!(handlers.contains("auth::auth_login"));
        assert!(handlers.contains("handlers::health::get_boot_report"));
    }
}
