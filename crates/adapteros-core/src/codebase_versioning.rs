//! Codebase adapter versioning logic
//!
//! Provides policies and utilities for automatic and explicit versioning of codebase adapters.
//!
//! # Versioning Triggers
//!
//! Codebase adapters can be versioned in two ways:
//! 1. **Threshold-based** (automatic): When `activation_count >= versioning_threshold`
//! 2. **Explicit**: Manual version creation via API
//!
//! # Version Bumping
//!
//! Versions follow semantic versioning patterns:
//! - **Patch**: Minor activation-based increments
//! - **Minor**: Feature updates within same codebase scope
//! - **Major**: Breaking changes or repo restructure
//!
//! 【2025-01-29†prd-adapters†codebase_versioning】

use crate::lifecycle::SemanticVersion;
use serde::{Deserialize, Serialize};

/// Default versioning threshold (activations before auto-version)
pub const DEFAULT_VERSIONING_THRESHOLD: i32 = 100;

/// Maximum allowed versioning threshold
pub const MAX_VERSIONING_THRESHOLD: i32 = 10000;

/// Minimum allowed versioning threshold
pub const MIN_VERSIONING_THRESHOLD: i32 = 1;

/// Versioning policy for codebase adapters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersioningPolicy {
    /// Activation count threshold for auto-versioning (default: 100)
    pub activation_threshold: i32,

    /// Optional size threshold in bytes for version creation
    pub size_threshold_bytes: Option<u64>,

    /// Whether to auto-version on session end
    pub version_on_session_end: bool,
}

impl Default for VersioningPolicy {
    fn default() -> Self {
        Self {
            activation_threshold: DEFAULT_VERSIONING_THRESHOLD,
            size_threshold_bytes: None,
            version_on_session_end: false,
        }
    }
}

impl VersioningPolicy {
    /// Create a new versioning policy with default threshold
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a policy with a custom activation threshold
    pub fn with_threshold(threshold: i32) -> Self {
        Self {
            activation_threshold: threshold
                .clamp(MIN_VERSIONING_THRESHOLD, MAX_VERSIONING_THRESHOLD),
            ..Default::default()
        }
    }

    /// Create a strict policy that versions on every session end
    pub fn strict() -> Self {
        Self {
            activation_threshold: DEFAULT_VERSIONING_THRESHOLD,
            size_threshold_bytes: None,
            version_on_session_end: true,
        }
    }
}

/// Version bump type for codebase adapters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionBump {
    /// Patch bump (0.0.x -> 0.0.x+1): Minor activation-based increments
    Patch,
    /// Minor bump (0.x.0 -> 0.x+1.0): Feature updates within same scope
    Minor,
    /// Major bump (x.0.0 -> x+1.0.0): Breaking changes or repo restructure
    Major,
}

impl VersionBump {
    /// Apply this bump to a semantic version
    pub fn apply(&self, version: &SemanticVersion) -> SemanticVersion {
        let mut new_version = version.clone();
        match self {
            VersionBump::Patch => new_version.bump_patch(),
            VersionBump::Minor => new_version.bump_minor(),
            VersionBump::Major => new_version.bump_major(),
        }
        new_version
    }
}

/// Context for versioning decision
#[derive(Debug, Clone)]
pub struct VersioningContext {
    /// Current activation count
    pub activation_count: i64,

    /// Configured versioning threshold
    pub versioning_threshold: i32,

    /// Current version string (e.g., "1.2.3")
    pub current_version: String,

    /// Whether the session is ending
    pub session_ending: bool,

    /// Optional size of adapter in bytes
    pub adapter_size_bytes: Option<u64>,
}

/// Result of versioning check
#[derive(Debug, Clone)]
pub struct VersioningDecision {
    /// Whether versioning should occur
    pub should_version: bool,

    /// Recommended bump type
    pub bump_type: VersionBump,

    /// Reason for the decision
    pub reason: VersioningReason,

    /// The next version string if versioning
    pub next_version: Option<String>,
}

