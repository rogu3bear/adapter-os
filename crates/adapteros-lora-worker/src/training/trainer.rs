//! Micro-LoRA training loop with forward/backward pass
//!
//! Implements LoRA training with low rank adaptation matrices.

use super::dataset::TrainingExample;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info};

/// Micro-LoRA trainer
#[derive(Debug)]
pub struct MicroLoRATrainer {
    config: TrainingConfig,
}

/// Training configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Training result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingResult {
    pub adapter_id: String,
    pub final_loss: f32,
    pub training_time_ms: u64,
    pub weights: LoRAWeights,
}

/// LoRA weight matrices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAWeights {
    /// Down-projection matrix (rank × hidden_dim)
    pub lora_a: Vec<Vec<f32>>,
    /// Up-projection matrix (hidden_dim × rank)
    pub lora_b: Vec<Vec<f32>>,
}

impl MicroLoRATrainer {
    /// Create a new trainer with configuration
    pub fn new(config: TrainingConfig) -> Self {
        Self { config }
    }

    /// Train LoRA adapter on examples
    pub async fn train(&self, examples: &[TrainingExample]) -> Result<TrainingResult> {
        info!(
            "Starting LoRA training: rank={}, epochs={}, examples={}",
            self.config.rank,
            self.config.epochs,
            examples.len()
        );

        let start = Instant::now();
        let adapter_id = Self::generate_adapter_id();

        // Initialize LoRA weights
        let mut weights = self.initialize_weights();

        // Training loop
        let mut final_loss = 0.0;
        for epoch in 0..self.config.epochs {
            debug!("Epoch {}/{}", epoch + 1, self.config.epochs);

            let epoch_loss = self.train_epoch(&mut weights, examples)?;
            final_loss = epoch_loss;

            info!("Epoch {} loss: {:.4}", epoch + 1, epoch_loss);

            // Early stopping if loss is very low
            if epoch_loss < 0.01 {
                info!("Early stopping: loss below threshold");
                break;
            }
        }

        let training_time_ms = start.elapsed().as_millis() as u64;

        info!(
            "Training complete: loss={:.4}, time={}ms",
            final_loss, training_time_ms
        );

        Ok(TrainingResult {
            adapter_id,
            final_loss,
            training_time_ms,
            weights,
        })
    }

    /// Initialize LoRA weight matrices
    fn initialize_weights(&self) -> LoRAWeights {
        // Initialize lora_a with small random values
        let lora_a = (0..self.config.rank)
            .map(|_| {
                (0..self.config.hidden_dim)
                    .map(|_| rand::random::<f32>() * 0.01)
                    .collect()
            })
            .collect();

        // Initialize lora_b with zeros (standard practice)
        let lora_b = (0..self.config.hidden_dim)
            .map(|_| vec![0.0; self.config.rank])
            .collect();

        LoRAWeights { lora_a, lora_b }
    }

    /// Train one epoch
    fn train_epoch(&self, weights: &mut LoRAWeights, examples: &[TrainingExample]) -> Result<f32> {
        let mut total_loss = 0.0;
        let mut num_batches = 0;

        // Process examples in batches
        for batch_start in (0..examples.len()).step_by(self.config.batch_size) {
            let batch_end = (batch_start + self.config.batch_size).min(examples.len());
            let batch = &examples[batch_start..batch_end];

            let loss = self.train_batch(weights, batch)?;
            total_loss += loss;
            num_batches += 1;
        }

        Ok(total_loss / num_batches as f32)
    }

    /// Train one batch
    fn train_batch(&self, weights: &mut LoRAWeights, batch: &[TrainingExample]) -> Result<f32> {
        let mut batch_loss = 0.0;

        for example in batch {
            // Forward pass
            let (output, hidden) = self.forward(weights, &example.input)?;

            // Compute loss (simplified cross-entropy)
            let loss = self.compute_loss(&output, &example.target);
            batch_loss += loss;

            // Backward pass and update weights
            self.backward_and_update(weights, &hidden, &output, &example.target, loss)?;
        }

        Ok(batch_loss / batch.len() as f32)
    }

    /// Forward pass with LoRA injection
    fn forward(&self, weights: &LoRAWeights, input: &[u32]) -> Result<(Vec<f32>, Vec<f32>)> {
        // Simplified forward pass
        // In production, this would integrate with the actual model

        // Create hidden state from input (simplified embedding)
        let hidden: Vec<f32> = input
            .iter()
            .take(self.config.hidden_dim)
            .map(|&token_id| (token_id as f32) / 1000.0)
            .collect();

        // Pad to hidden_dim if needed
        let mut hidden = hidden;
        while hidden.len() < self.config.hidden_dim {
            hidden.push(0.0);
        }

        // Apply LoRA: output = hidden + hidden * LoRA_B * LoRA_A
        let lora_output = self.apply_lora(&hidden, weights);

        // Combine base hidden with LoRA adjustment
        let output: Vec<f32> = hidden
            .iter()
            .zip(lora_output.iter())
            .map(|(h, l)| h + l * self.config.alpha / self.config.rank as f32)
            .collect();

        Ok((output, hidden))
    }

