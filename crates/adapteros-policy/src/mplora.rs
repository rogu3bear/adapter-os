//! DIR (Deterministic Inference Runtime) Policy Pack
//!
//! Enforces orthogonal multi-path LoRA constraints for DIR
//! Reference: <https://openreview.net/pdf?id=jqz6Msm3AF>

use adapteros_core::{AosError, Result, Q15_GATE_DENOMINATOR};
use adapteros_model_hub::manifest::RouterCfg;
use serde::{Deserialize, Serialize};

/// DIR (Deterministic Inference Runtime) policy enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MploraPolicy {
    pub orthogonal_constraints_required: bool,
    pub shared_downsample_required: bool,
    pub compression_ratio_min: f32,
    pub compression_ratio_max: f32,
    pub diversity_threshold_min: f32,
    pub similarity_threshold_max: f32,
    pub penalty_weight_min: f32,
    pub penalty_weight_max: f32,
    pub history_window_min: usize,
    pub history_window_max: usize,
}

impl Default for MploraPolicy {
    fn default() -> Self {
        Self {
            orthogonal_constraints_required: false,
            shared_downsample_required: false,
            compression_ratio_min: 0.5,
            compression_ratio_max: 1.0,
            diversity_threshold_min: 0.01,
            similarity_threshold_max: 0.9,
            penalty_weight_min: 0.01,
            penalty_weight_max: 0.5,
            history_window_min: 5,
            history_window_max: 100,
        }
    }
}

impl MploraPolicy {
    /// Validate router configuration against DIR policy
    pub fn validate_router_config(&self, config: &RouterCfg) -> Result<()> {
        // Validate compression ratio
        if config.compression_ratio < self.compression_ratio_min
            || config.compression_ratio > self.compression_ratio_max
        {
            return Err(AosError::Policy(format!(
                "Compression ratio {} must be between {} and {}",
                config.compression_ratio, self.compression_ratio_min, self.compression_ratio_max
            )));
        }

        // Validate diversity threshold
        if config.diversity_threshold < self.diversity_threshold_min {
            return Err(AosError::Policy(format!(
                "Diversity threshold {} must be >= {}",
                config.diversity_threshold, self.diversity_threshold_min
            )));
        }

        // Validate orthogonal penalty
        if config.orthogonal_penalty < self.penalty_weight_min
            || config.orthogonal_penalty > self.penalty_weight_max
        {
            return Err(AosError::Policy(format!(
                "Orthogonal penalty {} must be between {} and {}",
                config.orthogonal_penalty, self.penalty_weight_min, self.penalty_weight_max
            )));
        }

        // Validate orthogonal constraints requirement
        if self.orthogonal_constraints_required && !config.orthogonal_constraints {
            return Err(AosError::Policy(
                "Orthogonal constraints are required but not enabled".into(),
            ));
        }

        // Validate shared downsample requirement
        if self.shared_downsample_required && !config.shared_downsample {
            return Err(AosError::Policy(
                "Shared downsample is required but not enabled".into(),
            ));
        }

        Ok(())
    }

    /// Check orthogonal constraint compliance
    pub fn check_orthogonal_compliance(
        &self,
        adapter_indices: &[u16],
        gates: &[i16],
        similarity_scores: &[f32],
    ) -> Result<()> {
        if !self.orthogonal_constraints_required {
            return Ok(());
        }

        // Check for high similarity violations
        for &similarity in similarity_scores {
            if similarity > self.similarity_threshold_max {
                return Err(AosError::Policy(format!(
                    "Adapter similarity {} exceeds threshold {}",
                    similarity, self.similarity_threshold_max
                )));
            }
        }

        // Check for low diversity
        let diversity_score = self.compute_diversity_score(adapter_indices, gates);
        if diversity_score < self.diversity_threshold_min {
            return Err(AosError::Policy(format!(
                "Adapter diversity {} below threshold {}",
                diversity_score, self.diversity_threshold_min
            )));
        }

        Ok(())
    }

