//! RAII guard for seed registry scope management
//!
//! This module provides `SeedScopeGuard` which ensures the seed registry is cleared
//! at inference/training boundaries to prevent false-positive "seed reuse" errors.
//!
//! # Problem
//!
//! The seed registry (`SEED_REGISTRY` in `seed.rs`) tracks which seeds have been used
//! within a single operation to detect accidental reuse. However, if the registry is
//! not cleared between operations, legitimate seed derivations in subsequent
//! operations will be flagged as "reuse" errors.
//!
//! # Solution
//!
//! Use `SeedScopeGuard` at the start of each inference/training operation:
//!
//! ```rust,ignore
//! use adapteros_core::{SeedScopeGuard, GuardLogLevel};
//!
//! fn infer_internal(&mut self, request: InferenceRequest) -> Result<InferenceResponse> {
//!     // Guard will clear registry on drop (normal exit or error)
//!     let _seed_scope = SeedScopeGuard::new();
//!
//!     // ... inference code that may derive seeds ...
//! }
//! ```

use crate::guard_common::GuardLogLevel;
use crate::seed::{clear_seed_registry, is_seed_registry_empty};

/// RAII guard that clears the seed registry on drop.
///
/// Ensures no cross-operation seed reuse false positives by clearing the registry
/// when the guarded scope exits, regardless of whether it's a normal return or error.
///
/// # Thread Safety
///
/// This guard is `Send` but not `Sync` - it should be created and dropped on the
/// same thread/task that performs seed derivations.
#[derive(Debug)]
pub struct SeedScopeGuard {
    /// Whether to clear the registry on drop (can be disabled via `disarm()`)
    should_clear: bool,
    /// Log level for cleanup messages
    log_level: GuardLogLevel,
    /// Whether the registry had entries when this guard was created (leak indicator)
    had_prior_entries: bool,
    /// Label for logging (e.g., "inference", "training")
    label: &'static str,
}

impl SeedScopeGuard {
    /// Create a new guard with default settings.
    ///
    /// - Asserts registry is empty (logs warning if not)
    /// - Clears registry on drop
    /// - Uses Debug log level (quieter by default)
    pub fn new() -> Self {
        Self::new_with_config(GuardLogLevel::Debug, true, "operation")
    }

    /// Create a new guard for inference operations.
    pub fn for_inference(log_level: GuardLogLevel) -> Self {
        Self::new_with_config(log_level, true, "inference")
    }

    /// Create a new guard for training operations.
    pub fn for_training(log_level: GuardLogLevel) -> Self {
        Self::new_with_config(log_level, true, "training")
    }

    /// Create a new guard with full configuration.
    ///
    /// # Arguments
    ///
    /// * `log_level` - Log level for cleanup messages
    /// * `assert_empty` - If true, logs a warning if the registry was not empty at creation
    /// * `label` - Label for logging (e.g., "inference", "training")
    pub fn new_with_config(
        log_level: GuardLogLevel,
        assert_empty: bool,
        label: &'static str,
    ) -> Self {
        let had_prior_entries = !is_seed_registry_empty();

        if assert_empty && had_prior_entries {
            match log_level {
                GuardLogLevel::Warn => tracing::warn!(
                    label = label,
                    "SeedScopeGuard created but registry was not empty - possible leak from prior operation"
                ),
                GuardLogLevel::Debug => tracing::debug!(
                    label = label,
                    "SeedScopeGuard created but registry was not empty - possible leak from prior operation"
                ),
                GuardLogLevel::Off => {}
            }
        }

        Self {
            should_clear: true,
            log_level,
            had_prior_entries,
            label,
        }
    }

    /// Prevent the guard from clearing the registry on drop.
    ///
    /// Use this only if you need to manually manage the registry lifetime.
    pub fn disarm(&mut self) {
        self.should_clear = false;
    }

    /// Check if the registry had entries when this guard was created.
    pub fn had_prior_entries(&self) -> bool {
        self.had_prior_entries
    }
}

