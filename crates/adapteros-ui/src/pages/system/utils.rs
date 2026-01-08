//! System page utility functions
//!
//! Pure helper functions for formatting timestamps and durations.

/// Maximum workers to show initially (client-side pagination)
pub const WORKERS_PAGE_SIZE: usize = 10;

/// Maximum nodes to show initially (client-side pagination)
pub const NODES_PAGE_SIZE: usize = 10;

/// Format a timestamp for display
pub fn format_timestamp(timestamp: &str) -> String {
    // Try to parse and format nicely, otherwise return as-is
    if timestamp == "-" || timestamp.is_empty() {
        return "-".to_string();
    }

    // If it looks like an ISO timestamp, try to make it more readable
    if timestamp.contains('T') {
        // Extract time portion
        if let Some(time_part) = timestamp.split('T').nth(1) {
            let time = time_part.split('.').next().unwrap_or(time_part);
            return time.to_string();
        }
    }

    timestamp.to_string()
}

/// Format uptime in human-readable format
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
