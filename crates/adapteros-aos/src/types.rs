//! Common types for the AOS format
//!
//! These types are shared between the aos crate and single-file-adapter for
//! backward compatibility during migration.

use serde::{Deserialize, Serialize};

/// Configuration for weight groups
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeightGroupConfig {
    /// Whether to use separate positive/negative weights
    pub use_separate_weights: bool,
    /// Weight combination strategy for inference
    pub combination_strategy: CombinationStrategy,
}

/// Strategy for combining positive and negative weights
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CombinationStrategy {
    /// Simple difference: combined = positive - negative
    Difference,
    /// Weighted difference: combined = (positive * pos_scale) - (negative * neg_scale)
    WeightedDifference {
        /// Scaling factor applied to positive weights
        positive_scale: f32,
        /// Scaling factor applied to negative weights
        negative_scale: f32,
    },
    /// Separate inference: use positive and negative weights independently
    Separate,
}

impl Default for WeightGroupConfig {
    fn default() -> Self {
        Self {
            use_separate_weights: true,
            combination_strategy: CombinationStrategy::WeightedDifference {
                positive_scale: 1.0,
                negative_scale: 1.0,
            },
        }
    }
}
