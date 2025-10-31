#[path = "common/mod.rs"]
mod common;

use std::path::PathBuf;

use adapteros_cli::commands::infer;
use common::{CapturedRequest, StubHttpResponse, StubUdsServer};

fn req_json(req: &CapturedRequest) -> serde_json::Value {
    serde_json::from_slice(&req.body).expect("capture valid json")
}

#[tokio::test]
async fn run_infer_preloads_and_swaps_adapter() {
    let server = StubUdsServer::start(vec![
        StubHttpResponse::ok_json(serde_json::json!({"status": "preloaded"})),
        StubHttpResponse::ok_json(serde_json::json!({"status": "swapped"})),
        StubHttpResponse::ok_json(serde_json::json!({
            "text": "hello world",
            "trace": {
                "evidence": [
                    {
                        "doc_id": "doc",
                        "rev": "abc123",
                        "span_hash": "span",
                        "score": 0.9
                    }
                ]
            }
        })),
    ])
    .await
    .expect("start server");

    let socket = PathBuf::from(server.socket_path());

    infer::run(
        Some("adapter-123".to_string()),
        "Prompt text".to_string(),
        Some(64),
        true,
        socket,
        200,
        true,
        true,
    )
    .await
    .expect("infer run succeeds");

    let requests = server.captured_requests().await;
    assert_eq!(requests.len(), 3);

    let preload = req_json(&requests[0]);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/adapter");
    assert_eq!(preload["type"], "preload");
    assert_eq!(preload["adapter_id"], "adapter-123");

    let swap = req_json(&requests[1]);
    assert_eq!(swap["type"], "swap");
    assert_eq!(swap["add_ids"], serde_json::json!(["adapter-123"]));

    let inference = req_json(&requests[2]);
    assert_eq!(requests[2].path, "/inference");
    assert_eq!(inference["prompt"], "Prompt text");
    assert_eq!(inference["max_tokens"], 64);
    assert_eq!(inference["require_evidence"], true);
}

#[tokio::test]
async fn run_infer_without_adapter_skips_preload() {
    let server = StubUdsServer::start(vec![StubHttpResponse::ok_json(
        serde_json::json!({"text": "hello"}),
    )])
    .await
    .expect("start server");

    infer::run(
        None,
        "Prompt".to_string(),
        None,
        false,
        server.socket_path().to_path_buf(),
        200,
        false,
        false,
    )
    .await
    .expect("infer run succeeds");

    let requests = server.captured_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/inference");

    let inference = req_json(&requests[0]);
    assert_eq!(inference["max_tokens"], 128); // default fallback
}
