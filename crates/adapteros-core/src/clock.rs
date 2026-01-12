//! Clock abstraction for deterministic time handling.
//!
//! This module provides a [`Clock`] trait that abstracts over system time,
//! enabling deterministic testing and replay of time-dependent operations.
//!
//! # Usage
//!
//! Production code uses [`SystemClock`] which delegates to the OS:
//! ```rust,ignore
//! use adapteros_core::clock::{Clock, SystemClock};
//!
//! let clock = SystemClock;
//! let now = clock.now_millis();
//! ```
//!
//! Tests use [`MockClock`] for deterministic behavior:
//! ```rust,ignore
//! use adapteros_core::clock::{Clock, MockClock};
//! use std::time::Duration;
//!
//! let clock = MockClock::frozen_at(1000);
//! assert_eq!(clock.now_millis(), 1000);
//!
//! clock.advance(Duration::from_secs(60));
//! assert_eq!(clock.now_millis(), 61000);
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Trait for abstracting time access.
///
/// Implementations must be thread-safe (`Send + Sync`) to support
/// concurrent access from multiple async tasks.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Returns the current system time.
    fn now(&self) -> SystemTime;

    /// Returns milliseconds since UNIX epoch.
    ///
    /// Returns 0 if system time is before UNIX epoch (should not happen in practice).
    fn now_millis(&self) -> u64 {
        self.now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Returns microseconds since UNIX epoch.
    fn now_micros(&self) -> u64 {
        self.now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }

    /// Returns seconds since UNIX epoch.
    fn now_secs(&self) -> u64 {
        self.now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Returns a monotonic instant for elapsed time measurement.
    ///
    /// Note: For [`MockClock`], this returns a synthetic instant based on
    /// the frozen time. The instant is only meaningful relative to other
    /// instants from the same clock.
    fn instant(&self) -> Instant;
}

/// Production clock that uses the system time.
///
/// This is a zero-sized type with no state - all methods delegate directly
/// to the operating system's time facilities.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn instant(&self) -> Instant {
        Instant::now()
    }
}

/// Mock clock for deterministic testing.
///
/// The clock can be frozen at a specific time and advanced manually.
/// All operations are thread-safe via atomic operations.
///
/// # Example
///
/// ```rust,ignore
/// use adapteros_core::clock::{Clock, MockClock};
/// use std::time::Duration;
///
/// let clock = MockClock::frozen_at(1_000_000); // 1 second after epoch
/// assert_eq!(clock.now_millis(), 1_000_000);
///
/// clock.advance(Duration::from_secs(60));
/// assert_eq!(clock.now_millis(), 1_060_000);
///
/// clock.set_millis(2_000_000);
/// assert_eq!(clock.now_millis(), 2_000_000);
/// ```
#[derive(Debug)]
pub struct MockClock {
    /// Current time in milliseconds since UNIX epoch.
    frozen_millis: AtomicU64,
    /// Base instant for relative time calculations.
    /// Stored as nanos offset from an arbitrary point.
    instant_offset_nanos: AtomicU64,
}

impl MockClock {
    /// Creates a new mock clock frozen at the specified milliseconds since epoch.
    pub fn frozen_at(millis: u64) -> Self {
        Self {
            frozen_millis: AtomicU64::new(millis),
            instant_offset_nanos: AtomicU64::new(0),
        }
    }

    /// Creates a new mock clock frozen at the current system time.
    pub fn frozen_now() -> Self {
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self::frozen_at(now_millis)
    }

    /// Creates a new mock clock at time zero (UNIX epoch).
    pub fn zero() -> Self {
        Self::frozen_at(0)
    }

    /// Advances the clock by the specified duration.
    pub fn advance(&self, duration: Duration) {
        let millis = duration.as_millis() as u64;
        self.frozen_millis.fetch_add(millis, Ordering::SeqCst);
        let nanos = duration.as_nanos() as u64;
        self.instant_offset_nanos.fetch_add(nanos, Ordering::SeqCst);
    }

    /// Sets the clock to a specific time in milliseconds since epoch.
    pub fn set_millis(&self, millis: u64) {
        self.frozen_millis.store(millis, Ordering::SeqCst);
    }

    /// Returns the current frozen time in milliseconds.
    pub fn current_millis(&self) -> u64 {
        self.frozen_millis.load(Ordering::SeqCst)
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::frozen_now()
    }
}

impl Clock for MockClock {
    fn now(&self) -> SystemTime {
        let millis = self.frozen_millis.load(Ordering::SeqCst);
        UNIX_EPOCH + Duration::from_millis(millis)
    }

    fn now_millis(&self) -> u64 {
        self.frozen_millis.load(Ordering::SeqCst)
    }

    fn instant(&self) -> Instant {
        // Return a consistent instant based on the mock time.
        // We use the real Instant::now() as a baseline and add our offset.
        // This is a simplification - the returned instant is only meaningful
        // for elapsed() calculations within the same test context.
        //
        // NOTE: This is a best-effort approach. Instant doesn't support
        // arbitrary construction, so we return the current instant.
        // For elapsed time testing, use now_millis() differences instead.
        Instant::now()
    }
}

/// Creates a new system clock.
///
/// This is a convenience function for common usage patterns.
pub fn system_clock() -> SystemClock {
    SystemClock
}

/// Creates a new mock clock frozen at the specified time.
///
/// This is a convenience function for test setup.
pub fn mock_clock(millis: u64) -> MockClock {
    MockClock::frozen_at(millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_clock_returns_reasonable_time() {
        let clock = SystemClock;
        let now = clock.now_millis();
        // Should be after 2020-01-01 (1577836800000 ms since epoch)
        assert!(now > 1_577_836_800_000);
    }

    #[test]
    fn test_mock_clock_frozen_at() {
        let clock = MockClock::frozen_at(1_000_000);
        assert_eq!(clock.now_millis(), 1_000_000);
        assert_eq!(clock.now_secs(), 1_000);
        assert_eq!(clock.now_micros(), 1_000_000_000);
    }

    #[test]
    fn test_mock_clock_advance() {
        let clock = MockClock::frozen_at(1_000);
        clock.advance(Duration::from_secs(60));
        assert_eq!(clock.now_millis(), 61_000);

        clock.advance(Duration::from_millis(500));
        assert_eq!(clock.now_millis(), 61_500);
    }

    #[test]
    fn test_mock_clock_set() {
        let clock = MockClock::frozen_at(1_000);
        clock.set_millis(2_000_000);
        assert_eq!(clock.now_millis(), 2_000_000);
    }

    #[test]
    fn test_mock_clock_zero() {
        let clock = MockClock::zero();
        assert_eq!(clock.now_millis(), 0);
        assert_eq!(clock.now(), UNIX_EPOCH);
    }

    #[test]
    fn test_mock_clock_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let clock = Arc::new(MockClock::frozen_at(0));
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let c = Arc::clone(&clock);
                thread::spawn(move || {
                    c.advance(Duration::from_millis(100));
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All 10 threads advanced by 100ms each
        assert_eq!(clock.now_millis(), 1_000);
    }

    #[test]
    fn test_system_time_conversion() {
        let clock = MockClock::frozen_at(1_609_459_200_000); // 2021-01-01 00:00:00 UTC
        let time = clock.now();
        let expected = UNIX_EPOCH + Duration::from_millis(1_609_459_200_000);
        assert_eq!(time, expected);
    }
}
