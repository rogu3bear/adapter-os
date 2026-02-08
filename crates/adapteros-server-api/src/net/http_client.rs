use crate::api_error::ApiError;
use std::time::Duration;

pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
pub const DEFAULT_TOTAL_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_ERROR_BODY_CHARS: usize = 300;

pub fn truncate_body_chars(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

pub fn build_reqwest_client(
    connect_timeout: Duration,
    total_timeout: Duration,
) -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .connect_timeout(connect_timeout)
        .timeout(total_timeout)
        .build()
        .map_err(|e| {
            ApiError::internal("failed to build HTTP client").with_redacted_details(e.to_string())
        })
}
