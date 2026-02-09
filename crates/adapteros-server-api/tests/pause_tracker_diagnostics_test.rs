//! Tests for ServerPauseTracker diagnostic event emission.
//!
//! Verifies that InferencePaused and InferenceResumed events are emitted
//! when pauses are registered and reviews are submitted.

#![allow(clippy::useless_vec)]

use std::path::PathBuf;
use std::sync::Arc;

use adapteros_api_types::review::{PauseKind, Review, ReviewAssessment, SubmitReviewRequest};
use adapteros_core::B3Hash;
use adapteros_server_api::pause_tracker::ServerPauseTracker;
use adapteros_server_api::uds_client::WorkerStreamPaused;
use adapteros_telemetry::diagnostics::{
    DiagEvent, DiagLevel, DiagnosticsConfig, DiagnosticsService,
};

// =============================================================================
// Test: Pause Registration Emits Diagnostic Event
// =============================================================================

#[tokio::test]
async fn test_pause_registration_emits_diagnostic() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let tracker = ServerPauseTracker::new().with_diagnostics(Arc::clone(&service));

    // Register a paused inference
    let pause_event = WorkerStreamPaused {
        pause_id: "pause-diag-001".to_string(),
        inference_id: "infer-diag-001".to_string(),
        trigger_kind: "ExplicitTag".to_string(),
        context: Some("Review this generated code".to_string()),
        text_so_far: Some("fn calculate() { /* ... */ }".to_string()),
        token_count: 42,
    };

    tracker.register_pause(
        "tenant-1".to_string(),
        pause_event,
        PathBuf::from("var/run/worker.sock"),
    );

    // Verify InferencePaused event was emitted
    let envelope = receiver
        .recv()
        .await
        .expect("should receive InferencePaused event");

    match envelope.payload {
        DiagEvent::InferencePaused {
            pause_id,
            inference_id,
            pause_kind,
            trigger_kind,
            context_hash,
            token_count,
        } => {
            assert_eq!(pause_id, "pause-diag-001");
            assert_eq!(inference_id, "infer-diag-001");
            assert_eq!(pause_kind, "ReviewNeeded");
            assert_eq!(trigger_kind, Some("ExplicitTag".to_string()));
            assert_eq!(token_count, 42);
            // Verify context was hashed, not stored raw
            assert_ne!(context_hash, B3Hash::default());
        }
        other => panic!("Expected InferencePaused event, got {:?}", other),
    }
}

// =============================================================================
// Test: Pause Registration with Different Trigger Kinds
// =============================================================================

#[tokio::test]
async fn test_pause_trigger_kind_mapping() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let tracker = ServerPauseTracker::new().with_diagnostics(Arc::clone(&service));

    // Test various trigger kinds
    let test_cases = vec![
        ("ExplicitTag", "ReviewNeeded"),
        ("UncertaintySignal", "ReviewNeeded"),
        ("ComplexityThreshold", "ReviewNeeded"),
        ("policy", "PolicyApproval"),
        ("resource", "ResourceWait"),
        ("manual", "UserRequested"),
    ];

    for (i, (trigger, expected_kind)) in test_cases.iter().enumerate() {
        let pause_event = WorkerStreamPaused {
            pause_id: format!("pause-kind-{}", i),
            inference_id: format!("infer-kind-{}", i),
            trigger_kind: trigger.to_string(),
            context: None,
            text_so_far: None,
            token_count: 0,
        };

        tracker.register_pause(
            "tenant-1".to_string(),
            pause_event,
            PathBuf::from("var/run/worker.sock"),
        );

        let envelope = receiver.recv().await.expect("should receive event");
        match envelope.payload {
            DiagEvent::InferencePaused { pause_kind, .. } => {
                assert_eq!(
                    pause_kind, *expected_kind,
                    "Trigger '{}' should map to kind '{}'",
                    trigger, expected_kind
                );
            }
            other => panic!("Expected InferencePaused, got {:?}", other),
        }
    }
}

// =============================================================================
// Test: Pause Listing Returns Correct Info
// =============================================================================

#[tokio::test]
async fn test_pause_listing() {
    let tracker = ServerPauseTracker::new();

    // Register multiple pauses
    for i in 1..=3 {
        let pause_event = WorkerStreamPaused {
            pause_id: format!("pause-list-{:03}", i),
            inference_id: format!("infer-list-{:03}", i),
            trigger_kind: "ExplicitTag".to_string(),
            context: Some(format!("Context for pause {}", i)),
            text_so_far: Some(format!("Code fragment {}", i)),
            token_count: i * 10,
        };
        tracker.register_pause(
            "tenant-1".to_string(),
            pause_event,
            PathBuf::from("var/run/worker.sock"),
        );
    }

    // Verify listing
    let paused = tracker.list_paused();
    assert_eq!(paused.len(), 3);

    // Verify count
    assert_eq!(tracker.count(), 3);

    // Verify each can be queried
    for i in 1..=3 {
        let state = tracker.get_state_by_pause_id(&format!("pause-list-{:03}", i));
        assert!(state.is_some(), "Pause {} should exist", i);
        let info = state.unwrap();
        assert_eq!(info.kind, PauseKind::ReviewNeeded);
        assert_eq!(info.token_count, i * 10);
    }
}

// =============================================================================
// Test: Query by Inference ID
// =============================================================================

