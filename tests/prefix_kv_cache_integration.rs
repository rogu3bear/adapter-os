//! Integration tests for Prefix KV Cache functionality
//!
//! These tests verify the PRD requirements for PrefixKvCache v1:
//! - Prefix KV key binds model cache identity v2 and tokenizer hash
//! - Single-flight deduplication under concurrent misses
//! - Cache miss does not poison cache on error
//! - Receipt fields include prefix cache metrics

use adapteros_core::B3Hash;
use adapteros_lora_worker::{
    model_key::{FusionMode, ModelCacheIdentityV2, QuantizationMode},
    prefix_kv_cache::PrefixKvCache,
    prefix_kv_cache::PrefixKvCacheStats,
    prefix_kv_cache::PrefixKvEntry,
};

// =============================================================================
// Test: Prefix KV key binds model_cache_identity_v2 and tokenizer_hash
// =============================================================================

#[test]
fn test_prefix_kv_key_binds_model_cache_identity_v2_and_tokenizer_hash() {
    use adapteros_core::prefix_kv_key::compute_prefix_kv_key;

    let context_digest = B3Hash::hash(b"test-context");
    let prefix_tokens = vec![1u32, 2, 3, 4, 5];
    let tokenizer_manifest = B3Hash::hash(b"tokenizer-manifest");

    // Create two different model cache identities
    let identity_v2_a = ModelCacheIdentityV2 {
        kernel_version_id: "kernel-v1".to_string(),
        quantization_mode: QuantizationMode::Q4,
        fusion_mode: FusionMode::PerRequest,
        build_id: Some("build-123".to_string()),
        adapter_dir_hash: None,
        tokenizer_hash_b3: B3Hash::hash(b"tokenizer.json"),
        tokenizer_cfg_hash_b3: B3Hash::hash(b"tokenizer_config.json"),
        tenant_id: "test-tenant".to_string(),
        worker_id: 1,
    };

    let identity_v2_b = ModelCacheIdentityV2 {
        kernel_version_id: "kernel-v2".to_string(), // Different kernel
        quantization_mode: QuantizationMode::Q4,
        fusion_mode: FusionMode::PerRequest,
        build_id: Some("build-123".to_string()),
        adapter_dir_hash: None,
        tokenizer_hash_b3: B3Hash::hash(b"tokenizer.json"),
        tokenizer_cfg_hash_b3: B3Hash::hash(b"tokenizer_config.json"),
        tenant_id: "test-tenant".to_string(),
        worker_id: 1,
    };

    let key_a = compute_prefix_kv_key(
        &context_digest,
        &prefix_tokens,
        &tokenizer_manifest,
        &identity_v2_a.canonical_bytes(),
    );

    let key_b = compute_prefix_kv_key(
        &context_digest,
        &prefix_tokens,
        &tokenizer_manifest,
        &identity_v2_b.canonical_bytes(),
    );

    // Keys must differ when model identity changes
    assert_ne!(
        key_a, key_b,
        "Different model identities must produce different prefix KV keys"
    );

    // Same inputs must produce same key (determinism)
    let key_a_repeat = compute_prefix_kv_key(
        &context_digest,
        &prefix_tokens,
        &tokenizer_manifest,
        &identity_v2_a.canonical_bytes(),
    );

    assert_eq!(
        key_a, key_a_repeat,
        "Same inputs must produce identical prefix KV keys"
    );
}

#[test]
fn test_prefix_kv_key_changes_on_tokenizer_hash_change() {
    use adapteros_core::prefix_kv_key::compute_prefix_kv_key;

    let context_digest = B3Hash::hash(b"test-context");
    let prefix_tokens = vec![1u32, 2, 3];

    let identity = ModelCacheIdentityV2 {
        kernel_version_id: "kernel-v1".to_string(),
        quantization_mode: QuantizationMode::Q4,
        fusion_mode: FusionMode::PerRequest,
        build_id: None,
        adapter_dir_hash: None,
        tokenizer_hash_b3: B3Hash::hash(b"tokenizer.json"),
        tokenizer_cfg_hash_b3: B3Hash::hash(b"tokenizer_config.json"),
        tenant_id: "test-tenant".to_string(),
        worker_id: 1,
    };

    let tokenizer_manifest_a = B3Hash::hash(b"tokenizer-v1");
    let tokenizer_manifest_b = B3Hash::hash(b"tokenizer-v2");

    let key_a = compute_prefix_kv_key(
        &context_digest,
        &prefix_tokens,
        &tokenizer_manifest_a,
        &identity.canonical_bytes(),
    );

    let key_b = compute_prefix_kv_key(
        &context_digest,
        &prefix_tokens,
        &tokenizer_manifest_b,
        &identity.canonical_bytes(),
    );

    assert_ne!(
        key_a, key_b,
        "Different tokenizer manifests must produce different prefix KV keys"
    );
}

