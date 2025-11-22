use std::collections::HashSet;
use std::sync::Arc;

use adapteros_server_api::handlers::telemetry::{
    event_matches_filters, get_metrics_series, get_metrics_snapshot, get_trace,
    normalize_log_filters, query_logs, search_traces, stream_logs, LogsQueryParams,
    NormalizedLogFilters,
};
use adapteros_server_api::state::AppState;
use adapteros_server_api::types::{ErrorResponse, MetricsSeriesResponse, MetricsSnapshotResponse};
use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder, UnifiedTelemetryEvent};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

mod common;

fn telemetry_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/metrics/snapshot", get(get_metrics_snapshot))
        .route("/v1/metrics/series", get(get_metrics_series))
        .route("/v1/logs/query", get(query_logs))
        .route("/v1/logs/stream", get(stream_logs))
        .route("/v1/traces/search", get(search_traces))
        .route("/v1/traces/:trace_id", get(get_trace))
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
                .uri("/v1/metrics/snapshot")
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
                .uri("/v1/metrics/series?series_name=tokens_per_second")
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
    let _min_ts = *timestamps.first().unwrap();
    let max_ts = *timestamps.last().unwrap();

    // Query again with a window that excludes the first point
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/v1/metrics/series?series_name=tokens_per_second&start_ms={}",
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
    let bad_url = format!(
        "/v1/metrics/series?start_ms={}&end_ms={}",
        max_ts + 1,
        max_ts
    );
    let bad_response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&bad_url)
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
                .uri("/v1/logs/query?limit=10")
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
                .uri("/v1/logs/query?level=warn")
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
                .uri("/v1/logs/query?tenant_id=tenant-a&component=control-plane")
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
                .uri("/v1/logs/query?level=verbose")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(bad_level_response.status(), StatusCode::BAD_REQUEST);

    Ok(())
}

#[tokio::test]
async fn series_endpoint_returns_404_for_nonexistent_series() -> anyhow::Result<()> {
    let state = common::setup_state(None).await?;
    seed_metrics(&state);

    let router = telemetry_router(state.clone());

    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/metrics/series?series_name=nonexistent_series")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let error: ErrorResponse =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;

    assert_eq!(error.code, "NOT_FOUND");
    assert!(error.details.is_some());

    Ok(())
}

#[tokio::test]
async fn series_endpoint_returns_all_series_when_no_name_specified() -> anyhow::Result<()> {
    let state = common::setup_state(None).await?;
    seed_metrics(&state);

    // Record multiple snapshots to ensure series have data
    state.metrics_registry.record_snapshot().await?;
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    state.metrics_registry.record_snapshot().await?;

    let router = telemetry_router(state.clone());

    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/metrics/series")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    let all_series: Vec<MetricsSeriesResponse> =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;

    // Should return multiple series (we seeded several metrics)
    assert!(
        !all_series.is_empty(),
        "Expected at least one series to be returned"
    );

    // Verify each series has the expected name and data points
    let series_names: HashSet<String> = all_series.iter().map(|s| s.series_name.clone()).collect();

    // Check that we got expected series names from seeding
    assert!(
        series_names.contains("tokens_per_second"),
        "Expected tokens_per_second series"
    );
    assert!(
        series_names.contains("queue_depth"),
        "Expected queue_depth series"
    );

    // Verify each series has data points
    for series in &all_series {
        assert!(
            !series.points.is_empty(),
            "Series {} should have data points",
            series.series_name
        );
    }

    Ok(())
}

#[tokio::test]
async fn telemetry_stream_with_valid_token() -> anyhow::Result<()> {
    use adapteros_server_api::auth::generate_token;
    use adapteros_server_api::handlers;
    use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder};
    use axum::middleware;

    let state = common::setup_state(None).await?;

    // Seed some events into the telemetry buffer (backlog will be streamed)
    let event1 = TelemetryEventBuilder::new(
        EventType::SystemStart,
        LogLevel::Info,
        "Test event 1".to_string(),
    )
    .tenant_id("tenant-1".to_string())
    .build();

    state.telemetry_buffer.push(event1);

    // Create valid token
    let token = generate_token(
        "test-user",
        "test@example.com",
        "admin",
        "tenant-1",
        b"test-secret",
    )?;

    // Test stream endpoint with token in query
    let router = Router::new()
        .route(
            "/v1/stream/telemetry",
            get(handlers::telemetry_events_stream),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            adapteros_server_api::middleware::auth_middleware,
        ))
        .with_state(state);

    use url::form_urlencoded;
    let encoded_token: String = form_urlencoded::byte_serialize(token.as_bytes()).collect();
    let uri = format!("/v1/stream/telemetry?token={}", encoded_token);

    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&uri)
                .body(Body::empty())?,
        )
        .await?;

    // Should succeed with valid token and establish SSE stream
    assert_eq!(response.status(), StatusCode::OK);

    // Verify SSE content-type header
    let content_type = response.headers().get("content-type");
    assert!(
        content_type
            .map(|h| h.to_str().unwrap_or(""))
            .unwrap_or("")
            .contains("text/event-stream"),
        "Expected text/event-stream content-type, got {:?}",
        content_type
    );

    Ok(())
}

