//! Timestamp utilities for AdapterOS
//!
//! Provides consistent RFC 3339 timestamp generation across the codebase.

use std::time::SystemTime;

/// Returns the current UTC time as an RFC 3339 formatted string.
///
/// # Examples
///
/// ```rust
/// use adapteros_core::time::now_rfc3339;
///
/// let timestamp = now_rfc3339();
/// // Returns something like "2025-01-15T14:30:00Z"
/// ```
pub fn now_rfc3339() -> String {
    let duration = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time before UNIX epoch");

    let secs = duration.as_secs();

    // Calculate date and time components
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year, month, day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days_since_epoch);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day)
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm based on Howard Hinnant's date algorithms
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now_rfc3339_format() {
        let timestamp = now_rfc3339();
        // Check format: YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(timestamp.len(), 20);
        assert!(timestamp.ends_with('Z'));
        assert_eq!(&timestamp[4..5], "-");
        assert_eq!(&timestamp[7..8], "-");
        assert_eq!(&timestamp[10..11], "T");
        assert_eq!(&timestamp[13..14], ":");
        assert_eq!(&timestamp[16..17], ":");
    }

    #[test]
    fn test_days_to_ymd_epoch() {
        // 1970-01-01
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_known_date() {
        // 2025-01-01 is 20089 days since epoch
        let (y, m, d) = days_to_ymd(20089);
        assert_eq!((y, m, d), (2025, 1, 1));
    }
}
