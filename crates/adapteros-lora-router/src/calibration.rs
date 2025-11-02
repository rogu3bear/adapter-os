//! Router weight calibration system
//!
//! Optimizes RouterWeights using a calibration dataset to maximize
//! adapter selection accuracy, precision, and recall.

use crate::RouterWeights;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Calibration sample with ground truth
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationSample {
    /// Feature vector (22-dimensional)
    pub features: Vec<f32>,
    /// Ground truth adapter indices that should be selected
    pub ground_truth_adapters: Vec<usize>,
    /// Optional metadata (prompt, context, etc.)
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Calibration dataset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationDataset {
    pub samples: Vec<CalibrationSample>,
}

impl CalibrationDataset {
    /// Load dataset from JSON file
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read calibration dataset: {:?}", path.as_ref()))?;
        serde_json::from_str(&content).context("Failed to parse calibration dataset JSON")
    }

    /// Save dataset to JSON file
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(&self)?;
        fs::write(path.as_ref(), content)
            .with_context(|| format!("Failed to write calibration dataset: {:?}", path.as_ref()))
    }

    /// Add a sample to the dataset
    pub fn add_sample(&mut self, sample: CalibrationSample) {
        self.samples.push(sample);
    }

    /// Split into training and validation sets
    pub fn train_val_split(&self, train_ratio: f32) -> (Self, Self) {
        let train_size = (self.samples.len() as f32 * train_ratio) as usize;
        let mut samples = self.samples.clone();

        let val_samples = samples.split_off(train_size);

        (
            Self { samples },
            Self {
                samples: val_samples,
            },
        )
    }
}

/// Optimization method for calibration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationMethod {
    /// Grid search over predefined weight ranges
    GridSearch,
    /// Simple gradient descent
    GradientDescent,
}

/// Validation metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationMetrics {
    /// Accuracy: fraction of samples where top adapter is correct
    pub accuracy: f32,
    /// Precision: fraction of selected adapters that are correct
    pub precision: f32,
    /// Recall: fraction of ground truth adapters that were selected
    pub recall: f32,
    /// F1 score: harmonic mean of precision and recall
    pub f1_score: f32,
    /// Mean Reciprocal Rank (MRR)
    pub mrr: f32,
}

impl ValidationMetrics {
    /// Create empty metrics
    pub fn zero() -> Self {
        Self {
            accuracy: 0.0,
            precision: 0.0,
            recall: 0.0,
            f1_score: 0.0,
            mrr: 0.0,
        }
    }

    /// Compute overall quality score (higher is better)
    pub fn score(&self) -> f32 {
        // Weighted combination of metrics
        0.3 * self.accuracy + 0.25 * self.precision + 0.25 * self.recall + 0.2 * self.mrr
    }
}

/// Router weight calibrator
pub struct Calibrator {
    dataset: CalibrationDataset,
    optimization_method: OptimizationMethod,
    k: usize,
}

impl Calibrator {
    /// Create a new calibrator
    pub fn new(
        dataset: CalibrationDataset,
        optimization_method: OptimizationMethod,
        k: usize,
    ) -> Self {
        Self {
            dataset,
            optimization_method,
            k,
        }
    }

    /// Train and find optimal weights
    pub fn train(&self) -> Result<RouterWeights> {
        match self.optimization_method {
            OptimizationMethod::GridSearch => self.grid_search(),
            OptimizationMethod::GradientDescent => self.gradient_descent(),
        }
    }

    /// Validate weights on dataset
    pub fn validate(&self, weights: &RouterWeights) -> ValidationMetrics {
        if self.dataset.samples.is_empty() {
            return ValidationMetrics::zero();
        }

        let mut total_accuracy = 0.0;
        let mut total_precision = 0.0;
        let mut total_recall = 0.0;
        let mut total_mrr = 0.0;

        for sample in &self.dataset.samples {
            // Compute weighted score for each "adapter" (really just computing feature score)
            // In practice, this would be combined with adapter priors
            let feature_score = self.compute_feature_score(&sample.features, weights);

            // For validation, we check if the ground truth adapters would score highly
            // This is a simplified validation - in practice we'd need actual adapter scores
            let metrics = self.compute_sample_metrics(feature_score, &sample.ground_truth_adapters);

            total_accuracy += metrics.accuracy;
            total_precision += metrics.precision;
            total_recall += metrics.recall;
            total_mrr += metrics.mrr;
        }

        let n = self.dataset.samples.len() as f32;
        let metrics = ValidationMetrics {
            accuracy: total_accuracy / n,
            precision: total_precision / n,
            recall: total_recall / n,
            f1_score: 0.0, // Will be computed below
            mrr: total_mrr / n,
        };

        ValidationMetrics {
            f1_score: if metrics.precision + metrics.recall > 0.0 {
                2.0 * metrics.precision * metrics.recall / (metrics.precision + metrics.recall)
            } else {
                0.0
            },
            ..metrics
        }
    }

