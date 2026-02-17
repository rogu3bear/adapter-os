//! Workers page utility functions
//!
//! Pure helper functions for formatting and display.

use crate::components::{BadgeVariant, StatusVariant};
use serde::Deserialize;
use wasm_bindgen::JsValue;

/// Page size for client-side pagination (reduces initial DOM nodes)
pub const WORKERS_PAGE_SIZE: usize = 25;

/// Format an ISO timestamp for display (extracts time in 12-hour AM/PM format)
pub fn format_timestamp(timestamp: &str) -> String {
    if timestamp == "-" || timestamp.is_empty() {
        return "-".to_string();
    }
    if timestamp.contains('T') {
        if let Some(time_part) = timestamp.split('T').nth(1) {
            let time = time_part.split('.').next().unwrap_or(time_part);
            // Convert HH:MM:SS to 12-hour AM/PM
            let parts: Vec<&str> = time.split(':').collect();
            if parts.len() >= 2 {
                if let Ok(h24) = parts[0].parse::<u32>() {
                    let mins = parts[1];
                    let (h12, period) = match h24 {
                        0 => (12, "AM"),
                        1..=11 => (h24, "AM"),
                        12 => (12, "PM"),
                        _ => (h24 - 12, "PM"),
                    };
                    return format!("{}:{} {}", h12, mins, period);
                }
            }
            return time.to_string();
        }
    }
    timestamp.to_string()
}

/// Format uptime seconds into human-readable duration
pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

/// Format an ISO date string into a human-readable relative date
/// e.g., "Today", "Yesterday", "Jan 5", "Dec 28, 2025"
pub fn format_relative_date(iso_date: &str) -> String {
    // Parse date portion (YYYY-MM-DD) from ISO string
    let date_part = if iso_date.contains('T') {
        iso_date.split('T').next().unwrap_or(iso_date)
    } else {
        iso_date
    };

    // Get current date from JavaScript
    let now = js_sys::Date::new_0();
    let today_year = now.get_full_year() as i32;
    let today_month = now.get_month() + 1; // JS months are 0-indexed
    let today_day = now.get_date();

    // Parse the input date
    let parts: Vec<&str> = date_part.split('-').collect();
    if parts.len() != 3 {
        return date_part.to_string();
    }

    let year: i32 = parts[0].parse().unwrap_or(0);
    let month: u32 = parts[1].parse().unwrap_or(0);
    let day: u32 = parts[2].parse().unwrap_or(0);

    if year == 0 || month == 0 || day == 0 {
        return date_part.to_string();
    }

    // Check if it's today
    if year == today_year && month == today_month && day == today_day {
        return "today".to_string();
    }

    // Check if it's yesterday (simple check, doesn't handle month boundaries perfectly)
    if year == today_year && month == today_month && day == today_day.saturating_sub(1) {
        return "yesterday".to_string();
    }

    // Format as "Jan 5" or "Jan 5, 2025" if different year
    let month_name = match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    };

    if year == today_year {
        format!("{} {}", month_name, day)
    } else {
        format!("{} {}, {}", month_name, day, year)
    }
}

/// Human-readable name for a worker.
///
/// Prefers `WorkerResponse.display_name` (server-provided semantic name),
/// then falls back to `short_id` which returns the word alias (or hex truncation
/// for non-aliased types).
pub fn worker_display_name(id: &str, display_name: Option<&str>) -> String {
    if let Some(name) = display_name.filter(|s| !s.is_empty()) {
        return name.to_string();
    }
    adapteros_id::short_id(id)
}

/// Human-readable name from just an ID string (no `WorkerResponse` available).
pub fn worker_name_from_id(id: &str) -> String {
    worker_display_name(id, None)
}

/// Map worker status string to badge variant via StatusVariant
pub fn status_badge_variant(status: &str) -> BadgeVariant {
    StatusVariant::from_worker_status(status).to_badge_variant()
}

/// Health summary response from /v1/workers/health/summary
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerHealthSummary {
    pub summary: WorkerHealthSummaryCounts,
    pub workers: Vec<WorkerHealthRecord>,
    #[serde(default)]
    pub timestamp: String,
}

/// Summary counts for worker health
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WorkerHealthSummaryCounts {
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub healthy: usize,
    #[serde(default)]
    pub degraded: usize,
    #[serde(default)]
    pub crashed: usize,
    #[serde(default)]
    pub unknown: usize,
}

/// Per-worker health record (subset of /v1/workers/health/summary)
#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct WorkerHealthRecord {
    #[serde(default)]
    pub worker_id: String,
    #[serde(default)]
    pub tenant_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub health_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_response_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consecutive_slow: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consecutive_failures: Option<i64>,
    #[serde(default)]
    pub recent_incidents_24h: i64,
}

/// Map worker health status to badge variant via StatusVariant
pub fn health_badge_variant(status: &str) -> BadgeVariant {
    StatusVariant::from_worker_status(status).to_badge_variant()
}

/// Returns true for terminal worker states that are useful as history,
/// but noisy for QA when you only care about currently-running workers.
pub fn is_terminal_worker_status(status: &str) -> bool {
    let normalized = status.trim();
    normalized.eq_ignore_ascii_case("stopped")
        || normalized.eq_ignore_ascii_case("error")
        || normalized.eq_ignore_ascii_case("crashed")
        || normalized.eq_ignore_ascii_case("failed")
}

/// Best-effort recency check for worker timestamps.
///
/// Accepts either ISO-8601 (contains `T`) or SQLite-style (`YYYY-MM-DD HH:MM:SS`)
/// timestamps and treats them as UTC when timezone is missing.
pub fn is_recent_timestamp(ts: &str, max_age_secs: u64) -> bool {
    if ts.is_empty() || ts == "-" {
        return false;
    }

    let normalized = if ts.contains('T') {
        ts.to_string()
    } else {
        // SQLite datetime('now') format: "YYYY-MM-DD HH:MM:SS" (UTC).
        format!("{}Z", ts.replace(' ', "T"))
    };

    let parsed = js_sys::Date::new(&JsValue::from_str(&normalized));
    let ms = parsed.get_time();
    if ms.is_nan() {
        return false;
    }

    let now = js_sys::Date::new_0().get_time();
    let age_ms = now - ms;
    age_ms >= 0.0 && age_ms <= (max_age_secs as f64) * 1000.0
}
