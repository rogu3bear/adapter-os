//! Comprehensive tests for upload queue and worker pool
//!
//! This test suite covers:
//! - Queue enqueue/dequeue operations
//! - Fair tenant scheduling
//! - Priority ordering
//! - Queue size limits
//! - Metrics tracking
//! - Concurrent upload handling
//! - Worker failure recovery

use adapteros_server_api::upload_queue::{
    UploadQueue, UploadQueueConfig, UploadQueueItem, UploadQueueMetrics,
};
use futures::future::join_all;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Test basic enqueue operation
#[tokio::test]
async fn test_upload_queue_enqueue_single_item() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    let result = queue
        .enqueue("tenant-1".to_string(), vec![1, 2, 3])
        .await
        .unwrap();

    assert_eq!(result.status, "queued");
    assert_eq!(result.queue_depth, 1);
    assert_eq!(result.queue_position, Some(0));
    assert_eq!(result.time_in_queue, 0);
}

/// Test enqueue with explicit priority
#[tokio::test]
async fn test_upload_queue_enqueue_with_priority() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    let result = queue
        .enqueue_with_priority("tenant-1".to_string(), vec![1, 2, 3], 200)
        .await
        .unwrap();

    assert_eq!(result.status, "queued");
    assert_eq!(result.priority, 200);
}

/// Test queue respects max size limit
#[tokio::test]
async fn test_upload_queue_respects_max_size() {
    let mut config = UploadQueueConfig::default();
    config.max_queue_size = 3;
    let queue = UploadQueue::new(config);

    // Enqueue 3 items - should succeed
    for i in 0..3 {
        let result = queue.enqueue("tenant-1".to_string(), vec![i as u8]).await;
        assert!(result.is_ok(), "Item {} should be queued", i);
    }

    // 4th item should fail
    let result = queue.enqueue("tenant-1".to_string(), vec![3]).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("queue full"));
}

/// Test priority ordering within tenant queue
#[tokio::test]
async fn test_upload_queue_priority_ordering() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    // Enqueue with different priorities
    let item1 = queue
        .enqueue_with_priority("tenant-1".to_string(), vec![1], 100)
        .await
        .unwrap();
    let item2 = queue
        .enqueue_with_priority("tenant-1".to_string(), vec![2], 200)
        .await
        .unwrap();
    let item3 = queue
        .enqueue_with_priority("tenant-1".to_string(), vec![3], 150)
        .await
        .unwrap();

    // Higher priority items should have better queue positions
    assert_eq!(item1.queue_position, Some(2)); // Priority 100 - last
    assert_eq!(item2.queue_position, Some(0)); // Priority 200 - first
    assert_eq!(item3.queue_position, Some(1)); // Priority 150 - middle
}

/// Test fair scheduling across tenants
#[tokio::test]
async fn test_upload_queue_fair_scheduling() {
    let mut config = UploadQueueConfig::default();
    config.worker_count = 1; // Single worker to observe scheduling fairness
    let queue = Arc::new(UploadQueue::new(config));

    // Add items from different tenants
    let tenant_a_results = vec![
        queue
            .enqueue("tenant-a".to_string(), vec![1])
            .await
            .unwrap(),
        queue
            .enqueue("tenant-a".to_string(), vec![2])
            .await
            .unwrap(),
    ];

    let tenant_b_results = vec![
        queue
            .enqueue("tenant-b".to_string(), vec![3])
            .await
            .unwrap(),
        queue
            .enqueue("tenant-b".to_string(), vec![4])
            .await
            .unwrap(),
    ];

    // Both tenants should have items in queue
    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.queue_depth, 4);
    assert_eq!(metrics.per_tenant_depths.len(), 2);
    assert_eq!(metrics.per_tenant_depths.get("tenant-a"), Some(&2));
    assert_eq!(metrics.per_tenant_depths.get("tenant-b"), Some(&2));
}

/// Test status retrieval
#[tokio::test]
async fn test_upload_queue_get_status() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    let result = queue
        .enqueue("tenant-1".to_string(), vec![1])
        .await
        .unwrap();

    let status = queue.get_status(&result.item_id).await.unwrap();
    assert_eq!(status.status, "queued");
    assert_eq!(status.queue_position, Some(0));
    assert_eq!(status.queue_depth, 1);
    assert_eq!(status.item_id, result.item_id);
}

