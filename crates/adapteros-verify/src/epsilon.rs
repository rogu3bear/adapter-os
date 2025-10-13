//! Epsilon (ε) statistics for floating-point verification
//!
//! This module tracks numerical stability and floating-point errors across layers.
//! It enables verification that new runs stay within acceptable error bounds.

use crate::{VerifyError, VerifyResult};
use adapteros_telemetry::event::KernelNoiseEvent;
use adapteros_telemetry::replay::ReplayBundle;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-layer epsilon statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpsilonStats {
    /// L2 norm of the error vector
    pub l2_error: f64,
    /// Maximum absolute error
    pub max_error: f64,
    /// Mean absolute error
    pub mean_error: f64,
    /// Number of elements in the layer
    pub element_count: usize,
}

impl EpsilonStats {
    /// Check if this stat is within tolerance of another
    pub fn within_tolerance(&self, other: &EpsilonStats, tolerance: f64) -> bool {
        let l2_diff = (self.l2_error - other.l2_error).abs();
        let max_diff = (self.max_error - other.max_error).abs();
        let mean_diff = (self.mean_error - other.mean_error).abs();
        
        l2_diff <= tolerance && max_diff <= tolerance && mean_diff <= tolerance
    }

    /// Compute relative error compared to another stat
    pub fn relative_error(&self, other: &EpsilonStats) -> f64 {
        let l2_rel = if other.l2_error.abs() > 1e-12 {
            (self.l2_error - other.l2_error).abs() / other.l2_error
        } else {
            (self.l2_error - other.l2_error).abs()
        };
        
        let max_rel = if other.max_error.abs() > 1e-12 {
            (self.max_error - other.max_error).abs() / other.max_error
        } else {
            (self.max_error - other.max_error).abs()
        };
        
        l2_rel.max(max_rel)
    }
}

/// Complete epsilon statistics for a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpsilonStatistics {
    /// Statistics per layer
    pub layer_stats: HashMap<String, EpsilonStats>,
}

impl EpsilonStatistics {
    /// Create empty statistics
    pub fn new() -> Self {
        Self {
            layer_stats: HashMap::new(),
        }
    }

    /// Extract epsilon statistics from a replay bundle
    pub fn from_replay_bundle(bundle: &ReplayBundle) -> VerifyResult<Self> {
        let mut layer_stats = HashMap::new();
        
        // Find kernel noise events in the bundle
        for event in &bundle.events {
            if event.event_type == "kernel.noise" {
                // Try to parse the payload as a KernelNoiseEvent
                if let Ok(noise_event) = serde_json::from_value::<KernelNoiseEvent>(event.payload.clone()) {
                    layer_stats.insert(
                        noise_event.layer_id.clone(),
                        EpsilonStats {
                            l2_error: noise_event.l2_error,
                            max_error: noise_event.max_error,
                            mean_error: noise_event.mean_error,
                            element_count: noise_event.element_count,
                        },
                    );
                }
            }
        }
        
        if layer_stats.is_empty() {
            // No kernel noise events found - create synthetic stats
            // This is acceptable for bundles that don't track noise
            tracing::warn!("No kernel.noise events found in bundle, using default stats");
        }
        
        Ok(Self { layer_stats })
    }

    /// Get maximum epsilon across all layers
    pub fn max_epsilon(&self) -> f64 {
        self.layer_stats
            .values()
            .map(|s| s.max_error)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.0)
    }

    /// Get mean epsilon across all layers
    pub fn mean_epsilon(&self) -> f64 {
        if self.layer_stats.is_empty() {
            return 0.0;
        }
        
        let sum: f64 = self.layer_stats.values().map(|s| s.mean_error).sum();
        sum / self.layer_stats.len() as f64
    }

    /// Compare with another epsilon statistics object
    pub fn compare(&self, other: &EpsilonStatistics, tolerance: f64) -> EpsilonComparison {
        let mut matching_layers = Vec::new();
        let mut divergent_layers = Vec::new();
        let mut missing_in_current = Vec::new();
        let mut missing_in_golden = Vec::new();
        
        // Check layers in golden
        for (layer_id, golden_stats) in &other.layer_stats {
            if let Some(current_stats) = self.layer_stats.get(layer_id) {
                if current_stats.within_tolerance(golden_stats, tolerance) {
                    matching_layers.push(layer_id.clone());
                } else {
                    let rel_error = current_stats.relative_error(golden_stats);
                    divergent_layers.push(LayerDivergence {
                        layer_id: layer_id.clone(),
                        golden: golden_stats.clone(),
                        current: current_stats.clone(),
                        relative_error: rel_error,
                    });
                }
            } else {
                missing_in_current.push(layer_id.clone());
            }
        }
        
        // Check for layers in current that aren't in golden
        for layer_id in self.layer_stats.keys() {
            if !other.layer_stats.contains_key(layer_id) {
                missing_in_golden.push(layer_id.clone());
            }
        }
        
        EpsilonComparison {
            matching_layers,
            divergent_layers,
            missing_in_current,
            missing_in_golden,
            tolerance,
        }
    }
}

