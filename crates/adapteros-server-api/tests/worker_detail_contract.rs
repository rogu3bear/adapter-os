use adapteros_api_types::API_SCHEMA_VERSION;
use adapteros_server_api::handlers::worker_detail::{
    WorkerDetailResponse, WorkerResourceUsage, WorkerTask, WorkerType,
};
use serde_json::Value;

#[test]
fn worker_detail_response_uses_last_seen_at_contract_field() {
    let payload = WorkerDetailResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        id: "wrk_test".to_string(),
        tenant_id: "tenant-1".to_string(),
        node_id: "node-1".to_string(),
        plan_id: "plan-1".to_string(),
        worker_type: WorkerType::Inference,
        status: "serving".to_string(),
        pid: Some(1234),
        uds_path: "/tmp/aos.sock".to_string(),
        resource_usage: WorkerResourceUsage {
            cpu_usage_percent: 1.25,
            memory_usage_mb: 64.0,
            memory_limit_mb: None,
            thread_count: 4,
            requests_processed: 12,
            errors_count: 0,
            avg_latency_ms: 2.5,
            timestamp: 1_700_000_000,
        },
        active_tasks: vec![WorkerTask {
            task_id: "task-1".to_string(),
            task_type: "inference".to_string(),
            status: "running".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            progress_percent: Some(50.0),
        }],
        adapters_loaded: vec!["adapter-a".to_string()],
        uptime_seconds: 30,
        memory_headroom_pct: Some(22.0),
        k_current: Some(8),
        started_at: "2026-01-01T00:00:00Z".to_string(),
        last_seen_at: Some("2026-01-01T00:00:30Z".to_string()),
        coreml_failure_stage: None,
        coreml_failure_reason: None,
    };

    let json = serde_json::to_value(payload).expect("worker detail response should serialize");
    assert_eq!(
        json.get("last_seen_at").and_then(Value::as_str),
        Some("2026-01-01T00:00:30Z")
    );
    assert!(
        json.get("last_heartbeat_at").is_none(),
        "legacy field must not be serialized"
    );
}

#[test]
fn worker_detail_openapi_schema_uses_last_seen_at_only() {
    let spec = include_str!("../../../docs/api/openapi.json");
    let value: Value = serde_json::from_str(spec).expect("openapi JSON should parse");
    let properties = value
        .pointer("/components/schemas/WorkerDetailResponse/properties")
        .and_then(Value::as_object)
        .expect("WorkerDetailResponse schema properties should exist");

    assert!(
        properties.contains_key("last_seen_at"),
        "last_seen_at must be documented"
    );
    assert!(
        !properties.contains_key("last_heartbeat_at"),
        "last_heartbeat_at must not be documented"
    );
}
