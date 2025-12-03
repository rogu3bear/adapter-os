//! Worker Lifecycle Status Management
//!
//! Provides types and validation for worker lifecycle state transitions.
//!
//! # Worker Status
//!
//! Workers progress through these states:
//! - **Starting**: Worker is initializing, not ready to serve
//! - **Serving**: Worker is ready and accepting requests
//! - **Draining**: Worker is gracefully shutting down, rejecting new requests
//! - **Stopped**: Worker has cleanly stopped
//! - **Crashed**: Worker terminated abnormally
//!
//! # Valid Transitions
//!
//! ```text
//! starting → serving | crashed
//! serving → draining | crashed
//! draining → stopped | crashed
//! stopped → (terminal)
//! crashed → (terminal)
//! ```
//!
//! # Examples
//!
//! ```rust
//! use adapteros_core::worker_status::{WorkerStatus, WorkerStatusTransition};
//!
//! // Validate a transition
//! let transition = WorkerStatusTransition::new(
//!     WorkerStatus::Starting,
//!     WorkerStatus::Serving
//! );
//! assert!(transition.is_valid());
//!
//! // Invalid transition
//! let invalid = WorkerStatusTransition::new(
//!     WorkerStatus::Stopped,
//!     WorkerStatus::Serving
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
    /// Worker is initializing, not ready to serve requests
    Starting,
    /// Worker is ready and accepting inference requests
    Serving,
    /// Worker is gracefully shutting down, rejecting new requests
    Draining,
    /// Worker has cleanly stopped
    Stopped,
    /// Worker terminated abnormally
    Crashed,
}

impl WorkerStatus {
    /// Returns true if the worker can serve inference requests
    pub fn can_serve(&self) -> bool {
        matches!(self, WorkerStatus::Serving)
    }

    /// Returns true if this is a terminal state (no further transitions allowed)
    pub fn is_terminal(&self) -> bool {
        matches!(self, WorkerStatus::Stopped | WorkerStatus::Crashed)
    }

    /// Returns true if the worker is still running (not terminal)
    pub fn is_running(&self) -> bool {
        matches!(
            self,
            WorkerStatus::Starting | WorkerStatus::Serving | WorkerStatus::Draining
        )
    }

    /// Returns all valid states this worker can transition to
    pub fn valid_transitions(&self) -> &'static [WorkerStatus] {
        match self {
            WorkerStatus::Starting => &[WorkerStatus::Serving, WorkerStatus::Crashed],
            WorkerStatus::Serving => &[WorkerStatus::Draining, WorkerStatus::Crashed],
            WorkerStatus::Draining => &[WorkerStatus::Stopped, WorkerStatus::Crashed],
            WorkerStatus::Stopped => &[],
            WorkerStatus::Crashed => &[],
        }
    }

    /// Returns all states that can transition to this state
    pub fn valid_predecessors(&self) -> &'static [WorkerStatus] {
        match self {
            WorkerStatus::Starting => &[],
            WorkerStatus::Serving => &[WorkerStatus::Starting],
            WorkerStatus::Draining => &[WorkerStatus::Serving],
            WorkerStatus::Stopped => &[WorkerStatus::Draining],
            WorkerStatus::Crashed => &[
                WorkerStatus::Starting,
                WorkerStatus::Serving,
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
            WorkerStatus::Starting => "starting",
            WorkerStatus::Serving => "serving",
            WorkerStatus::Draining => "draining",
            WorkerStatus::Stopped => "stopped",
            WorkerStatus::Crashed => "crashed",
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
            "starting" => Ok(WorkerStatus::Starting),
            "serving" => Ok(WorkerStatus::Serving),
            "draining" => Ok(WorkerStatus::Draining),
            "stopped" => Ok(WorkerStatus::Stopped),
            "crashed" => Ok(WorkerStatus::Crashed),
            _ => Err(AosError::Validation(format!(
                "Invalid worker status: {}. Must be one of: starting, serving, draining, stopped, crashed",
                s
            ))),
        }
    }
}

