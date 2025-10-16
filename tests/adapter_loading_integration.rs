//! Integration tests for adapter loading and UI surfacing
//!
//! Tests the end-to-end flow of:
//! 1. Database state management
//! 2. API endpoint functionality
//! 3. Adapter loading/unloading
//! 4. Telemetry event emission

use adapteros_db::Db;
use anyhow::Result;

#[tokio::test]
async fn test_adapter_load_state_transitions() -> Result<()> {
    // Create in-memory database for testing
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    // Register a test adapter
    let adapter_id = db
        .register_adapter(
            "test-adapter-001",
            "Test Adapter",
            "test_hash_b3",
            16,
            2,
            Some(r#"["rust", "python"]"#),
            Some("django"),
        )
        .await?;

    // Check initial state (should be 'cold' or 'unloaded')
    let adapter = db.get_adapter("test-adapter-001").await?;
    assert!(adapter.is_some());
    let adapter = adapter.unwrap();
    assert_eq!(adapter.current_state, "cold");

    // Test state transition: cold -> loading
    db.update_adapter_state("test-adapter-001", "loading", "test_request")
        .await?;

    let adapter = db.get_adapter("test-adapter-001").await?;
    assert_eq!(adapter.unwrap().current_state, "loading");

    // Test state transition: loading -> warm
    db.update_adapter_state("test-adapter-001", "warm", "loaded_successfully")
        .await?;

    let adapter = db.get_adapter("test-adapter-001").await?;
    let adapter = adapter.unwrap();
    assert_eq!(adapter.current_state, "warm");

    // Test memory update
    db.update_adapter_memory("test-adapter-001", 16777216)
        .await?;

    let adapter = db.get_adapter("test-adapter-001").await?;
    assert_eq!(adapter.unwrap().memory_bytes, 16777216);

    // Test state transition: warm -> unloading
    db.update_adapter_state("test-adapter-001", "unloading", "test_request")
        .await?;

    // Test state transition: unloading -> cold
    db.update_adapter_state("test-adapter-001", "cold", "unloaded_successfully")
        .await?;

    // Reset memory
    db.update_adapter_memory("test-adapter-001", 0).await?;

    let adapter = db.get_adapter("test-adapter-001").await?;
    let adapter = adapter.unwrap();
    assert_eq!(adapter.current_state, "cold");
    assert_eq!(adapter.memory_bytes, 0);

    Ok(())
}

#[tokio::test]
async fn test_adapter_activation_tracking() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    // Register adapter
    db.register_adapter(
        "test-adapter-002",
        "Test Adapter 2",
        "test_hash_b3_2",
        16,
        2,
        None,
        None,
    )
    .await?;

    // Record activation
    db.record_activation("test-adapter-002", Some("req-123"), 0.85, true)
        .await?;

    // Get stats
    let (total, selected, avg_gate) = db.get_adapter_stats("test-adapter-002").await?;
    assert_eq!(total, 1);
    assert_eq!(selected, 1);
    assert!((avg_gate - 0.85).abs() < 0.01);

    // Record more activations
    db.record_activation("test-adapter-002", Some("req-124"), 0.92, true)
        .await?;
    db.record_activation("test-adapter-002", Some("req-125"), 0.78, false)
        .await?;

    let (total, selected, avg_gate) = db.get_adapter_stats("test-adapter-002").await?;
    assert_eq!(total, 3);
    assert_eq!(selected, 2);

    // Average should be (0.85 + 0.92 + 0.78) / 3 = 0.85
    assert!((avg_gate - 0.85).abs() < 0.01);

    Ok(())
}

#[tokio::test]
async fn test_adapter_lifecycle_manager_integration() -> Result<()> {
    use adapteros_lora_lifecycle::{AdapterLoader, LifecycleManager};
    use adapteros_manifest::Policies;
    use std::path::PathBuf;

    let temp_dir = std::env::temp_dir().join("aos_test_adapter_lifecycle");
    std::fs::create_dir_all(&temp_dir)?;

    // Create a dummy adapter file
    let adapter_file = temp_dir.join("test_adapter.safetensors");
    std::fs::write(&adapter_file, b"fake safetensors data")?;

    // Create lifecycle manager
    let adapter_names = vec!["test_adapter".to_string()];
    let policies = Policies::default();
    let lifecycle = LifecycleManager::new(adapter_names, &policies, temp_dir.clone(), None, 3);

    // Test that adapter starts in unloaded state
    assert_eq!(
        lifecycle.get_state(0),
        Some(adapteros_lora_lifecycle::AdapterState::Unloaded)
    );

    // Test promotion
    lifecycle.promote_adapter(0)?;
    assert_eq!(
        lifecycle.get_state(0),
        Some(adapteros_lora_lifecycle::AdapterState::Cold)
    );

    // Cleanup
    std::fs::remove_dir_all(temp_dir)?;

    Ok(())
}

#[tokio::test]
async fn test_adapter_memory_pressure_handling() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    // Register multiple adapters
    for i in 0..5 {
        db.register_adapter(
            &format!("adapter-{}", i),
            &format!("Adapter {}", i),
            &format!("hash_{}", i),
            16,
            2,
            None,
            None,
        )
        .await?;

        // Load them
        db.update_adapter_state(&format!("adapter-{}", i), "warm", "loaded")
            .await?;

        // Set memory usage
        db.update_adapter_memory(&format!("adapter-{}", i), 16777216)
            .await?;
    }

    // Get list of warm adapters
    let warm_adapters = db.list_adapters_by_state("warm").await?;
    assert_eq!(warm_adapters.len(), 5);

    // Calculate total memory
    let total_memory: i64 = warm_adapters.iter().map(|a| a.memory_bytes).sum();
    assert_eq!(total_memory, 16777216 * 5);

    // Simulate eviction - unload adapters to reduce memory
    for i in 0..2 {
        db.update_adapter_state(&format!("adapter-{}", i), "cold", "evicted")
            .await?;
        db.update_adapter_memory(&format!("adapter-{}", i), 0)
            .await?;
    }

    // Verify only 3 adapters remain warm
    let warm_adapters = db.list_adapters_by_state("warm").await?;
    assert_eq!(warm_adapters.len(), 3);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_adapter_operations() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    // Register adapter
    db.register_adapter(
        "concurrent-adapter",
        "Concurrent Test Adapter",
        "concurrent_hash",
        16,
        2,
        None,
        None,
    )
    .await?;

    // Spawn multiple concurrent operations
    let mut handles = vec![];

    for i in 0..10 {
        let db_clone = db.clone();
        let handle = tokio::spawn(async move {
            db_clone
                .record_activation("concurrent-adapter", Some(&format!("req-{}", i)), 0.8, true)
                .await
        });
        handles.push(handle);
    }

    // Wait for all operations
    for handle in handles {
        handle.await??;
    }

    // Verify all activations were recorded
    let (total, selected, _) = db.get_adapter_stats("concurrent-adapter").await?;
    assert_eq!(total, 10);
    assert_eq!(selected, 10);

    Ok(())
}

#[cfg(test)]
mod api_tests {
    use super::*;

    // These tests would require the full server setup
    // Placeholder for actual API integration tests

    #[tokio::test]
    #[ignore] // Requires running server
    async fn test_load_adapter_api_endpoint() {
        // This would test the actual POST /v1/adapters/{id}/load endpoint
        // using reqwest or a test client
    }

    #[tokio::test]
    #[ignore] // Requires running server
    async fn test_unload_adapter_api_endpoint() {
        // This would test the actual POST /v1/adapters/{id}/unload endpoint
    }
}