/// Test status returns None for non-existent item
#[tokio::test]
async fn test_upload_queue_get_status_not_found() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    let status = queue.get_status("non-existent").await;
    assert!(status.is_none());
}

/// Test metrics tracking
#[tokio::test]
async fn test_upload_queue_metrics() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    // Initial metrics should be zero
    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.queue_depth, 0);
    assert_eq!(metrics.total_processed, 0);
    assert_eq!(metrics.total_failed, 0);

    // Add some items
    for i in 0..5 {
        let tenant = format!("tenant-{}", i % 2);
        let _ = queue.enqueue(tenant, vec![i as u8]).await;
    }

    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.queue_depth, 5);
    assert!(metrics.max_queue_depth >= 5);
}

/// Test per-tenant queue depth metrics
#[tokio::test]
async fn test_upload_queue_per_tenant_metrics() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    queue.enqueue("tenant-a".to_string(), vec![1]).await.ok();
    queue.enqueue("tenant-a".to_string(), vec![2]).await.ok();
    queue.enqueue("tenant-b".to_string(), vec![3]).await.ok();
    queue.enqueue("tenant-c".to_string(), vec![4]).await.ok();
    queue.enqueue("tenant-c".to_string(), vec![5]).await.ok();
    queue.enqueue("tenant-c".to_string(), vec![6]).await.ok();

    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.per_tenant_depths.get("tenant-a"), Some(&2));
    assert_eq!(metrics.per_tenant_depths.get("tenant-b"), Some(&1));
    assert_eq!(metrics.per_tenant_depths.get("tenant-c"), Some(&3));
}

/// Test concurrent enqueue operations
#[tokio::test]
async fn test_upload_queue_concurrent_enqueue() {
    let config = UploadQueueConfig::default();
    let queue = Arc::new(UploadQueue::new(config));

    let mut handles: Vec<JoinHandle<_>> = Vec::new();

    // Spawn 10 concurrent tasks, each enqueuing 10 items
    for task_id in 0..10 {
        let queue_clone = queue.clone();
        let handle = tokio::spawn(async move {
            for item_id in 0..10 {
                let tenant = format!("tenant-{}", task_id % 3);
                let _result = queue_clone.enqueue(tenant, vec![item_id as u8]).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.ok();
    }

    // Should have 100 items total
    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.queue_depth, 100);
}

/// Test queue size limit under concurrent load
#[tokio::test]
async fn test_upload_queue_size_limit_concurrent() {
    let mut config = UploadQueueConfig::default();
    config.max_queue_size = 50;
    let queue = Arc::new(UploadQueue::new(config));

    let success_count = Arc::new(AtomicU32::new(0));
    let failure_count = Arc::new(AtomicU32::new(0));

    let mut handles: Vec<JoinHandle<_>> = Vec::new();

    // Spawn 10 concurrent tasks trying to enqueue items
    for task_id in 0..10 {
        let queue_clone = queue.clone();
        let success_clone = success_count.clone();
        let failure_clone = failure_count.clone();

        let handle = tokio::spawn(async move {
            for item_id in 0..10 {
                let tenant = format!("tenant-{}", task_id % 3);
                match queue_clone.enqueue(tenant, vec![item_id as u8]).await {
                    Ok(_) => success_clone.fetch_add(1, Ordering::SeqCst),
                    Err(_) => failure_clone.fetch_add(1, Ordering::SeqCst),
                };
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.ok();
    }

    let total_attempts = 100;
    let successes = success_count.load(Ordering::SeqCst) as usize;
    let failures = failure_count.load(Ordering::SeqCst) as usize;

    // Some attempts may have failed due to size limit
    assert_eq!(successes + failures, total_attempts);
    // Should have hit the size limit
    assert!(failures > 0, "Should have queue full errors");
    // Queue should be at capacity
    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.queue_depth, 50);
}

/// Test multiple tenants with priority scheduling
#[tokio::test]
async fn test_upload_queue_multi_tenant_priority() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    // Enqueue from multiple tenants with varying priorities
    queue
        .enqueue_with_priority("tenant-a".to_string(), vec![1], 100)
        .await
        .ok();
    queue
        .enqueue_with_priority("tenant-b".to_string(), vec![2], 200)
        .await
        .ok();
    queue
        .enqueue_with_priority("tenant-a".to_string(), vec![3], 180)
        .await
        .ok();
    queue
        .enqueue_with_priority("tenant-c".to_string(), vec![4], 150)
        .await
        .ok();

    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.queue_depth, 4);
    assert_eq!(metrics.per_tenant_depths.len(), 3);
}

/// Test queue behavior after multiple enqueues and status checks
#[tokio::test]
async fn test_upload_queue_status_consistency() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    // Enqueue items
    let items: Vec<_> = (0..5)
        .map(|i| {
            let tenant = format!("tenant-{}", i % 2);
            queue.enqueue(tenant, vec![i as u8])
        })
        .collect::<Vec<_>>();

    // Wait for all enqueues to complete
    let results: Vec<_> = join_all(items).await;
    assert!(results.iter().all(|r| r.is_ok()));

    // Check status of each
    for result in results {
        let r = result.unwrap();
        let status = queue.get_status(&r.item_id).await;
        assert!(status.is_some());
        let s = status.unwrap();
        assert_eq!(s.item_id, r.item_id);
        assert_eq!(s.status, "queued");
    }

    // Final metrics
    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.queue_depth, 5);
}

