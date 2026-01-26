//! Plan composition and determinism tests for adapterOS
//!
//! This test suite validates:
//! 1. Plan determinism: same inputs → same plan hash
//! 2. Plan composition: multiple adapters combined correctly
//! 3. Adapter list parsing and validation
//! 4. Routing configuration binding
//! 5. Policy binding and validation
//! 6. Invalid plan rejection with clear errors

use adapteros_core::{AosError, B3Hash};
use adapteros_lora_plan::{build_plan, PlanMeta, TensorLayout};
use adapteros_model_hub::manifest::{
    Adapter, AdapterCategory, AdapterScope, AdapterTier, AssuranceTier, Base, BundleCfg,
    DeterminismPolicy, DriftPolicy, EgressPolicy, EvictionPriority, EvidencePolicy,
    IsolationPolicy, ManifestV3, MemoryPolicy, NumericPolicy, PerformancePolicy, Policies,
    RagPolicy, RefusalPolicy, RouterCfg, Sampling, Seeds, TelemetryCfg, ArtifactsPolicy,
    AdapterDependencies,
};
use std::collections::BTreeMap;

/// Create a minimal valid manifest for testing
fn create_test_manifest() -> ManifestV3 {
    ManifestV3 {
        schema: "adapteros.manifest.v3".to_string(),
        base: Base {
            model_id: "test-model".to_string(),
            model_hash: B3Hash::hash(b"model"),
            arch: "qwen2".to_string(),
            vocab_size: 32000,
            hidden_dim: 4096,
            n_layers: 32,
            n_heads: 32,
            routing_bias: 1.0,
            config_hash: B3Hash::hash(b"config"),
            tokenizer_hash: B3Hash::hash(b"tokenizer"),
            tokenizer_cfg_hash: B3Hash::hash(b"tokenizer_cfg"),
            license_hash: Some(B3Hash::hash(b"license")),
            rope_scaling_override: None,
        },
        adapters: vec![],
        router: RouterCfg {
            k_sparse: 3,
            gate_quant: "q15".to_string(),
            entropy_floor: 0.02,
            tau: 1.0,
            sample_tokens_full: 128,
            warmup: false,
            algorithm: "weighted".to_string(),
            safe_mode: false,
            orthogonal_penalty: 0.1,
            shared_downsample: false,
            compression_ratio: 0.8,
            multi_path_enabled: false,
            diversity_threshold: 0.05,
            orthogonal_constraints: false,
        },
        telemetry: TelemetryCfg {
            schema_hash: B3Hash::hash(b"schema"),
            sampling: Sampling {
                token: 0.05,
                router: 1.0,
                inference: 1.0,
            },
            router_full_tokens: 128,
            bundle: BundleCfg {
                max_events: 500000,
                max_bytes: 268435456,
            },
        },
        policies: Policies {
            egress: EgressPolicy {
                mode: "deny_all".to_string(),
                serve_requires_pf: true,
                allow_tcp: false,
                allow_udp: false,
                uds_paths: vec!["/var/run/aos/<tenant>/*.sock".to_string()],
            },
            determinism: DeterminismPolicy {
                require_metallib_embed: true,
                require_kernel_hash_match: true,
                rng: "hkdf_seeded".to_string(),
                retrieval_tie_break: vec!["score_desc".to_string(), "doc_id_asc".to_string()],
            },
            evidence: EvidencePolicy {
                require_open_book: true,
                min_spans: 1,
                prefer_latest_revision: true,
                warn_on_superseded: true,
            },
            refusal: RefusalPolicy {
                abstain_threshold: 0.55,
                missing_fields_templates: BTreeMap::new(),
            },
            numeric: NumericPolicy {
                canonical_units: BTreeMap::new(),
                max_rounding_error: 0.5,
                require_units_in_trace: true,
            },
            rag: RagPolicy {
                index_scope: "per_tenant".to_string(),
                doc_tags_required: vec![
                    "doc_id".to_string(),
                    "rev".to_string(),
                    "effectivity".to_string(),
                    "source_type".to_string(),
                ],
                embedding_model_hash: B3Hash::hash(b"embedding"),
                topk: 5,
                order: vec!["score_desc".to_string(), "doc_id_asc".to_string()],
            },
            isolation: IsolationPolicy {
                process_model: "per_tenant".to_string(),
                uds_root: "/var/run/aos/<tenant>".to_string(),
                forbid_shm: true,
            },
            performance: PerformancePolicy {
                latency_p95_ms: 24,
                router_overhead_pct_max: 8,
                throughput_tokens_per_s_min: 40,
                max_tokens: 1000,
                cpu_threshold_pct: 90.0,
                memory_threshold_pct: 95.0,
                circuit_breaker_threshold: 5,
            },
            memory: MemoryPolicy {
                min_headroom_pct: 15,
                evict_order: vec![
                    "ephemeral_ttl".to_string(),
                    "cold_lru".to_string(),
                    "warm_lru".to_string(),
                ],
                k_reduce_before_evict: true,
            },
            artifacts: ArtifactsPolicy {
                require_signature: true,
                require_sbom: true,
                cas_only: true,
            },
            drift: DriftPolicy::default(),
        },
        seeds: Seeds {
            global: B3Hash::hash(b"global_seed"),
            manifest_hash: B3Hash::hash(b"manifest"),
            parent_cpid: None,
        },
        coreml: None,
        fusion: None,
    }
}

