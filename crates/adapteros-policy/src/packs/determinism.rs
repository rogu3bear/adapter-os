//! Determinism Policy Pack
//!
//! Enforces reproducible outputs through precompiled kernels, HKDF seeding,
//! deterministic retrieval ordering, and epsilon bounds validation.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Compiler flags that break determinism and must be rejected.
pub const FORBIDDEN_COMPILER_FLAGS: &[&str] = &[
    "-ffast-math",
    "-funsafe-math-optimizations",
    "-fno-math-errno",
    "-ffinite-math-only",
];

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
    // Patent 3535886.0002 Claim 5: Explicit kernel allow list
    /// Explicit kernel allow list (kernel names that are permitted)
    /// If None, all kernels are allowed (subject to deny list)
    #[serde(default)]
    pub kernel_allow_list: Option<Vec<String>>,
    /// Kernel deny list (always blocks these kernels, overrides allow list)
    #[serde(default)]
    pub kernel_deny_list: Vec<String>,
    /// Kernel version requirements (kernel_name -> required_version)
    #[serde(default)]
    pub kernel_version_requirements: HashMap<String, String>,
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
                allowed_compiler_flags: vec!["-O2".to_string()],
            },
            // Patent 3535886.0002 Claim 5: Kernel allow/deny lists
            kernel_allow_list: None, // None = all kernels allowed (except denied)
            kernel_deny_list: NON_DETERMINISTIC_KERNELS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            kernel_version_requirements: HashMap::new(),
        }
    }
}

/// Known deterministic kernels (Patent 3535886.0002 Claim 5)
///
/// These kernels are guaranteed to produce deterministic outputs.
pub const DETERMINISTIC_KERNELS: &[&str] = &[
    "gemm_f16_deterministic",
    "gemm_f32_deterministic",
    "attention_deterministic",
    "softmax_stable",
    "layer_norm_deterministic",
    "rope_deterministic",
    "silu_deterministic",
    "gelu_deterministic",
    "rms_norm_deterministic",
    "add_deterministic",
    "mul_deterministic",
    "matmul_deterministic",
];

