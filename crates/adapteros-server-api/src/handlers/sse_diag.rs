//! SSE stream diagnostics helpers.
//!
//! Provides utilities for emitting diagnostic events for SSE streams.

use crate::state::AppState;
use adapteros_diagnostics::{DiagEnvelope, DiagEvent, DiagRunId, DiagSeverity};
use adapteros_telemetry::tracing::TraceContext;

/// Reasons for SSE stream closure.
#[derive(Debug, Clone, Copy)]
pub enum StreamCloseReason {
    /// Stream completed successfully
    Complete,
    /// Client disconnected
    ClientDisconnect,
    /// Error during streaming
    Error,
    /// Stream timed out
    Timeout,
    /// Stream was cancelled
    Cancelled,
}

impl StreamCloseReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            StreamCloseReason::Complete => "complete",
            StreamCloseReason::ClientDisconnect => "client_disconnect",
            StreamCloseReason::Error => "error",
            StreamCloseReason::Timeout => "timeout",
            StreamCloseReason::Cancelled => "cancelled",
        }
    }
}

/// Emit a StreamClosed diagnostic event with correlation to the original inference run.
///
/// Should be called when an SSE stream terminates for any reason.
///
/// # Arguments
/// * `state` - Application state containing diagnostics service
/// * `request_id` - The request ID for correlation
/// * `tenant_id` - The tenant ID
/// * `reason` - Why the stream was closed
/// * `run_id` - Optional run ID for correlation; if None, a new root trace is created
/// * `trace_context` - Optional trace context for correlation; if None, a new root trace is created
///
/// For proper event correlation with the original inference run, both `run_id` and
/// `trace_context` should be provided from the inference context.
pub fn emit_stream_closed(
    state: &AppState,
    request_id: &str,
    tenant_id: &str,
    reason: StreamCloseReason,
    run_id: Option<&DiagRunId>,
    trace_context: Option<&TraceContext>,
) {
    let Some(service) = state.diagnostics_service.as_ref() else {
        return;
    };

    if !service.is_enabled() {
        return;
    }

    // Use provided trace context or create a new root (fallback for call sites without context)
    let owned_trace_context: TraceContext;
    let effective_trace_context = match trace_context {
        Some(ctx) => ctx,
        None => {
            owned_trace_context = TraceContext::new_root();
            &owned_trace_context
        }
    };

    // Use provided run_id or derive from trace context
    let owned_run_id: DiagRunId;
    let effective_run_id = match run_id {
        Some(id) => id.clone(),
        None => {
            owned_run_id = DiagRunId::from_trace_context(effective_trace_context);
            owned_run_id
        }
    };

    let envelope = DiagEnvelope::new(
        effective_trace_context,
        tenant_id,
        effective_run_id,
        match reason {
            StreamCloseReason::Complete => DiagSeverity::Info,
            StreamCloseReason::ClientDisconnect => DiagSeverity::Warn,
            _ => DiagSeverity::Error,
        },
        0, // mono_us not tracked for stream close
        DiagEvent::StreamClosed {
            request_id: request_id.to_string(),
            reason: reason.as_str().to_string(),
        },
    );

    let _ = service.emit(envelope);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_close_reason_as_str() {
        assert_eq!(StreamCloseReason::Complete.as_str(), "complete");
        assert_eq!(
            StreamCloseReason::ClientDisconnect.as_str(),
            "client_disconnect"
        );
        assert_eq!(StreamCloseReason::Error.as_str(), "error");
        assert_eq!(StreamCloseReason::Timeout.as_str(), "timeout");
        assert_eq!(StreamCloseReason::Cancelled.as_str(), "cancelled");
    }
}
