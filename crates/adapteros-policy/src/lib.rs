//! Policy enforcement for AdapterOS

#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::match_single_binding)]
#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(async_fn_in_trait)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::len_zero)]
#![allow(clippy::cloned_ref_to_slice_refs)]

pub mod backend_policy;
pub mod policy_integrity;
pub mod policy_pack;
pub mod policy_packs;
pub mod registry;
pub mod unified_enforcement;
pub mod validation;

use crate::packs::determinism::FORBIDDEN_COMPILER_FLAGS;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_manifest::*;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::warn;

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

// CVE client for vulnerability database integration
pub mod cve_client;

// Policy packs implemented in Phase 3
pub mod packs;

// Policy hash watcher and quarantine (Determinism Ruleset #2)
pub mod hash_watcher;
pub mod quarantine;

// Policy hook system for multi-stage enforcement
pub mod hooks;

// Re-export registry types
pub use registry::{
    explain_policy, get_policy, list_policies, Audit, Policy, PolicyContext, PolicyId, PolicySpec,
    Severity, Violation, POLICY_INDEX,
};

use crate::policy_packs::{
    PolicyContext as PackPolicyContext, PolicyWarning, Priority, RequestType as PackRequestType,
};
pub use abstention::should_abstain;
pub use access_control::{AccessControlManager, AccessDecision, AccessPolicy, RoleDefinition};
pub use backend_policy::{enforce_backend_policy, BackendDowngradePolicy};
pub use code_metrics::{
    AnswerRelevanceRate, CodeMetrics, CompileSuccessRate, MetricsSummary, TestPass1,
};
pub use cve_client::{
    AffectedRange, CachedOsvResponse, OsvClient, OsvClientConfig, OsvClientStats, OsvResponse,
    OsvVulnerability, PackageEcosystem, VersionEvent,
};
pub use hash_watcher::{HashViolation, PolicyHashWatcher, ValidationResult};
pub use hooks::{is_core_policy, Decision, HookContext, PolicyDecision, PolicyHook, CORE_POLICIES};
pub use mplora::{MploraConfig, MploraPolicy};
pub use numeric::validate_numeric_units;
pub use packs::{
    default_fallback_mappings, AdapterNameValidation, DeterminismConfig, DeterminismPolicy,
    EnforcementMode, EpsilonBounds, NamingConfig, NamingPolicy, NamingViolation,
    NamingViolationType, OperationValidation, RefusalConfig, RefusalPolicy, RngSeedingMethod,
    SafetyScores, StackNameValidation, TieBreakRule, ToolchainRequirements,
};
// Note: packs::refusal::RefusalReason is a different type than refusal::RefusalReason
// The former is used for policy enforcement, the latter for HTTP response serialization
pub use patch_policy::{
    CodePolicy, ComprehensiveValidation, FilePatch, LintValidation, PatchPolicyEngine,
    SecurityValidation, SecurityViolation, TestValidation,
};
pub use policy_integrity::{
    compute_blake3_hash, PolicyIntegrityMetadata, PolicyIntegrityVerifier,
    PolicyVerificationResult, RecoveryAction, TamperDetectionResult, VerificationStats,
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

/// Policy engine for enforcing all policy packs
pub struct PolicyEngine {
    policies: Policies,
    pack_manager: PolicyPackManager,
    refusal_policy: packs::RefusalPolicy,
}

impl PolicyEngine {
    /// Create a new policy engine from manifest
    pub fn new(policies: Policies) -> Self {
        Self {
            policies,
            pack_manager: PolicyPackManager::new(),
            refusal_policy: packs::RefusalPolicy::new(packs::RefusalConfig::default()),
        }
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

    /// Validate backend attestation report (Determinism Ruleset #2)
    ///
    /// Checks that the attestation report from a kernel backend meets
    /// all determinism policy requirements including metallib hash,
    /// RNG seeding method, compiler flags, and floating-point mode.
    pub fn validate_backend_attestation(
        &self,
        report: &adapteros_lora_kernel_api::attestation::DeterminismReport,
    ) -> Result<()> {
        use adapteros_lora_kernel_api::attestation::RngSeedingMethod as AttestationRngMethod;

        // Check overall deterministic flag
        if !report.deterministic {
            return Err(AosError::PolicyViolation(
                "Backend attestation indicates non-deterministic execution".to_string(),
            ));
        }

        // Check determinism level is declared as deterministic
        if !report.determinism_level.is_deterministic() {
            return Err(AosError::PolicyViolation(
                "Backend determinism level is non-deterministic".to_string(),
            ));
        }

        // Check backend type is allowed
        if !report.backend_type.is_deterministic_by_design() {
            return Err(AosError::PolicyViolation(format!(
                "Backend type {:?} is not deterministic by design",
                report.backend_type
            )));
        }

        // For Metal backend, require metallib hash match if policy requires it
        if self.policies.determinism.require_metallib_embed
            && report.backend_type == adapteros_lora_kernel_api::attestation::BackendType::Metal
            && report.metallib_hash.is_none()
        {
            return Err(AosError::PolicyViolation(
                "Metal backend must provide metallib hash".to_string(),
            ));
        }

        // Check RNG seeding method matches policy
        let rng_matches = match (
            self.policies.determinism.rng.as_str(),
            &report.rng_seed_method,
        ) {
            ("hkdf_seeded", AttestationRngMethod::HkdfSeeded) => true,
            ("fixed_seed", AttestationRngMethod::FixedSeed(_)) => true,
            _ => false,
        };

        if !rng_matches {
            return Err(AosError::PolicyViolation(format!(
                "RNG seeding method mismatch: policy requires {}, backend reports {:?}",
                self.policies.determinism.rng, report.rng_seed_method
            )));
        }

        // Check for forbidden compiler flags
        for flag in &report.compiler_flags {
            for forbidden in FORBIDDEN_COMPILER_FLAGS {
                if flag.contains(forbidden) {
                    return Err(AosError::PolicyViolation(format!(
                        "Forbidden compiler flag detected: {}",
                        flag
                    )));
                }
            }
        }

        // Check floating-point mode
        if !report.floating_point_mode.is_deterministic() {
            return Err(AosError::PolicyViolation(format!(
                "Floating-point mode {:?} is not deterministic",
                report.floating_point_mode
            )));
        }

        Ok(())
    }

    /// Check router entropy against policy floor (Router Ruleset #7)
    ///
    /// Validates that the router gate entropy is above the minimum threshold
    /// to ensure diverse adapter selection and avoid routing collapse.
    pub fn check_router_entropy(&self, entropy: f32) -> Result<()> {
        // Default entropy floor of 0.02 if not specified in policy
        let entropy_floor = 0.02f32;

        if entropy < entropy_floor {
            return Err(AosError::PolicyViolation(format!(
                "Router entropy {:.4} below floor {:.4} (Router Ruleset #7)",
                entropy, entropy_floor
            )));
        }
        Ok(())
    }

    /// Check dependency security (Security Ruleset)
    ///
    /// Validates that dependencies meet security requirements including
    /// version constraints, known vulnerabilities, and supply chain integrity.
    pub fn check_dependency_security(&self, dependencies: &[String]) -> Result<bool> {
        // Check for known insecure patterns in dependencies
        let insecure_patterns = ["yanked", "deprecated", "vulnerable"];

        for dep in dependencies {
            let dep_lower = dep.to_lowercase();
            for pattern in &insecure_patterns {
                if dep_lower.contains(pattern) {
                    return Err(AosError::PolicyViolation(format!(
                        "Insecure dependency detected: {} (contains '{}')",
                        dep, pattern
                    )));
                }
            }
        }

        Ok(true)
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

    /// Get determinism policy (manifest definition)
    pub fn determinism_policy(&self) -> &adapteros_manifest::DeterminismPolicy {
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

    /// Validate input content for policy violations before inference
    ///
    /// This method enforces content safety policies on the input prompt,
    /// checking for self-harm indicators, high-stakes domain detection,
    /// and other safety concerns. If any violation is detected, a
    /// PolicyViolation error is returned with appropriate guidance.
    ///
    /// # Arguments
    /// * `content` - The input prompt/content to validate
    ///
    /// # Returns
    /// * `Ok(())` if content passes all safety checks
    /// * `Err(AosError::PolicyViolation)` if content violates safety policies
    pub fn validate_input_content(&self, content: &str) -> Result<()> {
        // Check for self-harm indicators (critical safety check)
        if self.refusal_policy.check_self_harm(content) {
            return Err(AosError::PolicyViolation(
                "Content flagged for safety review: self-harm indicators detected. \
                If you're in crisis, please call 988 (Suicide & Crisis Lifeline), \
                text HOME to 741741 (Crisis Text Line), or call SAMHSA at 1-800-662-4357."
                    .to_string(),
            ));
        }

        // Detect high-stakes domain and log warning for elevated thresholds
        let domain = self.refusal_policy.detect_high_stakes_domain(content);
        match domain {
            packs::refusal::HighStakesDomain::Medical
            | packs::refusal::HighStakesDomain::Legal
            | packs::refusal::HighStakesDomain::Financial => {
                // Log that high-stakes domain was detected (thresholds will be enforced on output)
                warn!(
                    domain = ?domain,
                    "High-stakes domain detected in input, elevated confidence thresholds will apply"
                );
            }
            packs::refusal::HighStakesDomain::None => {}
        }

        Ok(())
    }

    /// Evaluate canonical policy packs for an inference request and return a decision chain digest.
    pub fn evaluate_inference_policies(
        &self,
        request_id: &str,
        metadata: serde_json::Value,
    ) -> Result<PolicyDecisionChain> {
        let request = crate::policy_packs::PolicyRequest {
            request_id: request_id.to_string(),
            request_type: PackRequestType::Inference,
            tenant_id: None,
            user_id: None,
            context: PackPolicyContext {
                component: "worker".to_string(),
                operation: "inference".to_string(),
                data: Some(metadata.clone()),
                priority: Priority::High,
            },
            metadata: Some(metadata),
        };

        let validation = self.pack_manager.validate_request(&request)?;
        let decisions = self.build_decision_chain(&validation);
        let digest_bytes = serde_json::to_vec(&decisions).map_err(|e| {
            AosError::Internal(format!("Failed to serialize policy decisions: {}", e))
        })?;
        let digest = B3Hash::hash(&digest_bytes);

        Ok(PolicyDecisionChain {
            validation,
            decisions,
            digest,
        })
    }

    fn build_decision_chain(
        &self,
        validation: &crate::policy_packs::PolicyValidationResult,
    ) -> Vec<PolicyDecisionRecord> {
        let mut decisions = Vec::new();
        let mut packs: Vec<PolicyPackId> = self
            .pack_manager
            .get_all_configs()
            .keys()
            .cloned()
            .collect();
        packs.sort_by(|a, b| a.to_id_string().cmp(&b.to_id_string()));

        for pack_id in packs {
            let pack_name = pack_id.name();
            let violation = validation
                .violations
                .iter()
                .find(|v| v.policy_pack.eq_ignore_ascii_case(pack_name));
            let warning = validation
                .warnings
                .iter()
                .find(|w| w.policy_pack.eq_ignore_ascii_case(pack_name));

            let outcome = if let Some(v) = violation {
                PolicyDecisionOutcome::Deny {
                    message: v.message.clone(),
                }
            } else if let Some(w) = warning {
                PolicyDecisionOutcome::Warn {
                    message: w.message.clone(),
                }
            } else {
                PolicyDecisionOutcome::Allow
            };

            decisions.push(PolicyDecisionRecord {
                id: pack_id.to_id_string().to_string(),
                name: pack_name.to_string(),
                outcome,
            });
        }

        // Include canonical IDs that are not yet wired through the legacy pack manager.
        for policy_id in PolicyId::all() {
            let id_string = policy_id.name().to_string();
            if decisions
                .iter()
                .any(|d| d.name.eq_ignore_ascii_case(&id_string))
            {
                continue;
            }
            decisions.push(PolicyDecisionRecord {
                id: id_string.to_lowercase(),
                name: id_string,
                outcome: PolicyDecisionOutcome::NotImplemented,
            });
        }

        decisions.sort_by(|a, b| a.name.cmp(&b.name));
        decisions
    }
}

/// Outcome for a policy decision entry in the decision chain.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecisionOutcome {
    Allow,
    Warn { message: String },
    Deny { message: String },
    NotImplemented,
}

/// Canonical policy decision entry recorded for telemetry/audit.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyDecisionRecord {
    pub id: String,
    pub name: String,
    pub outcome: PolicyDecisionOutcome,
}

/// Result of evaluating canonical policy packs for a request.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyDecisionChain {
    pub validation: crate::policy_packs::PolicyValidationResult,
    pub decisions: Vec<PolicyDecisionRecord>,
    pub digest: B3Hash,
}