/// Known non-deterministic kernels to always block (Patent 3535886.0002 Claim 5)
///
/// These kernels may produce non-deterministic outputs due to
/// atomic operations, non-deterministic reduction order, or
/// hardware-specific optimizations.
pub const NON_DETERMINISTIC_KERNELS: &[&str] = &[
    "flash_attention_v1",    // Uses atomic adds
    "gemm_tensorcore_async", // Non-deterministic reduction order
    "attention_fused_fast",  // Non-deterministic warp shuffle
    "softmax_fast",          // Approximation without determinism
    "layer_norm_fast",       // Non-deterministic parallel reduction
];

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

    /// Validate a kernel against the allow/deny lists (Patent 3535886.0002 Claim 5)
    ///
    /// # Arguments
    /// * `kernel_name` - Name of the kernel to validate
    ///
    /// # Returns
    /// * `Ok(())` if the kernel is allowed
    /// * `Err(PolicyViolation)` if the kernel is denied
    pub fn validate_kernel(&self, kernel_name: &str) -> Result<()> {
        // Check deny list first (always takes precedence)
        if self
            .config
            .kernel_deny_list
            .contains(&kernel_name.to_string())
        {
            return Err(AosError::PolicyViolation(format!(
                "Kernel '{}' is in the deny list and cannot be used for deterministic inference",
                kernel_name
            )));
        }

        // If allow list is set, kernel must be in it
        if let Some(ref allow_list) = self.config.kernel_allow_list {
            if !allow_list.contains(&kernel_name.to_string()) {
                return Err(AosError::PolicyViolation(format!(
                    "Kernel '{}' is not in the allow list. Allowed kernels: {:?}",
                    kernel_name, allow_list
                )));
            }
        }

        Ok(())
    }

    /// Validate a kernel with version requirements
    pub fn validate_kernel_version(&self, kernel_name: &str, version: &str) -> Result<()> {
        // First validate the kernel itself
        self.validate_kernel(kernel_name)?;

        // Then check version requirements
        if let Some(required_version) = self.config.kernel_version_requirements.get(kernel_name) {
            if version != required_version {
                return Err(AosError::PolicyViolation(format!(
                    "Kernel '{}' version mismatch: expected {}, got {}",
                    kernel_name, required_version, version
                )));
            }
        }

        Ok(())
    }

    /// Check if a kernel is known to be deterministic
    pub fn is_known_deterministic(kernel_name: &str) -> bool {
        DETERMINISTIC_KERNELS.contains(&kernel_name)
    }

    /// Check if a kernel is known to be non-deterministic
    pub fn is_known_non_deterministic(kernel_name: &str) -> bool {
        NON_DETERMINISTIC_KERNELS.contains(&kernel_name)
    }

    /// Get the list of allowed kernels (for reporting)
    pub fn allowed_kernels(&self) -> Option<&[String]> {
        self.config.kernel_allow_list.as_deref()
    }

    /// Get the list of denied kernels (for reporting)
    pub fn denied_kernels(&self) -> &[String] {
        &self.config.kernel_deny_list
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

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        use crate::Violation;

        let mut violations = Vec::new();
        let mut warnings = Vec::new();
        let metadata = ctx.metadata();

        // Check RNG seeding method
        if let Some(rng_method) = metadata.get("rng_seeding_method") {
            match rng_method.as_str() {
                "system_entropy" | "unseeded" => {
                    violations.push(Violation {
                        severity: Severity::Critical,
                        message: format!("Non-deterministic RNG seeding: {}", rng_method),
                        details: Some(
                            "Use HKDF-seeded or fixed-seed RNG for deterministic execution"
                                .to_string(),
                        ),
                    });
                }
                "hkdf_seeded" | "fixed_seed" => {
                    // Valid - matches expected deterministic seeding
                }
                _ => {
                    warnings.push(format!("Unknown RNG seeding method: {}", rng_method));
                }
            }
        } else {
            warnings.push("No RNG seeding method specified in context".to_string());
        }

        // Check metallib embedding if required
        if self.config.require_metallib_embed {
            match metadata.get("has_metallib") {
                Some(value) if value == "true" => {
                    // Valid - metallib is embedded
                }
                Some(value) if value == "false" => {
                    violations.push(Violation {
                        severity: Severity::Critical,
                        message: "Metallib embedding required but not present".to_string(),
                        details: Some("Policy requires embedded metallib blobs for deterministic kernel execution".to_string()),
                    });
                }
                None => {
                    warnings.push("Metallib embedding status not specified in context".to_string());
                }
                _ => {
                    warnings.push("Invalid metallib embedding status value".to_string());
                }
            }
        }

        // Check kernel hash if required
        if self.config.require_kernel_hash_match {
            if let (Some(expected), Some(actual)) = (
                metadata.get("expected_kernel_hash"),
                metadata.get("actual_kernel_hash"),
            ) {
                if expected != actual {
                    violations.push(Violation {
                        severity: Severity::Critical,
                        message: format!(
                            "Kernel hash mismatch: expected {}, got {}",
                            expected, actual
                        ),
                        details: Some(
                            "Kernel hash must match to ensure deterministic execution".to_string(),
                        ),
                    });
                }
            }
        }

        // Check compiler flags for forbidden options
        if let Some(flags) = metadata.get("compiler_flags") {
            for forbidden in FORBIDDEN_COMPILER_FLAGS {
                if flags.contains(forbidden) {
                    violations.push(Violation {
                        severity: Severity::Critical,
                        message: format!("Forbidden compiler flag detected: {}", forbidden),
                        details: Some(format!("flags: {}", flags)),
                    });
                }
            }
        }

        // Check floating-point mode
        if let Some(fp_mode) = metadata.get("floating_point_mode") {
            match fp_mode.as_str() {
                "fast_math" | "unsafe" | "unknown" => {
                    violations.push(Violation {
                        severity: Severity::High,
                        message: format!("Non-deterministic floating-point mode: {}", fp_mode),
                        details: Some("Use IEEE 754 compliant floating-point mode for deterministic execution".to_string()),
                    });
                }
                "ieee754" | "strict" => {
                    // Valid - IEEE 754 compliant mode
                }
                _ => {
                    warnings.push(format!("Unknown floating-point mode: {}", fp_mode));
                }
            }
        }

        // Check backend type
        if let Some(backend_type) = metadata.get("backend_type") {
            match backend_type.as_str() {
                "coreml" | "metal" => {
                    // Valid - deterministic backends
                }
                "mlx" => {
                    // MLX is deterministic when properly seeded
                    if !metadata
                        .get("rng_seeding_method")
                        .map(|s| s == "hkdf_seeded" || s == "fixed_seed")
                        .unwrap_or(false)
                    {
                        warnings.push(
                            "MLX backend requires proper RNG seeding for determinism".to_string(),
                        );
                    }
                }
                _ => {
                    warnings.push(format!(
                        "Unknown or potentially non-deterministic backend: {}",
                        backend_type
                    ));
                }
            }
        }

        // Check deterministic flag if present
        if let Some(deterministic_flag) = metadata.get("deterministic") {
            if deterministic_flag != "true" {
                violations.push(Violation {
                    severity: Severity::Critical,
                    message: "Backend reports non-deterministic execution".to_string(),
                    details: Some(format!("deterministic flag: {}", deterministic_flag)),
                });
            }
        }

        if violations.is_empty() {
            Ok(Audit::passed(self.id()).with_warnings(warnings))
        } else {
            Ok(Audit::failed(self.id(), violations).with_warnings(warnings))
        }
    }
}