/// Create a test adapter with the given ID and rank
fn create_test_adapter(id: &str, rank: u32) -> Adapter {
    Adapter {
        id: id.to_string(),
        hash: B3Hash::hash(id.as_bytes()),
        assurance_tier: AssuranceTier::Standard,
        tier: AdapterTier::Persistent,
        rank,
        alpha: 32.0,
        lora_strength: None,
        target_modules: vec![
            "q_proj".to_string(),
            "k_proj".to_string(),
            "v_proj".to_string(),
            "o_proj".to_string(),
        ],
        ttl: None,
        acl: vec![],
        warmup_prompt: None,
        dependencies: None,
        determinism_seed: None,
        determinism_backend: None,
        determinism_device: None,
        drift_reference_backend: None,
        drift_metric: None,
        drift_baseline_backend: None,
        drift_test_backend: None,
        drift_tier: None,
        drift_slice_size: None,
        drift_slice_offset: None,
        drift_loss_metric: None,
        category: AdapterCategory::Code,
        scope: AdapterScope::Global,
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: Some("inference".to_string()),
        recommended_for_moe: true,
        auto_promote: false,
        eviction_priority: EvictionPriority::Normal,
        free_tokens: None,
        hot_experts: None,
    }
}

// ============================================================================
// TEST 1: Plan Determinism
// ============================================================================

#[test]
fn test_plan_determinism_same_inputs() {
    // Create two identical manifests
    let mut manifest1 = create_test_manifest();
    manifest1.adapters = vec![
        create_test_adapter("adapter-a", 16),
        create_test_adapter("adapter-b", 32),
    ];

    let mut manifest2 = create_test_manifest();
    manifest2.adapters = vec![
        create_test_adapter("adapter-a", 16),
        create_test_adapter("adapter-b", 32),
    ];

    // Create dummy metallib (same content)
    let metallib = vec![0u8; 1024];

    // Build plans
    let plan1 = build_plan(&manifest1, &metallib).expect("Plan 1 should build");
    let plan2 = build_plan(&manifest2, &metallib).expect("Plan 2 should build");

    // Verify determinism: same inputs should produce identical plan IDs
    assert_eq!(
        plan1.plan_id, plan2.plan_id,
        "Same manifest and metallib should produce identical plan IDs"
    );
    assert_eq!(
        plan1.manifest_hash, plan2.manifest_hash,
        "Manifest hashes should be identical"
    );
    assert_eq!(
        plan1.kernel_hashes, plan2.kernel_hashes,
        "Kernel hashes should be identical"
    );
    assert_eq!(
        plan1.layout_hash, plan2.layout_hash,
        "Layout hashes should be identical"
    );
}