#[test]
fn test_prefix_kv_key_changes_on_context_digest_change() {
    use adapteros_core::prefix_kv_key::compute_prefix_kv_key;

    let context_a = B3Hash::hash(b"tenant-1-context");
    let context_b = B3Hash::hash(b"tenant-2-context");
    let prefix_tokens = vec![1u32, 2, 3];
    let tokenizer_manifest = B3Hash::hash(b"tokenizer");

    let identity = ModelCacheIdentityV2 {
        kernel_version_id: "kernel-v1".to_string(),
        quantization_mode: QuantizationMode::Q4,
        fusion_mode: FusionMode::PerRequest,
        build_id: None,
        adapter_dir_hash: None,
        tokenizer_hash_b3: B3Hash::hash(b"tokenizer.json"),
        tokenizer_cfg_hash_b3: B3Hash::hash(b"tokenizer_config.json"),
        tenant_id: "test-tenant".to_string(),
        worker_id: 1,
    };

    let key_a = compute_prefix_kv_key(
        &context_a,
        &prefix_tokens,
        &tokenizer_manifest,
        &identity.canonical_bytes(),
    );

    let key_b = compute_prefix_kv_key(
        &context_b,
        &prefix_tokens,
        &tokenizer_manifest,
        &identity.canonical_bytes(),
    );

    assert_ne!(
        key_a, key_b,
        "Different context digests must produce different prefix KV keys"
    );
}

// =============================================================================
// Test: Single-flight deduplication under concurrent misses
// =============================================================================

#[test]
fn test_prefix_kv_cache_singleflight_under_concurrent_misses() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::thread;

    let cache = Arc::new(PrefixKvCache::new(10 * 1024 * 1024)); // 10MB
    let key = B3Hash::hash(b"shared-prefix-key");
    let build_count = Arc::new(AtomicU32::new(0));
    let num_threads = 8;
    let barrier = Arc::new(Barrier::new(num_threads));

    // Spawn multiple threads that all try to get_or_build the same key
    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let cache = Arc::clone(&cache);
            let build_count = Arc::clone(&build_count);
            let barrier = Arc::clone(&barrier);

            thread::spawn(move || {
                // Synchronize all threads to start at the same time
                barrier.wait();

                cache.get_or_build(key, || {
                    // Simulate expensive build with sleep
                    thread::sleep(std::time::Duration::from_millis(50));
                    build_count.fetch_add(1, Ordering::SeqCst);

                    // Create a small entry
                    let keys = vec![vec![1.0f32; 64]; 2];
                    let values = vec![vec![2.0f32; 64]; 2];
                    Ok(PrefixKvEntry::new(
                        keys,
                        values,
                        "test-tenant".to_string(),
                        10,
                    ))
                })
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        let result = handle.join().unwrap();
        assert!(result.is_ok(), "All threads should succeed");
    }

    // Builder should have been called exactly once (single-flight)
    assert_eq!(
        build_count.load(Ordering::SeqCst),
        1,
        "Single-flight should deduplicate concurrent builds to exactly one"
    );

    // Entry should be in cache
    assert!(cache.get(&key).is_some(), "Entry should be cached");
}

// =============================================================================
// Test: Cache miss does not poison cache on error
// =============================================================================

#[test]
fn test_prefix_kv_cache_miss_does_not_poison_cache_on_error() {
    let cache = PrefixKvCache::new(1024 * 1024);
    let key = B3Hash::hash(b"error-key");

    // First attempt: builder returns error
    let result = cache.get_or_build(key, || {
        Err(adapteros_core::AosError::Validation(
            "simulated build failure".to_string(),
        ))
    });

    assert!(result.is_err(), "First attempt should fail");

    // Cache should NOT have a poisoned entry
    assert!(cache.get(&key).is_none(), "Failed build should not cache");
    assert_eq!(cache.len(), 0, "Cache should be empty after failed build");

    // Second attempt: builder succeeds
    let result = cache.get_or_build(key, || {
        let keys = vec![vec![1.0f32; 32]; 2];
        let values = vec![vec![2.0f32; 32]; 2];
        Ok(PrefixKvEntry::new(
            keys,
            values,
            "test-tenant".to_string(),
            5,
        ))
    });

    assert!(result.is_ok(), "Second attempt should succeed");
    assert!(cache.get(&key).is_some(), "Entry should now be cached");
    assert_eq!(cache.len(), 1, "Cache should have one entry");
}

// =============================================================================
// Test: LRU eviction by bytes
// =============================================================================

#[test]
fn test_prefix_kv_cache_lru_eviction() {
    // Small cache: 1200 bytes (fits ~1 entry of 1024 bytes)
    let cache = PrefixKvCache::new(1200);

    let key1 = B3Hash::hash(b"key1");
    let key2 = B3Hash::hash(b"key2");

    // Each entry: 2 layers * 64 floats * 4 bytes * 2 (K+V) = 1024 bytes
    let make_entry = |tenant: &str| {
        let keys = vec![vec![1.0f32; 64]; 2];
        let values = vec![vec![2.0f32; 64]; 2];
        PrefixKvEntry::new(keys, values, tenant.to_string(), 10)
    };

    // Insert first entry (1024 bytes) - should fit
    cache.insert(key1, make_entry("tenant1")).unwrap();
    assert_eq!(cache.len(), 1);
    assert!(cache.get(&key1).is_some(), "key1 should be present");

    // Insert second entry - should evict key1
    cache.insert(key2, make_entry("tenant2")).unwrap();

    // Cache should have 1 entry (key2)
    assert_eq!(cache.len(), 1);
    assert!(cache.get(&key1).is_none(), "key1 should be evicted");
    assert!(cache.get(&key2).is_some(), "key2 should be present");

    let stats = cache.stats();
    assert_eq!(stats.evictions, 1, "Should have exactly one eviction");
}

