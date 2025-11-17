//! Adapter Lifecycle State Machine (PRD 3)
//!
//! Defines strict lifecycle states and valid transitions for adapters.
//!
//! State transitions:
//! ```text
//! Registered -> Loaded -> Active -> Unloaded
//! ```
//!
//! No other transitions are allowed.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Adapter lifecycle state per PRD 3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterLifecycleState {
    /// Adapter exists in DB but not loaded in memory
    Registered,
    /// Adapter weights are loaded in memory
    Loaded,
    /// Adapter is active in a stack and available for routing
    Active,
    /// Adapter is not in memory
    Unloaded,
}

impl AdapterLifecycleState {
    /// Check if this state can transition to the target state
    pub fn can_transition_to(&self, target: &AdapterLifecycleState) -> bool {
        matches!(
            (self, target),
            (AdapterLifecycleState::Registered, AdapterLifecycleState::Loaded)
                | (AdapterLifecycleState::Loaded, AdapterLifecycleState::Active)
                | (AdapterLifecycleState::Active, AdapterLifecycleState::Unloaded)
        )
    }

    /// Get all valid next states from this state
    pub fn valid_next_states(&self) -> Vec<AdapterLifecycleState> {
        match self {
            AdapterLifecycleState::Registered => vec![AdapterLifecycleState::Loaded],
            AdapterLifecycleState::Loaded => vec![AdapterLifecycleState::Active],
            AdapterLifecycleState::Active => vec![AdapterLifecycleState::Unloaded],
            AdapterLifecycleState::Unloaded => vec![],
        }
    }

    /// Check if adapter is loaded (in memory)
    pub fn is_loaded(&self) -> bool {
        matches!(
            self,
            AdapterLifecycleState::Loaded | AdapterLifecycleState::Active
        )
    }

    /// Check if adapter can be used in a stack
    pub fn can_be_in_stack(&self) -> bool {
        matches!(
            self,
            AdapterLifecycleState::Loaded | AdapterLifecycleState::Active
        )
    }
}

impl fmt::Display for AdapterLifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterLifecycleState::Registered => write!(f, "registered"),
            AdapterLifecycleState::Loaded => write!(f, "loaded"),
            AdapterLifecycleState::Active => write!(f, "active"),
            AdapterLifecycleState::Unloaded => write!(f, "unloaded"),
        }
    }
}

impl std::str::FromStr for AdapterLifecycleState {
    type Err = crate::AosError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "registered" => Ok(AdapterLifecycleState::Registered),
            "loaded" => Ok(AdapterLifecycleState::Loaded),
            "active" => Ok(AdapterLifecycleState::Active),
            "unloaded" => Ok(AdapterLifecycleState::Unloaded),
            _ => Err(crate::AosError::Validation(format!(
                "Invalid lifecycle state: {}",
                s
            ))),
        }
    }
}

impl AdapterLifecycleState {
    /// Derive lifecycle state from memory tier state (current_state column)
    ///
    /// Maps existing AdapterState values to lifecycle semantics:
    /// - "unloaded" → Registered (exists in DB but not in memory)
    /// - "cold", "warm", "hot", "resident" → Loaded (in memory, can be used in stacks)
    ///
    /// Note: "Active" state is not stored; it's determined by whether the adapter
    /// is in the currently active stack.
    pub fn from_memory_tier_state(current_state: &str) -> Self {
        match current_state {
            "unloaded" => AdapterLifecycleState::Registered,
            "cold" | "warm" | "hot" | "resident" => AdapterLifecycleState::Loaded,
            _ => AdapterLifecycleState::Registered, // Defensive: treat unknown as registered
        }
    }

    /// Check if a memory tier state represents a loaded adapter
    pub fn is_loaded_state(current_state: &str) -> bool {
        matches!(current_state, "cold" | "warm" | "hot" | "resident")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        // Valid transitions
        assert!(AdapterLifecycleState::Registered
            .can_transition_to(&AdapterLifecycleState::Loaded));
        assert!(
            AdapterLifecycleState::Loaded.can_transition_to(&AdapterLifecycleState::Active)
        );
        assert!(
            AdapterLifecycleState::Active.can_transition_to(&AdapterLifecycleState::Unloaded)
        );
    }

    #[test]
    fn test_invalid_transitions() {
        // Invalid transitions
        assert!(!AdapterLifecycleState::Registered
            .can_transition_to(&AdapterLifecycleState::Active));
        assert!(!AdapterLifecycleState::Registered
            .can_transition_to(&AdapterLifecycleState::Unloaded));
        assert!(!AdapterLifecycleState::Loaded
            .can_transition_to(&AdapterLifecycleState::Registered));
        assert!(!AdapterLifecycleState::Loaded
            .can_transition_to(&AdapterLifecycleState::Unloaded));
        assert!(!AdapterLifecycleState::Active
            .can_transition_to(&AdapterLifecycleState::Registered));
        assert!(!AdapterLifecycleState::Active
            .can_transition_to(&AdapterLifecycleState::Loaded));
        assert!(!AdapterLifecycleState::Unloaded
            .can_transition_to(&AdapterLifecycleState::Registered));
        assert!(!AdapterLifecycleState::Unloaded
            .can_transition_to(&AdapterLifecycleState::Loaded));
        assert!(!AdapterLifecycleState::Unloaded
            .can_transition_to(&AdapterLifecycleState::Active));
    }

    #[test]
    fn test_valid_next_states() {
        assert_eq!(
            AdapterLifecycleState::Registered.valid_next_states(),
            vec![AdapterLifecycleState::Loaded]
        );
        assert_eq!(
            AdapterLifecycleState::Loaded.valid_next_states(),
            vec![AdapterLifecycleState::Active]
        );
        assert_eq!(
            AdapterLifecycleState::Active.valid_next_states(),
            vec![AdapterLifecycleState::Unloaded]
        );
        assert!(AdapterLifecycleState::Unloaded
            .valid_next_states()
            .is_empty());
    }

    #[test]
    fn test_is_loaded() {
        assert!(!AdapterLifecycleState::Registered.is_loaded());
        assert!(AdapterLifecycleState::Loaded.is_loaded());
        assert!(AdapterLifecycleState::Active.is_loaded());
        assert!(!AdapterLifecycleState::Unloaded.is_loaded());
    }

    #[test]
    fn test_can_be_in_stack() {
        assert!(!AdapterLifecycleState::Registered.can_be_in_stack());
        assert!(AdapterLifecycleState::Loaded.can_be_in_stack());
        assert!(AdapterLifecycleState::Active.can_be_in_stack());
        assert!(!AdapterLifecycleState::Unloaded.can_be_in_stack());
    }

    #[test]
    fn test_from_str() {
        assert_eq!(
            "registered".parse::<AdapterLifecycleState>().unwrap(),
            AdapterLifecycleState::Registered
        );
        assert_eq!(
            "loaded".parse::<AdapterLifecycleState>().unwrap(),
            AdapterLifecycleState::Loaded
        );
        assert_eq!(
            "active".parse::<AdapterLifecycleState>().unwrap(),
            AdapterLifecycleState::Active
        );
        assert_eq!(
            "unloaded".parse::<AdapterLifecycleState>().unwrap(),
            AdapterLifecycleState::Unloaded
        );
        assert!("invalid".parse::<AdapterLifecycleState>().is_err());
    }
}
