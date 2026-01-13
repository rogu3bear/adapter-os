//! Policy Hook System
//!
//! This module defines the policy hook system that enables fine-grained
//! policy enforcement at different stages of request processing.
//!
//! # Architecture
//!
//! The hook system operates at three key points:
//! 1. `OnRequestBeforeRouting` - Before adapter selection
//! 2. `OnBeforeInference` - After routing, before inference
//! 3. `OnAfterInference` - After inference completes
//!
//! # Citations
//! - Multi-hook policy architecture for request lifecycle enforcement
//! - AGENTS.md L96-142: Policy Engine implementation standards

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::policy_packs::PolicyPackId;

/// Policy hook points in the request lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyHook {
    /// Before routing: tenant isolation, rate limiting, request validation
    OnRequestBeforeRouting,
    /// Before inference: adapter validation, resource checks, determinism setup
    OnBeforeInference,
    /// After inference: output validation, evidence requirements, telemetry
    OnAfterInference,
}

impl PolicyHook {
    /// Get all hook variants
    pub fn all() -> Vec<PolicyHook> {
        vec![
            PolicyHook::OnRequestBeforeRouting,
            PolicyHook::OnBeforeInference,
            PolicyHook::OnAfterInference,
        ]
    }

    /// Get hook name for logging
    pub fn name(&self) -> &'static str {
        match self {
            PolicyHook::OnRequestBeforeRouting => "on_request_before_routing",
            PolicyHook::OnBeforeInference => "on_before_inference",
            PolicyHook::OnAfterInference => "on_after_inference",
        }
    }

    /// Get hook description
    pub fn description(&self) -> &'static str {
        match self {
            PolicyHook::OnRequestBeforeRouting => {
                "Enforced before adapter routing: tenant isolation, rate limiting"
            }
            PolicyHook::OnBeforeInference => {
                "Enforced after routing, before inference: adapter validation, resource checks"
            }
            PolicyHook::OnAfterInference => {
                "Enforced after inference: output validation, evidence requirements"
            }
        }
    }
}

/// Context provided to policy hook evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    /// Tenant making the request
    pub tenant_id: String,
    /// User making the request (optional for service accounts)
    pub user_id: Option<String>,
    /// Unique request identifier
    pub request_id: String,
    /// Hook being invoked
    pub hook: PolicyHook,
    /// Resource type being accessed (e.g., "adapter", "stack", "inference")
    pub resource_type: String,
    /// Specific resource identifier (e.g., adapter ID)
    pub resource_id: Option<String>,
    /// Additional context-specific metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl HookContext {
    /// Create a new hook context
    pub fn new(
        tenant_id: impl Into<String>,
        request_id: impl Into<String>,
        hook: PolicyHook,
        resource_type: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            user_id: None,
            request_id: request_id.into(),
            hook,
            resource_type: resource_type.into(),
            resource_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Set user ID
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set resource ID
    pub fn with_resource_id(mut self, resource_id: impl Into<String>) -> Self {
        self.resource_id = Some(resource_id.into());
        self
    }

    /// Add metadata entry
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Add input prompt to metadata
    pub fn with_input(mut self, input: impl Into<String>) -> Self {
        self.metadata
            .insert("input".to_string(), serde_json::Value::String(input.into()));
        self
    }

    /// Add multiple metadata entries
    pub fn with_metadata_map(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata.extend(metadata);
        self
    }
}

/// Policy decision result
///
/// Note: `policy_pack_id` uses `String` instead of `PolicyPackId` enum to allow
/// interoperability with the database layer (tenant_policy_bindings) which stores
/// policy IDs as strings. This supports both canonical policy packs and future
/// custom policies that may not be in the enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    /// Policy pack that made this decision (string form for DB compatibility)
    pub policy_pack_id: String,
    /// Hook that was evaluated
    pub hook: PolicyHook,
    /// Decision outcome
    pub decision: Decision,
    /// Human-readable reason for the decision
    pub reason: String,
}

impl PolicyDecision {
    /// Create a new policy decision
    pub fn new(
        policy_pack_id: impl Into<String>,
        hook: PolicyHook,
        decision: Decision,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            policy_pack_id: policy_pack_id.into(),
            hook,
            decision,
            reason: reason.into(),
        }
    }

    /// Create an allow decision
    pub fn allow(
        policy_pack_id: impl Into<String>,
        hook: PolicyHook,
        reason: impl Into<String>,
    ) -> Self {
        Self::new(policy_pack_id, hook, Decision::Allow, reason)
    }

    /// Create a deny decision
    pub fn deny(
        policy_pack_id: impl Into<String>,
        hook: PolicyHook,
        reason: impl Into<String>,
    ) -> Self {
        Self::new(policy_pack_id, hook, Decision::Deny, reason)
    }

    /// Create a modify decision
    pub fn modify(
        policy_pack_id: impl Into<String>,
        hook: PolicyHook,
        reason: impl Into<String>,
        modifications: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self::new(
            policy_pack_id,
            hook,
            Decision::Modify { modifications },
            reason,
        )
    }

    /// Check if this decision denies the request
    pub fn is_deny(&self) -> bool {
        matches!(self.decision, Decision::Deny)
    }

    /// Check if this decision allows the request
    pub fn is_allow(&self) -> bool {
        matches!(self.decision, Decision::Allow)
    }

    /// Check if this decision modifies the request
    pub fn is_modify(&self) -> bool {
        matches!(self.decision, Decision::Modify { .. })
    }
}