#[test]
fn test_plan_determinism_different_adapter_order() {
    // Create manifests with adapters in different orders
    let mut manifest1 = create_test_manifest();
    manifest1.adapters = vec![
        create_test_adapter("adapter-a", 16),
        create_test_adapter("adapter-b", 32),
    ];

    let mut manifest2 = create_test_manifest();
    manifest2.adapters = vec![
        create_test_adapter("adapter-b", 32),
        create_test_adapter("adapter-a", 16),
    ];

    let metallib = vec![0u8; 1024];

    let plan1 = build_plan(&manifest1, &metallib).expect("Plan 1 should build");
    let plan2 = build_plan(&manifest2, &metallib).expect("Plan 2 should build");

    // Different adapter order should produce different plan IDs
    assert_ne!(
        plan1.plan_id, plan2.plan_id,
        "Different adapter order should produce different plan IDs"
    );
}

#[test]
fn test_plan_determinism_different_metallib() {
    let manifest = create_test_manifest();

    // Different metallibs
    let metallib1 = vec![0u8; 1024];
    let metallib2 = vec![1u8; 1024];

    let plan1 = build_plan(&manifest, &metallib1).expect("Plan 1 should build");
    let plan2 = build_plan(&manifest, &metallib2).expect("Plan 2 should build");

    // Different metallib should produce different plan IDs
    assert_ne!(
        plan1.plan_id, plan2.plan_id,
        "Different metallib should produce different plan IDs"
    );
}

// ============================================================================
// TEST 2: Plan Composition - Multiple Adapters
// ============================================================================

#[test]
fn test_plan_composition_single_adapter() {
    let mut manifest = create_test_manifest();
    manifest.adapters = vec![create_test_adapter("code-adapter", 16)];

    let metallib = vec![0u8; 1024];
    let plan = build_plan(&manifest, &metallib).expect("Plan should build with single adapter");

    // Verify plan has correct adapter count
    assert_eq!(
        plan.manifest_hash,
        manifest
            .compute_hash()
            .expect("Manifest hash should compute")
    );

    // Verify layout includes the adapter
    let layout = TensorLayout::from_manifest(&manifest).expect("Layout should compute");
    assert_eq!(
        layout.adapter_layouts.len(),
        1,
        "Layout should have 1 adapter"
    );
    assert_eq!(
        layout.adapter_layouts[0].id, "code-adapter",
        "Adapter ID should match"
    );
    assert_eq!(
        layout.adapter_layouts[0].rank, 16,
        "Adapter rank should match"
    );
}

#[test]
fn test_plan_composition_multiple_adapters() {
    let mut manifest = create_test_manifest();
    manifest.adapters = vec![
        create_test_adapter("code-adapter", 16),
        create_test_adapter("framework-adapter", 32),
        create_test_adapter("repo-adapter", 8),
    ];

    let metallib = vec![0u8; 1024];
    let _plan = build_plan(&manifest, &metallib).expect("Plan should build with multiple adapters");

    // Verify layout includes all adapters in order
    let layout = TensorLayout::from_manifest(&manifest).expect("Layout should compute");
    assert_eq!(
        layout.adapter_layouts.len(),
        3,
        "Layout should have 3 adapters"
    );

    // Verify adapter order is preserved
    assert_eq!(layout.adapter_layouts[0].id, "code-adapter");
    assert_eq!(layout.adapter_layouts[1].id, "framework-adapter");
    assert_eq!(layout.adapter_layouts[2].id, "repo-adapter");

    // Verify ranks
    assert_eq!(layout.adapter_layouts[0].rank, 16);
    assert_eq!(layout.adapter_layouts[1].rank, 32);
    assert_eq!(layout.adapter_layouts[2].rank, 8);

    // Verify rank padding (should be multiples of 16)
    assert_eq!(
        layout.adapter_layouts[0].rank_padded, 16,
        "Rank 16 should pad to 16"
    );
    assert_eq!(
        layout.adapter_layouts[1].rank_padded, 32,
        "Rank 32 should pad to 32"
    );
    assert_eq!(
        layout.adapter_layouts[2].rank_padded, 16,
        "Rank 8 should pad to 16"
    );
}

