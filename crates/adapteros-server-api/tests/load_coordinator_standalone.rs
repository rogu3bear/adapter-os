//! Standalone tests for LoadCoordinator
//!
//! These tests verify the thundering herd protection functionality.

use adapteros_core::AosError;
use adapteros_lora_lifecycle::loader::{AdapterHandle, AdapterMetadata};
use adapteros_server_api::LoadCoordinator;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

fn create_test_handle(id: u16) -> AdapterHandle {
    AdapterHandle {
        adapter_id: id,
        path: PathBuf::from(format!("/test/adapter_{}.aos", id)),
        memory_bytes: 1024 * 1024,
        metadata: AdapterMetadata {
            num_parameters: 1000,
            rank: Some(8),
            target_modules: vec!["q_proj".to_string(), "v_proj".to_string()],
            ..Default::default()
        },
    }
}

#[tokio::test]
async fn test_single_request() {
    let coordinator = LoadCoordinator::new();
    let load_count = Arc::new(AtomicU32::new(0));
    let load_count_clone = load_count.clone();

    let result = coordinator
        .load_or_wait("test-adapter", || async move {
            load_count_clone.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            Ok(create_test_handle(42))
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(load_count.load(Ordering::SeqCst), 1);
    assert_eq!(result.unwrap().adapter_id, 42);
}

#[tokio::test]
async fn test_concurrent_requests_coalesce() {
    let coordinator = Arc::new(LoadCoordinator::new());
    let load_count = Arc::new(AtomicU32::new(0));

    let mut handles = vec![];

    // Spawn 10 concurrent requests
    for _ in 0..10 {
        let coord = coordinator.clone();
        let count = load_count.clone();
        let handle = tokio::spawn(async move {
            coord
                .load_or_wait("test-adapter", || async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    Ok(create_test_handle(42))
                })
                .await
        });
        handles.push(handle);
    }

    // Wait for all requests to complete
    let results: Vec<_> = futures::future::join_all(handles).await;

    // All should succeed
    for result in results {
        let adapter = result.unwrap().unwrap();
        assert_eq!(adapter.adapter_id, 42);
    }

    // Load should only have been called once
    assert_eq!(load_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_error_propagation() {
    let coordinator = Arc::new(LoadCoordinator::new());

    let mut handles = vec![];

    // Spawn 5 concurrent requests that will fail
    for _ in 0..5 {
        let coord = coordinator.clone();
        let handle = tokio::spawn(async move {
            coord
                .load_or_wait("failing-adapter", || async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
                    Err(AosError::Lifecycle("Test failure".to_string()))
                })
                .await
        });
        handles.push(handle);
    }

    // Wait for all requests
    let results: Vec<_> = futures::future::join_all(handles).await;

    // All should receive the error
    for result in results {
        let err = result.unwrap().unwrap_err();
        assert!(matches!(err, AosError::Lifecycle(_)));
    }
}

#[tokio::test]
async fn test_is_loading() {
    let coordinator = Arc::new(LoadCoordinator::new());
    let started = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();

    assert!(!coordinator.is_loading("test-adapter"));

    // Start a load in background
    let coord_clone = coordinator.clone();
    let load_handle = tokio::spawn(async move {
        coord_clone
            .load_or_wait("test-adapter", || async move {
                started_clone.store(true, Ordering::SeqCst);
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                Ok(create_test_handle(42))
            })
            .await
    });

    // Wait for load to start
    while !started.load(Ordering::SeqCst) {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }

    assert!(coordinator.is_loading("test-adapter"));
    assert_eq!(coordinator.waiter_count("test-adapter"), 1);

    // Wait for load to complete
    load_handle.await.unwrap().unwrap();

    assert!(!coordinator.is_loading("test-adapter"));
    assert_eq!(coordinator.waiter_count("test-adapter"), 0);
}

#[tokio::test]
async fn test_metrics() {
    let coordinator = Arc::new(LoadCoordinator::new());

    // No pending loads initially
    let metrics = coordinator.metrics();
    assert_eq!(metrics.pending_loads, 0);
    assert_eq!(metrics.total_waiters, 0);

    // Start multiple loads
    let coord_clone1 = coordinator.clone();
    let handle1 = tokio::spawn(async move {
        coord_clone1
            .load_or_wait("adapter-1", || async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                Ok(create_test_handle(1))
            })
            .await
    });

    let coord_clone2 = coordinator.clone();
    let handle2 = tokio::spawn(async move {
        coord_clone2
            .load_or_wait("adapter-2", || async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                Ok(create_test_handle(2))
            })
            .await
    });

    // Wait for loads to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let metrics = coordinator.metrics();
    assert_eq!(metrics.pending_loads, 2);
    assert_eq!(metrics.total_waiters, 2);
    assert!(metrics.oldest_load_age_ms > 0);

    // Wait for completion
    handle1.await.unwrap().unwrap();
    handle2.await.unwrap().unwrap();

    let metrics = coordinator.metrics();
    assert_eq!(metrics.pending_loads, 0);
    assert_eq!(metrics.total_waiters, 0);
}

#[tokio::test]
async fn test_sequential_loads_same_model() {
    let coordinator = LoadCoordinator::new();
    let load_count = Arc::new(AtomicU32::new(0));

    // First load
    let count1 = load_count.clone();
    let result1 = coordinator
        .load_or_wait("test-adapter", || async move {
            count1.fetch_add(1, Ordering::SeqCst);
            Ok(create_test_handle(42))
        })
        .await;
    assert!(result1.is_ok());

    // Second load (not concurrent, should trigger new load)
    let count2 = load_count.clone();
    let result2 = coordinator
        .load_or_wait("test-adapter", || async move {
            count2.fetch_add(1, Ordering::SeqCst);
            Ok(create_test_handle(43))
        })
        .await;
    assert!(result2.is_ok());

    // Both loads should have been called
    assert_eq!(load_count.load(Ordering::SeqCst), 2);
    assert_eq!(result2.unwrap().adapter_id, 43);
}

#[tokio::test]
async fn test_waiter_count_increases() {
    let coordinator = Arc::new(LoadCoordinator::new());
    let started = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();

    // Start first load
    let coord1 = coordinator.clone();
    let _load1 = tokio::spawn(async move {
        coord1
            .load_or_wait("test-adapter", || async move {
                started_clone.store(true, Ordering::SeqCst);
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                Ok(create_test_handle(42))
            })
            .await
    });

    // Wait for first load to start
    while !started.load(Ordering::SeqCst) {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }

    assert_eq!(coordinator.waiter_count("test-adapter"), 1);

    // Start second waiter
    let coord2 = coordinator.clone();
    let _load2 = tokio::spawn(async move {
        coord2
            .load_or_wait("test-adapter", || async move { Ok(create_test_handle(42)) })
            .await
    });

    // Give it a moment to register
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    assert_eq!(coordinator.waiter_count("test-adapter"), 2);
}
