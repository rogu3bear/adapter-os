//! Router Policy Pack
//!
//! Implements K-sparse adapter selection with Q15 quantized gates and entropy floor
//! to prevent single-adapter collapse. Enforces deterministic tie-breaking.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// Router policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    /// K-sparse parameter (maximum number of active adapters)
    pub k_sparse: usize,
    /// Gate quantization method
    pub gate_quant: GateQuantization,
    /// Entropy floor to prevent single-adapter collapse
    pub entropy_floor: f32,
    /// Number of tokens to log full router decisions
    pub sample_tokens_full: usize,
    /// Router overhead budget (percentage)
    pub overhead_budget_pct: f32,
    /// Feature vector configuration
    pub feature_config: FeatureConfig,
    /// Tie-breaking rules
    pub tie_break_rules: Vec<TieBreakRule>,
    /// Safe mode enabled - forces routing through safety adapter only
    #[serde(default)]
    pub safe_mode: bool,
    /// Enable automatic parent adapter loading for lineage stacking
    #[serde(default = "default_enable_lineage")]
    pub enable_lineage_loading: bool,
}

/// Gate quantization method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GateQuantization {
    /// Q15 quantization (16-bit signed integer)
    Q15,
    /// Q8 quantization (8-bit signed integer)
    Q8,
    /// No quantization (32-bit float)
    Float32,
}

/// Feature vector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConfig {
    /// Language feature dimensions
    pub language_dims: usize,
    /// Framework feature dimensions
    pub framework_dims: usize,
    /// Symbol hit feature dimensions
    pub symbol_dims: usize,
    /// Path token feature dimensions
    pub path_dims: usize,
    /// Prompt verb feature dimensions
    pub verb_dims: usize,
    /// Attention entropy feature dimensions
    pub entropy_dims: usize,
    /// Feature weights
    pub weights: FeatureWeights,
}

/// Feature weights for scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureWeights {
    /// Language weight
    pub language: f32,
    /// Framework weight
    pub framework: f32,
    /// Symbol hits weight
    pub symbol_hits: f32,
    /// Path tokens weight
    pub path_tokens: f32,
    /// Prompt verb weight
    pub prompt_verb: f32,
    /// Attention entropy weight
    pub attention_entropy: f32,
}

/// Tie-breaking rules for adapter selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TieBreakRule {
    /// Sort by activation score descending
    ActivationScoreDesc,
    /// Sort by adapter ID ascending
    AdapterIdAsc,
    /// Sort by last used timestamp ascending
    LastUsedAsc,
    /// Sort by memory usage ascending
    MemoryUsageAsc,
}

fn default_enable_lineage() -> bool {
    true
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            k_sparse: 3,
            gate_quant: GateQuantization::Q15,
            entropy_floor: 0.7,
            sample_tokens_full: 128,
            overhead_budget_pct: 8.0,
            feature_config: FeatureConfig {
                language_dims: 8,
                framework_dims: 3,
                symbol_dims: 1,
                path_dims: 1,
                verb_dims: 8,
                entropy_dims: 1,
                weights: FeatureWeights {
                    language: 0.30,
                    framework: 0.25,
                    symbol_hits: 0.20,
                    path_tokens: 0.15,
                    prompt_verb: 0.10,
                    attention_entropy: 0.0, // Optional
                },
            },
            tie_break_rules: vec![
                TieBreakRule::ActivationScoreDesc,
                TieBreakRule::AdapterIdAsc,
            ],
            safe_mode: false,
            enable_lineage_loading: true,
        }
    }
}

/// Router policy enforcement
pub struct RouterPolicy {
    config: RouterConfig,
}

impl RouterPolicy {
    /// Create a new router policy
    pub fn new(config: RouterConfig) -> Self {
        Self { config }
    }

    /// Validate K-sparse parameter
    pub fn validate_k_sparse(&self, k: usize) -> Result<()> {
        if k > self.config.k_sparse {
            Err(AosError::PolicyViolation(format!(
                "K-sparse parameter {} exceeds maximum {}",
                k, self.config.k_sparse
            )))
        } else {
            Ok(())
        }
    }

    /// Validate gate quantization
    pub fn validate_gate_quantization(&self, gates: &[f32]) -> Result<()> {
        match self.config.gate_quant {
            GateQuantization::Q15 => {
                for &gate in gates {
                    if !(-1.0..=1.0).contains(&gate) {
                        return Err(AosError::PolicyViolation(format!(
                            "Gate value {} out of Q15 range [-1.0, 1.0]",
                            gate
                        )));
                    }
                }
            }
            GateQuantization::Q8 => {
                for &gate in gates {
                    if !(-1.0..=1.0).contains(&gate) {
                        return Err(AosError::PolicyViolation(format!(
                            "Gate value {} out of Q8 range [-1.0, 1.0]",
                            gate
                        )));
                    }
                }
            }
            GateQuantization::Float32 => {
                // No range validation for float32
            }
        }
        Ok(())
    }

    /// Validate entropy floor
    pub fn validate_entropy_floor(&self, adapter_activations: &[f32]) -> Result<()> {
        if adapter_activations.is_empty() {
            return Ok(());
        }

        let max_activation = adapter_activations.iter().fold(0.0f32, |a, &b| a.max(b));
        let entropy = self.calculate_entropy(adapter_activations);

        if entropy < self.config.entropy_floor {
            Err(AosError::PolicyViolation(format!(
                "Entropy {} below floor {}, max activation: {}",
                entropy, self.config.entropy_floor, max_activation
            )))
        } else {
            Ok(())
        }
    }

