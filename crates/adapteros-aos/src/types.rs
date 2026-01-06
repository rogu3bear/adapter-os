//! Shared AOS archive types.

use serde::{Deserialize, Serialize};

/// Strategy for combining positive and negative weight groups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CombinationStrategy {
    /// Positive - negative weights.
    Difference,
    /// Weighted positive/negative difference.
    WeightedDifference {
        positive_scale: f32,
        negative_scale: f32,
    },
    /// Keep positive weights only.
    Separate,
}

impl Default for CombinationStrategy {
    fn default() -> Self {
        Self::WeightedDifference {
            positive_scale: 1.0,
            negative_scale: 1.0,
        }
    }
}

/// Configuration for separated weight groups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WeightGroupConfig {
    pub use_separate_weights: bool,
    pub combination_strategy: CombinationStrategy,
}

impl Default for WeightGroupConfig {
    fn default() -> Self {
        Self {
            use_separate_weights: true,
            combination_strategy: CombinationStrategy::default(),
        }
    }
}
