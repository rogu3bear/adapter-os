//! RAII stage guard for automatic stage enter/exit events.
//!
//! The `StageGuard` ensures that stage exit events are always emitted,
//! even if the stage fails or panics. It uses Drop semantics to guarantee
//! proper cleanup.

use super::{DiagEnvelope, DiagEvent, DiagRunId, DiagSeverity, DiagStage, DiagnosticsService};
use crate::tracing::TraceContext;
use std::cell::Cell;
use std::sync::Arc;
use std::time::Instant;

/// RAII guard that emits StageEnter on creation and StageExit on drop.
///
/// The guard tracks whether the stage completed successfully or failed.
/// On drop, it emits a `StageExit` event with:
/// - `ok: true` if `complete()` was called
/// - `ok: false` with error_code if `fail()` was called or the guard is dropped without completing
///
/// # Example
///
/// ```ignore
/// let guard = StageGuard::new(
///     &service,
///     DiagStage::RequestValidation,
///     &trace_ctx,
///     &run_id,
///     "tenant-123",
/// );
///
/// // Do work...
/// if success {
///     guard.complete();
/// } else {
///     guard.fail("E1001");
/// }
/// // StageExit is emitted on drop
/// ```
pub struct StageGuard {
    service: Arc<DiagnosticsService>,
    stage: DiagStage,
    trace_context: TraceContext,
    run_id: DiagRunId,
    tenant_id: String,
    start_time: Instant,
    /// Marked true once complete() or fail() is called
    finished: Cell<bool>,
    /// Error code if fail() was called
    error_code: Cell<Option<String>>,
}

impl StageGuard {
    /// Create a new StageGuard, emitting a StageEnter event.
    ///
    /// The guard will emit a StageExit event when dropped.
    pub fn new(
        service: Arc<DiagnosticsService>,
        stage: DiagStage,
        trace_context: &TraceContext,
        run_id: &DiagRunId,
        tenant_id: impl Into<String>,
    ) -> Self {
        let tenant_id = tenant_id.into();
        let start_time = Instant::now();

        // Emit StageEnter event
        let enter_envelope = DiagEnvelope::new(
            trace_context,
            &tenant_id,
            run_id.clone(),
            DiagSeverity::Info,
            0, // mono_us will be relative to run start
            DiagEvent::StageEnter { stage },
        );
        let _ = service.emit(enter_envelope);

        Self {
            service,
            stage,
            trace_context: trace_context.clone(),
            run_id: run_id.clone(),
            tenant_id,
            start_time,
            finished: Cell::new(false),
            error_code: Cell::new(None),
        }
    }

    /// Mark the stage as successfully completed.
    ///
    /// Call this when the stage finishes successfully.
    /// The StageExit event on drop will have `ok: true`.
    pub fn complete(self) {
        self.finished.set(true);
        // Drop will emit the StageExit event
    }

    /// Mark the stage as failed with an error code.
    ///
    /// Call this when the stage fails.
    /// The StageExit event on drop will have `ok: false` and the error code.
    pub fn fail(self, error_code: impl Into<String>) {
        self.finished.set(true);
        self.error_code.set(Some(error_code.into()));
        // Drop will emit the StageExit event
    }

    /// Get the duration since stage start.
    pub fn elapsed_us(&self) -> u64 {
        self.start_time.elapsed().as_micros() as u64
    }
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        let duration_us = self.elapsed_us();
        let error_code = self.error_code.take();
        let ok = self.finished.get() && error_code.is_none();

        // Emit StageExit event
        let exit_envelope = DiagEnvelope::new(
            &self.trace_context,
            &self.tenant_id,
            self.run_id.clone(),
            if ok {
                DiagSeverity::Info
            } else {
                DiagSeverity::Error
            },
            duration_us,
            DiagEvent::StageExit {
                stage: self.stage,
                duration_us,
                ok,
                error_code,
            },
        );
        let _ = self.service.emit(exit_envelope);
    }
}

