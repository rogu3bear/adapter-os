//! Shared utility functions for formatting dates, bytes, and other values.
//!
//! This module provides canonical implementations of common formatting functions
//! to avoid code duplication across the UI crate.

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

/// Returns the current UTC time in a WASM-safe way.
///
/// In `wasm32-unknown-unknown`, `std::time::SystemTime` is not available and
/// can panic when transitively used by date/time libraries. Use JS `Date`.
pub fn now_utc() -> chrono::DateTime<chrono::Utc> {
    #[cfg(target_arch = "wasm32")]
    {
        let ms = js_sys::Date::new_0().get_time();
        let secs = (ms / 1000.0).floor() as i64;
        let sub_ms = ms - (secs as f64 * 1000.0);
        let nsec = (sub_ms * 1_000_000.0).round().clamp(0.0, 999_999_999.0) as u32;

        chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nsec)
            .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::MIN_UTC)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        chrono::Utc::now()
    }
}

/// Format an ISO 8601 date string for display, showing date and time in 12-hour AM/PM.
///
/// Extracts the date (YYYY-MM-DD) and time (h:MM AM/PM) portions from an ISO timestamp.
/// Falls back to the original string if it's too short.
///
/// # Examples
///
/// ```
/// use adapteros_ui::utils::format_datetime;
///
/// assert_eq!(format_datetime("2024-01-15T14:30:00Z"), "2024-01-15 2:30 PM");
/// assert_eq!(format_datetime("2024-01-15"), "2024-01-15");
/// ```
pub fn format_datetime(date_str: &str) -> String {
    if date_str.len() >= 16 {
        let date_part = &date_str[0..10];
        let time_24h = &date_str[11..16]; // "HH:MM"
        let parts: Vec<&str> = time_24h.split(':').collect();
        if parts.len() == 2 {
            if let Ok(h24) = parts[0].parse::<u32>() {
                let mins = parts[1];
                let (h12, period) = match h24 {
                    0 => (12, "AM"),
                    1..=11 => (h24, "AM"),
                    12 => (12, "PM"),
                    _ => (h24 - 12, "PM"),
                };
                return format!("{} {}:{} {}", date_part, h12, mins, period);
            }
        }
        format!("{} {}", date_part, time_24h)
    } else {
        date_str.to_string()
    }
}

/// Format an ISO 8601 date string for display, showing only the date portion.
///
/// Extracts just the date (YYYY-MM-DD) from an ISO timestamp by splitting on 'T'.
/// Falls back to the original string if no 'T' separator is found.
///
/// # Examples
///
/// ```
/// use adapteros_ui::utils::format_date;
///
/// assert_eq!(format_date("2024-01-15T14:30:00Z"), "2024-01-15");
/// assert_eq!(format_date("2024-01-15"), "2024-01-15");
/// ```
pub fn format_date(date_str: &str) -> String {
    date_str.split('T').next().unwrap_or(date_str).to_string()
}

/// Format a byte count as a human-readable size string.
///
/// Uses binary units (KiB = 1024 bytes) and formats with one decimal place
/// for KB, MB, and GB values.
///
/// # Examples
///
/// ```
/// use adapteros_ui::utils::format_bytes;
///
/// assert_eq!(format_bytes(512), "512 B");
/// assert_eq!(format_bytes(1536), "1.5 KB");
/// assert_eq!(format_bytes(1_572_864), "1.5 MB");
/// assert_eq!(format_bytes(1_610_612_736), "1.5 GB");
/// ```
pub fn format_bytes(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Copy text to the clipboard using the browser's Clipboard API.
///
/// This function uses the navigator.clipboard.writeText() method to copy
/// text to the system clipboard. It handles all error cases gracefully
/// and returns a boolean indicating success.
///
/// # Returns
///
/// - `true` if the text was successfully copied to the clipboard
/// - `false` if the clipboard API is unavailable or the operation failed
///
/// # Example
///
/// ```ignore
/// use adapteros_ui::utils::copy_to_clipboard;
///
/// let success = copy_to_clipboard("Hello, clipboard!").await;
/// if success {
///     // Show success toast
/// }
/// ```
pub async fn copy_to_clipboard(text: &str) -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };

    let navigator = window.navigator();

    // Get clipboard from navigator using JS reflection
    let clipboard = js_sys::Reflect::get(&navigator, &wasm_bindgen::JsValue::from_str("clipboard"))
        .ok()
        .filter(|v| !v.is_undefined());

    let clipboard = match clipboard {
        Some(c) => c,
        None => return false,
    };

    // Call writeText method
    let write_text_fn =
        match js_sys::Reflect::get(&clipboard, &wasm_bindgen::JsValue::from_str("writeText")) {
            Ok(f) => f,
            Err(_) => return false,
        };

    let write_text_fn = match write_text_fn.dyn_ref::<js_sys::Function>() {
        Some(f) => f,
        None => return false,
    };

    let promise = match write_text_fn.call1(&clipboard, &wasm_bindgen::JsValue::from_str(text)) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let promise = match promise.dyn_into::<js_sys::Promise>() {
        Ok(p) => p,
        Err(_) => return false,
    };

    JsFuture::from(promise).await.is_ok()
}

