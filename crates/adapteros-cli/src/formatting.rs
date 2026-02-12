//! Formatting utilities for CLI output.
//!
//! This module provides consistent formatting functions for common CLI output needs:
//! - Byte sizes (KB, MB, GB)
//! - Durations and time ago displays
//! - ID and hash truncation
//!
//! # Examples
//!
//! ```
//! use adapteros_cli::formatting::{format_bytes, format_duration, truncate_id};
//! use std::time::Duration;
//!
//! assert_eq!(format_bytes(1024), "1.0 KB");
//! assert_eq!(format_bytes(1_500_000), "1.4 MB");
//! assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
//! assert_eq!(truncate_id("adapter-123456789"), "adapter-");
//! ```

use chrono::{DateTime, Utc};
use std::time::Duration;

/// Format bytes as human-readable string with appropriate unit.
///
/// Uses binary units (1 KB = 1024 bytes) and rounds to one decimal place.
///
/// # Examples
///
/// ```
/// use adapteros_cli::formatting::format_bytes;
///
/// assert_eq!(format_bytes(0), "0 B");
/// assert_eq!(format_bytes(512), "512 B");
/// assert_eq!(format_bytes(1024), "1.0 KB");
/// assert_eq!(format_bytes(1536), "1.5 KB");
/// assert_eq!(format_bytes(1_048_576), "1.0 MB");
/// assert_eq!(format_bytes(1_500_000_000), "1.4 GB");
/// assert_eq!(format_bytes(1_099_511_627_776), "1.0 TB");
/// ```
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format duration as human-readable string.
///
/// Shows the largest two time units (e.g., "2h 30m", "45s").
/// For durations less than 1 second, shows milliseconds.
///
/// # Examples
///
/// ```
/// use adapteros_cli::formatting::format_duration;
/// use std::time::Duration;
///
/// assert_eq!(format_duration(Duration::from_secs(0)), "0s");
/// assert_eq!(format_duration(Duration::from_secs(30)), "30s");
/// assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
/// assert_eq!(format_duration(Duration::from_secs(3661)), "1h 1m");
/// assert_eq!(format_duration(Duration::from_secs(90000)), "1d 1h");
/// assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
/// ```
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();

    if total_secs == 0 {
        let millis = duration.as_millis();
        if millis == 0 {
            return "0s".to_string();
        }
        return format!("{}ms", millis);
    }

    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    let mut parts = Vec::new();

    if days > 0 {
        parts.push(format!("{}d", days));
        if hours > 0 {
            parts.push(format!("{}h", hours));
        }
    } else if hours > 0 {
        parts.push(format!("{}h", hours));
        if minutes > 0 {
            parts.push(format!("{}m", minutes));
        }
    } else if minutes > 0 {
        parts.push(format!("{}m", minutes));
        if seconds > 0 {
            parts.push(format!("{}s", seconds));
        }
    } else {
        parts.push(format!("{}s", seconds));
    }

    parts.join(" ")
}

/// Format seconds as human-readable string.
///
/// Convenience wrapper around `format_duration` for when you have seconds as u64.
///
/// # Examples
///
/// ```
/// use adapteros_cli::formatting::format_seconds;
///
/// assert_eq!(format_seconds(0), "0s");
/// assert_eq!(format_seconds(125), "2m 5s");
/// assert_eq!(format_seconds(3661), "1h 1m");
/// ```
pub fn format_seconds(secs: u64) -> String {
    format_duration(Duration::from_secs(secs))
}

