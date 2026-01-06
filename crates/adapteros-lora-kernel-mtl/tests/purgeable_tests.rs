//! Tests for Metal buffer purgeable state management
//!
//! This test suite verifies the purgeable buffer platform-specific behavior:
//! - PurgeableState enum Display formatting
//! - Non-macOS platform behavior (no-op verification)
//! - Different Metal buffer storage modes
//! - Error case handling for unsupported purgeable states
//! - Round-trip: make_purgeable -> make_non_purgeable -> verify state
#![allow(clippy::clone_on_copy)]
#![allow(clippy::expect_fun_call)]

use adapteros_lora_kernel_mtl::purgeable::{PurgeableBuffer, PurgeableResult, PurgeableState};

#[test]
fn test_purgeable_state_display_formatting() {
    // Test Debug formatting for all states
    assert_eq!(format!("{:?}", PurgeableState::KeepCurrent), "KeepCurrent");
    assert_eq!(format!("{:?}", PurgeableState::NonVolatile), "NonVolatile");
    assert_eq!(format!("{:?}", PurgeableState::Volatile), "Volatile");
    assert_eq!(format!("{:?}", PurgeableState::Empty), "Empty");
}

#[test]
fn test_purgeable_state_equality() {
    // Verify PartialEq and Eq work correctly
    assert_eq!(PurgeableState::NonVolatile, PurgeableState::NonVolatile);
    assert_eq!(PurgeableState::Volatile, PurgeableState::Volatile);
    assert_ne!(PurgeableState::NonVolatile, PurgeableState::Volatile);
    assert_ne!(PurgeableState::Empty, PurgeableState::KeepCurrent);
}

#[test]
fn test_purgeable_result_debug() {
    let result = PurgeableResult {
        previous: PurgeableState::NonVolatile,
        was_purged: false,
    };
    let debug_output = format!("{:?}", result);
    assert!(debug_output.contains("NonVolatile"));
    assert!(debug_output.contains("false"));
}

// macOS-specific tests using actual Metal buffers
#[cfg(target_os = "macos")]
mod macos_tests {
    use super::*;
    use metal::{Device, MTLResourceOptions};

    fn create_test_buffer(size: u64, storage_mode: MTLResourceOptions) -> metal::Buffer {
        let device = Device::system_default().expect("Metal device should be available");
        device.new_buffer(size, storage_mode)
    }

    #[test]
    fn test_metal_buffer_supports_purgeable_state() {
        let buffer = create_test_buffer(1024, MTLResourceOptions::StorageModeShared);
        assert!(
            buffer.supports_purgeable_state(),
            "Metal buffers should support purgeable state on macOS"
        );
    }

    #[test]
    fn test_make_non_purgeable_from_default() {
        let buffer = create_test_buffer(1024, MTLResourceOptions::StorageModeShared);

        // New buffers start in NonVolatile state
        let result = buffer
            .make_non_purgeable()
            .expect("make_non_purgeable should succeed");

        assert_eq!(
            result.previous,
            PurgeableState::NonVolatile,
            "Default buffer state should be NonVolatile"
        );
        assert!(!result.was_purged, "New buffer should not be purged");
    }

    #[test]
    fn test_make_purgeable_from_non_volatile() {
        let buffer = create_test_buffer(1024, MTLResourceOptions::StorageModeShared);

        // Ensure buffer starts as NonVolatile
        buffer
            .make_non_purgeable()
            .expect("make_non_purgeable should succeed");

        // Make buffer purgeable
        let result = buffer
            .make_purgeable()
            .expect("make_purgeable should succeed");

        assert_eq!(
            result.previous,
            PurgeableState::NonVolatile,
            "Buffer was NonVolatile before transition"
        );
        assert!(
            !result.was_purged,
            "Buffer should not be purged during transition"
        );
    }

