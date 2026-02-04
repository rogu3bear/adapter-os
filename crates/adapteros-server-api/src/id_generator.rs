use adapteros_core::ids::{
    generate_id, generate_id_with_suffix_len, generate_suffix, slugify, IdKind,
};
use chrono::Utc;

pub fn date_slug() -> String {
    Utc::now().format("%Y%m%d").to_string()
}

pub fn readable_id(kind: IdKind, slug_source: &str) -> String {
    generate_id(kind, slug_source)
}

pub fn readable_id_with_len(kind: IdKind, slug_source: &str, suffix_len: usize) -> String {
    generate_id_with_suffix_len(kind, slug_source, suffix_len)
}

pub fn readable_request_id() -> String {
    readable_id_with_len(IdKind::Request, &date_slug(), 6)
}

pub fn readable_trace_id() -> String {
    readable_id_with_len(IdKind::Trace, &date_slug(), 6)
}

pub fn readable_run_id() -> String {
    readable_id_with_len(IdKind::Run, &date_slug(), 6)
}

pub fn readable_session_id(slug_source: &str) -> String {
    readable_id(IdKind::Session, &slugify(slug_source))
}

pub fn readable_message_id(slug_source: &str) -> String {
    readable_id(IdKind::Message, &slugify(slug_source))
}

pub fn readable_openai_chatcmpl_id() -> String {
    format!("chatcmpl_{}", generate_suffix(8))
}

pub fn readable_openai_chatcmpl_dash_id() -> String {
    format!("chatcmpl-{}", generate_suffix(8))
}
