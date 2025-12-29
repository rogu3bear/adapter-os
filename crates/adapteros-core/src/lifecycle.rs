//! Adapter and Stack Lifecycle Management
//!
//! Provides types and validation for lifecycle state transitions and version management.
//!
//! # Lifecycle States
//!
//! Adapters and stacks progress through these states:
//! - **Draft**: Version created; .aos missing or incomplete
//! - **Training**: Training job running
//! - **Ready**: .aos uploaded, hash verified, basic validation passed
//! - **Active**: Selected for production traffic; eligible for routing
//! - **Deprecated**: No longer preferred; still routable for rollback
//! - **Retired**: Not allowed in new routes; kept for audit
//! - **Failed**: Training or validation failed; not routable
//!
//! # Valid Transitions
//!
//! ```text
//! Draft → Training → Ready → Active → Deprecated → Retired
//!   ↘         ↘        ↘       ↘  ↖ (rollback)     ↗
//!    └────────┴────────┴───────┴──► Failed    (ephemeral: Active → Retired)
//! ```
//!
//! ## Tier-Specific Rules
//!
//! - **Ephemeral adapters**: Cannot be deprecated; transition directly from Active to Retired
//! - **Persistent/Warm adapters**: Must follow the full lifecycle through Deprecated
//! - **Terminal states**: Retired and Failed are terminal (no transitions out)
//!
//! # Versioning
//!
//! - Versions follow semantic versioning (e.g., "1.0.0", "2.1.3")
//! - Version increments on:
//!   - Weight changes (adapter retraining)
//!   - Lifecycle state transitions
//!   - Stack composition changes
//! - All versions remain loadable until parent entity is retired
//!
//! # Examples
//!
//! ```rust
//! use adapteros_core::lifecycle::{LifecycleState, LifecycleTransition};
//!
//! // Validate a transition
//! let transition = LifecycleTransition::new(
//!     LifecycleState::Active,
//!     LifecycleState::Deprecated
//! );
//! assert!(transition.is_valid());
//!
//! // Invalid transition
//! let invalid = LifecycleTransition::new(
//!     LifecycleState::Deprecated,
//!     LifecycleState::Active
//! );
//! assert!(!invalid.is_valid());
//! ```

use crate::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Lifecycle state for adapters and stacks
///
/// Represents the availability and maturity of an adapter or stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleState {
    /// Under development, not production-ready
    Draft,
    /// Training job running
    Training,
    /// Artifact uploaded and validated
    Ready,
    /// Production-ready and available for use
    Active,
    /// Still functional but discouraged, migration recommended
    Deprecated,
    /// No longer available, cannot be loaded
    Retired,
    /// Training or validation failed; not routable
    Failed,
}

impl LifecycleState {
    /// Returns true if the state allows loading/activation
    pub fn is_loadable(&self) -> bool {
        matches!(
            self,
            LifecycleState::Ready | LifecycleState::Active | LifecycleState::Deprecated
        )
    }

    /// Returns true if the state allows modifications
    pub fn is_mutable(&self) -> bool {
        matches!(self, LifecycleState::Draft | LifecycleState::Training)
    }

    /// Returns true if this is a terminal state (no further transitions)
    pub fn is_terminal(&self) -> bool {
        matches!(self, LifecycleState::Retired | LifecycleState::Failed)
    }

    /// Returns the next valid state in the lifecycle progression
    pub fn next(&self) -> Option<LifecycleState> {
        match self {
            LifecycleState::Draft => Some(LifecycleState::Training),
            LifecycleState::Training => Some(LifecycleState::Ready),
            LifecycleState::Ready => Some(LifecycleState::Active),
            LifecycleState::Active => Some(LifecycleState::Deprecated),
            LifecycleState::Deprecated => Some(LifecycleState::Retired),
            LifecycleState::Retired => None,
            LifecycleState::Failed => None,
        }
    }

