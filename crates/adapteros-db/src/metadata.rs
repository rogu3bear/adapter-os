/// Canonical metadata schemas for adapters and stacks
///
/// This module defines the single source of truth for adapter and stack metadata,
/// ensuring consistency across storage, APIs, and telemetry.
///
/// **PRD-02: Adapter & Stack Metadata Normalization + Version Guarantees**
///
/// Citation: PRD-02 (2025-11-17)

use serde::{Deserialize, Serialize};
use std::str::FromStr;

// Re-export lifecycle types from core
pub use adapteros_core::LifecycleState;

/// API schema version for backward compatibility tracking
pub const API_SCHEMA_VERSION: &str = "1.0.0";

/// Canonical adapter metadata
///
/// This struct represents the authoritative schema for adapter metadata.
/// All API responses and telemetry bundles should derive from this struct.
///
/// **Version Guarantees:**
/// - Minor version changes: additive fields only (backward compatible)
/// - Major version changes: require explicit migration path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMeta {
    // Core identity
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub adapter_id: Option<String>,

    // Versioning
    pub version: String,             // Semantic version (e.g., "1.0.0") or monotonic
    pub lifecycle_state: LifecycleState,  // draft/active/deprecated/retired

    // Classification
    pub category: String,
    pub scope: String,
    pub tier: String,                // ephemeral/persistent/warm

    // Technical metadata
    pub hash_b3: String,
    pub rank: i32,
    pub alpha: f64,
    pub targets_json: String,
    pub acl_json: Option<String>,

    // Semantic naming (from migration 0061)
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,

    // Fork metadata
    pub parent_id: Option<String>,
    pub fork_type: Option<ForkType>,
    pub fork_reason: Option<String>,

    // Framework metadata
    pub framework: Option<String>,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub languages_json: Option<String>,

    // Source tracking
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub intent: Option<String>,

    // Runtime state
    pub current_state: String,       // Runtime loading state (unloaded/cold/warm/hot/resident)
    pub load_state: String,
    pub pinned: bool,
    pub memory_bytes: i64,
    pub last_activated: Option<String>,
    pub activation_count: i64,

    // TTL
    pub expires_at: Option<String>,

    // Timestamps
    pub created_at: String,
    pub updated_at: String,
    pub last_loaded_at: Option<String>,
}

/// Canonical adapter stack metadata
///
/// This struct represents the authoritative schema for adapter stack metadata.
/// All API responses should derive from this struct.
///
/// **Version Guarantees:**
/// - Minor version changes: additive fields only (backward compatible)
/// - Major version changes: require explicit migration path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterStackMeta {
    // Core identity
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,

    // Versioning
    pub version: String,             // Semantic version (e.g., "1.0.0") or monotonic
    pub lifecycle_state: LifecycleState,  // draft/active/deprecated/retired

    // Configuration
    pub adapter_ids: Vec<String>,
    pub workflow_type: Option<WorkflowType>,

    // Timestamps and audit
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
}

// LifecycleState is re-exported from adapteros_core (see top of file)
// This consolidation eliminates duplicate definitions and ensures
// lifecycle logic is centralized in the core crate.
//
// Legacy comment preserved for reference:
// Adapter/Stack lifecycle state
//
// **State Transition Rules:**
// - draft → active → deprecated → retired
// - retired is a terminal state (no transitions out)
// - ephemeral adapters: draft → active → retired (skip deprecated)

/// Fork type for adapter lineage tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForkType {
    Parameter,    // Parameter tuning fork
    Data,         // Data modification fork
    Architecture, // Architecture modification fork
}

impl ForkType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "parameter" => Some(Self::Parameter),
            "data" => Some(Self::Data),
            "architecture" => Some(Self::Architecture),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Parameter => "parameter",
            Self::Data => "data",
            Self::Architecture => "architecture",
        }
    }
}

/// Workflow type for adapter stacks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    Parallel,
    UpstreamDownstream,
    Sequential,
}

impl WorkflowType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "parallel" => Some(Self::Parallel),
            "upstreamdownstream" => Some(Self::UpstreamDownstream),
            "sequential" => Some(Self::Sequential),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Parallel => "Parallel",
            Self::UpstreamDownstream => "UpstreamDownstream",
            Self::Sequential => "Sequential",
        }
    }
}

/// Validation errors for metadata
#[derive(Debug, thiserror::Error)]
pub enum MetadataValidationError {
    #[error("Invalid lifecycle state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Invalid lifecycle state for tier: {state} is not valid for tier {tier}")]
    InvalidStateForTier { state: String, tier: String },