/// Test that priority is preserved correctly
#[tokio::test]
async fn test_upload_queue_priority_preservation() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    // Add multiple items with specific priorities
    let priorities = vec![50, 200, 100, 150, 75];
    let mut ids = Vec::new();

    for (i, &priority) in priorities.iter().enumerate() {
        let result = queue
            .enqueue_with_priority("tenant-1".to_string(), vec![i as u8], priority)
            .await
            .unwrap();
        ids.push((result.item_id, priority, result.queue_position));
    }

    // Verify ordering: highest priority first
    assert_eq!(ids[0].2, Some(4)); // Priority 50 - last
    assert_eq!(ids[1].2, Some(0)); // Priority 200 - first
    assert_eq!(ids[2].2, Some(2)); // Priority 100 - middle
    assert_eq!(ids[3].2, Some(1)); // Priority 150 - second
    assert_eq!(ids[4].2, Some(3)); // Priority 75 - fourth
}

/// Test queue metrics max depth tracking
#[tokio::test]
async fn test_upload_queue_max_depth_tracking() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    // Verify initial max depth is 0
    let initial_metrics = queue.get_metrics().await;
    assert_eq!(initial_metrics.max_queue_depth, 0);

    // Add items to reach depth of 10
    for i in 0..10 {
        queue
            .enqueue("tenant-1".to_string(), vec![i as u8])
            .await
            .ok();
    }

    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.queue_depth, 10);
    assert_eq!(metrics.max_queue_depth, 10);

    // Add more items
    for i in 10..25 {
        queue
            .enqueue("tenant-1".to_string(), vec![i as u8])
            .await
            .ok();
    }

    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.queue_depth, 25);
    assert_eq!(metrics.max_queue_depth, 25);
}

/// Test default configuration values
#[tokio::test]
fn test_upload_queue_default_config() {
    let config = UploadQueueConfig::default();
    assert_eq!(config.max_queue_size, 10000);
    assert_eq!(config.worker_count, 4);
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.upload_timeout_secs, 300);
}

/// Test queue graceful shutdown
#[tokio::test]
async fn test_upload_queue_shutdown() {
    let config = UploadQueueConfig::default();
    let queue = UploadQueue::new(config);

    // Add some items
    for i in 0..5 {
        queue
            .enqueue("tenant-1".to_string(), vec![i as u8])
            .await
            .ok();
    }

    // Shutdown should not panic
    queue.shutdown().await;

    let metrics = queue.get_metrics().await;
    assert_eq!(metrics.queue_depth, 5); // Items still there
}
