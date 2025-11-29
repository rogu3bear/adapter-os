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

impl Default for RouterConfig {
    fn default() -> Self {
        // Read k_sparse from environment with schema default (4) as fallback
        let k_sparse = std::env::var("AOS_ROUTER_K_SPARSE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);
        Self {
            k_sparse,
            gate_quant: GateQuantization::Q15,
            entropy_floor: 0.02,
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
        }
    }
}

/// Router policy enforcement
pub struct RouterPolicy {
    config: RouterConfig,
}

/// Adapter metadata for policy validation
#[derive(Debug, Clone)]
pub struct AdapterMetadata {
    pub id: String,
    pub tier: String,
    pub tags: Vec<String>,
    pub forbidden_peers: Vec<String>,
}

/// Stack configuration for policy validation
#[derive(Debug, Clone)]
pub struct StackConfiguration {
    pub id: String,
    pub adapter_ids: Vec<String>,
    pub adapters: Vec<AdapterMetadata>,
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

    /// Validate stack configuration (cp-router-003)
    ///
    /// Checks:
    /// 1. K ≤ N adapters in stack
    /// 2. T1 adapters do not combine with forbidden peers
    /// 3. Adapters with conflicting tags cannot co-activate
    pub fn validate_stack_configuration(&self, stack: &StackConfiguration) -> Result<()> {
        // Rule 1: K ≤ N adapters
        if stack.adapters.len() < self.config.k_sparse {
            return Err(AosError::PolicyViolation(format!(
                "Stack has {} adapters but K-sparse requires at least {}",
                stack.adapters.len(),
                self.config.k_sparse
            )));
        }

        // Rule 2: T1 adapters must not combine with forbidden peers
        for adapter in &stack.adapters {
            if adapter.tier == "tier_1" && !adapter.forbidden_peers.is_empty() {
                // Check if any forbidden peer is in the stack
                for peer_id in &adapter.forbidden_peers {
                    if stack.adapter_ids.contains(peer_id) {
                        return Err(AosError::PolicyViolation(format!(
                            "Tier-1 adapter '{}' cannot be in same stack as forbidden peer '{}'",
                            adapter.id, peer_id
                        )));
                    }
                }
            }
        }

        // Rule 3: Adapters with conflicting tags cannot co-activate
        let conflicting_tag_pairs = [
            ("security", "performance"),
            ("strict", "permissive"),
            ("production", "experimental"),
        ];

        for adapter in &stack.adapters {
            for (tag_a, tag_b) in &conflicting_tag_pairs {
                if adapter.tags.contains(&tag_a.to_string()) {
                    // Check if any other adapter has the conflicting tag
                    for other in &stack.adapters {
                        if other.id != adapter.id && other.tags.contains(&tag_b.to_string()) {
                            return Err(AosError::PolicyViolation(format!(
                                "Adapters '{}' (tag: '{}') and '{}' (tag: '{}') have conflicting tags and cannot co-activate",
                                adapter.id, tag_a, other.id, tag_b
                            )));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate router decision at runtime (cp-router-003)
    ///
    /// Checks made at decision time if configuration changed:
    /// 1. Selected adapters do not exceed K
    /// 2. Entropy floor is enforced
    /// 3. No forbidden peer combinations in selection
    pub fn validate_decision(
        &self,
        selected_indices: &[u16],
        gates: &[f32],
        stack: Option<&StackConfiguration>,
    ) -> Result<()> {
        // Rule 1: Selected adapters ≤ K
        if selected_indices.len() > self.config.k_sparse {
            return Err(AosError::PolicyViolation(format!(
                "Decision selected {} adapters but K-sparse limit is {}",
                selected_indices.len(),
                self.config.k_sparse
            )));
        }

        // Rule 2: Enforce entropy floor
        if gates.len() != selected_indices.len() {
            return Err(AosError::PolicyViolation(format!(
                "Gate count ({}) does not match selected adapter count ({})",
                gates.len(),
                selected_indices.len()
            )));
        }

        let entropy = self.calculate_entropy(gates);
        if entropy < self.config.entropy_floor {
            return Err(AosError::PolicyViolation(format!(
                "Decision entropy {} below floor {}",
                entropy, self.config.entropy_floor
            )));
        }

        // Rule 3: Check forbidden peer combinations in selection (if stack provided)
        if let Some(stack) = stack {
            let selected_adapter_ids: Vec<&String> = selected_indices
                .iter()
                .filter_map(|&idx| stack.adapters.get(idx as usize).map(|a| &a.id))
                .collect();

            for &adapter_id in &selected_adapter_ids {
                if let Some(adapter) = stack.adapters.iter().find(|a| &a.id == adapter_id) {
                    if adapter.tier == "tier_1" {
                        for forbidden_peer in &adapter.forbidden_peers {
                            if selected_adapter_ids.contains(&forbidden_peer) {
                                return Err(AosError::PolicyViolation(format!(
                                    "Decision selected tier-1 adapter '{}' with forbidden peer '{}'",
                                    adapter_id, forbidden_peer
                                )));
                            }
                        }
                    }
                }
            }
        }

        Ok(())
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

    /// Helper to create test adapter metadata
    fn create_adapter(
        id: &str,
        tier: &str,
        tags: Vec<String>,
        forbidden_peers: Vec<String>,
    ) -> AdapterMetadata {
        AdapterMetadata {
            id: id.to_string(),
            tier: tier.to_string(),
            tags,
            forbidden_peers,
        }
    }

    /// Helper to create test stack configuration
    fn create_stack(id: &str, adapters: Vec<AdapterMetadata>) -> StackConfiguration {
        let adapter_ids = adapters.iter().map(|a| a.id.clone()).collect();
        StackConfiguration {
            id: id.to_string(),
            adapter_ids,
            adapters,
        }
    }

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
        assert_eq!(config.k_sparse, 4);
        assert_eq!(config.entropy_floor, 0.02);
        assert_eq!(config.sample_tokens_full, 128);
        assert_eq!(config.overhead_budget_pct, 8.0);
    }

    #[test]
    fn test_k_sparse_validation() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Valid case
        assert!(policy.validate_k_sparse(4).is_ok());
        assert!(policy.validate_k_sparse(1).is_ok());

        // Invalid case
        assert!(policy.validate_k_sparse(5).is_err());
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

        // Low entropy (bad) - single adapter dominates with > 99.9%
        assert!(policy.validate_entropy_floor(&[0.9997, 0.0001, 0.0001, 0.0001]).is_err());
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

    #[test]
    fn test_stack_configuration_validation_success() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Valid stack with 4 adapters (K=4)
        let stack = StackConfiguration {
            id: "test-stack".to_string(),
            adapter_ids: vec!["a1".to_string(), "a2".to_string(), "a3".to_string(), "a4".to_string()],
            adapters: vec![
                AdapterMetadata {
                    id: "a1".to_string(),
                    tier: "tier_0".to_string(),
                    tags: vec!["security".to_string()],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a2".to_string(),
                    tier: "tier_1".to_string(),
                    tags: vec!["reliability".to_string()],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a3".to_string(),
                    tier: "tier_2".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a4".to_string(),
                    tier: "tier_0".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
            ],
        };

        assert!(policy.validate_stack_configuration(&stack).is_ok());
    }

    #[test]
    fn test_stack_configuration_insufficient_adapters() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Stack with only 2 adapters (K=4)
        let stack = StackConfiguration {
            id: "test-stack".to_string(),
            adapter_ids: vec!["a1".to_string(), "a2".to_string()],
            adapters: vec![
                AdapterMetadata {
                    id: "a1".to_string(),
                    tier: "tier_0".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a2".to_string(),
                    tier: "tier_1".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
            ],
        };

        let result = policy.validate_stack_configuration(&stack);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("K-sparse requires at least"));
    }

    #[test]
    fn test_stack_configuration_forbidden_peer_violation() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Stack with forbidden peer combination
        let stack = StackConfiguration {
            id: "test-stack".to_string(),
            adapter_ids: vec!["a1".to_string(), "a2".to_string(), "a3".to_string(), "a4".to_string()],
            adapters: vec![
                AdapterMetadata {
                    id: "a1".to_string(),
                    tier: "tier_1".to_string(),
                    tags: vec![],
                    forbidden_peers: vec!["a2".to_string()],
                },
                AdapterMetadata {
                    id: "a2".to_string(),
                    tier: "tier_0".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a3".to_string(),
                    tier: "tier_2".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a4".to_string(),
                    tier: "tier_0".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
            ],
        };

        let result = policy.validate_stack_configuration(&stack);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("forbidden peer"));
    }

    #[test]
    fn test_stack_configuration_conflicting_tags() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Stack with conflicting tags
        let stack = StackConfiguration {
            id: "test-stack".to_string(),
            adapter_ids: vec!["a1".to_string(), "a2".to_string(), "a3".to_string(), "a4".to_string()],
            adapters: vec![
                AdapterMetadata {
                    id: "a1".to_string(),
                    tier: "tier_0".to_string(),
                    tags: vec!["security".to_string()],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a2".to_string(),
                    tier: "tier_1".to_string(),
                    tags: vec!["performance".to_string()],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a3".to_string(),
                    tier: "tier_2".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a4".to_string(),
                    tier: "tier_0".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
            ],
        };

        let result = policy.validate_stack_configuration(&stack);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("conflicting tags"));
    }

    #[test]
    fn test_decision_validation_success() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Valid decision with 4 adapters and proper entropy
        let selected_indices = vec![0, 1, 2, 3];
        let gates = vec![0.3, 0.3, 0.25, 0.15]; // Reasonable entropy

        assert!(policy
            .validate_decision(&selected_indices, &gates, None)
            .is_ok());
    }

    #[test]
    fn test_decision_validation_exceeds_k() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Decision selects 5 adapters but K=4
        let selected_indices = vec![0, 1, 2, 3, 4];
        let gates = vec![0.25, 0.25, 0.2, 0.15, 0.15];

        let result = policy.validate_decision(&selected_indices, &gates, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("K-sparse limit"));
    }

    #[test]
    fn test_decision_validation_low_entropy() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        // Decision with low entropy (single adapter dominates)
        let selected_indices = vec![0, 1, 2, 3];
        let gates = vec![0.9997, 0.0001, 0.0001, 0.0001]; // Very low entropy, below 0.02

        let result = policy.validate_decision(&selected_indices, &gates, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("entropy"));
    }

    #[test]
    fn test_decision_validation_forbidden_peer_in_selection() {
        let config = RouterConfig::default();
        let policy = RouterPolicy::new(config);

        let stack = StackConfiguration {
            id: "test-stack".to_string(),
            adapter_ids: vec!["a1".to_string(), "a2".to_string(), "a3".to_string(), "a4".to_string()],
            adapters: vec![
                AdapterMetadata {
                    id: "a1".to_string(),
                    tier: "tier_1".to_string(),
                    tags: vec![],
                    forbidden_peers: vec!["a2".to_string()],
                },
                AdapterMetadata {
                    id: "a2".to_string(),
                    tier: "tier_0".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a3".to_string(),
                    tier: "tier_2".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
                AdapterMetadata {
                    id: "a4".to_string(),
                    tier: "tier_0".to_string(),
                    tags: vec![],
                    forbidden_peers: vec![],
                },
            ],
        };

        // Selection includes both a1 (tier-1) and a2 (forbidden peer)
        let selected_indices = vec![0, 1, 2, 3];
        let gates = vec![0.3, 0.3, 0.25, 0.15];

        let result = policy.validate_decision(&selected_indices, &gates, Some(&stack));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("forbidden peer"));
    }
}