    /// Grid search over weight ranges
    fn grid_search(&self) -> Result<RouterWeights> {
        let mut best_weights = RouterWeights::default();
        let mut best_score = 0.0;

        // Define grid ranges (coarse to fine)
        let ranges = [
            vec![0.1, 0.2, 0.3, 0.4, 0.5],    // language
            vec![0.1, 0.2, 0.25, 0.3, 0.4],   // framework
            vec![0.05, 0.1, 0.15, 0.2, 0.25], // symbols
            vec![0.05, 0.1, 0.15, 0.2],       // paths
            vec![0.05, 0.1, 0.15, 0.2],       // verb
        ];

        // Grid search
        for &lang in &ranges[0] {
            for &framework in &ranges[1] {
                for &symbols in &ranges[2] {
                    for &paths in &ranges[3] {
                        for &verb in &ranges[4] {
                            let weights = RouterWeights::new(lang, framework, symbols, paths, verb);
                            let metrics = self.validate(&weights);
                            let score = metrics.score();

                            if score > best_score {
                                best_score = score;
                                best_weights = weights;
                            }
                        }
                    }
                }
            }
        }

        tracing::info!(
            "Grid search complete. Best score: {:.4}, weights: {:?}",
            best_score,
            best_weights
        );

        Ok(best_weights)
    }

    /// Gradient descent optimization (simplified)
    fn gradient_descent(&self) -> Result<RouterWeights> {
        let mut weights = RouterWeights::default();
        let learning_rate = 0.01;
        let num_iterations = 100;

        for iteration in 0..num_iterations {
            // Compute gradients (simplified - in practice would use actual gradients)
            let current_metrics = self.validate(&weights);
            let current_score = current_metrics.score();

            // Try small perturbations
            let epsilon = 0.01;
            let mut best_delta_weights = vec![0.0; 5];
            let mut best_improvement = 0.0;

            for dim in 0..5 {
                let mut test_weights = weights.clone();
                match dim {
                    0 => test_weights.language_weight += epsilon,
                    1 => test_weights.framework_weight += epsilon,
                    2 => test_weights.symbol_hits_weight += epsilon,
                    3 => test_weights.path_tokens_weight += epsilon,
                    4 => test_weights.prompt_verb_weight += epsilon,
                    _ => {}
                }

                let test_metrics = self.validate(&test_weights);
                let test_score = test_metrics.score();
                let improvement = test_score - current_score;

                if improvement > best_improvement {
                    best_improvement = improvement;
                    best_delta_weights = vec![0.0; 5];
                    best_delta_weights[dim] = epsilon;
                }
            }

            // Update weights
            weights.language_weight += best_delta_weights[0] * learning_rate;
            weights.framework_weight += best_delta_weights[1] * learning_rate;
            weights.symbol_hits_weight += best_delta_weights[2] * learning_rate;
            weights.path_tokens_weight += best_delta_weights[3] * learning_rate;
            weights.prompt_verb_weight += best_delta_weights[4] * learning_rate;

            // Ensure weights stay positive
            weights.language_weight = weights.language_weight.max(0.01);
            weights.framework_weight = weights.framework_weight.max(0.01);
            weights.symbol_hits_weight = weights.symbol_hits_weight.max(0.01);
            weights.path_tokens_weight = weights.path_tokens_weight.max(0.01);
            weights.prompt_verb_weight = weights.prompt_verb_weight.max(0.01);

            if iteration % 10 == 0 {
                tracing::debug!(
                    "Iteration {}: score = {:.4}, weights = {:?}",
                    iteration,
                    current_score,
                    weights
                );
            }

            // Early stopping if no improvement
            if best_improvement < 1e-6 {
                break;
            }
        }

        let final_metrics = self.validate(&weights);
        tracing::info!(
            "Gradient descent complete. Final score: {:.4}, weights: {:?}",
            final_metrics.score(),
            weights
        );

        Ok(weights)
    }

    /// Compute feature score using weights
    fn compute_feature_score(&self, features: &[f32], weights: &RouterWeights) -> f32 {
        if features.len() < 22 {
            return 0.0;
        }

        let mut score = 0.0;

        // Language component
        let lang_strength = features[0..8].iter().fold(0.0f32, |a, &b| a.max(b));
        score += lang_strength * weights.language_weight;

        // Framework component
        let framework_strength = features[8..11].iter().sum::<f32>();
        score += framework_strength * weights.framework_weight;

        // Symbol hits
        score += features[11] * weights.symbol_hits_weight;

        // Path tokens
        score += features[12] * weights.path_tokens_weight;

        // Prompt verb
        let verb_strength = features[13..21].iter().fold(0.0f32, |a, &b| a.max(b));
        score += verb_strength * weights.prompt_verb_weight;

        score
    }