    /// Returns all states that can transition to this state
    pub fn valid_predecessors(&self) -> &'static [LifecycleState] {
        match self {
            LifecycleState::Draft => &[],
            LifecycleState::Training => &[LifecycleState::Draft],
            LifecycleState::Ready => &[LifecycleState::Training, LifecycleState::Active],
            LifecycleState::Active => &[LifecycleState::Ready],
            LifecycleState::Deprecated => &[LifecycleState::Active],
            LifecycleState::Retired => &[LifecycleState::Deprecated],
            LifecycleState::Failed => &[
                LifecycleState::Draft,
                LifecycleState::Training,
                LifecycleState::Ready,
                LifecycleState::Active,
                LifecycleState::Deprecated,
                LifecycleState::Retired,
            ],
        }
    }

    /// Check if this state is valid for the given tier
    ///
    /// Ephemeral adapters cannot be deprecated (they go directly to retired).
    pub fn is_valid_for_tier(&self, tier: &str) -> bool {
        match (self, tier) {
            // ephemeral adapters cannot be deprecated
            (LifecycleState::Deprecated, "ephemeral") => false,
            _ => true,
        }
    }

    /// Check if transition from current state to new state is valid
    ///
    /// This is equivalent to checking if `new_state` is in `self.valid_predecessors()`,
    /// but provided for compatibility with existing code.
    pub fn can_transition_to(&self, new_state: LifecycleState) -> bool {
        LifecycleTransition::new(*self, new_state).is_valid()
    }

    /// Check if transition from current state to new state is valid for a specific tier
    ///
    /// This method enforces tier-specific transition rules:
    /// - Ephemeral adapters: Active -> Retired is allowed (skip Deprecated)
    /// - Ephemeral adapters: Active -> Deprecated is blocked
    /// - Non-ephemeral adapters: must go through Deprecated before Retired
    ///
    /// # Arguments
    /// * `new_state` - The target lifecycle state
    /// * `tier` - The adapter tier (e.g., "ephemeral", "warm", "persistent")
    ///
    /// # Returns
    /// `true` if the transition is valid for the given tier, `false` otherwise
    pub fn can_transition_to_for_tier(&self, new_state: LifecycleState, tier: &str) -> bool {
        // First check if the target state is valid for this tier
        if !new_state.is_valid_for_tier(tier) {
            return false;
        }

        // For ephemeral adapters, Active -> Retired is allowed (skipping Deprecated)
        if tier == "ephemeral" {
            match (*self, new_state) {
                // Ephemeral can skip Deprecated and go directly to Retired
                (LifecycleState::Active, LifecycleState::Retired) => return true,
                // Block transition to Deprecated for ephemeral
                (_, LifecycleState::Deprecated) => return false,
                _ => {}
            }
        } else {
            // Non-ephemeral adapters cannot skip Deprecated
            if let (LifecycleState::Active, LifecycleState::Retired) = (*self, new_state) {
                return false;
            }
        }

        // Fall back to base transition rules
        self.can_transition_to(new_state)
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Training => "training",
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Deprecated => "deprecated",
            Self::Retired => "retired",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LifecycleState::Draft => write!(f, "draft"),
            LifecycleState::Training => write!(f, "training"),
            LifecycleState::Ready => write!(f, "ready"),
            LifecycleState::Active => write!(f, "active"),
            LifecycleState::Deprecated => write!(f, "deprecated"),
            LifecycleState::Retired => write!(f, "retired"),
            LifecycleState::Failed => write!(f, "failed"),
        }
    }
}

impl FromStr for LifecycleState {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(LifecycleState::Draft),
            "training" => Ok(LifecycleState::Training),
            "ready" => Ok(LifecycleState::Ready),
            "active" => Ok(LifecycleState::Active),
            "deprecated" => Ok(LifecycleState::Deprecated),
            "retired" => Ok(LifecycleState::Retired),
            "failed" => Ok(LifecycleState::Failed),
            _ => Err(AosError::Validation(format!(
                "Invalid lifecycle state: {}. Must be one of: draft, training, ready, active, deprecated, retired, failed",
                s
            ))),
        }
    }
}

/// A lifecycle state transition
///
/// Validates that transitions follow the allowed progression:
/// Draft → Training → Ready → Active → Deprecated → Retired
/// Optional rollback: Active → Ready
/// Failure path: Any → Failed
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleTransition {
    pub from: LifecycleState,
    pub to: LifecycleState,
}

impl LifecycleTransition {
    /// Create a new lifecycle transition
    pub fn new(from: LifecycleState, to: LifecycleState) -> Self {
        Self { from, to }
    }

    /// Check if this transition is valid
    ///
    /// Valid transitions:
    /// - Draft → Training
    /// - Training → Ready
    /// - Ready → Active
    /// - Active → Deprecated
    /// - Deprecated → Retired
    /// - Active → Ready (rollback)
    /// - Any state → Failed
    /// - Any state → same state (no-op)
    pub fn is_valid(&self) -> bool {
        // No-op transitions are always valid
        if self.from == self.to {
            return true;
        }

        // Check if 'to' state lists 'from' as a valid predecessor
        self.to.valid_predecessors().contains(&self.from)
    }