/// Format a timestamp as relative time (e.g., "Just now", "5 min ago", "2 days ago").
///
/// Parses an RFC 3339 timestamp and returns a human-readable relative time string.
/// Falls back to the original string if parsing fails.
///
/// # Time Ranges
///
/// - Less than 1 minute: "Just now"
/// - 1-59 minutes: "{n} min ago"
/// - 1-23 hours: "{n} hours ago"
/// - 1-6 days: "{n} days ago"
/// - 7+ days: Formatted as "Mon DD" (e.g., "Jan 15")
///
/// # Example
///
/// ```ignore
/// use adapteros_ui::utils::format_relative_time;
///
/// // Assuming current time is 2024-01-15T14:35:00Z
/// assert_eq!(format_relative_time("2024-01-15T14:34:00Z"), "1 min ago");
/// assert_eq!(format_relative_time("2024-01-15T12:35:00Z"), "2 hours ago");
/// ```
pub fn format_relative_time(timestamp: &str) -> String {
    use chrono::{DateTime, Utc};

    let Ok(dt) = DateTime::parse_from_rfc3339(timestamp) else {
        return timestamp.to_string();
    };

    let now = crate::utils::now_utc();
    let diff = now.signed_duration_since(dt.with_timezone(&Utc));

    if diff.num_minutes() < 1 {
        "Just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{} min ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{} hours ago", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{} days ago", diff.num_days())
    } else {
        dt.format("%b %d").to_string()
    }
}

/// Convert a string to a URL-friendly slug.
///
/// Converts the input to lowercase, replaces non-alphanumeric characters with
/// dashes, and removes leading/trailing dashes. Returns "item" if the result
/// would be empty.
///
/// # Examples
///
/// ```
/// use adapteros_ui::utils::slugify;
///
/// assert_eq!(slugify("Hello World!"), "hello-world");
/// assert_eq!(slugify("My Test Adapter"), "my-test-adapter");
/// assert_eq!(slugify("  spaces  "), "spaces");
/// assert_eq!(slugify("!!!"), "item");
/// ```
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed
    }
}

/// Title-case a single word: capitalize the first character, lowercase the rest.
///
/// # Examples
///
/// ```
/// use adapteros_ui::utils::title_case;
///
/// assert_eq!(title_case("running"), "Running");
/// assert_eq!(title_case("OOM"), "Oom");
/// assert_eq!(title_case(""), "");
/// ```
pub fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            let rest: String = chars.flat_map(|c| c.to_lowercase()).collect();
            format!("{}{}", upper, rest)
        }
    }
}