/// Policy decision outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Allow the operation to proceed
    Allow,
    /// Deny the operation
    Deny,
    /// Allow with modifications to the request/response
    Modify {
        /// Key-value pairs of modifications to apply
        modifications: HashMap<String, serde_json::Value>,
    },
}

impl Decision {
    /// Create a modify decision with modifications
    pub fn modify(modifications: HashMap<String, serde_json::Value>) -> Self {
        Decision::Modify { modifications }
    }

    /// Get modifications if this is a modify decision
    pub fn modifications(&self) -> Option<&HashMap<String, serde_json::Value>> {
        match self {
            Decision::Modify { modifications } => Some(modifications),
            _ => None,
        }
    }
}

/// Core policies that are always enabled by default
///
/// These four policies enforce fundamental security and correctness guarantees:
/// - Egress: Zero data exfiltration during serving
/// - Determinism: Identical inputs produce identical outputs
/// - Isolation: Process, file, and key isolation per tenant
/// - Evidence: Answers cite sources or abstain
pub const CORE_POLICIES: [PolicyPackId; 4] = [
    PolicyPackId::Egress,
    PolicyPackId::Determinism,
    PolicyPackId::Isolation,
    PolicyPackId::Evidence,
];

/// Check if a policy pack is a core policy (always enabled)
pub fn is_core_policy(policy_pack_id: &PolicyPackId) -> bool {
    CORE_POLICIES.contains(policy_pack_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_hook_all() {
        let hooks = PolicyHook::all();
        assert_eq!(hooks.len(), 3);
        assert!(hooks.contains(&PolicyHook::OnRequestBeforeRouting));
        assert!(hooks.contains(&PolicyHook::OnBeforeInference));
        assert!(hooks.contains(&PolicyHook::OnAfterInference));
    }

    #[test]
    fn test_hook_context_builder() {
        let ctx = HookContext::new(
            "tenant-1",
            "req-123",
            PolicyHook::OnBeforeInference,
            "adapter",
        )
        .with_user_id("user-42")
        .with_resource_id("adapter-xyz")
        .with_metadata("key", "value");

        assert_eq!(ctx.tenant_id, "tenant-1");
        assert_eq!(ctx.user_id, Some("user-42".to_string()));
        assert_eq!(ctx.request_id, "req-123");
        assert_eq!(ctx.hook, PolicyHook::OnBeforeInference);
        assert_eq!(ctx.resource_type, "adapter");
        assert_eq!(ctx.resource_id, Some("adapter-xyz".to_string()));
        assert!(ctx.metadata.contains_key("key"));
    }

    #[test]
    fn test_policy_decision_constructors() {
        let allow = PolicyDecision::allow(
            PolicyPackId::Egress,
            PolicyHook::OnRequestBeforeRouting,
            "No egress detected",
        );
        assert!(allow.is_allow());
        assert!(!allow.is_deny());
        assert!(!allow.is_modify());

        let deny = PolicyDecision::deny(
            PolicyPackId::Isolation,
            PolicyHook::OnBeforeInference,
            "Tenant boundary violation",
        );
        assert!(deny.is_deny());
        assert!(!deny.is_allow());
        assert!(!deny.is_modify());

        let mut mods = HashMap::new();
        mods.insert("max_tokens".to_string(), serde_json::json!(100));
        let modify = PolicyDecision::modify(
            PolicyPackId::Performance,
            PolicyHook::OnBeforeInference,
            "Reduced token limit",
            mods,
        );
        assert!(modify.is_modify());
        assert!(!modify.is_allow());
        assert!(!modify.is_deny());
    }

    #[test]
    fn test_decision_modifications() {
        let mut mods = HashMap::new();
        mods.insert("temperature".to_string(), serde_json::json!(0.7));
        let decision = Decision::modify(mods.clone());

        assert!(decision.modifications().is_some());
        assert_eq!(decision.modifications().unwrap().len(), 1);

        let allow = Decision::Allow;
        assert!(allow.modifications().is_none());
    }

    #[test]
    fn test_core_policies() {
        assert_eq!(CORE_POLICIES.len(), 4);
        assert!(is_core_policy(&PolicyPackId::Egress));
        assert!(is_core_policy(&PolicyPackId::Determinism));
        assert!(is_core_policy(&PolicyPackId::Isolation));
        assert!(is_core_policy(&PolicyPackId::Evidence));
        assert!(!is_core_policy(&PolicyPackId::Performance));
        assert!(!is_core_policy(&PolicyPackId::Router));
    }

    #[test]
    fn test_hook_serialization() {
        let hook = PolicyHook::OnBeforeInference;
        let json = serde_json::to_string(&hook).unwrap();
        assert_eq!(json, r#""on_before_inference""#);

        let deserialized: PolicyHook = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, hook);
    }

    #[test]
    fn test_decision_serialization() {
        let decision = Decision::Allow;
        let json = serde_json::to_string(&decision).unwrap();
        assert_eq!(json, r#""allow""#);

        let mut mods = HashMap::new();
        mods.insert("key".to_string(), serde_json::json!("value"));
        let modify = Decision::modify(mods);
        let json = serde_json::to_string(&modify).unwrap();
        assert!(json.contains("modify"));
        assert!(json.contains("modifications"));
    }
}
