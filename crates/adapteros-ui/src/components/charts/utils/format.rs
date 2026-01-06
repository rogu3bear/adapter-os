//! Formatting utilities for chart axis labels and tooltips.

/// Format a timestamp for display on time axis.
///
/// Automatically chooses format based on time range.
pub fn format_time(timestamp_ms: u64, range_ms: u64) -> String {
    // Convert to seconds for easier calculation
    let secs = (timestamp_ms / 1000) as i64;

    if range_ms < 60_000 {
        // Under 1 minute: show seconds
        format_seconds(secs)
    } else if range_ms < 3_600_000 {
        // Under 1 hour: show minutes:seconds
        format_minutes_seconds(secs)
    } else if range_ms < 86_400_000 {
        // Under 1 day: show hours:minutes
        format_hours_minutes(secs)
    } else {
        // Over 1 day: show date
        format_date(secs)
    }
}

/// Format as seconds (":SS").
fn format_seconds(secs: i64) -> String {
    format!(":{:02}", secs % 60)
}

/// Format as minutes:seconds ("MM:SS").
fn format_minutes_seconds(secs: i64) -> String {
    let mins = (secs / 60) % 60;
    let secs = secs % 60;
    format!("{:02}:{:02}", mins, secs)
}

/// Format as hours:minutes ("HH:MM").
fn format_hours_minutes(secs: i64) -> String {
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    format!("{:02}:{:02}", hours, mins)
}

/// Format as short date ("Jan 3").
fn format_date(secs: i64) -> String {
    // Simple date formatting without external crates
    // This is a simplified version; production might use chrono
    let days_since_epoch = secs / 86400;
    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    // Very rough approximation (ignores leap years for simplicity)
    let _year_approx = 1970 + (days_since_epoch / 365);
    let day_of_year = days_since_epoch % 365;

    // Approximate month and day
    let days_in_months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut remaining_days = day_of_year;
    let mut month_idx = 0;

    for (i, &days) in days_in_months.iter().enumerate() {
        if remaining_days < days as i64 {
            month_idx = i;
            break;
        }
        remaining_days -= days as i64;
    }

    let day = remaining_days + 1;
    format!("{} {}", month_names[month_idx], day)
}

/// Format a timestamp for tooltip display (full precision).
pub fn format_timestamp_full(timestamp_ms: u64) -> String {
    let secs = (timestamp_ms / 1000) as i64;
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let secs = secs % 60;
    let millis = timestamp_ms % 1000;

    format!("{:02}:{:02}:{:02}.{:03}", hours, mins, secs, millis)
}

/// Format a number for axis labels.
pub fn format_number(value: f64) -> String {
    if value.abs() >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value.abs() >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else if value.abs() >= 100.0 {
        format!("{:.0}", value)
    } else if value.abs() >= 10.0 {
        format!("{:.1}", value)
    } else if value.abs() >= 1.0 {
        format!("{:.2}", value)
    } else {
        format!("{:.3}", value)
    }
}

/// Format a percentage value.
pub fn format_percent(value: f64) -> String {
    if value >= 10.0 {
        format!("{:.0}%", value)
    } else if value >= 1.0 {
        format!("{:.1}%", value)
    } else {
        format!("{:.2}%", value)
    }
}

/// Format a latency value in milliseconds.
pub fn format_latency(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.2}s", ms / 1000.0)
    } else if ms >= 100.0 {
        format!("{:.0}ms", ms)
    } else if ms >= 10.0 {
        format!("{:.1}ms", ms)
    } else {
        format!("{:.2}ms", ms)
    }
}

/// Format a throughput value (requests/second).
pub fn format_throughput(rps: f64) -> String {
    if rps >= 1000.0 {
        format!("{:.1}K/s", rps / 1000.0)
    } else if rps >= 100.0 {
        format!("{:.0}/s", rps)
    } else if rps >= 10.0 {
        format!("{:.1}/s", rps)
    } else {
        format!("{:.2}/s", rps)
    }
}

/// Format duration in human-readable form.
pub fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        let mins = seconds / 60;
        let secs = seconds % 60;
        if secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m {}s", mins, secs)
        }
    } else if seconds < 86400 {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        if mins == 0 {
            format!("{}h", hours)
        } else {
            format!("{}h {}m", hours, mins)
        }
    } else {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        if hours == 0 {
            format!("{}d", days)
        } else {
            format!("{}d {}h", days, hours)
        }
    }
}

/// Escape text for safe SVG rendering.
pub fn escape_svg(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(1_500_000.0), "1.5M");
        assert_eq!(format_number(2_500.0), "2.5K");
        assert_eq!(format_number(150.0), "150");
        assert_eq!(format_number(15.5), "15.5");
        assert_eq!(format_number(1.55), "1.55");
        assert_eq!(format_number(0.123), "0.123");
    }

    #[test]
    fn test_format_percent() {
        assert_eq!(format_percent(95.0), "95%");
        assert_eq!(format_percent(5.5), "5.5%");
        assert_eq!(format_percent(0.25), "0.25%");
    }

    #[test]
    fn test_format_latency() {
        assert_eq!(format_latency(2500.0), "2.50s");
        assert_eq!(format_latency(250.0), "250ms");
        assert_eq!(format_latency(25.5), "25.5ms");
        assert_eq!(format_latency(2.55), "2.55ms");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3660), "1h 1m");
        assert_eq!(format_duration(90000), "1d 1h");
    }

    #[test]
    fn test_escape_svg() {
        assert_eq!(escape_svg("<script>"), "&lt;script&gt;");
        assert_eq!(escape_svg("a & b"), "a &amp; b");
    }
}
