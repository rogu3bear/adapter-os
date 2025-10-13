//! MPLoRA Policy Pack
//!
//! Enforces MPLoRA (Multi-Path LoRA) configuration and constraints.
//! Manages shared downsample, compression ratios, and orthogonal constraints.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MPLoRA policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MploraConfig {
    /// Enable shared downsample
    pub shared_downsample: bool,
    /// Compression ratio
    pub compression_ratio: f32,
    /// Enable orthogonal constraints
    pub orthogonal_constraints: bool,
    /// Similarity threshold
    pub similarity_threshold: f32,
    /// Penalty weight
    pub penalty_weight: f32,
    /// History window size
    pub history_window: usize,
    /// Path constraints
    pub path_constraints: PathConstraints,
    /// Performance constraints
    pub performance_constraints: PerformanceConstraints,
}

/// Path constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathConstraints {
    /// Maximum number of paths
    pub max_paths: usize,
    /// Minimum path weight
    pub min_path_weight: f32,
    /// Maximum path weight
    pub max_path_weight: f32,
    /// Path selection strategy
    pub selection_strategy: PathSelectionStrategy,
}

/// Path selection strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PathSelectionStrategy {
    /// Greedy selection
    Greedy,
    /// Top-K selection
    TopK,
    /// Weighted random selection
    WeightedRandom,
    /// Deterministic selection
    Deterministic,
}

/// Performance constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConstraints {
    /// Maximum inference time (ms)
    pub max_inference_time_ms: u64,
    /// Maximum memory usage (MB)
    pub max_memory_usage_mb: usize,
    /// Maximum CPU usage (percentage)
    pub max_cpu_usage_pct: f32,
    /// Enable performance monitoring
    pub enable_monitoring: bool,
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
            path_constraints: PathConstraints {
                max_paths: 8,
                min_path_weight: 0.1,
                max_path_weight: 1.0,
                selection_strategy: PathSelectionStrategy::TopK,
            },
            performance_constraints: PerformanceConstraints {
                max_inference_time_ms: 100,
                max_memory_usage_mb: 1024,
                max_cpu_usage_pct: 80.0,
                enable_monitoring: true,
            },
        }
    }
}

/// MPLoRA path information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MploraPath {
    /// Path ID
    pub path_id: u32,
    /// Path weight
    pub weight: f32,
    /// Path rank
    pub rank: usize,
    /// Path activations
    pub activations: Vec<f32>,
    /// Path metadata
    pub metadata: HashMap<String, String>,
}

/// MPLoRA performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MploraPerformanceMetrics {
    /// Inference time (ms)
    pub inference_time_ms: u64,
    /// Memory usage (MB)
    pub memory_usage_mb: usize,
    /// CPU usage (percentage)
    pub cpu_usage_pct: f32,
    /// Path utilization
    pub path_utilization: f32,
    /// Compression efficiency
    pub compression_efficiency: f32,
}

/// MPLoRA policy enforcement
pub struct MploraPolicy {
    config: MploraConfig,
}

impl MploraPolicy {
    /// Create a new MPLoRA policy
    pub fn new(config: MploraConfig) -> Self {
        Self { config }
    }

    /// Validate compression ratio
    pub fn validate_compression_ratio(&self, ratio: f32) -> Result<()> {
        if !(0.1..=1.0).contains(&ratio) {
            Err(AosError::PolicyViolation(format!(
                "Compression ratio {} must be between 0.1 and 1.0",
                ratio
            )))
        } else if (ratio - self.config.compression_ratio).abs() > 0.1 {
            Err(AosError::PolicyViolation(format!(
                "Compression ratio {} does not match policy requirement {}",
                ratio, self.config.compression_ratio
            )))
        } else {
            Ok(())
        }
    }

    /// Validate similarity threshold
    pub fn validate_similarity_threshold(&self, threshold: f32) -> Result<()> {
        if !(0.0..=1.0).contains(&threshold) {
            Err(AosError::PolicyViolation(format!(
                "Similarity threshold {} must be between 0.0 and 1.0",
                threshold
            )))
        } else if (threshold - self.config.similarity_threshold).abs() > 0.1 {
            Err(AosError::PolicyViolation(format!(
                "Similarity threshold {} does not match policy requirement {}",
                threshold, self.config.similarity_threshold
            )))
        } else {
            Ok(())
        }
    }

