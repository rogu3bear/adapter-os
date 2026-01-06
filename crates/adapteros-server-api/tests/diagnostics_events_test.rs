//! Integration tests for diagnostic events in SQLite.
//!
//! Tests that stage events are properly captured during inference.

use adapteros_diagnostics::{
    DiagEnvelope, DiagEvent, DiagRunId, DiagSeverity, DiagStage, DiagnosticsConfig,
    DiagnosticsService, StageGuard,
};
use adapteros_telemetry::tracing::TraceContext;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Test that StageGuard emits enter and exit events on successful completion.
#[tokio::test]
async fn test_stage_guard_success_flow() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: adapteros_diagnostics::DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let trace_ctx = TraceContext::new_root();
    let run_id = DiagRunId::new_random();

    service.start_run(&run_id);

    // Create and complete a stage guard
    {
        let guard = StageGuard::new(
            Arc::clone(&service),
            DiagStage::RequestValidation,
            &trace_ctx,
            &run_id,
            "test-tenant",
        );
        guard.complete();
    }

    // Verify StageEnter event
    let enter = receiver.recv().await.unwrap();
    assert!(matches!(
        enter.payload,
        DiagEvent::StageEnter {
            stage: DiagStage::RequestValidation
        }
    ));
    assert_eq!(enter.tenant_id, "test-tenant");

    // Verify StageExit event with ok=true
    let exit = receiver.recv().await.unwrap();
    match exit.payload {
        DiagEvent::StageExit {
            stage,
            ok,
            error_code,
            duration_us,
        } => {
            assert_eq!(stage, DiagStage::RequestValidation);
            assert!(ok);
            assert!(error_code.is_none());
            assert!(duration_us > 0);
        }
        _ => panic!("Expected StageExit event, got {:?}", exit.payload),
    }
}

/// Test that StageGuard emits exit with error on failure.
#[tokio::test]
async fn test_stage_guard_failure_flow() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: adapteros_diagnostics::DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let trace_ctx = TraceContext::new_root();
    let run_id = DiagRunId::new_random();

    service.start_run(&run_id);

    // Create and fail a stage guard
    {
        let guard = StageGuard::new(
            Arc::clone(&service),
            DiagStage::WorkerInference,
            &trace_ctx,
            &run_id,
            "test-tenant",
        );
        guard.fail("E2001");
    }

    // Skip StageEnter
    let _ = receiver.recv().await.unwrap();

    // Verify StageExit event with ok=false
    let exit = receiver.recv().await.unwrap();
    match exit.payload {
        DiagEvent::StageExit {
            stage,
            ok,
            error_code,
            ..
        } => {
            assert_eq!(stage, DiagStage::WorkerInference);
            assert!(!ok);
            assert_eq!(error_code, Some("E2001".to_string()));
        }
        _ => panic!("Expected StageExit event, got {:?}", exit.payload),
    }
}

/// Test that StageGuard emits exit on panic/early drop.
#[tokio::test]
async fn test_stage_guard_drop_without_complete() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: adapteros_diagnostics::DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let trace_ctx = TraceContext::new_root();
    let run_id = DiagRunId::new_random();

    service.start_run(&run_id);

    // Create a stage guard but don't call complete() or fail()
    {
        let _guard = StageGuard::new(
            Arc::clone(&service),
            DiagStage::AdapterResolution,
            &trace_ctx,
            &run_id,
            "test-tenant",
        );
        // Guard will be dropped here without complete/fail
    }

    // Skip StageEnter
    let _ = receiver.recv().await.unwrap();

    // Verify StageExit with ok=false (incomplete)
    let exit = receiver.recv().await.unwrap();
    match exit.payload {
        DiagEvent::StageExit {
            stage,
            ok,
            error_code,
            ..
        } => {
            assert_eq!(stage, DiagStage::AdapterResolution);
            assert!(
                !ok,
                "StageExit should indicate failure when guard is dropped without complete"
            );
            assert!(
                error_code.is_none(),
                "No error code should be set for incomplete drop"
            );
        }
        _ => panic!("Expected StageExit event, got {:?}", exit.payload),
    }
}

/// Test RunStarted and RunFinished events.
#[tokio::test]
async fn test_run_lifecycle_events() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: adapteros_diagnostics::DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let trace_ctx = TraceContext::new_root();
    let run_id = DiagRunId::new_random();

    service.start_run(&run_id);

    // Emit RunStarted
    let started = DiagEnvelope::new(
        &trace_ctx,
        "test-tenant",
        run_id.clone(),
        DiagSeverity::Info,
        0,
        DiagEvent::RunStarted {
            request_id: "req-123".to_string(),
            is_replay: false,
        },
    );
    service.emit(started).expect("emit should succeed");

    // Emit RunFinished
    let finished = DiagEnvelope::new(
        &trace_ctx,
        "test-tenant",
        run_id.clone(),
        DiagSeverity::Info,
        1000,
        DiagEvent::RunFinished {
            request_id: "req-123".to_string(),
            total_duration_us: 1000,
        },
    );
    service.emit(finished).expect("emit should succeed");

    // Verify RunStarted
    let started_event = receiver.recv().await.unwrap();
    match started_event.payload {
        DiagEvent::RunStarted {
            request_id,
            is_replay,
        } => {
            assert_eq!(request_id, "req-123");
            assert!(!is_replay);
        }
        _ => panic!("Expected RunStarted event"),
    }

    // Verify RunFinished
    let finished_event = receiver.recv().await.unwrap();
    match finished_event.payload {
        DiagEvent::RunFinished {
            request_id,
            total_duration_us,
        } => {
            assert_eq!(request_id, "req-123");
            assert_eq!(total_duration_us, 1000);
        }
        _ => panic!("Expected RunFinished event"),
    }
}