// =============================================================================
// Test: Entry refcount tracking
// =============================================================================

#[test]
fn test_prefix_kv_entry_refcount_tracking() {
    let keys = vec![vec![1.0f32; 32]; 2];
    let values = vec![vec![2.0f32; 32]; 2];
    let entry = PrefixKvEntry::new(keys, values, "tenant".to_string(), 5);

    // Initially not in use
    assert!(!entry.is_in_use(), "Entry should not be in use initially");

    // Acquire reference
    entry.acquire();
    assert!(entry.is_in_use(), "Entry should be in use after acquire");

    // Acquire another reference
    entry.acquire();
    assert!(entry.is_in_use(), "Entry should still be in use");

    // Release one reference
    entry.release();
    assert!(
        entry.is_in_use(),
        "Entry should still be in use after one release"
    );

    // Release final reference
    entry.release();
    assert!(
        !entry.is_in_use(),
        "Entry should not be in use after all releases"
    );
}

// =============================================================================
// Test: ModelCacheIdentityV2 canonical bytes determinism
// =============================================================================

#[test]
fn test_model_cache_identity_v2_canonical_bytes_deterministic() {
    let identity = ModelCacheIdentityV2 {
        kernel_version_id: "kernel-v1".to_string(),
        quantization_mode: QuantizationMode::Q4,
        fusion_mode: FusionMode::PerRequest,
        build_id: Some("build-123".to_string()),
        adapter_dir_hash: Some(B3Hash::hash(b"adapter-dir")),
        tokenizer_hash_b3: B3Hash::hash(b"tokenizer.json"),
        tokenizer_cfg_hash_b3: B3Hash::hash(b"tokenizer_config.json"),
        tenant_id: "test-tenant".to_string(),
        worker_id: 1,
    };

    let bytes1 = identity.canonical_bytes();
    let bytes2 = identity.canonical_bytes();

    assert_eq!(
        bytes1, bytes2,
        "canonical_bytes must be deterministic across calls"
    );

    // Verify digest is also deterministic
    let digest1 = identity.digest();
    let digest2 = identity.digest();

    assert_eq!(
        digest1, digest2,
        "digest must be deterministic across calls"
    );
}

#[test]
fn test_model_cache_identity_v2_different_fields_different_bytes() {
    let base = ModelCacheIdentityV2 {
        kernel_version_id: "kernel-v1".to_string(),
        quantization_mode: QuantizationMode::Q4,
        fusion_mode: FusionMode::PerRequest,
        build_id: None,
        adapter_dir_hash: None,
        tokenizer_hash_b3: B3Hash::hash(b"tokenizer.json"),
        tokenizer_cfg_hash_b3: B3Hash::hash(b"tokenizer_config.json"),
        tenant_id: "test-tenant".to_string(),
        worker_id: 1,
    };

    let different_kernel = ModelCacheIdentityV2 {
        kernel_version_id: "kernel-v2".to_string(),
        ..base.clone()
    };

    let different_quant = ModelCacheIdentityV2 {
        quantization_mode: QuantizationMode::Q8,
        ..base.clone()
    };

    let different_tokenizer = ModelCacheIdentityV2 {
        tokenizer_hash_b3: B3Hash::hash(b"different-tokenizer.json"),
        ..base.clone()
    };

    assert_ne!(
        base.canonical_bytes(),
        different_kernel.canonical_bytes(),
        "Different kernel versions must produce different bytes"
    );

    assert_ne!(
        base.canonical_bytes(),
        different_quant.canonical_bytes(),
        "Different quantization modes must produce different bytes"
    );

    assert_ne!(
        base.canonical_bytes(),
        different_tokenizer.canonical_bytes(),
        "Different tokenizer hashes must produce different bytes"
    );
}

// =============================================================================
// Test: Prefix token encoding
// =============================================================================

#[test]
fn test_prefix_token_encoding_deterministic() {
    use adapteros_core::prefix_kv_key::encode_prefix_tokens;

    let tokens = vec![100u32, 200, 300, 400, 500];

    let encoded1 = encode_prefix_tokens(&tokens);
    let encoded2 = encode_prefix_tokens(&tokens);

    assert_eq!(encoded1, encoded2, "Token encoding must be deterministic");

    // Verify format: 4 bytes count + 4 bytes per token
    assert_eq!(
        encoded1.len(),
        4 + tokens.len() * 4,
        "Encoded length should be 4 + 4*token_count"
    );
}

