#![cfg(all(test, feature = "extended-tests"))]

//! Comprehensive failure scenario tests for adapter loading/unloading
//!
//! Tests partial failures including:
//! - Database succeeds but runtime load fails
//! - Runtime load succeeds but database update fails
//! - Partial unload failures
//! - State consistency recovery
//! - Error rollback mechanisms
//! - Edge cases that cause production issues

use adapteros_db::{AdapterRegistrationBuilder, Db};
use adapteros_lora_lifecycle::AdapterLoader;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

fn new_test_dir(prefix: &str) -> Result<PathBuf> {
    let temp_root = PathBuf::from("var/tmp");
    std::fs::create_dir_all(&temp_root)?;
    let temp_dir = temp_root.join(format!("{}_{}", prefix, Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir)?;
    Ok(temp_dir)
}

/// Mock adapter loader that can simulate failures
struct MockAdapterLoader {
    base_path: PathBuf,
    should_fail_load: bool,
    should_fail_unload: bool,
    fail_on_nth_load: Option<usize>,
    load_count: Arc<Mutex<usize>>,
}

impl MockAdapterLoader {
    fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            should_fail_load: false,
            should_fail_unload: false,
            fail_on_nth_load: None,
            load_count: Arc::new(Mutex::new(0)),
        }
    }

    fn set_fail_load(&mut self, fail: bool) {
        self.should_fail_load = fail;
    }

    fn set_fail_unload(&mut self, fail: bool) {
        self.should_fail_unload = fail;
    }

    fn set_fail_on_nth_load(&mut self, n: usize) {
        self.fail_on_nth_load = Some(n);
    }

    async fn load_adapter_async(
        &mut self,
        _adapter_id: u16,
        adapter_name: &str,
    ) -> Result<adapteros_lora_lifecycle::AdapterHandle, adapteros_core::AosError> {
        let mut count = self.load_count.lock().await;
        *count += 1;

        // Check if we should fail on this specific load
        if let Some(n) = self.fail_on_nth_load {
            if *count == n {
                return Err(adapteros_core::AosError::Lifecycle(
                    "Simulated runtime load failure".to_string(),
                ));
            }
        }

        if self.should_fail_load {
            return Err(adapteros_core::AosError::Lifecycle(
                "Simulated runtime load failure".to_string(),
            ));
        }

        let adapter_path = self.base_path.join(format!("{}.safetensors", adapter_name));
        if !adapter_path.exists() {
            return Err(adapteros_core::AosError::Lifecycle(format!(
                "Adapter file not found: {}",
                adapter_path.display()
            )));
        }

        let metadata = std::fs::metadata(&adapter_path)
            .map_err(|e| adapteros_core::AosError::Lifecycle(format!("IO error: {}", e)))?;

        Ok(adapteros_lora_lifecycle::AdapterHandle {
            adapter_id: 0,
            path: adapter_path,
            memory_bytes: metadata.len() as usize,
        })
    }

    fn unload_adapter(&mut self, _adapter_id: u16) -> Result<(), adapteros_core::AosError> {
        if self.should_fail_unload {
            return Err(adapteros_core::AosError::Lifecycle(
                "Simulated runtime unload failure".to_string(),
            ));
        }
        Ok(())
    }
}

