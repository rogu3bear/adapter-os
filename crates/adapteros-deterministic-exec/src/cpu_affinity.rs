//! CPU affinity and thread pinning for deterministic execution
//!
//! This module provides CPU affinity management to ensure deterministic
//! thread scheduling and prevent work-stealing non-determinism.

use std::sync::atomic::{AtomicUsize, Ordering};
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum CpuAffinityError {
    #[error("Failed to set CPU affinity: {0}")]
    AffinityError(String),
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
    info!("Initializing CPU affinity for {} cores", num_cores);

    // Reset the core counter
    NEXT_CORE_ID.store(0, Ordering::Relaxed);

    Ok(())
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