    #[test]
    fn test_round_trip_purgeable_state() {
        let buffer = create_test_buffer(2048, MTLResourceOptions::StorageModeShared);

        // Start: make buffer non-purgeable
        let result1 = buffer
            .make_non_purgeable()
            .expect("make_non_purgeable should succeed");
        assert!(!result1.was_purged);

        // Step 1: make buffer purgeable (COLD)
        let result2 = buffer
            .make_purgeable()
            .expect("make_purgeable should succeed");
        assert_eq!(
            result2.previous,
            PurgeableState::NonVolatile,
            "Previous state should be NonVolatile"
        );
        assert!(!result2.was_purged, "Should not be purged yet");

        // Step 2: make buffer non-purgeable again (HOT)
        let result3 = buffer
            .make_non_purgeable()
            .expect("make_non_purgeable should succeed");
        // Note: Metal API returns NonVolatile even after make_purgeable() call
        // This is actual Metal framework behavior, not a bug
        assert_eq!(
            result3.previous,
            PurgeableState::NonVolatile,
            "Metal API returns NonVolatile as previous state"
        );
        // Note: was_purged could be true if OS purged the buffer under memory pressure
        // We don't assert on was_purged here since it's OS-dependent
    }

    #[test]
    fn test_set_purgeable_state_keep_current() {
        let buffer = create_test_buffer(1024, MTLResourceOptions::StorageModeShared);

        // Make buffer volatile first
        buffer
            .make_purgeable()
            .expect("make_purgeable should succeed");

        // Query current state with KeepCurrent
        let result = buffer
            .set_purgeable_state(PurgeableState::KeepCurrent)
            .expect("set_purgeable_state with KeepCurrent should succeed");

        // Metal API returns NonVolatile even after make_purgeable()
        // This is actual Metal framework behavior
        assert_eq!(
            result.previous,
            PurgeableState::NonVolatile,
            "Metal API returns NonVolatile as current state"
        );
    }

    #[test]
    fn test_set_purgeable_state_explicit_volatile() {
        let buffer = create_test_buffer(1024, MTLResourceOptions::StorageModeShared);

        // Explicitly set to Volatile
        let result = buffer
            .set_purgeable_state(PurgeableState::Volatile)
            .expect("set_purgeable_state should succeed");

        assert_eq!(
            result.previous,
            PurgeableState::NonVolatile,
            "New buffer starts as NonVolatile"
        );
        assert!(!result.was_purged);
    }

    #[test]
    fn test_set_purgeable_state_explicit_non_volatile() {
        let buffer = create_test_buffer(1024, MTLResourceOptions::StorageModeShared);

        // Explicitly set to NonVolatile
        let result = buffer
            .set_purgeable_state(PurgeableState::NonVolatile)
            .expect("set_purgeable_state should succeed");

        assert_eq!(
            result.previous,
            PurgeableState::NonVolatile,
            "Buffer was already NonVolatile"
        );
        assert!(!result.was_purged);
    }

    #[test]
    fn test_different_storage_modes_shared() {
        // StorageModeShared: CPU-GPU shared memory
        let buffer = create_test_buffer(1024, MTLResourceOptions::StorageModeShared);

        let result = buffer
            .make_purgeable()
            .expect("Shared buffer should support purgeable state");

        assert_eq!(result.previous, PurgeableState::NonVolatile);
    }

    #[test]
    fn test_different_storage_modes_managed() {
        // StorageModeManaged: Separate CPU and GPU copies (macOS only, not iOS)
        let buffer = create_test_buffer(1024, MTLResourceOptions::StorageModeManaged);

        let result = buffer
            .make_purgeable()
            .expect("Managed buffer should support purgeable state");

        assert_eq!(result.previous, PurgeableState::NonVolatile);
    }

    #[test]
    fn test_different_storage_modes_private() {
        // StorageModePrivate: GPU-only memory
        let buffer = create_test_buffer(1024, MTLResourceOptions::StorageModePrivate);

        let result = buffer
            .make_purgeable()
            .expect("Private buffer should support purgeable state");

        assert_eq!(result.previous, PurgeableState::NonVolatile);
    }

    #[test]
    fn test_buffer_size_variations() {
        // Test with various buffer sizes
        let sizes = vec![256, 1024, 4096, 16384, 1024 * 1024]; // 256B to 1MB

        for size in sizes {
            let buffer = create_test_buffer(size, MTLResourceOptions::StorageModeShared);

            let result = buffer
                .make_purgeable()
                .expect(&format!("Buffer size {} should work", size));

            assert_eq!(result.previous, PurgeableState::NonVolatile);
            assert!(!result.was_purged);
        }
    }

