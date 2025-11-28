//! Policy Registry - Canonical 20 Policy Packs
//!
//! This module defines the complete set of 20 policy packs enforced by AdapterOS.
//! Each policy pack has a unique ID, name, and enforcement logic.

use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a policy pack
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PolicyId {
    Egress = 1,
    Determinism = 2,
    Router = 3,
    Evidence = 4,
    Refusal = 5,
    Numeric = 6,
    Rag = 7,
    Isolation = 8,
    Telemetry = 9,
    Retention = 10,
    Performance = 11,
    Memory = 12,
    Artifacts = 13,
    Secrets = 14,
    BuildRelease = 15,
    Compliance = 16,
    Incident = 17,
    Output = 18,
    Adapters = 19,
    DeterministicIo = 20,
    Drift = 21,
    Mplora = 22,
    Naming = 23,
    DependencySecurity = 24,
}

impl PolicyId {
    /// Get all policy IDs in order
    pub fn all() -> &'static [PolicyId; 24] {
        &[
            PolicyId::Egress,
            PolicyId::Determinism,
            PolicyId::Router,
            PolicyId::Evidence,
            PolicyId::Refusal,
            PolicyId::Numeric,
            PolicyId::Rag,
            PolicyId::Isolation,
            PolicyId::Telemetry,
            PolicyId::Retention,
            PolicyId::Performance,
            PolicyId::Memory,
            PolicyId::Artifacts,
            PolicyId::Secrets,
            PolicyId::BuildRelease,
            PolicyId::Compliance,
            PolicyId::Incident,
            PolicyId::Output,
            PolicyId::Adapters,
            PolicyId::DeterministicIo,
            PolicyId::Drift,
            PolicyId::Mplora,
            PolicyId::Naming,
            PolicyId::DependencySecurity,
        ]
    }

    /// Get policy name
    pub fn name(&self) -> &'static str {
        match self {
            PolicyId::Egress => "Egress",
            PolicyId::Determinism => "Determinism",
            PolicyId::Router => "Router",
            PolicyId::Evidence => "Evidence",
            PolicyId::Refusal => "Refusal",
            PolicyId::Numeric => "Numeric",
            PolicyId::Rag => "RAG",
            PolicyId::Isolation => "Isolation",
            PolicyId::Telemetry => "Telemetry",
            PolicyId::Retention => "Retention",
            PolicyId::Performance => "Performance",
            PolicyId::Memory => "Memory",
            PolicyId::Artifacts => "Artifacts",
            PolicyId::Secrets => "Secrets",
            PolicyId::BuildRelease => "Build/Release",
            PolicyId::Compliance => "Compliance",
            PolicyId::Incident => "Incident",
            PolicyId::Output => "Output",
            PolicyId::Adapters => "Adapters",
            PolicyId::DeterministicIo => "Deterministic I/O",
            PolicyId::Drift => "Drift",
            PolicyId::Mplora => "MPLoRA",
            PolicyId::Naming => "Naming",
            PolicyId::DependencySecurity => "Dependency Security",
        }
    }

    /// Get policy description
    pub fn description(&self) -> &'static str {
        match self {
            PolicyId::Egress => "Control outbound network and protocols; PF firewall enforcement",
            PolicyId::Determinism => "Enforce executor, hashes, replay, epsilon bounds",
            PolicyId::Router => "Deterministic tie-break and route selection with Q15 gates",
            PolicyId::Evidence => "Trace, signatures, and audit artifacts with open-book enforcement",
            PolicyId::Refusal => "Deny unsafe operations and redact outputs on low confidence",
            PolicyId::Numeric => "Precision modes, epsilon budgets, and strict math operations",
            PolicyId::Rag => "Retrieval provenance and cache rules with tenant isolation",
            PolicyId::Isolation => "Process, memory, and adapter sandbox boundaries",
            PolicyId::Telemetry => "Deterministic logging and metrics with canonical JSON",
            PolicyId::Retention => "Data lifetime, TTL, and purge proof with CPID tracking",
            PolicyId::Performance => "Throughput budgets without nondeterministic paths",
            PolicyId::Memory => "UMA behavior, pinning, page-out guards, 15% headroom",
            PolicyId::Artifacts => "Models, adapters, and build outputs as signed objects with SBOM",
            PolicyId::Secrets => "Vault use, zero egress, zero logs with Secure Enclave",
            PolicyId::BuildRelease => "Toolchain pins, kernel hashes, SBOM, and hallucination thresholds",
            PolicyId::Compliance => "CMMC/ITAR policy hooks and reports with evidence linking",
            PolicyId::Incident => "Freeze, capture, and post-mortem bundles with runbook procedures",
            PolicyId::Output => "Canonical formats, normalization, and PII filters",
            PolicyId::Adapters => "Load order, composition, capability ACLs, and activation thresholds",
            PolicyId::DeterministicIo => "File reads/writes via hashed wrappers; no wall-clock; stubbed network",
            PolicyId::Drift => "Environment fingerprint tracking and drift detection with cryptographic verification",
            PolicyId::Mplora => "Orthogonal multi-path LoRA constraints enforcement with shared downsample validation",
            PolicyId::Naming => "Adapter and stack naming conventions with reserved namespace and hierarchy enforcement",
            PolicyId::DependencySecurity => "CVE database integration, vulnerability scoring, caching, and supply chain validation",
        }
    }

    /// Get enforcement point
    pub fn enforcement_point(&self) -> &'static str {
        match self {
            PolicyId::Egress => "worker startup, runtime checks",
            PolicyId::Determinism => "kernel loading, execution start, replay verification",
            PolicyId::Router => "adapter selection, gate computation",
            PolicyId::Evidence => "pre-generation policy check",
            PolicyId::Refusal => "post-inference policy check",
            PolicyId::Numeric => "output validation",
            PolicyId::Rag => "evidence retrieval",
            PolicyId::Isolation => "tenant initialization",
            PolicyId::Telemetry => "event logging throughout system",
            PolicyId::Retention => "bundle GC",
            PolicyId::Performance => "promotion gate, runtime monitoring",
            PolicyId::Memory => "memory watermark monitoring",
            PolicyId::Artifacts => "artifact import, bundle loading",
            PolicyId::Secrets => "key derivation, artifact encryption",
            PolicyId::BuildRelease => "promotion gate, CI pipeline",
            PolicyId::Compliance => "promotion gate, audit reporting",
            PolicyId::Incident => "incident detection, automated response",
            PolicyId::Output => "response builder",
            PolicyId::Adapters => "adapter registration, eviction checks",
            PolicyId::DeterministicIo => "I/O layer, filesystem operations",
            PolicyId::Drift => "startup verification, runtime drift checks",
            PolicyId::Mplora => "adapteros-router, adapteros-kernel-mtl",
            PolicyId::Naming => "adapter registration, stack creation, API endpoints",
            PolicyId::DependencySecurity => {
                "adapter registration, build pipeline, dependency validation"
            }
        }
    }

    /// Check if policy is fully implemented
    pub fn is_implemented(&self) -> bool {
        match self {
            PolicyId::Egress => true,
            PolicyId::Determinism => true,
            PolicyId::Router => true,
            PolicyId::Evidence => true,
            PolicyId::Refusal => true,
            PolicyId::Numeric => true,
            PolicyId::Rag => true,
            PolicyId::Isolation => true,
            PolicyId::Telemetry => true,
            PolicyId::Retention => true,
            PolicyId::Performance => true,
            PolicyId::Memory => true,
            PolicyId::Artifacts => true,
            PolicyId::Secrets => true,
            PolicyId::BuildRelease => true,
            PolicyId::Compliance => true,
            PolicyId::Incident => true,
            PolicyId::Output => true,
            PolicyId::Adapters => true,
            PolicyId::DeterministicIo => true,
            PolicyId::Drift => true,
            PolicyId::Mplora => true,
            PolicyId::Naming => true,
            PolicyId::DependencySecurity => true,
        }
    }
}

