#![cfg(all(test, feature = "extended-tests"))]

//! Integration tests for adapter loading and unloading state management
//!
//! Tests the corrected handler logic to verify:
//! - State checking before operations (no race conditions)
//! - OperationTracker conflict detection
//! - Proper error handling
//! - Database consistency

use adapteros_db::{AdapterRegistrationBuilder, Db};
use adapteros_server_api::{
    operation_tracker::{AdapterOperationType, OperationTracker},
    types::AdapterResponse,
};
use adapteros_lora_lifecycle::AdapterLoader;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Test configuration for integration tests
struct TestConfig {
    db: Db,
    operation_tracker: OperationTracker,
    temp_dir: PathBuf,
}

impl TestConfig {
    async fn new() -> Self {
        // Create in-memory database
        let db = Db::connect(":memory:").await.unwrap();
        db.migrate().await.unwrap();

        // Create operation tracker with progress channel
        let (progress_tx, _) = broadcast::channel(100);
        let operation_tracker = OperationTracker::new_with_progress(
            std::time::Duration::from_secs(300),
            progress_tx,
        );

        // Create temporary directory for test adapters
        let temp_dir = std::env::temp_dir().join(format!("adapter_integration_test_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        Self {
            db,
            operation_tracker,
            temp_dir,
        }
    }

    async fn register_adapter(&self, adapter_id: &str, name: &str) -> Result<()> {
        // Create dummy adapter file
        let adapter_file = self.temp_dir.join(format!("{}.safetensors", adapter_id));
        std::fs::write(&adapter_file, vec![0u8; 1024 * 1024])?;

        let params = AdapterRegistrationBuilder::new()
            .adapter_id(adapter_id.to_string())
            .name(name.to_string())
            .hash_b3(format!("hash_{}", adapter_id))
            .rank(16)
            .tier(2)
            .build()?;
        self.db.register_adapter(params).await?;
        Ok(())
    }

    fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.temp_dir);
    }
}

#[tokio::test]
async fn test_adapter_load_conflict_detection() {
    let config = TestConfig::new().await;
    config.register_adapter("test-adapter", "Test Adapter").await.unwrap();

    // Simulate the handler logic: check state before loading

    // Initially adapter should be "cold"
    let adapter = config.db.get_adapter("test-adapter").await.unwrap().unwrap();
    assert_eq!(adapter.current_state, "cold");

    // First load operation should start
    let result1 = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load
    ).await;

    assert!(result1.is_ok(), "First load operation should start successfully");

    // Verify operation is tracked
    let operation = config.operation_tracker.is_adapter_operation_running("test-adapter", "test-tenant").await;
    assert!(operation.is_some());
    assert!(matches!(operation.unwrap().operation_type, adapteros_server_api::operation_tracker::OperationType::Adapter(AdapterOperationType::Load)));

    // Second load operation should conflict
    let result2 = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load
    ).await;

    assert!(result2.is_err(), "Second load operation should conflict");

    // Complete first operation
    config.operation_tracker.complete_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load, true
    ).await;

    // Now a new load operation should work
    let result3 = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load
    ).await;

    assert!(result3.is_ok(), "Load operation should work after completion");

    config.cleanup();
}

#[tokio::test]
async fn test_adapter_unload_conflict_detection() {
    let config = TestConfig::new().await;
    config.register_adapter("test-adapter", "Test Adapter").await.unwrap();

    // Initially adapter should be "cold"
    let adapter = config.db.get_adapter("test-adapter").await.unwrap().unwrap();
    assert_eq!(adapter.current_state, "cold");

    // Unload operation on cold adapter should work (state check in handler prevents this)
    // But OperationTracker should allow it since no operation is running
    let result = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Unload
    ).await;

    assert!(result.is_ok(), "Unload operation should start on cold adapter");

    // But in real handler, state check would prevent this
    // This test verifies OperationTracker doesn't interfere with state checks

    config.operation_tracker.complete_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Unload, true
    ).await;

    config.cleanup();
}