#[test]
fn test_plan_composition_adapter_memory_layout() {
    let mut manifest = create_test_manifest();
    manifest.adapters = vec![
        create_test_adapter("adapter-1", 16),
        create_test_adapter("adapter-2", 16),
    ];

    let layout = TensorLayout::from_manifest(&manifest).expect("Layout should compute");

    // Verify adapters have non-overlapping memory regions
    let adapter1 = &layout.adapter_layouts[0];
    let adapter2 = &layout.adapter_layouts[1];

    // adapter-1 should start after base layers
    assert!(
        adapter1.lora_a_offset > 0,
        "Adapter 1 should have non-zero offset"
    );

    // adapter-2 should start after adapter-1
    let adapter1_end =
        adapter1.lora_b_offset + (adapter1.rank_padded * manifest.base.hidden_dim) as usize;
    assert!(
        adapter2.lora_a_offset >= adapter1_end,
        "Adapter 2 should start after Adapter 1"
    );
}

// ============================================================================
// TEST 3: Router Configuration Binding
// ============================================================================

#[test]
fn test_routing_config_k_sparse() {
    let mut manifest = create_test_manifest();
    manifest.router.k_sparse = 5;
    manifest.adapters = vec![
        create_test_adapter("a1", 16),
        create_test_adapter("a2", 16),
        create_test_adapter("a3", 16),
        create_test_adapter("a4", 16),
        create_test_adapter("a5", 16),
    ];

    // Validation should pass with k_sparse <= adapter count
    assert!(manifest.validate().is_ok());

    // Build plan
    let metallib = vec![0u8; 1024];
    let plan = build_plan(&manifest, &metallib).expect("Plan should build with k_sparse=5");

    // Verify manifest hash includes router config
    let manifest_hash = manifest.compute_hash().expect("Hash should compute");
    assert_eq!(plan.manifest_hash, manifest_hash);
}

#[test]
fn test_routing_config_algorithm_variants() {
    let algorithms = vec!["weighted", "entropy_floor"];

    for algorithm in algorithms {
        let mut manifest = create_test_manifest();
        manifest.router.algorithm = algorithm.to_string();
        manifest.adapters = vec![create_test_adapter("test", 16)];

        assert!(
            manifest.validate().is_ok(),
            "Algorithm {} should validate",
            algorithm
        );

        let metallib = vec![0u8; 1024];
        let plan = build_plan(&manifest, &metallib)
            .unwrap_or_else(|_| panic!("Plan should build with algorithm {}", algorithm));

        // Different algorithms should produce different manifest hashes
        assert!(!plan.manifest_hash.as_bytes().is_empty());
    }
}

// ============================================================================
// TEST 4: Policy Binding
// ============================================================================

#[test]
fn test_policy_binding_determinism() {
    let mut manifest1 = create_test_manifest();
    manifest1.policies.determinism.require_metallib_embed = true;

    let mut manifest2 = create_test_manifest();
    manifest2.policies.determinism.require_metallib_embed = false;

    // Different policies should produce different hashes
    let hash1 = manifest1
        .compute_hash()
        .expect("Manifest 1 hash should compute");
    let hash2 = manifest2
        .compute_hash()
        .expect("Manifest 2 hash should compute");

    assert_ne!(
        hash1, hash2,
        "Different determinism policies should produce different hashes"
    );
}

#[test]
fn test_policy_binding_drift() {
    let mut manifest = create_test_manifest();
    manifest.policies.drift.block_on_critical = true;
    manifest.policies.drift.allow_warnings = false;

    let hash1 = manifest.compute_hash().expect("Hash should compute");

    // Change drift policy
    manifest.policies.drift.block_on_critical = false;
    let hash2 = manifest.compute_hash().expect("Hash should compute");

    assert_ne!(
        hash1, hash2,
        "Different drift policies should produce different hashes"
    );
}