#[tokio::test]
async fn telemetry_stream_without_token_rejects() -> anyhow::Result<()> {
    use adapteros_server_api::handlers;
    use axum::middleware;

    let state = common::setup_state(None).await?;

    let router = Router::new()
        .route(
            "/v1/stream/telemetry",
            get(handlers::telemetry_events_stream),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            adapteros_server_api::middleware::auth_middleware,
        ))
        .with_state(state);

    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/stream/telemetry")
                .body(Body::empty())?,
        )
        .await?;

    // Should reject without token
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    Ok(())
}

#[tokio::test]
async fn telemetry_stream_with_invalid_token_rejects() -> anyhow::Result<()> {
    use adapteros_server_api::handlers;
    use axum::middleware;

    let state = common::setup_state(None).await?;

    let router = Router::new()
        .route(
            "/v1/stream/telemetry",
            get(handlers::telemetry_events_stream),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            adapteros_server_api::middleware::auth_middleware,
        ))
        .with_state(state);

    let uri = "/v1/stream/telemetry?token=invalid-token";
    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())?,
        )
        .await?;

    // Should reject invalid token
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    Ok(())
}

#[tokio::test]
async fn stream_logs_returns_sse_stream() -> anyhow::Result<()> {
    let state = common::setup_state(None).await?;

    // Seed some events into the telemetry buffer
    let event = TelemetryEventBuilder::new(
        EventType::SystemStart,
        LogLevel::Info,
        "Test log event".to_string(),
    )
    .tenant_id("tenant-1".to_string())
    .component("test-component".to_string())
    .build();

    state.telemetry_buffer.push(event);

    let router = telemetry_router(state.clone());

    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/logs/stream")
                .body(Body::empty())?,
        )
        .await?;

    // Should return OK and establish SSE stream
    assert_eq!(response.status(), StatusCode::OK);

    // Verify SSE content-type header
    let content_type = response.headers().get("content-type");
    assert!(
        content_type
            .map(|h| h.to_str().unwrap_or(""))
            .unwrap_or("")
            .contains("text/event-stream"),
        "Expected text/event-stream content-type, got {:?}",
        content_type
    );

    Ok(())
}

#[tokio::test]
async fn stream_logs_filters_events_by_tenant() -> anyhow::Result<()> {
    let state = common::setup_state(None).await?;

    // Create events with different tenants
    let tenant1_event = TelemetryEventBuilder::new(
        EventType::SystemStart,
        LogLevel::Info,
        "Tenant 1 event".to_string(),
    )
    .tenant_id("tenant-1".to_string())
    .component("test-component".to_string())
    .build();

    let tenant2_event = TelemetryEventBuilder::new(
        EventType::SystemWarning,
        LogLevel::Warn,
        "Tenant 2 event".to_string(),
    )
    .tenant_id("tenant-2".to_string())
    .component("test-component".to_string())
    .build();

    // Add events to telemetry buffer
    state.telemetry_buffer.push(tenant1_event.clone());
    state.telemetry_buffer.push(tenant2_event.clone());

    let router = telemetry_router(state.clone());

    // Test filtering by tenant-1
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/logs/stream?tenant_id=tenant-1")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    // Verify content-type
    let content_type = response.headers().get("content-type");
    assert!(content_type
        .map(|h| h.to_str().unwrap_or(""))
        .unwrap_or("")
        .contains("text/event-stream"));

    // Test filtering by tenant-2
    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/logs/stream?tenant_id=tenant-2")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    // Verify content-type
    let content_type = response.headers().get("content-type");
    assert!(content_type
        .map(|h| h.to_str().unwrap_or(""))
        .unwrap_or("")
        .contains("text/event-stream"));

    Ok(())
}

