//! KV cache quota and residency management
//!
//! Defines constants and policies for managing KV cache entries between
//! HOT (non-purgeable) and COLD (purgeable) states on Metal backends.

use std::time::Duration;

/// Number of accesses required to promote a COLD entry to HOT
///
/// Entries that are accessed frequently (>= this threshold) are promoted
/// to HOT status and marked non-purgeable to prevent OS memory reclamation.
pub const HOT_PROMOTION_THRESHOLD: usize = 3;

/// Time window for recency-based HOT promotion
///
/// Entries accessed within this time window are considered recent and
/// may be promoted to HOT status even if they haven't reached the
/// access count threshold.
pub const HOT_RECENCY_WINDOW: Duration = Duration::from_secs(60);

/// Idle time before a HOT entry can be demoted to COLD
///
/// HOT entries that haven't been accessed for this duration may be
/// demoted to COLD status and marked purgeable.
pub const COLD_DEMOTION_IDLE_TIME: Duration = Duration::from_secs(120);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(HOT_PROMOTION_THRESHOLD, 3);
        assert_eq!(HOT_RECENCY_WINDOW, Duration::from_secs(60));
        assert_eq!(COLD_DEMOTION_IDLE_TIME, Duration::from_secs(120));
    }
}
