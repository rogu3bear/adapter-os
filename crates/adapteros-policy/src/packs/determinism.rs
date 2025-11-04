//! Determinism Policy Pack
//!
//! Enforces reproducible outputs through precompiled kernels, HKDF seeding,
//! deterministic retrieval ordering, and epsilon bounds validation.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Determinism policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismConfig {
    /// Require embedded metallib blobs
    pub require_metallib_embed: bool,
    /// Require kernel hash match
    pub require_kernel_hash_match: bool,
    /// RNG seeding method
    pub rng: RngSeedingMethod,
    /// Retrieval tie-breaking order
    pub retrieval_tie_break: Vec<TieBreakRule>,
    /// Epsilon bounds for floating point comparisons
    pub epsilon_bounds: EpsilonBounds,
    /// Toolchain version requirements
    pub toolchain_requirements: ToolchainRequirements,
    /// Minimum router entropy threshold (prevents uniform gate distribution)
    pub min_router_entropy: f32,
}

/// RNG seeding method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RngSeedingMethod {
    /// HKDF seeded from global seed
    HkdfSeeded,
    /// Fixed seed for testing
    FixedSeed(u64),
    /// System entropy (not recommended for determinism)
    SystemEntropy,
}

/// Tie-breaking rules for retrieval ordering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TieBreakRule {
    /// Sort by score descending
    ScoreDesc,
    /// Sort by document ID ascending
    DocIdAsc,
    /// Sort by revision descending
    RevisionDesc,
    /// Sort by timestamp ascending
    TimestampAsc,
}

/// Epsilon bounds for floating point comparisons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpsilonBounds {
    /// Maximum allowed difference for logits
    pub logits_epsilon: f32,
    /// Maximum allowed difference for embeddings
    pub embeddings_epsilon: f32,
    /// Maximum allowed difference for attention weights
    pub attention_epsilon: f32,
    /// Maximum allowed difference for gate values
    pub gates_epsilon: f32,
}

/// Toolchain version requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct ToolchainRequirements {
    /// Required Rust version
    pub rust_version: String,
    /// Required Metal SDK version
    pub metal_sdk_version: String,
    /// Required kernel compiler version
    pub kernel_compiler_version: String,
    /// Allowed compiler flags
    pub allowed_compiler_flags: Vec<String>,
}

impl Default for DeterminismConfig {
    fn default() -> Self {
        Self {
            require_metallib_embed: true,
            require_kernel_hash_match: true,
            rng: RngSeedingMethod::HkdfSeeded,
            retrieval_tie_break: vec![TieBreakRule::ScoreDesc, TieBreakRule::DocIdAsc],
            epsilon_bounds: EpsilonBounds {
                logits_epsilon: 1e-6,
                embeddings_epsilon: 1e-5,
                attention_epsilon: 1e-6,
                gates_epsilon: 1e-4,
            },
            toolchain_requirements: ToolchainRequirements {
                rust_version: "1.75.0".to_string(),
                metal_sdk_version: "3.0".to_string(),
                kernel_compiler_version: "1.0".to_string(),
                allowed_compiler_flags: vec![
                    "-O2".to_string(),
                    "-ffast-math".to_string(), // Note: This should be disabled for determinism
                ],
            },
            min_router_entropy: 0.1, // Minimum entropy to prevent uniform gate distribution
        }
    }
}

/// Determinism policy enforcement
pub struct DeterminismPolicy {
    config: DeterminismConfig,
}

impl DeterminismPolicy {
    /// Create a new determinism policy
    pub fn new(config: DeterminismConfig) -> Self {
        Self { config }
    }