#[test]
fn test_prefix_token_encoding_different_tokens_different_output() {
    use adapteros_core::prefix_kv_key::encode_prefix_tokens;

    let tokens_a = vec![1u32, 2, 3];
    let tokens_b = vec![1u32, 2, 4]; // Different last token

    let encoded_a = encode_prefix_tokens(&tokens_a);
    let encoded_b = encode_prefix_tokens(&tokens_b);

    assert_ne!(
        encoded_a, encoded_b,
        "Different tokens must produce different encodings"
    );
}

// =============================================================================
// Test: Cache stats accuracy
// =============================================================================

#[test]
fn test_prefix_kv_cache_stats_accuracy() {
    let cache = PrefixKvCache::new(1024 * 1024);

    let initial_stats = cache.stats();
    assert_eq!(initial_stats.hits, 0);
    assert_eq!(initial_stats.misses, 0);
    assert_eq!(initial_stats.evictions, 0);
    assert_eq!(initial_stats.entry_count, 0);
    assert_eq!(initial_stats.used_bytes, 0);

    let key = B3Hash::hash(b"stats-test-key");

    // Miss on get
    assert!(cache.get(&key).is_none());
    assert_eq!(cache.stats().misses, 1);

    // Insert entry
    let keys = vec![vec![1.0f32; 32]; 2];
    let values = vec![vec![2.0f32; 32]; 2];
    let entry = PrefixKvEntry::new(keys, values, "tenant".to_string(), 10);
    let entry_bytes = entry.kv_bytes;

    cache.insert(key, entry).unwrap();

    let after_insert = cache.stats();
    assert_eq!(after_insert.entry_count, 1);
    assert_eq!(after_insert.used_bytes, entry_bytes);

    // Hit on get
    assert!(cache.get(&key).is_some());
    assert_eq!(cache.stats().hits, 1);

    // Another hit
    assert!(cache.get(&key).is_some());
    assert_eq!(cache.stats().hits, 2);

    // Remove entry
    cache.remove(&key);
    let after_remove = cache.stats();
    assert_eq!(after_remove.entry_count, 0);
    assert_eq!(after_remove.used_bytes, 0);
}

#[test]
fn test_prefix_kv_cache_stats_hit_rate() {
    let stats = PrefixKvCacheStats {
        hits: 80,
        misses: 20,
        evictions: 5,
        entry_count: 10,
        used_bytes: 1000,
        max_bytes: 10000,
        in_flight_builds: 0,
    };

    let hit_rate = stats.hit_rate_percent();
    assert!((hit_rate - 80.0).abs() < 0.01, "Hit rate should be 80%");

    // Test zero division case
    let empty_stats = PrefixKvCacheStats::default();
    assert_eq!(
        empty_stats.hit_rate_percent(),
        0.0,
        "Empty stats should return 0% hit rate"
    );
}

// =============================================================================
// Test: Database integration - prefix templates and receipts
// =============================================================================

#[tokio::test]
async fn test_prefix_template_crud_integration() {
    use adapteros_api_types::prefix_templates::{
        CreatePrefixTemplateRequest, PrefixMode, UpdatePrefixTemplateRequest,
    };
    use adapteros_db::Db;
    use std::sync::Arc;

    let db = Arc::new(Db::new_in_memory().await.expect("Failed to create test db"));

    // Create tenant first (FK constraint)
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')")
        .execute(db.pool())
        .await
        .expect("Failed to create tenant");

    // Create multiple prefix templates
    let system_tpl = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::System,
            template_text: "You are a helpful AI assistant.".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .expect("Failed to create system template");

    let user_tpl = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "User context prefix.".to_string(),
            priority: Some(20),
            enabled: Some(true),
        })
        .await
        .expect("Failed to create user template");

    // List templates - should be ordered by priority descending
    let templates = db
        .list_prefix_templates("tenant-1")
        .await
        .expect("Failed to list templates");
    assert_eq!(templates.len(), 2);
    assert_eq!(templates[0].priority, 20); // User template first (higher priority)
    assert_eq!(templates[1].priority, 10); // System template second

    // Get best match for mode
    let best_user = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::User)
        .await
        .expect("Failed to get template for mode")
        .expect("Should find user template");
    assert_eq!(best_user.id, user_tpl.id);
    assert_eq!(best_user.mode, PrefixMode::User);

    // Get mode with no specific template - should fall back to system
    let builder_template = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::Builder)
        .await
        .expect("Failed to get template for mode")
        .expect("Should fall back to system template");
    assert_eq!(builder_template.id, system_tpl.id);
    assert_eq!(builder_template.mode, PrefixMode::System);

    // Update template
    let updated = db
        .update_prefix_template(
            &user_tpl.id,
            UpdatePrefixTemplateRequest {
                mode: None,
                template_text: Some("Updated user prefix.".to_string()),
                priority: Some(30),
                enabled: None,
            },
        )
        .await
        .expect("Failed to update template")
        .expect("Template should exist");

    assert_eq!(updated.template_text, "Updated user prefix.");
    assert_eq!(updated.priority, 30);

    // Verify hash was recomputed
    let expected_hash = B3Hash::hash(b"Updated user prefix.");
    assert_eq!(updated.template_hash_b3, expected_hash);

    // Delete template
    let deleted = db
        .delete_prefix_template(&user_tpl.id)
        .await
        .expect("Failed to delete template");
    assert!(deleted, "Delete should return true");

    // Verify deletion
    let after_delete = db
        .get_prefix_template(&user_tpl.id)
        .await
        .expect("Failed to query template");
    assert!(after_delete.is_none(), "Template should be deleted");

    // List should now only have system template
    let templates = db
        .list_prefix_templates("tenant-1")
        .await
        .expect("Failed to list templates");
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].id, system_tpl.id);
}

