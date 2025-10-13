//! Pluggable scoring functions for adapter routing

use super::{Decision, Router};
use smallvec::SmallVec;

/// Trait for pluggable scoring functions
pub trait ScoringFunction: Send + Sync {
    /// Get the name of this scoring algorithm
    fn name(&self) -> &str;

    /// Score adapters and return top-K decision
    ///
    /// # Arguments
    /// * `features` - Feature vector (21-dimensional for code routing)
    /// * `priors` - Prior scores for each adapter
    /// * `k` - Number of adapters to select
    /// * `tau` - Temperature for softmax
    /// * `eps` - Entropy floor
    ///
    /// # Returns
    /// Decision with selected adapter indices and Q15 gates
    fn score(&mut self, features: &[f32], priors: &[f32], k: usize, tau: f32, eps: f32)
        -> Decision;
}

/// Default weighted scorer using existing Router logic
pub struct WeightedScorer {
    router: Router,
}

impl WeightedScorer {
    pub fn new(router: Router) -> Self {
        Self { router }
    }
}

impl ScoringFunction for WeightedScorer {
    fn name(&self) -> &str {
        "weighted"
    }

    fn score(
        &mut self,
        features: &[f32],
        priors: &[f32],
        _k: usize,
        _tau: f32,
        _eps: f32,
    ) -> Decision {
        // Use the existing router logic
        self.router.route(features, priors)
    }
}

/// Entropy floor scorer - emphasizes uniform distribution
pub struct EntropyFloorScorer;

impl EntropyFloorScorer {
    pub fn new(_k: usize, _eps: f32) -> Self {
        Self
    }
}

impl ScoringFunction for EntropyFloorScorer {
    fn name(&self) -> &str {
        "entropy_floor"
    }

    fn score(
        &mut self,
        _features: &[f32],
        priors: &[f32],
        k: usize,
        tau: f32,
        eps: f32,
    ) -> Decision {
        // Select top K by priors
        let mut scores: Vec<(usize, f32)> = priors
            .iter()
            .enumerate()
            .map(|(i, &prior)| (i, prior))
            .collect();

        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let top_k: Vec<(usize, f32)> = scores.into_iter().take(k).collect();

        // Apply strong entropy floor - more uniform distribution
        let min_gate = (eps * 2.0) / k as f32; // Double the entropy floor
        let mut gates: Vec<f32> = vec![min_gate; k];

        // Add small variation based on scores
        let max_score = top_k
            .iter()
            .map(|(_, s)| s)
            .fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        for (i, (_, score)) in top_k.iter().enumerate() {
            let variation = ((score - max_score) / tau).exp() * 0.1; // Small variation
            gates[i] += variation;
        }

        // Renormalize
        let sum_gates: f32 = gates.iter().sum();
        for g in &mut gates {
            *g /= sum_gates;
        }

        // Quantize to Q15
        let gates_q15: SmallVec<[i16; 8]> = gates
            .iter()
            .map(|&g| {
                let q = (g * 32767.0).round() as i16;
                q.max(0)
            })
            .collect();

        let indices: SmallVec<[u16; 8]> = top_k.iter().map(|(i, _)| *i as u16).collect();

        Decision { indices, gates_q15 }
    }
}

/// Create a scoring function from algorithm name
pub fn create_scorer(algorithm: &str, router: Router) -> Box<dyn ScoringFunction> {
    match algorithm {
        "weighted" => Box::new(WeightedScorer::new(router)),
        "entropy_floor" => {
            let k = 3; // Default K
            let eps = 0.02; // Default entropy floor
            Box::new(EntropyFloorScorer::new(k, eps))
        }
        _ if algorithm.starts_with("plugin:") => {
            // For plugin scorers, we would load them dynamically
            // For now, fall back to weighted
            tracing::warn!(
                algorithm,
                "Unknown plugin algorithm, falling back to weighted"
            );
            Box::new(WeightedScorer::new(router))
        }
        _ => {
            tracing::warn!(algorithm, "Unknown algorithm, falling back to weighted");
            Box::new(WeightedScorer::new(router))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RouterWeights;

    #[test]
    fn test_entropy_floor_scorer() {
        let mut scorer = EntropyFloorScorer::new(3, 0.02);

        let features = vec![0.0; 21];
        let priors = vec![0.9, 0.5, 0.3, 0.1, 0.0];

        let decision = scorer.score(&features, &priors, 3, 1.0, 0.02);

        assert_eq!(decision.indices.len(), 3);
        assert_eq!(decision.gates_q15.len(), 3);

        // Gates should be more uniform due to strong entropy floor
        let gates_f32 = decision.gates_f32();
        let max_gate = gates_f32.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let min_gate = gates_f32.iter().fold(f32::INFINITY, |a, &b| a.min(b));

        // Difference should be small (more uniform)
        assert!((max_gate - min_gate) < 0.5);
    }

    #[test]
    fn test_create_scorer() {
        let weights = RouterWeights::default();
        let router = Router::new_with_weights(weights, 3, 1.0, 0.02);

        let scorer = create_scorer("weighted", router);
        assert_eq!(scorer.name(), "weighted");

        let weights2 = RouterWeights::default();
        let router2 = Router::new_with_weights(weights2, 3, 1.0, 0.02);
        let scorer2 = create_scorer("entropy_floor", router2);
        assert_eq!(scorer2.name(), "entropy_floor");
    }
}
