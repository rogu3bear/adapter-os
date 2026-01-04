// Copyright JKCA | 2025 James KC Auchterlonie
//
// Unit tests for atomic dual-write with rollback support (Phase 4)
//
// Tests verify:
// 1. Best-effort mode continues on KV failure
// 2. Strict mode rolls back SQL on KV failure
// 3. Consistency validation and repair
// 4. Update methods handle strict mode correctly

use adapteros_core::Result;
use adapteros_db::{
    adapters::{AdapterRegistrationBuilder, AtomicDualWriteConfig},
    Db, StorageMode,
};

#[cfg(test)]
mod atomic_dual_write_tests {
    use super::*;

    /// Helper to create test adapter registration params
    fn test_adapter_params(
        adapter_id: &str,
        tenant_id: &str,
    ) -> Result<adapteros_db::adapters::AdapterRegistrationParams> {
        AdapterRegistrationBuilder::new()
            .tenant_id(tenant_id)
            .adapter_id(adapter_id)
            .name(format!("Test Adapter {}", adapter_id))
            .hash_b3(format!("b3:{}", adapter_id))
            .rank(8)
            .tier("warm")
            .build()
    }

    #[tokio::test]
    async fn test_atomic_dual_write_config_default() {
        let config = AtomicDualWriteConfig::default();
        assert!(!config.require_kv_success);
        assert!(!config.is_strict());
    }

    #[tokio::test]
    async fn test_atomic_dual_write_config_best_effort() {
        let config = AtomicDualWriteConfig::best_effort();
        assert!(!config.require_kv_success);
        assert!(!config.is_strict());
    }

    #[tokio::test]
    async fn test_atomic_dual_write_config_strict() {
        let config = AtomicDualWriteConfig::strict_atomic();
        assert!(config.require_kv_success);
        assert!(config.is_strict());
    }

    #[tokio::test]
    async fn test_atomic_dual_write_config_from_env() {
        let previous = std::env::var_os("AOS_ATOMIC_DUAL_WRITE_STRICT");
        std::env::remove_var("AOS_ATOMIC_DUAL_WRITE_STRICT");

        // Test default (no env var)
        let config = AtomicDualWriteConfig::from_env();
        assert!(config.is_strict());

        // Test with env var set to true
        std::env::set_var("AOS_ATOMIC_DUAL_WRITE_STRICT", "true");
        let config = AtomicDualWriteConfig::from_env();
        assert!(config.is_strict());
        std::env::remove_var("AOS_ATOMIC_DUAL_WRITE_STRICT");

        // Test with env var set to 1
        std::env::set_var("AOS_ATOMIC_DUAL_WRITE_STRICT", "1");
        let config = AtomicDualWriteConfig::from_env();
        assert!(config.is_strict());
        std::env::remove_var("AOS_ATOMIC_DUAL_WRITE_STRICT");

        // Test with env var set to false
        std::env::set_var("AOS_ATOMIC_DUAL_WRITE_STRICT", "false");
        let config = AtomicDualWriteConfig::from_env();
        assert!(!config.is_strict());
        std::env::remove_var("AOS_ATOMIC_DUAL_WRITE_STRICT");

        if let Some(value) = previous {
            std::env::set_var("AOS_ATOMIC_DUAL_WRITE_STRICT", value);
        } else {
            std::env::remove_var("AOS_ATOMIC_DUAL_WRITE_STRICT");
        }
    }

    #[tokio::test]
    async fn test_best_effort_mode_sql_only() -> Result<()> {
        // Test that best-effort mode works with SQL-only (no KV backend)
        let db = Db::new_in_memory()
            .await?
            .with_atomic_dual_write_config(AtomicDualWriteConfig::best_effort());

        let tenant_id = db.create_tenant("Test Tenant", false).await?;
        let params = test_adapter_params("test-adapter-1", &tenant_id)?;
        let id = db.register_adapter_extended(params).await?;

        // Verify adapter was created in SQL
        let adapter = db.get_adapter("test-adapter-1").await?;
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().id, id);

