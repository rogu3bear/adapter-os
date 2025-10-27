//! Orthogonal constraint enforcement for MPLoRA
//!
//! Implements orthogonal multi-path LoRA routing as described in:
//! MPLoRA: Orthogonal Multi-Path Low-Rank Adaptation for Parameter Efficient Fine-Tuning
//! https://openreview.net/pdf?id=jqz6Msm3AF

use adapteros_core::Result;
use std::collections::VecDeque;

/// Orthogonal constraint tracker
#[derive(Debug, Clone)]
pub struct OrthogonalConstraints {
    /// Adapter activation history for similarity computation
    activation_history: VecDeque<Vec<f32>>,
    /// Similarity threshold for orthogonal enforcement
    similarity_threshold: f32,
    /// Penalty weight for similar activations
    penalty_weight: f32,
    /// History window size
    history_window: usize,
}

impl OrthogonalConstraints {
    /// Create new orthogonal constraints tracker
    pub fn new(similarity_threshold: f32, penalty_weight: f32, history_window: usize) -> Self {
        Self {
            activation_history: VecDeque::new(),
            similarity_threshold,
            penalty_weight,
            history_window,
        }
    }

    /// Compute orthogonal penalty for adapter selection
    pub fn compute_penalty(&self, adapter_indices: &[u16], gates: &[i16]) -> f32 {
        if self.activation_history.is_empty() {
            return 0.0;
        }

        let mut total_penalty = 0.0;
        let current_activation = self.gates_to_activation_vector(adapter_indices, gates);

        for historical_activation in &self.activation_history {
            let similarity =
                self.compute_cosine_similarity(&current_activation, historical_activation);
            if similarity > self.similarity_threshold {
                total_penalty += self.penalty_weight * similarity;
            }
        }

        total_penalty
    }

    /// Update activation history
    pub fn update_history(&mut self, adapter_indices: &[u16], gates: &[i16]) {
        let activation = self.gates_to_activation_vector(adapter_indices, gates);
        self.activation_history.push_back(activation);

        // Maintain history window
        while self.activation_history.len() > self.history_window {
            self.activation_history.pop_front();
        }
    }

    /// Convert Q15 gates to normalized activation vector
    fn gates_to_activation_vector(&self, adapter_indices: &[u16], gates: &[i16]) -> Vec<f32> {
        // Convert Q15 gates to normalized activation vector
        let max_index = adapter_indices.iter().copied().map(|v| v as usize).max().unwrap_or(0);
        let mut activation = vec![0.0; max_index + 1];

        for (idx, gate) in adapter_indices.iter().zip(gates.iter()) {
            let value = *gate as f32 / 32767.0;
            activation[*idx as usize] = (value * 10_000.0).round() / 10_000.0;
        }

        activation
    }

    /// Compute cosine similarity between two activation vectors
    fn compute_cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        // Compare over common length; treat missing tail as zeros
        let n = core::cmp::min(a.len(), b.len());
        if n == 0 {
            return 0.0;
        }
        let dot_product: f32 = a.iter().take(n).zip(b.iter().take(n)).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().take(n).map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().take(n).map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot_product / (norm_a * norm_b)
        }
    }

    /// Get current diversity score
    pub fn diversity_score(&self) -> f32 {
        if self.activation_history.len() < 2 {
            return 1.0; // Maximum diversity for insufficient history
        }

        let mut total_similarity = 0.0;
        let mut comparisons = 0;

        for i in 0..self.activation_history.len() {
            for j in (i + 1)..self.activation_history.len() {
                let similarity = self.compute_cosine_similarity(
                    &self.activation_history[i],
                    &self.activation_history[j],
                );
                total_similarity += similarity;
                comparisons += 1;
            }
        }

        if comparisons == 0 {
            1.0
        } else {
            1.0 - (total_similarity / comparisons as f32)
        }
    }

    /// Check if orthogonal constraints are satisfied
    pub fn check_constraints(&self, adapter_indices: &[u16], gates: &[i16]) -> Result<()> {
        let penalty = self.compute_penalty(adapter_indices, gates);

        if penalty > 0.5 {
            return Err(adapteros_core::AosError::Policy(format!(
                "Orthogonal constraint violation: penalty {} exceeds threshold 0.5",
                penalty
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orthogonal_constraints_creation() {
        let constraints = OrthogonalConstraints::new(0.7, 0.1, 10);
        assert_eq!(constraints.similarity_threshold, 0.7);
        assert_eq!(constraints.penalty_weight, 0.1);
        assert_eq!(constraints.history_window, 10);
    }

    #[test]
    fn test_empty_history_penalty() {
        let constraints = OrthogonalConstraints::new(0.7, 0.1, 10);
        let penalty = constraints.compute_penalty(&[0, 1], &[16383, 16383]);
        assert_eq!(penalty, 0.0);
    }

    #[test]
    fn test_activation_vector_conversion() {
        let constraints = OrthogonalConstraints::new(0.7, 0.1, 10);
        let activation = constraints.gates_to_activation_vector(&[0, 1], &[16383, 16383]);

        assert_eq!(activation[0], 0.5); // 16383 / 32767 ≈ 0.5
        assert_eq!(activation[1], 0.5);
        assert_eq!(activation[2], 0.0);
    }

    #[test]
    fn test_cosine_similarity() {
        let constraints = OrthogonalConstraints::new(0.7, 0.1, 10);

        // Identical vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(constraints.compute_cosine_similarity(&a, &b), 1.0);

        // Orthogonal vectors
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert_eq!(constraints.compute_cosine_similarity(&a, &b), 0.0);

        // Zero vectors
        let a = vec![0.0, 0.0];
        let b = vec![0.0, 0.0];
        assert_eq!(constraints.compute_cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_history_update() {
        let mut constraints = OrthogonalConstraints::new(0.7, 0.1, 3);

        // Add activations
        constraints.update_history(&[0, 1], &[16383, 16383]);
        constraints.update_history(&[2, 3], &[16383, 16383]);
        constraints.update_history(&[4, 5], &[16383, 16383]);
        constraints.update_history(&[6, 7], &[16383, 16383]); // Should remove first

        assert_eq!(constraints.activation_history.len(), 3);
    }

    #[test]
    fn test_diversity_score() {
        let mut constraints = OrthogonalConstraints::new(0.7, 0.1, 10);

        // Empty history
        assert_eq!(constraints.diversity_score(), 1.0);

        // Single activation
        constraints.update_history(&[0, 1], &[16383, 16383]);
        assert_eq!(constraints.diversity_score(), 1.0);

        // Identical activations (low diversity)
        constraints.update_history(&[0, 1], &[16383, 16383]);
        let score = constraints.diversity_score();
        assert!(score < 1.0);

        // Different activations (high diversity)
        constraints.update_history(&[2, 3], &[16383, 16383]);
        let score = constraints.diversity_score();
        assert!(score > 0.0);
    }
}