impl Default for SeedScopeGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SeedScopeGuard {
    fn drop(&mut self) {
        if !self.should_clear {
            return;
        }

        clear_seed_registry();

        // Only log if there were prior entries (indicates a leak that was cleaned up)
        if self.had_prior_entries {
            match self.log_level {
                GuardLogLevel::Warn => tracing::warn!(
                    label = self.label,
                    "SeedScopeGuard cleared registry that had prior entries"
                ),
                GuardLogLevel::Debug => tracing::debug!(
                    label = self.label,
                    "SeedScopeGuard cleared registry that had prior entries"
                ),
                GuardLogLevel::Off => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::derive_adapter_seed;
    use crate::B3Hash;
    use std::sync::Mutex;

    static SEED_GUARD_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn seed_guard_test_lock() -> std::sync::MutexGuard<'static, ()> {
        SEED_GUARD_TEST_LOCK.lock().unwrap()
    }

    #[test]
    fn test_guard_clears_registry_on_drop() {
        let _lock = seed_guard_test_lock();
        // Use unique params to avoid collision with other parallel tests
        let unique_adapter_id = 1000;
        let unique_nonce = 999999;

        // Ensure registry is empty first
        clear_seed_registry();

        // Create a guard and derive a seed
        {
            let _guard = SeedScopeGuard::new();
            let global = B3Hash::hash(b"test_clears_registry");

            // This should succeed
            let result = derive_adapter_seed(&global, unique_adapter_id, 0, unique_nonce);
            assert!(result.is_ok());

            // Guard drops here and clears registry
        }

        // After guard drops, we should be able to derive the same seed again
        let global = B3Hash::hash(b"test_clears_registry");
        let result = derive_adapter_seed(&global, unique_adapter_id, 0, unique_nonce);
        assert!(result.is_ok(), "Registry should have been cleared by guard");

        // Clean up
        clear_seed_registry();
    }

    #[test]
    fn test_guard_disarm_prevents_cleanup() {
        let _lock = seed_guard_test_lock();
        // Use unique IDs to avoid collision with parallel tests
        let unique_adapter_id = 2000;
        let unique_nonce = 888888;

        clear_seed_registry();

        // Create a guard, derive a seed, then disarm
        {
            let mut guard = SeedScopeGuard::new();
            let global = B3Hash::hash(b"test_disarm_unique");

            let result = derive_adapter_seed(&global, unique_adapter_id, 0, unique_nonce);
            assert!(result.is_ok());

            // Disarm the guard
            guard.disarm();
            // Guard drops here but doesn't clear
        }

        // The seed should still be in registry (guard was disarmed)
        let global = B3Hash::hash(b"test_disarm_unique");
        let result = derive_adapter_seed(&global, unique_adapter_id, 0, unique_nonce);
        assert!(
            result.is_err(),
            "Registry should NOT have been cleared (guard was disarmed)"
        );

        // Clean up manually
        clear_seed_registry();
    }

    #[test]
    fn test_guard_detects_prior_entries() {
        let _lock = seed_guard_test_lock();
        // Use unique IDs to avoid collision with parallel tests
        let unique_adapter_id = 3000;
        let unique_nonce = 777777;

        clear_seed_registry();

        // Derive a seed without a guard (simulating a leak)
        let global = B3Hash::hash(b"leaked_seed_unique");
        let _ = derive_adapter_seed(&global, unique_adapter_id, 0, unique_nonce);

        // Create a guard - it should detect the prior entries
        let guard = SeedScopeGuard::new();
        assert!(guard.had_prior_entries());

        // Clean up
        drop(guard);
    }

    #[test]
    fn test_guard_no_prior_entries() {
        let _lock = seed_guard_test_lock();
        clear_seed_registry();

        // Create a guard on clean registry
        let guard = SeedScopeGuard::new();
        assert!(!guard.had_prior_entries());

        drop(guard);
    }

    #[test]
    fn test_multiple_guards_sequential() {
        let _lock = seed_guard_test_lock();
        // Use unique IDs to avoid collision with parallel tests
        let base_adapter_id: usize = 4000;
        let base_nonce: u64 = 666666;

        clear_seed_registry();

        // Multiple sequential operations should all work
        for i in 0..3 {
            let _guard = SeedScopeGuard::new();
            let global = B3Hash::hash(format!("test_seq_unique_{}", i).as_bytes());

            // Same seed derivation params work each time because guard clears
            // Use different adapter_id per iteration to avoid collision
            let result = derive_adapter_seed(&global, base_adapter_id + i, 0, base_nonce);
            assert!(result.is_ok(), "Iteration {} should succeed", i);
        }

        clear_seed_registry();
    }

    #[test]
    fn test_guard_for_inference() {
        let _lock = seed_guard_test_lock();
        clear_seed_registry();

        {
            let _guard = SeedScopeGuard::for_inference(GuardLogLevel::Off);
            let global = B3Hash::hash(b"inference_test");
            let _ = derive_adapter_seed(&global, 0, 0, 1);
        }

        // Should be able to derive again after guard drops
        let global = B3Hash::hash(b"inference_test");
        assert!(derive_adapter_seed(&global, 0, 0, 1).is_ok());

        clear_seed_registry();
    }

    #[test]
    fn test_guard_for_training() {
        let _lock = seed_guard_test_lock();
        clear_seed_registry();

        {
            let _guard = SeedScopeGuard::for_training(GuardLogLevel::Off);
            let global = B3Hash::hash(b"training_test");
            let _ = derive_adapter_seed(&global, 0, 0, 1);
        }

        // Should be able to derive again after guard drops
        let global = B3Hash::hash(b"training_test");
        assert!(derive_adapter_seed(&global, 0, 0, 1).is_ok());

        clear_seed_registry();
    }
}