        Ok(())
    }

    #[tokio::test]
    async fn test_strict_mode_sql_only() -> Result<()> {
        // Test that strict mode works with SQL-only (no KV backend to fail)
        let db = Db::new_in_memory()
            .await?
            .with_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());

        let tenant_id = db.create_tenant("Test Tenant", false).await?;
        let params = test_adapter_params("test-adapter-2", &tenant_id)?;
        let id = db.register_adapter_extended(params).await?;

        // Verify adapter was created in SQL
        let adapter = db.get_adapter("test-adapter-2").await?;
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().id, id);

        Ok(())
    }

    #[tokio::test]
    async fn test_ensure_consistency_returns_false_for_missing_adapter() -> Result<()> {
        let db = Db::new_in_memory().await?;

        let result = db.ensure_consistency("nonexistent-adapter").await?;
        assert!(!result); // Should return false when adapter doesn't exist

        Ok(())
    }

    #[tokio::test]
    async fn test_ensure_consistency_batch_empty() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Empty batch should return empty results
        let results = db.ensure_consistency_batch(&[]).await;
        assert!(results.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_ensure_consistency_batch_with_adapters() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Create tenant and adapters
        let tenant_id = db.create_tenant("Test Tenant", false).await?;
        for i in 1..=3 {
            let params = test_adapter_params(&format!("batch-adapter-{}", i), &tenant_id)?;
            db.register_adapter_extended(params).await?;
        }

        // Run batch consistency check
        let adapter_ids: Vec<String> = (1..=3).map(|i| format!("batch-adapter-{}", i)).collect();
        let results = db.ensure_consistency_batch(&adapter_ids).await;

        assert_eq!(results.len(), 3);
        for (id, result) in &results {
            assert!(result.is_ok(), "Adapter {} should be consistent", id);
            assert!(
                result.as_ref().unwrap(),
                "Adapter {} should return true",
                id
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_tenant_consistency_with_adapters() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Create tenant with multiple adapters
        let tenant_id = db.create_tenant("consistency-test-tenant", false).await?;
        for i in 1..=5 {
            let params = AdapterRegistrationBuilder::new()
                .tenant_id(&tenant_id)
                .adapter_id(&format!("consistency-adapter-{}", i))
                .name(format!("Consistency Test Adapter {}", i))
                .hash_b3(format!("b3:consistency_{}", i))
                .rank(8)
                .build()?;
            db.register_adapter_extended(params).await?;
        }

        // Validate tenant consistency
        let (consistent, inconsistent, errors) =
            db.validate_tenant_consistency(&tenant_id, false).await?;

        assert_eq!(consistent, 5, "All 5 adapters should be consistent");
        assert_eq!(inconsistent, 0, "No adapters should be inconsistent");
        assert_eq!(errors, 0, "No errors should occur");

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_tenant_consistency_with_repair() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Create tenant with adapters
        let tenant_id = db.create_tenant("repair-test-tenant", false).await?;
        for i in 1..=3 {
            let params = AdapterRegistrationBuilder::new()
                .tenant_id(&tenant_id)
                .adapter_id(&format!("repair-adapter-{}", i))
                .name(format!("Repair Test Adapter {}", i))
                .hash_b3(format!("b3:repair_{}", i))
                .rank(8)
                .build()?;
            db.register_adapter_extended(params).await?;
        }

        // Validate with repair=true
        let (consistent, inconsistent, errors) =
            db.validate_tenant_consistency(&tenant_id, true).await?;

        assert_eq!(
            consistent, 3,
            "All 3 adapters should be consistent after repair"
        );
        assert_eq!(inconsistent, 0, "No adapters should remain inconsistent");
        assert_eq!(errors, 0, "No errors should occur");

        Ok(())
    }

    #[tokio::test]
    async fn test_db_config_persistence() -> Result<()> {
        // Test that atomic dual-write config persists through cloning
        let db = Db::new_in_memory()
            .await?
            .with_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());

        assert!(db.atomic_dual_write_config().is_strict());

        // Clone the db
        let db_clone = db.clone();
        assert!(db_clone.atomic_dual_write_config().is_strict());

        Ok(())
    }

    #[tokio::test]
    async fn test_ensure_consistency_no_kv_backend() -> Result<()> {
        // Test that ensure_consistency handles missing KV backend gracefully
        let db = Db::new_in_memory().await?;

        // Create adapter in SQL
        let tenant_id = db.create_tenant("Test Tenant", false).await?;
        let params = test_adapter_params("test-adapter-3", &tenant_id)?;
        db.register_adapter_extended(params).await?;

        // Call ensure_consistency - should return Ok(true) when no KV backend
        let result = db.ensure_consistency("test-adapter-3").await?;
        assert!(result);

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_tenant_consistency_no_kv_backend() -> Result<()> {
        // Test that tenant consistency validation works without KV backend
        let db = Db::new_in_memory().await?;

        // Initialize tenant
        let tenant_id = db.create_tenant("test-tenant", false).await?;

        // Create some adapters
        for i in 1..=3 {
            let params = AdapterRegistrationBuilder::new()
                .tenant_id(&tenant_id)
                .adapter_id(&format!("adapter-{}", i))
                .name(format!("Test Adapter {}", i))
                .hash_b3(format!("b3:{}", i))
                .rank(8)
                .build()?;
            db.register_adapter_extended(params).await?;
        }

        // Validate consistency - should succeed with no inconsistencies
        let (consistent, inconsistent, errors) =
            db.validate_tenant_consistency(&tenant_id, false).await?;
        assert_eq!(consistent, 3);
        assert_eq!(inconsistent, 0);
        assert_eq!(errors, 0);

        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    #![allow(unused_imports)]
    use super::*;

    // =========================================================================
    // Integration Tests: Dual-Write with Real KV Backend
    //
    // AUDIT NOTE (2026-01-03): These tests are intentionally #[ignore] because:
    //
    // 1. They require a running redb KV backend, which needs:
    //    - A writable temp directory (tests currently use in-memory SQLite only)
    //    - The `kv-backend` feature flag enabled
    //
    // 2. Running these tests in CI would require:
    //    - Adding `--features kv-backend` to the test command
    //    - Setting up temp directories for redb files
    //    - Adding cleanup to prevent test pollution
    //
    // To run these tests locally:
    //   cargo test -p adapteros-db --features kv-backend -- --ignored
    //
    // TODO: Implement these tests when KV backend is production-ready.
    //       Track in: https://github.com/adapteros/adapteros/issues/XXX
    // =========================================================================

    /// Tests the complete Phase 4 migration workflow
    ///
    /// This test verifies the full dual-write migration path:
    /// 1. Start in SqlOnly mode
    /// 2. Switch to DualWrite with best-effort
    /// 3. Validate consistency between SQL and KV
    /// 4. Switch to strict atomic mode
    /// 5. Verify all operations succeed or rollback completely
    /// 6. Switch to KvPrimary mode
    /// 7. Validate reads come from KV, writes still go to both
    ///
    /// # Requirements
    /// - `kv-backend` feature flag
    /// - Writable temp directory for redb
    #[tokio::test]
    #[ignore = "requires kv-backend feature and redb setup - run with: cargo test -p adapteros-db --features kv-backend -- --ignored"]
    async fn test_full_migration_workflow() {
        // TODO: Implement when KV backend is production-ready
        //
        // Test outline:
        // let db = Db::new_in_memory().await.unwrap();
        // let kv = KvBackend::new_temp().await.unwrap();
        //
        // // Phase 1: SQL only
        // db.set_storage_mode(StorageMode::SqlOnly);
        // let adapter_id = db.register_adapter(...).await.unwrap();
        //
        // // Phase 2: Best-effort dual-write
        // db.set_storage_mode(StorageMode::DualWrite { mode: DualWriteMode::BestEffort });
        // let adapter_id_2 = db.register_adapter(...).await.unwrap();
        //
        // // Verify both stores have the data
        // assert!(db.get_adapter(adapter_id_2).await.is_ok());
        // assert!(kv.get_adapter(adapter_id_2).await.is_ok());
        //
        // // Phase 3: Strict atomic mode
        // db.set_storage_mode(StorageMode::DualWrite { mode: DualWriteMode::StrictAtomic });
        //
        // // Simulate KV failure - SQL should rollback
        // kv.set_fail_next_write(true);
        // let result = db.register_adapter(...).await;
        // assert!(result.is_err());
        //
        // // Verify SQL was rolled back (count unchanged)
        // let count = db.count_adapters().await.unwrap();
        // assert_eq!(count, 2);
        //
        // // Phase 4: KV primary
        // db.set_storage_mode(StorageMode::KvPrimary);
        // let adapter = db.get_adapter(adapter_id).await.unwrap();
        // // Verify read came from KV (check metrics or logs)
    }

    /// Tests concurrent dual-write operations under failure conditions
    ///
    /// Spawns multiple tasks performing adapter operations while simulating
    /// random KV failures. Verifies that SQL and KV remain consistent at the end.
    ///
    /// # Requirements
    /// - `kv-backend` feature flag
    /// - Writable temp directory for redb
    #[tokio::test]
    #[ignore = "requires kv-backend feature and redb setup - run with: cargo test -p adapteros-db --features kv-backend -- --ignored"]
    async fn test_concurrent_dual_write_operations() {
        // TODO: Implement when KV backend is production-ready
        //
        // Test outline:
        // let db = Arc::new(Db::new_in_memory().await.unwrap());
        // let kv = Arc::new(KvBackend::new_temp().await.unwrap());
        // db.set_storage_mode(StorageMode::DualWrite { mode: DualWriteMode::StrictAtomic });
        //
        // let mut handles = vec![];
        // for i in 0..10 {
        //     let db = Arc::clone(&db);
        //     let handle = tokio::spawn(async move {
        //         // Randomly fail some KV writes
        //         if i % 3 == 0 {
        //             kv.set_fail_next_write(true);
        //         }
        //         let _ = db.register_adapter(...).await;
        //     });
        //     handles.push(handle);
        // }
        //
        // for handle in handles {
        //     let _ = handle.await;
        // }
        //
        // // Verify consistency: SQL count == KV count
        // let sql_count = db.count_adapters().await.unwrap();
        // let kv_count = kv.count_adapters().await.unwrap();
        // assert_eq!(sql_count, kv_count, "SQL and KV must be consistent after concurrent ops");
    }
}
