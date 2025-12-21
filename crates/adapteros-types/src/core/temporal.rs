//! Temporal types for timestamps and durations

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// ISO 8601 timestamp (always serialized as string at API boundary)
pub type Timestamp = String;

/// Convert a DateTime to an ISO 8601 timestamp string
pub fn to_timestamp(dt: DateTime<Utc>) -> Timestamp {
    dt.to_rfc3339()
}

/// Convert an ISO 8601 timestamp string to DateTime
pub fn from_timestamp(ts: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    DateTime::parse_from_rfc3339(ts).map(|dt| dt.with_timezone(&Utc))
}

/// Duration in milliseconds (for timeouts, intervals, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DurationMs(pub u64);

impl DurationMs {
    /// Create a new duration in milliseconds
    pub fn new(ms: u64) -> Self {
        Self(ms)
    }

    /// Get the duration in milliseconds
    pub fn as_millis(&self) -> u64 {
        self.0
    }

    /// Convert to standard library Duration
    pub fn as_std_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.0)
    }
}

impl From<u64> for DurationMs {
    fn from(ms: u64) -> Self {
        Self(ms)
    }
}

impl From<std::time::Duration> for DurationMs {
    fn from(duration: std::time::Duration) -> Self {
        Self(duration.as_millis() as u64)
    }
}