#[tokio::test]
async fn test_prefix_template_tenant_isolation() {
    use adapteros_api_types::prefix_templates::{CreatePrefixTemplateRequest, PrefixMode};
    use adapteros_db::Db;
    use std::sync::Arc;

    let db = Arc::new(Db::new_in_memory().await.expect("Failed to create test db"));

    // Create two tenants
    sqlx::query(
        "INSERT INTO tenants (id, name) VALUES ('tenant-1', 'Tenant 1'), ('tenant-2', 'Tenant 2')",
    )
    .execute(db.pool())
    .await
    .expect("Failed to create tenants");

    // Create templates for each tenant
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Tenant 1 prefix".to_string(),
        priority: None,
        enabled: None,
    })
    .await
    .expect("Failed to create tenant 1 template");

    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-2".to_string(),
        mode: PrefixMode::User,
        template_text: "Tenant 2 prefix".to_string(),
        priority: None,
        enabled: None,
    })
    .await
    .expect("Failed to create tenant 2 template");

    // Verify tenant 1 only sees their template
    let tenant1_templates = db
        .list_prefix_templates("tenant-1")
        .await
        .expect("Failed to list tenant 1 templates");
    assert_eq!(tenant1_templates.len(), 1);
    assert_eq!(tenant1_templates[0].template_text, "Tenant 1 prefix");

    // Verify tenant 2 only sees their template
    let tenant2_templates = db
        .list_prefix_templates("tenant-2")
        .await
        .expect("Failed to list tenant 2 templates");
    assert_eq!(tenant2_templates.len(), 1);
    assert_eq!(tenant2_templates[0].template_text, "Tenant 2 prefix");

    // Verify cross-tenant lookup returns nothing
    let tenant1_mode = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::User)
        .await
        .expect("Failed to query")
        .expect("Should find template");
    assert_eq!(tenant1_mode.template_text, "Tenant 1 prefix");

    let tenant2_mode = db
        .get_prefix_template_for_mode("tenant-2", &PrefixMode::User)
        .await
        .expect("Failed to query")
        .expect("Should find template");
    assert_eq!(tenant2_mode.template_text, "Tenant 2 prefix");
}

#[tokio::test]
async fn test_prefix_kv_receipt_fields_persistence() {
    use adapteros_db::{
        Db, SqlTraceSink, TraceFinalization, TraceSink, TraceStart, TraceTokenInput,
    };
    use std::sync::Arc;

    let db = Arc::new(Db::new_in_memory().await.expect("Failed to create test db"));

    // Create tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')")
        .execute(db.pool())
        .await
        .expect("Failed to create tenant");

    let context_digest = B3Hash::hash(b"test-context").to_bytes();
    let trace_id = "trace-prefix-kv-test".to_string();

    let start = TraceStart {
        trace_id: trace_id.clone(),
        tenant_id: "tenant-1".to_string(),
        request_id: Some("req-prefix-1".to_string()),
        context_digest,
    };

    let mut sink = SqlTraceSink::new(db.clone(), start, 32)
        .await
        .expect("Failed to create trace sink");

    // Record some tokens
    sink.record_token(TraceTokenInput {
        token_index: 0,
        adapter_ids: vec!["adapter-a".into()],
        gates_q15: vec![100],
        policy_mask_digest_b3: None,
        allowed_mask: None,
        policy_overrides_applied: None,
        backend_id: Some("coreml".into()),
        kernel_version_id: Some("v1".into()),
    })
    .await
    .expect("Failed to record token");

    // Finalize with prefix KV cache fields populated
    let prefix_kv_key = B3Hash::hash(b"prefix-kv-key-test");
    let model_identity_digest = B3Hash::hash(b"model-identity-v2");

    let output_tokens = [10, 20, 30];
    let receipt = sink
        .finalize(TraceFinalization {
            output_tokens: &output_tokens,
            logical_prompt_tokens: 50,
            prefix_cached_token_count: 20,
            billed_input_tokens: 30,
            logical_output_tokens: output_tokens.len() as u32,
            billed_output_tokens: output_tokens.len() as u32,
            stop_reason_code: Some("eos".to_string()),
            stop_reason_token_index: Some(2),
            stop_policy_digest_b3: None,
            tenant_kv_quota_bytes: 10_000_000,
            tenant_kv_bytes_used: 5_000_000,
            kv_evictions: 2,
            kv_residency_policy_id: Some("policy-1".to_string()),
            kv_quota_enforced: true,
            prefix_kv_key_b3: Some(prefix_kv_key),
            prefix_cache_hit: true,
            prefix_kv_bytes: 4096,
            model_cache_identity_v2_digest_b3: Some(model_identity_digest),
            attestation: None,
            equipment_profile: None,
            // Phase 3: Crypto Receipt Dual-Write
            crypto_receipt_digest_b3: None,
            receipt_parity_verified: None,
            tenant_id: None,
        })
        .await
        .expect("Failed to finalize trace");

    // Verify receipt fields
    assert_eq!(receipt.prefix_cached_token_count, 20);

    // Query the database directly to verify persistence
    // Note: model_cache_identity_v2_digest_b3 is stored as BLOB, prefix_kv_key_b3 as TEXT (hex)
    let (
        stored_prefix_kv_key,
        stored_prefix_cache_hit,
        stored_prefix_kv_bytes,
        stored_model_identity,
    ): (Option<String>, i64, i64, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT prefix_kv_key_b3, prefix_cache_hit, prefix_kv_bytes, model_cache_identity_v2_digest_b3
         FROM inference_trace_receipts
         WHERE trace_id = ?",
    )
    .bind(&trace_id)
    .fetch_one(db.pool())
    .await
    .expect("Failed to query receipt");

    assert_eq!(
        stored_prefix_kv_key,
        Some(prefix_kv_key.to_hex()),
        "prefix_kv_key_b3 should be persisted"
    );
    assert_eq!(
        stored_prefix_cache_hit, 1,
        "prefix_cache_hit should be 1 (true)"
    );
    assert_eq!(
        stored_prefix_kv_bytes, 4096,
        "prefix_kv_bytes should be persisted"
    );
    // Convert BLOB to B3Hash for comparison
    let expected_model_identity_bytes = model_identity_digest.as_bytes().to_vec();
    assert_eq!(
        stored_model_identity,
        Some(expected_model_identity_bytes),
        "model_cache_identity_v2_digest_b3 should be persisted"
    );
}

