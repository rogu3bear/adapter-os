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
//! # Determinism Compatibility
//!
//! Only certain lifecycle states support deterministic operations (inference/training):
//! - **Ready**: Artifact validated, deterministic operations allowed
//! - **Active**: Production state, deterministic operations allowed
//!
//! States like Draft, Training, Deprecated, Retired, and Failed do not guarantee
//! determinism because artifacts may be incomplete, in-progress, or unavailable.
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
use thiserror::Error;

/// Errors specific to lifecycle state operations
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    /// Transition would violate determinism guarantees
    #[error(
        "Determinism violation: cannot transition from {from} to {to} when determinism is required"
    )]
    DeterminismViolation {
        from: LifecycleState,
        to: LifecycleState,
    },
}

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

    /// Returns true if this state allows alias swaps.
    ///
    /// Alias swaps require stable, non-terminal states, with optional support
    /// for training adapters in controlled workflows.
    pub fn allows_alias_swap(&self, allow_training: bool) -> bool {
        matches!(self, LifecycleState::Ready | LifecycleState::Active)
            || (allow_training && *self == LifecycleState::Training)
    }

    /// Returns true if the lifecycle state allows deterministic operations.
    ///
    /// Only Ready and Active states support deterministic inference/training
    /// because these states have validated, complete artifacts.
    pub fn is_determinism_compatible(&self) -> bool {
        matches!(self, LifecycleState::Ready | LifecycleState::Active)
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
            LifecycleState::Retired => &[LifecycleState::Deprecated, LifecycleState::Active],
            LifecycleState::Failed => &[
                LifecycleState::Draft,
                LifecycleState::Training,
                LifecycleState::Ready,
                LifecycleState::Active,
                LifecycleState::Deprecated,
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

/// Validates that a state transition preserves determinism guarantees.
///
/// Returns an error if the transition would break determinism requirements.
/// Only Ready and Active states are determinism-compatible.
///
/// # Arguments
/// * `from` - The current lifecycle state
/// * `to` - The target lifecycle state
/// * `requires_determinism` - Whether the operation requires determinism
///
/// # Returns
/// `Ok(())` if the transition is valid, or `Err(LifecycleError::DeterminismViolation)`
/// if the target state doesn't support determinism when required.
pub fn validate_deterministic_transition(
    from: &LifecycleState,
    to: &LifecycleState,
    requires_determinism: bool,
) -> std::result::Result<(), LifecycleError> {
    if requires_determinism && !to.is_determinism_compatible() {
        return Err(LifecycleError::DeterminismViolation {
            from: *from,
            to: *to,
        });
    }
    Ok(())
}

/// A lifecycle state transition
///
/// Validates that transitions follow the allowed progression:
/// Draft → Training → Ready → Active → Deprecated → Retired
/// Optional rollback: Active → Ready
/// Failure path: Any non-terminal → Failed
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
    /// - Active → Retired (ephemeral tier)
    /// - Active → Ready (rollback)
    /// - Any non-terminal state → Failed
    /// - Any state → same state (no-op)
    pub fn is_valid(&self) -> bool {
        // No-op transitions are always valid
        if self.from == self.to {
            return true;
        }

        if self.from.is_terminal() {
            return false;
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
                "Invalid lifecycle transition: {} → {}. Valid transitions: draft→training→ready→active→deprecated→retired, active→retired (ephemeral), active→ready, any non-terminal→failed",
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
                match self.from {
                    LifecycleState::Active =>
                        "deprecated (or retired for ephemeral tier)".to_string(),
                    _ => self
                        .from
                        .next()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "none (terminal state)".to_string()),
                }
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

/// Preflight check status for adapter activation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PreflightStatus {
    /// Preflight not yet run
    #[default]
    Pending,
    /// Preflight passed all checks
    Passed,
    /// Preflight failed one or more checks
    Failed,
    /// Preflight skipped (e.g., for hotfix deployments)
    Skipped,
}

impl fmt::Display for PreflightStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Passed => write!(f, "passed"),
            Self::Failed => write!(f, "failed"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

/// Context for validating lifecycle transitions
///
/// Contains all the information needed to validate whether a lifecycle
/// transition should be allowed, including tier, preflight status, artifact
/// presence, training evidence, and optional overrides.
#[derive(Debug, Clone, Default)]
pub struct ValidationContext {
    /// Current adapter tier
    pub tier: Option<String>,
    /// Preflight check status
    pub preflight_status: PreflightStatus,
    /// Whether to allow bypassing preflight (for emergencies)
    pub bypass_preflight: bool,
    /// Whether an immutable artifact is available (.aos path + hash + content hash)
    pub has_artifact: bool,
    /// Whether training evidence/snapshot is available
    pub has_training_evidence: bool,
    /// Count of active references (stacks using this adapter)
    pub active_references: u64,
    /// Conflicting adapter IDs for single-active-per-repo enforcement
    pub conflicting_adapters: Vec<String>,
    /// User/system initiating the transition
    pub initiated_by: Option<String>,
    /// Reason for the transition
    pub reason: Option<String>,
    /// Whether this is a hotfix deployment
    pub is_hotfix: bool,
}

impl ValidationContext {
    /// Create a new validation context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the adapter tier
    pub fn with_tier(mut self, tier: impl Into<String>) -> Self {
        self.tier = Some(tier.into());
        self
    }

    /// Set the preflight status
    pub fn with_preflight_status(mut self, status: PreflightStatus) -> Self {
        self.preflight_status = status;
        self
    }

    /// Enable preflight bypass (requires explicit opt-in)
    pub fn with_bypass_preflight(mut self, bypass: bool) -> Self {
        self.bypass_preflight = bypass;
        self
    }

    /// Set whether the artifact is available
    pub fn with_artifact(mut self, has_artifact: bool) -> Self {
        self.has_artifact = has_artifact;
        self
    }

    /// Set whether training evidence is available
    pub fn with_training_evidence(mut self, has_training_evidence: bool) -> Self {
        self.has_training_evidence = has_training_evidence;
        self
    }

    /// Set active reference count
    pub fn with_active_references(mut self, active_references: u64) -> Self {
        self.active_references = active_references;
        self
    }

    /// Set conflicting adapter IDs
    pub fn with_conflicting_adapters(mut self, adapters: Vec<String>) -> Self {
        self.conflicting_adapters = adapters;
        self
    }

    /// Set the initiator
    pub fn with_initiated_by(mut self, initiated_by: impl Into<String>) -> Self {
        self.initiated_by = Some(initiated_by.into());
        self
    }

    /// Set the transition reason
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Mark as hotfix deployment
    pub fn with_hotfix(mut self, is_hotfix: bool) -> Self {
        self.is_hotfix = is_hotfix;
        self
    }

    /// Check if preflight requirements are satisfied for activation
    ///
    /// Returns true if:
    /// - Preflight status is Passed
    /// - Preflight bypass is explicitly enabled
    /// - This is a hotfix deployment (implicit bypass)
    pub fn preflight_satisfied(&self) -> bool {
        if self.bypass_preflight || self.is_hotfix {
            return true;
        }
        self.preflight_status == PreflightStatus::Passed
    }
}

/// Lifecycle rule enforcement result
#[derive(Debug, Clone)]
pub struct RuleViolation {
    /// Rule that was violated
    pub rule: String,
    /// Human-readable description of the violation
    pub message: String,
    /// Whether this violation can be bypassed
    pub bypassable: bool,
}

impl RuleViolation {
    pub fn new(rule: impl Into<String>, message: impl Into<String>, bypassable: bool) -> Self {
        Self {
            rule: rule.into(),
            message: message.into(),
            bypassable,
        }
    }
}

/// Validate a lifecycle transition with full context
///
/// This enforces all lifecycle rules including:
/// - Basic transition validity (state machine rules)
/// - Tier-specific transitions (ephemeral skips deprecated)
/// - Preflight requirements for activation
///
/// # Arguments
/// * `from` - Current lifecycle state
/// * `to` - Target lifecycle state
/// * `context` - Validation context with tier, preflight status, etc.
///
/// # Returns
/// Ok(()) if the transition is valid, or Err with list of violations
pub fn validate_transition_with_context(
    from: LifecycleState,
    to: LifecycleState,
    context: &ValidationContext,
) -> std::result::Result<(), Vec<RuleViolation>> {
    let tier = context.tier.as_deref().unwrap_or("warm");
    let mut ctx = serde_json::Map::new();
    ctx.insert(
        "tier".to_string(),
        serde_json::Value::String(tier.to_string()),
    );
    ctx.insert(
        "preflight_status".to_string(),
        serde_json::Value::String(context.preflight_status.to_string()),
    );
    ctx.insert(
        "bypass_preflight".to_string(),
        serde_json::Value::Bool(context.bypass_preflight),
    );
    ctx.insert(
        "is_hotfix".to_string(),
        serde_json::Value::Bool(context.is_hotfix),
    );
    ctx.insert(
        "has_artifact".to_string(),
        serde_json::Value::Bool(context.has_artifact),
    );
    ctx.insert(
        "has_training_evidence".to_string(),
        serde_json::Value::Bool(context.has_training_evidence),
    );
    ctx.insert(
        "active_references".to_string(),
        serde_json::Value::Number(context.active_references.into()),
    );
    ctx.insert(
        "conflicting_adapters".to_string(),
        serde_json::Value::Array(
            context
                .conflicting_adapters
                .iter()
                .cloned()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );

    let validator = LifecycleValidator::with_defaults();
    let result = validator.validate_transition(from, to, &serde_json::Value::Object(ctx));
    if result.valid {
        return Ok(());
    }

    let violations = result
        .errors()
        .into_iter()
        .map(|violation| {
            let rule = match violation.constraint_id.as_str() {
                "builtin:state_machine" | "builtin:tier_specific" => "state_transition",
                "builtin:artifact_required" => "artifact_required",
                "builtin:training_evidence_required" => "training_evidence_required",
                "builtin:single_active_per_repo" => "single_active_per_repo",
                "builtin:preflight_required" => "preflight_required",
                "builtin:no_active_references" => "no_active_references",
                _ => violation.constraint_id.as_str(),
            };

            RuleViolation::new(
                rule,
                violation.message.clone(),
                violation.constraint_id == "builtin:preflight_required",
            )
        })
        .collect();

    Err(violations)
}

/// Check if an adapter can be activated based on its current state and context
///
/// Convenience function that checks if transition to Active state is valid.
pub fn can_activate(
    current_state: LifecycleState,
    context: &ValidationContext,
) -> std::result::Result<(), Vec<RuleViolation>> {
    validate_transition_with_context(current_state, LifecycleState::Active, context)
}

/// Check if an adapter can be swapped into an alias target based on lifecycle constraints.
///
/// Alias swaps validate the adapter against lifecycle constraints without
/// mutating state. By default, swaps validate as if transitioning to Active.
/// When training swaps are allowed, a Training adapter is validated against
/// the Ready transition rules instead.
pub fn validate_alias_swap(
    current_state: LifecycleState,
    allow_training: bool,
    context: &ValidationContext,
) -> std::result::Result<(), Vec<RuleViolation>> {
    let target_state = if allow_training && current_state == LifecycleState::Training {
        LifecycleState::Ready
    } else {
        LifecycleState::Active
    };
    validate_transition_with_context(current_state, target_state, context)
}

// =============================================================================
// Lifecycle Rule Constraints
// =============================================================================

/// A constraint that can be applied to lifecycle transitions
///
/// Constraints define additional rules beyond the basic state machine that
/// must be satisfied for a transition to be allowed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConstraint {
    /// Unique identifier for the constraint
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description of what this constraint enforces
    pub description: Option<String>,
    /// The type of constraint
    pub constraint_type: ConstraintType,
    /// Priority (higher = evaluated first)
    pub priority: i32,
    /// Whether this constraint is enabled
    pub enabled: bool,
}

/// Types of lifecycle constraints
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    /// Requires an artifact to be present before transition
    ArtifactRequired,
    /// Requires training evidence/snapshot before transition
    TrainingEvidenceRequired,
    /// Requires no active references (e.g., stacks using this adapter)
    NoActiveReferences,
    /// Enforces single-active per repository
    SingleActivePerRepo,
    /// Enforces tier-specific transition rules
    TierSpecific,
    /// Requires preflight checks to pass
    PreflightRequired,
    /// Custom constraint with a specific rule name
    Custom(String),
}

impl ConstraintType {
    /// Get the string representation of the constraint type
    pub fn as_str(&self) -> &str {
        match self {
            ConstraintType::ArtifactRequired => "artifact_required",
            ConstraintType::TrainingEvidenceRequired => "training_evidence_required",
            ConstraintType::NoActiveReferences => "no_active_references",
            ConstraintType::SingleActivePerRepo => "single_active_per_repo",
            ConstraintType::TierSpecific => "tier_specific",
            ConstraintType::PreflightRequired => "preflight_required",
            ConstraintType::Custom(name) => name,
        }
    }
}

impl LifecycleConstraint {
    /// Create a new artifact required constraint
    pub fn artifact_required() -> Self {
        Self {
            id: "builtin:artifact_required".to_string(),
            name: "Artifact Required".to_string(),
            description: Some(
                "Requires .aos artifact (path, hash, content hash) before entering ready/active/deprecated/retired".to_string()
            ),
            constraint_type: ConstraintType::ArtifactRequired,
            priority: 100,
            enabled: true,
        }
    }

    /// Create a new training evidence required constraint
    pub fn training_evidence_required() -> Self {
        Self {
            id: "builtin:training_evidence_required".to_string(),
            name: "Training Evidence Required".to_string(),
            description: Some(
                "Requires training snapshot/metrics evidence before entering active state"
                    .to_string(),
            ),
            constraint_type: ConstraintType::TrainingEvidenceRequired,
            priority: 90,
            enabled: true,
        }
    }

    /// Create a new single-active-per-repo constraint
    pub fn single_active_per_repo() -> Self {
        Self {
            id: "builtin:single_active_per_repo".to_string(),
            name: "Single Active Per Repository".to_string(),
            description: Some(
                "Only one adapter can be active per repository/branch combination".to_string(),
            ),
            constraint_type: ConstraintType::SingleActivePerRepo,
            priority: 80,
            enabled: true,
        }
    }

    /// Create a new tier-specific constraint
    pub fn tier_specific() -> Self {
        Self {
            id: "builtin:tier_specific".to_string(),
            name: "Tier-Specific Rules".to_string(),
            description: Some(
                "Enforces tier-specific transition rules (e.g., ephemeral cannot be deprecated)"
                    .to_string(),
            ),
            constraint_type: ConstraintType::TierSpecific,
            priority: 110,
            enabled: true,
        }
    }

    /// Create a new preflight required constraint
    pub fn preflight_required() -> Self {
        Self {
            id: "builtin:preflight_required".to_string(),
            name: "Preflight Checks Required".to_string(),
            description: Some("Requires preflight checks to pass before activation".to_string()),
            constraint_type: ConstraintType::PreflightRequired,
            priority: 95,
            enabled: true,
        }
    }

    /// Create a new no-active-references constraint
    pub fn no_active_references() -> Self {
        Self {
            id: "builtin:no_active_references".to_string(),
            name: "No Active References".to_string(),
            description: Some(
                "Warns when deprecating/retiring adapters that have active stack references"
                    .to_string(),
            ),
            constraint_type: ConstraintType::NoActiveReferences,
            priority: 70,
            enabled: true,
        }
    }

    /// Create a custom constraint
    pub fn custom(
        id: impl Into<String>,
        name: impl Into<String>,
        rule_name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            constraint_type: ConstraintType::Custom(rule_name.into()),
            priority: 0,
            enabled: true,
        }
    }

    /// Set the priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set the description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Enable or disable the constraint
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Check if this constraint applies to the given transition
    pub fn applies_to_transition(&self, from: LifecycleState, to: LifecycleState) -> bool {
        if !self.enabled {
            return false;
        }

        match &self.constraint_type {
            ConstraintType::ArtifactRequired => {
                // Applies when transitioning to ready, active, deprecated, or retired
                matches!(
                    to,
                    LifecycleState::Ready
                        | LifecycleState::Active
                        | LifecycleState::Deprecated
                        | LifecycleState::Retired
                )
            }
            ConstraintType::TrainingEvidenceRequired => {
                // Applies when transitioning to active
                matches!(to, LifecycleState::Active)
            }
            ConstraintType::SingleActivePerRepo => {
                // Applies when transitioning to active
                matches!(to, LifecycleState::Active)
            }
            ConstraintType::NoActiveReferences => {
                // Applies when transitioning to deprecated or retired
                matches!(to, LifecycleState::Deprecated | LifecycleState::Retired)
            }
            ConstraintType::TierSpecific => {
                // Always applies
                true
            }
            ConstraintType::PreflightRequired => {
                // Applies when transitioning to active from ready
                matches!((from, to), (LifecycleState::Ready, LifecycleState::Active))
            }
            ConstraintType::Custom(_) => {
                // Custom constraints always apply (filtering should be done externally)
                true
            }
        }
    }
}

/// Represents a violation of a lifecycle constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintViolation {
    /// The constraint that was violated
    pub constraint_id: String,
    /// The constraint name
    pub constraint_name: String,
    /// Human-readable description of the violation
    pub message: String,
    /// Severity of the violation
    pub severity: ViolationSeverity,
    /// Additional context about the violation
    pub context: Option<serde_json::Value>,
}