impl Default for EpsilonStatistics {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of comparing epsilon statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpsilonComparison {
    /// Layers that match within tolerance
    pub matching_layers: Vec<String>,
    /// Layers that diverge beyond tolerance
    pub divergent_layers: Vec<LayerDivergence>,
    /// Layers in golden but not in current
    pub missing_in_current: Vec<String>,
    /// Layers in current but not in golden
    pub missing_in_golden: Vec<String>,
    /// Tolerance used for comparison
    pub tolerance: f64,
}

impl EpsilonComparison {
    /// Check if the comparison passed (no divergences)
    pub fn passed(&self) -> bool {
        self.divergent_layers.is_empty() 
            && self.missing_in_current.is_empty()
            && self.missing_in_golden.is_empty()
    }

    /// Get a summary string
    pub fn summary(&self) -> String {
        if self.passed() {
            format!("✓ All {} layers within tolerance (ε < {:.2e})", 
                self.matching_layers.len(), self.tolerance)
        } else {
            let mut parts = Vec::new();
            
            if !self.divergent_layers.is_empty() {
                parts.push(format!("{} divergent layers", self.divergent_layers.len()));
            }
            if !self.missing_in_current.is_empty() {
                parts.push(format!("{} missing in current", self.missing_in_current.len()));
            }
            if !self.missing_in_golden.is_empty() {
                parts.push(format!("{} missing in golden", self.missing_in_golden.len()));
            }
            
            format!("✗ Epsilon verification failed: {}", parts.join(", "))
        }
    }
}

/// Layer-specific divergence information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerDivergence {
    /// Layer identifier
    pub layer_id: String,
    /// Golden run statistics
    pub golden: EpsilonStats,
    /// Current run statistics
    pub current: EpsilonStats,
    /// Relative error
    pub relative_error: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epsilon_stats_within_tolerance() {
        let stats_a = EpsilonStats {
            l2_error: 1e-6,
            max_error: 5e-6,
            mean_error: 2e-6,
            element_count: 1000,
        };
        
        let stats_b = EpsilonStats {
            l2_error: 1.1e-6,
            max_error: 5.1e-6,
            mean_error: 2.1e-6,
            element_count: 1000,
        };
        
        assert!(stats_a.within_tolerance(&stats_b, 2e-7));
        assert!(!stats_a.within_tolerance(&stats_b, 5e-8));
    }

    #[test]
    fn test_epsilon_stats_relative_error() {
        let stats_a = EpsilonStats {
            l2_error: 1e-6,
            max_error: 5e-6,
            mean_error: 2e-6,
            element_count: 1000,
        };
        
        let stats_b = EpsilonStats {
            l2_error: 2e-6,
            max_error: 5e-6,
            mean_error: 2e-6,
            element_count: 1000,
        };
        
        let rel_error = stats_a.relative_error(&stats_b);
        assert!((rel_error - 0.5).abs() < 1e-9); // 1e-6 / 2e-6 = 0.5
    }

    #[test]
    fn test_epsilon_statistics_max() {
        let mut layer_stats = HashMap::new();
        layer_stats.insert(
            "layer_0".to_string(),
            EpsilonStats {
                l2_error: 1e-7,
                max_error: 5e-7,
                mean_error: 2e-7,
                element_count: 1000,
            },
        );
        layer_stats.insert(
            "layer_1".to_string(),
            EpsilonStats {
                l2_error: 1e-6,
                max_error: 8e-6,
                mean_error: 3e-6,
                element_count: 1000,
            },
        );
        
        let stats = EpsilonStatistics { layer_stats };
        assert!((stats.max_epsilon() - 8e-6).abs() < 1e-12);
    }

    #[test]
    fn test_epsilon_comparison_passed() {
        let mut layer_stats_a = HashMap::new();
        layer_stats_a.insert(
            "layer_0".to_string(),
            EpsilonStats {
                l2_error: 1e-6,
                max_error: 5e-6,
                mean_error: 2e-6,
                element_count: 1000,
            },
        );
        
        let mut layer_stats_b = HashMap::new();
        layer_stats_b.insert(
            "layer_0".to_string(),
            EpsilonStats {
                l2_error: 1.01e-6,
                max_error: 5.01e-6,
                mean_error: 2.01e-6,
                element_count: 1000,
            },
        );
        
        let stats_a = EpsilonStatistics { layer_stats: layer_stats_a };
        let stats_b = EpsilonStatistics { layer_stats: layer_stats_b };
        
        let comparison = stats_a.compare(&stats_b, 2e-8);
        assert!(comparison.passed());
        assert_eq!(comparison.matching_layers.len(), 1);
        assert_eq!(comparison.divergent_layers.len(), 0);
    }

    #[test]
    fn test_epsilon_comparison_divergent() {
        let mut layer_stats_a = HashMap::new();
        layer_stats_a.insert(
            "layer_0".to_string(),
            EpsilonStats {
                l2_error: 1e-6,
                max_error: 5e-6,
                mean_error: 2e-6,
                element_count: 1000,
            },
        );
        
        let mut layer_stats_b = HashMap::new();
        layer_stats_b.insert(
            "layer_0".to_string(),
            EpsilonStats {
                l2_error: 2e-6,
                max_error: 10e-6,
                mean_error: 4e-6,
                element_count: 1000,
            },
        );
        
        let stats_a = EpsilonStatistics { layer_stats: layer_stats_a };
        let stats_b = EpsilonStatistics { layer_stats: layer_stats_b };
        
        let comparison = stats_a.compare(&stats_b, 1e-7);
        assert!(!comparison.passed());
        assert_eq!(comparison.divergent_layers.len(), 1);
        assert_eq!(comparison.divergent_layers[0].layer_id, "layer_0");
    }
}

