//! Contrastive loss training for embedding fine-tuning
//!
//! Implements InfoNCE loss with temperature scaling for learning
//! domain-specific embeddings via (query, relevant_chunk) pairs.
//!
//! # Loss Function
//!
//! The InfoNCE contrastive loss is computed as:
//!
//! ```text
//! loss = -log(exp(sim(a,p)/τ) / (exp(sim(a,p)/τ) + Σexp(sim(a,n)/τ)))
//! ```
//!
//! Where:
//! - `a` is the anchor embedding
//! - `p` is the positive embedding
//! - `n` are the negative embeddings
//! - `τ` (tau) is the temperature parameter
//! - `sim` is cosine similarity

use serde::{Deserialize, Serialize};

/// Temperature for softmax in contrastive loss (default 0.07)
pub const DEFAULT_TEMPERATURE: f32 = 0.07;

/// A training pair for contrastive learning
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingPair {
    /// Anchor text (query)
    pub anchor: String,
    /// Positive text (relevant chunk)
    pub positive: String,
    /// Hard negatives (irrelevant chunks)
    pub negatives: Vec<String>,
}

impl TrainingPair {
    /// Create a new training pair
    pub fn new(anchor: impl Into<String>, positive: impl Into<String>) -> Self {
        Self {
            anchor: anchor.into(),
            positive: positive.into(),
            negatives: Vec::new(),
        }
    }

    /// Add a hard negative to this training pair
    pub fn with_negative(mut self, negative: impl Into<String>) -> Self {
        self.negatives.push(negative.into());
        self
    }

    /// Add multiple hard negatives to this training pair
    pub fn with_negatives(mut self, negatives: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.negatives.extend(negatives.into_iter().map(|n| n.into()));
        self
    }
}

/// A batch of training pairs with pre-computed embeddings
#[derive(Debug, Clone)]
pub struct TrainingBatch {
    /// Anchor embeddings
    pub anchors: Vec<Vec<f32>>,
    /// Positive embeddings (same index as anchors)
    pub positives: Vec<Vec<f32>>,
    /// Negative embeddings per anchor
    pub negatives: Vec<Vec<Vec<f32>>>,
}

impl TrainingBatch {
    /// Create a new empty training batch
    pub fn new() -> Self {
        Self {
            anchors: Vec::new(),
            positives: Vec::new(),
            negatives: Vec::new(),
        }
    }

    /// Add a training sample to the batch
    pub fn add_sample(
        &mut self,
        anchor: Vec<f32>,
        positive: Vec<f32>,
        negatives: Vec<Vec<f32>>,
    ) {
        self.anchors.push(anchor);
        self.positives.push(positive);
        self.negatives.push(negatives);
    }

    /// Get the number of samples in this batch
    pub fn len(&self) -> usize {
        self.anchors.len()
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }
}

impl Default for TrainingBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute cosine similarity between two vectors
///
/// Returns a value in the range [-1, 1] where:
/// - 1 means identical direction
/// - 0 means orthogonal
/// - -1 means opposite direction
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a < 1e-9 || norm_b < 1e-9 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Compute InfoNCE contrastive loss
///
/// The loss is computed as:
/// ```text
/// loss = -log(exp(sim(a,p)/τ) / (exp(sim(a,p)/τ) + Σexp(sim(a,n)/τ)))
/// ```
///
/// Uses the log-sum-exp trick for numerical stability.
///
/// # Arguments
///
/// * `anchor` - The anchor embedding vector
/// * `positive` - The positive (similar) embedding vector
/// * `negatives` - Slice of negative (dissimilar) embedding vectors
/// * `temperature` - Temperature parameter τ for scaling similarities
///
/// # Returns
///
/// The contrastive loss value. Lower values indicate better separation
/// between positive and negative samples.
pub fn contrastive_loss(
    anchor: &[f32],
    positive: &[f32],
    negatives: &[&[f32]],
    temperature: f32,
) -> f32 {
    // Compute scaled similarities
    let pos_sim = cosine_similarity(anchor, positive) / temperature;

    // Collect all similarities (positive + negatives) for log-sum-exp
    let mut all_sims = vec![pos_sim];
    for neg in negatives {
        let neg_sim = cosine_similarity(anchor, neg) / temperature;
        all_sims.push(neg_sim);
    }

    // Use log-sum-exp trick for numerical stability:
    // log(Σexp(x_i)) = max(x) + log(Σexp(x_i - max(x)))
    let max_sim = all_sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    if max_sim.is_infinite() {
        return f32::INFINITY; // Signal degenerate input
    }

    let log_sum_exp = max_sim + all_sims.iter().map(|&x| (x - max_sim).exp()).sum::<f32>().ln();

    // InfoNCE loss: -log(exp(pos_sim) / Σexp(all_sims))
    //             = -(pos_sim - log_sum_exp)
    //             = log_sum_exp - pos_sim
    log_sum_exp - pos_sim
}