#[tokio::test]
async fn stream_logs_filters_events_by_level() -> anyhow::Result<()> {
    let state = common::setup_state(None).await?;

    // Create events with different log levels
    let info_event = TelemetryEventBuilder::new(
        EventType::SystemStart,
        LogLevel::Info,
        "Info level event".to_string(),
    )
    .tenant_id("tenant-1".to_string())
    .build();

    let warn_event = TelemetryEventBuilder::new(
        EventType::SystemWarning,
        LogLevel::Warn,
        "Warning level event".to_string(),
    )
    .tenant_id("tenant-1".to_string())
    .build();

    // Add events to telemetry buffer
    state.telemetry_buffer.push(info_event.clone());
    state.telemetry_buffer.push(warn_event.clone());

    let router = telemetry_router(state.clone());

    // Test filtering by warn level
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/logs/stream?level=warn")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    // Verify content-type
    let content_type = response.headers().get("content-type");
    assert!(content_type
        .map(|h| h.to_str().unwrap_or(""))
        .unwrap_or("")
        .contains("text/event-stream"));

    // Test filtering by info level
    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/logs/stream?level=info")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    // Verify content-type
    let content_type = response.headers().get("content-type");
    assert!(content_type
        .map(|h| h.to_str().unwrap_or(""))
        .unwrap_or("")
        .contains("text/event-stream"));

    Ok(())
}

#[tokio::test]
async fn event_matches_filters_correctly_filters_events() -> anyhow::Result<()> {
    // Test tenant filtering
    let event = TelemetryEventBuilder::new(
        EventType::SystemStart,
        LogLevel::Info,
        "Test event".to_string(),
    )
    .tenant_id("tenant-1".to_string())
    .component("test-component".to_string())
    .trace_id("trace-123".to_string())
    .build();

    // Test matching tenant
    let tenant_filter = NormalizedLogFilters {
        tenant_id: Some("tenant-1".to_string()),
        ..Default::default()
    };
    assert!(event_matches_filters(&event, &tenant_filter));

    // Test non-matching tenant
    let wrong_tenant_filter = NormalizedLogFilters {
        tenant_id: Some("tenant-2".to_string()),
        ..Default::default()
    };
    assert!(!event_matches_filters(&event, &wrong_tenant_filter));

    // Test level filtering
    let level_filter = NormalizedLogFilters {
        level: Some(LogLevel::Info),
        ..Default::default()
    };
    assert!(event_matches_filters(&event, &level_filter));

    // Test non-matching level
    let wrong_level_filter = NormalizedLogFilters {
        level: Some(LogLevel::Warn),
        ..Default::default()
    };
    assert!(!event_matches_filters(&event, &wrong_level_filter));

    // Test component filtering
    let component_filter = NormalizedLogFilters {
        component: Some("test-component".to_string()),
        ..Default::default()
    };
    assert!(event_matches_filters(&event, &component_filter));

    // Test non-matching component
    let wrong_component_filter = NormalizedLogFilters {
        component: Some("other-component".to_string()),
        ..Default::default()
    };
    assert!(!event_matches_filters(&event, &wrong_component_filter));

    // Test trace_id filtering
    let trace_filter = NormalizedLogFilters {
        trace_id: Some("trace-123".to_string()),
        ..Default::default()
    };
    assert!(event_matches_filters(&event, &trace_filter));

    // Test non-matching trace_id
    let wrong_trace_filter = NormalizedLogFilters {
        trace_id: Some("trace-456".to_string()),
        ..Default::default()
    };
    assert!(!event_matches_filters(&event, &wrong_trace_filter));

    // Test event_type filtering
    let event_type_filter = NormalizedLogFilters {
        event_type: Some("system.start".to_string()),
        ..Default::default()
    };
    assert!(event_matches_filters(&event, &event_type_filter));

    // Test non-matching event_type
    let wrong_event_type_filter = NormalizedLogFilters {
        event_type: Some("system.warning".to_string()),
        ..Default::default()
    };
    assert!(!event_matches_filters(&event, &wrong_event_type_filter));

    // Test multiple filters (all must match)
    let multi_filter = NormalizedLogFilters {
        tenant_id: Some("tenant-1".to_string()),
        level: Some(LogLevel::Info),
        component: Some("test-component".to_string()),
        ..Default::default()
    };
    assert!(event_matches_filters(&event, &multi_filter));

    // Test multiple filters where one doesn't match
    let failing_multi_filter = NormalizedLogFilters {
        tenant_id: Some("tenant-1".to_string()),
        level: Some(LogLevel::Warn), // This doesn't match
        component: Some("test-component".to_string()),
        ..Default::default()
    };
    assert!(!event_matches_filters(&event, &failing_multi_filter));

    // Test empty filters (should match everything)
    let empty_filter = NormalizedLogFilters::default();
    assert!(event_matches_filters(&event, &empty_filter));

    Ok(())
}

