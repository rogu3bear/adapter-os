//! Workers page utility functions
//!
//! Pure helper functions for formatting and display.

use crate::components::BadgeVariant;

/// Page size for client-side pagination (reduces initial DOM nodes)
pub const WORKERS_PAGE_SIZE: usize = 25;

/// Format an ISO timestamp for display (extracts time portion)
pub fn format_timestamp(timestamp: &str) -> String {
    if timestamp == "-" || timestamp.is_empty() {
        return "-".to_string();
    }
    if timestamp.contains('T') {
        if let Some(time_part) = timestamp.split('T').nth(1) {
            let time = time_part.split('.').next().unwrap_or(time_part);
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

/// Truncate an ID for display (first 12 chars)
pub fn short_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..12])
    } else {
        id.to_string()
    }
}

/// Truncate a hash for display (first 8 chars)
pub fn short_hash(hash: &str) -> String {
    if hash.len() > 8 {
        format!("{}...", &hash[..8])
    } else {
        hash.to_string()
    }
}

/// Map worker status string to badge variant
pub fn status_badge_variant(status: &str) -> BadgeVariant {
    match status {
        "healthy" => BadgeVariant::Success,
        "draining" => BadgeVariant::Warning,
        "registered" => BadgeVariant::Secondary,
        "error" | "stopped" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    }
}