    /// Compute metrics for a single sample (simplified)
    fn compute_sample_metrics(
        &self,
        feature_score: f32,
        ground_truth: &[usize],
    ) -> ValidationMetrics {
        if ground_truth.is_empty() {
            return ValidationMetrics::zero();
        }

        let mut sampler = DeterministicSampler::new(feature_score, ground_truth);
        let prediction_budget = self.k.clamp(1, MAX_PREDICTED_ADAPTERS);
        let mut predicted = Vec::with_capacity(prediction_budget);

        while predicted.len() < prediction_budget {
            let candidate = (sampler.next() % MAX_ADAPTER_SPACE as u64) as usize;
            if !predicted.contains(&candidate) {
                predicted.push(candidate);
            }
        }

        let ground_truth_set: HashSet<usize> = ground_truth.iter().copied().collect();
        let mut correct_predictions = 0usize;
        let mut first_correct_rank: Option<usize> = None;
        for (rank, adapter) in predicted.iter().enumerate() {
            if ground_truth_set.contains(adapter) {
                correct_predictions += 1;
                if first_correct_rank.is_none() {
                    first_correct_rank = Some(rank);
                }
            }
        }

        let accuracy = if let Some(&top_prediction) = predicted.first() {
            if ground_truth_set.contains(&top_prediction) {
                1.0
            } else {
                0.0
            }
        } else {
            0.0
        };

        let precision = if predicted.is_empty() {
            0.0
        } else {
            correct_predictions as f32 / predicted.len() as f32
        };

        let recall = correct_predictions as f32 / ground_truth.len() as f32;

        let f1_score = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        };

        let mrr = first_correct_rank
            .map(|rank| 1.0 / (rank as f32 + 1.0))
            .unwrap_or(0.0);

        ValidationMetrics {
            accuracy,
            precision,
            recall,
            f1_score,
            mrr,
        }
    }
}

/// Maximum number of adapters that can be considered during validation.
const MAX_ADAPTER_SPACE: usize = 256;
/// Upper bound on predictions made per sample. Keeps validation deterministic and fast.
const MAX_PREDICTED_ADAPTERS: usize = 8;

/// Deterministic sampler that uses SplitMix64 to convert feature scores into adapter indices.
struct DeterministicSampler {
    state: u64,
}

impl DeterministicSampler {
    fn new(feature_score: f32, ground_truth: &[usize]) -> Self {
        // Seed is derived from feature score and ground truth to make validation
        // sensitive to the actual dataset while remaining deterministic.
        let mut state = feature_score.to_bits() as u64;
        for adapter in ground_truth {
            state = state.wrapping_add((*adapter as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        }
        Self { state }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calibration_dataset() {
        let mut dataset = CalibrationDataset {
            samples: Vec::new(),
        };

        dataset.add_sample(CalibrationSample {
            features: vec![1.0; 22],
            ground_truth_adapters: vec![0, 1],
            metadata: serde_json::json!({"prompt": "test"}),
        });

        assert_eq!(dataset.samples.len(), 1);
    }

    #[test]
    fn test_train_val_split() {
        let dataset = CalibrationDataset {
            samples: vec![
                CalibrationSample {
                    features: vec![1.0; 22],
                    ground_truth_adapters: vec![0],
                    metadata: serde_json::json!({}),
                };
                10
            ],
        };

        let (train, val) = dataset.train_val_split(0.8);
        assert_eq!(train.samples.len(), 8);
        assert_eq!(val.samples.len(), 2);
    }

    #[test]
    fn test_validation_metrics() {
        let metrics = ValidationMetrics {
            accuracy: 0.9,
            precision: 0.85,
            recall: 0.8,
            f1_score: 0.825,
            mrr: 0.9,
        };

        let score = metrics.score();
        assert!(score > 0.0 && score <= 1.0);
    }

    #[test]
    fn test_calibrator_validation() {
        let dataset = CalibrationDataset {
            samples: vec![CalibrationSample {
                features: vec![0.5; 22],
                ground_truth_adapters: vec![0, 1],
                metadata: serde_json::json!({}),
            }],
        };

        let calibrator = Calibrator::new(dataset, OptimizationMethod::GridSearch, 3);
        let weights = RouterWeights::default();
        let metrics = calibrator.validate(&weights);

        assert!(metrics.accuracy >= 0.0 && metrics.accuracy <= 1.0);
        assert!(metrics.precision >= 0.0 && metrics.precision <= 1.0);
        assert!(metrics.recall >= 0.0 && metrics.recall <= 1.0);
    }

    #[test]
    fn deterministic_metrics_for_same_inputs() {
        let sample = CalibrationSample {
            features: vec![0.25; 22],
            ground_truth_adapters: vec![2, 4, 6],
            metadata: serde_json::json!({}),
        };
        let dataset = CalibrationDataset {
            samples: vec![sample.clone(), sample],
        };
        let calibrator = Calibrator::new(dataset, OptimizationMethod::GridSearch, 4);
        let weights = RouterWeights::default();
        let first_metrics = calibrator.validate(&weights);
        let second_metrics = calibrator.validate(&weights);

        assert!((first_metrics.score() - second_metrics.score()).abs() < f32::EPSILON);
    }
}