/// Reason for versioning decision
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersioningReason {
    /// Activation count exceeded threshold
    ThresholdExceeded,
    /// Session ended with versioning policy
    SessionEnded,
    /// Size threshold exceeded
    SizeThresholdExceeded,
    /// Explicit version request
    ExplicitRequest,
    /// Threshold not yet reached
    ThresholdNotReached,
    /// No versioning policy active
    NoPolicyActive,
}

impl VersioningDecision {
    /// Create a decision to not version
    pub fn no_version(reason: VersioningReason) -> Self {
        Self {
            should_version: false,
            bump_type: VersionBump::Patch,
            reason,
            next_version: None,
        }
    }

    /// Create a decision to version
    pub fn version(bump_type: VersionBump, reason: VersioningReason, next_version: String) -> Self {
        Self {
            should_version: true,
            bump_type,
            reason,
            next_version: Some(next_version),
        }
    }
}

/// Check if a codebase adapter should auto-version based on activation count.
///
/// This is the primary versioning check called after each activation increment.
///
/// # Arguments
///
/// * `activation_count` - Current activation count
/// * `versioning_threshold` - Configured threshold for auto-versioning
///
/// # Returns
///
/// `true` if `activation_count >= versioning_threshold`
pub fn should_auto_version(activation_count: i64, versioning_threshold: i32) -> bool {
    activation_count >= versioning_threshold as i64
}

/// Evaluate versioning for a codebase adapter.
///
/// Considers all versioning triggers (threshold, session end, size) and returns
/// a decision with the recommended action.
///
/// # Arguments
///
/// * `context` - The versioning context with current state
/// * `policy` - The versioning policy to apply
///
/// # Returns
///
/// A `VersioningDecision` indicating whether to version and why.
pub fn evaluate_versioning(
    context: &VersioningContext,
    policy: &VersioningPolicy,
) -> VersioningDecision {
    // Parse current version
    let current = context
        .current_version
        .parse::<SemanticVersion>()
        .unwrap_or_else(|_| SemanticVersion::new(0, 0, 1));

    // Check threshold first (most common trigger)
    if should_auto_version(context.activation_count, policy.activation_threshold) {
        let next = VersionBump::Patch.apply(&current);
        return VersioningDecision::version(
            VersionBump::Patch,
            VersioningReason::ThresholdExceeded,
            next.to_string(),
        );
    }

    // Check size threshold if configured
    if let (Some(size_limit), Some(current_size)) =
        (policy.size_threshold_bytes, context.adapter_size_bytes)
    {
        if current_size >= size_limit {
            let next = VersionBump::Patch.apply(&current);
            return VersioningDecision::version(
                VersionBump::Patch,
                VersioningReason::SizeThresholdExceeded,
                next.to_string(),
            );
        }
    }

    // Check session ending with version policy
    if context.session_ending && policy.version_on_session_end {
        let next = VersionBump::Patch.apply(&current);
        return VersioningDecision::version(
            VersionBump::Patch,
            VersioningReason::SessionEnded,
            next.to_string(),
        );
    }

    // No versioning needed
    VersioningDecision::no_version(VersioningReason::ThresholdNotReached)
}

/// Create parameters for a new version of a codebase adapter.
///
/// This generates the fields needed to register the new version as a child
/// of the source adapter, preserving lineage.
///
/// # Arguments
///
/// * `source_adapter_id` - The adapter being versioned
/// * `source_version` - Current version string
/// * `bump_type` - Type of version bump
/// * `base_adapter_id` - The core adapter this codebase extends
///
/// # Returns
///
/// Tuple of (new_version_string, parent_id for lineage)
pub fn create_version_params(
    source_adapter_id: &str,
    source_version: &str,
    bump_type: VersionBump,
    _base_adapter_id: &str,
) -> (String, String) {
    let current = source_version
        .parse::<SemanticVersion>()
        .unwrap_or_else(|_| SemanticVersion::new(0, 0, 1));

    let next = bump_type.apply(&current);

    (next.to_string(), source_adapter_id.to_string())
}

