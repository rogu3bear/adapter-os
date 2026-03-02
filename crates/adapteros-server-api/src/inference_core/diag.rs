//! Diagnostics instrumentation for InferenceCore.
//!
//! Provides helpers for emitting diagnostic events during inference.

use crate::state::AppState;
use adapteros_error_registry::HasECode;
use adapteros_telemetry::diagnostics::{
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
///
/// FIXED (Issue 2.1): Use HasECode trait instead of hardcoded mappings.
/// This ensures compile-time exhaustiveness checking and prevents drift.
pub fn extract_error_code(error: &crate::types::InferenceError) -> &'static str {
    use adapteros_error_registry::HasECode;
    error.ecode().as_str()
}

/// Suggest recovery action for an error.
pub fn suggest_recovery(error: &crate::types::InferenceError) -> Option<&'static str> {
    use crate::types::InferenceError;
    match error {
        InferenceError::ValidationError(_) => Some("Check request parameters"),
        InferenceError::BitIdenticalAdapterPinRequired(_) => {
            Some("Use adapter_repo_id@adapter_version_id")
        }
        InferenceError::BitIdenticalAdapterPinInvalid(_) => {
            Some("Use valid pinned adapter versions for this tenant")
        }
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
        InferenceError::AdapterTenantMismatch { .. } => Some("Verify adapter belongs to tenant"),
        InferenceError::AdapterBaseModelMismatch { .. } => {
            Some("Use adapters trained on the active base model")
        }
        InferenceError::ReplayError(_) => Some("Verify replay metadata"),
        InferenceError::DeterminismError(_) => Some("Provide required seed parameter"),
        InferenceError::CacheBudgetExceeded { .. } => {
            Some("Reduce concurrent requests or increase cache budget")
        }
        InferenceError::WorkerIdUnavailable { .. } => Some("Ensure worker is registered"),
        InferenceError::InternalError(_) => Some("Contact support with request_id"),
        InferenceError::DuplicateRequest { .. } => Some("Wait for in-flight request to complete"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_error_code() {
        use crate::types::InferenceError;

        // FIXED (Issue 2.1): Updated test expectations to match correct semantic mappings
        // Original "E1001" was incorrect (E1001 is Crypto/Signing, not validation)
        // ValidationError correctly maps to E8001 (CLI/Config - Invalid Configuration)
        assert_eq!(
            extract_error_code(&InferenceError::ValidationError("test".into())),
            "E8001"
        );
        // Timeout maps to E2003 (Policy/Determinism - Egress Violation, closest semantic match)
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
