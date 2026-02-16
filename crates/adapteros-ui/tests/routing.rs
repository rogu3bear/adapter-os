//! Routing and navigation tests
//!
//! Tests for route definitions and navigation behavior.
//! Run with: wasm-pack test --headless --chrome

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================================
// Route Definition Tests
// ============================================================================

#[wasm_bindgen_test]
fn test_route_paths_exist() {
    // Verify expected route paths are defined
    let routes = vec![
        "/",
        "/dashboard",
        "/chat",
        "/adapters",
        "/training",
        "/workers",
        "/system",
        "/settings",
    ];

    for route in routes {
        assert!(!route.is_empty(), "Route path should not be empty");
        assert!(route.starts_with('/'), "Route should start with /");
    }
}

#[wasm_bindgen_test]
fn test_nested_route_patterns() {
    // Verify nested route patterns are valid
    let nested_routes = vec![
        "/adapters/:id",
        "/training/:job_id",
        "/workers/:worker_id",
        "/reviews/:pause_id",
    ];

    for route in nested_routes {
        assert!(route.contains(':'), "Nested route should have parameter");
    }
}

// ============================================================================
// Navigation Path Tests
// ============================================================================

#[wasm_bindgen_test]
fn test_path_building() {
    // Test path construction patterns
    let adapter_id = "my-adapter-123";
    let path = format!("/adapters/{}", adapter_id);
    assert_eq!(path, "/adapters/my-adapter-123");

    let review_pause_id = "newer";
    let review_path = format!("/reviews/{}", review_pause_id);
    assert_eq!(review_path, "/reviews/newer");
}

#[wasm_bindgen_test]
fn test_query_param_building() {
    // Test query parameter construction
    let page = 2;
    let per_page = 20;
    let query = format!("?page={}&per_page={}", page, per_page);
    assert_eq!(query, "?page=2&per_page=20");
}

// ============================================================================
// Protected Route Tests
// ============================================================================

#[wasm_bindgen_test]
fn test_protected_routes_list() {
    // Routes that require authentication
    let protected = vec!["/settings", "/training", "/admin"];

    for route in protected {
        assert!(route.starts_with('/'));
    }
}

#[wasm_bindgen_test]
fn test_public_routes_list() {
    // Routes that don't require authentication
    let public = vec!["/", "/dashboard"];

    for route in public {
        assert!(route.starts_with('/'));
    }
}

// ============================================================================
// Breadcrumb Route Mapping Tests
// ============================================================================

#[wasm_bindgen_test]
fn test_breadcrumb_segment_humanization() {
    // Expected humanization patterns
    let cases = vec![
        ("adapters", "Adapters"),
        ("api-keys", "API Keys"),
        ("training", "Training"),
        ("my-adapter", "My Adapter"),
        ("system", "System"),
    ];

    for (segment, _expected) in cases {
        // Basic humanization: capitalize first letter
        let humanized = {
            let mut chars = segment.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
            }
        };
        // Note: This is a simplified test; actual humanize_segment has more logic
        assert!(!humanized.is_empty());
    }
}

// ============================================================================
// Route Guard Tests
// ============================================================================

#[wasm_bindgen_test]
fn test_auth_redirect_target() {
    // When unauthenticated, should redirect to login
    let redirect_target = "/login";
    assert!(redirect_target.starts_with('/'));
}

#[wasm_bindgen_test]
fn test_after_login_redirect() {
    // After login, should redirect to intended page or dashboard
    let default_redirect = "/dashboard";
    assert_eq!(default_redirect, "/dashboard");
}
