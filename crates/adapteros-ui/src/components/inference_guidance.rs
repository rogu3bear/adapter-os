//! Inference readiness guidance
//!
//! Maps inference blockers to user-facing reasons and next-step actions.
//! Owns priority ordering: the UI should display the highest-priority blocker
//! regardless of backend emission order, since detection order != resolution order.

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

/// Resolution priority — lower number = fix this first.
///
/// This is the dependency chain: you can't load a model without a worker,
/// can't have a worker without a database, etc. TelemetryDegraded is last
/// because it's non-blocking for most users.
fn blocker_priority(blocker: &InferenceBlocker) -> u8 {
    match blocker {
        InferenceBlocker::BootFailed => 0,
        InferenceBlocker::SystemBooting => 1,
        InferenceBlocker::DatabaseUnavailable => 2,
        InferenceBlocker::WorkerMissing => 3,
        InferenceBlocker::NoModelLoaded => 4,
        InferenceBlocker::ActiveModelMismatch => 5,
        InferenceBlocker::TelemetryDegraded => 6,
    }
}

/// Pick the highest-priority blocker from a slice.
pub fn primary_blocker(blockers: &[InferenceBlocker]) -> Option<&InferenceBlocker> {
    blockers.iter().min_by_key(|b| blocker_priority(b))
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
        InferenceBlocker::WorkerMissing => "No workers connected",
        InferenceBlocker::NoModelLoaded => "No model loaded",
        InferenceBlocker::ActiveModelMismatch => "Selected model not loaded on any worker",
        InferenceBlocker::TelemetryDegraded => "Telemetry degraded",
        InferenceBlocker::SystemBooting => "System starting up",
        InferenceBlocker::BootFailed => "System failed to start",
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
            label: "Load active model",
            href: "/models",
        },
        InferenceBlocker::TelemetryDegraded => InferenceAction {
            label: "View monitoring",
            href: "/monitoring",
        },
        InferenceBlocker::BootFailed => InferenceAction {
            label: "View errors",
            href: "/errors",
        },
        InferenceBlocker::DatabaseUnavailable | InferenceBlocker::SystemBooting => {
            fallback_action()
        }
    }
}