    /// Apply LoRA transformation
    fn apply_lora(&self, hidden: &[f32], weights: &LoRAWeights) -> Vec<f32> {
        // Compute: hidden * LoRA_A^T * LoRA_B^T

        // First: hidden * LoRA_A^T = intermediate (size: rank)
        let mut intermediate = vec![0.0; self.config.rank];
        for r in 0..self.config.rank {
            for (h_idx, &h_val) in hidden.iter().enumerate() {
                if h_idx < weights.lora_a[r].len() {
                    intermediate[r] += h_val * weights.lora_a[r][h_idx];
                }
            }
        }

        // Second: intermediate * LoRA_B^T = output (size: hidden_dim)
        let mut output = vec![0.0; self.config.hidden_dim];
        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for (r, &inter_val) in intermediate.iter().enumerate() {
                    if r < weights.lora_b[h_idx].len() {
                        output[h_idx] += inter_val * weights.lora_b[h_idx][r];
                    }
                }
            }
        }

        output
    }

    /// Compute loss (simplified cross-entropy)
    fn compute_loss(&self, output: &[f32], target: &[u32]) -> f32 {
        let mut loss = 0.0;
        let n = output.len().min(target.len());

        for i in 0..n {
            let target_val = (target[i] as f32) / 1000.0;
            let diff = output[i] - target_val;
            loss += diff * diff; // MSE for simplicity
        }

        loss / n as f32
    }

    /// Backward pass and weight update
    fn backward_and_update(
        &self,
        weights: &mut LoRAWeights,
        hidden: &[f32],
        output: &[f32],
        target: &[u32],
        loss: f32,
    ) -> Result<()> {
        // Simplified gradient descent
        // In production, use proper backpropagation

        let n = output.len().min(target.len());
        let learning_rate = self.config.learning_rate;

        // Compute gradient (simplified)
        let mut grad_output = vec![0.0; output.len()];
        for i in 0..n {
            let target_val = (target[i] as f32) / 1000.0;
            grad_output[i] = 2.0 * (output[i] - target_val) / n as f32;
        }

        // Update LoRA_A
        for r in 0..self.config.rank {
            for h_idx in 0..self.config.hidden_dim.min(hidden.len()) {
                if h_idx < weights.lora_a[r].len() {
                    let grad = grad_output[h_idx] * hidden[h_idx] * loss;
                    weights.lora_a[r][h_idx] -= learning_rate * grad;
                }
            }
        }

        // Update LoRA_B
        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for r in 0..self.config.rank {
                    if r < weights.lora_b[h_idx].len() {
                        let grad = grad_output[h_idx] * hidden[h_idx] * loss;
                        weights.lora_b[h_idx][r] -= learning_rate * grad;
                    }
                }
            }
        }

        Ok(())
    }

    /// Generate unique adapter ID
    fn generate_adapter_id() -> String {
        use std::time::SystemTime;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("microlora_{}", timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_initialize_weights() {
        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 768,
            ..Default::default()
        };
        let trainer = MicroLoRATrainer::new(config);
        let weights = trainer.initialize_weights();

        assert_eq!(weights.lora_a.len(), 4);
        assert_eq!(weights.lora_a[0].len(), 768);
        assert_eq!(weights.lora_b.len(), 768);
        assert_eq!(weights.lora_b[0].len(), 4);
    }

    #[test]
    fn test_forward_pass() {
        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 768,
            ..Default::default()
        };
        let trainer = MicroLoRATrainer::new(config);
        let weights = trainer.initialize_weights();

        let input = vec![1, 2, 3, 4, 5];
        let (output, hidden) = trainer.forward(&weights, &input).unwrap();

        assert_eq!(output.len(), 768);
        assert_eq!(hidden.len(), 768);
    }

    #[tokio::test]
    async fn test_train_small() {
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 64,
            batch_size: 2,
            epochs: 1,
            learning_rate: 0.01,
            ..Default::default()
        };
        let trainer = MicroLoRATrainer::new(config);

        let examples = vec![
            TrainingExample {
                input: vec![1, 2, 3],
                target: vec![4, 5, 6],
                metadata: HashMap::new(),
            },
            TrainingExample {
                input: vec![7, 8, 9],
                target: vec![10, 11, 12],
                metadata: HashMap::new(),
            },
        ];

        let result = trainer.train(&examples).await.unwrap();
        assert!(result.final_loss >= 0.0);
        assert!(result.training_time_ms > 0);
        assert_eq!(result.weights.lora_a.len(), 2);
    }
}