    /// Validate this transition, returning an error if invalid
    pub fn validate(&self) -> Result<()> {
        if self.is_valid() {
            Ok(())
        } else {
            Err(AosError::PolicyViolation(format!(
                "Invalid lifecycle transition: {} → {}. Valid transitions: draft→training→ready→active→deprecated→retired, active→ready, any→failed",
                self.from, self.to
            )))
        }
    }

    /// Get a human-readable description of why this transition is invalid
    pub fn validation_error(&self) -> Option<String> {
        if self.is_valid() {
            None
        } else {
            Some(format!(
                "Cannot transition from {} to {}. Valid next state: {}",
                self.from,
                self.to,
                self.from
                    .next()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "none (terminal state)".to_string())
            ))
        }
    }
}

/// Semantic version (major.minor.patch)
///
/// Follows semantic versioning 2.0.0 specification.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SemanticVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemanticVersion {
    /// Create a new semantic version
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Increment the major version (breaking change)
    pub fn bump_major(&mut self) {
        self.major += 1;
        self.minor = 0;
        self.patch = 0;
    }

    /// Increment the minor version (new feature, backward-compatible)
    pub fn bump_minor(&mut self) {
        self.minor += 1;
        self.patch = 0;
    }

    /// Increment the patch version (bug fix, backward-compatible)
    pub fn bump_patch(&mut self) {
        self.patch += 1;
    }

    /// Parse from string (e.g., "1.2.3")
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(AosError::Validation(format!(
                "Invalid semantic version: {}. Expected format: major.minor.patch",
                s
            )));
        }

        let major = parts[0].parse().map_err(|_| {
            AosError::Validation(format!("Invalid major version number: {}", parts[0]))
        })?;
        let minor = parts[1].parse().map_err(|_| {
            AosError::Validation(format!("Invalid minor version number: {}", parts[1]))
        })?;
        let patch = parts[2].parse().map_err(|_| {
            AosError::Validation(format!("Invalid patch version number: {}", parts[2]))
        })?;

        Ok(Self::new(major, minor, patch))
    }
}

impl fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for SemanticVersion {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// Reason for a lifecycle transition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionReason {
    /// Human-readable reason for the transition
    pub reason: String,
    /// User or system that initiated the transition
    pub initiated_by: String,
}