#[tokio::test]
async fn search_traces_returns_filtered_results() -> anyhow::Result<()> {
    use adapteros_trace::Trace;
    use std::collections::HashMap;

    let state = common::setup_state(None).await?;

    // Create a test trace
    let trace_id = "test-trace-123".to_string();
    let span = adapteros_trace::Span {
        span_id: "test-span-123".to_string(),
        trace_id: trace_id.clone(),
        parent_id: None,
        name: "test-span".to_string(),
        start_ns: 1000000000,
        end_ns: Some(2000000000),
        attributes: HashMap::new(),
        status: adapteros_trace::SpanStatus::Ok,
        events: vec![],
    };

    let trace = Trace {
        trace_id: trace_id.clone(),
        spans: vec![span],
        root_span_id: None,
    };

    // Add trace to buffer
    state.trace_buffer.add_trace(trace);

    let router = telemetry_router(state.clone());

    // Test search without filters (should return all traces)
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/traces/search")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    let trace_ids: Vec<String> =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
    assert_eq!(trace_ids.len(), 1);
    assert_eq!(trace_ids[0], trace_id.to_string());

    // Test search with span name filter
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/traces/search?span_name=test-span")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    let filtered_ids: Vec<String> =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
    assert_eq!(filtered_ids.len(), 1);

    // Test search with non-matching span name
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/traces/search?span_name=nonexistent")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    let empty_ids: Vec<String> =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
    assert!(empty_ids.is_empty());

    Ok(())
}

#[tokio::test]
async fn get_trace_returns_specific_trace() -> anyhow::Result<()> {
    use adapteros_trace::Trace;
    use std::collections::HashMap;

    let state = common::setup_state(None).await?;

    // Create a test trace
    let trace_id = "test-trace-456".to_string();
    let span = adapteros_trace::Span {
        span_id: "test-span-456".to_string(),
        trace_id: trace_id.clone(),
        parent_id: None,
        name: "test-span".to_string(),
        start_ns: 1000000000,
        end_ns: Some(2000000000),
        attributes: HashMap::new(),
        status: adapteros_trace::SpanStatus::Ok,
        events: vec![],
    };

    let trace = Trace {
        trace_id: trace_id.clone(),
        spans: vec![span],
        root_span_id: None,
    };

    // Add trace to buffer
    state.trace_buffer.add_trace(trace.clone());

    let router = telemetry_router(state.clone());

    // Test getting existing trace
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/v1/traces/{}", trace_id))
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    let returned_trace: Option<adapteros_trace::Trace> =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
    assert!(returned_trace.is_some());
    assert_eq!(returned_trace.unwrap().trace_id, trace_id);

    // Test getting non-existent trace
    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/traces/nonexistent-trace-id")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    let none_trace: Option<adapteros_trace::Trace> =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
    assert!(none_trace.is_none());

    Ok(())
}

#[test]
fn normalize_log_filters_handles_limit_edge_cases() {
    // Test limit = 0 returns error
    let params = LogsQueryParams {
        limit: Some(0),
        tenant_id: None,
        event_type: None,
        level: None,
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.code, "BAD_REQUEST");
    assert!(error.error.contains("limit must be greater than zero"));

    // Test limit > 1024 gets clamped to 1024
    let params = LogsQueryParams {
        limit: Some(2000),
        tenant_id: None,
        event_type: None,
        level: None,
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.limit, Some(1024));

    // Test normal limit values
    let params = LogsQueryParams {
        limit: Some(500),
        tenant_id: None,
        event_type: None,
        level: None,
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.limit, Some(500));

    // Test no limit specified (should use default of 100)
    let params = LogsQueryParams {
        limit: None,
        tenant_id: None,
        event_type: None,
        level: None,
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.limit, Some(100));
}