/// Test RunFailed event.
#[tokio::test]
async fn test_run_failed_event() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: adapteros_diagnostics::DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let trace_ctx = TraceContext::new_root();
    let run_id = DiagRunId::new_random();

    service.start_run(&run_id);

    // Emit RunFailed
    let failed = DiagEnvelope::new(
        &trace_ctx,
        "test-tenant",
        run_id.clone(),
        DiagSeverity::Error,
        500,
        DiagEvent::RunFailed {
            request_id: "req-456".to_string(),
            error_code: "E2001".to_string(),
            recovery_action: Some("Retry after worker restart".to_string()),
        },
    );
    service.emit(failed).expect("emit should succeed");

    // Verify RunFailed
    let failed_event = receiver.recv().await.unwrap();
    match failed_event.payload {
        DiagEvent::RunFailed {
            request_id,
            error_code,
            recovery_action,
        } => {
            assert_eq!(request_id, "req-456");
            assert_eq!(error_code, "E2001");
            assert_eq!(
                recovery_action,
                Some("Retry after worker restart".to_string())
            );
        }
        _ => panic!("Expected RunFailed event"),
    }
}

/// Test StreamClosed event.
#[tokio::test]
async fn test_stream_closed_event() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: adapteros_diagnostics::DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let trace_ctx = TraceContext::new_root();
    let run_id = DiagRunId::new_random();

    service.start_run(&run_id);

    // Emit StreamClosed
    let closed = DiagEnvelope::new(
        &trace_ctx,
        "test-tenant",
        run_id.clone(),
        DiagSeverity::Info,
        250,
        DiagEvent::StreamClosed {
            request_id: "stream-789".to_string(),
            reason: "complete".to_string(),
        },
    );
    service.emit(closed).expect("emit should succeed");

    // Verify StreamClosed
    let closed_event = receiver.recv().await.unwrap();
    match closed_event.payload {
        DiagEvent::StreamClosed { request_id, reason } => {
            assert_eq!(request_id, "stream-789");
            assert_eq!(reason, "complete");
        }
        _ => panic!("Expected StreamClosed event"),
    }
}

/// Test multiple stages in sequence (simulates inference pipeline).
#[tokio::test]
async fn test_multiple_stages_sequence() {
    let config = DiagnosticsConfig {
        enabled: true,
        level: adapteros_diagnostics::DiagLevel::Tokens,
        channel_capacity: 100,
        ..Default::default()
    };
    let (service, mut receiver) = DiagnosticsService::new(config);
    let service = Arc::new(service);

    let trace_ctx = TraceContext::new_root();
    let run_id = DiagRunId::new_random();

    service.start_run(&run_id);

    // Stage 1: RequestValidation
    {
        let guard = StageGuard::new(
            Arc::clone(&service),
            DiagStage::RequestValidation,
            &trace_ctx,
            &run_id,
            "test-tenant",
        );
        guard.complete();
    }

    // Stage 2: AdapterResolution
    {
        let guard = StageGuard::new(
            Arc::clone(&service),
            DiagStage::AdapterResolution,
            &trace_ctx,
            &run_id,
            "test-tenant",
        );
        guard.complete();
    }

    // Stage 3: WorkerInference
    {
        let guard = StageGuard::new(
            Arc::clone(&service),
            DiagStage::WorkerInference,
            &trace_ctx,
            &run_id,
            "test-tenant",
        );
        guard.complete();
    }

    // Verify we have 6 events (3 enter + 3 exit)
    let mut events = Vec::new();
    for _ in 0..6 {
        events.push(receiver.recv().await.unwrap());
    }

    // Check stages appear in order
    let stages: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.payload {
            DiagEvent::StageEnter { stage } => Some(*stage),
            DiagEvent::StageExit { stage, ok, .. } => {
                assert!(ok, "All stages should complete successfully");
                Some(*stage)
            }
            _ => None,
        })
        .collect();

    assert_eq!(stages.len(), 6);
    assert_eq!(stages[0], DiagStage::RequestValidation); // Enter
    assert_eq!(stages[1], DiagStage::RequestValidation); // Exit
    assert_eq!(stages[2], DiagStage::AdapterResolution); // Enter
    assert_eq!(stages[3], DiagStage::AdapterResolution); // Exit
    assert_eq!(stages[4], DiagStage::WorkerInference); // Enter
    assert_eq!(stages[5], DiagStage::WorkerInference); // Exit
}