// =============================================================================
// EP-4: OnBeforeInference Hook (PRD-DET-001)
// =============================================================================

/// Context for validating inference determinism (EP-4).
///
/// This struct captures all determinism-critical context that must be
/// validated before inference begins at enforcement point EP-4.
#[derive(Debug, Clone)]
pub struct InferenceDeterminismContext {
    /// Seed mode (Strict/BestEffort/NonDeterministic)
    pub seed_mode: adapteros_core::seed::SeedMode,
    /// Backend type
    pub backend_type: String,
    /// Determinism level from attestation
    pub determinism_level: adapteros_lora_kernel_api::attestation::DeterminismLevel,
    /// Whether backend attestation is verified
    pub attestation_verified: bool,
    /// Whether seed lineage is bound
    pub seed_lineage_bound: bool,
}

/// Validate inference context before inference begins (PRD-DET-001: EP-4).
///
/// This is the enforcement point for the `OnBeforeInference` policy hook.
/// It verifies that all determinism requirements are met before inference
/// proceeds.
///
/// # Enforcement Point: EP-4
///
/// Location: `adapteros-policy/src/packs/determinism.rs:validate_inference_context`
/// Action: Return `AosError::DeterminismViolation` if validation fails
///
/// # Arguments
///
/// * `ctx` - The inference determinism context to validate
///
/// # Returns
///
/// * `Ok(())` if inference can proceed deterministically
/// * `Err(AosError::DeterminismViolation)` with reason if validation fails
pub fn validate_inference_context(ctx: &InferenceDeterminismContext) -> Result<()> {
    use adapteros_core::seed::SeedMode;
    use adapteros_lora_kernel_api::attestation::DeterminismLevel;

    // EP-4.1: Strict mode requires full determinism
    if ctx.seed_mode == SeedMode::Strict {
        // Must have verified attestation
        if !ctx.attestation_verified {
            return Err(AosError::DeterminismViolation(
                "EP-4: Strict mode requires verified backend attestation".into(),
            ));
        }

        // Must have BitExact or BoundedTolerance level
        if ctx.determinism_level == DeterminismLevel::None {
            return Err(AosError::DeterminismViolation(
                "EP-4: Strict mode requires determinism level > None".into(),
            ));
        }

        // Must have seed lineage bound (for replay verification)
        if !ctx.seed_lineage_bound {
            return Err(AosError::DeterminismViolation(
                "EP-4: Strict mode requires seed lineage binding".into(),
            ));
        }
    }

    // EP-4.2: BestEffort mode allows degraded operation but warns
    if ctx.seed_mode == SeedMode::BestEffort && !ctx.attestation_verified {
        tracing::warn!(
            backend_type = %ctx.backend_type,
            "EP-4: BestEffort mode without verified attestation (degraded determinism)"
        );
    }

    // EP-4.3: NonDeterministic mode bypasses checks (benchmarking only)
    if ctx.seed_mode == SeedMode::NonDeterministic {
        tracing::warn!("EP-4: NonDeterministic mode active - inference is not replayable");
    }

    Ok(())
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

    #[test]
    fn default_allowed_flags_exclude_forbidden_entries() {
        let config = DeterminismConfig::default();

        for forbidden in FORBIDDEN_COMPILER_FLAGS {
            assert!(
                !config
                    .toolchain_requirements
                    .allowed_compiler_flags
                    .iter()
                    .any(|flag| flag.contains(forbidden)),
                "Allowed compiler flags must not include forbidden flag {}",
                forbidden
            );
        }
    }

    // Patent 3535886.0002 Claim 5: Kernel allow list tests

    #[test]
    fn test_kernel_deny_list_default() {
        let config = DeterminismConfig::default();
        // Default should have non-deterministic kernels in deny list
        assert!(!config.kernel_deny_list.is_empty());
        assert!(config
            .kernel_deny_list
            .contains(&"flash_attention_v1".to_string()));
    }

    #[test]
    fn test_kernel_validation_denied() {
        let config = DeterminismConfig::default();
        let policy = DeterminismPolicy::new(config);

        // Denied kernel should fail
        assert!(policy.validate_kernel("flash_attention_v1").is_err());
    }

    #[test]
    fn test_kernel_validation_allowed_by_default() {
        let mut config = DeterminismConfig::default();
        config.kernel_deny_list.clear(); // Clear deny list for test
        let policy = DeterminismPolicy::new(config);

        // When no allow list is set, any non-denied kernel is allowed
        assert!(policy.validate_kernel("gemm_f16_deterministic").is_ok());
        assert!(policy.validate_kernel("custom_kernel").is_ok());
    }

    #[test]
    fn test_kernel_validation_explicit_allow_list() {
        let mut config = DeterminismConfig::default();
        config.kernel_allow_list = Some(vec![
            "gemm_f16_deterministic".to_string(),
            "attention_deterministic".to_string(),
        ]);
        config.kernel_deny_list.clear();
        let policy = DeterminismPolicy::new(config);

        // Allowed kernel should pass
        assert!(policy.validate_kernel("gemm_f16_deterministic").is_ok());

        // Non-allowed kernel should fail
        assert!(policy.validate_kernel("custom_kernel").is_err());
    }

    #[test]
    fn test_kernel_deny_list_overrides_allow_list() {
        let mut config = DeterminismConfig::default();
        config.kernel_allow_list = Some(vec![
            "flash_attention_v1".to_string(), // Try to allow a non-deterministic kernel
        ]);
        // Keep default deny list which includes flash_attention_v1
        let policy = DeterminismPolicy::new(config);

        // Deny list should take precedence
        assert!(policy.validate_kernel("flash_attention_v1").is_err());
    }

    #[test]
    fn test_kernel_version_validation() {
        let mut config = DeterminismConfig::default();
        config.kernel_deny_list.clear();
        config
            .kernel_version_requirements
            .insert("gemm_f16_deterministic".to_string(), "1.2.0".to_string());
        let policy = DeterminismPolicy::new(config);

        // Correct version should pass
        assert!(policy
            .validate_kernel_version("gemm_f16_deterministic", "1.2.0")
            .is_ok());

        // Wrong version should fail
        assert!(policy
            .validate_kernel_version("gemm_f16_deterministic", "1.1.0")
            .is_err());
    }

    #[test]
    fn test_is_known_deterministic() {
        assert!(DeterminismPolicy::is_known_deterministic(
            "gemm_f16_deterministic"
        ));
        assert!(DeterminismPolicy::is_known_deterministic(
            "attention_deterministic"
        ));
        assert!(!DeterminismPolicy::is_known_deterministic("custom_kernel"));
    }

    #[test]
    fn test_is_known_non_deterministic() {
        assert!(DeterminismPolicy::is_known_non_deterministic(
            "flash_attention_v1"
        ));
        assert!(DeterminismPolicy::is_known_non_deterministic(
            "gemm_tensorcore_async"
        ));
        assert!(!DeterminismPolicy::is_known_non_deterministic(
            "gemm_f16_deterministic"
        ));
    }
}