// =============================================================================
// Test: Cache invalidation on template changes
// =============================================================================

#[test]
fn test_prefix_kv_cache_invalidation_on_template_change() {
    use adapteros_core::prefix_kv_key::compute_prefix_kv_key;
    use adapteros_lora_worker::model_key::{FusionMode, ModelCacheIdentityV2, QuantizationMode};

    let context_digest = B3Hash::hash(b"tenant-context");
    let tokenizer_manifest = B3Hash::hash(b"tokenizer-manifest");

    let identity = ModelCacheIdentityV2 {
        kernel_version_id: "kernel-v1".to_string(),
        quantization_mode: QuantizationMode::Q4,
        fusion_mode: FusionMode::PerRequest,
        build_id: None,
        adapter_dir_hash: None,
        tokenizer_hash_b3: B3Hash::hash(b"tokenizer.json"),
        tokenizer_cfg_hash_b3: B3Hash::hash(b"tokenizer_config.json"),
        tenant_id: "test-tenant".to_string(),
        worker_id: 1,
    };

    // Original template tokens
    let template_v1_tokens = vec![100u32, 200, 300];
    let key_v1 = compute_prefix_kv_key(
        &context_digest,
        &template_v1_tokens,
        &tokenizer_manifest,
        &identity.canonical_bytes(),
    );

    // Updated template tokens (different text)
    let template_v2_tokens = vec![100u32, 200, 301]; // Last token changed
    let key_v2 = compute_prefix_kv_key(
        &context_digest,
        &template_v2_tokens,
        &tokenizer_manifest,
        &identity.canonical_bytes(),
    );

    // Keys must differ - changing template text invalidates cache
    assert_ne!(
        key_v1, key_v2,
        "Changing template text must produce different cache key, invalidating old entries"
    );

    // Cache simulation
    let cache = PrefixKvCache::new(10 * 1024 * 1024);

    let make_entry = || {
        let keys = vec![vec![1.0f32; 64]; 2];
        let values = vec![vec![2.0f32; 64]; 2];
        PrefixKvEntry::new(keys, values, "test-tenant".to_string(), 10)
    };

    // Insert entry for v1
    cache.insert(key_v1, make_entry()).unwrap();
    assert!(cache.get(&key_v1).is_some(), "v1 entry should be cached");

    // After template update, v1 key is no longer valid
    assert!(
        cache.get(&key_v2).is_none(),
        "v2 key should miss (new template)"
    );

    // Insert entry for v2
    cache.insert(key_v2, make_entry()).unwrap();

    // Both keys can coexist (old entries age out via LRU)
    assert!(cache.get(&key_v1).is_some(), "v1 entry still in cache");
    assert!(cache.get(&key_v2).is_some(), "v2 entry now cached");
}

// =============================================================================
// Test: Tenant isolation in prefix cache
// =============================================================================

