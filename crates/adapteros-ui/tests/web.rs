//! Browser-based integration tests for the Leptos UI
//!
//! Run with: wasm-pack test --headless --chrome

#![cfg(target_arch = "wasm32")]

use adapteros_ui::api::{api_base_url, ApiClient, ApiError};
use adapteros_ui::sse::parse_sse_event_with_info;
use leptos::mount::mount_to;
use leptos::prelude::IntoView;
use leptos::{leptos_dom::helpers::document, view};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================================
// API Base URL Tests
// ============================================================================

#[wasm_bindgen_test]
fn test_api_base_url_uses_origin() {
    // In browser context, should use window.location.origin
    let url = api_base_url();
    assert!(!url.is_empty());
    // Should start with http or https
    assert!(url.starts_with("http"));
}

// ============================================================================
// API Client Tests
// ============================================================================

#[wasm_bindgen_test]
fn test_api_client_creation() {
    let client = ApiClient::new();
    assert!(!client.is_authenticated());
}

#[wasm_bindgen_test]
fn test_api_client_not_authenticated_by_default() {
    let client = ApiClient::new();
    assert!(!client.is_authenticated());
}

// ============================================================================
// SSE Streaming Tests
// ============================================================================

#[wasm_bindgen_test]
fn test_sse_parse_token_event_adapteros_format() {
    let event = r#"data: {"event":"Token","text":"Hello"}"#;
    let parsed = parse_sse_event_with_info(event);
    assert_eq!(parsed.token, Some("Hello".to_string()));
    assert!(parsed.finish_reason.is_none());
}

#[wasm_bindgen_test]
fn test_sse_parse_token_event_openai_format() {
    let event = r#"data: {"choices": [{"delta": {"content": "Hello"}}]}"#;
    let parsed = parse_sse_event_with_info(event);
    assert_eq!(parsed.token, Some("Hello".to_string()));
    assert!(parsed.finish_reason.is_none());
}

#[wasm_bindgen_test]
fn test_sse_parse_done_event_adapteros_format() {
    let event =
        r#"data: {"event":"Done","total_tokens":42,"latency_ms":123,"trace_id":"trace-123"}"#;
    let parsed = parse_sse_event_with_info(event);
    assert!(parsed.token.is_none());
    assert_eq!(parsed.trace_id, Some("trace-123".to_string()));
    assert_eq!(parsed.latency_ms, Some(123));
    assert_eq!(parsed.token_count, Some(42));
}

#[wasm_bindgen_test]
fn test_sse_parse_done_event_openai_format_with_finish_reason() {
    let event = r#"data: {"choices": [{"delta": {}, "finish_reason": "stop"}]}"#;
    let parsed = parse_sse_event_with_info(event);
    assert!(parsed.token.is_none());
    assert_eq!(parsed.finish_reason, Some("stop".to_string()));
}

#[wasm_bindgen_test]
fn test_sse_parse_done_marker() {
    let event = "data: [DONE]";
    let parsed = parse_sse_event_with_info(event);
    assert!(parsed.token.is_none());
    assert!(parsed.finish_reason.is_none());
}

