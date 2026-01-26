//! Training types for .aos format
//!
//! These are simple DTOs that are also defined in adapteros-lora-worker.
//! We define them here to avoid circular dependencies.

use serde::{Deserialize, Serialize};

pub type TrainingExample = adapteros_types::training::TrainingExampleV1;

/// Weight group combination settings for separated training
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WeightGroupConfig {
    /// Whether to combine weight groups by summing them. If false, they are concatenated.
    #[serde(default)]
    pub sum_weight_groups: bool,
}

/// Training configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingConfig {
    /// LoRA rank
    pub rank: usize,
    /// LoRA alpha scaling factor
    pub alpha: f32,
    /// Learning rate
    pub learning_rate: f32,
    /// Batch size
    pub batch_size: usize,
    /// Number of epochs
    pub epochs: usize,
    /// Hidden dimension size
    pub hidden_dim: usize,
    /// Weight group combination settings for separated training
    #[serde(default = "default_weight_group_config")]
    pub weight_group_config: WeightGroupConfig,
}

fn default_weight_group_config() -> WeightGroupConfig {
    WeightGroupConfig::default()
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            rank: 4,
            alpha: 16.0,
            learning_rate: 1e-4,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
            weight_group_config: WeightGroupConfig::default(),
        }
    }
}