#[test]
fn test_prefix_kv_cache_tenant_isolation() {
    use adapteros_core::prefix_kv_key::compute_prefix_kv_key;
    use adapteros_lora_worker::model_key::{FusionMode, ModelCacheIdentityV2, QuantizationMode};

    let prefix_tokens = vec![100u32, 200, 300];
    let tokenizer_manifest = B3Hash::hash(b"tokenizer-manifest");

    // Two tenants with identical prefix text
    let tenant1_context = B3Hash::hash(b"tenant-1-context");
    let tenant2_context = B3Hash::hash(b"tenant-2-context");

    let identity_tenant1 = ModelCacheIdentityV2 {
        kernel_version_id: "kernel-v1".to_string(),
        quantization_mode: QuantizationMode::Q4,
        fusion_mode: FusionMode::PerRequest,
        build_id: None,
        adapter_dir_hash: None,
        tokenizer_hash_b3: B3Hash::hash(b"tokenizer.json"),
        tokenizer_cfg_hash_b3: B3Hash::hash(b"tokenizer_config.json"),
        tenant_id: "tenant-1".to_string(),
        worker_id: 1,
    };

    let identity_tenant2 = ModelCacheIdentityV2 {
        kernel_version_id: "kernel-v1".to_string(),
        quantization_mode: QuantizationMode::Q4,
        fusion_mode: FusionMode::PerRequest,
        build_id: None,
        adapter_dir_hash: None,
        tokenizer_hash_b3: B3Hash::hash(b"tokenizer.json"),
        tokenizer_cfg_hash_b3: B3Hash::hash(b"tokenizer_config.json"),
        tenant_id: "tenant-2".to_string(),
        worker_id: 1,
    };

    let key_tenant1 = compute_prefix_kv_key(
        &tenant1_context,
        &prefix_tokens,
        &tokenizer_manifest,
        &identity_tenant1.canonical_bytes(),
    );

    let key_tenant2 = compute_prefix_kv_key(
        &tenant2_context,
        &prefix_tokens,
        &tokenizer_manifest,
        &identity_tenant2.canonical_bytes(),
    );

    // Different tenants must have different cache keys
    assert_ne!(
        key_tenant1, key_tenant2,
        "Different tenants must have different prefix KV cache keys for isolation"
    );

    // Verify tenant ID is part of ModelCacheIdentityV2
    assert_ne!(
        identity_tenant1.canonical_bytes(),
        identity_tenant2.canonical_bytes(),
        "Different tenant IDs must produce different identity bytes"
    );
}

// =============================================================================
// Test: Memory pressure and eviction
// =============================================================================

#[test]
fn test_prefix_kv_cache_eviction_under_memory_pressure() {
    // Very small cache: 2KB total
    let cache = PrefixKvCache::new(2048);

    let make_entry = |size_per_layer: usize| {
        let keys = vec![vec![1.0f32; size_per_layer]; 2];
        let values = vec![vec![2.0f32; size_per_layer]; 2];
        PrefixKvEntry::new(keys, values, "tenant".to_string(), 10)
    };

    // Entry size: 2 layers * 128 floats * 4 bytes * 2 (K+V) = 2048 bytes (fits exactly)
    let entry1 = make_entry(128);
    let key1 = B3Hash::hash(b"key1");

    cache.insert(key1, entry1).unwrap();
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.used_bytes(), 2048);

    // Try to insert another entry of same size - requires eviction
    let entry2 = make_entry(128);
    let key2 = B3Hash::hash(b"key2");

    cache.insert(key2, entry2).unwrap();

    // key1 should be evicted (LRU)
    assert_eq!(cache.len(), 1);
    assert!(cache.get(&key1).is_none(), "key1 should be evicted");
    assert!(cache.get(&key2).is_some(), "key2 should be present");
    assert_eq!(cache.stats().evictions, 1);
}

#[test]
fn test_prefix_kv_cache_eviction_respects_refcount() {
    // Small cache: 1.5KB (fits one 1KB entry with some headroom)
    let cache = PrefixKvCache::new(1536);

    let make_entry = || {
        // 2 layers * 64 floats * 4 bytes * 2 (K+V) = 1024 bytes
        let keys = vec![vec![1.0f32; 64]; 2];
        let values = vec![vec![2.0f32; 64]; 2];
        PrefixKvEntry::new(keys, values, "tenant".to_string(), 10)
    };

    let key1 = B3Hash::hash(b"key1");
    let key2 = B3Hash::hash(b"key2");

    // Insert first entry and acquire a reference
    cache.insert(key1, make_entry()).unwrap();
    let entry1_ref = cache.get(&key1).unwrap();
    entry1_ref.acquire(); // Mark as in-use

    // Try to insert another 1KB entry - would need to evict key1, but it's in use
    let result = cache.insert(key2, make_entry());

    assert!(result.is_err(), "Should fail to evict in-use entry");
    assert!(cache.get(&key1).is_some(), "key1 should still be present");
    assert!(cache.get(&key2).is_none(), "key2 should not be inserted");

    // Release the reference
    entry1_ref.release();

    // Now insertion should succeed
    let result = cache.insert(key2, make_entry());
    assert!(result.is_ok(), "Should succeed after releasing reference");
    assert!(cache.get(&key2).is_some(), "key2 should now be inserted");
}

// =============================================================================
// Test: Prefix KV cache hit/miss metrics
// =============================================================================