/// Severity levels for constraint violations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViolationSeverity {
    /// Error - blocks the transition
    Error,
    /// Warning - allows transition but should be noted
    Warning,
    /// Info - informational only
    Info,
}

impl ConstraintViolation {
    /// Create a new error-level violation
    pub fn error(
        constraint_id: impl Into<String>,
        constraint_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            constraint_id: constraint_id.into(),
            constraint_name: constraint_name.into(),
            message: message.into(),
            severity: ViolationSeverity::Error,
            context: None,
        }
    }

    /// Create a new warning-level violation
    pub fn warning(
        constraint_id: impl Into<String>,
        constraint_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            constraint_id: constraint_id.into(),
            constraint_name: constraint_name.into(),
            message: message.into(),
            severity: ViolationSeverity::Warning,
            context: None,
        }
    }

    /// Add context to the violation
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }

    /// Check if this is a blocking error
    pub fn is_blocking(&self) -> bool {
        matches!(self.severity, ViolationSeverity::Error)
    }
}

/// Result of applying lifecycle constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintValidationResult {
    /// Whether all constraints passed (no blocking errors)
    pub valid: bool,
    /// List of all violations found
    pub violations: Vec<ConstraintViolation>,
    /// IDs of constraints that were evaluated
    pub evaluated_constraints: Vec<String>,
}