/// Compute batch contrastive loss (average over all samples)
///
/// # Arguments
///
/// * `batch` - A training batch with pre-computed embeddings
/// * `temperature` - Temperature parameter for scaling similarities
///
/// # Returns
///
/// The average contrastive loss across all samples in the batch.
pub fn batch_contrastive_loss(batch: &TrainingBatch, temperature: f32) -> f32 {
    if batch.is_empty() {
        return 0.0;
    }

    let total_loss: f32 = batch
        .anchors
        .iter()
        .zip(batch.positives.iter())
        .zip(batch.negatives.iter())
        .map(|((anchor, positive), negatives)| {
            let neg_refs: Vec<&[f32]> = negatives.iter().map(|v| v.as_slice()).collect();
            contrastive_loss(anchor, positive, &neg_refs, temperature)
        })
        .sum();

    total_loss / batch.len() as f32
}

/// Training configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingConfig {
    /// Temperature for contrastive loss
    pub temperature: f32,
    /// Learning rate
    pub learning_rate: f32,
    /// Number of epochs
    pub epochs: usize,
    /// Batch size
    pub batch_size: usize,
    /// Random seed for reproducibility
    pub seed: u64,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            temperature: DEFAULT_TEMPERATURE,
            learning_rate: 1e-4,
            epochs: 3,
            batch_size: 32,
            seed: 42,
        }
    }
}

impl TrainingConfig {
    /// Create a new training config with custom temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        assert!(
            temperature > 0.0 && temperature.is_finite(),
            "temperature must be positive and finite"
        );
        self.temperature = temperature;
        self
    }

    /// Create a new training config with custom learning rate
    pub fn with_learning_rate(mut self, learning_rate: f32) -> Self {
        self.learning_rate = learning_rate;
        self
    }

    /// Create a new training config with custom epochs
    pub fn with_epochs(mut self, epochs: usize) -> Self {
        self.epochs = epochs;
        self
    }

    /// Create a new training config with custom batch size
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Create a new training config with custom seed
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }
}

/// Training statistics for a single epoch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochStats {
    /// Epoch number (0-indexed)
    pub epoch: usize,
    /// Average loss for this epoch
    pub loss: f32,
    /// Number of batches processed
    pub batches: usize,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Training progress callback
pub type ProgressCallback = Box<dyn Fn(&EpochStats) + Send + Sync>;

/// Embedding trainer for contrastive learning
pub struct EmbeddingTrainer {
    /// Training configuration
    config: TrainingConfig,
    /// Epoch statistics history
    stats: Vec<EpochStats>,
}

impl EmbeddingTrainer {
    /// Create a new embedding trainer with the given configuration
    pub fn new(config: TrainingConfig) -> Self {
        Self {
            config,
            stats: Vec::new(),
        }
    }

    /// Get the training configuration
    pub fn config(&self) -> &TrainingConfig {
        &self.config
    }

    /// Get the training statistics history
    pub fn stats(&self) -> &[EpochStats] {
        &self.stats
    }

    /// Compute loss for a single batch (useful for validation)
    pub fn compute_batch_loss(&self, batch: &TrainingBatch) -> f32 {
        batch_contrastive_loss(batch, self.config.temperature)
    }