    #[error("Invalid version format: {0}")]
    InvalidVersion(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid lifecycle state: {0}")]
    InvalidLifecycleState(String),
}

/// Convert database Adapter to canonical AdapterMeta
impl From<crate::adapters::Adapter> for AdapterMeta {
    fn from(adapter: crate::adapters::Adapter) -> Self {
        AdapterMeta {
            id: adapter.id,
            tenant_id: adapter.tenant_id,
            name: adapter.name,
            adapter_id: adapter.adapter_id,
            version: adapter.version,
            lifecycle_state: LifecycleState::from_str(&adapter.lifecycle_state)
                .unwrap_or(LifecycleState::Active),
            category: adapter.category,
            scope: adapter.scope,
            tier: adapter.tier,
            hash_b3: adapter.hash_b3,
            rank: adapter.rank,
            alpha: adapter.alpha,
            targets_json: adapter.targets_json,
            acl_json: adapter.acl_json,
            adapter_name: adapter.adapter_name,
            tenant_namespace: adapter.tenant_namespace,
            domain: adapter.domain,
            purpose: adapter.purpose,
            revision: adapter.revision,
            parent_id: adapter.parent_id,
            fork_type: adapter.fork_type.as_deref().and_then(ForkType::from_str),
            fork_reason: adapter.fork_reason,
            framework: adapter.framework,
            framework_id: adapter.framework_id,
            framework_version: adapter.framework_version,
            languages_json: adapter.languages_json,
            repo_id: adapter.repo_id,
            commit_sha: adapter.commit_sha,
            intent: adapter.intent,
            current_state: adapter.current_state,
            load_state: adapter.load_state,
            pinned: adapter.pinned != 0,
            memory_bytes: adapter.memory_bytes,
            last_activated: adapter.last_activated,
            activation_count: adapter.activation_count,
            expires_at: adapter.expires_at,
            created_at: adapter.created_at,
            updated_at: adapter.updated_at,
            last_loaded_at: adapter.last_loaded_at,
        }
    }
}

/// Convert database StackRecord to canonical AdapterStackMeta
impl From<crate::traits::StackRecord> for AdapterStackMeta {
    fn from(stack: crate::traits::StackRecord) -> Self {
        let adapter_ids: Vec<String> = serde_json::from_str(&stack.adapter_ids_json)
            .unwrap_or_else(|_| vec![]);

        AdapterStackMeta {
            id: stack.id,
            tenant_id: stack.tenant_id,
            name: stack.name,
            description: stack.description,
            version: stack.version.to_string(),
            lifecycle_state: LifecycleState::from_str(&stack.lifecycle_state)
                .unwrap_or(LifecycleState::Active),
            adapter_ids,
            workflow_type: stack.workflow_type.as_deref().and_then(WorkflowType::from_str),
            created_at: stack.created_at,
            updated_at: stack.updated_at,
            created_by: stack.created_by,
        }
    }
}

/// Validate state transition
pub fn validate_state_transition(
    current: LifecycleState,
    new: LifecycleState,
    tier: &str,
) -> Result<(), MetadataValidationError> {
    if !current.can_transition_to(new) {
        return Err(MetadataValidationError::InvalidStateTransition {
            from: current.to_string(),
            to: new.to_string(),
        });
    }

    if !new.is_valid_for_tier(tier) {
        return Err(MetadataValidationError::InvalidStateForTier {
            state: new.to_string(),
            tier: tier.to_string(),
        });
    }

    Ok(())
}

/// Validate semantic version format (basic check)
pub fn validate_version(version: &str) -> Result<(), MetadataValidationError> {
    // Simple validation: either semver (X.Y.Z) or monotonic (digits)
    let is_semver = version
        .split('.')
        .all(|part| part.parse::<u32>().is_ok());

    let is_monotonic = version.parse::<u64>().is_ok();

    if !is_semver && !is_monotonic {
        return Err(MetadataValidationError::InvalidVersion(
            format!("Version must be semver (X.Y.Z) or monotonic integer, got: {}", version)
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_state_transitions() {
        // Valid transitions
        assert!(LifecycleState::Draft.can_transition_to(LifecycleState::Active));
        assert!(LifecycleState::Active.can_transition_to(LifecycleState::Deprecated));
        assert!(LifecycleState::Deprecated.can_transition_to(LifecycleState::Retired));

        // Invalid transitions (backward)
        assert!(!LifecycleState::Active.can_transition_to(LifecycleState::Draft));
        assert!(!LifecycleState::Deprecated.can_transition_to(LifecycleState::Active));

        // Can't transition out of retired
        assert!(!LifecycleState::Retired.can_transition_to(LifecycleState::Active));
        assert!(!LifecycleState::Retired.can_transition_to(LifecycleState::Deprecated));
    }

    #[test]
    fn test_lifecycle_state_tier_validation() {
        // ephemeral adapters can't be deprecated
        assert!(!LifecycleState::Deprecated.is_valid_for_tier("ephemeral"));
        assert!(LifecycleState::Active.is_valid_for_tier("ephemeral"));
        assert!(LifecycleState::Retired.is_valid_for_tier("ephemeral"));

        // persistent adapters can be in any state
        assert!(LifecycleState::Deprecated.is_valid_for_tier("persistent"));
    }

    #[test]
    fn test_version_validation() {
        // Valid semver
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("2.1.3").is_ok());

        // Valid monotonic
        assert!(validate_version("42").is_ok());
        assert!(validate_version("123").is_ok());

        // Invalid
        assert!(validate_version("invalid").is_err());
        assert!(validate_version("1.2.x").is_err());
    }

    #[test]
    fn test_state_transition_validation() {
        // Valid transition
        let result = validate_state_transition(
            LifecycleState::Draft,
            LifecycleState::Active,
            "persistent"
        );
        assert!(result.is_ok());

        // Invalid transition (backward)
        let result = validate_state_transition(
            LifecycleState::Active,
            LifecycleState::Draft,
            "persistent"
        );
        assert!(result.is_err());

        // Invalid state for tier
        let result = validate_state_transition(
            LifecycleState::Active,
            LifecycleState::Deprecated,
            "ephemeral"
        );
        assert!(result.is_err());
    }
}