impl Default for ConstraintValidationResult {
    fn default() -> Self {
        Self::valid()
    }
}

impl ConstraintValidationResult {
    /// Create a valid result with no violations
    pub fn valid() -> Self {
        Self {
            valid: true,
            violations: Vec::new(),
            evaluated_constraints: Vec::new(),
        }
    }

    /// Create an invalid result with a single violation
    pub fn invalid(violation: ConstraintViolation) -> Self {
        Self {
            valid: false,
            violations: vec![violation],
            evaluated_constraints: Vec::new(),
        }
    }

    /// Add evaluated constraint IDs
    pub fn with_evaluated(mut self, constraint_ids: Vec<String>) -> Self {
        self.evaluated_constraints = constraint_ids;
        self
    }

    /// Merge another result into this one
    pub fn merge(&mut self, other: ConstraintValidationResult) {
        self.violations.extend(other.violations);
        self.evaluated_constraints
            .extend(other.evaluated_constraints);
        // Invalid if any blocking violations
        self.valid = self.valid && !self.violations.iter().any(|v| v.is_blocking());
    }

    /// Get all blocking errors
    pub fn errors(&self) -> Vec<&ConstraintViolation> {
        self.violations.iter().filter(|v| v.is_blocking()).collect()
    }

    /// Get all warnings
    pub fn warnings(&self) -> Vec<&ConstraintViolation> {
        self.violations
            .iter()
            .filter(|v| matches!(v.severity, ViolationSeverity::Warning))
            .collect()
    }