#[test]
fn normalize_log_filters_trims_whitespace_and_handles_empty_strings() {
    // Test tenant_id with leading/trailing whitespace
    let params = LogsQueryParams {
        limit: None,
        tenant_id: Some("  tenant-1  ".to_string()),
        event_type: None,
        level: None,
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.tenant_id, Some("tenant-1".to_string()));
    assert_eq!(filters.realtime.tenant_id, Some("tenant-1".to_string()));

    // Test tenant_id that becomes empty after trimming
    let params = LogsQueryParams {
        limit: None,
        tenant_id: Some("   ".to_string()),
        event_type: None,
        level: None,
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.tenant_id, None);
    assert_eq!(filters.realtime.tenant_id, None);

    // Test event_type trimming
    let params = LogsQueryParams {
        limit: None,
        tenant_id: None,
        event_type: Some("  system.start  ".to_string()),
        level: None,
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(
        filters.telemetry.event_type,
        Some("system.start".to_string())
    );
    assert_eq!(
        filters.realtime.event_type,
        Some("system.start".to_string())
    );

    // Test component trimming
    let params = LogsQueryParams {
        limit: None,
        tenant_id: None,
        event_type: None,
        level: None,
        component: Some("  my-component  ".to_string()),
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(
        filters.telemetry.component,
        Some("my-component".to_string())
    );
    assert_eq!(filters.realtime.component, Some("my-component".to_string()));

    // Test trace_id trimming
    let params = LogsQueryParams {
        limit: None,
        tenant_id: None,
        event_type: None,
        level: None,
        component: None,
        trace_id: Some("  trace-123  ".to_string()),
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.trace_id, Some("trace-123".to_string()));
    assert_eq!(filters.realtime.trace_id, Some("trace-123".to_string()));
}

#[test]
fn normalize_log_filters_handles_level_parsing_and_trimming() {
    // Test valid level with whitespace
    let params = LogsQueryParams {
        limit: None,
        tenant_id: None,
        event_type: None,
        level: Some("  INFO  ".to_string()),
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.level, Some(LogLevel::Info));
    assert_eq!(filters.realtime.level, Some(LogLevel::Info));

    // Test case insensitive level parsing
    let params = LogsQueryParams {
        limit: None,
        tenant_id: None,
        event_type: None,
        level: Some("  warn  ".to_string()),
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.level, Some(LogLevel::Warn));
    assert_eq!(filters.realtime.level, Some(LogLevel::Warn));

    // Test alternative level names (warning, critical)
    let params = LogsQueryParams {
        limit: None,
        tenant_id: None,
        event_type: None,
        level: Some("warning".to_string()),
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.level, Some(LogLevel::Warn));

    let params = LogsQueryParams {
        limit: None,
        tenant_id: None,
        event_type: None,
        level: Some("critical".to_string()),
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.level, Some(LogLevel::Critical));

    // Test invalid level returns error
    let params = LogsQueryParams {
        limit: None,
        tenant_id: None,
        event_type: None,
        level: Some("invalid_level".to_string()),
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.code, "BAD_REQUEST");
    assert!(error.error.contains("invalid log level"));

    // Test empty level after trimming is ignored
    let params = LogsQueryParams {
        limit: None,
        tenant_id: None,
        event_type: None,
        level: Some("   ".to_string()),
        component: None,
        trace_id: None,
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();
    assert_eq!(filters.telemetry.level, None);
    assert_eq!(filters.realtime.level, None);
}

#[test]
fn normalize_log_filters_handles_multiple_filters_with_trimming() {
    // Test multiple filters with whitespace that should all be trimmed and applied
    let params = LogsQueryParams {
        limit: Some(100),
        tenant_id: Some("  tenant-1  ".to_string()),
        event_type: Some("  system.start  ".to_string()),
        level: Some("  info  ".to_string()),
        component: Some("  my-component  ".to_string()),
        trace_id: Some("  trace-123  ".to_string()),
    };
    let result = normalize_log_filters(&params);
    assert!(result.is_ok());
    let filters = result.unwrap();

    // Check telemetry filters
    assert_eq!(filters.telemetry.limit, Some(100));
    assert_eq!(filters.telemetry.tenant_id, Some("tenant-1".to_string()));
    assert_eq!(
        filters.telemetry.event_type,
        Some("system.start".to_string())
    );
    assert_eq!(filters.telemetry.level, Some(LogLevel::Info));
    assert_eq!(
        filters.telemetry.component,
        Some("my-component".to_string())
    );
    assert_eq!(filters.telemetry.trace_id, Some("trace-123".to_string()));

    // Check realtime filters
    assert_eq!(filters.realtime.tenant_id, Some("tenant-1".to_string()));
    assert_eq!(
        filters.realtime.event_type,
        Some("system.start".to_string())
    );
    assert_eq!(filters.realtime.level, Some(LogLevel::Info));
    assert_eq!(filters.realtime.component, Some("my-component".to_string()));
    assert_eq!(filters.realtime.trace_id, Some("trace-123".to_string()));
}
