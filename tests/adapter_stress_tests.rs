#![cfg(all(test, feature = "extended-tests"))]

//! Stress tests for adapter loading and unloading under high concurrency
//!
//! Tests concurrent operations including:
//! - Multiple simultaneous adapter loads
//! - Concurrent load/unload operations
//! - Race condition detection
//! - Memory pressure scenarios
//! - Operation timeout handling
//! - Database consistency under load

use adapteros_db::{AdapterRegistrationBuilder, Db};
use adapteros_lora_lifecycle::{AdapterLoader, LifecycleManager};
use adapteros_manifest::Policies;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration, Instant};
use uuid::Uuid;

/// Test configuration for stress tests
struct StressTestConfig {
    num_concurrent_ops: usize,
    num_adapters: usize,
    operation_timeout_ms: u64,
}

impl Default for StressTestConfig {
    fn default() -> Self {
        Self {
            num_concurrent_ops: 50,
            num_adapters: 10,
            operation_timeout_ms: 5000,
        }
    }
}

fn new_test_dir(prefix: &str) -> Result<PathBuf> {
    let temp_root = PathBuf::from("var/tmp");
    std::fs::create_dir_all(&temp_root)?;
    let temp_dir = temp_root.join(format!("{}_{}", prefix, Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir)?;
    Ok(temp_dir)
}

/// Test concurrent adapter loads on different adapters
#[tokio::test]
async fn test_concurrent_load_different_adapters() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let config = StressTestConfig::default();
    let num_adapters = config.num_adapters;

    // Create temporary directory for adapter files
    let temp_dir = new_test_dir("aos_stress_test")?;

    // Register multiple adapters
    let mut adapter_ids = Vec::new();
    for i in 0..num_adapters {
        let adapter_id = format!("stress-adapter-{}", i);
        let adapter_file = temp_dir.join(format!("{}.safetensors", adapter_id));

        // Create dummy adapter file
        std::fs::write(&adapter_file, vec![0u8; 1024 * 1024])?; // 1MB dummy file

        let params = AdapterRegistrationBuilder::new()
            .adapter_id(adapter_id.clone())
            .name(format!("Stress Adapter {}", i))
            .hash_b3(format!("hash_{}", i))
            .rank(16)
            .tier(2)
            .build()?;
        db.register_adapter(params).await?;
        adapter_ids.push(adapter_id);
    }

    // Create lifecycle manager
    let adapter_names: Vec<String> = (0..num_adapters)
        .map(|i| format!("stress-adapter-{}", i))
        .collect();
    let policies = Policies::default();
    let lifecycle = Arc::new(Mutex::new(LifecycleManager::new(
        adapter_names,
        &policies,
        temp_dir.clone(),
        None,
        3,
    )));

    // Spawn concurrent load operations
    let start_time = Instant::now();
    let mut handles = Vec::new();

    for i in 0..config.num_concurrent_ops {
        let db_clone = db.clone();
        let lifecycle_clone = lifecycle.clone();
        let adapter_id = adapter_ids[i % num_adapters].clone();
        let adapter_idx = (i % num_adapters) as u16;
        let adapter_name = format!("stress-adapter-{}", i % num_adapters);

        let handle = tokio::spawn(async move {
            // Update DB state to loading
            let _ = db_clone
                .update_adapter_state(&adapter_id, "loading", "concurrent_load")
                .await;

            // Attempt to load adapter
            let mut loader = AdapterLoader::new(temp_dir.clone());
            let load_result = loader.load_adapter_async(adapter_idx, &adapter_name).await;

            match load_result {
                Ok(_handle) => {
                    // Update DB state to warm
                    let _ = db_clone
                        .update_adapter_state(&adapter_id, "warm", "loaded_successfully")
                        .await;
                    Ok(())
                }
                Err(e) => {
                    // Rollback on error
                    let _ = db_clone
                        .update_adapter_state(&adapter_id, "cold", "load_failed")
                        .await;
                    Err(e)
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
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

    let elapsed = start_time.elapsed();

    // Verify database consistency
    let warm_adapters = db.list_adapters_by_state("default-tenant", "warm").await?;
    let cold_adapters = db.list_adapters_by_state("default-tenant", "cold").await?;
    let loading_adapters = db
        .list_adapters_by_state("default-tenant", "loading")
        .await?;

    // All adapters should be in a consistent state (not stuck in "loading")
    assert_eq!(
        loading_adapters.len(),
        0,
        "No adapters should be stuck in loading state"
    );

    // Verify state consistency: adapters should be either warm or cold
    assert_eq!(
        warm_adapters.len() + cold_adapters.len(),
        num_adapters,
        "All adapters should be in warm or cold state"
    );

    println!(
        "Concurrent load test completed: {} success, {} failures, {}ms elapsed",
        success_count,
        failure_count,
        elapsed.as_millis()
    );

    // Cleanup
    std::fs::remove_dir_all(&temp_dir)?;

    Ok(())
}

/// Test concurrent loads/unloads on the same adapter
#[tokio::test]
async fn test_concurrent_load_unload_same_adapter() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_stress_test")?;

    let adapter_id = "concurrent-adapter";
    let adapter_file = temp_dir.join("concurrent-adapter.safetensors");
    std::fs::write(&adapter_file, vec![0u8; 1024 * 1024])?;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id.to_string())
        .name("Concurrent Test Adapter")
        .hash_b3("concurrent_hash")
        .rank(16)
        .tier(2)
        .build()?;
    db.register_adapter(params).await?;

    let num_ops = 30;
    let mut handles = Vec::new();

    for i in 0..num_ops {
        let db_clone = db.clone();
        let adapter_id_clone = adapter_id.to_string();
        let temp_dir_clone = temp_dir.clone();
        let is_load = i % 2 == 0;

        let handle = tokio::spawn(async move {
            if is_load {
                // Load operation
                db_clone
                    .update_adapter_state(&adapter_id_clone, "loading", "concurrent_load")
                    .await?;

                let mut loader = AdapterLoader::new(temp_dir_clone.clone());
                match loader.load_adapter_async(0, "concurrent-adapter").await {
                    Ok(_) => {
                        db_clone
                            .update_adapter_state(&adapter_id_clone, "warm", "loaded_successfully")
                            .await?;
                        Ok(())
                    }
                    Err(e) => {
                        db_clone
                            .update_adapter_state(&adapter_id_clone, "cold", "load_failed")
                            .await?;
                        Err(e)
                    }
                }
            } else {
                // Unload operation
                let adapter = db_clone.get_adapter(&adapter_id_clone).await?;
                if adapter.is_some() && adapter.unwrap().current_state == "warm" {
                    db_clone
                        .update_adapter_state(&adapter_id_clone, "unloading", "concurrent_unload")
                        .await?;

                    let mut loader = AdapterLoader::new(temp_dir_clone);
                    match loader.unload_adapter(0) {
                        Ok(_) => {
                            db_clone
                                .update_adapter_state(
                                    &adapter_id_clone,
                                    "cold",
                                    "unloaded_successfully",
                                )
                                .await?;
                            db_clone.update_adapter_memory(&adapter_id_clone, 0).await?;
                            Ok(())
                        }
                        Err(e) => {
                            db_clone
                                .update_adapter_state(&adapter_id_clone, "warm", "unload_failed")
                                .await?;
                            Err(e)
                        }
                    }
                } else {
                    Ok(()) // Already unloaded, skip
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all operations
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await);
    }

    // Verify final state is consistent
    let adapter = db.get_adapter(adapter_id).await?;
    assert!(adapter.is_some());
    let adapter = adapter.unwrap();

    // Should be in a valid state
    assert!(
        matches!(
            adapter.current_state.as_str(),
            "warm" | "cold" | "unloading"
        ),
        "Adapter should be in a valid state, got: {}",
        adapter.current_state
    );

    // If in unloading state, wait a bit and check again
    if adapter.current_state == "unloading" {
        sleep(Duration::from_millis(100)).await;
        let adapter = db.get_adapter(adapter_id).await?.unwrap();
        assert!(
            matches!(adapter.current_state.as_str(), "warm" | "cold"),
            "Adapter should transition out of unloading state"
        );
    }

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Test rapid load/unload cycles
#[tokio::test]
async fn test_rapid_load_unload_cycles() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_stress_test")?;

    let adapter_id = "rapid-cycle-adapter";
    let adapter_file = temp_dir.join("rapid-cycle-adapter.safetensors");
    std::fs::write(&adapter_file, vec![0u8; 512 * 1024])?; // Smaller file for faster cycles

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id.to_string())
        .name("Rapid Cycle Adapter")
        .hash_b3("rapid_hash")
        .rank(16)
        .tier(2)
        .build()?;
    db.register_adapter(params).await?;

    let num_cycles = 20;
    let mut loader = AdapterLoader::new(temp_dir.clone());

    for cycle in 0..num_cycles {
        // Load
        db.update_adapter_state(adapter_id, "loading", &format!("cycle_{}_load", cycle))
            .await?;

        match loader.load_adapter_async(0, "rapid-cycle-adapter").await {
            Ok(handle) => {
                db.update_adapter_state(adapter_id, "warm", "loaded_successfully")
                    .await?;
                db.update_adapter_memory(adapter_id, handle.memory_bytes() as i64)
                    .await?;
            }
            Err(e) => {
                db.update_adapter_state(adapter_id, "cold", "load_failed")
                    .await?;
                eprintln!("Load failed in cycle {}: {}", cycle, e);
                continue;
            }
        }

        // Brief delay
        sleep(Duration::from_millis(10)).await;

        // Unload
        db.update_adapter_state(adapter_id, "unloading", &format!("cycle_{}_unload", cycle))
            .await?;

        match loader.unload_adapter(0) {
            Ok(_) => {
                db.update_adapter_state(adapter_id, "cold", "unloaded_successfully")
                    .await?;
                db.update_adapter_memory(adapter_id, 0).await?;
            }
            Err(e) => {
                db.update_adapter_state(adapter_id, "warm", "unload_failed")
                    .await?;
                eprintln!("Unload failed in cycle {}: {}", cycle, e);
            }
        }

        // Brief delay between cycles
        sleep(Duration::from_millis(10)).await;
    }

    // Verify final state
    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert!(
        matches!(adapter.current_state.as_str(), "warm" | "cold"),
        "Adapter should be in warm or cold state after cycles"
    );

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Test memory pressure with many concurrent loads
#[tokio::test]
async fn test_memory_pressure_concurrent_loads() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_stress_test")?;

    // Create multiple adapters with larger files to simulate memory pressure
    let num_adapters = 15;
    let file_size = 10 * 1024 * 1024; // 10MB each

    let mut adapter_ids = Vec::new();
    for i in 0..num_adapters {
        let adapter_id = format!("memory-adapter-{}", i);
        let adapter_file = temp_dir.join(format!("{}.safetensors", adapter_id));
        std::fs::write(&adapter_file, vec![0u8; file_size])?;

        let params = AdapterRegistrationBuilder::new()
            .adapter_id(adapter_id.clone())
            .name(format!("Memory Adapter {}", i))
            .hash_b3(format!("memory_hash_{}", i))
            .rank(16)
            .tier(2)
            .build()?;
        db.register_adapter(params).await?;
        adapter_ids.push(adapter_id);
    }

    // Try to load all adapters concurrently
    let mut handles = Vec::new();
    for (idx, adapter_id) in adapter_ids.iter().enumerate() {
        let db_clone = db.clone();
        let adapter_id_clone = adapter_id.clone();
        let temp_dir_clone = temp_dir.clone();
        let adapter_name = format!("memory-adapter-{}", idx);

        let handle = tokio::spawn(async move {
            db_clone
                .update_adapter_state(&adapter_id_clone, "loading", "memory_pressure_load")
                .await?;

            let mut loader = AdapterLoader::new(temp_dir_clone);
            match loader.load_adapter_async(idx as u16, &adapter_name).await {
                Ok(handle) => {
                    db_clone
                        .update_adapter_state(&adapter_id_clone, "warm", "loaded_successfully")
                        .await?;
                    db_clone
                        .update_adapter_memory(&adapter_id_clone, handle.memory_bytes() as i64)
                        .await?;
                    Ok(())
                }
                Err(e) => {
                    db_clone
                        .update_adapter_state(&adapter_id_clone, "cold", "load_failed")
                        .await?;
                    Err(e)
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

    // Verify database consistency
    let warm_adapters = db.list_adapters_by_state("default-tenant", "warm").await?;
    let cold_adapters = db.list_adapters_by_state("default-tenant", "cold").await?;
    let loading_adapters = db
        .list_adapters_by_state("default-tenant", "loading")
        .await?;

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

    // Calculate total memory usage
    let total_memory: i64 = warm_adapters.iter().map(|a| a.memory_bytes).sum();
    println!(
        "Memory pressure test: {} loaded, {} failed, {}MB total memory",
        success_count,
        failure_count,
        total_memory / (1024 * 1024)
    );

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Test operation timeout handling
#[tokio::test]
async fn test_operation_timeout_handling() -> Result<()> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    let temp_dir = new_test_dir("aos_stress_test")?;

    let adapter_id = "timeout-adapter";
    let adapter_file = temp_dir.join("timeout-adapter.safetensors");
    std::fs::write(&adapter_file, vec![0u8; 1024 * 1024])?;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id.to_string())
        .name("Timeout Test Adapter")
        .hash_b3("timeout_hash")
        .rank(16)
        .tier(2)
        .build()?;
    db.register_adapter(params).await?;

    // Start a load operation
    db.update_adapter_state(adapter_id, "loading", "timeout_test")
        .await?;

    // Simulate timeout by setting a very short timeout and blocking operation
    let timeout_ms = 100;
    let db_clone = db.clone();
    let adapter_id_clone = adapter_id.to_string();
    let temp_dir_clone = temp_dir.clone();

    let handle = tokio::spawn(async move {
        // Simulate slow operation
        sleep(Duration::from_millis(timeout_ms * 2)).await;

        let mut loader = AdapterLoader::new(temp_dir_clone);
        match loader.load_adapter_async(0, "timeout-adapter").await {
            Ok(_) => {
                db_clone
                    .update_adapter_state(&adapter_id_clone, "warm", "loaded_successfully")
                    .await
            }
            Err(e) => {
                db_clone
                    .update_adapter_state(&adapter_id_clone, "cold", "load_failed")
                    .await
            }
        }
    });

    // Wait for timeout
    tokio::select! {
        _ = handle => {
            // Operation completed
        }
        _ = sleep(Duration::from_millis(timeout_ms)) => {
            // Timeout occurred - check state
            let adapter = db.get_adapter(adapter_id).await?;
            if let Some(adapter) = adapter {
                // Adapter might still be in loading state, which is acceptable
                // But we should eventually recover
                assert!(
                    matches!(adapter.current_state.as_str(), "loading" | "warm" | "cold"),
                    "Adapter should be in a valid state during timeout"
                );
            }
        }
    }

    // Wait a bit more and verify final state
    sleep(Duration::from_millis(timeout_ms * 3)).await;
    let adapter = db.get_adapter(adapter_id).await?.unwrap();
    assert!(
        matches!(adapter.current_state.as_str(), "warm" | "cold"),
        "Adapter should eventually reach final state"
    );

    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}