    /// Convert to AosError if invalid
    pub fn to_error(&self) -> Option<AosError> {
        if self.valid {
            None
        } else {
            let messages: Vec<String> = self.errors().iter().map(|v| v.message.clone()).collect();
            Some(AosError::PolicyViolation(messages.join("; ")))
        }
    }
}

/// Lifecycle validator that applies multiple constraints
///
/// This is the main entry point for validating lifecycle transitions with
/// custom constraints.
///
/// # Example
///
/// ```rust
/// use adapteros_core::lifecycle::{
///     LifecycleState, LifecycleValidator, LifecycleConstraint, ConstraintType
/// };
///
/// let validator = LifecycleValidator::new()
///     .with_constraint(LifecycleConstraint::tier_specific())
///     .with_constraint(LifecycleConstraint::artifact_required());
///
/// // Validate a transition
/// let context = serde_json::json!({
///     "tier": "persistent",
///     "has_artifact": true
/// });
///
/// let result = validator.validate_transition(
///     LifecycleState::Ready,
///     LifecycleState::Active,
///     &context
/// );
///
/// if !result.valid {
///     for error in result.errors() {
///         println!("Error: {}", error.message);
///     }
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct LifecycleValidator {
    constraints: Vec<LifecycleConstraint>,
}

impl LifecycleValidator {
    /// Create a new validator with no constraints
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a validator with default built-in constraints
    pub fn with_defaults() -> Self {
        Self::new()
            .with_constraint(LifecycleConstraint::tier_specific())
            .with_constraint(LifecycleConstraint::artifact_required())
            .with_constraint(LifecycleConstraint::training_evidence_required())
            .with_constraint(LifecycleConstraint::single_active_per_repo())
            .with_constraint(LifecycleConstraint::preflight_required())
            .with_constraint(LifecycleConstraint::no_active_references())
    }

