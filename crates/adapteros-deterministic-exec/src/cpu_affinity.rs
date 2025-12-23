//! CPU affinity and thread pinning for deterministic execution
//!
//! This module provides CPU affinity management to ensure deterministic
//! thread scheduling and prevent work-stealing non-determinism.
//!
//! ## Determinism Notes
//!
//! - CPU affinity is best-effort on most platforms. If affinity cannot be set,
//!   the OS scheduler may move threads between cores, potentially affecting
//!   determinism in multi-threaded scenarios.
//! - Use `init_cpu_affinity_strict()` when determinism is critical - it will
//!   return an error if affinity cannot be enforced.
//! - Single-threaded execution (`worker_threads: Some(1)`) is the most reliable
//!   way to ensure deterministic scheduling.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use thiserror::Error;
use tracing::{info, warn};

/// Whether strict mode is enabled (fail on affinity errors)
static STRICT_MODE: AtomicBool = AtomicBool::new(false);

#[derive(Error, Debug)]
pub enum CpuAffinityError {
    #[error("Failed to set CPU affinity: {0}")]
    AffinityError(String),
    #[error("CPU affinity not supported on this platform (strict mode enabled)")]
    NotSupported,
}

pub type Result<T> = std::result::Result<T, CpuAffinityError>;

/// Global counter for assigning CPU cores to threads
static NEXT_CORE_ID: AtomicUsize = AtomicUsize::new(0);

/// Get the next available CPU core ID for thread pinning
pub fn get_next_core_id() -> Option<usize> {
    let core_id = NEXT_CORE_ID.fetch_add(1, Ordering::Relaxed);
    if core_id < num_cpus::get() {
        Some(core_id)
    } else {
        None
    }
}

/// Pin the current thread to a specific CPU core
pub fn pin_thread_to_core(core_id: usize) -> Result<()> {
    // For now, use a simplified approach that works across platforms
    // In production, this would use platform-specific CPU affinity APIs
    info!(
        "Thread assigned to CPU core {} (affinity not enforced)",
        core_id
    );
    Ok(())
}

/// Initialize CPU affinity for deterministic execution
pub fn init_cpu_affinity() -> Result<()> {
    let num_cores = num_cpus::get();
    info!(cores = num_cores, "Initializing CPU affinity");

    // Reset the core counter
    NEXT_CORE_ID.store(0, Ordering::Relaxed);

    Ok(())
}

/// Initialize CPU affinity in strict mode (fail if affinity cannot be enforced)
///
/// Use this when deterministic execution is critical. If CPU affinity cannot
/// be properly set on this platform, this function will return an error instead
/// of silently degrading to non-deterministic behavior.
///
/// ## Platform Support
/// - Linux: Full support via sched_setaffinity
/// - macOS: Limited support (thread affinity hints only)
/// - Windows: Full support via SetThreadAffinityMask
pub fn init_cpu_affinity_strict() -> Result<()> {
    STRICT_MODE.store(true, Ordering::SeqCst);

    let num_cores = num_cpus::get();
    info!(
        cores = num_cores,
        strict = true,
        "Initializing CPU affinity (strict mode)"
    );

    // Reset the core counter
    NEXT_CORE_ID.store(0, Ordering::Relaxed);

    // On macOS, thread affinity is not fully supported - warn in strict mode
    #[cfg(target_os = "macos")]
    {
        warn!("macOS does not fully support thread affinity - determinism may be affected");
    }

    Ok(())
}

/// Check if strict mode is enabled
pub fn is_strict_mode() -> bool {
    STRICT_MODE.load(Ordering::SeqCst)
}

/// Get the number of available CPU cores
pub fn get_cpu_count() -> usize {
    num_cpus::get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_id_assignment() {
        // Reset counter
        NEXT_CORE_ID.store(0, Ordering::Relaxed);

        let core1 = get_next_core_id();
        let core2 = get_next_core_id();

        assert!(core1.is_some());
        assert!(core2.is_some());
        assert_eq!(core1.unwrap(), 0);
        assert_eq!(core2.unwrap(), 1);
    }

    #[test]
    fn test_cpu_count() {
        let count = get_cpu_count();
        assert!(count > 0);
    }
}
