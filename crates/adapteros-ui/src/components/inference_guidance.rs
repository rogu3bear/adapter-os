//! Inference readiness guidance helpers
//!
//! Maps inference blockers to user-facing reasons and next-step actions.

use adapteros_api_types::{InferenceBlocker, InferenceReadyState};

/// Primary next-step action for resolving an inference blocker.
#[derive(Debug, Clone, Copy)]
pub struct InferenceAction {
    pub label: &'static str,
    pub href: &'static str,
}

/// Guidance for an inference-not-ready state.
#[derive(Debug, Clone, Copy)]
pub struct InferenceGuidance {
    pub reason: &'static str,
    pub action: InferenceAction,
}

/// Build guidance from an inference readiness state + optional blocker.
pub fn guidance_for(
    readiness: InferenceReadyState,
    blocker: Option<&InferenceBlocker>,
) -> InferenceGuidance {
    if let Some(blocker) = blocker {
        InferenceGuidance {
            reason: blocker_reason(blocker),
            action: blocker_action(blocker),
        }
    } else {
        let reason = match readiness {
            InferenceReadyState::Unknown => "Status unknown",
            InferenceReadyState::False => "Reason unavailable",
            InferenceReadyState::True => "Inference ready",
        };
        InferenceGuidance {
            reason,
            action: fallback_action(),
        }
    }
}

fn fallback_action() -> InferenceAction {
    InferenceAction {
        label: "View system status",
        href: "/system",
    }
}

fn blocker_reason(blocker: &InferenceBlocker) -> &'static str {
    match blocker {
        InferenceBlocker::DatabaseUnavailable => "Database unavailable",
        InferenceBlocker::WorkerMissing => "No workers running",
        InferenceBlocker::NoModelLoaded => "No model loaded",
        InferenceBlocker::ActiveModelMismatch => "Active model mismatch",
        InferenceBlocker::TelemetryDegraded => "Telemetry degraded",
        InferenceBlocker::SystemBooting => "System booting",
        InferenceBlocker::BootFailed => "Boot failed",
    }
}

fn blocker_action(blocker: &InferenceBlocker) -> InferenceAction {
    match blocker {
        InferenceBlocker::NoModelLoaded => InferenceAction {
            label: "Load a model",
            href: "/models",
        },
        InferenceBlocker::WorkerMissing => InferenceAction {
            label: "Start a worker",
            href: "/workers",
        },
        InferenceBlocker::ActiveModelMismatch => InferenceAction {
            label: "Review models",
            href: "/models",
        },
        InferenceBlocker::TelemetryDegraded => InferenceAction {
            label: "Open monitoring",
            href: "/monitoring",
        },
        InferenceBlocker::DatabaseUnavailable
        | InferenceBlocker::SystemBooting
        | InferenceBlocker::BootFailed => fallback_action(),
    }
}