    /// Add a constraint to the validator
    pub fn with_constraint(mut self, constraint: LifecycleConstraint) -> Self {
        self.constraints.push(constraint);
        // Sort by priority (highest first)
        self.constraints.sort_by(|a, b| b.priority.cmp(&a.priority));
        self
    }

    /// Add multiple constraints
    pub fn with_constraints(mut self, constraints: Vec<LifecycleConstraint>) -> Self {
        self.constraints.extend(constraints);
        self.constraints.sort_by(|a, b| b.priority.cmp(&a.priority));
        self
    }

    /// Get all constraints
    pub fn constraints(&self) -> &[LifecycleConstraint] {
        &self.constraints
    }

    /// Validate a lifecycle transition with the configured constraints
    ///
    /// The context should contain fields that constraints can check against:
    /// - `tier`: The adapter tier (ephemeral, warm, persistent)
    /// - `has_artifact`: Whether the artifact exists (bool)
    /// - `has_training_evidence`: Whether training evidence exists (bool)
    /// - `active_references`: Count of active references (number)
    /// - `conflicting_adapters`: List of conflicting adapter IDs (array)
    /// - `preflight_status`: Preflight check status (pending, passed, failed, skipped)
    /// - `bypass_preflight`: Whether to bypass preflight (bool)
    /// - `is_hotfix`: Whether this is a hotfix deployment (bool)
    ///
    /// # Arguments
    /// * `from` - The current lifecycle state
    /// * `to` - The target lifecycle state
    /// * `context` - JSON object with field values for constraint evaluation
    pub fn validate_transition(
        &self,
        from: LifecycleState,
        to: LifecycleState,
        context: &serde_json::Value,
    ) -> ConstraintValidationResult {
        let mut result = ConstraintValidationResult::valid();
        let mut evaluated = Vec::new();

        // First, check basic state machine validity
        let transition = LifecycleTransition::new(from, to);
        let tier = context.get("tier").and_then(|v| v.as_str());
        let transition_valid = match tier {
            Some(tier) => from.can_transition_to_for_tier(to, tier),
            None => transition.is_valid(),
        };
        if !transition_valid {
            let detail = match tier {
                Some(tier) => format!("Invalid transition: {} -> {} for tier '{}'", from, to, tier),
                None => format!(
                    "Invalid transition: {} -> {}. {}",
                    from,
                    to,
                    transition.validation_error().unwrap_or_default()
                ),
            };
            return ConstraintValidationResult::invalid(ConstraintViolation::error(
                "builtin:state_machine",
                "State Machine",
                detail,
            ));
        }

        // Apply each constraint
        for constraint in &self.constraints {
            if !constraint.applies_to_transition(from, to) {
                continue;
            }

            evaluated.push(constraint.id.clone());

            if let Some(violation) = self.evaluate_constraint(constraint, from, to, context) {
                result.violations.push(violation);
                if result.violations.iter().any(|v| v.is_blocking()) {
                    result.valid = false;
                }
            }
        }

        result.evaluated_constraints = evaluated;
        result
    }

    /// Validate a transition for a specific tier
    ///
    /// Convenience method that adds tier information to the context.
    pub fn validate_transition_for_tier(
        &self,
        from: LifecycleState,
        to: LifecycleState,
        tier: &str,
        context: &serde_json::Value,
    ) -> ConstraintValidationResult {
        // Merge tier into context
        let mut ctx = context.clone();
        if let serde_json::Value::Object(ref mut map) = ctx {
            map.insert(
                "tier".to_string(),
                serde_json::Value::String(tier.to_string()),
            );
        }

        self.validate_transition(from, to, &ctx)
    }

    /// Evaluate a single constraint
    fn evaluate_constraint(
        &self,
        constraint: &LifecycleConstraint,
        from: LifecycleState,
        to: LifecycleState,
        context: &serde_json::Value,
    ) -> Option<ConstraintViolation> {
        match &constraint.constraint_type {
            ConstraintType::TierSpecific => {
                self.evaluate_tier_constraint(constraint, from, to, context)
            }
            ConstraintType::ArtifactRequired => {
                self.evaluate_artifact_constraint(constraint, context)
            }
            ConstraintType::TrainingEvidenceRequired => {
                self.evaluate_training_evidence_constraint(constraint, context)
            }
            ConstraintType::SingleActivePerRepo => {
                self.evaluate_single_active_constraint(constraint, context)
            }
            ConstraintType::NoActiveReferences => {
                self.evaluate_no_references_constraint(constraint, context)
            }
            ConstraintType::PreflightRequired => {
                self.evaluate_preflight_constraint(constraint, context)
            }
            ConstraintType::Custom(_) => {
                // Custom constraints require external evaluation
                None
            }
        }
    }

