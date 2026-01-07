//! Boot-time invariant metrics infrastructure.
//!
//! This module provides static counters for tracking boot-time invariant checks.
//! These counters are updated during the boot process by the invariants module
//! in adapteros-server, and can be read at any time by the health endpoint.
//!
//! # Design
//!
//! The metrics are stored in static atomics to:
//! 1. Allow updates from the boot process (which runs before AppState exists)
//! 2. Allow reads from HTTP handlers after boot completes
//! 3. Avoid circular dependencies between adapteros-server and adapteros-server-api

use std::sync::atomic::{AtomicU64, Ordering};

// ============================================================================
// Boot-time metrics (captured before MetricsExporter is available)
// ============================================================================

/// Counter for invariant checks performed at boot
static BOOT_INVARIANTS_CHECKED: AtomicU64 = AtomicU64::new(0);
/// Counter for invariant violations detected at boot
static BOOT_INVARIANTS_VIOLATED: AtomicU64 = AtomicU64::new(0);
/// Counter for fatal violations that blocked boot
static BOOT_INVARIANTS_FATAL: AtomicU64 = AtomicU64::new(0);
/// Counter for invariant checks skipped via config escape hatch
static BOOT_INVARIANTS_SKIPPED: AtomicU64 = AtomicU64::new(0);

/// Snapshot of boot-time invariant metrics.
///
/// This is a read-only view of the invariant counters captured
/// at a point in time.
#[derive(Debug, Clone, Copy, Default)]
pub struct BootInvariantMetrics {
    /// Number of invariant checks performed
    pub checked: u64,
    /// Number of violations detected (fatal + non-fatal)
    pub violated: u64,
    /// Number of fatal violations that would block boot in production
    pub fatal: u64,
    /// Number of checks skipped via config escape hatch
    pub skipped: u64,
}

/// Get current boot invariant metrics snapshot.
///
/// This function reads the current values of all invariant counters
/// and returns them as a snapshot. It's safe to call from any thread.
pub fn boot_invariant_metrics() -> BootInvariantMetrics {
    BootInvariantMetrics {
        checked: BOOT_INVARIANTS_CHECKED.load(Ordering::Relaxed),
        violated: BOOT_INVARIANTS_VIOLATED.load(Ordering::Relaxed),
        fatal: BOOT_INVARIANTS_FATAL.load(Ordering::Relaxed),
        skipped: BOOT_INVARIANTS_SKIPPED.load(Ordering::Relaxed),
    }
}

/// Record an invariant check.
///
/// This should be called once for each invariant check performed,
/// regardless of whether it passed or failed.
pub fn record_invariant_check() {
    BOOT_INVARIANTS_CHECKED.fetch_add(1, Ordering::Relaxed);
}

/// Record an invariant violation.
///
/// # Arguments
///
/// * `fatal` - Whether this violation is fatal (would block boot in production)
pub fn record_invariant_violation(fatal: bool) {
    BOOT_INVARIANTS_VIOLATED.fetch_add(1, Ordering::Relaxed);
    if fatal {
        BOOT_INVARIANTS_FATAL.fetch_add(1, Ordering::Relaxed);
    }
}

/// Record a skipped invariant check.
///
/// This should be called when an invariant check is skipped due to
/// a config escape hatch.
pub fn record_invariant_skipped() {
    BOOT_INVARIANTS_SKIPPED.fetch_add(1, Ordering::Relaxed);
}

/// Reset all invariant metrics to zero.
///
/// This is primarily useful for testing to ensure a clean state.
#[cfg(test)]
pub fn reset_invariant_metrics() {
    BOOT_INVARIANTS_CHECKED.store(0, Ordering::Relaxed);
    BOOT_INVARIANTS_VIOLATED.store(0, Ordering::Relaxed);
    BOOT_INVARIANTS_FATAL.store(0, Ordering::Relaxed);
    BOOT_INVARIANTS_SKIPPED.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invariant_metrics_recording() {
        reset_invariant_metrics();

        // Record some checks
        record_invariant_check();
        record_invariant_check();
        record_invariant_check();

        // Record violations
        record_invariant_violation(false); // non-fatal
        record_invariant_violation(true); // fatal

        // Record skip
        record_invariant_skipped();

        let metrics = boot_invariant_metrics();
        assert_eq!(metrics.checked, 3);
        assert_eq!(metrics.violated, 2);
        assert_eq!(metrics.fatal, 1);
        assert_eq!(metrics.skipped, 1);
    }
}
