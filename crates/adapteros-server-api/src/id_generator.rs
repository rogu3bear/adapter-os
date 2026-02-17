use adapteros_id::{IdPrefix, TypedId};

/// Generate a typed ID for the given prefix. The `_slug_source` parameter is
/// retained for call-site compatibility but no longer affects the output.
pub fn readable_id(prefix: IdPrefix, _slug_source: &str) -> String {
    TypedId::new(prefix).to_string()
}

pub fn readable_request_id() -> String {
    TypedId::new(IdPrefix::Req).to_string()
}

pub fn readable_trace_id() -> String {
    TypedId::new(IdPrefix::Trc).to_string()
}

pub fn readable_run_id() -> String {
    TypedId::new(IdPrefix::Run).to_string()
}

pub fn readable_session_id(_slug_source: &str) -> String {
    TypedId::new(IdPrefix::Ses).to_string()
}

pub fn readable_message_id(_slug_source: &str) -> String {
    TypedId::new(IdPrefix::Msg).to_string()
}

/// OpenAI chat-completion ID with underscore separator (API contract).
pub fn readable_openai_chatcmpl_id() -> String {
    let id = TypedId::new(IdPrefix::Req);
    format!("chatcmpl_{}", id.short_hex())
}

/// OpenAI chat-completion ID with dash separator (API contract).
pub fn readable_openai_chatcmpl_dash_id() -> String {
    let id = TypedId::new(IdPrefix::Req);
    format!("chatcmpl-{}", id.short_hex())
}
