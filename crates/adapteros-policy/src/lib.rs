//! Policy enforcement for AdapterOS

pub mod policy_pack;
pub mod policy_packs;
pub mod registry;
pub mod unified_enforcement;
pub mod validation;

use adapteros_core::{AosError, Result};
use adapteros_manifest::*;

pub mod abstention;
pub mod access_control;
pub mod code_metrics;
pub mod egress;
pub mod mplora;
pub mod numeric;
pub mod patch_policy;
pub mod refusal;
pub mod security_monitoring;
pub mod security_response;
pub mod threat_detection;

// Policy packs implemented in Phase 3
pub mod packs;

// Policy hash watcher and quarantine (Determinism Ruleset #2)
pub mod hash_watcher;
pub mod quarantine;

// Re-export registry types
pub use registry::{
    explain_policy, get_policy, list_policies, Audit, Policy, PolicyContext, PolicyId, PolicySpec,
    Severity, Violation, POLICY_INDEX,
};

pub use abstention::should_abstain;
pub use access_control::{AccessControlManager, AccessDecision, AccessPolicy, RoleDefinition};
pub use code_metrics::{
    AnswerRelevanceRate, CodeMetrics, CompileSuccessRate, MetricsSummary, TestPass1,
};
pub use hash_watcher::{HashViolation, PolicyHashWatcher, ValidationResult};
pub use mplora::{MploraConfig, MploraPolicy};
pub use numeric::validate_numeric_units;
pub use patch_policy::{
    CodePolicy, ComprehensiveValidation, FilePatch, LintValidation, PatchPolicyEngine,
    SecurityValidation, SecurityViolation, TestValidation,
};
pub use policy_pack::{PolicyPackRegistry, SignedPolicyPack};
pub use policy_packs::{
    AdapterLifecycleValidator, ArtifactsValidator, BuildReleaseValidator, ComplianceValidator,
    DeterminismValidator, EgressValidator, EnforcementLevel, EvidenceValidator, FullPackValidator,
    IncidentValidator, IsolationValidator, LlmOutputValidator, MemoryValidator,
    NumericUnitsValidator, PerformanceValidator, PolicyPackConfig, PolicyPackId, PolicyPackManager,
    PolicyPackValidator, RagIndexValidator, RefusalValidator, RetentionValidator, RouterValidator,
    SecretsValidator, TelemetryValidator,
};
pub use packs::{
    AdapterNameValidation, NamingConfig, NamingPolicy, NamingViolation, NamingViolationType,
    StackNameValidation,
};
pub use quarantine::{QuarantineManager, QuarantineOperation};
pub use refusal::{RefusalReason, RefusalResponse};
pub use security_monitoring::{SecurityMonitoringService, SecurityReport};
pub use security_response::{ResponseAction, ResponsePlan, ResponsePolicy, SecurityResponseEngine};
pub use threat_detection::{ThreatAssessment, ThreatDetectionEngine, ThreatSeverity, ThreatSignal};
pub use unified_enforcement::{
    EnforcementAction, Operation, OperationType, PolicyComplianceReport, PolicyEnforcementResult,
    PolicyEnforcer, PolicyRequest, PolicyValidationResult, PolicyViolation, RequestType,
    UnifiedPolicyEnforcer, ViolationSeverity,
};

/// Policy engine for enforcing all 20 policy packs
pub struct PolicyEngine {
    policies: Policies,
}

impl PolicyEngine {
    /// Create a new policy engine from manifest
    pub fn new(policies: Policies) -> Self {
        Self { policies }
    }

    /// Check if evidence is sufficient
    pub fn check_evidence(&self, span_count: usize) -> Result<()> {
        if self.policies.evidence.require_open_book && span_count < self.policies.evidence.min_spans
        {
            return Err(AosError::PolicyViolation(format!(
                "Insufficient evidence: {} spans, need {}",
                span_count, self.policies.evidence.min_spans
            )));
        }
        Ok(())
    }

    /// Check if confidence meets threshold
    pub fn check_confidence(&self, confidence: f32) -> Result<()> {
        if confidence < self.policies.refusal.abstain_threshold {
            return Err(AosError::PolicyViolation(format!(
                "Confidence {} below threshold {}",
                confidence, self.policies.refusal.abstain_threshold
            )));
        }
        Ok(())
    }

