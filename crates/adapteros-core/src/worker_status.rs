//! Worker Lifecycle Status Management
//!
//! Provides types and validation for worker lifecycle state transitions.
//!
//! # Worker Status
//!
//! Workers progress through these states:
//! - **Created**: Process launched, manifest not yet bound/registered
//! - **Registered**: Control plane accepted registration and manifest hash
//! - **Healthy**: Worker is ready and accepting requests; UDS is listening
//! - **Draining**: Graceful shutdown; rejecting new requests
//! - **Stopped**: Clean shutdown complete
//! - **Error**: Fatal/health failure path (terminal)
//!
//! # Valid Transitions
//!
//! ```text
//! created → registered | error
//! registered → healthy | error
//! healthy → draining | error
//! stopped → (terminal)
//! error → (terminal)
//! ```
//!
//! # Examples
//!
//! ```rust
//! use adapteros_core::worker_status::{WorkerStatus, WorkerStatusTransition};
//!
//! // Validate a transition
//! let transition = WorkerStatusTransition::new(
//!     WorkerStatus::Created,
//!     WorkerStatus::Registered
//! );
//! assert!(transition.is_valid());
//!
//! // Invalid transition
//! let invalid = WorkerStatusTransition::new(
//!     WorkerStatus::Stopped,
//!     WorkerStatus::Healthy
//! );
//! assert!(!invalid.is_valid());
//! ```

use crate::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Worker lifecycle status
///
/// Represents the operational state of a worker process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerStatus {
    /// Process launched, manifest not yet bound/registered
    Created,
    /// Control plane accepted registration and manifest hash
    Registered,
    /// Worker is ready and accepting inference requests (UDS listening)
    Healthy,
    /// Worker is gracefully shutting down, rejecting new requests
    Draining,
    /// Worker has cleanly stopped
    Stopped,
    /// Worker terminated abnormally or failed health checks
    Error,
}

impl WorkerStatus {
    /// Returns true if the worker can serve inference requests
    pub fn can_serve(&self) -> bool {
        matches!(self, WorkerStatus::Healthy)
    }

    /// Returns true if this is a terminal state (no further transitions allowed)
    pub fn is_terminal(&self) -> bool {
        matches!(self, WorkerStatus::Stopped | WorkerStatus::Error)
    }

    /// Returns true if the worker is still running (not terminal)
    pub fn is_running(&self) -> bool {
        matches!(
            self,
            WorkerStatus::Created
                | WorkerStatus::Registered
                | WorkerStatus::Healthy
                | WorkerStatus::Draining
        )
    }

    /// Returns all valid states this worker can transition to
    pub fn valid_transitions(&self) -> &'static [WorkerStatus] {
        match self {
            WorkerStatus::Created => &[WorkerStatus::Registered, WorkerStatus::Error],
            WorkerStatus::Registered => &[WorkerStatus::Healthy, WorkerStatus::Error],
            WorkerStatus::Healthy => &[WorkerStatus::Draining, WorkerStatus::Error],
            WorkerStatus::Draining => &[WorkerStatus::Stopped, WorkerStatus::Error],
            WorkerStatus::Stopped => &[],
            WorkerStatus::Error => &[],
        }
    }

    /// Returns all states that can transition to this state
    pub fn valid_predecessors(&self) -> &'static [WorkerStatus] {
        match self {
            WorkerStatus::Created => &[],
            WorkerStatus::Registered => &[WorkerStatus::Created],
            WorkerStatus::Healthy => &[WorkerStatus::Registered],
            WorkerStatus::Draining => &[WorkerStatus::Healthy],
            WorkerStatus::Stopped => &[WorkerStatus::Draining],
            WorkerStatus::Error => &[
                WorkerStatus::Created,
                WorkerStatus::Registered,
                WorkerStatus::Healthy,
                WorkerStatus::Draining,
            ],
        }
    }

    /// Check if transition from this state to new state is valid
    pub fn can_transition_to(&self, new_status: WorkerStatus) -> bool {
        // No-op transitions are always valid
        if *self == new_status {
            return true;
        }
        self.valid_transitions().contains(&new_status)
    }

    /// Attempt to transition to a new status, returning error if invalid
    pub fn transition_to(&self, new_status: WorkerStatus) -> Result<WorkerStatus> {
        if self.can_transition_to(new_status) {
            Ok(new_status)
        } else {
            Err(AosError::Lifecycle(format!(
                "Invalid worker transition: {} -> {}. Valid transitions from {}: {:?}",
                self,
                new_status,
                self,
                self.valid_transitions()
            )))
        }
    }

    /// Convert to string representation for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkerStatus::Created => "created",
            WorkerStatus::Registered => "registered",
            WorkerStatus::Healthy => "healthy",
            WorkerStatus::Draining => "draining",
            WorkerStatus::Stopped => "stopped",
            WorkerStatus::Error => "error",
        }
    }

    /// Backwards-compatible alias for legacy persisted values.
    pub fn legacy_alias(&self) -> &'static str {
        match self {
            WorkerStatus::Created => "starting",
            WorkerStatus::Registered => "starting",
            WorkerStatus::Healthy => "serving",
            WorkerStatus::Draining => "draining",
            WorkerStatus::Stopped => "stopped",
            WorkerStatus::Error => "crashed",
        }
    }
}