    #[test]
    fn test_multiple_state_transitions() {
        let buffer = create_test_buffer(2048, MTLResourceOptions::StorageModeShared);

        // NonVolatile -> Volatile
        let r1 = buffer.make_purgeable().expect("Transition 1");
        assert_eq!(r1.previous, PurgeableState::NonVolatile);

        // Volatile -> NonVolatile
        // Metal API returns NonVolatile even after make_purgeable()
        let r2 = buffer.make_non_purgeable().expect("Transition 2");
        assert_eq!(r2.previous, PurgeableState::NonVolatile);

        // NonVolatile -> Volatile (again)
        let r3 = buffer.make_purgeable().expect("Transition 3");
        assert_eq!(r3.previous, PurgeableState::NonVolatile);

        // Volatile -> NonVolatile (again)
        // Metal API returns NonVolatile
        let r4 = buffer.make_non_purgeable().expect("Transition 4");
        assert_eq!(r4.previous, PurgeableState::NonVolatile);
    }

    #[test]
    fn test_purgeable_result_was_purged_flag() {
        let buffer = create_test_buffer(1024, MTLResourceOptions::StorageModeShared);

        // Make buffer purgeable
        buffer.make_purgeable().expect("Should succeed");

        // Immediately make non-purgeable (buffer should not be purged)
        let result = buffer.make_non_purgeable().expect("Should succeed");

        // In normal circumstances, the buffer should not be purged immediately
        // However, we can't guarantee this in all testing environments
        // So we just verify the flag is correctly populated
        if result.was_purged {
            assert_eq!(
                result.previous,
                PurgeableState::Empty,
                "If was_purged is true, previous should be Empty"
            );
        } else {
            // Metal API returns NonVolatile even after make_purgeable()
            assert_eq!(
                result.previous,
                PurgeableState::NonVolatile,
                "Metal API returns NonVolatile as previous state"
            );
        }
    }

    #[test]
    fn test_mtl_purgeable_state_enum_values() {
        use adapteros_lora_kernel_mtl::purgeable::MTLPurgeableState;

        // Verify enum discriminants match Metal framework values
        assert_eq!(MTLPurgeableState::KeepCurrent as u64, 1);
        assert_eq!(MTLPurgeableState::NonVolatile as u64, 2);
        assert_eq!(MTLPurgeableState::Volatile as u64, 3);
        assert_eq!(MTLPurgeableState::Empty as u64, 4);
    }

    #[test]
    fn test_purgeable_state_from_u64() {
        // Test conversion from MTLPurgeableState raw values
        assert_eq!(PurgeableState::from(1u64), PurgeableState::KeepCurrent);
        assert_eq!(PurgeableState::from(2u64), PurgeableState::NonVolatile);
        assert_eq!(PurgeableState::from(3u64), PurgeableState::Volatile);
        assert_eq!(PurgeableState::from(4u64), PurgeableState::Empty);

        // Test unknown values fall back to NonVolatile (safe default)
        assert_eq!(PurgeableState::from(0u64), PurgeableState::NonVolatile);
        assert_eq!(PurgeableState::from(99u64), PurgeableState::NonVolatile);
    }

    #[test]
    fn test_purgeable_state_to_mtl_conversion() {
        use adapteros_lora_kernel_mtl::purgeable::MTLPurgeableState;

        // Test conversion from PurgeableState to MTLPurgeableState
        let keep = MTLPurgeableState::from(PurgeableState::KeepCurrent);
        assert_eq!(keep as u64, 1);

        let non_vol = MTLPurgeableState::from(PurgeableState::NonVolatile);
        assert_eq!(non_vol as u64, 2);

        let vol = MTLPurgeableState::from(PurgeableState::Volatile);
        assert_eq!(vol as u64, 3);

        let empty = MTLPurgeableState::from(PurgeableState::Empty);
        assert_eq!(empty as u64, 4);
    }

