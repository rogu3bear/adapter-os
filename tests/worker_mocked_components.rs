#![cfg(all(test, feature = "extended-tests"))]

//! Integration tests for Worker mocked components
//!
//! Tests the real implementations of:
//! 1. Evidence retrieval with EmbeddingModel
//! 2. KV cache with Metal buffers
//! 3. Token embedding clarifications

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::MockKernels;
use adapteros_lora_rag::RagSystem;
use adapteros_lora_worker::{InferenceRequest, KvCache, SequenceId, Worker};
use adapteros_manifest::ManifestV3;
use adapteros_telemetry::TelemetryWriter;
use std::path::PathBuf;

/// Test KV cache allocation and deallocation
#[test]
fn test_kv_cache_lifecycle() {
    let mut cache = KvCache::new(10 * 1024 * 1024); // 10 MB

    // Test allocation
    let seq1 = cache.allocate(128).expect("Should allocate seq1");
    let seq2 = cache.allocate(256).expect("Should allocate seq2");
    let seq3 = cache.allocate(512).expect("Should allocate seq3");

    assert_eq!(cache.active_sequences(), 3);
    assert!(cache.is_allocated(seq1));
    assert!(cache.is_allocated(seq2));
    assert!(cache.is_allocated(seq3));

    // Test usage tracking
    let (used, capacity) = cache.usage();
    assert!(used > 0);
    assert_eq!(capacity, 10 * 1024 * 1024);
    assert!(cache.usage_percent() > 0.0);
    assert!(cache.usage_percent() < 100.0);

    // Test deallocation
    cache.free(seq2).expect("Should free seq2");
    assert_eq!(cache.active_sequences(), 2);
    assert!(!cache.is_allocated(seq2));

    // Test zeroize
    cache.zeroize();
    assert_eq!(cache.active_sequences(), 0);
    assert_eq!(cache.used_bytes, 0);
}

/// Test KV cache OOM handling
#[test]
fn test_kv_cache_oom() {
    let mut cache = KvCache::new(1024); // Very small: 1 KB

    // This should fail due to insufficient capacity
    let result = cache.allocate(1024);
    assert!(result.is_err());

    match result {
        Err(AosError::MemoryPressure(_)) => {
            // Expected error type
        }
        _ => panic!("Expected MemoryPressure error"),
    }
}

/// Test KV cache sequence zeroization for security
#[test]
fn test_kv_cache_zeroize_sequence() {
    let mut cache = KvCache::new(10 * 1024 * 1024);

    let seq1 = cache.allocate(64).expect("Should allocate seq1");
    let seq2 = cache.allocate(128).expect("Should allocate seq2");

    assert_eq!(cache.active_sequences(), 2);

    // Zeroize specific sequence
    cache.zeroize_sequence(seq1).expect("Should zeroize seq1");

    assert_eq!(cache.active_sequences(), 1);
    assert!(!cache.is_allocated(seq1));
    assert!(cache.is_allocated(seq2));
}

/// Test KV cache allocation info retrieval
#[test]
fn test_kv_cache_allocation_info() {
    let mut cache = KvCache::new(10 * 1024 * 1024);

    let seq_id = cache.allocate(128).expect("Should allocate");

    // Get allocation info
    let allocation = cache.get_allocation(seq_id);
    assert!(allocation.is_some());

    let (k_offset, k_size, v_offset, v_size) = allocation.unwrap();
    assert_eq!(k_offset, 0); // First allocation starts at 0
    assert!(k_size > 0);
    assert_eq!(v_offset, k_offset + k_size); // V follows K
    assert_eq!(v_size, k_size); // K and V same size
}

/// Test KV cache integration with memory monitoring
#[test]
fn test_kv_cache_memory_pressure() {
    let mut cache = KvCache::new(100 * 1024); // 100 KB - small capacity

    // Allocate until we hit capacity
    let mut sequences = Vec::new();
    loop {
        match cache.allocate(128) {
            Ok(seq_id) => {
                sequences.push(seq_id);
            }
            Err(AosError::MemoryPressure(_)) => {
                break; // Expected
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    // Should have allocated several sequences before hitting limit
    assert!(sequences.len() > 0);
    assert!(cache.usage_percent() > 90.0); // Should be nearly full

    // Free one sequence and verify we can allocate again
    let freed_seq = sequences.pop().unwrap();
    cache.free(freed_seq).expect("Should free sequence");

    let new_seq = cache.allocate(64).expect("Should allocate after freeing");
    assert!(cache.is_allocated(new_seq));
}

/// Benchmark: KV cache allocation performance
#[test]
fn bench_kv_cache_allocation() {
    let mut cache = KvCache::new(1024 * 1024 * 1024); // 1 GB

    let start = std::time::Instant::now();
    let iterations = 1000;

    let mut seq_ids = Vec::new();
    for _ in 0..iterations {
        let seq_id = cache.allocate(128).expect("Should allocate");
        seq_ids.push(seq_id);
    }

    let alloc_duration = start.elapsed();

    let start = std::time::Instant::now();
    for seq_id in seq_ids {
        cache.free(seq_id).expect("Should free");
    }
    let free_duration = start.elapsed();

    println!("KV Cache Performance:");
    println!(
        "  Allocations: {} in {:?} ({:.2} µs/alloc)",
        iterations,
        alloc_duration,
        alloc_duration.as_micros() as f64 / iterations as f64
    );
    println!(
        "  Deallocations: {} in {:?} ({:.2} µs/free)",
        iterations,
        free_duration,
        free_duration.as_micros() as f64 / iterations as f64
    );

    // Performance assertions
    assert!(alloc_duration.as_millis() < 100); // Should be fast
    assert!(free_duration.as_millis() < 100);
}
