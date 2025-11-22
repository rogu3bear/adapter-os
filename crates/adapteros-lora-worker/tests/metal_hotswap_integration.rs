//! Integration tests for Metal kernel and Hot-Swap system interaction
//!
//! These tests verify the integration between the hot-swap adapter system
//! and Metal/GPU backends, including VRAM tracking and integrity verification.
//!
//! Note: Some tests are limited because the current AdapterTable implementation
//! has a bug where `self.active` is not updated during swap operations.
//! This is tracked for future fix.

use adapteros_core::B3Hash;
use adapteros_lora_worker::{AdapterTable, GpuFingerprint, Stack, StackCheckpoint};

#[tokio::test]
async fn test_adapter_table_basic_preload_and_swap() {
    let table = AdapterTable::new();

    // Preload adapter
    let hash = B3Hash::hash(b"test_adapter");
    table
        .preload("test_adapter".to_string(), hash, 10)
        .await
        .expect("Preload should succeed");

    // Swap in adapter - returns VRAM delta and count
    let (delta, count) = table
        .swap(&["test_adapter".to_string()], &[])
        .await
        .expect("Swap should succeed");

    assert_eq!(delta, 10, "VRAM delta should be +10");
    assert_eq!(count, 1, "Should have swapped 1 adapter");

    // Verify stack hash is computed (deterministic)
    let stack_hash = table.compute_stack_hash();
    let stack_hash_2 = table.compute_stack_hash();
    assert_eq!(stack_hash, stack_hash_2, "Stack hash should be deterministic");
}

#[tokio::test]
async fn test_adapter_table_swap_returns_correct_counts() {
    let table = AdapterTable::new();

    // Preload two adapters
    let hash_a = B3Hash::hash(b"adapter_a");
    let hash_b = B3Hash::hash(b"adapter_b");

    table
        .preload("adapter_a".to_string(), hash_a, 10)
        .await
        .unwrap();
    table
        .preload("adapter_b".to_string(), hash_b, 20)
        .await
        .unwrap();

    // Swap in adapter_a
    let (delta, count) = table
        .swap(&["adapter_a".to_string()], &[])
        .await
        .unwrap();
    assert_eq!(delta, 10, "First swap VRAM delta should be 10");
    assert_eq!(count, 1, "First swap should add 1 adapter");

    // Swap in adapter_b (already staged from preload)
    let (delta2, count2) = table
        .swap(&["adapter_b".to_string()], &[])
        .await
        .unwrap();
    assert_eq!(delta2, 20, "Second swap VRAM delta should be 20");
    assert_eq!(count2, 1, "Second swap should add 1 adapter");
}

#[tokio::test]
async fn test_adapter_table_rollback() {
    let table = AdapterTable::new();

    // Setup: preload adapters
    let hash_orig = B3Hash::hash(b"original");

    table
        .preload("original".to_string(), hash_orig, 10)
        .await
        .unwrap();

    // Swap in original
    table
        .swap(&["original".to_string()], &[])
        .await
        .unwrap();

    let original_gen = table.current_stack();

    // Preload and swap to replacement
    let hash_repl = B3Hash::hash(b"replacement");
    table
        .preload("replacement".to_string(), hash_repl, 20)
        .await
        .unwrap();
    table
        .swap(&["replacement".to_string()], &[])
        .await
        .unwrap();

    // Rollback to original state
    table.rollback().await.unwrap();

    // Verify generation went back
    let after_rollback_gen = table.current_stack();
    assert_eq!(
        after_rollback_gen, original_gen,
        "Generation should match original after rollback"
    );
}

#[test]
fn test_stack_struct_properties() {
    // Test Stack struct properties
    let stack = Stack {
        generation: 42,
        active: std::collections::HashMap::new(),
    };

    assert_eq!(stack.generation, 42);
    assert!(stack.active.is_empty());
}

#[test]
fn test_stack_checkpoint_serialization() {
    // Test that StackCheckpoint can be serialized and deserialized
    let checkpoint = StackCheckpoint {
        timestamp: 1234567890,
        metadata_hash: B3Hash::hash(b"metadata"),
        cross_layer_hash: Some(B3Hash::hash(b"cross_layer")),
        gpu_fingerprints: vec![GpuFingerprint {
            adapter_id: "test_adapter".to_string(),
            buffer_bytes: 2048,
            checkpoint_hash: B3Hash::hash(b"buffer"),
        }],
        adapter_ids: vec!["test_adapter".to_string()],
    };

    let serialized = serde_json::to_string(&checkpoint).expect("Serialization should succeed");
    let deserialized: StackCheckpoint =
        serde_json::from_str(&serialized).expect("Deserialization should succeed");

    assert_eq!(deserialized.timestamp, checkpoint.timestamp);
    assert_eq!(deserialized.metadata_hash, checkpoint.metadata_hash);
    assert_eq!(deserialized.adapter_ids, checkpoint.adapter_ids);
    assert_eq!(
        deserialized.gpu_fingerprints.len(),
        checkpoint.gpu_fingerprints.len()
    );
}