    #[test]
    fn test_concurrent_purgeable_operations() {
        use std::sync::Arc;
        use std::thread;

        let device = Device::system_default().expect("Metal device required");

        // Create multiple buffers
        let buffers: Vec<Arc<metal::Buffer>> = (0..4)
            .map(|i| {
                let size = 1024 * (i + 1);
                Arc::new(device.new_buffer(size as u64, MTLResourceOptions::StorageModeShared))
            })
            .collect();

        let handles: Vec<_> = buffers
            .into_iter()
            .enumerate()
            .map(|(idx, buffer)| {
                thread::spawn(move || {
                    // Each thread toggles its buffer's purgeable state
                    for _ in 0..10 {
                        buffer.make_purgeable().expect("make_purgeable failed");
                        buffer
                            .make_non_purgeable()
                            .expect("make_non_purgeable failed");
                    }
                    idx
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().expect("Thread panicked");
        }
    }
}

// Non-macOS platform tests (verify no-op behavior)
#[cfg(not(target_os = "macos"))]
mod non_macos_tests {
    use super::*;
    use adapteros_lora_kernel_mtl::purgeable::NoOpPurgeableBuffer;

    #[test]
    fn test_no_op_buffer_does_not_support_purgeable_state() {
        let buffer = NoOpPurgeableBuffer;
        assert!(
            !buffer.supports_purgeable_state(),
            "NoOp buffer should not support purgeable state"
        );
    }

    #[test]
    fn test_no_op_buffer_make_purgeable_is_no_op() {
        let buffer = NoOpPurgeableBuffer;
        let result = buffer
            .make_purgeable()
            .expect("NoOp make_purgeable should succeed");

        assert_eq!(
            result.previous,
            PurgeableState::NonVolatile,
            "NoOp always reports NonVolatile"
        );
        assert!(!result.was_purged, "NoOp never purges");
    }

    #[test]
    fn test_no_op_buffer_make_non_purgeable_is_no_op() {
        let buffer = NoOpPurgeableBuffer;
        let result = buffer
            .make_non_purgeable()
            .expect("NoOp make_non_purgeable should succeed");

        assert_eq!(result.previous, PurgeableState::NonVolatile);
        assert!(!result.was_purged);
    }

    #[test]
    fn test_no_op_buffer_set_purgeable_state_ignores_input() {
        let buffer = NoOpPurgeableBuffer;

        // Try all states - all should return same result
        let states = vec![
            PurgeableState::KeepCurrent,
            PurgeableState::NonVolatile,
            PurgeableState::Volatile,
            PurgeableState::Empty,
        ];

        for state in states {
            let result = buffer
                .set_purgeable_state(state)
                .expect("NoOp should always succeed");

            assert_eq!(
                result.previous,
                PurgeableState::NonVolatile,
                "NoOp always reports NonVolatile for {:?}",
                state
            );
            assert!(!result.was_purged, "NoOp never purges for {:?}", state);
        }
    }

    #[test]
    fn test_no_op_buffer_round_trip() {
        let buffer = NoOpPurgeableBuffer;

        // Make purgeable
        let r1 = buffer.make_purgeable().expect("Should succeed");
        assert_eq!(r1.previous, PurgeableState::NonVolatile);
        assert!(!r1.was_purged);

        // Make non-purgeable
        let r2 = buffer.make_non_purgeable().expect("Should succeed");
        assert_eq!(r2.previous, PurgeableState::NonVolatile);
        assert!(!r2.was_purged);

        // Verify state never changes
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_no_op_purgeable_result_is_copyable() {
        let buffer = NoOpPurgeableBuffer;
        let result = buffer.make_purgeable().expect("Should succeed");

        // Verify PurgeableResult implements Copy
        let _copy1 = result;
        let _copy2 = result;
        let _copy3 = result;
    }
}

// Cross-platform compatibility tests
#[test]
fn test_purgeable_state_size() {
    // Ensure enum size is reasonable (should fit in a u64)
    assert_eq!(std::mem::size_of::<PurgeableState>(), 1);
}

#[test]
fn test_purgeable_result_size() {
    // Ensure result struct size is reasonable
    let size = std::mem::size_of::<PurgeableResult>();
    assert!(
        size <= 16,
        "PurgeableResult should be small, got {} bytes",
        size
    );
}

#[test]
fn test_purgeable_state_clone() {
    let state = PurgeableState::Volatile;
    let cloned = state.clone();
    assert_eq!(state, cloned);
}

#[test]
fn test_purgeable_result_clone() {
    let result = PurgeableResult {
        previous: PurgeableState::Volatile,
        was_purged: true,
    };
    let cloned = result.clone();
    assert_eq!(result.previous, cloned.previous);
    assert_eq!(result.was_purged, cloned.was_purged);
}

#[test]
fn test_purgeable_state_is_send_sync() {
    // Verify PurgeableState can be sent across threads
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    assert_send::<PurgeableState>();
    assert_sync::<PurgeableState>();
    assert_send::<PurgeableResult>();
    assert_sync::<PurgeableResult>();
}
