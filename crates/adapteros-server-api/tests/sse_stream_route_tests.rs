//! Route registration guards for determinism-relevant SSE surfaces.

#[test]
fn protected_stream_routes_include_alerts_anomalies_and_dashboard() {
    let routes_source = include_str!("../src/routes/mod.rs");
    for route in [
        "/v1/stream/alerts",
        "/v1/stream/anomalies",
        "/v1/stream/dashboard",
    ] {
        assert!(
            routes_source.contains(route),
            "expected protected route registration for {route}"
        );
    }
}

#[test]
fn stream_handlers_expose_expected_determinism_guard_paths() {
    let handlers_source = include_str!("../src/handlers/streams/mod.rs");
    for needle in [
        "pub async fn alerts_stream(",
        "pub async fn anomalies_stream(",
        "pub async fn dashboard_stream(",
        "determinism_guard_stream_status",
        "\"determinism_guard\"",
        "decode_tenant_scoped_replay_events_for_tenant(",
    ] {
        assert!(
            handlers_source.contains(needle),
            "expected streams handler source to contain {needle}"
        );
    }
}
