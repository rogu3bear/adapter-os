//! Integration tests for operation tracking functionality
//!
//! Tests operation progress broadcasting and status tracking.
//!
//! Citations:
//! - Operation tracker: [source: crates/adapteros-server-api/src/operation_tracker.rs L1-L50]
//! - Progress broadcasting: [source: crates/adapteros-server-api/src/state.rs L437-438]
//! - SSE handler: [source: crates/adapteros-server-api/src/handlers.rs L9677-9719]

use adapteros_server_api::{operation_tracker::OperationTracker, types::OperationProgressEvent};
use tokio::sync::broadcast;
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_operation_tracker_broadcasts_progress() {
    // Create broadcast channel
    let (progress_tx, mut progress_rx) = broadcast::channel::<OperationProgressEvent>(10);

    // Create operation tracker with progress broadcasting
    let tracker = OperationTracker::new_with_progress(
        Duration::from_secs(300), // 5 minute timeout
        progress_tx,
    );

    // Start an operation
    let operation_id = "test-load-adapter-1";
    tracker.start_operation(operation_id, "adapter", "load").await.unwrap();

    // Update progress
    tracker.update_progress(operation_id, 50.0, Some("Loading model weights...")).await.unwrap();

    // Verify progress event was broadcasted
    let received_event = tokio::time::timeout(
        Duration::from_secs(1),
        progress_rx.recv()
    ).await.unwrap().unwrap();

    assert_eq!(received_event.operation_id, operation_id);
    assert_eq!(received_event.progress_pct, 50.0);
    assert_eq!(received_event.status, "running");
    assert!(received_event.message.as_ref().unwrap().contains("Loading model weights"));
}

#[tokio::test]
async fn test_operation_completion_broadcasts_final_status() {
    // Test that operation completion broadcasts final 100% progress
    let (progress_tx, mut progress_rx) = broadcast::channel::<OperationProgressEvent>(10);
    let tracker = OperationTracker::new_with_progress(Duration::from_secs(300), progress_tx);

    let operation_id = "test-complete-op";
    tracker.start_operation(operation_id, "model", "unload").await.unwrap();

    // Complete operation
    tracker.complete_operation(operation_id, true, Some("Operation completed successfully")).await.unwrap();

    // Verify completion event
    let mut completion_received = false;
    let mut timeout_count = 0;
    while timeout_count < 10 {
        match tokio::time::timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Ok(event)) if event.operation_id == operation_id && event.progress_pct == 100.0 => {
                assert_eq!(event.status, "completed");
                assert!(event.message.as_ref().unwrap().contains("successfully"));
                completion_received = true;
                break;
            }
            Ok(_) => continue, // Other events
            Err(_) => {
                timeout_count += 1;
            }
        }
    }

    assert!(completion_received, "Completion event not received");
}

#[tokio::test]
async fn test_operation_timeout_handling() {
    // Test that operations are cleaned up after timeout
    let (progress_tx, _progress_rx) = broadcast::channel::<OperationProgressEvent>(10);
    let tracker = OperationTracker::new_with_progress(Duration::from_millis(100), progress_tx);

    let operation_id = "test-timeout-op";
    tracker.start_operation(operation_id, "adapter", "load").await.unwrap();

    // Wait for timeout
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify operation is cleaned up
    let status = tracker.get_operation_status(operation_id).await;
    assert!(status.is_none(), "Operation should be cleaned up after timeout");
}

#[tokio::test]
async fn test_operation_status_query() {
    // Test querying operation status
    let (progress_tx, _progress_rx) = broadcast::channel::<OperationProgressEvent>(10);
    let tracker = OperationTracker::new_with_progress(Duration::from_secs(300), progress_tx);

    let operation_id = "test-query-op";
    tracker.start_operation(operation_id, "model", "load").await.unwrap();

    // Query status
    let status = tracker.get_operation_status(operation_id).await;
    assert!(status.is_some());

    let event = status.unwrap();
    assert_eq!(event.operation_id, operation_id);
    assert_eq!(event.operation_type, "load");
    assert_eq!(event.status, "running");
    assert!(event.progress_pct >= 0.0 && event.progress_pct <= 100.0);
}
