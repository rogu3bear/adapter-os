//! Timestamp utilities for adapterOS
//!
//! Provides consistent RFC 3339 timestamp generation and Unix timestamp utilities
//! across the codebase.

use std::time::{SystemTime, UNIX_EPOCH};

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
        .duration_since(UNIX_EPOCH)
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

/// Get current Unix timestamp in seconds.
///
/// Returns 0 if the system time is before the Unix epoch (highly unlikely).
///
/// # Examples
///
/// ```rust
/// use adapteros_core::time::unix_timestamp_secs;
///
/// let timestamp = unix_timestamp_secs();
/// // Returns something like 1737816000
/// ```
pub fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Get current Unix timestamp in milliseconds.
///
/// Returns 0 if the system time is before the Unix epoch (highly unlikely).
///
/// # Examples
///
/// ```rust
/// use adapteros_core::time::unix_timestamp_millis;
///
/// let timestamp = unix_timestamp_millis();
/// // Returns something like 1737816000000
/// ```
pub fn unix_timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Get current Unix timestamp in microseconds (u64 for compatibility).
///
/// Returns 0 if the system time is before the Unix epoch (highly unlikely).
///
/// # Examples
///
/// ```rust
/// use adapteros_core::time::unix_timestamp_micros;
///
/// let timestamp = unix_timestamp_micros();
/// // Returns something like 1737816000000000
/// ```
pub fn unix_timestamp_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Get current Unix timestamp in microseconds (u128 for high precision).
///
/// Returns 0 if the system time is before the Unix epoch (highly unlikely).
///
/// # Examples
///
/// ```rust
/// use adapteros_core::time::unix_timestamp_micros_u128;
///
/// let timestamp = unix_timestamp_micros_u128();
/// // Returns something like 1737816000000000
/// ```
pub fn unix_timestamp_micros_u128() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0)
}

/// Get current Unix timestamp in nanoseconds.
///
/// Returns 0 if the system time is before the Unix epoch (highly unlikely).
///
/// # Examples
///
/// ```rust
/// use adapteros_core::time::unix_timestamp_nanos;
///
/// let timestamp = unix_timestamp_nanos();
/// // Returns something like 1737816000000000000
/// ```
pub fn unix_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
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

    #[test]
    fn test_unix_timestamp_secs() {
        let timestamp = unix_timestamp_secs();
        // Should be a reasonable timestamp (after 2020-01-01 which is ~1577836800)
        assert!(timestamp > 1577836800);
        // Should be before 2100-01-01 (which is ~4102444800)
        assert!(timestamp < 4102444800);
    }

    #[test]
    fn test_unix_timestamp_millis() {
        let timestamp = unix_timestamp_millis();
        // Should be a reasonable timestamp in milliseconds
        assert!(timestamp > 1577836800000);
        assert!(timestamp < 4102444800000);
    }

    #[test]
    fn test_unix_timestamp_micros() {
        let timestamp = unix_timestamp_micros();
        // Should be a reasonable timestamp in microseconds
        assert!(timestamp > 1577836800000000);
        assert!(timestamp < 4102444800000000);
    }

    #[test]
    fn test_unix_timestamp_micros_u128() {
        let timestamp = unix_timestamp_micros_u128();
        // Should be a reasonable timestamp in microseconds
        assert!(timestamp > 1577836800000000);
        assert!(timestamp < 4102444800000000);
    }

    #[test]
    fn test_unix_timestamp_nanos() {
        let timestamp = unix_timestamp_nanos();
        // Should be a reasonable timestamp in nanoseconds
        assert!(timestamp > 1577836800000000000);
        assert!(timestamp < 4102444800000000000);
    }

    #[test]
    fn test_timestamp_precision_relationships() {
        // Take snapshots close in time
        let secs = unix_timestamp_secs();
        let millis = unix_timestamp_millis();
        let micros = unix_timestamp_micros();
        let nanos = unix_timestamp_nanos();

        // Verify the relationships (allowing for small timing differences)
        assert!((millis / 1000).abs_diff(secs) <= 1);
        assert!((micros / 1_000_000).abs_diff(secs) <= 1);
        assert!((nanos / 1_000_000_000).abs_diff(secs as u128) <= 1);
    }
}