/// Helper to create a scoped stage guard with automatic emit.
///
/// This is a convenience macro for creating StageGuard instances.
#[macro_export]
macro_rules! stage_guard {
    ($service:expr, $stage:expr, $trace_ctx:expr, $run_id:expr, $tenant_id:expr) => {
        $crate::stage_guard::StageGuard::new(
            std::sync::Arc::clone($service),
            $stage,
            $trace_ctx,
            $run_id,
            $tenant_id,
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::DiagnosticsConfig;
    use tokio::sync::mpsc;

    fn test_service() -> (Arc<DiagnosticsService>, mpsc::Receiver<DiagEnvelope>) {
        let config = DiagnosticsConfig {
            enabled: true,
            level: super::super::DiagLevel::Tokens,
            channel_capacity: 100,
            ..Default::default()
        };
        let (service, receiver) = DiagnosticsService::new(config);
        (Arc::new(service), receiver)
    }

    #[tokio::test]
    async fn test_stage_guard_emits_enter_and_exit_on_complete() {
        let (service, mut receiver) = test_service();
        let trace_ctx = TraceContext::new_root();
        let run_id = DiagRunId::new_random();

        service.start_run(&run_id);

        {
            let guard = StageGuard::new(
                Arc::clone(&service),
                DiagStage::RequestValidation,
                &trace_ctx,
                &run_id,
                "tenant-123",
            );
            // Simulate work
            guard.complete();
        }

        // Check StageEnter was emitted
        let enter_event = receiver.recv().await.unwrap();
        assert!(matches!(
            enter_event.payload,
            DiagEvent::StageEnter {
                stage: DiagStage::RequestValidation
            }
        ));

        // Check StageExit was emitted
        let exit_event = receiver.recv().await.unwrap();
        match exit_event.payload {
            DiagEvent::StageExit {
                stage,
                ok,
                error_code,
                ..
            } => {
                assert_eq!(stage, DiagStage::RequestValidation);
                assert!(ok);
                assert!(error_code.is_none());
            }
            _ => panic!("Expected StageExit event"),
        }
    }

    #[tokio::test]
    async fn test_stage_guard_emits_exit_with_error_on_fail() {
        let (service, mut receiver) = test_service();
        let trace_ctx = TraceContext::new_root();
        let run_id = DiagRunId::new_random();

        service.start_run(&run_id);

        {
            let guard = StageGuard::new(
                Arc::clone(&service),
                DiagStage::AdapterResolution,
                &trace_ctx,
                &run_id,
                "tenant-456",
            );
            guard.fail("E2001");
        }

        // Skip StageEnter
        let _ = receiver.recv().await.unwrap();

        // Check StageExit with error
        let exit_event = receiver.recv().await.unwrap();
        match exit_event.payload {
            DiagEvent::StageExit {
                stage,
                ok,
                error_code,
                ..
            } => {
                assert_eq!(stage, DiagStage::AdapterResolution);
                assert!(!ok);
                assert_eq!(error_code, Some("E2001".to_string()));
            }
            _ => panic!("Expected StageExit event"),
        }
    }

    #[tokio::test]
    async fn test_stage_guard_emits_exit_on_drop_without_complete() {
        let (service, mut receiver) = test_service();
        let trace_ctx = TraceContext::new_root();
        let run_id = DiagRunId::new_random();

        service.start_run(&run_id);

        {
            let _guard = StageGuard::new(
                Arc::clone(&service),
                DiagStage::WorkerInference,
                &trace_ctx,
                &run_id,
                "tenant-789",
            );
            // No complete() or fail() called - guard dropped
        }

        // Skip StageEnter
        let _ = receiver.recv().await.unwrap();

        // Check StageExit indicates incomplete (ok=false, no error code)
        let exit_event = receiver.recv().await.unwrap();
        match exit_event.payload {
            DiagEvent::StageExit {
                stage,
                ok,
                error_code,
                duration_us: _,
            } => {
                assert_eq!(stage, DiagStage::WorkerInference);
                assert!(!ok); // Not explicitly completed
                assert!(error_code.is_none()); // No error was set
                                               // duration_us can be 0 if the guard is dropped immediately
            }
            _ => panic!("Expected StageExit event"),
        }
    }

    #[tokio::test]
    async fn test_stage_guard_elapsed_time() {
        let (service, _receiver) = test_service();
        let trace_ctx = TraceContext::new_root();
        let run_id = DiagRunId::new_random();

        let guard = StageGuard::new(
            service,
            DiagStage::EvidenceTelemetry,
            &trace_ctx,
            &run_id,
            "tenant-test",
        );

        // Small sleep to ensure elapsed time > 0
        tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;

        let elapsed = guard.elapsed_us();
        assert!(elapsed >= 100);
    }
}
