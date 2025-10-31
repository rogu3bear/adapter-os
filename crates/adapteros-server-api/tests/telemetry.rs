use std::sync::Arc;

use adapteros_server_api::handlers::telemetry::{
    get_metrics_series, get_metrics_snapshot, query_logs,
};
use adapteros_server_api::state::AppState;
use adapteros_server_api::types::{MetricsSeriesResponse, MetricsSnapshotResponse};
use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder, UnifiedTelemetryEvent};
use axum::body::{to_bytes, Body};
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use http::{Request, StatusCode};
use tower::ServiceExt;

mod common;

fn telemetry_router(state: AppState) -> Router {
    Router::new()
        .route("/api/metrics/snapshot", get(get_metrics_snapshot))
        .route("/api/metrics/series", get(get_metrics_series))
        .route("/api/logs/query", get(query_logs))
        .with_state(state)
}

fn seed_metrics(state: &AppState) {
    let collector = Arc::clone(&state.metrics_collector);
    collector.update_queue_depth("request", "default", 7.0);
    collector.update_queue_depth("kernel", "default", 3.0);
    collector.update_adapter_queue_depth("default", "default", 2.0);
    collector.update_active_sessions(2.0);
    collector.update_tokens_per_second("default", 42.0);
    collector.record_tokens_generated("default", "default", 128);
    collector.update_memory_usage("worker", "default", 32.0 * 1024.0 * 1024.0);
    collector.record_policy_violation("egress", "attempt");
    collector.record_adapter_activation("default", "default");
    collector.record_adapter_eviction("default", "default", "memory");
}

#[tokio::test]
async fn snapshot_endpoint_returns_live_metrics() -> anyhow::Result<()> {
    let state = common::setup_state(None).await?;
    seed_metrics(&state);
    state.metrics_registry.record_snapshot().await?;

    let router = telemetry_router(state.clone());

    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/metrics/snapshot")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    let snapshot: MetricsSnapshotResponse =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;

    assert!(snapshot.throughput.tokens_per_second > 0.0);
    assert!(snapshot.system.active_sessions >= 2.0);
    assert!(
        snapshot.policy.violations_total >= 1,
        "Expected policy violation count to be populated"
    );
    Ok(())
}

#[tokio::test]
async fn series_endpoint_filters_by_name_and_window() -> anyhow::Result<()> {
    let state = common::setup_state(None).await?;
    seed_metrics(&state);

    // Record two snapshots to ensure multiple points exist
    state.metrics_registry.record_snapshot().await?;
    // Slight delay to ensure timestamp progression
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    state
        .metrics_collector
        .update_tokens_per_second("default", 84.0);
    state.metrics_registry.record_snapshot().await?;

    let router = telemetry_router(state.clone());

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/metrics/series?series_name=tokens_per_second")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    let series: Vec<MetricsSeriesResponse> =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].series_name, "tokens_per_second");
    assert!(
        series[0].points.len() >= 2,
        "expected at least two datapoints after seeding snapshots"
    );

    let latest_value = series[0]
        .points
        .last()
        .expect("at least one datapoint should exist")
        .value;
    assert!(
        (latest_value - 84.0).abs() < f64::EPSILON,
        "expected latest datapoint to match most recent tokens_per_second value"
    );

    let timestamps: Vec<u64> = series[0].points.iter().map(|pt| pt.timestamp_ms).collect();
    let min_ts = *timestamps.first().unwrap();
    let max_ts = *timestamps.last().unwrap();

    // Query again with a window that excludes the first point
    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/metrics/series?series_name=tokens_per_second&start_ms={}",
                    max_ts - 1
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let windowed: Vec<MetricsSeriesResponse> =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
    assert_eq!(windowed.len(), 1);
    assert!(
        windowed[0]
            .points
            .iter()
            .all(|pt| pt.timestamp_ms >= max_ts - 1),
        "window filtering should drop older points"
    );

    // Invalid window returns 400
    let bad_response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/metrics/series?start_ms={}&end_ms={}",
                    max_ts, min_ts
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(bad_response.status(), StatusCode::BAD_REQUEST);

    Ok(())
}

#[tokio::test]
async fn logs_query_returns_filtered_events() -> anyhow::Result<()> {
    let state = common::setup_state(None).await?;

    let base_event = TelemetryEventBuilder::new(
        EventType::SystemStart,
        LogLevel::Info,
        "Control plane initialized".to_string(),
    )
    .component("control-plane".to_string())
    .tenant_id("tenant-a".to_string())
    .build();

    let warning_event = TelemetryEventBuilder::new(
        EventType::SystemWarning,
        LogLevel::Warn,
        "High memory pressure".to_string(),
    )
    .component("scheduler".to_string())
    .tenant_id("tenant-b".to_string())
    .trace_id("trace-123".to_string())
    .build();

    state.telemetry_buffer.push(base_event.clone());
    state.telemetry_buffer.push(warning_event.clone());

    let router = telemetry_router(state.clone());

    let all_response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/logs/query?limit=10")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(all_response.status(), StatusCode::OK);
    let all_logs: Vec<UnifiedTelemetryEvent> =
        serde_json::from_slice(&to_bytes(all_response.into_body(), usize::MAX).await?)?;
    assert_eq!(all_logs.len(), 2);

    let warn_response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/logs/query?level=warn")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(warn_response.status(), StatusCode::OK);
    let warn_logs: Vec<UnifiedTelemetryEvent> =
        serde_json::from_slice(&to_bytes(warn_response.into_body(), usize::MAX).await?)?;
    assert_eq!(warn_logs.len(), 1);
    assert_eq!(warn_logs[0].component.as_deref(), Some("scheduler"));
    assert_eq!(warn_logs[0].trace_id.as_deref(), Some("trace-123"));

    let tenant_filter_response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/logs/query?tenant_id=tenant-a&component=control-plane")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(tenant_filter_response.status(), StatusCode::OK);
    let tenant_logs: Vec<UnifiedTelemetryEvent> =
        serde_json::from_slice(&to_bytes(tenant_filter_response.into_body(), usize::MAX).await?)?;
    assert_eq!(tenant_logs.len(), 1);
    assert_eq!(tenant_logs[0].tenant_id.as_deref(), Some("tenant-a"));

    let bad_level_response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/logs/query?level=verbose")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(bad_level_response.status(), StatusCode::BAD_REQUEST);

    Ok(())
}