    /// Validate path constraints
    pub fn validate_path_constraints(&self, paths: &[MploraPath]) -> Result<()> {
        if paths.len() > self.config.path_constraints.max_paths {
            return Err(AosError::PolicyViolation(format!(
                "Number of paths {} exceeds maximum {}",
                paths.len(),
                self.config.path_constraints.max_paths
            )));
        }

        for path in paths {
            if path.weight < self.config.path_constraints.min_path_weight
                || path.weight > self.config.path_constraints.max_path_weight
            {
                return Err(AosError::PolicyViolation(format!(
                    "Path weight {} is out of range [{}, {}]",
                    path.weight,
                    self.config.path_constraints.min_path_weight,
                    self.config.path_constraints.max_path_weight
                )));
            }
        }

        Ok(())
    }

    /// Validate performance constraints
    pub fn validate_performance_constraints(
        &self,
        metrics: &MploraPerformanceMetrics,
    ) -> Result<()> {
        if metrics.inference_time_ms > self.config.performance_constraints.max_inference_time_ms {
            return Err(AosError::PolicyViolation(format!(
                "Inference time {}ms exceeds maximum {}ms",
                metrics.inference_time_ms,
                self.config.performance_constraints.max_inference_time_ms
            )));
        }

        if metrics.memory_usage_mb > self.config.performance_constraints.max_memory_usage_mb {
            return Err(AosError::PolicyViolation(format!(
                "Memory usage {}MB exceeds maximum {}MB",
                metrics.memory_usage_mb, self.config.performance_constraints.max_memory_usage_mb
            )));
        }

        if metrics.cpu_usage_pct > self.config.performance_constraints.max_cpu_usage_pct {
            return Err(AosError::PolicyViolation(format!(
                "CPU usage {}% exceeds maximum {}%",
                metrics.cpu_usage_pct, self.config.performance_constraints.max_cpu_usage_pct
            )));
        }

        Ok(())
    }

    /// Validate orthogonal constraints
    pub fn validate_orthogonal_constraints(&self, paths: &[MploraPath]) -> Result<()> {
        if !self.config.orthogonal_constraints {
            return Ok(());
        }

        // Check that paths are orthogonal (simplified check)
        for i in 0..paths.len() {
            for j in (i + 1)..paths.len() {
                let similarity = self.calculate_path_similarity(&paths[i], &paths[j]);
                if similarity > self.config.similarity_threshold {
                    return Err(AosError::PolicyViolation(format!(
                        "Paths {} and {} are too similar: {} > {}",
                        paths[i].path_id,
                        paths[j].path_id,
                        similarity,
                        self.config.similarity_threshold
                    )));
                }
            }
        }

        Ok(())
    }

    /// Calculate path similarity
    fn calculate_path_similarity(&self, path1: &MploraPath, path2: &MploraPath) -> f32 {
        // Simplified similarity calculation based on activations
        if path1.activations.len() != path2.activations.len() {
            return 0.0;
        }

        let dot_product: f32 = path1
            .activations
            .iter()
            .zip(path2.activations.iter())
            .map(|(a, b)| a * b)
            .sum();

        let norm1: f32 = path1.activations.iter().map(|a| a * a).sum::<f32>().sqrt();
        let norm2: f32 = path2.activations.iter().map(|a| a * a).sum::<f32>().sqrt();

        if norm1 == 0.0 || norm2 == 0.0 {
            return 0.0;
        }

        dot_product / (norm1 * norm2)
    }

    /// Validate shared downsample configuration
    pub fn validate_shared_downsample(&self, enabled: bool) -> Result<()> {
        if enabled != self.config.shared_downsample {
            Err(AosError::PolicyViolation(format!(
                "Shared downsample setting {} does not match policy requirement {}",
                enabled, self.config.shared_downsample
            )))
        } else {
            Ok(())
        }
    }

    /// Validate penalty weight
    pub fn validate_penalty_weight(&self, weight: f32) -> Result<()> {
        if !(0.0..=1.0).contains(&weight) {
            Err(AosError::PolicyViolation(format!(
                "Penalty weight {} must be between 0.0 and 1.0",
                weight
            )))
        } else if (weight - self.config.penalty_weight).abs() > 0.05 {
            Err(AosError::PolicyViolation(format!(
                "Penalty weight {} does not match policy requirement {}",
                weight, self.config.penalty_weight
            )))
        } else {
            Ok(())
        }
    }

    /// Validate history window
    pub fn validate_history_window(&self, window_size: usize) -> Result<()> {
        if window_size != self.config.history_window {
            Err(AosError::PolicyViolation(format!(
                "History window size {} does not match policy requirement {}",
                window_size, self.config.history_window
            )))
        } else {
            Ok(())
        }
    }