#[test]
fn test_gpu_fingerprint_creation() {
    let fingerprint = GpuFingerprint {
        adapter_id: "test".to_string(),
        buffer_bytes: 4096,
        checkpoint_hash: B3Hash::hash(b"test_buffer"),
    };

    assert_eq!(fingerprint.adapter_id, "test");
    assert_eq!(fingerprint.buffer_bytes, 4096);
}

#[tokio::test]
async fn test_cross_layer_hash_computation() {
    let table = AdapterTable::new();

    // Create GPU fingerprints
    let gpu_fps = vec![
        GpuFingerprint {
            adapter_id: "adapter_0".to_string(),
            buffer_bytes: 1024,
            checkpoint_hash: B3Hash::hash(b"buffer_0"),
        },
        GpuFingerprint {
            adapter_id: "adapter_1".to_string(),
            buffer_bytes: 2048,
            checkpoint_hash: B3Hash::hash(b"buffer_1"),
        },
    ];

    // Cross-layer hash should be deterministic
    let hash1 = table.compute_cross_layer_hash(&gpu_fps);
    let hash2 = table.compute_cross_layer_hash(&gpu_fps);
    assert_eq!(hash1, hash2, "Cross-layer hash should be deterministic");

    // Different inputs should produce different hashes
    let different_fps = vec![GpuFingerprint {
        adapter_id: "different".to_string(),
        buffer_bytes: 512,
        checkpoint_hash: B3Hash::hash(b"different"),
    }];
    let hash3 = table.compute_cross_layer_hash(&different_fps);
    assert_ne!(hash1, hash3, "Different inputs should produce different hashes");
}

#[tokio::test]
async fn test_checkpoint_storage_and_retrieval() {
    let table = AdapterTable::new();

    // Create checkpoint without needing swap (checkpoint stores whatever's in active)
    let gpu_fp = GpuFingerprint {
        adapter_id: "test".to_string(),
        buffer_bytes: 1024,
        checkpoint_hash: B3Hash::hash(b"state"),
    };
    let checkpoint = table.create_checkpoint(vec![gpu_fp]);

    // Verify checkpoint was stored
    let checkpoints = table.get_checkpoints(10);
    assert_eq!(checkpoints.len(), 1, "Should have one checkpoint");
    assert_eq!(checkpoints[0].timestamp, checkpoint.timestamp);
}

#[tokio::test]
async fn test_checkpoint_limit() {
    let table = AdapterTable::with_checkpoint_limit(3);

    // Create 5 checkpoints
    for i in 0..5 {
        let gpu_fp = GpuFingerprint {
            adapter_id: format!("adapter_{}", i),
            buffer_bytes: i as u64 * 1024,
            checkpoint_hash: B3Hash::hash(format!("state_{}", i).as_bytes()),
        };
        table.create_checkpoint(vec![gpu_fp]);
    }

    // Should only have 3 checkpoints (the most recent)
    let checkpoints = table.get_checkpoints(100);
    assert_eq!(checkpoints.len(), 3, "Should respect checkpoint limit");
}

#[tokio::test]
async fn test_refcount_operations() {
    let table = AdapterTable::new();

    // Setup adapter
    let hash = B3Hash::hash(b"refcount_adapter");
    table
        .preload("refcount_adapter".to_string(), hash, 10)
        .await
        .unwrap();
    table
        .swap(&["refcount_adapter".to_string()], &[])
        .await
        .unwrap();

    // Increment refcount
    table.inc_ref("refcount_adapter").await;

    // Check refcount
    {
        let refcounts = table.refcounts().lock().await;
        let rc = refcounts
            .get("refcount_adapter")
            .map(|r| r.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0);
        assert_eq!(rc, 1, "Refcount should be 1 after increment");
    }

    // Decrement refcount
    let final_rc = table.dec_ref("refcount_adapter").await;
    assert_eq!(final_rc, 0, "Refcount should be 0 after decrement");
}