/// Format timestamp string as "time ago" relative to now.
///
/// Returns strings like "2m ago", "1h ago", "3d ago", or "just now" for very recent times.
/// If the timestamp cannot be parsed, returns "unknown".
///
/// # Examples
///
/// ```no_run
/// use adapteros_cli::formatting::format_time_ago;
///
/// // Assuming current time, these would produce appropriate relative times
/// assert_eq!(format_time_ago("2025-11-29T10:00:00Z"), "2m ago");
/// ```
pub fn format_time_ago(timestamp: &str) -> String {
    let parsed = match DateTime::parse_from_rfc3339(timestamp) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => {
            // Try parsing as naive datetime and assume UTC
            match timestamp.parse::<DateTime<Utc>>() {
                Ok(dt) => dt,
                Err(_) => return "unknown".to_string(),
            }
        }
    };

    let now = Utc::now();
    let duration = now.signed_duration_since(parsed);

    if duration.num_seconds() < 0 {
        return "just now".to_string();
    }

    let secs = duration.num_seconds() as u64;
    let mins = secs / 60;
    let hours = mins / 60;
    let days = hours / 24;

    if days > 0 {
        format!("{}d ago", days)
    } else if hours > 0 {
        format!("{}h ago", hours)
    } else if mins > 0 {
        format!("{}m ago", mins)
    } else if secs > 5 {
        format!("{}s ago", secs)
    } else {
        "just now".to_string()
    }
}

/// Truncate string to 8 characters for ID display.
///
/// Used to show compact identifiers in tables and lists.
///
/// # Examples
///
/// ```
/// use adapteros_cli::formatting::truncate_id;
///
/// assert_eq!(truncate_id("adapter-123456789"), "adapter-");
/// assert_eq!(truncate_id("short"), "short");
/// assert_eq!(truncate_id("exactly8"), "exactly8");
/// ```
pub fn truncate_id(s: &str) -> &str {
    if s.len() <= 8 {
        s
    } else {
        &s[..8]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(1_572_864), "1.5 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
        assert_eq!(format_bytes(1_500_000_000), "1.4 GB");
        assert_eq!(format_bytes(1_099_511_627_776), "1.0 TB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
        assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
        assert_eq!(format_duration(Duration::from_secs(30)), "30s");
        assert_eq!(format_duration(Duration::from_secs(60)), "1m");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h");
        assert_eq!(format_duration(Duration::from_secs(3661)), "1h 1m");
        assert_eq!(format_duration(Duration::from_secs(7200)), "2h");
        assert_eq!(format_duration(Duration::from_secs(86400)), "1d");
        assert_eq!(format_duration(Duration::from_secs(90000)), "1d 1h");
        assert_eq!(format_duration(Duration::from_secs(90061)), "1d 1h");
    }

    #[test]
    fn test_format_seconds() {
        assert_eq!(format_seconds(0), "0s");
        assert_eq!(format_seconds(45), "45s");
        assert_eq!(format_seconds(125), "2m 5s");
        assert_eq!(format_seconds(3661), "1h 1m");
    }

    #[test]
    fn test_format_time_ago() {
        // Test with a known past timestamp
        let now = Utc::now();

        // 2 minutes ago
        let two_min_ago = (now - chrono::Duration::minutes(2)).to_rfc3339();
        assert_eq!(format_time_ago(&two_min_ago), "2m ago");

        // 1 hour ago
        let one_hour_ago = (now - chrono::Duration::hours(1)).to_rfc3339();
        assert_eq!(format_time_ago(&one_hour_ago), "1h ago");

        // 3 days ago
        let three_days_ago = (now - chrono::Duration::days(3)).to_rfc3339();
        assert_eq!(format_time_ago(&three_days_ago), "3d ago");

        // Just now (2 seconds ago)
        let just_now = (now - chrono::Duration::seconds(2)).to_rfc3339();
        assert_eq!(format_time_ago(&just_now), "just now");

        // Invalid timestamp
        assert_eq!(format_time_ago("not-a-timestamp"), "unknown");
    }

    #[test]
    fn test_truncate_id() {
        assert_eq!(truncate_id("short"), "short");
        assert_eq!(truncate_id("exactly8"), "exactly8");
        assert_eq!(truncate_id("adapter-123456789"), "adapter-");
        assert_eq!(truncate_id("a"), "a");
        assert_eq!(truncate_id(""), "");
    }
}
