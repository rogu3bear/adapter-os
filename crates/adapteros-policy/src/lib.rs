//! Policy enforcement for AdapterOS

pub mod policy_pack;
pub mod registry;
pub mod validation;

use adapteros_core::{AosError, Result};
use adapteros_manifest::*;

pub mod abstention;
pub mod code_metrics;
pub mod egress;
pub mod mplora;
pub mod numeric;
pub mod patch_policy;
pub mod refusal;

// Policy packs implemented in Phase 3
pub mod packs;

// Re-export registry types
pub use registry::{
    explain_policy, get_policy, list_policies, Audit, Policy, PolicyContext, PolicyId, PolicySpec,
    Severity, Violation, POLICY_INDEX,
};

pub use abstention::should_abstain;
pub use code_metrics::{
    AnswerRelevanceRate, CodeMetrics, CompileSuccessRate, MetricsSummary, TestPass1,
};
pub use mplora::{MploraConfig, MploraPolicy};
pub use numeric::validate_numeric_units;
pub use patch_policy::{
    CodePolicy, ComprehensiveValidation, FilePatch, LintValidation, PatchPolicyEngine,
    SecurityValidation, SecurityViolation, TestValidation,
};
pub use policy_pack::{PolicyPackRegistry, SignedPolicyPack};
pub use refusal::{RefusalReason, RefusalResponse};

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
        if max_tokens > 1000 {
            // Would be configurable in real implementation
            return Err(AosError::PolicyViolation(format!(
                "Request exceeds max tokens limit: {} > 1000",
                max_tokens
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
        if cpu_usage > 90.0 {
            return Err(AosError::PerformanceViolation(format!(
                "CPU usage {}% exceeds threshold 90%",
                cpu_usage
            )));
        }

        if memory_usage > 95.0 {
            return Err(AosError::MemoryPressure(format!(
                "Memory usage {}% exceeds threshold 95%",
                memory_usage
            )));
        }

        Ok(())
    }

    /// Check memory headroom policy (Memory Ruleset #12)
    pub fn check_memory_headroom(&self, headroom_pct: f32) -> Result<()> {
        if headroom_pct < 15.0 {
            return Err(AosError::MemoryPressure(format!(
                "Insufficient memory headroom: {:.1}% < 15% (Memory Ruleset #12)",
                headroom_pct
            )));
        }
        Ok(())
    }
    pub fn should_open_circuit_breaker(&self, failure_count: usize) -> bool {
        failure_count >= 5 // Would be configurable in real implementation
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