impl fmt::Display for WorkerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for WorkerStatus {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "created" | "starting" => Ok(WorkerStatus::Created),
            "registered" => Ok(WorkerStatus::Registered),
            "healthy" | "serving" => Ok(WorkerStatus::Healthy),
            "draining" => Ok(WorkerStatus::Draining),
            "stopped" => Ok(WorkerStatus::Stopped),
            "error" | "crashed" => Ok(WorkerStatus::Error),
            _ => Err(AosError::Validation(format!(
                "Invalid worker status: {}. Must be one of: created, registered, healthy, draining, stopped, error",
                s
            ))),
        }
    }
}

/// A worker status transition
///
/// Validates that transitions follow the allowed state machine:
/// - created → registered | error
/// - registered → healthy | error
/// - healthy → draining | error
/// - stopped, error → (terminal, no transitions)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerStatusTransition {
    pub from: WorkerStatus,
    pub to: WorkerStatus,
}

impl WorkerStatusTransition {
    /// Create a new worker status transition
    pub fn new(from: WorkerStatus, to: WorkerStatus) -> Self {
        Self { from, to }
    }

    /// Check if this transition is valid
    pub fn is_valid(&self) -> bool {
        self.from.can_transition_to(self.to)
    }

    /// Validate this transition, returning an error if invalid
    pub fn validate(&self) -> Result<()> {
        if self.is_valid() {
            Ok(())
        } else {
            Err(AosError::Lifecycle(format!(
                "Invalid worker transition: {} -> {}. Valid transitions from {}: {:?}",
                self.from,
                self.to,
                self.from,
                self.from.valid_transitions()
            )))
        }
    }

