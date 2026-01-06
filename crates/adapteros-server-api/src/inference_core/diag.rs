//! Diagnostics instrumentation for InferenceCore.
//!
//! Provides helpers for emitting diagnostic events during inference.

use crate::state::AppState;
use adapteros_diagnostics::{
    DiagEnvelope, DiagEvent, DiagRunId, DiagSeverity, DiagStage, DiagnosticsService, StageGuard,
};
use adapteros_telemetry::tracing::TraceContext;
use std::sync::Arc;
use std::time::Instant;

/// Run context for diagnostic event emission.
///
/// Holds references to the diagnostics service and run metadata.
/// Created at the start of route_and_infer and used throughout the pipeline.
pub struct DiagRunContext {
    pub service: Arc<DiagnosticsService>,
    pub trace_context: TraceContext,
    pub run_id: DiagRunId,
    pub tenant_id: String,
    pub request_id: String,
    pub start_time: Instant,
}

impl DiagRunContext {
    /// Create a new diagnostic run context.
    ///
    /// If the diagnostics service is disabled, returns None.
    pub fn try_new(
        state: &AppState,
        request_id: &str,
        tenant_id: &str,
        trace_context: &TraceContext,
    ) -> Option<Self> {
        let service = state.diagnostics_service.as_ref()?.clone();
        if !service.is_enabled() {
            return None;
        }

        let run_id = DiagRunId::from_trace_context(trace_context);
        service.start_run(&run_id);

        Some(Self {
            service,
            trace_context: trace_context.clone(),
            run_id,
            tenant_id: tenant_id.to_string(),
            request_id: request_id.to_string(),
            start_time: Instant::now(),
        })
    }

    /// Emit RunStarted event.
    pub fn emit_run_started(&self, is_replay: bool) {
        let envelope = DiagEnvelope::new(
            &self.trace_context,
            &self.tenant_id,
            self.run_id.clone(),
            DiagSeverity::Info,
            0,
            DiagEvent::RunStarted {
                request_id: self.request_id.clone(),
                is_replay,
            },
        );
        let _ = self.service.emit(envelope);
    }

    /// Emit RunFinished event.
    pub fn emit_run_finished(&self) {
        let duration_us = self.start_time.elapsed().as_micros() as u64;
        let envelope = DiagEnvelope::new(
            &self.trace_context,
            &self.tenant_id,
            self.run_id.clone(),
            DiagSeverity::Info,
            duration_us,
            DiagEvent::RunFinished {
                request_id: self.request_id.clone(),
                total_duration_us: duration_us,
            },
        );
        let _ = self.service.emit(envelope);
        self.service.end_run(&self.run_id);
    }

    /// Emit RunFailed event.
    pub fn emit_run_failed(&self, error_code: &str, recovery_action: Option<&str>) {
        let duration_us = self.start_time.elapsed().as_micros() as u64;
        let envelope = DiagEnvelope::new(
            &self.trace_context,
            &self.tenant_id,
            self.run_id.clone(),
            DiagSeverity::Error,
            duration_us,
            DiagEvent::RunFailed {
                request_id: self.request_id.clone(),
                error_code: error_code.to_string(),
                recovery_action: recovery_action.map(|s| s.to_string()),
            },
        );
        let _ = self.service.emit(envelope);
        self.service.end_run(&self.run_id);
    }

    /// Create a stage guard for the given stage.
    #[allow(dead_code)]
    pub fn stage_guard(&self, stage: DiagStage) -> StageGuard {
        StageGuard::new(
            Arc::clone(&self.service),
            stage,
            &self.trace_context,
            &self.run_id,
            &self.tenant_id,
        )
    }
}

/// Extract error code from InferenceError.
pub fn extract_error_code(error: &crate::types::InferenceError) -> &'static str {
    use crate::types::InferenceError;
    match error {
        InferenceError::ValidationError(_) => "E1001",
        InferenceError::PermissionDenied(_) => "E1002",
        InferenceError::PolicyViolation { .. } => "E1003",
        InferenceError::WorkerNotAvailable(_) => "E2001",
        InferenceError::WorkerError(_) => "E2002",
        InferenceError::Timeout(_) => "E2003",
        InferenceError::RoutingBypass(_) => "E2004",
        InferenceError::BackpressureError(_) => "E2005",
        InferenceError::NoCompatibleWorker { .. } => "E2006",
        InferenceError::WorkerDegraded { .. } => "E2007",
        InferenceError::ClientClosed(_) => "E3001",
        InferenceError::DatabaseError(_) => "E4001",
        InferenceError::RagError(_) => "E5001",
        InferenceError::ModelNotReady(_) => "E6001",
        InferenceError::AdapterNotLoadable { .. } => "E6002",
        InferenceError::AdapterNotFound(_) => "E6003",
        InferenceError::ReplayError(_) => "E7001",
        InferenceError::DeterminismError(_) => "E8001",
        InferenceError::CacheBudgetExceeded { .. } => "E9001",
        InferenceError::WorkerIdUnavailable { .. } => "E9002",
        InferenceError::InternalError(_) => "E9999",
    }
}

/// Suggest recovery action for an error.
pub fn suggest_recovery(error: &crate::types::InferenceError) -> Option<&'static str> {
    use crate::types::InferenceError;
    match error {
        InferenceError::ValidationError(_) => Some("Check request parameters"),
        InferenceError::PermissionDenied(_) => Some("Verify credentials and tenant access"),
        InferenceError::PolicyViolation { .. } => Some("Review policy configuration"),
        InferenceError::WorkerNotAvailable(_) => Some("Retry after worker restart"),
        InferenceError::WorkerError(_) => Some("Check worker logs for details"),
        InferenceError::Timeout(_) => Some("Retry with shorter max_tokens or timeout"),
        InferenceError::RoutingBypass(_) => Some("Use standard routing path"),
        InferenceError::BackpressureError(_) => Some("Wait for memory pressure to reduce"),
        InferenceError::NoCompatibleWorker { .. } => Some("Check manifest compatibility"),
        InferenceError::WorkerDegraded { .. } => Some("Retry or wait for full worker availability"),
        InferenceError::ClientClosed(_) => None, // Client disconnect, no recovery
        InferenceError::DatabaseError(_) => Some("Check database connectivity"),
        InferenceError::RagError(_) => Some("Verify RAG collection exists"),
        InferenceError::ModelNotReady(_) => Some("Wait for model loading to complete"),
        InferenceError::AdapterNotLoadable { .. } => Some("Check adapter lifecycle state"),
        InferenceError::AdapterNotFound(_) => Some("Verify adapter ID exists"),
        InferenceError::ReplayError(_) => Some("Verify replay metadata"),
        InferenceError::DeterminismError(_) => Some("Provide required seed parameter"),
        InferenceError::CacheBudgetExceeded { .. } => {
            Some("Reduce concurrent requests or increase cache budget")
        }
        InferenceError::WorkerIdUnavailable { .. } => Some("Ensure worker is registered"),
        InferenceError::InternalError(_) => Some("Contact support with request_id"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_error_code() {
        use crate::types::InferenceError;

        assert_eq!(
            extract_error_code(&InferenceError::ValidationError("test".into())),
            "E1001"
        );
        assert_eq!(
            extract_error_code(&InferenceError::Timeout("test".into())),
            "E2003"
        );
    }

    #[test]
    fn test_suggest_recovery() {
        use crate::types::InferenceError;

        assert!(suggest_recovery(&InferenceError::ValidationError("test".into())).is_some());
        // ClientClosed has no recovery action
        assert!(suggest_recovery(&InferenceError::ClientClosed("test".into())).is_none());
    }
}