    fn evaluate_tier_constraint(
        &self,
        constraint: &LifecycleConstraint,
        from: LifecycleState,
        to: LifecycleState,
        context: &serde_json::Value,
    ) -> Option<ConstraintViolation> {
        let tier = context
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("persistent");

        if !from.can_transition_to_for_tier(to, tier) {
            Some(ConstraintViolation::error(
                &constraint.id,
                &constraint.name,
                format!(
                    "Transition from '{}' to '{}' is not allowed for tier '{}'",
                    from, to, tier
                ),
            ))
        } else {
            None
        }
    }

    fn evaluate_artifact_constraint(
        &self,
        constraint: &LifecycleConstraint,
        context: &serde_json::Value,
    ) -> Option<ConstraintViolation> {
        let has_artifact = context
            .get("has_artifact")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !has_artifact {
            Some(ConstraintViolation::error(
                &constraint.id,
                &constraint.name,
                "Immutable .aos artifact (path, hash, content hash) required before entering ready/active/deprecated/retired".to_string(),
            ))
        } else {
            None
        }
    }

    fn evaluate_training_evidence_constraint(
        &self,
        constraint: &LifecycleConstraint,
        context: &serde_json::Value,
    ) -> Option<ConstraintViolation> {
        let has_evidence = context
            .get("has_training_evidence")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !has_evidence {
            Some(ConstraintViolation::error(
                &constraint.id,
                &constraint.name,
                "Active state requires a training snapshot/metrics evidence".to_string(),
            ))
        } else {
            None
        }
    }

    fn evaluate_single_active_constraint(
        &self,
        constraint: &LifecycleConstraint,
        context: &serde_json::Value,
    ) -> Option<ConstraintViolation> {
        let conflicting = context
            .get("conflicting_adapters")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len())
            .unwrap_or(0);

        if conflicting > 0 {
            let adapters: Vec<String> = context
                .get("conflicting_adapters")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            Some(
                ConstraintViolation::error(
                    &constraint.id,
                    &constraint.name,
                    format!(
                        "Active state requires uniqueness per repo/branch; adapter(s) {} already active",
                        adapters.join(", ")
                    ),
                )
                .with_context(serde_json::json!({
                    "conflicting_adapters": adapters
                })),
            )
        } else {
            None
        }
    }

    fn evaluate_no_references_constraint(
        &self,
        constraint: &LifecycleConstraint,
        context: &serde_json::Value,
    ) -> Option<ConstraintViolation> {
        let active_refs = context
            .get("active_references")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if active_refs > 0 {
            Some(
                ConstraintViolation::warning(
                    &constraint.id,
                    &constraint.name,
                    format!(
                        "{} active stack(s) reference this adapter. Consider updating them before deprecating.",
                        active_refs
                    ),
                )
                .with_context(serde_json::json!({
                    "active_references": active_refs
                })),
            )
        } else {
            None
        }
    }

    fn evaluate_preflight_constraint(
        &self,
        constraint: &LifecycleConstraint,
        context: &serde_json::Value,
    ) -> Option<ConstraintViolation> {
        // Check bypass conditions first
        let bypass_preflight = context
            .get("bypass_preflight")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let is_hotfix = context
            .get("is_hotfix")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if bypass_preflight || is_hotfix {
            return None;
        }

        // Check preflight status
        let preflight_status = context
            .get("preflight_status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending");

        if preflight_status != "passed" {
            Some(ConstraintViolation::error(
                &constraint.id,
                &constraint.name,
                format!(
                    "Preflight checks must pass before activation. Current status: {}",
                    preflight_status
                ),
            ))
        } else {
            None
        }
    }
}

impl LifecycleTransition {
    /// Validate this transition with the given constraints
    ///
    /// This is a convenience method that creates a validator and applies the given constraints.
    pub fn validate_with_constraints(
        &self,
        constraints: &[LifecycleConstraint],
        context: &serde_json::Value,
    ) -> ConstraintValidationResult {
        let validator = LifecycleValidator::new().with_constraints(constraints.to_vec());
        validator.validate_transition(self.from, self.to, context)
    }

    /// Validate this transition for a specific tier
    ///
    /// Returns an error if the transition is not valid for the given tier.
    pub fn validate_for_tier(&self, tier: &str) -> Result<()> {
        if !self.from.can_transition_to_for_tier(self.to, tier) {
            return Err(AosError::PolicyViolation(format!(
                "Transition from '{}' to '{}' is not allowed for tier '{}'",
                self.from, self.to, tier
            )));
        }
        Ok(())
    }
}

/// Apply lifecycle rule constraints to a transition
///
/// This is the main entry point for applying lifecycle rule constraints.
/// It validates that all constraints are satisfied before allowing the transition.
///
/// # Arguments
/// * `from` - The current lifecycle state
/// * `to` - The target lifecycle state
/// * `context` - JSON context including tier, preflight status, etc.
/// * `constraints` - List of constraints to apply
///
/// # Returns
/// A `ConstraintValidationResult` indicating whether the transition is allowed
///
/// # Example
///
/// ```rust
/// use adapteros_core::lifecycle::{
///     apply_lifecycle_constraints, LifecycleState, LifecycleConstraint
/// };
///
/// let constraints = vec![
///     LifecycleConstraint::tier_specific(),
///     LifecycleConstraint::artifact_required(),
/// ];
///
/// let context = serde_json::json!({
///     "tier": "persistent",
///     "has_artifact": true
/// });
///
/// let result = apply_lifecycle_constraints(
///     LifecycleState::Ready,
///     LifecycleState::Active,
///     &context,
///     &constraints
/// );
///
/// if !result.valid {
///     for error in result.errors() {
///         eprintln!("Constraint violation: {}", error.message);
///     }
/// }
/// ```
pub fn apply_lifecycle_constraints(
    from: LifecycleState,
    to: LifecycleState,
    context: &serde_json::Value,
    constraints: &[LifecycleConstraint],
) -> ConstraintValidationResult {
    let validator = LifecycleValidator::new().with_constraints(constraints.to_vec());
    validator.validate_transition(from, to, context)
}

