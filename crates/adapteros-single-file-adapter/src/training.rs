//! Training types for .aos format
//!
//! These are simple DTOs that are also defined in adapteros-lora-worker.
//! We define them here to avoid circular dependencies.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Single training example
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingExample {
    /// Input token IDs
    pub input: Vec<u32>,
    /// Target token IDs
    pub target: Vec<u32>,
    /// Example metadata
    pub metadata: HashMap<String, String>,
    /// Sample weight (positive reinforces, negative penalizes)
    #[serde(default = "default_sample_weight")]
    pub weight: f32,
}

fn default_sample_weight() -> f32 {
    1.0
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
        }
    }
}
