#![no_main]

use adapteros_core::B3Hash;
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_lora_worker::model_handle_cache::{ModelHandle, ModelHandleCache};
use adapteros_lora_worker::model_key::{ModelCacheIdentity, ModelKey};
use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;

// Helper to generate a deterministic ModelKey from a small ID
fn make_key(id: u8, backend: BackendType) -> ModelKey {
    // Use fixed data based on ID to ensure collisions when ID matches
    let data = vec![id];
    let hash = B3Hash::hash(&data);
    let identity = ModelCacheIdentity::for_backend(backend);
    ModelKey::new(backend, hash, identity)
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    // Initialize cache with small random memory limit (100 - 10000 bytes)
    // Small limit forces frequent evictions
    let max_memory = u.int_in_range::<u64>(100..=10000).unwrap_or(1000);
    let cache = ModelHandleCache::new(max_memory);

    let num_ops = u.int_in_range::<usize>(10..=100).unwrap_or(20);

    for _ in 0..num_ops {
        let op_type = u.int_in_range::<u8>(0..=8).unwrap_or(0);
        let key_id = u.int_in_range::<u8>(0..=10).unwrap_or(0); // Small key space for collisions

        // Random backend type
        let backend_byte = u.int_in_range::<u8>(0..=3).unwrap_or(0);
        let backend = match backend_byte {
            0 => BackendType::Metal,
            1 => BackendType::Mlx,
            2 => BackendType::CoreML,
            _ => BackendType::Mock,
        };

        let key = make_key(key_id, backend);

        match op_type {
            0..=2 => {
                // Load Model (30% chance)
                // Random size: 10 to max_memory/2
                let size = u.int_in_range::<u64>(10..=max_memory / 2).unwrap_or(50);

                // We mock the loader
                let _ = cache.get_or_load(&key, || {
                    Ok((ModelHandle::Metal(Arc::new(vec![0; size as usize])), size))
                });
            }
            3 => {
                // Load Base Model (Pinning) (10% chance)
                let size = u.int_in_range::<u64>(10..=max_memory / 2).unwrap_or(50);
                let _ = cache.get_or_load_base_model(&key, || {
                    Ok((ModelHandle::Metal(Arc::new(vec![0; size as usize])), size))
                });
            }
            4 => {
                // Pin (10% chance)
                cache.pin(&key);
            }
            5 => {
                // Unpin (10% chance)
                cache.unpin(&key);
            }
            6 => {
                // Mark Active (10% chance)
                cache.mark_active(&key);
            }
            7 => {
                // Mark Inactive (10% chance)
                cache.mark_inactive(&key);
            }
            8 => {
                // Cleanup / Unpin All (rare)
                if u.ratio(1, 10).unwrap_or(false) {
                    cache.cleanup_all();
                } else {
                    cache.unpin_all();
                }
            }
            _ => {}
        }

        // Invariants Check
        let stats = cache.stats();
        let current_mem = cache.memory_usage();
        let pinned_mem = cache.pinned_memory_bytes();
        let pinned_count = cache.pinned_count();

        assert_eq!(
            stats.total_memory_bytes, current_mem,
            "Stats memory must match actual memory"
        );

        // If we are over budget, it MUST be due to pinned or active entries (if implemented correctly)
        // Note: verifying active entries is hard from outside without iterating all keys,
        // but we can at least check pinned consistency.

        if pinned_count == 0 {
            assert_eq!(pinned_mem, 0, "Pinned memory should be 0 if count is 0");
        }

        // Check pinning consistency
        let pinned_keys = cache.pinned_keys();
        assert_eq!(
            pinned_keys.len(),
            pinned_count,
            "Pinned keys list length must match count"
        );
        for pk in pinned_keys {
            assert!(cache.is_pinned(&pk), "Pinned key must report is_pinned");
        }
    }

    // Final cleanup
    cache.cleanup_all();
    assert_eq!(cache.len(), 0, "Cache should be empty after cleanup_all");
    assert_eq!(
        cache.memory_usage(),
        0,
        "Memory should be 0 after cleanup_all"
    );
});