/// Apply default lifecycle constraints to a transition
///
/// Uses the built-in default constraints:
/// - Tier-specific rules
/// - Artifact required for ready/active/deprecated/retired
/// - Training evidence required for active
/// - Single-active per repository
/// - Preflight required for activation
/// - No active references warning for deprecation/retirement
pub fn apply_default_constraints(
    from: LifecycleState,
    to: LifecycleState,
    context: &serde_json::Value,
) -> ConstraintValidationResult {
    let validator = LifecycleValidator::with_defaults();
    validator.validate_transition(from, to, context)
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
        assert!(
            LifecycleTransition::new(LifecycleState::Active, LifecycleState::Retired).is_valid()
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
        assert!(
            !LifecycleTransition::new(LifecycleState::Retired, LifecycleState::Failed).is_valid()
        );
        assert!(
            !LifecycleTransition::new(LifecycleState::Failed, LifecycleState::Ready).is_valid()
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

    #[test]
    fn test_validation_context_preflight_satisfied() {
        // Preflight pending - not satisfied
        let ctx = ValidationContext::new();
        assert!(!ctx.preflight_satisfied());

        // Preflight passed - satisfied
        let ctx = ValidationContext::new().with_preflight_status(PreflightStatus::Passed);
        assert!(ctx.preflight_satisfied());

        // Preflight failed - not satisfied
        let ctx = ValidationContext::new().with_preflight_status(PreflightStatus::Failed);
        assert!(!ctx.preflight_satisfied());

        // Preflight bypass - satisfied even if pending
        let ctx = ValidationContext::new().with_bypass_preflight(true);
        assert!(ctx.preflight_satisfied());

        // Hotfix deployment - satisfied even if pending
        let ctx = ValidationContext::new().with_hotfix(true);
        assert!(ctx.preflight_satisfied());
    }

    #[test]
    fn test_validate_transition_with_preflight() {
        // Ready -> Active without preflight should fail
        let ctx = ValidationContext::new()
            .with_tier("warm")
            .with_artifact(true)
            .with_training_evidence(true);
        let result =
            validate_transition_with_context(LifecycleState::Ready, LifecycleState::Active, &ctx);
        assert!(result.is_err());
        let violations = result.unwrap_err();
        assert!(violations.iter().any(|v| v.rule == "preflight_required"));

        // Ready -> Active with preflight passed should succeed
        let ctx = ValidationContext::new()
            .with_tier("warm")
            .with_preflight_status(PreflightStatus::Passed)
            .with_artifact(true)
            .with_training_evidence(true);
        let result =
            validate_transition_with_context(LifecycleState::Ready, LifecycleState::Active, &ctx);
        assert!(result.is_ok());

        // Ready -> Active with bypass should succeed
        let ctx = ValidationContext::new()
            .with_tier("warm")
            .with_bypass_preflight(true)
            .with_artifact(true)
            .with_training_evidence(true);
        let result =
            validate_transition_with_context(LifecycleState::Ready, LifecycleState::Active, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_can_activate() {
        // From Ready with preflight passed
        let ctx = ValidationContext::new()
            .with_tier("warm")
            .with_preflight_status(PreflightStatus::Passed)
            .with_artifact(true)
            .with_training_evidence(true);
        assert!(can_activate(LifecycleState::Ready, &ctx).is_ok());

        // From Ready without preflight
        let ctx = ValidationContext::new()
            .with_tier("warm")
            .with_artifact(true)
            .with_training_evidence(true);
        assert!(can_activate(LifecycleState::Ready, &ctx).is_err());

        // From Draft (invalid transition)
        let ctx = ValidationContext::new()
            .with_tier("warm")
            .with_preflight_status(PreflightStatus::Passed)
            .with_artifact(true)
            .with_training_evidence(true);
        assert!(can_activate(LifecycleState::Draft, &ctx).is_err());
    }

    #[test]
    fn test_determinism_compatible() {
        // Only Ready and Active support determinism
        assert!(!LifecycleState::Draft.is_determinism_compatible());
        assert!(!LifecycleState::Training.is_determinism_compatible());
        assert!(LifecycleState::Ready.is_determinism_compatible());
        assert!(LifecycleState::Active.is_determinism_compatible());
        assert!(!LifecycleState::Deprecated.is_determinism_compatible());
        assert!(!LifecycleState::Retired.is_determinism_compatible());
        assert!(!LifecycleState::Failed.is_determinism_compatible());
    }

    #[test]
    fn test_allows_alias_swap() {
        assert!(LifecycleState::Ready.allows_alias_swap(false));
        assert!(LifecycleState::Active.allows_alias_swap(false));
        assert!(!LifecycleState::Training.allows_alias_swap(false));
        assert!(LifecycleState::Training.allows_alias_swap(true));
        assert!(!LifecycleState::Draft.allows_alias_swap(true));
        assert!(!LifecycleState::Deprecated.allows_alias_swap(true));
        assert!(!LifecycleState::Retired.allows_alias_swap(true));
        assert!(!LifecycleState::Failed.allows_alias_swap(true));
    }

    #[test]
    fn test_validate_deterministic_transition() {
        // Transitions to determinism-compatible states should succeed
        assert!(validate_deterministic_transition(
            &LifecycleState::Training,
            &LifecycleState::Ready,
            true
        )
        .is_ok());
        assert!(validate_deterministic_transition(
            &LifecycleState::Ready,
            &LifecycleState::Active,
            true
        )
        .is_ok());

        // Transitions to non-determinism-compatible states should fail when required
        let result = validate_deterministic_transition(
            &LifecycleState::Active,
            &LifecycleState::Deprecated,
            true,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LifecycleError::DeterminismViolation { .. }
        ));

        // When determinism is not required, any transition is allowed
        assert!(validate_deterministic_transition(
            &LifecycleState::Active,
            &LifecycleState::Deprecated,
            false
        )
        .is_ok());
    }

    #[test]
    fn test_tier_specific_state_validity() {
        // Ephemeral adapters cannot be deprecated
        assert!(!LifecycleState::Deprecated.is_valid_for_tier("ephemeral"));
        assert!(LifecycleState::Active.is_valid_for_tier("ephemeral"));
        assert!(LifecycleState::Ready.is_valid_for_tier("ephemeral"));
        assert!(LifecycleState::Retired.is_valid_for_tier("ephemeral"));
        assert!(LifecycleState::Failed.is_valid_for_tier("ephemeral"));

        // Persistent and warm adapters can be in any state
        assert!(LifecycleState::Deprecated.is_valid_for_tier("persistent"));
        assert!(LifecycleState::Deprecated.is_valid_for_tier("warm"));
    }

    #[test]
    fn test_tier_specific_transitions_ephemeral() {
        // Ephemeral adapters can go directly from Active to Retired
        assert!(
            LifecycleState::Active.can_transition_to_for_tier(LifecycleState::Retired, "ephemeral")
        );

        // Ephemeral adapters cannot go to Deprecated
        assert!(!LifecycleState::Active
            .can_transition_to_for_tier(LifecycleState::Deprecated, "ephemeral"));

        // Ephemeral adapters can still use standard transitions
        assert!(
            LifecycleState::Draft.can_transition_to_for_tier(LifecycleState::Training, "ephemeral")
        );
        assert!(
            LifecycleState::Training.can_transition_to_for_tier(LifecycleState::Ready, "ephemeral")
        );
        assert!(
            LifecycleState::Ready.can_transition_to_for_tier(LifecycleState::Active, "ephemeral")
        );
        assert!(
            LifecycleState::Active.can_transition_to_for_tier(LifecycleState::Failed, "ephemeral")
        );
    }

    #[test]
    fn test_validator_allows_ephemeral_active_to_retired() {
        let validator =
            LifecycleValidator::new().with_constraint(LifecycleConstraint::tier_specific());
        let context = serde_json::json!({"tier": "ephemeral"});

        let result = validator.validate_transition(
            LifecycleState::Active,
            LifecycleState::Retired,
            &context,
        );

        assert!(
            result.valid,
            "Expected Active -> Retired to be valid for ephemeral tier"
        );
        assert!(result.violations.is_empty());
    }

    #[test]
    fn test_tier_specific_transitions_persistent() {
        // Persistent adapters MUST go through Deprecated before Retired
        assert!(!LifecycleState::Active
            .can_transition_to_for_tier(LifecycleState::Retired, "persistent"));
        assert!(LifecycleState::Active
            .can_transition_to_for_tier(LifecycleState::Deprecated, "persistent"));
        assert!(LifecycleState::Deprecated
            .can_transition_to_for_tier(LifecycleState::Retired, "persistent"));

        // Warm adapters follow the same rules as persistent
        assert!(!LifecycleState::Active.can_transition_to_for_tier(LifecycleState::Retired, "warm"));
        assert!(
            LifecycleState::Active.can_transition_to_for_tier(LifecycleState::Deprecated, "warm")
        );
    }

    #[test]
    fn test_terminal_states_for_all_tiers() {
        // Retired is terminal for all tiers
        for tier in &["ephemeral", "warm", "persistent"] {
            assert!(
                !LifecycleState::Retired.can_transition_to_for_tier(LifecycleState::Active, tier)
            );
            assert!(
                !LifecycleState::Retired.can_transition_to_for_tier(LifecycleState::Draft, tier)
            );
        }

        // Failed is terminal for all tiers
        for tier in &["ephemeral", "warm", "persistent"] {
            assert!(
                !LifecycleState::Failed.can_transition_to_for_tier(LifecycleState::Active, tier)
            );
            assert!(!LifecycleState::Failed.can_transition_to_for_tier(LifecycleState::Draft, tier));
        }
    }

    #[test]
    fn test_rollback_transition_all_tiers() {
        // Active -> Ready (rollback) should be valid for all tiers
        for tier in &["ephemeral", "warm", "persistent"] {
            assert!(
                LifecycleState::Active.can_transition_to_for_tier(LifecycleState::Ready, tier),
                "Rollback Active -> Ready should be valid for tier: {}",
                tier
            );
        }
    }
}
