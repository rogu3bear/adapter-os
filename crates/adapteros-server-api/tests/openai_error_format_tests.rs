//! OpenAI error envelope contract tests.

use adapteros_server_api::handlers::openai_compat::{OpenAiErrorBody, OpenAiErrorResponse};

#[test]
fn openai_error_envelope_shape_is_stable() {
    let error = OpenAiErrorResponse {
        error: OpenAiErrorBody {
            message: "invalid request".to_string(),
            error_type: "invalid_request_error".to_string(),
            code: Some("BAD_REQUEST".to_string()),
            param: Some("messages".to_string()),
        },
    };

    let value = serde_json::to_value(&error).expect("serialize");
    assert!(value.get("error").is_some());
    assert_eq!(value["error"]["message"], "invalid request");
    assert_eq!(value["error"]["type"], "invalid_request_error");
    assert_eq!(value["error"]["code"], "BAD_REQUEST");
    assert_eq!(value["error"]["param"], "messages");
}

#[test]
fn openai_error_optional_fields_omit_when_absent() {
    let error = OpenAiErrorResponse {
        error: OpenAiErrorBody {
            message: "server failure".to_string(),
            error_type: "server_error".to_string(),
            code: None,
            param: None,
        },
    };

    let value = serde_json::to_value(&error).expect("serialize");
    assert!(value["error"].get("code").is_none());
    assert!(value["error"].get("param").is_none());
    assert_eq!(value["error"]["type"], "server_error");
}
