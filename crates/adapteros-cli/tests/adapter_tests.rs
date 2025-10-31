#[path = "common/mod.rs"]
mod common;

use adapteros_cli::commands::adapter::{handle_adapter_command, AdapterCommand};
use adapteros_cli::output::{OutputMode, OutputWriter};
use adapteros_core::AosError;
use common::{CapturedRequest, StubHttpResponse, StubUdsServer};

struct EnvOverride {
    key: &'static str,
}

impl EnvOverride {
    fn set(key: &'static str, value: &str) -> Self {
        std::env::set_var(key, value);
        Self { key }
    }
}

impl Drop for EnvOverride {
    fn drop(&mut self) {
        std::env::remove_var(self.key);
    }
}

fn request_json(req: &CapturedRequest) -> serde_json::Value {
    serde_json::from_slice(&req.body).expect("request json")
}

fn json_output() -> OutputWriter {
    OutputWriter::new(OutputMode::Json, false)
}

#[tokio::test]
async fn list_adapters_fetches_worker_data() {
    let server = StubUdsServer::start(vec![StubHttpResponse::ok_json(serde_json::json!([
        {
            "id": "adapter-1",
            "adapter_id": "adapter-1",
            "name": "Adapter One",
            "hash_b3": "b3:aaaa",
            "rank": 1,
            "tier": 0,
            "languages": ["rust"],
            "framework": null,
            "created_at": "2025-01-01T00:00:00Z",
            "stats": null
        }
    ]))])
    .await
    .expect("start server");

    let override_guard = EnvOverride::set(
        "AOS_TEST_SOCKET_OVERRIDE",
        server.socket_path().to_str().expect("socket path utf-8"),
    );

    handle_adapter_command(
        AdapterCommand::List {
            json: true,
            tenant: None,
        },
        &json_output(),
    )
    .await
    .expect("list adapters succeeds");

    drop(override_guard);

    let requests = server.captured_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/adapters");
}

#[tokio::test]
async fn profile_adapter_fetches_worker_profile() {
    let server = StubUdsServer::start(vec![StubHttpResponse::ok_json(serde_json::json!({
        "state": "hot",
        "activation_pct": 45.2,
        "activations": 120,
        "total_tokens": 250,
        "avg_latency_us": 123.4,
        "memory_kb": 16384,
        "quality_delta": 0.67,
        "recent_activations": [
            {"start_token": 0, "end_token": 50, "count": 5}
        ],
        "performance_metrics": {
            "p50_latency_us": 111.0,
            "p95_latency_us": 150.0,
            "p99_latency_us": 190.0,
            "throughput_tokens_per_sec": 33.2,
            "error_rate": 0.01
        },
        "policy_compliance": {
            "determinism_score": 0.98,
            "evidence_coverage": 0.95,
            "refusal_rate": 0.02,
            "policy_violations": 0
        }
    }))])
    .await
    .expect("start server");

    let override_guard = EnvOverride::set(
        "AOS_TEST_SOCKET_OVERRIDE",
        server.socket_path().to_str().expect("socket path utf-8"),
    );

    handle_adapter_command(
        AdapterCommand::Profile {
            adapter_id: "adapter-1".to_string(),
            json: true,
            tenant: None,
        },
        &json_output(),
    )
    .await
    .expect("profile adapter succeeds");

    drop(override_guard);

    let requests = server.captured_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/adapter/adapter-1");
}

#[tokio::test]
async fn profile_adapter_rejects_invalid_id() {
    let err = handle_adapter_command(
        AdapterCommand::Profile {
            adapter_id: "".to_string(),
            json: true,
            tenant: None,
        },
        &json_output(),
    )
    .await
    .expect_err("profile should fail");

    match err {
        AosError::Parse(msg) => {
            assert!(msg.contains("Adapter ID cannot be empty"));
        }
        other => panic!("unexpected error: {:?}", other),
    }
}

#[tokio::test]
async fn list_adapters_falls_back_on_empty_worker_response() {
    let server = StubUdsServer::start(vec![StubHttpResponse::ok_json(serde_json::json!([]))])
        .await
        .expect("start server");

    let override_guard = EnvOverride::set(
        "AOS_TEST_SOCKET_OVERRIDE",
        server.socket_path().to_str().expect("socket path utf-8"),
    );

    // Should not error even when worker returns empty list
    handle_adapter_command(
        AdapterCommand::List {
            json: true,
            tenant: None,
        },
        &json_output(),
    )
    .await
    .expect("list adapters succeeds with empty worker response");

    drop(override_guard);

    let requests = server.captured_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/adapters");

    let body = request_json(&requests[0]);
    assert!(body.as_array().is_some());
}
