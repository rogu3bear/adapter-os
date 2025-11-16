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

        // Compute entropy
        let entropy: f32 = gates
            .iter()
            .filter(|&&g| g > 0.0)
            .map(|&g| -g * g.log2())
            .sum();

        // Build candidates with raw scores
        let candidates: Vec<super::DecisionCandidate> = top_k
            .iter()
            .zip(gates.iter())
            .zip(gates_q15.iter())
            .map(
                |(((idx, score), _gate_f32), gate_q15)| super::DecisionCandidate {
                    adapter_idx: *idx as u16,
                    raw_score: *score,
                    gate_q15: *gate_q15,
                },
            )
            .collect();

        Decision {
            indices,
            gates_q15,
            entropy,
            candidates,
        }
    }
}

/// Adapter-aware scorer: incorporates simple per-adapter feature interactions
/// using framework match and a light language compatibility heuristic.
pub struct AdapterAwareScorer {
    /// Optional mapping of adapter index -> framework id
    adapter_frameworks: Vec<Option<String>>,
    /// Language index with strongest signal (0..7) from features
    dominant_lang: Option<usize>,
}

impl AdapterAwareScorer {
    pub fn new(adapter_frameworks: Vec<Option<String>>, features: &[f32]) -> Self {
        // Determine dominant language from first 8 dims
        let dominant_lang = if features.len() >= 8 {
            let (idx, _) = features
                .iter()
                .take(8)
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or((0, &0.0));
            Some(idx)
        } else {
            None
        };
        Self {
            adapter_frameworks,
            dominant_lang,
        }
    }
}

impl ScoringFunction for AdapterAwareScorer {
    fn name(&self) -> &str {
        "adapter_aware"
    }

    fn score(
        &mut self,
        features: &[f32],
        priors: &[f32],
        k: usize,
        tau: f32,
        eps: f32,
    ) -> Decision {
        // Base: priors
        let mut scores: Vec<(usize, f32)> =
            priors.iter().enumerate().map(|(i, &p)| (i, p)).collect();

        // Framework contributions from features[8..11]
        let fw_signals = if features.len() >= 11 {
            &features[8..11]
        } else {
            &[][..]
        };

        // Apply simple framework and language boosts
        for s in scores.iter_mut() {
            // Framework: if adapter has a framework id, lightly boost by top signal
            if let Some(Some(_fw)) = self.adapter_frameworks.get(s.0) {
                if !fw_signals.is_empty() {
                    s.1 += fw_signals.iter().copied().fold(0.0, f32::max) * 0.1;
                }
            }
            // Language: apply small bump if dominant language exists
            if self.dominant_lang.is_some() {
                s.1 += 0.05;
            }
        }

        // Sort by score desc, index for stability
        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        // Top-k softmax with entropy floor
        let top_k: Vec<(usize, f32)> = scores.into_iter().take(k).collect();
        let max_score = top_k
            .iter()
            .map(|(_, s)| s)
            .fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let exp_scores: Vec<f32> = top_k
            .iter()
            .map(|(_, s)| ((s - max_score) / tau).exp())
            .collect();
        let sum_exp: f32 = exp_scores.iter().sum();
        let mut gates: Vec<f32> = exp_scores.iter().map(|e| e / sum_exp).collect();
        let min_gate = eps / k as f32;
        for g in &mut gates {
            *g = g.max(min_gate);
        }
        let sum_g: f32 = gates.iter().sum();
        for g in &mut gates {
            *g /= sum_g;
        }
        let gates_q15: SmallVec<[i16; 8]> = gates
            .iter()
            .map(|&g| (g * 32767.0).round() as i16)
            .collect();
        let indices: SmallVec<[u16; 8]> = top_k.iter().map(|(i, _)| *i as u16).collect();

        // Calculate Shannon entropy from gate distribution
        let entropy: f32 = gates
            .iter()
            .map(|&p| if p > 0.0 { -p * p.log2() } else { 0.0 })
            .sum();

        // Create candidates from top_k results
        let candidates: Vec<crate::DecisionCandidate> = top_k
            .iter()
            .zip(gates_q15.iter())
            .map(|((idx, raw_score), &gate_q15)| crate::DecisionCandidate {
                adapter_idx: *idx as u16,
                raw_score: *raw_score,
                gate_q15,
            })
            .collect();

        Decision {
            indices,
            gates_q15,
            entropy,
            candidates,
        }
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