#[tokio::test]
async fn test_query_by_inference_id() {
    let tracker = ServerPauseTracker::new();

    let pause_event = WorkerStreamPaused {
        pause_id: "pause-query-001".to_string(),
        inference_id: "unique-infer-id-12345".to_string(),
        trigger_kind: "UncertaintySignal".to_string(),
        context: Some("Uncertainty in output".to_string()),
        text_so_far: Some("Generated content".to_string()),
        token_count: 25,
    };

    tracker.register_pause(
        "tenant-1".to_string(),
        pause_event,
        PathBuf::from("var/run/worker.sock"),
    );

    // Query by inference ID
    let state = tracker.get_state_by_inference("unique-infer-id-12345");
    assert!(state.is_some());

    let info = state.unwrap();
    assert_eq!(info.pause_id, "pause-query-001");
    assert_eq!(info.inference_id, "unique-infer-id-12345");
    assert!(info.text_so_far.is_some());
    assert_eq!(info.text_so_far.unwrap(), "Generated content");

    // Query non-existent inference
    let not_found = tracker.get_state_by_inference("nonexistent-infer-id");
    assert!(not_found.is_none());
}

// =============================================================================
// Test: Pause Removal
// =============================================================================

#[tokio::test]
async fn test_pause_removal() {
    let tracker = ServerPauseTracker::new();

    // Register a pause
    let pause_event = WorkerStreamPaused {
        pause_id: "pause-remove-001".to_string(),
        inference_id: "infer-remove-001".to_string(),
        trigger_kind: "ExplicitTag".to_string(),
        context: None,
        text_so_far: None,
        token_count: 0,
    };

    tracker.register_pause(
        "tenant-1".to_string(),
        pause_event,
        PathBuf::from("var/run/worker.sock"),
    );
    assert_eq!(tracker.count(), 1);

    // Remove the pause
    tracker.remove("pause-remove-001");
    assert_eq!(tracker.count(), 0);

    // Verify it's gone
    assert!(tracker.get_state_by_pause_id("pause-remove-001").is_none());
}

// =============================================================================
// Test: Review Submission Error - Pause Not Found
// =============================================================================

#[tokio::test]
async fn test_review_submission_pause_not_found() {
    let tracker = ServerPauseTracker::new();

    // Try to submit review for non-existent pause
    let review_request = SubmitReviewRequest {
        pause_id: "nonexistent-pause".to_string(),
        review: Review {
            assessment: ReviewAssessment::Approved,
            issues: vec![],
            suggestions: vec![],
            comments: None,
            confidence: None,
        },
        reviewer: "test-reviewer".to_string(),
    };

    let result = tracker.submit_review(review_request).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("No paused inference found"),
        "Error should mention pause not found, got: {}",
        err
    );
    assert!(
        err.to_string().contains("nonexistent-pause"),
        "Error should include pause_id, got: {}",
        err
    );
}

// =============================================================================
// Test: Context Hash Determinism
// =============================================================================

#[tokio::test]
async fn test_context_hash_determinism() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let tracker = ServerPauseTracker::new().with_diagnostics(Arc::clone(&service));

    // Register with same context twice
    let context = "Identical context for hashing test".to_string();

    for i in 1..=2 {
        let pause_event = WorkerStreamPaused {
            pause_id: format!("pause-hash-{}", i),
            inference_id: format!("infer-hash-{}", i),
            trigger_kind: "ExplicitTag".to_string(),
            context: Some(context.clone()),
            text_so_far: None,
            token_count: 0,
        };
        tracker.register_pause(
            "tenant-1".to_string(),
            pause_event,
            PathBuf::from("var/run/worker.sock"),
        );
    }

    // Get both events
    let event1 = receiver.recv().await.expect("first event");
    let event2 = receiver.recv().await.expect("second event");

    // Extract context hashes
    let hash1 = match event1.payload {
        DiagEvent::InferencePaused { context_hash, .. } => context_hash,
        _ => panic!("Expected InferencePaused"),
    };
    let hash2 = match event2.payload {
        DiagEvent::InferencePaused { context_hash, .. } => context_hash,
        _ => panic!("Expected InferencePaused"),
    };

    // Same context should produce same hash
    assert_eq!(hash1, hash2, "Same context should produce identical hash");
}

// =============================================================================
// Test: Empty Context Hash
// =============================================================================

#[tokio::test]
async fn test_empty_context_hash() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let tracker = ServerPauseTracker::new().with_diagnostics(Arc::clone(&service));

    // Register with no context
    let pause_event = WorkerStreamPaused {
        pause_id: "pause-empty-ctx".to_string(),
        inference_id: "infer-empty-ctx".to_string(),
        trigger_kind: "ExplicitTag".to_string(),
        context: None,
        text_so_far: None,
        token_count: 0,
    };

    tracker.register_pause(
        "tenant-1".to_string(),
        pause_event,
        PathBuf::from("var/run/worker.sock"),
    );

    let envelope = receiver.recv().await.expect("should receive event");
    match envelope.payload {
        DiagEvent::InferencePaused { context_hash, .. } => {
            // Empty string hash should be consistent
            let expected_hash = B3Hash::hash(b"");
            assert_eq!(context_hash, expected_hash);
        }
        _ => panic!("Expected InferencePaused"),
    }
}
