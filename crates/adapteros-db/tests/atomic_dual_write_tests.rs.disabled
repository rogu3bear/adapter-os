// Copyright JKCA | 2025 James KC Auchterlonie
//
// Unit tests for atomic dual-write with rollback support (Phase 4)
//
// Tests verify:
// 1. Best-effort mode continues on KV failure
// 2. Strict mode rolls back SQL on KV failure
// 3. Consistency validation and repair
// 4. Update methods handle strict mode correctly

use adapteros_db::{
    adapters::{AdapterRegistrationBuilder, AtomicDualWriteConfig},
    Db, StorageMode,
};
use adapteros_core::Result;

#[cfg(test)]
mod atomic_dual_write_tests {
    use super::*;

    /// Helper to create a test database with both SQL and KV backends
    async fn setup_dual_write_db(config: AtomicDualWriteConfig) -> Result<Db> {
        let db = Db::new_in_memory().await?;

        // TODO: Initialize KV backend for testing
        // This requires mocking or test implementation of KvDb

        Ok(db.with_atomic_dual_write_config(config))
    }

    /// Helper to create test adapter registration params
    fn test_adapter_params(adapter_id: &str) -> Result<adapteros_db::adapters::AdapterRegistrationParams> {
        AdapterRegistrationBuilder::new()
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
        // Test default (no env var)
        let config = AtomicDualWriteConfig::from_env();
        assert!(!config.is_strict());

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
    }

    #[tokio::test]
    async fn test_best_effort_mode_sql_only() -> Result<()> {
        // Test that best-effort mode works with SQL-only (no KV backend)
        let db = Db::new_in_memory().await?
            .with_atomic_dual_write_config(AtomicDualWriteConfig::best_effort());

        let params = test_adapter_params("test-adapter-1")?;
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
        let db = Db::new_in_memory().await?
            .with_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());

        let params = test_adapter_params("test-adapter-2")?;
        let id = db.register_adapter_extended(params).await?;

        // Verify adapter was created in SQL
        let adapter = db.get_adapter("test-adapter-2").await?;
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().id, id);

        Ok(())
    }

    // TODO: Add tests with mocked KV backend failures
    // These tests require:
    // 1. Mock KV backend that can simulate failures
    // 2. Verification that SQL rollback occurs in strict mode
    // 3. Verification that warnings are logged in best-effort mode

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_best_effort_mode_continues_on_kv_failure() {
        // Setup: Create DB with best-effort config and failing KV backend
        // Action: Register adapter
        // Verify: SQL insert succeeds, warning logged, operation succeeds
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_strict_mode_rolls_back_on_kv_failure() {
        // Setup: Create DB with strict config and failing KV backend
        // Action: Register adapter
        // Verify: SQL insert rolled back, error returned, adapter not in SQL
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_strict_mode_rollback_failure_logs_critical() {
        // Setup: Create DB with strict config, failing KV backend, and SQL that prevents DELETE
        // Action: Register adapter
        // Verify: CRITICAL error logged, error indicates manual intervention needed
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_ensure_consistency_repairs_missing_kv_entry() {
        // Setup: Create adapter in SQL only
        // Action: Call ensure_consistency()
        // Verify: Adapter created in KV with matching data
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_ensure_consistency_repairs_inconsistent_data() {
        // Setup: Create adapter in both SQL and KV with different state
        // Action: Call ensure_consistency()
        // Verify: KV updated to match SQL (source of truth)
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_ensure_consistency_returns_false_for_missing_adapter() -> Result<()> {
        let db = Db::new_in_memory().await?;

        let result = db.ensure_consistency("nonexistent-adapter").await?;
        assert!(!result); // Should return false when adapter doesn't exist

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_ensure_consistency_batch() {
        // Setup: Create multiple adapters with various consistency states
        // Action: Call ensure_consistency_batch()
        // Verify: All adapters repaired, results returned for each
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_validate_tenant_consistency() {
        // Setup: Create tenant with multiple adapters in various states
        // Action: Call validate_tenant_consistency()
        // Verify: Returns correct counts of (consistent, inconsistent, errors)
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_update_state_strict_mode_logs_on_kv_failure() {
        // Setup: Create adapter, enable strict mode, mock KV failure on update
        // Action: Call update_adapter_state_tx()
        // Verify: SQL commits, error returned, consistency warning logged
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_update_memory_strict_mode_logs_on_kv_failure() {
        // Setup: Create adapter, enable strict mode, mock KV failure on update
        // Action: Call update_adapter_memory_tx()
        // Verify: SQL commits, error returned, consistency warning logged
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_update_tier_strict_mode_logs_on_kv_failure() {
        // Setup: Create adapter, enable strict mode, mock KV failure on update
        // Action: Call update_adapter_tier()
        // Verify: SQL commits, error returned, consistency warning logged
    }

    #[tokio::test]
    #[ignore] // Requires KV backend implementation
    async fn test_delete_strict_mode_logs_on_kv_failure() {
        // Setup: Create adapter, enable strict mode, mock KV failure on delete
        // Action: Call delete_adapter()
        // Verify: SQL delete succeeds, warning logged about orphaned KV entry
    }

    #[tokio::test]
    async fn test_db_config_persistence() -> Result<()> {
        // Test that atomic dual-write config persists through cloning
        let db = Db::new_in_memory().await?
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
        let params = test_adapter_params("test-adapter-3")?;
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
        db.register_tenant("test-tenant", 1000, 1000).await?;

        // Create some adapters
        for i in 1..=3 {
            let params = AdapterRegistrationBuilder::new()
                .tenant_id("test-tenant")
                .adapter_id(&format!("adapter-{}", i))
                .name(format!("Test Adapter {}", i))
                .hash_b3(format!("b3:{}", i))
                .rank(8)
                .build()?;
            db.register_adapter_extended(params).await?;
        }

        // Validate consistency - should succeed with no inconsistencies
        let (consistent, inconsistent, errors) = db.validate_tenant_consistency("test-tenant").await?;
        assert_eq!(consistent, 3);
        assert_eq!(inconsistent, 0);
        assert_eq!(errors, 0);

        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    // Integration tests that test the full flow with real KV backend
    // These are separate from unit tests as they require more setup

    #[tokio::test]
    #[ignore] // Requires full KV backend setup
    async fn test_full_migration_workflow() {
        // Test the complete Phase 4 migration workflow:
        // 1. Start in SqlOnly mode
        // 2. Switch to DualWrite with best-effort
        // 3. Validate consistency
        // 4. Switch to strict atomic mode
        // 5. Verify all operations succeed
        // 6. Switch to KvPrimary
        // 7. Validate reads come from KV
    }

    #[tokio::test]
    #[ignore] // Requires full KV backend setup
    async fn test_concurrent_dual_write_operations() {
        // Test that concurrent operations in strict mode handle failures correctly
        // Spawn multiple tasks performing adapter operations
        // Simulate random KV failures
        // Verify consistency at the end
    }
}