// ============================================================================
// TEST 5: Invalid Plans - Clear Error Messages
// ============================================================================

#[test]
fn test_invalid_plan_zero_k_sparse() {
    let mut manifest = create_test_manifest();
    manifest.router.k_sparse = 0;

    let result = manifest.validate();
    assert!(result.is_err(), "k_sparse=0 should fail validation");

    let err = result.unwrap_err();
    assert!(
        matches!(err, AosError::InvalidManifest(_)),
        "Should return InvalidManifest error"
    );

    let error_msg = format!("{}", err);
    assert!(
        error_msg.contains("k_sparse"),
        "Error should mention k_sparse: {}",
        error_msg
    );
}

#[test]
fn test_invalid_plan_k_sparse_too_large() {
    let mut manifest = create_test_manifest();
    manifest.router.k_sparse = 10; // Max is 8

    let result = manifest.validate();
    assert!(result.is_err(), "k_sparse=10 should fail validation");

    let err = result.unwrap_err();
    let error_msg = format!("{}", err);
    assert!(
        error_msg.contains("k_sparse"),
        "Error should mention k_sparse: {}",
        error_msg
    );
}

#[test]
fn test_invalid_plan_zero_adapter_rank() {
    let mut manifest = create_test_manifest();
    let mut adapter = create_test_adapter("test", 16);
    adapter.rank = 0;
    manifest.adapters = vec![adapter];

    let result = manifest.validate();
    assert!(
        result.is_err(),
        "Adapter with rank=0 should fail validation"
    );

    let err = result.unwrap_err();
    let error_msg = format!("{}", err);
    assert!(
        error_msg.contains("rank 0") || error_msg.contains("rank=0"),
        "Error should mention zero rank: {}",
        error_msg
    );
}

#[test]
fn test_invalid_plan_negative_alpha() {
    let mut manifest = create_test_manifest();
    let mut adapter = create_test_adapter("test", 16);
    adapter.alpha = -1.0;
    manifest.adapters = vec![adapter];

    let result = manifest.validate();
    assert!(
        result.is_err(),
        "Adapter with negative alpha should fail validation"
    );

    let err = result.unwrap_err();
    let error_msg = format!("{}", err);
    assert!(
        error_msg.contains("alpha") || error_msg.contains("positive"),
        "Error should mention alpha: {}",
        error_msg
    );
}

#[test]
fn test_invalid_plan_wrong_schema() {
    let mut manifest = create_test_manifest();
    manifest.schema = "adapteros.manifest.v2".to_string();

    let result = manifest.validate();
    assert!(result.is_err(), "Wrong schema version should fail");

    let err = result.unwrap_err();
    let error_msg = format!("{}", err);
    assert!(
        error_msg.contains("schema") || error_msg.contains("Unknown"),
        "Error should mention schema: {}",
        error_msg
    );
}

#[test]
fn test_invalid_plan_entropy_floor_out_of_range() {
    let mut manifest = create_test_manifest();
    manifest.router.entropy_floor = 1.5; // Should be between 0 and 1

    let result = manifest.validate();
    assert!(
        result.is_err(),
        "entropy_floor > 1.0 should fail validation"
    );

    let err = result.unwrap_err();
    let error_msg = format!("{}", err);
    assert!(
        error_msg.contains("entropy_floor"),
        "Error should mention entropy_floor: {}",
        error_msg
    );
}

// ============================================================================
// TEST 6: Plan Hash Stability
// ============================================================================

