//! Contract test: worker/control-plane HTTP path strings must exist in OpenAPI.
//!
//! Goal: catch drift where worker code keeps calling a path that the control plane
//! no longer serves (or vice-versa).

use serde_json::Value;

fn openapi_paths_object() -> serde_json::Map<String, Value> {
    let spec = include_str!("../../../docs/api/openapi.json");
    let v: Value = serde_json::from_str(spec).expect("openapi.json must be valid JSON");
    let paths = v
        .get("paths")
        .and_then(|p| p.as_object())
        .expect("openapi.json must contain top-level 'paths' object");
    paths.clone()
}

#[test]
fn worker_control_plane_paths_and_methods_are_documented_in_openapi() {
    let paths = openapi_paths_object();

    // Paths referenced by crates/adapteros-lora-worker during registration/health reporting.
    let required = [
        ("/v1/workers/register", "post"),
        ("/v1/workers/status", "post"),
        ("/v1/workers/fatal", "post"),
        ("/v1/tenants/{tenant_id}/manifests/{manifest_hash}", "get"),
    ];

    for (path, method) in required {
        let path_item = paths.get(path).unwrap_or_else(|| {
            panic!("OpenAPI spec missing required worker/control-plane path: {path}")
        });
        assert!(
            path_item.get(method).is_some(),
            "OpenAPI spec missing required worker/control-plane operation: {} {}",
            method.to_ascii_uppercase(),
            path
        );
    }
}