impl fmt::Display for PolicyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Policy specification with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySpec {
    pub id: PolicyId,
    pub name: &'static str,
    pub description: &'static str,
    pub enforcement_point: &'static str,
    pub implemented: bool,
}

impl PolicySpec {
    /// Create a policy spec from an ID
    pub fn from_id(id: PolicyId) -> Self {
        Self {
            id,
            name: id.name(),
            description: id.description(),
            enforcement_point: id.enforcement_point(),
            implemented: id.is_implemented(),
        }
    }
}

/// The canonical registry of all 22 policy packs
pub static POLICY_INDEX: once_cell::sync::Lazy<[PolicySpec; 22]> =
    once_cell::sync::Lazy::new(|| {
        [
            PolicySpec::from_id(PolicyId::Egress),
            PolicySpec::from_id(PolicyId::Determinism),
            PolicySpec::from_id(PolicyId::Router),
            PolicySpec::from_id(PolicyId::Evidence),
            PolicySpec::from_id(PolicyId::Refusal),
            PolicySpec::from_id(PolicyId::Numeric),
            PolicySpec::from_id(PolicyId::Rag),
            PolicySpec::from_id(PolicyId::Isolation),
            PolicySpec::from_id(PolicyId::Telemetry),
            PolicySpec::from_id(PolicyId::Retention),
            PolicySpec::from_id(PolicyId::Performance),
            PolicySpec::from_id(PolicyId::Memory),
            PolicySpec::from_id(PolicyId::Artifacts),
            PolicySpec::from_id(PolicyId::Secrets),
            PolicySpec::from_id(PolicyId::BuildRelease),
            PolicySpec::from_id(PolicyId::Compliance),
            PolicySpec::from_id(PolicyId::Incident),
            PolicySpec::from_id(PolicyId::Output),
            PolicySpec::from_id(PolicyId::Adapters),
            PolicySpec::from_id(PolicyId::DeterministicIo),
            PolicySpec::from_id(PolicyId::Drift),
            PolicySpec::from_id(PolicyId::Mplora),
        ]
    });