#[test]
fn test_prefix_kv_cache_hit_miss_metrics() {
    let cache = PrefixKvCache::new(10 * 1024 * 1024);

    let make_entry = || {
        let keys = vec![vec![1.0f32; 32]; 2];
        let values = vec![vec![2.0f32; 32]; 2];
        PrefixKvEntry::new(keys, values, "tenant".to_string(), 5)
    };

    let key1 = B3Hash::hash(b"key1");
    let key2 = B3Hash::hash(b"key2");
    let key3 = B3Hash::hash(b"key3");

    // Initial stats
    let initial = cache.stats();
    assert_eq!(initial.hits, 0);
    assert_eq!(initial.misses, 0);

    // Miss on key1
    assert!(cache.get(&key1).is_none());
    assert_eq!(cache.stats().misses, 1);
    assert_eq!(cache.stats().hits, 0);

    // Insert key1
    cache.insert(key1, make_entry()).unwrap();

    // Hit on key1
    assert!(cache.get(&key1).is_some());
    assert_eq!(cache.stats().hits, 1);
    assert_eq!(cache.stats().misses, 1);

    // Another hit on key1
    assert!(cache.get(&key1).is_some());
    assert_eq!(cache.stats().hits, 2);

    // Miss on key2
    assert!(cache.get(&key2).is_none());
    assert_eq!(cache.stats().misses, 2);

    // Insert key2
    cache.insert(key2, make_entry()).unwrap();

    // Hit on key2
    assert!(cache.get(&key2).is_some());
    assert_eq!(cache.stats().hits, 3);

    // Miss on key3
    assert!(cache.get(&key3).is_none());
    assert_eq!(cache.stats().misses, 3);

    // Final stats
    let final_stats = cache.stats();
    assert_eq!(final_stats.hits, 3);
    assert_eq!(final_stats.misses, 3);
    assert_eq!(final_stats.entry_count, 2);

    // Hit rate should be 50%
    let hit_rate = final_stats.hit_rate_percent();
    assert!((hit_rate - 50.0).abs() < 0.01, "Hit rate should be 50%");
}

// =============================================================================
// Test: Prefix resolver integration
// =============================================================================

#[tokio::test]
async fn test_prefix_resolver_integration() {
    use adapteros_api_types::prefix_templates::{CreatePrefixTemplateRequest, PrefixMode};
    use adapteros_db::Db;
    use adapteros_server_api::prefix_resolver::PrefixResolver;
    use std::sync::Arc;

    let db = Arc::new(Db::new_in_memory().await.expect("Failed to create test db"));

    // Create tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')")
        .execute(db.pool())
        .await
        .expect("Failed to create tenant");

    // Create a prefix template
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::System,
        template_text: "You are a helpful AI assistant.".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .expect("Failed to create template");

    let resolver = PrefixResolver::new(db);

    // Mock tokenizer function
    let tokenize_fn = |text: &str| -> adapteros_core::Result<Vec<u32>> {
        assert_eq!(text, "You are a helpful AI assistant.");
        Ok(vec![100, 200, 300, 400, 500])
    };

    // Resolve prefix
    let resolved = resolver
        .resolve_prefix("tenant-1", &PrefixMode::System, tokenize_fn)
        .await
        .expect("Failed to resolve prefix")
        .expect("Should find template");

    assert_eq!(resolved.token_ids, vec![100, 200, 300, 400, 500]);
    assert_eq!(resolved.template.mode, PrefixMode::System);
    assert_eq!(
        resolved.template.template_text,
        "You are a helpful AI assistant."
    );

    // Verify tokenized hash is deterministic
    let hash1 = resolved.tokenized_hash;

    let resolved2 = resolver
        .resolve_prefix("tenant-1", &PrefixMode::System, |_| {
            Ok(vec![100, 200, 300, 400, 500])
        })
        .await
        .expect("Failed to resolve prefix")
        .expect("Should find template");

    assert_eq!(
        hash1, resolved2.tokenized_hash,
        "Tokenized hash should be deterministic"
    );
}

#[tokio::test]
async fn test_prefix_resolver_fallback_to_system() {
    use adapteros_api_types::prefix_templates::{CreatePrefixTemplateRequest, PrefixMode};
    use adapteros_db::Db;
    use adapteros_server_api::prefix_resolver::PrefixResolver;
    use std::sync::Arc;

    let db = Arc::new(Db::new_in_memory().await.expect("Failed to create test db"));

    // Create tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')")
        .execute(db.pool())
        .await
        .expect("Failed to create tenant");

    // Create only a system template (no user-specific template)
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::System,
        template_text: "System fallback prefix.".to_string(),
        priority: Some(5),
        enabled: Some(true),
    })
    .await
    .expect("Failed to create template");

    let resolver = PrefixResolver::new(db);

    // Request user mode - should fall back to system
    let resolved = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, |text| {
            assert_eq!(text, "System fallback prefix.");
            Ok(vec![1, 2, 3])
        })
        .await
        .expect("Failed to resolve prefix")
        .expect("Should fall back to system template");

    assert_eq!(resolved.template.mode, PrefixMode::System);
    assert_eq!(resolved.template.template_text, "System fallback prefix.");
}
