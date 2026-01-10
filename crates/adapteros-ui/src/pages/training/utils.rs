//! Training page utility functions
//!
//! Pure helper functions for formatting dates, numbers, and durations.

/// Format a date string for display
pub fn format_date(date_str: &str) -> String {
    // Simple formatting - just show date and time
    // In a real app, use a proper date library
    if date_str.len() >= 16 {
        format!("{} {}", &date_str[0..10], &date_str[11..16])
    } else {
        date_str.to_string()
    }
}

/// Format a large number with commas
pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format duration in milliseconds to human readable
pub fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;

    if hours > 0 {
        format!("{}h {}m", hours, mins % 60)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

/// Human-friendly label for preprocessing cache state.
#[allow(dead_code)] // Used by training preprocessing cache UI; keep for upcoming panels.
pub fn preprocess_state_label(cache_hit: bool, needs_reprocess: bool) -> &'static str {
    if needs_reprocess {
        "needs reprocess"
    } else if cache_hit {
        "cache hit"
    } else {
        "cache pending"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_label_reports_reprocess_first() {
        assert_eq!(preprocess_state_label(false, true), "needs reprocess");
        assert_eq!(preprocess_state_label(true, true), "needs reprocess");
    }

    #[test]
    fn preprocess_label_reports_cache_hit() {
        assert_eq!(preprocess_state_label(true, false), "cache hit");
        assert_eq!(preprocess_state_label(false, false), "cache pending");
    }
}