/// Policy enforcement trait
pub trait Policy {
    /// Get policy ID
    fn id(&self) -> PolicyId;

    /// Get policy name
    fn name(&self) -> &'static str;

    /// Get policy severity
    fn severity(&self) -> Severity;

    /// Enforce the policy against a context
    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit>;
}

/// Context for policy enforcement
pub trait PolicyContext {
    /// Get context type name
    fn context_type(&self) -> &str {
        "unknown"
    }

    /// Downcast to Any for dynamic type checking
    fn as_any(&self) -> &dyn std::any::Any;

    /// Get metadata for policy enforcement
    fn metadata(&self) -> &std::collections::HashMap<String, String> {
        static EMPTY: once_cell::sync::Lazy<std::collections::HashMap<String, String>> =
            once_cell::sync::Lazy::new(|| std::collections::HashMap::new());
        &EMPTY
    }
}

/// Audit result from policy enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Audit {
    pub policy_id: PolicyId,
    pub passed: bool,
    pub violations: Vec<Violation>,
    pub warnings: Vec<String>,
}

impl Audit {
    pub fn passed(policy_id: PolicyId) -> Self {
        Self {
            policy_id,
            passed: true,
            violations: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn failed(policy_id: PolicyId, violations: Vec<Violation>) -> Self {
        Self {
            policy_id,
            passed: false,
            violations,
            warnings: Vec::new(),
        }
    }

    pub fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = warnings;
        self
    }
}

/// Policy violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub severity: Severity,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

/// List all policies
pub fn list_policies() -> &'static [PolicySpec; 22] {
    &POLICY_INDEX
}

/// Get policy by ID
pub fn get_policy(id: PolicyId) -> &'static PolicySpec {
    &POLICY_INDEX[id as usize - 1]
}

/// Explain a policy
pub fn explain_policy(id: PolicyId) -> String {
    let spec = get_policy(id);
    format!(
        "Policy #{}: {}\n\n\
         Description: {}\n\n\
         Enforcement Point: {}\n\n\
         Status: {}\n",
        id as usize,
        spec.name,
        spec.description,
        spec.enforcement_point,
        if spec.implemented {
            "Implemented"
        } else {
            "Not Yet Implemented"
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_count() {
        assert_eq!(POLICY_INDEX.len(), 22, "Must have exactly 22 policy packs");
    }

    #[test]
    fn test_policy_ids_unique() {
        let ids: Vec<_> = POLICY_INDEX.iter().map(|p| p.id as usize).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "Policy IDs must be unique");
    }

    #[test]
    fn test_policy_ids_sequential() {
        for (idx, spec) in POLICY_INDEX.iter().enumerate() {
            assert_eq!(
                spec.id as usize,
                idx + 1,
                "Policy IDs must be sequential starting from 1"
            );
        }
    }

    #[test]
    fn test_all_policies_have_descriptions() {
        for spec in POLICY_INDEX.iter() {
            assert!(
                !spec.description.is_empty(),
                "Policy {} must have a description",
                spec.name
            );
            assert!(
                !spec.enforcement_point.is_empty(),
                "Policy {} must have an enforcement point",
                spec.name
            );
        }
    }
}