    /// Compute diversity score for adapter selection
    fn compute_diversity_score(&self, adapter_indices: &[u16], gates: &[i16]) -> f32 {
        if adapter_indices.len() < 2 {
            return 1.0; // Maximum diversity for single adapter
        }

        // Compute gate entropy as diversity measure
        let total_gate: f32 = gates.iter().map(|&g| g as f32 / Q15_GATE_DENOMINATOR).sum();
        if total_gate == 0.0 {
            return 0.0;
        }

        let mut entropy = 0.0;
        for &gate in gates {
            let p = (gate as f32 / Q15_GATE_DENOMINATOR) / total_gate;
            if p > 0.0 {
                entropy -= p * p.log2();
            }
        }

        // Normalize entropy to [0, 1] range
        let max_entropy = (adapter_indices.len() as f32).log2();
        if max_entropy > 0.0 {
            entropy / max_entropy
        } else {
            0.0
        }
    }

    /// Validate DIR configuration parameters
    pub fn validate_mplora_config(&self, config: &MploraConfig) -> Result<()> {
        // Validate compression ratio
        if config.compression_ratio < self.compression_ratio_min
            || config.compression_ratio > self.compression_ratio_max
        {
            return Err(AosError::Policy(format!(
                "DIR compression ratio {} must be between {} and {}",
                config.compression_ratio, self.compression_ratio_min, self.compression_ratio_max
            )));
        }

        // Validate similarity threshold
        if config.similarity_threshold < 0.0 || config.similarity_threshold > 1.0 {
            return Err(AosError::Policy(format!(
                "Similarity threshold {} must be between 0 and 1",
                config.similarity_threshold
            )));
        }

        // Validate penalty weight
        if config.penalty_weight < self.penalty_weight_min
            || config.penalty_weight > self.penalty_weight_max
        {
            return Err(AosError::Policy(format!(
                "Penalty weight {} must be between {} and {}",
                config.penalty_weight, self.penalty_weight_min, self.penalty_weight_max
            )));
        }

        // Validate history window
        if config.history_window < self.history_window_min
            || config.history_window > self.history_window_max
        {
            return Err(AosError::Policy(format!(
                "History window {} must be between {} and {}",
                config.history_window, self.history_window_min, self.history_window_max
            )));
        }

        Ok(())
    }

    /// Check if DIR features are properly configured
    pub fn check_mplora_configuration(&self, config: &MploraConfig) -> Result<()> {
        // Check if orthogonal constraints are properly configured
        if config.orthogonal_constraints {
            if config.similarity_threshold <= 0.0 || config.similarity_threshold >= 1.0 {
                return Err(AosError::Policy(
                    "Orthogonal constraints enabled but similarity threshold not properly set"
                        .into(),
                ));
            }

            if config.penalty_weight <= 0.0 {
                return Err(AosError::Policy(
                    "Orthogonal constraints enabled but penalty weight not properly set".into(),
                ));
            }
        }

        // Check if shared downsample is properly configured
        if config.shared_downsample
            && (config.compression_ratio <= 0.0 || config.compression_ratio > 1.0)
        {
            return Err(AosError::Policy(
                "Shared downsample enabled but compression ratio not properly set".into(),
            ));
        }

        Ok(())
    }
}

/// DIR configuration for policy validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MploraConfig {
    pub shared_downsample: bool,
    pub compression_ratio: f32,
    pub orthogonal_constraints: bool,
    pub similarity_threshold: f32,
    pub penalty_weight: f32,
    pub history_window: usize,
}