    /// Get a human-readable description of why this transition is invalid
    pub fn validation_error(&self) -> Option<String> {
        if self.is_valid() {
            None
        } else {
            Some(format!(
                "Cannot transition worker from {} to {}. Valid next states: {:?}",
                self.from,
                self.to,
                self.from.valid_transitions()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_status_can_serve() {
        assert!(!WorkerStatus::Created.can_serve());
        assert!(!WorkerStatus::Registered.can_serve());
        assert!(WorkerStatus::Healthy.can_serve());
        assert!(!WorkerStatus::Draining.can_serve());
        assert!(!WorkerStatus::Stopped.can_serve());
        assert!(!WorkerStatus::Error.can_serve());
    }

    #[test]
    fn test_worker_status_is_terminal() {
        assert!(!WorkerStatus::Created.is_terminal());
        assert!(!WorkerStatus::Registered.is_terminal());
        assert!(!WorkerStatus::Healthy.is_terminal());
        assert!(!WorkerStatus::Draining.is_terminal());
        assert!(WorkerStatus::Stopped.is_terminal());
        assert!(WorkerStatus::Error.is_terminal());
    }

    #[test]
    fn test_worker_status_is_running() {
        assert!(WorkerStatus::Created.is_running());
        assert!(WorkerStatus::Registered.is_running());
        assert!(WorkerStatus::Healthy.is_running());
        assert!(WorkerStatus::Draining.is_running());
        assert!(!WorkerStatus::Stopped.is_running());
        assert!(!WorkerStatus::Error.is_running());
    }

    #[test]
    fn test_valid_transitions_from_created() {
        assert!(WorkerStatus::Created.can_transition_to(WorkerStatus::Registered));
        assert!(WorkerStatus::Created.can_transition_to(WorkerStatus::Error));
        assert!(!WorkerStatus::Created.can_transition_to(WorkerStatus::Healthy));
        assert!(!WorkerStatus::Created.can_transition_to(WorkerStatus::Draining));
        assert!(!WorkerStatus::Created.can_transition_to(WorkerStatus::Stopped));
    }

    #[test]
    fn test_valid_transitions_from_registered() {
        assert!(WorkerStatus::Registered.can_transition_to(WorkerStatus::Healthy));
        assert!(WorkerStatus::Registered.can_transition_to(WorkerStatus::Error));
        assert!(!WorkerStatus::Registered.can_transition_to(WorkerStatus::Created));
        assert!(!WorkerStatus::Registered.can_transition_to(WorkerStatus::Draining));
        assert!(!WorkerStatus::Registered.can_transition_to(WorkerStatus::Stopped));
    }

    #[test]
    fn test_valid_transitions_from_draining() {
        assert!(WorkerStatus::Draining.can_transition_to(WorkerStatus::Stopped));
        assert!(WorkerStatus::Draining.can_transition_to(WorkerStatus::Error));
        assert!(!WorkerStatus::Draining.can_transition_to(WorkerStatus::Created));
        assert!(!WorkerStatus::Draining.can_transition_to(WorkerStatus::Registered));
        assert!(!WorkerStatus::Draining.can_transition_to(WorkerStatus::Healthy));
    }

    #[test]
    fn test_terminal_states_no_transitions() {
        assert_eq!(WorkerStatus::Stopped.valid_transitions(), &[]);
        assert_eq!(WorkerStatus::Error.valid_transitions(), &[]);

        assert!(!WorkerStatus::Stopped.can_transition_to(WorkerStatus::Created));
        assert!(!WorkerStatus::Stopped.can_transition_to(WorkerStatus::Healthy));
        assert!(!WorkerStatus::Error.can_transition_to(WorkerStatus::Created));
        assert!(!WorkerStatus::Error.can_transition_to(WorkerStatus::Healthy));
    }

    #[test]
    fn test_noop_transitions_always_valid() {
        assert!(WorkerStatus::Created.can_transition_to(WorkerStatus::Created));
        assert!(WorkerStatus::Healthy.can_transition_to(WorkerStatus::Healthy));
        assert!(WorkerStatus::Draining.can_transition_to(WorkerStatus::Draining));
        assert!(WorkerStatus::Stopped.can_transition_to(WorkerStatus::Stopped));
        assert!(WorkerStatus::Error.can_transition_to(WorkerStatus::Error));
    }

    #[test]
    fn test_worker_status_from_str() {
        assert_eq!(
            WorkerStatus::from_str("created").unwrap(),
            WorkerStatus::Created
        );
        assert_eq!(
            WorkerStatus::from_str("registered").unwrap(),
            WorkerStatus::Registered
        );
        assert_eq!(
            WorkerStatus::from_str("healthy").unwrap(),
            WorkerStatus::Healthy
        );
        assert_eq!(
            WorkerStatus::from_str("serving").unwrap(),
            WorkerStatus::Healthy
        );
        assert_eq!(
            WorkerStatus::from_str("DRAINING").unwrap(),
            WorkerStatus::Draining
        );
        assert_eq!(
            WorkerStatus::from_str("Stopped").unwrap(),
            WorkerStatus::Stopped
        );
        assert_eq!(
            WorkerStatus::from_str("crashed").unwrap(),
            WorkerStatus::Error
        );
        assert_eq!(
            WorkerStatus::from_str("error").unwrap(),
            WorkerStatus::Error
        );
        assert!(WorkerStatus::from_str("invalid").is_err());
    }

    #[test]
    fn test_worker_status_display() {
        assert_eq!(WorkerStatus::Created.to_string(), "created");
        assert_eq!(WorkerStatus::Registered.to_string(), "registered");
        assert_eq!(WorkerStatus::Healthy.to_string(), "healthy");
        assert_eq!(WorkerStatus::Draining.to_string(), "draining");
        assert_eq!(WorkerStatus::Stopped.to_string(), "stopped");
        assert_eq!(WorkerStatus::Error.to_string(), "error");
    }

    #[test]
    fn test_worker_status_transition_validation() {
        let valid = WorkerStatusTransition::new(WorkerStatus::Created, WorkerStatus::Registered);
        assert!(valid.is_valid());
        assert!(valid.validate().is_ok());
        assert!(valid.validation_error().is_none());

        let invalid = WorkerStatusTransition::new(WorkerStatus::Stopped, WorkerStatus::Healthy);
        assert!(!invalid.is_valid());
        assert!(invalid.validate().is_err());
        assert!(invalid.validation_error().is_some());
    }

    #[test]
    fn test_transition_to_returns_error_on_invalid() {
        let result = WorkerStatus::Stopped.transition_to(WorkerStatus::Healthy);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid worker transition"));
    }

    #[test]
    fn test_transition_to_returns_ok_on_valid() {
        let result = WorkerStatus::Created.transition_to(WorkerStatus::Registered);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), WorkerStatus::Registered);
    }
}