/// Test: DB update succeeds but runtime load fails (should rollback)
#[tokio::test]
async fn test_db_succeeds_runtime_load_fails() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_failure_test")?;

    let adapter_id = "failure-adapter-1";
    let adapter_file = temp_dir.join("failure-adapter-1.safetensors");
    std::fs::write(&adapter_file, vec![0u8; 1024 * 1024])?;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id.to_string())
        .name("Failure Test Adapter 1")
        .hash_b3("failure_hash_1")
        .rank(16)
        .tier(2)
        .build()?;
    db.register_adapter(params).await?;

    // Verify initial state
    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(adapter.current_state, "cold");

    // Step 1: Update DB state to "loading" (simulates DB success)
    db.update_adapter_state(adapter_id, "loading", "test_request")
        .await?;

    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(adapter.current_state, "loading");

    // Step 2: Simulate runtime load failure
    let mut mock_loader = MockAdapterLoader::new(temp_dir.clone());
    mock_loader.set_fail_load(true);

    match mock_loader.load_adapter_async(0, "failure-adapter-1").await {
        Ok(_) => panic!("Expected load to fail"),
        Err(_) => {
            // Step 3: Rollback DB state (this is what the handler should do)
            db.update_adapter_state(adapter_id, "cold", "load_failed")
                .await?;
        }
    }

    // Step 4: Verify rollback was successful
    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(
        adapter.current_state, "cold",
        "Adapter state should be rolled back to cold after runtime failure"
    );

    // Verify no memory was allocated
    assert_eq!(
        adapter.memory_bytes, 0,
        "Memory should be 0 after failed load"
    );

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Test: Runtime load succeeds but DB update fails (should handle gracefully)
#[tokio::test]
async fn test_runtime_load_succeeds_db_update_fails() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_failure_test")?;

    let adapter_id = "failure-adapter-2";
    let adapter_file = temp_dir.join("failure-adapter-2.safetensors");
    std::fs::write(&adapter_file, vec![0u8; 1024 * 1024])?;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id.to_string())
        .name("Failure Test Adapter 2")
        .hash_b3("failure_hash_2")
        .rank(16)
        .tier(2)
        .build()?;
    db.register_adapter(params).await?;

    // Step 1: Runtime load succeeds
    let mut loader = AdapterLoader::new(temp_dir.clone());
    let handle = loader.load_adapter_async(0, "failure-adapter-2").await?;

    // Step 2: Simulate DB update failure by using invalid adapter ID
    // (This simulates a scenario where DB update fails)
    let invalid_id = "nonexistent-adapter";
    let db_update_result = db
        .update_adapter_state(invalid_id, "warm", "loaded_successfully")
        .await;

    // The DB update should fail (adapter not found)
    assert!(db_update_result.is_err());

    // Step 3: In production, we should attempt to unload the adapter
    // since DB state update failed, to maintain consistency
    loader.unload_adapter(0)?;

    // Step 4: Verify adapter is still in cold state (DB update failed)
    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(
        adapter.current_state, "cold",
        "Adapter should remain in cold state when DB update fails"
    );

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Test: DB update succeeds but runtime unload fails
#[tokio::test]
async fn test_db_succeeds_runtime_unload_fails() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_failure_test")?;

    let adapter_id = "failure-adapter-3";
    let adapter_file = temp_dir.join("failure-adapter-3.safetensors");
    std::fs::write(&adapter_file, vec![0u8; 1024 * 1024])?;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id.to_string())
        .name("Failure Test Adapter 3")
        .hash_b3("failure_hash_3")
        .rank(16)
        .tier(2)
        .build()?;
    db.register_adapter(params).await?;

    // First, load the adapter successfully
    let mut loader = AdapterLoader::new(temp_dir.clone());
    let handle = loader.load_adapter_async(0, "failure-adapter-3").await?;

    db.update_adapter_state(adapter_id, "warm", "loaded_successfully")
        .await?;
    db.update_adapter_memory(adapter_id, handle.memory_bytes() as i64)
        .await?;

    // Verify loaded state
    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(adapter.current_state, "warm");

    // Step 1: Update DB state to "unloading"
    db.update_adapter_state(adapter_id, "unloading", "test_unload")
        .await?;

    // Step 2: Simulate runtime unload failure
    let mut mock_loader = MockAdapterLoader::new(temp_dir.clone());
    mock_loader.set_fail_unload(true);

    match mock_loader.unload_adapter(0) {
        Ok(_) => panic!("Expected unload to fail"),
        Err(_) => {
            // Step 3: Rollback DB state to "warm" (adapter still loaded)
            db.update_adapter_state(adapter_id, "warm", "unload_failed")
                .await?;
        }
    }

    // Step 4: Verify state reflects that adapter is still loaded
    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(
        adapter.current_state, "warm",
        "Adapter should remain in warm state when unload fails"
    );

    // Memory should still be allocated
    assert!(
        adapter.memory_bytes > 0,
        "Memory should still be allocated after failed unload"
    );

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Test: Partial failure during concurrent operations
#[tokio::test]
async fn test_partial_failure_concurrent_ops() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_failure_test")?;

    // Create multiple adapters
    let num_adapters = 5;
    let mut adapter_ids = Vec::new();

    for i in 0..num_adapters {
        let adapter_id = format!("concurrent-failure-{}", i);
        let adapter_file = temp_dir.join(format!("{}.safetensors", adapter_id));
        std::fs::write(&adapter_file, vec![0u8; 1024 * 1024])?;

        let params = AdapterRegistrationBuilder::new()
            .adapter_id(adapter_id.clone())
            .name(format!("Concurrent Failure Adapter {}", i))
            .hash_b3(format!("concurrent_hash_{}", i))
            .rank(16)
            .tier(2)
            .build()?;
        db.register_adapter(params).await?;
        adapter_ids.push(adapter_id);
    }

    // Simulate concurrent loads where some succeed and some fail
    let mut handles = Vec::new();

    for (idx, adapter_id) in adapter_ids.iter().enumerate() {
        let db_clone = db.clone();
        let adapter_id_clone = adapter_id.clone();
        let temp_dir_clone = temp_dir.clone();
        let adapter_name = format!("concurrent-failure-{}", idx);

        // Make every 3rd adapter fail
        let should_fail = idx % 3 == 2;

        let handle = tokio::spawn(async move {
            db_clone
                .update_adapter_state(&adapter_id_clone, "loading", "concurrent_load")
                .await?;

            let mut mock_loader = MockAdapterLoader::new(temp_dir_clone);
            mock_loader.set_fail_load(should_fail);

            match mock_loader
                .load_adapter_async(idx as u16, &adapter_name)
                .await
            {
                Ok(_handle) => {
                    db_clone
                        .update_adapter_state(&adapter_id_clone, "warm", "loaded_successfully")
                        .await?;
                    Ok(())
                }
                Err(_) => {
                    // Rollback on failure
                    db_clone
                        .update_adapter_state(&adapter_id_clone, "cold", "load_failed")
                        .await?;
                    Err(())
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all operations
    let mut success_count = 0;
    let mut failure_count = 0;

    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => success_count += 1,
            Ok(Err(_)) => failure_count += 1,
            Err(e) => {
                eprintln!("Task panicked: {}", e);
                failure_count += 1;
            }
        }
    }

    // Verify all adapters are in consistent states
    let warm_adapters = db.list_adapters_by_state("default-tenant", "warm").await?;
    let cold_adapters = db.list_adapters_by_state("default-tenant", "cold").await?;
    let loading_adapters = db.list_adapters_by_state("default-tenant", "loading").await?;

    assert_eq!(
        loading_adapters.len(),
        0,
        "No adapters should be stuck in loading state"
    );

    assert_eq!(
        warm_adapters.len() + cold_adapters.len(),
        num_adapters,
        "All adapters should be accounted for"
    );

    // Verify expected success/failure counts
    // Adapters at indices 0, 1, 3, 4 should succeed (4 total)
    // Adapters at indices 2 should fail (1 total)
    assert_eq!(success_count, 4, "Expected 4 successful loads");
    assert_eq!(failure_count, 1, "Expected 1 failed load");

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Test: State recovery after partial failure
#[tokio::test]
async fn test_state_recovery_after_failure() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_failure_test")?;

    let adapter_id = "recovery-adapter";
    let adapter_file = temp_dir.join("recovery-adapter.safetensors");
    std::fs::write(&adapter_file, vec![0u8; 1024 * 1024])?;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id.to_string())
        .name("Recovery Test Adapter")
        .hash_b3("recovery_hash")
        .rank(16)
        .tier(2)
        .build()?;
    db.register_adapter(params).await?;

    // Scenario: Adapter is stuck in "loading" state (simulating a crash)
    db.update_adapter_state(adapter_id, "loading", "stuck_state")
        .await?;

    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(adapter.current_state, "loading");

    // Recovery: Attempt to load, which should detect inconsistent state
    let mut loader = AdapterLoader::new(temp_dir.clone());

    // First, try to recover by rolling back to cold
    db.update_adapter_state(adapter_id, "cold", "recovery_rollback")
        .await?;

    // Now attempt a fresh load
    match loader.load_adapter_async(0, "recovery-adapter").await {
        Ok(handle) => {
            db.update_adapter_state(adapter_id, "warm", "recovered_and_loaded")
                .await?;
            db.update_adapter_memory(adapter_id, handle.memory_bytes() as i64)
                .await?;
        }
        Err(e) => {
            db.update_adapter_state(adapter_id, "cold", "recovery_failed")
                .await?;
            return Err(anyhow::anyhow!("Recovery failed: {}", e));
        }
    }

    // Verify recovery was successful
    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(
        adapter.current_state, "warm",
        "Adapter should be in warm state after recovery"
    );

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Test: Memory consistency after failures
#[tokio::test]
async fn test_memory_consistency_after_failures() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_failure_test")?;

    let adapter_id = "memory-consistency-adapter";
    let adapter_file = temp_dir.join("memory-consistency-adapter.safetensors");
    let file_size = 2 * 1024 * 1024; // 2MB
    std::fs::write(&adapter_file, vec![0u8; file_size])?;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id.to_string())
        .name("Memory Consistency Adapter")
        .hash_b3("memory_consistency_hash")
        .rank(16)
        .tier(2)
        .build()?;
    db.register_adapter(params).await?;

    // Load adapter
    let mut loader = AdapterLoader::new(temp_dir.clone());
    let handle = loader
        .load_adapter_async(0, "memory-consistency-adapter")
        .await?;

    db.update_adapter_state(adapter_id, "warm", "loaded_successfully")
        .await?;
    db.update_adapter_memory(adapter_id, handle.memory_bytes() as i64)
        .await?;

    // Verify memory is recorded
    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(adapter.current_state, "warm");
    assert_eq!(adapter.memory_bytes, file_size as i64);

    // Simulate unload failure but DB thinks it succeeded
    // This creates an inconsistency
    db.update_adapter_state(adapter_id, "unloading", "unload_attempt")
        .await?;

    // Simulate runtime unload failure
    let mut mock_loader = MockAdapterLoader::new(temp_dir.clone());
    mock_loader.set_fail_unload(true);

    match mock_loader.unload_adapter(0) {
        Ok(_) => panic!("Expected unload to fail"),
        Err(_) => {
            // Rollback DB state
            db.update_adapter_state(adapter_id, "warm", "unload_failed")
                .await?;
            // IMPORTANT: Memory should NOT be reset to 0 on unload failure
            // It should remain at the loaded value
        }
    }

    // Verify memory consistency: should still reflect loaded state
    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(
        adapter.memory_bytes, file_size as i64,
        "Memory should remain allocated after unload failure"
    );
    assert_eq!(
        adapter.current_state, "warm",
        "State should be warm after unload failure"
    );

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Test: Multiple failure scenarios in sequence
#[tokio::test]
async fn test_multiple_failure_scenarios_sequence() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_failure_test")?;

    let adapter_id = "sequence-failure-adapter";
    let adapter_file = temp_dir.join("sequence-failure-adapter.safetensors");
    std::fs::write(&adapter_file, vec![0u8; 1024 * 1024])?;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id.to_string())
        .name("Sequence Failure Adapter")
        .hash_b3("sequence_failure_hash")
        .rank(16)
        .tier(2)
        .build()?;
    db.register_adapter(params).await?;

    // Scenario 1: Load fails, rollback succeeds
    db.update_adapter_state(adapter_id, "loading", "scenario_1")
        .await?;

    let mut mock_loader = MockAdapterLoader::new(temp_dir.clone());
    mock_loader.set_fail_load(true);

    match mock_loader
        .load_adapter_async(0, "sequence-failure-adapter")
        .await
    {
        Ok(_) => panic!("Expected failure"),
        Err(_) => {
            db.update_adapter_state(adapter_id, "cold", "scenario_1_failed")
                .await?;
        }
    }

    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(adapter.current_state, "cold");

    // Scenario 2: Load succeeds, verify final state
    mock_loader.set_fail_load(false);
    let mut real_loader = AdapterLoader::new(temp_dir.clone());

    db.update_adapter_state(adapter_id, "loading", "scenario_2")
        .await?;

    match real_loader
        .load_adapter_async(0, "sequence-failure-adapter")
        .await
    {
        Ok(handle) => {
            db.update_adapter_state(adapter_id, "warm", "scenario_2_success")
                .await?;
            db.update_adapter_memory(adapter_id, handle.memory_bytes() as i64)
                .await?;
        }
        Err(e) => {
            db.update_adapter_state(adapter_id, "cold", "scenario_2_failed")
                .await?;
            return Err(anyhow::anyhow!("Load failed: {}", e));
        }
    }

    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(adapter.current_state, "warm");
    assert!(adapter.memory_bytes > 0);

    // Scenario 3: Unload succeeds
    db.update_adapter_state(adapter_id, "unloading", "scenario_3")
        .await?;

    match real_loader.unload_adapter(0) {
        Ok(_) => {
            db.update_adapter_state(adapter_id, "cold", "scenario_3_success")
                .await?;
            db.update_adapter_memory(adapter_id, 0).await?;
        }
        Err(e) => {
            db.update_adapter_state(adapter_id, "warm", "scenario_3_failed")
                .await?;
            return Err(anyhow::anyhow!("Unload failed: {}", e));
        }
    }

    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert_eq!(adapter.current_state, "cold");
    assert_eq!(adapter.memory_bytes, 0);

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}