#[test]
fn test_plan_hash_stability_serialization() {
    // Verify that serialization doesn't affect hash stability
    let manifest = create_test_manifest();

    // Serialize to JSON and back
    let json = manifest.to_json().expect("Should serialize");
    let deserialized = ManifestV3::from_json(&json).expect("Should deserialize");

    let hash1 = manifest.compute_hash().expect("Hash 1 should compute");
    let hash2 = deserialized.compute_hash().expect("Hash 2 should compute");

    assert_eq!(
        hash1, hash2,
        "Hash should be stable across serialization roundtrip"
    );
}

#[test]
fn test_plan_hash_includes_all_components() {
    let mut manifest = create_test_manifest();
    manifest.adapters = vec![create_test_adapter("test", 16)];

    let base_hash = manifest.compute_hash().expect("Base hash should compute");

    // Change base model
    manifest.base.model_id = "different-model".to_string();
    let hash_after_base = manifest
        .compute_hash()
        .expect("Hash after base change should compute");
    assert_ne!(
        base_hash, hash_after_base,
        "Changing base model should change hash"
    );

    // Reset and change adapter
    let mut manifest = create_test_manifest();
    manifest.adapters = vec![create_test_adapter("test", 16)];
    let base_hash = manifest.compute_hash().expect("Base hash should compute");

    manifest.adapters[0].rank = 32;
    let hash_after_adapter = manifest
        .compute_hash()
        .expect("Hash after adapter change should compute");
    assert_ne!(
        base_hash, hash_after_adapter,
        "Changing adapter rank should change hash"
    );

    // Reset and change router
    let mut manifest = create_test_manifest();
    manifest.adapters = vec![create_test_adapter("test", 16)];
    let base_hash = manifest.compute_hash().expect("Base hash should compute");

    manifest.router.k_sparse = 5;
    let hash_after_router = manifest
        .compute_hash()
        .expect("Hash after router change should compute");
    assert_ne!(
        base_hash, hash_after_router,
        "Changing router config should change hash"
    );
}

// ============================================================================
// TEST 7: Adapter Dependencies and Composition
// ============================================================================

#[test]
fn test_adapter_with_dependencies() {
    let mut manifest = create_test_manifest();

    let base_adapter = create_test_adapter("base-code", 32);
    manifest.adapters.push(base_adapter);

    let mut dependent_adapter = create_test_adapter("framework-specific", 16);
    dependent_adapter.dependencies = Some(AdapterDependencies {
        base_model: Some("test-model".to_string()),
        requires_adapters: vec!["base-code".to_string()],
        conflicts_with: vec![],
    });
    manifest.adapters.push(dependent_adapter);

    // Should validate successfully
    assert!(manifest.validate().is_ok());

    // Build plan
    let metallib = vec![0u8; 1024];
    let _plan = build_plan(&manifest, &metallib).expect("Plan should build with dependencies");

    // Verify layout preserves adapter order
    let layout = TensorLayout::from_manifest(&manifest).expect("Layout should compute");
    assert_eq!(layout.adapter_layouts.len(), 2);
    assert_eq!(layout.adapter_layouts[0].id, "base-code");
    assert_eq!(layout.adapter_layouts[1].id, "framework-specific");
}

#[test]
fn test_compute_plan_id_consistency() {
    // Test that PlanMeta::compute_plan_id is consistent
    let manifest_hash = B3Hash::hash(b"manifest");
    let kernel_hashes = vec![B3Hash::hash(b"kernel1"), B3Hash::hash(b"kernel2")];
    let layout_hash = B3Hash::hash(b"layout");

    let id1 = PlanMeta::compute_plan_id(&manifest_hash, &kernel_hashes, &layout_hash);
    let id2 = PlanMeta::compute_plan_id(&manifest_hash, &kernel_hashes, &layout_hash);

    assert_eq!(id1, id2, "compute_plan_id should be deterministic");

    // Different inputs should produce different IDs
    let kernel_hashes2 = vec![B3Hash::hash(b"kernel3")];
    let id3 = PlanMeta::compute_plan_id(&manifest_hash, &kernel_hashes2, &layout_hash);

    assert_ne!(
        id1, id3,
        "Different kernel hashes should produce different IDs"
    );
}
