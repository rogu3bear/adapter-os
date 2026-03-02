//! OpenAI compatibility tests for chat/completions request and contract fields.

use adapteros_server_api::handlers::openai_compat::{
    OpenAiChatCompletionsRequest, OpenAiCompletionsRequest,
};
use adapteros_server_api::routes::ApiDoc;
use serde_json::json;
use utoipa::OpenApi;

#[test]
fn api_01_response_format_json_schema_deserializes() {
    let payload = json!({
        "model": "adapteros",
        "messages": [{"role": "user", "content": "Return JSON"}],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "receipt",
                "schema": {
                    "type": "object",
                    "required": ["status"],
                    "properties": { "status": {"type": "string"} }
                }
            }
        }
    });

    let req: OpenAiChatCompletionsRequest = serde_json::from_value(payload).expect("deserialize");
    assert_eq!(
        req.response_format.as_ref().map(|f| f.format_type.as_str()),
        Some("json_schema")
    );
    assert!(req.response_format.and_then(|f| f.json_schema).is_some());
}

#[test]
fn api_02_tools_and_tool_choice_deserialize() {
    let payload = json!({
        "model": "adapteros",
        "messages": [{"role": "user", "content": "Call the weather tool"}],
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Fetch weather",
                "parameters": {
                    "type": "object",
                    "properties": {"city": {"type": "string"}}
                }
            }
        }],
        "tool_choice": {
            "type": "function",
            "function": {"name": "get_weather"}
        }
    });

    let req: OpenAiChatCompletionsRequest = serde_json::from_value(payload).expect("deserialize");
    assert_eq!(req.tools.as_ref().map(|tools| tools.len()), Some(1));
    assert!(req.tool_choice.is_some());
}

#[test]
fn api_03_usage_shape_is_coherent() {
    let usage = json!({
        "prompt_tokens": 8,
        "completion_tokens": 12,
        "total_tokens": 20
    });
    let prompt = usage["prompt_tokens"].as_u64().unwrap();
    let completion = usage["completion_tokens"].as_u64().unwrap();
    let total = usage["total_tokens"].as_u64().unwrap();
    assert_eq!(prompt + completion, total);
}

#[test]
fn api_04_missing_openai_parameters_deserialize() {
    let chat_payload = json!({
        "model": "adapteros",
        "messages": [{"role": "user", "content": "hi"}],
        "seed": 42,
        "stop": ["END"],
        "frequency_penalty": 0.0,
        "presence_penalty": 0.0,
        "logprobs": false
    });
    let chat_req: OpenAiChatCompletionsRequest =
        serde_json::from_value(chat_payload).expect("chat deserialize");
    assert_eq!(chat_req.seed, Some(42));
    assert!(chat_req.stop.is_some());

    let completion_payload = json!({
        "model": "adapteros",
        "prompt": "hello",
        "seed": 42,
        "stop": "END",
        "frequency_penalty": 0.0,
        "presence_penalty": 0.0,
        "logprobs": false
    });
    let completion_req: OpenAiCompletionsRequest =
        serde_json::from_value(completion_payload).expect("completions deserialize");
    assert_eq!(completion_req.seed, Some(42));
    assert!(completion_req.stop.is_some());
}

#[test]
fn api_05_openapi_contains_phase_04_chat_fields() {
    let spec = serde_json::to_value(ApiDoc::openapi()).expect("openapi value");
    let schemas = spec["components"]["schemas"]
        .as_object()
        .expect("schemas object");

    let req_props = schemas["OpenAiChatCompletionsRequest"]["properties"]
        .as_object()
        .expect("chat request properties");

    for field in [
        "response_format",
        "tools",
        "tool_choice",
        "seed",
        "stop",
        "frequency_penalty",
        "presence_penalty",
        "logprobs",
    ] {
        assert!(
            req_props.contains_key(field),
            "missing field {field} in OpenAPI schema"
        );
    }
}