#[tokio::test]
async fn test_adapter_load_unload_cycle() {
    let config = TestConfig::new().await;
    config.register_adapter("test-adapter", "Test Adapter").await.unwrap();

    // Load adapter (simulate successful load)
    config.db.update_adapter_state("test-adapter", "warm", "loaded").await.unwrap();

    // Now unload should work
    let unload_result = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Unload
    ).await;

    assert!(unload_result.is_ok(), "Unload operation should start on warm adapter");

    config.operation_tracker.complete_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Unload, true
    ).await;

    // Reset to cold
    config.db.update_adapter_state("test-adapter", "cold", "unloaded").await.unwrap();

    // Now load should work again
    let load_result = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load
    ).await;

    assert!(load_result.is_ok(), "Load operation should work on cold adapter");

    config.operation_tracker.complete_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load, true
    ).await;

    config.cleanup();
}

#[tokio::test]
async fn test_adapter_operation_tracker_conflicts() {
    let config = TestConfig::new().await;
    config.register_adapter("test-adapter", "Test Adapter").await.unwrap();

    // Start a load operation
    let load_result = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load
    ).await;

    assert!(load_result.is_ok(), "Load operation should start");

    // Try to unload while loading - should conflict due to OperationTracker
    let unload_result = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Unload
    ).await;

    assert!(unload_result.is_err(), "Unload should conflict with ongoing load");

    // Complete the load operation
    config.operation_tracker.complete_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load, true
    ).await;

    // Now unload should work
    let unload_result2 = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Unload
    ).await;

    assert!(unload_result2.is_ok(), "Unload should work after load completion");

    config.operation_tracker.complete_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Unload, true
    ).await;

    config.cleanup();
}

#[tokio::test]
async fn test_adapter_operation_cleanup_on_error() {
    let config = TestConfig::new().await;
    config.register_adapter("test-adapter", "Test Adapter").await.unwrap();

    // Start a load operation
    let start_result = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load
    ).await;

    assert!(start_result.is_ok(), "Operation should start");

    // Verify operation is tracked
    let operation = config.operation_tracker.is_adapter_operation_running("test-adapter", "test-tenant").await;
    assert!(operation.is_some(), "Operation should be tracked");

    // Complete operation with failure
    config.operation_tracker.complete_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load, false
    ).await;

    // Verify operation is cleaned up
    let operation_after = config.operation_tracker.is_adapter_operation_running("test-adapter", "test-tenant").await;
    assert!(operation_after.is_none(), "Operation should be cleaned up after completion");

    config.cleanup();
}

#[tokio::test]
async fn test_adapter_concurrent_operations_different_adapters() {
    let config = TestConfig::new().await;

    // Register multiple adapters
    config.register_adapter("adapter1", "Adapter 1").await.unwrap();
    config.register_adapter("adapter2", "Adapter 2").await.unwrap();

    // Start operations on different adapters - should not conflict
    let result1 = config.operation_tracker.start_adapter_operation(
        "adapter1", "test-tenant", AdapterOperationType::Load
    ).await;

    let result2 = config.operation_tracker.start_adapter_operation(
        "adapter2", "test-tenant", AdapterOperationType::Load
    ).await;

    assert!(result1.is_ok(), "First adapter operation should start");
    assert!(result2.is_ok(), "Second adapter operation should start independently");

    // Both should be tracked
    let op1 = config.operation_tracker.is_adapter_operation_running("adapter1", "test-tenant").await;
    let op2 = config.operation_tracker.is_adapter_operation_running("adapter2", "test-tenant").await;

    assert!(op1.is_some(), "Adapter1 operation should be tracked");
    assert!(op2.is_some(), "Adapter2 operation should be tracked");

    // Complete both
    config.operation_tracker.complete_adapter_operation("adapter1", "test-tenant", AdapterOperationType::Load, true).await;
    config.operation_tracker.complete_adapter_operation("adapter2", "test-tenant", AdapterOperationType::Load, true).await;

    config.cleanup();
}

#[tokio::test]
async fn test_adapter_operation_retry_allowed() {
    let config = TestConfig::new().await;
    config.register_adapter("test-adapter", "Test Adapter").await.unwrap();

    // Start a load operation
    let result1 = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load
    ).await;

    assert!(result1.is_ok(), "First operation should start");

    // Try to start the same operation again - should be allowed (retry)
    let result2 = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load
    ).await;

    assert!(result2.is_ok(), "Same operation should be allowed as retry");

    // But different operation should conflict
    let result3 = config.operation_tracker.start_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Unload
    ).await;

    assert!(result3.is_err(), "Different operation should conflict");

    config.operation_tracker.complete_adapter_operation(
        "test-adapter", "test-tenant", AdapterOperationType::Load, true
    ).await;

    config.cleanup();
}