    /// Select paths based on strategy
    pub fn select_paths(
        &self,
        available_paths: &[MploraPath],
        k: usize,
    ) -> Result<Vec<MploraPath>> {
        if k > self.config.path_constraints.max_paths {
            return Err(AosError::PolicyViolation(format!(
                "Requested {} paths exceeds maximum {}",
                k, self.config.path_constraints.max_paths
            )));
        }

        let selected_paths: Vec<MploraPath> = match self.config.path_constraints.selection_strategy
        {
            PathSelectionStrategy::TopK => {
                let mut paths = available_paths.to_vec();
                paths.sort_by(|a, b| {
                    b.weight
                        .partial_cmp(&a.weight)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                paths.into_iter().take(k).collect::<Vec<_>>()
            }
            PathSelectionStrategy::Greedy => {
                // Simplified greedy selection
                let mut paths = available_paths.to_vec();
                paths.sort_by(|a, b| {
                    b.weight
                        .partial_cmp(&a.weight)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                paths.into_iter().take(k).collect::<Vec<_>>()
            }
            PathSelectionStrategy::WeightedRandom => {
                // Simplified weighted random selection
                let mut paths = available_paths.to_vec();
                paths.sort_by(|a, b| {
                    b.weight
                        .partial_cmp(&a.weight)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                paths.into_iter().take(k).collect::<Vec<_>>()
            }
            PathSelectionStrategy::Deterministic => {
                // Deterministic selection based on path ID
                let mut paths = available_paths.to_vec();
                paths.sort_by(|a, b| a.path_id.cmp(&b.path_id));
                paths.into_iter().take(k).collect::<Vec<_>>()
            }
        };

        // Validate selected paths
        self.validate_path_constraints(&selected_paths)?;
        self.validate_orthogonal_constraints(&selected_paths)?;

        Ok(selected_paths)
    }

    /// Calculate compression efficiency
    pub fn calculate_compression_efficiency(
        &self,
        original_size: usize,
        compressed_size: usize,
    ) -> f32 {
        if original_size == 0 {
            return 0.0;
        }
        1.0 - (compressed_size as f32 / original_size as f32)
    }

    /// Check if performance monitoring is enabled
    pub fn is_performance_monitoring_enabled(&self) -> bool {
        self.config.performance_constraints.enable_monitoring
    }
}

impl Policy for MploraPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Mplora
    }

    fn name(&self) -> &'static str {
        "MPLoRA"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let violations = Vec::new();

        // Basic validation - in a real implementation, this would check
        // specific policy requirements

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
    fn test_mplora_policy_creation() {
        let config = MploraConfig::default();
        let policy = MploraPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Mplora);
        assert_eq!(policy.name(), "MPLoRA");
        assert_eq!(policy.severity(), Severity::Medium);
    }

    #[test]
    fn test_mplora_config_default() {
        let config = MploraConfig::default();
        assert_eq!(config.compression_ratio, 0.8);
        assert_eq!(config.similarity_threshold, 0.7);
        assert_eq!(config.penalty_weight, 0.1);
        assert_eq!(config.history_window, 10);
        assert_eq!(config.path_constraints.max_paths, 8);
    }

    #[test]
    fn test_validate_compression_ratio() {
        let config = MploraConfig::default();
        let policy = MploraPolicy::new(config);

        // Valid ratio
        assert!(policy.validate_compression_ratio(0.8).is_ok());

        // Invalid ratio (out of range)
        assert!(policy.validate_compression_ratio(1.5).is_err());

        // Invalid ratio (doesn't match policy)
        assert!(policy.validate_compression_ratio(0.5).is_err());
    }

    #[test]
    fn test_validate_similarity_threshold() {
        let config = MploraConfig::default();
        let policy = MploraPolicy::new(config);

        // Valid threshold
        assert!(policy.validate_similarity_threshold(0.7).is_ok());

        // Invalid threshold (out of range)
        assert!(policy.validate_similarity_threshold(1.5).is_err());

        // Invalid threshold (doesn't match policy)
        assert!(policy.validate_similarity_threshold(0.5).is_err());
    }

    #[test]
    fn test_validate_path_constraints() {
        let config = MploraConfig::default();
        let policy = MploraPolicy::new(config);

        let valid_paths = vec![
            MploraPath {
                path_id: 1,
                weight: 0.5,
                rank: 1,
                activations: vec![1.0, 0.0, 0.0],
                metadata: HashMap::new(),
            },
            MploraPath {
                path_id: 2,
                weight: 0.3,
                rank: 2,
                activations: vec![0.0, 1.0, 0.0],
                metadata: HashMap::new(),
            },
        ];

        assert!(policy.validate_path_constraints(&valid_paths).is_ok());

        // Too many paths
        let too_many_paths = vec![
            MploraPath {
                path_id: 1,
                weight: 0.5,
                rank: 1,
                activations: vec![1.0, 0.0, 0.0],
                metadata: HashMap::new(),
            };
            10 // Exceeds max_paths
        ];

        assert!(policy.validate_path_constraints(&too_many_paths).is_err());

        // Invalid weight
        let invalid_weight_paths = vec![MploraPath {
            path_id: 1,
            weight: 0.05, // Below min_path_weight
            rank: 1,
            activations: vec![1.0, 0.0, 0.0],
            metadata: HashMap::new(),
        }];

        assert!(policy
            .validate_path_constraints(&invalid_weight_paths)
            .is_err());
    }

    #[test]
    fn test_validate_performance_constraints() {
        let config = MploraConfig::default();
        let policy = MploraPolicy::new(config);

        let valid_metrics = MploraPerformanceMetrics {
            inference_time_ms: 50,
            memory_usage_mb: 512,
            cpu_usage_pct: 40.0,
            path_utilization: 0.8,
            compression_efficiency: 0.7,
        };

        assert!(policy
            .validate_performance_constraints(&valid_metrics)
            .is_ok());

        let invalid_metrics = MploraPerformanceMetrics {
            inference_time_ms: 150, // Exceeds max_inference_time_ms
            memory_usage_mb: 512,
            cpu_usage_pct: 40.0,
            path_utilization: 0.8,
            compression_efficiency: 0.7,
        };

        assert!(policy
            .validate_performance_constraints(&invalid_metrics)
            .is_err());
    }

    #[test]
    fn test_validate_orthogonal_constraints() {
        let mut config = MploraConfig::default();
        config.orthogonal_constraints = true;
        let policy = MploraPolicy::new(config);

        let orthogonal_paths = vec![
            MploraPath {
                path_id: 1,
                weight: 0.5,
                rank: 1,
                activations: vec![1.0, 0.0, 0.0],
                metadata: HashMap::new(),
            },
            MploraPath {
                path_id: 2,
                weight: 0.3,
                rank: 2,
                activations: vec![0.0, 1.0, 0.0],
                metadata: HashMap::new(),
            },
        ];

        assert!(policy
            .validate_orthogonal_constraints(&orthogonal_paths)
            .is_ok());

        let similar_paths = vec![
            MploraPath {
                path_id: 1,
                weight: 0.5,
                rank: 1,
                activations: vec![1.0, 0.0, 0.0],
                metadata: HashMap::new(),
            },
            MploraPath {
                path_id: 2,
                weight: 0.3,
                rank: 2,
                activations: vec![0.9, 0.1, 0.0], // Similar to first path
                metadata: HashMap::new(),
            },
        ];

        assert!(policy
            .validate_orthogonal_constraints(&similar_paths)
            .is_err());
    }

    #[test]
    fn test_select_paths() {
        let config = MploraConfig::default();
        let policy = MploraPolicy::new(config);

        let available_paths = vec![
            MploraPath {
                path_id: 1,
                weight: 0.8,
                rank: 1,
                activations: vec![1.0, 0.0, 0.0],
                metadata: HashMap::new(),
            },
            MploraPath {
                path_id: 2,
                weight: 0.6,
                rank: 2,
                activations: vec![0.0, 1.0, 0.0],
                metadata: HashMap::new(),
            },
            MploraPath {
                path_id: 3,
                weight: 0.4,
                rank: 3,
                activations: vec![0.0, 0.0, 1.0],
                metadata: HashMap::new(),
            },
        ];

        let selected = policy.select_paths(&available_paths, 2);
        assert!(selected.is_ok());
        assert_eq!(selected.unwrap().len(), 2);
    }

    #[test]
    fn test_calculate_compression_efficiency() {
        let config = MploraConfig::default();
        let policy = MploraPolicy::new(config);

        let efficiency = policy.calculate_compression_efficiency(1000, 800);
        assert_eq!(efficiency, 0.2); // 20% compression

        let efficiency_zero = policy.calculate_compression_efficiency(0, 100);
        assert_eq!(efficiency_zero, 0.0);
    }

    #[test]
    fn test_is_performance_monitoring_enabled() {
        let config = MploraConfig::default();
        let policy = MploraPolicy::new(config);

        assert!(policy.is_performance_monitoring_enabled());

        let mut config_disabled = MploraConfig::default();
        config_disabled.performance_constraints.enable_monitoring = false;
        let policy_disabled = MploraPolicy::new(config_disabled);

        assert!(!policy_disabled.is_performance_monitoring_enabled());
    }
}