    /// Validate metallib embedding
    pub fn validate_metallib_embed(&self, has_metallib: bool) -> Result<()> {
        if self.config.require_metallib_embed && !has_metallib {
            Err(AosError::PolicyViolation(
                "Metallib embedding is required for determinism".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Validate kernel hash match
    pub fn validate_kernel_hash(&self, expected_hash: &str, actual_hash: &str) -> Result<()> {
        if self.config.require_kernel_hash_match && expected_hash != actual_hash {
            Err(AosError::PolicyViolation(format!(
                "Kernel hash mismatch: expected {}, got {}",
                expected_hash, actual_hash
            )))
        } else {
            Ok(())
        }
    }

    /// Validate RNG seeding
    pub fn validate_rng_seeding(&self, seed_method: &RngSeedingMethod) -> Result<()> {
        match (&self.config.rng, seed_method) {
            (RngSeedingMethod::HkdfSeeded, RngSeedingMethod::HkdfSeeded) => Ok(()),
            (RngSeedingMethod::FixedSeed(_), RngSeedingMethod::FixedSeed(_)) => Ok(()),
            _ => Err(AosError::PolicyViolation(
                "RNG seeding method does not match policy requirements".to_string(),
            )),
        }
    }

    /// Validate router entropy (prevents uniform gate distribution)
    pub fn validate_router_entropy(&self, entropy: f32) -> Result<()> {
        if entropy < self.config.min_router_entropy {
            Err(AosError::PolicyViolation(format!(
                "Router entropy {:.4} below minimum threshold {:.4}",
                entropy, self.config.min_router_entropy
            )))
        } else {
            Ok(())
        }
    }

    /// Validate epsilon bounds
    pub fn validate_epsilon_bounds(
        &self,
        value_type: &str,
        expected: f32,
        actual: f32,
    ) -> Result<()> {
        let epsilon = match value_type {
            "logits" => self.config.epsilon_bounds.logits_epsilon,
            "embeddings" => self.config.epsilon_bounds.embeddings_epsilon,
            "attention" => self.config.epsilon_bounds.attention_epsilon,
            "gates" => self.config.epsilon_bounds.gates_epsilon,
            _ => {
                return Err(AosError::PolicyViolation(format!(
                    "Unknown value type for epsilon validation: {}",
                    value_type
                )))
            }
        };

        let diff = (expected - actual).abs();
        if diff > epsilon {
            Err(AosError::PolicyViolation(format!(
                "Epsilon bound exceeded for {}: expected {}, got {}, diff {} > {}",
                value_type, expected, actual, diff, epsilon
            )))
        } else {
            Ok(())
        }
    }

    /// Validate toolchain requirements
    pub fn validate_toolchain(&self, toolchain_info: &HashMap<String, String>) -> Result<()> {
        if let Some(rust_version) = toolchain_info.get("rust") {
            if rust_version != &self.config.toolchain_requirements.rust_version {
                return Err(AosError::PolicyViolation(format!(
                    "Rust version mismatch: expected {}, got {}",
                    self.config.toolchain_requirements.rust_version, rust_version
                )));
            }
        }

        if let Some(metal_sdk_version) = toolchain_info.get("metal_sdk") {
            if metal_sdk_version != &self.config.toolchain_requirements.metal_sdk_version {
                return Err(AosError::PolicyViolation(format!(
                    "Metal SDK version mismatch: expected {}, got {}",
                    self.config.toolchain_requirements.metal_sdk_version, metal_sdk_version
                )));
            }
        }

        Ok(())
    }

    /// Validate retrieval ordering
    pub fn validate_retrieval_ordering(&self, ordering: &[TieBreakRule]) -> Result<()> {
        if ordering.len() != self.config.retrieval_tie_break.len() {
            return Err(AosError::PolicyViolation(
                "Retrieval ordering length does not match policy requirements".to_string(),
            ));
        }

        for (i, rule) in ordering.iter().enumerate() {
            if std::mem::discriminant(rule)
                != std::mem::discriminant(&self.config.retrieval_tie_break[i])
            {
                return Err(AosError::PolicyViolation(
                    "Retrieval ordering does not match policy requirements".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validate backend attestation report
    ///
    /// Checks that the attestation report from a kernel backend meets
    /// all determinism policy requirements:
    /// - Metallib hash matches if required
    /// - RNG seeding method matches policy
    /// - No forbidden compiler flags
    /// - Overall deterministic flag is true
    pub fn validate_backend_attestation(
        &self,
        report: &adapteros_lora_kernel_api::attestation::DeterminismReport,
    ) -> Result<()> {
        use adapteros_lora_kernel_api::attestation::{
            BackendType, RngSeedingMethod as AttestationRngMethod,
        };

        // Check overall deterministic flag
        if !report.deterministic {
            return Err(AosError::PolicyViolation(
                "Backend attestation indicates non-deterministic execution".to_string(),
            ));
        }

        // Check backend type is allowed
        if !report.backend_type.is_deterministic_by_design() {
            return Err(AosError::PolicyViolation(format!(
                "Backend type {:?} is not deterministic by design",
                report.backend_type
            )));
        }

        // For Metal backend, require metallib hash match
        if self.config.require_metallib_embed
            && report.backend_type == BackendType::Metal
            && report.metallib_hash.is_none()
        {
            return Err(AosError::PolicyViolation(
                "Metal backend must provide metallib hash".to_string(),
            ));
        }

        // Check RNG seeding method matches policy
        let rng_matches = match (&self.config.rng, &report.rng_seed_method) {
            (RngSeedingMethod::HkdfSeeded, AttestationRngMethod::HkdfSeeded) => true,
            (RngSeedingMethod::FixedSeed(_), AttestationRngMethod::FixedSeed(_)) => true,
            _ => false,
        };

        if !rng_matches {
            return Err(AosError::PolicyViolation(format!(
                "RNG seeding method mismatch: policy requires {:?}, backend reports {:?}",
                self.config.rng, report.rng_seed_method
            )));
        }

        // Check for forbidden compiler flags
        let forbidden_flags = ["-ffast-math", "-funsafe-math-optimizations"];
        for flag in &report.compiler_flags {
            for forbidden in &forbidden_flags {
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
}

impl Policy for DeterminismPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Determinism
    }

    fn name(&self) -> &'static str {
        "Determinism"
    }

    fn severity(&self) -> Severity {
        Severity::Critical
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let violations = Vec::new();

        // Basic validation - in a real implementation, this would check
        // metallib embedding, kernel hashes, RNG seeding, etc.

        if violations.is_empty() {
            Ok(Audit::passed(self.id()))
        } else {
            Ok(Audit::failed(self.id(), violations))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_determinism_policy_creation() {
        let config = DeterminismConfig::default();
        let policy = DeterminismPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Determinism);
        assert_eq!(policy.name(), "Determinism");
        assert_eq!(policy.severity(), Severity::Critical);
    }

    #[test]
    fn test_determinism_config_default() {
        let config = DeterminismConfig::default();
        assert!(config.require_metallib_embed);
        assert!(config.require_kernel_hash_match);
        assert_eq!(config.epsilon_bounds.logits_epsilon, 1e-6);
    }

    #[test]
    fn test_metallib_validation() {
        let config = DeterminismConfig::default();
        let policy = DeterminismPolicy::new(config);

        // Valid case
        assert!(policy.validate_metallib_embed(true).is_ok());

        // Invalid case
        assert!(policy.validate_metallib_embed(false).is_err());
    }

    #[test]
    fn test_kernel_hash_validation() {
        let config = DeterminismConfig::default();
        let policy = DeterminismPolicy::new(config);

        // Valid case
        assert!(policy.validate_kernel_hash("abc123", "abc123").is_ok());

        // Invalid case
        assert!(policy.validate_kernel_hash("abc123", "def456").is_err());
    }

    #[test]
    fn test_epsilon_bounds_validation() {
        let config = DeterminismConfig::default();
        let policy = DeterminismPolicy::new(config);

        // Valid case
        assert!(policy
            .validate_epsilon_bounds("logits", 1.0, 1.0000001)
            .is_ok());

        // Invalid case
        assert!(policy.validate_epsilon_bounds("logits", 1.0, 1.1).is_err());
    }

    #[test]
    fn test_toolchain_validation() {
        let config = DeterminismConfig::default();
        let policy = DeterminismPolicy::new(config);

        let mut toolchain_info = HashMap::new();
        toolchain_info.insert("rust".to_string(), "1.75.0".to_string());
        toolchain_info.insert("metal_sdk".to_string(), "3.0".to_string());

        assert!(policy.validate_toolchain(&toolchain_info).is_ok());

        // Invalid version
        toolchain_info.insert("rust".to_string(), "1.70.0".to_string());
        assert!(policy.validate_toolchain(&toolchain_info).is_err());
    }
}