/// Generate a new adapter ID for a versioned codebase adapter.
///
/// Format: `{base_adapter_id}-v{major}.{minor}.{patch}`
///
/// # Arguments
///
/// * `base_adapter_id` - The core adapter ID
/// * `version` - The new version
///
/// # Returns
///
/// The new adapter ID string
pub fn generate_versioned_adapter_id(base_adapter_id: &str, version: &SemanticVersion) -> String {
    format!("{}-v{}", base_adapter_id, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_auto_version() {
        assert!(!should_auto_version(0, 100));
        assert!(!should_auto_version(99, 100));
        assert!(should_auto_version(100, 100));
        assert!(should_auto_version(101, 100));
        assert!(should_auto_version(1000, 100));
    }

    #[test]
    fn test_versioning_policy_default() {
        let policy = VersioningPolicy::default();
        assert_eq!(policy.activation_threshold, DEFAULT_VERSIONING_THRESHOLD);
        assert!(!policy.version_on_session_end);
    }

    #[test]
    fn test_versioning_policy_with_threshold() {
        let policy = VersioningPolicy::with_threshold(50);
        assert_eq!(policy.activation_threshold, 50);

        // Test clamping
        let policy = VersioningPolicy::with_threshold(0);
        assert_eq!(policy.activation_threshold, MIN_VERSIONING_THRESHOLD);

        let policy = VersioningPolicy::with_threshold(99999);
        assert_eq!(policy.activation_threshold, MAX_VERSIONING_THRESHOLD);
    }

    #[test]
    fn test_version_bump_apply() {
        let v = SemanticVersion::new(1, 2, 3);

        let patched = VersionBump::Patch.apply(&v);
        assert_eq!(patched.to_string(), "1.2.4");

        let minor = VersionBump::Minor.apply(&v);
        assert_eq!(minor.to_string(), "1.3.0");

        let major = VersionBump::Major.apply(&v);
        assert_eq!(major.to_string(), "2.0.0");
    }

    #[test]
    fn test_evaluate_versioning_threshold_exceeded() {
        let context = VersioningContext {
            activation_count: 100,
            versioning_threshold: 100,
            current_version: "1.0.0".to_string(),
            session_ending: false,
            adapter_size_bytes: None,
        };
        let policy = VersioningPolicy::default();

        let decision = evaluate_versioning(&context, &policy);
        assert!(decision.should_version);
        assert_eq!(decision.reason, VersioningReason::ThresholdExceeded);
        assert_eq!(decision.next_version, Some("1.0.1".to_string()));
    }

    #[test]
    fn test_evaluate_versioning_below_threshold() {
        let context = VersioningContext {
            activation_count: 50,
            versioning_threshold: 100,
            current_version: "1.0.0".to_string(),
            session_ending: false,
            adapter_size_bytes: None,
        };
        let policy = VersioningPolicy::default();

        let decision = evaluate_versioning(&context, &policy);
        assert!(!decision.should_version);
        assert_eq!(decision.reason, VersioningReason::ThresholdNotReached);
    }

    #[test]
    fn test_evaluate_versioning_session_end() {
        let context = VersioningContext {
            activation_count: 50,
            versioning_threshold: 100,
            current_version: "1.0.0".to_string(),
            session_ending: true,
            adapter_size_bytes: None,
        };
        let policy = VersioningPolicy::strict();

        let decision = evaluate_versioning(&context, &policy);
        assert!(decision.should_version);
        assert_eq!(decision.reason, VersioningReason::SessionEnded);
    }

    #[test]
    fn test_create_version_params() {
        let (new_version, parent_id) = create_version_params(
            "code.myrepo.abc123",
            "1.2.3",
            VersionBump::Minor,
            "core-adapter",
        );

        assert_eq!(new_version, "1.3.0");
        assert_eq!(parent_id, "code.myrepo.abc123");
    }

    #[test]
    fn test_generate_versioned_adapter_id() {
        let version = SemanticVersion::new(2, 1, 0);
        let id = generate_versioned_adapter_id("core-adapter", &version);
        assert_eq!(id, "core-adapter-v2.1.0");
    }
}