#[wasm_bindgen_test]
fn test_sse_accumulates_tokens() {
    // Simulate token accumulation
    let mut content = String::new();

    let events = vec![
        r#"data: {"choices": [{"delta": {"content": "Hello"}}]}"#,
        r#"data: {"choices": [{"delta": {"content": " "}}]}"#,
        r#"data: {"choices": [{"delta": {"content": "World"}}]}"#,
    ];

    for event in events {
        let parsed = parse_sse_event_with_info(event);
        if let Some(token) = parsed.token {
            content.push_str(&token);
        }
    }

    assert_eq!(content, "Hello World");
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[wasm_bindgen_test]
fn test_api_error_aborted_is_aborted() {
    let error = ApiError::Aborted;
    assert!(error.is_aborted());
}

#[wasm_bindgen_test]
fn test_api_error_network_is_not_aborted() {
    let error = ApiError::Network("connection failed".to_string());
    assert!(!error.is_aborted());
}

#[wasm_bindgen_test]
fn test_api_error_network_is_retryable() {
    let error = ApiError::Network("connection failed".to_string());
    assert!(error.is_retryable());
}

#[wasm_bindgen_test]
fn test_api_error_unauthorized_requires_auth() {
    let error = ApiError::Unauthorized;
    assert!(error.requires_auth());
}

#[wasm_bindgen_test]
fn test_api_error_http_not_retryable() {
    let error = ApiError::Http {
        status: 400,
        message: "Bad request".to_string(),
    };
    assert!(!error.is_retryable());
}

#[wasm_bindgen_test]
fn test_api_error_server_is_retryable() {
    let error = ApiError::Server("Internal error".to_string());
    assert!(error.is_retryable());
}

#[wasm_bindgen_test]
fn test_api_error_structured_has_code() {
    let error = ApiError::Structured {
        error: "Test error".to_string(),
        code: "TEST_ERROR".to_string(),
        failure_code: None,
        hint: None,
        details: Box::new(None),
        request_id: None,
        error_id: None,
        fingerprint: None,
        session_id: None,
        diag_trace_id: None,
        otel_trace_id: None,
    };
    assert_eq!(error.code(), Some("TEST_ERROR"));
}

#[wasm_bindgen_test]
fn test_api_error_display() {
    let error = ApiError::Network("timeout".to_string());
    let display = error.to_string();
    assert!(display.contains("timeout"));
}

// ============================================================================
// Route/page smoke mounts
// ============================================================================

fn mount_component<V: IntoView + 'static>(id: &str, view_fn: impl FnOnce() -> V + 'static) {
    let doc = document();
    let body = doc.body().expect("body");
    let container = doc.create_element("div").expect("create div");
    container.set_id(id);
    let container: web_sys::HtmlElement = container.unchecked_into();
    body.append_child(&container).expect("append");
    let _ = mount_to(container, move || view_fn());
}

#[wasm_bindgen_test]
fn mount_smoke_datasets_training_chat_diff_runs_repositories() {
    // Datasets list should mount without panic
    mount_component("datasets-smoke", || {
        view! { <adapteros_ui::pages::datasets::Datasets/> }
    });
    // Training page (list/detail shell) should mount without panic
    mount_component("training-smoke", || {
        view! { <adapteros_ui::pages::training::Training/> }
    });
    // Chat page should mount without panic (no network required)
    mount_component(
        "chat-smoke",
        || view! { <adapteros_ui::pages::chat::Chat/> },
    );
    // Diff page should mount without panic
    mount_component(
        "diff-smoke",
        || view! { <adapteros_ui::pages::diff::Diff/> },
    );
    // Runs list should mount without panic
    mount_component("runs-smoke", || {
        view! { <adapteros_ui::pages::flight_recorder::FlightRecorder/> }
    });
    // Repositories list should mount without panic
    mount_component("repositories-smoke", || {
        view! { <adapteros_ui::pages::repositories::Repositories/> }
    });
}

// ============================================================================
// Dataset wizard validation helpers
// ============================================================================

use adapteros_ui::pages::training::dataset_wizard::{
    parse_csv_rows, parse_jsonl_rows, parse_text_rows, CsvMapping, TextStrategy,
};

#[wasm_bindgen_test]
fn dataset_jsonl_requires_prompt_and_response() {
    let content = r#"
{"prompt":"Hi","response":"There","weight":1.2}
{"prompt":"Missing target"}
"#;
    let result = parse_jsonl_rows(content, "sample.jsonl");
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("target/response")));
}

#[wasm_bindgen_test]
fn dataset_csv_weight_must_be_positive() {
    let mapping = CsvMapping {
        input_col: "input".to_string(),
        target_col: "target".to_string(),
        weight_col: Some("weight".to_string()),
    };
    let csv = "input,target,weight\nhello,world,0\n";
    let result = parse_csv_rows(csv, &mapping, "rows.csv");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .iter()
        .any(|e| e.contains("weight must be > 0")));
}

#[wasm_bindgen_test]
fn dataset_text_pairing_produces_pairs() {
    let text = "Question one?\n\nAnswer one.\n\nQuestion two?\n\nAnswer two.";
    let rows =
        parse_text_rows(text, TextStrategy::PairAdjacent, "notes.md").expect("pairs should parse");
    assert_eq!(rows.len(), 2);
    assert!(rows[0].prompt.contains("Question one"));
    assert!(rows[0].response.contains("Answer one"));
}
