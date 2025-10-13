/// Agent C Integration Tests
/// Tests for adapter lifecycle, memory management, and routing

#[cfg(test)]
mod tests {
    use anyhow::Result;

    #[tokio::test]
    async fn test_pinned_adapters_database() -> Result<()> {
        // Test pinned adapters database operations
        let db = mplora_db::Db::connect(":memory:").await?;
        db.migrate().await?;

        // Pin an adapter
        let id = db
            .pin_adapter(
                "tenant_test",
                "adapter_123",
                None,
                "Critical for production",
                Some("admin@example.com"),
            )
            .await?;

        assert!(!id.is_empty());

        // Check if pinned
        let is_pinned = db.is_pinned("tenant_test", "adapter_123").await?;
        assert!(is_pinned);

        // List pinned adapters
        let pinned = db.list_pinned_adapters("tenant_test").await?;
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned[0].adapter_id, "adapter_123");

        // Unpin adapter
        db.unpin_adapter("tenant_test", "adapter_123").await?;

        // Verify unpinned
        let is_pinned = db.is_pinned("tenant_test", "adapter_123").await?;
        assert!(!is_pinned);

        Ok(())
    }

    #[tokio::test]
    async fn test_pinned_adapter_ttl_expiration() -> Result<()> {
        // Test TTL expiration for pinned adapters
        let db = mplora_db::Db::connect(":memory:").await?;
        db.migrate().await?;

        // Pin adapter with expired TTL (past date)
        db.pin_adapter(
            "tenant_test",
            "adapter_expired",
            Some("2020-01-01 00:00:00"),
            "Expired pin",
            None,
        )
        .await?;

        // Should not be considered pinned due to expired TTL
        let is_pinned = db.is_pinned("tenant_test", "adapter_expired").await?;
        assert!(!is_pinned);

        // Cleanup expired pins
        let cleaned = db.cleanup_expired_pins().await?;
        assert_eq!(cleaned, 1);

        Ok(())
    }


    #[test]
    fn test_manifest_warmup_field() {
        // Test that warmup field is properly parsed from manifest
        use mplora_manifest::ManifestV3;

        let manifest_json = r#"{
            "schema": "adapteros.manifest.v3",
            "base": {
                "model_id": "qwen2.5-7b",
                "model_hash": "b3:0000000000000000000000000000000000000000000000000000000000000000",
                "arch": "qwen2",
                "vocab_size": 152064,
                "hidden_dim": 3584,
                "n_layers": 28,
                "n_heads": 28,
                "config_hash": "b3:0000000000000000000000000000000000000000000000000000000000000000",
                "tokenizer_hash": "b3:0000000000000000000000000000000000000000000000000000000000000000",
                "tokenizer_cfg_hash": "b3:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "adapters": [{
                "id": "test_adapter",
                "hash": "b3:0000000000000000000000000000000000000000000000000000000000000000",
                "tier": "persistent",
                "rank": 8,
                "alpha": 16.0,
                "target_modules": ["q_proj"],
                "warmup_prompt": "import torch"
            }],
            "router": {
                "k_sparse": 3,
                "gate_quant": "q15",
                "entropy_floor": 0.02,
                "tau": 1.0,
                "sample_tokens_full": 128,
                "warmup": true,
                "algorithm": "weighted"
            },
            "telemetry": {
                "schema_hash": "b3:0000000000000000000000000000000000000000000000000000000000000000",
                "sampling": {"token": 0.05, "router": 1.0, "inference": 1.0},
                "router_full_tokens": 128,
                "bundle": {"max_events": 500000, "max_bytes": 268435456}
            },
            "policies": {
                "egress": {"mode": "deny_all", "serve_requires_pf": true, "allow_tcp": false, "allow_udp": false, "uds_paths": []},
                "determinism": {"require_metallib_embed": true, "require_kernel_hash_match": true, "rng": "hkdf_seeded", "retrieval_tie_break": ["score_desc"]},
                "evidence": {"require_open_book": false, "min_spans": 1, "prefer_latest_revision": true, "warn_on_superseded": true},
                "refusal": {"abstain_threshold": 0.55, "missing_fields_templates": {}},
                "numeric": {"canonical_units": {}, "max_rounding_error": 0.5, "require_units_in_trace": true},
                "rag": {"index_scope": "per_tenant", "doc_tags_required": [], "embedding_model_hash": "b3:0000000000000000000000000000000000000000000000000000000000000000", "topk": 5, "order": []},
                "isolation": {"process_model": "per_tenant", "uds_root": "/var/run/aos", "forbid_shm": true},
                "performance": {"latency_p95_ms": 24, "router_overhead_pct_max": 8, "throughput_tokens_per_s_min": 40},
                "memory": {"min_headroom_pct": 15, "evict_order": [], "k_reduce_before_evict": true},
                "artifacts": {"require_signature": true, "require_sbom": true, "cas_only": true}
            },
            "seeds": {
                "global": "b3:0000000000000000000000000000000000000000000000000000000000000000",
                "manifest_hash": "b3:0000000000000000000000000000000000000000000000000000000000000000"
            }
        }"#;

        let manifest: ManifestV3 =
            serde_json::from_str(manifest_json).expect("Failed to parse manifest");

        assert!(manifest.router.warmup);
        assert_eq!(manifest.router.algorithm, "weighted");
        assert_eq!(
            manifest.adapters[0].warmup_prompt,
            Some("import torch".to_string())
        );
    }

    #[test]
    fn test_adapter_dependencies() {
        // Test adapter dependency validation
        use mplora_manifest::AdapterDependencies;

        let deps = AdapterDependencies {
            base_model: Some("qwen2.5-7b".to_string()),
            requires_adapters: vec!["base_adapter".to_string()],
            conflicts_with: vec!["legacy_adapter".to_string()],
        };

        assert_eq!(deps.base_model.as_deref(), Some("qwen2.5-7b"));
        assert_eq!(deps.requires_adapters.len(), 1);
        assert_eq!(deps.conflicts_with.len(), 1);
    }
}