impl Default for MploraConfig {
    fn default() -> Self {
        Self {
            shared_downsample: false,
            compression_ratio: 0.8,
            orthogonal_constraints: false,
            similarity_threshold: 0.7,
            penalty_weight: 0.1,
            history_window: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapteros_policy_default() {
        let policy = MploraPolicy::default();
        assert!(!policy.orthogonal_constraints_required);
        assert!(!policy.shared_downsample_required);
        assert_eq!(policy.compression_ratio_min, 0.5);
        assert_eq!(policy.compression_ratio_max, 1.0);
        assert_eq!(policy.diversity_threshold_min, 0.01);
        assert_eq!(policy.similarity_threshold_max, 0.9);
    }

    #[test]
    fn test_router_config_validation() {
        let policy = MploraPolicy::default();

        // Valid configuration
        let valid_config = RouterCfg {
            k_sparse: 3,
            gate_quant: "q15".to_string(),
            entropy_floor: 0.02,
            tau: 1.0,
            sample_tokens_full: 128,
            warmup: false,
            algorithm: "weighted".to_string(),
            safe_mode: false,
            orthogonal_penalty: 0.1,
            shared_downsample: false,
            compression_ratio: 0.8,
            multi_path_enabled: false,
            diversity_threshold: 0.05,
            orthogonal_constraints: false,
        };

        assert!(policy.validate_router_config(&valid_config).is_ok());

        // Invalid compression ratio
        let mut invalid_config = valid_config.clone();
        invalid_config.compression_ratio = 0.3; // Below minimum
        assert!(policy.validate_router_config(&invalid_config).is_err());

        // Invalid diversity threshold
        let mut invalid_config = valid_config.clone();
        invalid_config.diversity_threshold = 0.005; // Below minimum
        assert!(policy.validate_router_config(&invalid_config).is_err());
    }

    #[test]
    fn test_orthogonal_compliance() {
        let policy = MploraPolicy {
            orthogonal_constraints_required: true,
            similarity_threshold_max: 0.8,
            diversity_threshold_min: 0.1,
            ..Default::default()
        };

        // Valid adapter selection
        let adapter_indices = vec![0, 1, 2];
        let gates = vec![16383, 16383, 16383]; // Q15 values
        let similarity_scores = vec![0.3, 0.4, 0.5];

        assert!(policy
            .check_orthogonal_compliance(&adapter_indices, &gates, &similarity_scores)
            .is_ok());

        // High similarity violation
        let high_similarity_scores = vec![0.9, 0.8, 0.7];
        assert!(policy
            .check_orthogonal_compliance(&adapter_indices, &gates, &high_similarity_scores)
            .is_err());
    }

    #[test]
    fn test_diversity_score_computation() {
        let policy = MploraPolicy::default();

        // High diversity (uniform gates)
        let adapter_indices = vec![0, 1, 2];
        let uniform_gates = vec![10922, 10922, 10922]; // Equal Q15 values
        let diversity = policy.compute_diversity_score(&adapter_indices, &uniform_gates);
        assert!(diversity > 0.8); // Should be high diversity

        // Low diversity (single adapter dominant)
        let dominant_gates = vec![32767, 0, 0]; // One adapter dominant
        let diversity = policy.compute_diversity_score(&adapter_indices, &dominant_gates);
        assert!(diversity < 0.5); // Should be low diversity
    }

    #[test]
    fn test_mplora_config_validation() {
        let policy = MploraPolicy::default();

        // Valid DIR configuration
        let valid_config = MploraConfig {
            shared_downsample: true,
            compression_ratio: 0.8,
            orthogonal_constraints: true,
            similarity_threshold: 0.7,
            penalty_weight: 0.1,
            history_window: 10,
        };

        assert!(policy.validate_mplora_config(&valid_config).is_ok());

        // Invalid compression ratio
        let mut invalid_config = valid_config.clone();
        invalid_config.compression_ratio = 0.3; // Below minimum
        assert!(policy.validate_mplora_config(&invalid_config).is_err());

        // Invalid similarity threshold
        let mut invalid_config = valid_config.clone();
        invalid_config.similarity_threshold = 1.5; // Above maximum
        assert!(policy.validate_mplora_config(&invalid_config).is_err());
    }
}