/// A worker status transition
///
/// Validates that transitions follow the allowed state machine:
/// - starting → serving | crashed
/// - serving → draining | crashed
/// - draining → stopped | crashed
/// - stopped, crashed → (terminal, no transitions)
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
        assert!(!WorkerStatus::Starting.can_serve());
        assert!(WorkerStatus::Serving.can_serve());
        assert!(!WorkerStatus::Draining.can_serve());
        assert!(!WorkerStatus::Stopped.can_serve());
        assert!(!WorkerStatus::Crashed.can_serve());
    }

    #[test]
    fn test_worker_status_is_terminal() {
        assert!(!WorkerStatus::Starting.is_terminal());
        assert!(!WorkerStatus::Serving.is_terminal());
        assert!(!WorkerStatus::Draining.is_terminal());
        assert!(WorkerStatus::Stopped.is_terminal());
        assert!(WorkerStatus::Crashed.is_terminal());
    }

    #[test]
    fn test_worker_status_is_running() {
        assert!(WorkerStatus::Starting.is_running());
        assert!(WorkerStatus::Serving.is_running());
        assert!(WorkerStatus::Draining.is_running());
        assert!(!WorkerStatus::Stopped.is_running());
        assert!(!WorkerStatus::Crashed.is_running());
    }

    #[test]
    fn test_valid_transitions_from_starting() {
        assert!(WorkerStatus::Starting.can_transition_to(WorkerStatus::Serving));
        assert!(WorkerStatus::Starting.can_transition_to(WorkerStatus::Crashed));
        assert!(!WorkerStatus::Starting.can_transition_to(WorkerStatus::Draining));
        assert!(!WorkerStatus::Starting.can_transition_to(WorkerStatus::Stopped));
    }

    #[test]
    fn test_valid_transitions_from_serving() {
        assert!(WorkerStatus::Serving.can_transition_to(WorkerStatus::Draining));
        assert!(WorkerStatus::Serving.can_transition_to(WorkerStatus::Crashed));
        assert!(!WorkerStatus::Serving.can_transition_to(WorkerStatus::Starting));
        assert!(!WorkerStatus::Serving.can_transition_to(WorkerStatus::Stopped));
    }

    #[test]
    fn test_valid_transitions_from_draining() {
        assert!(WorkerStatus::Draining.can_transition_to(WorkerStatus::Stopped));
        assert!(WorkerStatus::Draining.can_transition_to(WorkerStatus::Crashed));
        assert!(!WorkerStatus::Draining.can_transition_to(WorkerStatus::Starting));
        assert!(!WorkerStatus::Draining.can_transition_to(WorkerStatus::Serving));
    }

    #[test]
    fn test_terminal_states_no_transitions() {
        assert_eq!(WorkerStatus::Stopped.valid_transitions(), &[]);
        assert_eq!(WorkerStatus::Crashed.valid_transitions(), &[]);

        assert!(!WorkerStatus::Stopped.can_transition_to(WorkerStatus::Starting));
        assert!(!WorkerStatus::Stopped.can_transition_to(WorkerStatus::Serving));
        assert!(!WorkerStatus::Crashed.can_transition_to(WorkerStatus::Starting));
        assert!(!WorkerStatus::Crashed.can_transition_to(WorkerStatus::Serving));
    }

    #[test]
    fn test_noop_transitions_always_valid() {
        assert!(WorkerStatus::Starting.can_transition_to(WorkerStatus::Starting));
        assert!(WorkerStatus::Serving.can_transition_to(WorkerStatus::Serving));
        assert!(WorkerStatus::Draining.can_transition_to(WorkerStatus::Draining));
        assert!(WorkerStatus::Stopped.can_transition_to(WorkerStatus::Stopped));
        assert!(WorkerStatus::Crashed.can_transition_to(WorkerStatus::Crashed));
    }

    #[test]
    fn test_worker_status_from_str() {
        assert_eq!(
            WorkerStatus::from_str("starting").unwrap(),
            WorkerStatus::Starting
        );
        assert_eq!(
            WorkerStatus::from_str("serving").unwrap(),
            WorkerStatus::Serving
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
            WorkerStatus::Crashed
        );
        assert!(WorkerStatus::from_str("invalid").is_err());
    }

    #[test]
    fn test_worker_status_display() {
        assert_eq!(WorkerStatus::Starting.to_string(), "starting");
        assert_eq!(WorkerStatus::Serving.to_string(), "serving");
        assert_eq!(WorkerStatus::Draining.to_string(), "draining");
        assert_eq!(WorkerStatus::Stopped.to_string(), "stopped");
        assert_eq!(WorkerStatus::Crashed.to_string(), "crashed");
    }

    #[test]
    fn test_worker_status_transition_validation() {
        let valid = WorkerStatusTransition::new(WorkerStatus::Starting, WorkerStatus::Serving);
        assert!(valid.is_valid());
        assert!(valid.validate().is_ok());
        assert!(valid.validation_error().is_none());

        let invalid = WorkerStatusTransition::new(WorkerStatus::Stopped, WorkerStatus::Serving);
        assert!(!invalid.is_valid());
        assert!(invalid.validate().is_err());
        assert!(invalid.validation_error().is_some());
    }

    #[test]
    fn test_transition_to_returns_error_on_invalid() {
        let result = WorkerStatus::Stopped.transition_to(WorkerStatus::Serving);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid worker transition"));
    }

    #[test]
    fn test_transition_to_returns_ok_on_valid() {
        let result = WorkerStatus::Starting.transition_to(WorkerStatus::Serving);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), WorkerStatus::Serving);
    }
}