/// Convert an underscore/hyphen-separated string to human-readable Title Case.
///
/// Splits on `_` and `-`, title-cases each word, and joins with spaces.
///
/// # Examples
///
/// ```
/// use adapteros_ui::utils::humanize;
///
/// assert_eq!(humanize("training_job"), "Training Job");
/// assert_eq!(humanize("memory_spike"), "Memory Spike");
/// assert_eq!(humanize("active"), "Active");
/// assert_eq!(humanize(""), "");
/// ```
pub fn humanize(s: &str) -> String {
    s.split(['_', '-'])
        .map(title_case)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convert a backend status/state code to a user-facing label.
///
/// Uses curated mappings for common API statuses and falls back to [`humanize`]
/// for unknown values.
pub fn status_display_label(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    match trimmed.to_ascii_lowercase().as_str() {
        "ok" | "healthy" => "Healthy".to_string(),
        "ready" => "Ready".to_string(),
        "indexed" => "Indexed".to_string(),
        "uploaded" => "Uploaded".to_string(),
        "processing" => "Processing".to_string(),
        "chunked" => "Chunked".to_string(),
        "embedded" => "Embedded".to_string(),
        "running" => "Running".to_string(),
        "completed" => "Completed".to_string(),
        "success" => "Success".to_string(),
        "failure" | "failed" => "Failed".to_string(),
        "warning" => "Warning".to_string(),
        "degraded" => "Degraded".to_string(),
        "error" => "Error".to_string(),
        "pending" => "Pending".to_string(),
        "queued" => "Queued".to_string(),
        "created" => "Created".to_string(),
        "registered" => "Registered".to_string(),
        "draining" => "Draining".to_string(),
        "stopped" => "Stopped".to_string(),
        "paused" => "Paused".to_string(),
        "active" => "Active".to_string(),
        "archived" => "Archived".to_string(),
        "unknown" => "Unknown".to_string(),
        "not_ready" | "not-ready" => "Not Ready".to_string(),
        "valid" => "Valid".to_string(),
        "invalid" => "Invalid".to_string(),
        "blocked" => "Blocked".to_string(),
        "allowed" => "Allowed".to_string(),
        "allowed_with_warning" | "allowed-with-warning" => "Allowed With Warning".to_string(),
        "needs_approval" | "needs-approval" => "Needs Approval".to_string(),
        "unverified" => "Unverified".to_string(),
        "not_verified" | "not-verified" => "Not Verified".to_string(),
        other => humanize(other),
    }
}

/// Return a user-facing status label and preserve raw status when it differs.
///
/// Example output: `Allowed With Warning (allowed_with_warning)`.
pub fn status_display_with_raw(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let label = status_display_label(trimmed);
    if label.eq_ignore_ascii_case(trimmed) {
        label
    } else {
        format!("{} ({})", label, trimmed)
    }
}

/// Generate a random alphanumeric suffix of the specified length.
///
/// Uses a base32-like alphabet (lowercase letters and digits 2-7) to generate
/// a random string. This is useful for creating unique identifiers.
///
/// # Example
///
/// ```ignore
/// use adapteros_ui::utils::random_suffix;
///
/// let suffix = random_suffix(6);
/// assert_eq!(suffix.len(), 6);
/// // e.g., "abc234"
/// ```
pub fn random_suffix(len: usize) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let idx = (js_sys::Math::random() * 32.0).floor() as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// Build a chat URL that creates a new session with an adapter pre-pinned.
///
/// Returns a path like `/chat?adapter=my-adapter-id` that quickstart parses
/// before creating the session and carrying adapter context forward.
pub fn chat_path_with_adapter(adapter_id: &str) -> String {
    // Keep this URL-safe even if adapter ids ever include non-url characters.
    let encoded = js_sys::encode_uri_component(adapter_id)
        .as_string()
        .unwrap_or_else(|| adapter_id.to_string());
    format!("/chat?adapter={}", encoded)
}

/// Return a stable UI session id for correlation across requests (ses-...).
///
/// Stored in `sessionStorage` so refresh preserves it per-tab; users can rotate it by clearing
/// session storage.
#[cfg(target_arch = "wasm32")]
pub fn ui_session_id() -> String {
    const KEY: &str = "aos_ui_session_id";

    let window = match web_sys::window() {
        Some(w) => w,
        None => return adapteros_id::TypedId::new(adapteros_id::IdPrefix::Ses).to_string(),
    };

    let storage = window.session_storage().ok().flatten();
    let storage = match storage {
        Some(s) => s,
        None => return adapteros_id::TypedId::new(adapteros_id::IdPrefix::Ses).to_string(),
    };

    if let Ok(Some(existing)) = storage.get_item(KEY) {
        if !existing.trim().is_empty() {
            return existing;
        }
    }

    let id = adapteros_id::TypedId::new(adapteros_id::IdPrefix::Ses).to_string();
    let _ = storage.set_item(KEY, &id);
    id
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ui_session_id() -> String {
    adapteros_id::TypedId::new(adapteros_id::IdPrefix::Ses).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_datetime_with_full_timestamp() {
        assert_eq!(
            format_datetime("2024-01-15T14:30:00Z"),
            "2024-01-15 2:30 PM"
        );
        assert_eq!(
            format_datetime("2024-12-31T23:59:59.999Z"),
            "2024-12-31 11:59 PM"
        );
    }

    #[test]
    fn format_datetime_with_short_string() {
        assert_eq!(format_datetime("2024-01-15"), "2024-01-15");
        assert_eq!(format_datetime("short"), "short");
        assert_eq!(format_datetime(""), "");
    }

    #[test]
    fn format_date_splits_on_t() {
        assert_eq!(format_date("2024-01-15T14:30:00Z"), "2024-01-15");
        assert_eq!(format_date("2024-12-31T00:00:00"), "2024-12-31");
    }

    #[test]
    fn format_date_no_t_separator() {
        assert_eq!(format_date("2024-01-15"), "2024-01-15");
        assert_eq!(format_date("no-separator"), "no-separator");
    }

    #[test]
    fn format_bytes_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(1_572_864), "1.5 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
        assert_eq!(format_bytes(1_610_612_736), "1.5 GB");
    }

    #[test]
    fn format_bytes_negative() {
        // Negative bytes should still format (defensive)
        assert_eq!(format_bytes(-100), "-100 B");
    }

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("My Test Adapter"), "my-test-adapter");
    }

    #[test]
    fn slugify_special_characters() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("test@example#123"), "test-example-123");
        assert_eq!(slugify("foo---bar"), "foo-bar");
    }

    #[test]
    fn slugify_leading_trailing() {
        assert_eq!(slugify("  spaces  "), "spaces");
        assert_eq!(slugify("---dashes---"), "dashes");
        assert_eq!(slugify("!@#start"), "start");
    }

    #[test]
    fn slugify_empty_result() {
        assert_eq!(slugify("!!!"), "item");
        assert_eq!(slugify(""), "item");
        assert_eq!(slugify("   "), "item");
    }

    #[test]
    fn slugify_preserves_numbers() {
        assert_eq!(slugify("Model v2.1"), "model-v2-1");
        assert_eq!(slugify("test123"), "test123");
    }

    #[test]
    fn title_case_basic() {
        assert_eq!(title_case("running"), "Running");
        assert_eq!(title_case("active"), "Active");
        assert_eq!(title_case("OOM"), "Oom");
    }

    #[test]
    fn title_case_edge_cases() {
        assert_eq!(title_case(""), "");
        assert_eq!(title_case("a"), "A");
        assert_eq!(title_case("A"), "A");
    }

    #[test]
    fn humanize_underscore_separated() {
        assert_eq!(humanize("training_job"), "Training Job");
        assert_eq!(humanize("memory_spike"), "Memory Spike");
        assert_eq!(humanize("latency_degradation"), "Latency Degradation");
    }

    #[test]
    fn humanize_single_word() {
        assert_eq!(humanize("active"), "Active");
        assert_eq!(humanize("admin"), "Admin");
    }

    #[test]
    fn humanize_hyphenated() {
        assert_eq!(humanize("api-keys"), "Api Keys");
        assert_eq!(humanize("needs-approval"), "Needs Approval");
    }

    #[test]
    fn humanize_empty() {
        assert_eq!(humanize(""), "");
    }

    #[test]
    fn status_display_label_maps_common_statuses() {
        assert_eq!(status_display_label("ok"), "Healthy");
        assert_eq!(
            status_display_label("allowed_with_warning"),
            "Allowed With Warning"
        );
        assert_eq!(status_display_label("needs-approval"), "Needs Approval");
        assert_eq!(status_display_label("not_verified"), "Not Verified");
    }

    #[test]
    fn status_display_label_falls_back_to_humanize() {
        assert_eq!(status_display_label("circuit_open"), "Circuit Open");
    }

    #[test]
    fn status_display_with_raw_preserves_raw_when_label_differs() {
        assert_eq!(
            status_display_with_raw("allowed_with_warning"),
            "Allowed With Warning (allowed_with_warning)"
        );
        assert_eq!(status_display_with_raw("ready"), "Ready");
    }

    // Note: format_relative_time, random_suffix, generate_readable_id, and copy_to_clipboard
    // require WASM environment (js_sys, web_sys, chrono with wasm) and cannot be tested
    // in native mode. They should be tested via wasm-pack or integration tests.
}