    /// Check if request should be allowed based on resource limits
    pub fn check_resource_limits(&self, max_tokens: usize) -> Result<()> {
        // Check against policy limits
        if max_tokens > self.policies.performance.max_tokens {
            return Err(AosError::PolicyViolation(format!(
                "Request exceeds max tokens limit: {} > {}",
                max_tokens, self.policies.performance.max_tokens
            )));
        }

        Ok(())
    }

    /// Check if circuit breaker should be opened
    pub fn check_circuit_breaker(&self, failure_count: usize, threshold: usize) -> Result<()> {
        if failure_count >= threshold {
            return Err(AosError::PolicyViolation(format!(
                "Circuit breaker opened: {} failures >= threshold {}",
                failure_count, threshold
            )));
        }
        Ok(())
    }

    /// Check system resource thresholds
    pub fn check_system_thresholds(&self, cpu_usage: f32, memory_usage: f32) -> Result<()> {
        if cpu_usage > self.policies.performance.cpu_threshold_pct {
            return Err(AosError::PerformanceViolation(format!(
                "CPU usage {:.1}% exceeds threshold {:.1}%",
                cpu_usage, self.policies.performance.cpu_threshold_pct
            )));
        }

        if memory_usage > self.policies.performance.memory_threshold_pct {
            return Err(AosError::MemoryPressure(format!(
                "Memory usage {:.1}% exceeds threshold {:.1}%",
                memory_usage, self.policies.performance.memory_threshold_pct
            )));
        }

        Ok(())
    }

    /// Check memory headroom policy (Memory Ruleset #12)
    pub fn check_memory_headroom(&self, headroom_pct: f32) -> Result<()> {
        let min_headroom = self.policies.memory.min_headroom_pct as f32;
        if headroom_pct < min_headroom {
            return Err(AosError::MemoryPressure(format!(
                "Insufficient memory headroom: {:.1}% < {:.1}% (Memory Ruleset #12)",
                headroom_pct, min_headroom
            )));
        }
        Ok(())
    }
    pub fn should_open_circuit_breaker(&self, failure_count: usize) -> bool {
        failure_count >= self.policies.performance.circuit_breaker_threshold
    }

    /// Check if memory pressure exceeds limits
    pub fn check_memory_pressure(
        &self,
        memory_usage_bytes: u64,
        max_memory_bytes: u64,
    ) -> Result<()> {
        if memory_usage_bytes > max_memory_bytes {
            return Err(AosError::MemoryPressure(format!(
                "Memory usage {} bytes exceeds limit {} bytes",
                memory_usage_bytes, max_memory_bytes
            )));
        }
        Ok(())
    }

    /// Check if CPU time exceeds limits
    pub fn check_cpu_time(&self, cpu_time_secs: u64, max_cpu_time_secs: u64) -> Result<()> {
        if cpu_time_secs > max_cpu_time_secs {
            return Err(AosError::PolicyViolation(format!(
                "CPU time {} seconds exceeds limit {} seconds",
                cpu_time_secs, max_cpu_time_secs
            )));
        }
        Ok(())
    }

    /// Validate numeric value with units
    pub fn validate_numeric(&self, value: f32, unit: &str, domain: &str) -> Result<String> {
        if let Some(canonical) = self.policies.numeric.canonical_units.get(domain) {
            // In production, this would do actual unit conversion
            // For now, just validate that units are present
            if self.policies.numeric.require_units_in_trace && unit.is_empty() {
                return Err(AosError::PolicyViolation(
                    "Units required in trace".to_string(),
                ));
            }
            Ok(format!("{} {}", value, canonical))
        } else {
            Ok(format!("{} {}", value, unit))
        }
    }

    /// Get egress policy
    pub fn egress_policy(&self) -> &EgressPolicy {
        &self.policies.egress
    }

    /// Get determinism policy
    pub fn determinism_policy(&self) -> &DeterminismPolicy {
        &self.policies.determinism
    }

    /// Get memory policy
    pub fn memory_policy(&self) -> &MemoryPolicy {
        &self.policies.memory
    }

    /// Get performance policy
    pub fn performance_policy(&self) -> &PerformancePolicy {
        &self.policies.performance
    }
}