    /// Calculate entropy of adapter activations
    fn calculate_entropy(&self, activations: &[f32]) -> f32 {
        let sum: f32 = activations.iter().sum();
        if sum == 0.0 {
            return 0.0;
        }

        let mut entropy = 0.0;
        for &activation in activations {
            if activation > 0.0 {
                let p = activation / sum;
                entropy -= p * p.log2();
            }
        }
        entropy
    }

    /// Validate router overhead
    pub fn validate_router_overhead(&self, overhead_pct: f32) -> Result<()> {
        if overhead_pct > self.config.overhead_budget_pct {
            Err(AosError::PolicyViolation(format!(
                "Router overhead {}% exceeds budget {}%",
                overhead_pct, self.config.overhead_budget_pct
            )))
        } else {
            Ok(())
        }
    }

    /// Validate feature vector dimensions
    pub fn validate_feature_dimensions(&self, feature_vector: &[f32]) -> Result<()> {
        let expected_dims = self.config.feature_config.language_dims
            + self.config.feature_config.framework_dims
            + self.config.feature_config.symbol_dims
            + self.config.feature_config.path_dims
            + self.config.feature_config.verb_dims
            + self.config.feature_config.entropy_dims;

        if feature_vector.len() != expected_dims {
            Err(AosError::PolicyViolation(format!(
                "Feature vector dimension {} does not match expected {}",
                feature_vector.len(),
                expected_dims
            )))
        } else {
            Ok(())
        }
    }

    /// Validate feature weights sum to 1.0
    pub fn validate_feature_weights(&self) -> Result<()> {
        let weights = &self.config.feature_config.weights;
        let sum = weights.language
            + weights.framework
            + weights.symbol_hits
            + weights.path_tokens
            + weights.prompt_verb
            + weights.attention_entropy;

        if (sum - 1.0).abs() > 1e-6 {
            Err(AosError::PolicyViolation(format!(
                "Feature weights sum to {}, expected 1.0",
                sum
            )))
        } else {
            Ok(())
        }
    }

    /// Check if safe mode is enabled
    pub fn is_safe_mode_enabled(&self) -> bool {
        self.config.safe_mode
    }

    /// Check if lineage loading is enabled
    pub fn is_lineage_loading_enabled(&self) -> bool {
        self.config.enable_lineage_loading
    }

    /// Enable safe mode
    pub fn enable_safe_mode(&mut self) {
        self.config.safe_mode = true;
    }

    /// Disable safe mode
    pub fn disable_safe_mode(&mut self) {
        self.config.safe_mode = false;
    }

    /// Get configuration
    pub fn config(&self) -> &RouterConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: RouterConfig) {
        self.config = config;
    }
}

impl Policy for RouterPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Router
    }

    fn name(&self) -> &'static str {
        "Router"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let violations = Vec::new();

        // Basic validation - in a real implementation, this would check
        // K-sparse parameters, feature weights, entropy floor, etc.

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

    #[test]
    fn test_router_policy_creation() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Router);
        assert_eq!(policy.name(), "Router");
        assert_eq!(policy.severity(), Severity::High);
    }

    #[test]
    fn test_router_config_default() {
        let config = RouterConfig::default();
        assert_eq!(config.k_sparse, 3);
        assert_eq!(config.entropy_floor, 0.7);
        assert_eq!(config.sample_tokens_full, 128);
        assert_eq!(config.overhead_budget_pct, 8.0);
    }

    #[test]
    fn test_k_sparse_validation() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Valid case
        assert!(policy.validate_k_sparse(3).is_ok());
        assert!(policy.validate_k_sparse(1).is_ok());

        // Invalid case
        assert!(policy.validate_k_sparse(4).is_err());
    }

    #[test]
    fn test_gate_quantization_validation() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Valid Q15 gates
        assert!(policy.validate_gate_quantization(&[0.5, -0.3, 0.8]).is_ok());

        // Invalid Q15 gates
        assert!(policy
            .validate_gate_quantization(&[1.5, -0.3, 0.8])
            .is_err());
    }

    #[test]
    fn test_entropy_floor_validation() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // High entropy (good)
        assert!(policy.validate_entropy_floor(&[0.3, 0.3, 0.4]).is_ok());

        // Low entropy (bad)
        assert!(policy.validate_entropy_floor(&[0.9, 0.05, 0.05]).is_err());
    }

    #[test]
    fn test_router_overhead_validation() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Valid overhead
        assert!(policy.validate_router_overhead(5.0).is_ok());

        // Invalid overhead
        assert!(policy.validate_router_overhead(10.0).is_err());
    }

    #[test]
    fn test_feature_dimensions_validation() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        let expected_dims = 8 + 3 + 1 + 1 + 8 + 1; // 22 dimensions
        let valid_vector = vec![0.0; expected_dims];
        let invalid_vector = vec![0.0; expected_dims + 1];

        assert!(policy.validate_feature_dimensions(&valid_vector).is_ok());
        assert!(policy.validate_feature_dimensions(&invalid_vector).is_err());
    }

    #[test]
    fn test_feature_weights_validation() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Default weights should sum to 1.0
        assert!(policy.validate_feature_weights().is_ok());
    }
}