    /// Record epoch statistics
    pub fn record_epoch(&mut self, epoch: usize, loss: f32, batches: usize, duration_ms: u64) {
        self.stats.push(EpochStats {
            epoch,
            loss,
            batches,
            duration_ms,
        });
    }

    /// Get the best (lowest) loss seen so far
    pub fn best_loss(&self) -> Option<f32> {
        self.stats.iter().map(|s| s.loss).reduce(f32::min)
    }

    /// Get the most recent loss
    pub fn current_loss(&self) -> Option<f32> {
        self.stats.last().map(|s| s.loss)
    }

    /// Reset training statistics
    pub fn reset_stats(&mut self) {
        self.stats.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Cosine similarity tests
    // ========================================================================

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6, "Identical vectors should have similarity 1.0");
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "Orthogonal vectors should have similarity 0.0");
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6, "Opposite vectors should have similarity -1.0");
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0, "Empty vectors should return 0.0");
    }

    #[test]
    fn test_cosine_similarity_mismatched_length() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0, "Mismatched length vectors should return 0.0");
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0, "Zero vector should return 0.0");
    }

    // ========================================================================
    // Contrastive loss tests
    // ========================================================================

    #[test]
    fn test_contrastive_loss_perfect_match() {
        // Anchor = positive, should have low loss
        let anchor = vec![1.0, 0.0, 0.0];
        let positive = vec![1.0, 0.0, 0.0];
        let neg1 = vec![0.0, 1.0, 0.0];
        let neg2 = vec![0.0, 0.0, 1.0];
        let negatives: Vec<&[f32]> = vec![&neg1, &neg2];

        let loss = contrastive_loss(&anchor, &positive, &negatives, 0.07);
        assert!(loss < 0.1, "Perfect match should have very low loss, got: {}", loss);
    }

    #[test]
    fn test_contrastive_loss_poor_match() {
        // Anchor != positive (orthogonal), should have higher loss than perfect match
        let anchor = vec![1.0, 0.0, 0.0];
        let positive = vec![0.0, 1.0, 0.0]; // Orthogonal
        let neg1 = vec![0.0, 0.0, 1.0];
        let negatives: Vec<&[f32]> = vec![&neg1];

        let loss = contrastive_loss(&anchor, &positive, &negatives, 0.07);
        // When positive is orthogonal to anchor, exp(0/τ) = 1
        // When negative is also orthogonal, exp(0/τ) = 1
        // So loss = -ln(1/(1+1)) = -ln(0.5) = ln(2) ≈ 0.693
        // This is worse than perfect match (loss ≈ 0), so test that
        let perfect_loss = contrastive_loss(&anchor, &anchor, &negatives, 0.07);
        assert!(
            loss > perfect_loss * 2.0,
            "Poor match (loss={}) should be much higher than perfect match (loss={})",
            loss, perfect_loss
        );
    }

    #[test]
    fn test_contrastive_loss_no_negatives() {
        // With no negatives, loss should be 0 (perfect)
        let anchor = vec![1.0, 0.0, 0.0];
        let positive = vec![1.0, 0.0, 0.0];
        let negatives: Vec<&[f32]> = vec![];

        let loss = contrastive_loss(&anchor, &positive, &negatives, 0.07);
        assert!(loss.abs() < 1e-6, "No negatives should give ~0 loss, got: {}", loss);
    }

    #[test]
    fn test_contrastive_loss_temperature_effect() {
        let anchor = vec![1.0, 0.0, 0.0];
        let positive = vec![0.9, 0.1, 0.0]; // Slightly different
        let neg1 = vec![0.0, 1.0, 0.0];
        let negatives: Vec<&[f32]> = vec![&neg1];

        // Lower temperature makes the loss more sensitive to differences
        let loss_low_temp = contrastive_loss(&anchor, &positive, &negatives, 0.01);
        let loss_high_temp = contrastive_loss(&anchor, &positive, &negatives, 1.0);

        // With higher temperature, the distribution is smoother (loss is different)
        // The actual relationship depends on the similarity values
        assert!(loss_low_temp != loss_high_temp, "Temperature should affect loss");
    }

    // ========================================================================
    // Batch loss tests
    // ========================================================================

    #[test]
    fn test_batch_contrastive_loss_empty() {
        let batch = TrainingBatch::new();
        let loss = batch_contrastive_loss(&batch, 0.07);
        assert_eq!(loss, 0.0, "Empty batch should have 0 loss");
    }

    #[test]
    fn test_batch_contrastive_loss_single() {
        let mut batch = TrainingBatch::new();
        batch.add_sample(
            vec![1.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0],
            vec![vec![0.0, 1.0, 0.0]],
        );

        let loss = batch_contrastive_loss(&batch, 0.07);
        let expected_loss = contrastive_loss(
            &[1.0, 0.0, 0.0],
            &[1.0, 0.0, 0.0],
            &[&[0.0, 1.0, 0.0]],
            0.07,
        );
        assert!((loss - expected_loss).abs() < 1e-6, "Single sample batch loss should match direct computation");
    }

    #[test]
    fn test_batch_contrastive_loss_multiple() {
        let mut batch = TrainingBatch::new();
        // Perfect match
        batch.add_sample(
            vec![1.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0],
            vec![vec![0.0, 1.0, 0.0]],
        );
        // Poor match
        batch.add_sample(
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![vec![0.0, 0.0, 1.0]],
        );

        let loss = batch_contrastive_loss(&batch, 0.07);
        assert!(loss > 0.0, "Batch with mixed quality should have non-zero loss");
    }

    // ========================================================================
    // TrainingPair tests
    // ========================================================================

    #[test]
    fn test_training_pair_serialization() {
        let pair = TrainingPair {
            anchor: "what is LoRA?".to_string(),
            positive: "LoRA is a low-rank adaptation technique".to_string(),
            negatives: vec!["weather forecast".to_string()],
        };
        let json = serde_json::to_string(&pair).unwrap();
        let parsed: TrainingPair = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.anchor, pair.anchor);
        assert_eq!(parsed.positive, pair.positive);
        assert_eq!(parsed.negatives, pair.negatives);
    }

    #[test]
    fn test_training_pair_builder() {
        let pair = TrainingPair::new("anchor text", "positive text")
            .with_negative("negative 1")
            .with_negative("negative 2");

        assert_eq!(pair.anchor, "anchor text");
        assert_eq!(pair.positive, "positive text");
        assert_eq!(pair.negatives.len(), 2);
    }

    #[test]
    fn test_training_pair_with_negatives() {
        let pair = TrainingPair::new("anchor", "positive")
            .with_negatives(vec!["neg1", "neg2", "neg3"]);

        assert_eq!(pair.negatives.len(), 3);
    }

    // ========================================================================
    // TrainingBatch tests
    // ========================================================================

    #[test]
    fn test_training_batch_new() {
        let batch = TrainingBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }

    #[test]
    fn test_training_batch_add_sample() {
        let mut batch = TrainingBatch::new();
        batch.add_sample(
            vec![1.0, 2.0, 3.0],
            vec![4.0, 5.0, 6.0],
            vec![vec![7.0, 8.0, 9.0]],
        );

        assert!(!batch.is_empty());
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.anchors.len(), 1);
        assert_eq!(batch.positives.len(), 1);
        assert_eq!(batch.negatives.len(), 1);
    }

    // ========================================================================
    // TrainingConfig tests
    // ========================================================================

    #[test]
    fn test_training_config_default() {
        let config = TrainingConfig::default();
        assert!((config.temperature - DEFAULT_TEMPERATURE).abs() < 1e-6);
        assert!((config.learning_rate - 1e-4).abs() < 1e-9);
        assert_eq!(config.epochs, 3);
        assert_eq!(config.batch_size, 32);
        assert_eq!(config.seed, 42);
    }

    #[test]
    fn test_training_config_builder() {
        let config = TrainingConfig::default()
            .with_temperature(0.1)
            .with_learning_rate(1e-5)
            .with_epochs(10)
            .with_batch_size(64)
            .with_seed(123);

        assert!((config.temperature - 0.1).abs() < 1e-6);
        assert!((config.learning_rate - 1e-5).abs() < 1e-9);
        assert_eq!(config.epochs, 10);
        assert_eq!(config.batch_size, 64);
        assert_eq!(config.seed, 123);
    }

    #[test]
    fn test_training_config_serialization() {
        let config = TrainingConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: TrainingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    #[should_panic(expected = "temperature must be positive")]
    fn test_training_config_invalid_temperature_zero() {
        TrainingConfig::default().with_temperature(0.0);
    }

    #[test]
    #[should_panic(expected = "temperature must be positive")]
    fn test_training_config_invalid_temperature_negative() {
        TrainingConfig::default().with_temperature(-0.1);
    }

    #[test]
    #[should_panic(expected = "temperature must be positive")]
    fn test_training_config_invalid_temperature_infinite() {
        TrainingConfig::default().with_temperature(f32::INFINITY);
    }

    // ========================================================================
    // EmbeddingTrainer tests
    // ========================================================================

    #[test]
    fn test_embedding_trainer_new() {
        let config = TrainingConfig::default();
        let trainer = EmbeddingTrainer::new(config.clone());

        assert_eq!(trainer.config(), &config);
        assert!(trainer.stats().is_empty());
        assert_eq!(trainer.best_loss(), None);
        assert_eq!(trainer.current_loss(), None);
    }

    #[test]
    fn test_embedding_trainer_record_epoch() {
        let config = TrainingConfig::default();
        let mut trainer = EmbeddingTrainer::new(config);

        trainer.record_epoch(0, 1.5, 10, 1000);
        trainer.record_epoch(1, 1.2, 10, 900);
        trainer.record_epoch(2, 1.0, 10, 850);

        assert_eq!(trainer.stats().len(), 3);
        assert!((trainer.best_loss().unwrap() - 1.0).abs() < 1e-6);
        assert!((trainer.current_loss().unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_trainer_compute_batch_loss() {
        let config = TrainingConfig::default();
        let trainer = EmbeddingTrainer::new(config);

        let mut batch = TrainingBatch::new();
        batch.add_sample(
            vec![1.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0],
            vec![vec![0.0, 1.0, 0.0]],
        );

        let loss = trainer.compute_batch_loss(&batch);
        assert!(loss >= 0.0, "Loss should be non-negative");
    }

    #[test]
    fn test_embedding_trainer_reset_stats() {
        let config = TrainingConfig::default();
        let mut trainer = EmbeddingTrainer::new(config);

        trainer.record_epoch(0, 1.5, 10, 1000);
        assert!(!trainer.stats().is_empty());

        trainer.reset_stats();
        assert!(trainer.stats().is_empty());
        assert_eq!(trainer.best_loss(), None);
    }

    // ========================================================================
    // Edge cases and numerical stability
    // ========================================================================

    #[test]
    fn test_contrastive_loss_numerical_stability() {
        // Test with very small temperature (could cause overflow)
        let anchor = vec![1.0, 0.0, 0.0];
        let positive = vec![0.99, 0.1, 0.0];
        let neg1 = vec![0.0, 1.0, 0.0];
        let negatives: Vec<&[f32]> = vec![&neg1];

        // Should not panic or return NaN/Inf with very small temperature
        let loss = contrastive_loss(&anchor, &positive, &negatives, 0.001);
        assert!(loss.is_finite(), "Loss should be finite with small temperature");
    }

    #[test]
    fn test_contrastive_loss_many_negatives() {
        let anchor = vec![1.0, 0.0, 0.0];
        let positive = vec![1.0, 0.0, 0.0];

        // Many negatives should increase the loss (more denominator terms)
        let neg: Vec<f32> = vec![0.0, 1.0, 0.0];
        let negatives_few: Vec<&[f32]> = vec![&neg];
        let negatives_many: Vec<&[f32]> = vec![&neg; 10];

        let loss_few = contrastive_loss(&anchor, &positive, &negatives_few, 0.07);
        let loss_many = contrastive_loss(&anchor, &positive, &negatives_many, 0.07);

        assert!(loss_many > loss_few, "More negatives should increase loss");
    }
}