impl TransitionReason {
    pub fn new(reason: impl Into<String>, initiated_by: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            initiated_by: initiated_by.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_state_progression() {
        assert_eq!(LifecycleState::Draft.next(), Some(LifecycleState::Training));
        assert_eq!(LifecycleState::Training.next(), Some(LifecycleState::Ready));
        assert_eq!(LifecycleState::Ready.next(), Some(LifecycleState::Active));
        assert_eq!(
            LifecycleState::Active.next(),
            Some(LifecycleState::Deprecated)
        );
        assert_eq!(
            LifecycleState::Deprecated.next(),
            Some(LifecycleState::Retired)
        );
        assert_eq!(LifecycleState::Retired.next(), None);
        assert_eq!(LifecycleState::Failed.next(), None);
    }

    #[test]
    fn test_lifecycle_state_loadable() {
        assert!(!LifecycleState::Draft.is_loadable());
        assert!(!LifecycleState::Training.is_loadable());
        assert!(LifecycleState::Ready.is_loadable());
        assert!(LifecycleState::Active.is_loadable());
        assert!(LifecycleState::Deprecated.is_loadable());
        assert!(!LifecycleState::Retired.is_loadable());
        assert!(!LifecycleState::Failed.is_loadable());
    }

    #[test]
    fn test_lifecycle_state_mutable() {
        assert!(LifecycleState::Draft.is_mutable());
        assert!(LifecycleState::Training.is_mutable());
        assert!(!LifecycleState::Ready.is_mutable());
        assert!(!LifecycleState::Deprecated.is_mutable());
        assert!(!LifecycleState::Retired.is_mutable());
        assert!(!LifecycleState::Failed.is_mutable());
    }

    #[test]
    fn test_valid_transitions() {
        // Valid forward transitions
        assert!(
            LifecycleTransition::new(LifecycleState::Draft, LifecycleState::Training).is_valid()
        );
        assert!(
            LifecycleTransition::new(LifecycleState::Training, LifecycleState::Ready).is_valid()
        );
        assert!(LifecycleTransition::new(LifecycleState::Ready, LifecycleState::Active).is_valid());
        assert!(
            LifecycleTransition::new(LifecycleState::Active, LifecycleState::Deprecated).is_valid()
        );
        assert!(
            LifecycleTransition::new(LifecycleState::Deprecated, LifecycleState::Retired)
                .is_valid()
        );
        assert!(LifecycleTransition::new(LifecycleState::Active, LifecycleState::Ready).is_valid());
        assert!(
            LifecycleTransition::new(LifecycleState::Training, LifecycleState::Failed).is_valid()
        );
        assert!(LifecycleTransition::new(LifecycleState::Ready, LifecycleState::Failed).is_valid());
        assert!(
            LifecycleTransition::new(LifecycleState::Active, LifecycleState::Failed).is_valid()
        );

        // No-op transitions (same state)
        assert!(
            LifecycleTransition::new(LifecycleState::Active, LifecycleState::Active).is_valid()
        );
    }

    #[test]
    fn test_invalid_transitions() {
        // Backward transitions
        assert!(
            !LifecycleTransition::new(LifecycleState::Training, LifecycleState::Draft).is_valid()
        );
        assert!(!LifecycleTransition::new(LifecycleState::Ready, LifecycleState::Draft).is_valid());
        assert!(
            !LifecycleTransition::new(LifecycleState::Deprecated, LifecycleState::Active)
                .is_valid()
        );
        assert!(
            !LifecycleTransition::new(LifecycleState::Retired, LifecycleState::Active).is_valid()
        );
        assert!(
            !LifecycleTransition::new(LifecycleState::Retired, LifecycleState::Deprecated)
                .is_valid()
        );

        // Skip-ahead transitions
        assert!(!LifecycleTransition::new(LifecycleState::Draft, LifecycleState::Ready).is_valid());
        assert!(
            !LifecycleTransition::new(LifecycleState::Draft, LifecycleState::Active).is_valid()
        );
        assert!(
            !LifecycleTransition::new(LifecycleState::Draft, LifecycleState::Retired).is_valid()
        );
        assert!(
            !LifecycleTransition::new(LifecycleState::Ready, LifecycleState::Retired).is_valid()
        );
    }

    #[test]
    fn test_lifecycle_state_from_str() {
        assert_eq!(
            LifecycleState::from_str("draft").unwrap(),
            LifecycleState::Draft
        );
        assert_eq!(
            LifecycleState::from_str("training").unwrap(),
            LifecycleState::Training
        );
        assert_eq!(
            LifecycleState::from_str("ready").unwrap(),
            LifecycleState::Ready
        );
        assert_eq!(
            LifecycleState::from_str("active").unwrap(),
            LifecycleState::Active
        );
        assert_eq!(
            LifecycleState::from_str("DEPRECATED").unwrap(),
            LifecycleState::Deprecated
        );
        assert_eq!(
            LifecycleState::from_str("retired").unwrap(),
            LifecycleState::Retired
        );
        assert_eq!(
            LifecycleState::from_str("failed").unwrap(),
            LifecycleState::Failed
        );
        assert!(LifecycleState::from_str("invalid").is_err());
    }

    #[test]
    fn test_semantic_version_parsing() {
        let v = SemanticVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.to_string(), "1.2.3");

        assert!(SemanticVersion::parse("1.2").is_err());
        assert!(SemanticVersion::parse("1.2.3.4").is_err());
        assert!(SemanticVersion::parse("a.b.c").is_err());
    }

    #[test]
    fn test_semantic_version_bumping() {
        let mut v = SemanticVersion::new(1, 2, 3);

        v.bump_patch();
        assert_eq!(v.to_string(), "1.2.4");

        v.bump_minor();
        assert_eq!(v.to_string(), "1.3.0");

        v.bump_major();
        assert_eq!(v.to_string(), "2.0.0");
    }

    #[test]
    fn test_semantic_version_ordering() {
        let v1 = SemanticVersion::new(1, 0, 0);
        let v2 = SemanticVersion::new(1, 1, 0);
        let v3 = SemanticVersion::new(2, 0, 0);

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v1 < v3);
    }

    #[test]
    fn test_transition_validation() {
        let valid = LifecycleTransition::new(LifecycleState::Draft, LifecycleState::Training);
        assert!(valid.validate().is_ok());

        let invalid = LifecycleTransition::new(LifecycleState::Ready, LifecycleState::Draft);
        assert!(invalid.validate().is_err());
        assert!(invalid.validation_error().is_some());
    }
}
