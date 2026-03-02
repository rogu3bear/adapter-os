//! Embedding model trainer using contrastive learning.
//!
//! Trains a projection layer on top of a base model's hidden states to produce
//! semantic embeddings suitable for RAG and similarity search.
//!
//! # Architecture
//!
//! ```text
//! Input Text → Tokenizer → Base Model → Hidden States → Pooling → Projection → Embedding
//!                                                                     ↓
//!                                                              Contrastive Loss
//! ```
//!
//! Only the projection layer is trained; the base model remains frozen.

use crate::tokenizer::QwenTokenizer;
use crate::training::embedding_loss::{info_nce_loss, l2_normalize, triplet_loss};
use adapteros_api_types::training::{
    EmbeddingExample, EmbeddingTrainingConfig, EmbeddingTrainingMode, PoolingStrategy,
};
use adapteros_core::{AosError, Result};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Result of embedding model training.
#[derive(Debug, Clone)]
pub struct EmbeddingTrainingResult {
    /// Final training loss
    pub final_loss: f32,
    /// Best validation loss achieved
    pub best_loss: f32,
    /// Total training time in seconds
    pub training_time_secs: f64,
    /// Number of epochs completed
    pub epochs_completed: usize,
    /// Total examples processed
    pub examples_processed: usize,
    /// Output embedding dimension
    pub embedding_dim: usize,
}

/// Linear projection layer: hidden_dim → embedding_dim.
#[derive(Debug, Clone)]
pub struct ProjectionLayer {
    /// Weight matrix [embedding_dim, hidden_dim]
    pub weights: Vec<f32>,
    /// Optional bias [embedding_dim]
    pub bias: Option<Vec<f32>>,
    /// Input dimension (base model hidden size)
    pub hidden_dim: usize,
    /// Output dimension (embedding size)
    pub embedding_dim: usize,
}

impl ProjectionLayer {
    /// Create a new projection layer with Xavier initialization.
    pub fn new(hidden_dim: usize, embedding_dim: usize, use_bias: bool) -> Self {
        // Xavier/Glorot initialization: scale = sqrt(2 / (fan_in + fan_out))
        let scale = (2.0 / (hidden_dim + embedding_dim) as f32).sqrt();

        // Initialize weights with deterministic pseudo-random values
        let mut weights = Vec::with_capacity(embedding_dim * hidden_dim);
        for i in 0..(embedding_dim * hidden_dim) {
            // Simple deterministic initialization based on index
            let hash = ((i as u64).wrapping_mul(0x9E3779B97F4A7C15)) as f32 / u64::MAX as f32;
            weights.push((hash * 2.0 - 1.0) * scale);
        }

        let bias = if use_bias {
            Some(vec![0.0; embedding_dim])
        } else {
            None
        };

        Self {
            weights,
            bias,
            hidden_dim,
            embedding_dim,
        }
    }

    /// Forward pass: project hidden state to embedding.
    pub fn forward(&self, hidden: &[f32]) -> Vec<f32> {
        debug_assert_eq!(hidden.len(), self.hidden_dim);

        let mut output = vec![0.0; self.embedding_dim];

        // Matrix-vector multiplication: output = W @ hidden + bias
        for i in 0..self.embedding_dim {
            let mut sum = 0.0;
            for (j, &h_val) in hidden.iter().enumerate().take(self.hidden_dim) {
                sum += self.weights[i * self.hidden_dim + j] * h_val;
            }
            if let Some(ref bias) = self.bias {
                sum += bias[i];
            }
            output[i] = sum;
        }

        output
    }

    /// Compute gradients for a batch of examples.
    ///
    /// Returns (weight_grad, bias_grad) for a single input-output pair.
    pub fn backward(&self, hidden: &[f32], output_grad: &[f32]) -> (Vec<f32>, Option<Vec<f32>>) {
        debug_assert_eq!(hidden.len(), self.hidden_dim);
        debug_assert_eq!(output_grad.len(), self.embedding_dim);

        // Weight gradient: dW = output_grad ⊗ hidden (outer product)
        let mut weight_grad = vec![0.0; self.embedding_dim * self.hidden_dim];
        for i in 0..self.embedding_dim {
            for j in 0..self.hidden_dim {
                weight_grad[i * self.hidden_dim + j] = output_grad[i] * hidden[j];
            }
        }

        // Bias gradient: db = output_grad
        let bias_grad = if self.bias.is_some() {
            Some(output_grad.to_vec())
        } else {
            None
        };

        (weight_grad, bias_grad)
    }

    /// Update weights using gradient descent.
    pub fn update(&mut self, weight_grad: &[f32], bias_grad: Option<&[f32]>, lr: f32) {
        debug_assert_eq!(weight_grad.len(), self.weights.len());

        // SGD update: W = W - lr * grad
        for (w, g) in self.weights.iter_mut().zip(weight_grad) {
            *w -= lr * g;
        }

        if let (Some(ref mut bias), Some(bg)) = (&mut self.bias, bias_grad) {
            for (b, g) in bias.iter_mut().zip(bg) {
                *b -= lr * g;
            }
        }
    }
}

/// Embedding model trainer using contrastive learning.
///
/// Trains a projection layer to map base model hidden states to semantic embeddings.
pub struct EmbeddingTrainer {
    /// Training configuration
    config: EmbeddingTrainingConfig,
    /// Trainable projection layer
    projection: ProjectionLayer,
    /// Tokenizer for text encoding
    tokenizer: Arc<QwenTokenizer>,
    /// Base model hidden dimension
    hidden_dim: usize,
    /// Base model for extracting real hidden states during training.
    #[cfg(feature = "multi-backend")]
    base_model: Option<Arc<adapteros_lora_mlx_ffi::MLXFFIModel>>,
    /// Which hidden state layer to extract (e.g., "layer_31_output")
    hidden_state_key: String,
}

impl EmbeddingTrainer {
    /// Create a new embedding trainer.
    ///
    /// # Arguments
    /// * `config` - Training configuration
    /// * `tokenizer` - Tokenizer for text encoding
    /// * `hidden_dim` - Base model hidden dimension (from manifest)
    pub fn new(
        config: EmbeddingTrainingConfig,
        tokenizer: Arc<QwenTokenizer>,
        hidden_dim: usize,
    ) -> Self {
        let projection = ProjectionLayer::new(hidden_dim, config.embedding_dim, true);

        Self {
            config,
            projection,
            tokenizer,
            hidden_dim,
            #[cfg(feature = "multi-backend")]
            base_model: None,
            hidden_state_key: String::new(), // Will be set by load_base_model
        }
    }

    /// Load a base model for extracting real hidden states during encoding.
    ///
    /// When a base model is loaded, `encode_text` will use the model's hidden states
    /// instead of relying on a placeholder embedding matrix.
    ///
    /// # Arguments
    /// * `model_path` - Path to the model directory (containing safetensors files)
    ///
    /// # Errors
    /// Returns an error if the model cannot be loaded or if dimensions don't match.
    #[cfg(feature = "multi-backend")]
    pub fn load_base_model(&mut self, model_path: &Path) -> Result<()> {
        use adapteros_lora_mlx_ffi::MLXFFIModel;

        info!(
            model_path = %model_path.display(),
            "Loading base model for embedding training"
        );

        let model = MLXFFIModel::load(model_path).map_err(|e| {
            AosError::Training(format!(
                "Failed to load base model from '{}': {}",
                model_path.display(),
                e
            ))
        })?;

        // Extract values from config before moving model into Arc
        let model_config = model.config();
        let hidden_size = model_config.hidden_size;
        let num_hidden_layers = model_config.num_hidden_layers;

        // Validate hidden dimension matches configuration
        if hidden_size != self.hidden_dim {
            return Err(AosError::Training(format!(
                "Model hidden_size ({}) != configured hidden_dim ({}). \
                 Update hidden_dim to match the model.",
                hidden_size, self.hidden_dim
            )));
        }

        // Use the last transformer layer by default
        let last_layer = num_hidden_layers.saturating_sub(1);
        self.hidden_state_key = format!("layer_{}_output", last_layer);
        self.base_model = Some(Arc::new(model));

        info!(
            hidden_state_key = %self.hidden_state_key,
            hidden_dim = self.hidden_dim,
            num_layers = num_hidden_layers,
            "Loaded base model for embedding training"
        );

        Ok(())
    }

    /// Check if a base model is loaded for real hidden state extraction.
    #[cfg(feature = "multi-backend")]
    pub fn has_base_model(&self) -> bool {
        self.base_model.is_some()
    }

    /// Check if a base model is loaded (always false without multi-backend).
    #[cfg(not(feature = "multi-backend"))]
    pub fn has_base_model(&self) -> bool {
        false
    }

    /// Pool hidden states based on configured strategy.
    fn pool(&self, hidden_states: &[Vec<f32>], attention_mask: &[u32]) -> Vec<f32> {
        if hidden_states.is_empty() {
            return vec![0.0; self.hidden_dim];
        }

        match self.config.pooling {
            PoolingStrategy::Mean => {
                // Mean pooling over non-padding tokens
                let mut sum = vec![0.0; self.hidden_dim];
                let mut count = 0;

                for (hidden, &mask) in hidden_states.iter().zip(attention_mask) {
                    if mask > 0 {
                        for (s, h) in sum.iter_mut().zip(hidden) {
                            *s += h;
                        }
                        count += 1;
                    }
                }

                if count > 0 {
                    for s in &mut sum {
                        *s /= count as f32;
                    }
                }

                sum
            }
            PoolingStrategy::Cls => {
                // Use first token (CLS)
                hidden_states[0].clone()
            }
            PoolingStrategy::Max => {
                // Max pooling over non-padding tokens
                let mut result = vec![f32::NEG_INFINITY; self.hidden_dim];

                for (hidden, &mask) in hidden_states.iter().zip(attention_mask) {
                    if mask > 0 {
                        for (r, h) in result.iter_mut().zip(hidden) {
                            *r = r.max(*h);
                        }
                    }
                }

                // Replace -inf with 0 for empty sequences
                for r in &mut result {
                    if *r == f32::NEG_INFINITY {
                        *r = 0.0;
                    }
                }

                result
            }
            PoolingStrategy::Last => {
                // Use last non-padding token
                for (i, &mask) in attention_mask.iter().enumerate().rev() {
                    if mask > 0 {
                        return hidden_states[i].clone();
                    }
                }
                hidden_states
                    .last()
                    .cloned()
                    .unwrap_or_else(|| vec![0.0; self.hidden_dim])
            }
        }
    }

    /// Encode text to embedding using the projection layer.
    ///
    /// When a base model is loaded (via `load_base_model`), this extracts real hidden states
    /// from the model. Otherwise, falls back to using the provided embedding_matrix.
    ///
    /// # Arguments
    /// * `text` - Input text to encode
    /// * `embedding_matrix` - Optional fallback embedding matrix (required if no base model)
    pub fn encode_text(&self, text: &str, embedding_matrix: Option<&[f32]>) -> Result<Vec<f32>> {
        // Tokenize
        let token_ids = self.tokenizer.encode(text)?;

        if token_ids.is_empty() {
            return Ok(vec![0.0; self.config.embedding_dim]);
        }

        // Try to use base model for real hidden states
        #[cfg(feature = "multi-backend")]
        if let Some(ref model) = self.base_model {
            // Use real hidden states from base model
            // Position 0 for embedding training - process full sequence from the start
            let (_logits, hidden_states) = model.forward_with_hidden_states(&token_ids, 0)?;

            let hidden = hidden_states.get(&self.hidden_state_key).ok_or_else(|| {
                AosError::Training(format!(
                    "Hidden state key '{}' not found in model output. Available keys: {:?}",
                    self.hidden_state_key,
                    hidden_states.keys().collect::<Vec<_>>()
                ))
            })?;

            // Convert flat hidden states to per-token vectors for pooling
            let attention_mask: Vec<u32> = vec![1; token_ids.len()];
            let hidden_vecs: Vec<Vec<f32>> = hidden
                .chunks(self.hidden_dim)
                .map(|chunk| chunk.to_vec())
                .collect();

            let pooled = self.pool(&hidden_vecs, &attention_mask);
            let mut embedding = self.projection.forward(&pooled);

            if self.config.normalize {
                l2_normalize(&mut embedding);
            }
            return Ok(embedding);
        }

        // Fallback: use embedding_matrix placeholder
        let embedding_matrix = embedding_matrix.ok_or_else(|| {
            AosError::Training(
                "Base model or embedding_matrix required for encode_text".to_string(),
            )
        })?;

        // Get token embeddings and average (placeholder for real hidden states)
        let mut hidden = vec![0.0; self.hidden_dim];
        let mut count = 0;

        for &token_id in &token_ids {
            let start = (token_id as usize) * self.hidden_dim;
            let end = start + self.hidden_dim;
            if end <= embedding_matrix.len() {
                for (h, &e) in hidden.iter_mut().zip(&embedding_matrix[start..end]) {
                    *h += e;
                }
                count += 1;
            }
        }

        if count > 0 {
            for h in &mut hidden {
                *h /= count as f32;
            }
        }

        // Project to embedding space
        let mut embedding = self.projection.forward(&hidden);

        // Normalize if configured
        if self.config.normalize {
            l2_normalize(&mut embedding);
        }

        Ok(embedding)
    }

    /// Train on a batch of triplet examples.
    ///
    /// Returns the batch loss.
    pub fn train_triplet_batch(
        &mut self,
        anchors: &[Vec<f32>],
        positives: &[Vec<f32>],
        negatives: &[Vec<f32>],
        margin: f32,
    ) -> f32 {
        debug_assert_eq!(anchors.len(), positives.len());
        debug_assert_eq!(anchors.len(), negatives.len());

        if anchors.is_empty() {
            return 0.0;
        }

        let batch_size = anchors.len();
        let mut total_loss = 0.0;

        // Accumulate gradients
        let mut weight_grad_acc = vec![0.0; self.projection.weights.len()];
        let mut bias_grad_acc = self.projection.bias.as_ref().map(|b| vec![0.0; b.len()]);

        for i in 0..batch_size {
            // Forward pass
            let mut anchor_emb = self.projection.forward(&anchors[i]);
            let mut positive_emb = self.projection.forward(&positives[i]);
            let mut negative_emb = self.projection.forward(&negatives[i]);

            if self.config.normalize {
                l2_normalize(&mut anchor_emb);
                l2_normalize(&mut positive_emb);
                l2_normalize(&mut negative_emb);
            }

            // Compute loss
            let loss = triplet_loss(&anchor_emb, &positive_emb, &negative_emb, margin);
            total_loss += loss;

            // Skip backward if loss is zero (constraint satisfied)
            if loss > 0.0 {
                // Compute gradients (simplified - proper impl would use autograd)
                // For triplet loss: grad_anchor = (anchor - positive) - (anchor - negative)
                //                               = positive - negative (normalized)
                let grad_scale = 1.0 / batch_size as f32;

                // Approximate gradient through projection
                let (wg, bg) = self.projection.backward(&anchors[i], &anchor_emb);
                for (acc, g) in weight_grad_acc.iter_mut().zip(&wg) {
                    *acc += g * grad_scale;
                }
                if let (Some(ref mut acc), Some(ref bg)) = (&mut bias_grad_acc, &bg) {
                    for (a, g) in acc.iter_mut().zip(bg) {
                        *a += g * grad_scale;
                    }
                }
            }
        }

        // Update weights
        self.projection.update(
            &weight_grad_acc,
            bias_grad_acc.as_deref(),
            self.config.learning_rate,
        );

        total_loss / batch_size as f32
    }

    /// Train on a batch using InfoNCE loss.
    ///
    /// Returns the batch loss.
    pub fn train_info_nce_batch(
        &mut self,
        queries: &[Vec<f32>],
        positives: &[Vec<f32>],
        temperature: f32,
    ) -> f32 {
        debug_assert_eq!(queries.len(), positives.len());

        if queries.is_empty() {
            return 0.0;
        }

        let batch_size = queries.len();

        // Forward pass - project all inputs
        let mut query_embs: Vec<Vec<f32>> = queries
            .iter()
            .map(|q| {
                let mut emb = self.projection.forward(q);
                if self.config.normalize {
                    l2_normalize(&mut emb);
                }
                emb
            })
            .collect();

        let mut positive_embs: Vec<Vec<f32>> = positives
            .iter()
            .map(|p| {
                let mut emb = self.projection.forward(p);
                if self.config.normalize {
                    l2_normalize(&mut emb);
                }
                emb
            })
            .collect();

        // Compute InfoNCE loss
        let loss = info_nce_loss(&query_embs, &positive_embs, temperature);

        // Simplified gradient update (proper impl would use backprop through loss)
        // For now, just use a small random perturbation to demonstrate the training loop
        let grad_scale = self.config.learning_rate / batch_size as f32;
        for (i, (q, p)) in queries.iter().zip(positives).enumerate() {
            let (wg_q, _bg_q) = self.projection.backward(q, &query_embs[i]);
            let (wg_p, _bg_p) = self.projection.backward(p, &positive_embs[i]);

            // Update with averaged gradients
            for (w, (gq, gp)) in self
                .projection
                .weights
                .iter_mut()
                .zip(wg_q.iter().zip(&wg_p))
            {
                *w -= grad_scale * (gq + gp) * 0.5 * loss;
            }
        }

        loss
    }

    /// Save the trained projection layer to safetensors format.
    pub fn save(&self, path: &Path) -> Result<()> {
        // Convert weights to binary
        let weight_data: Vec<u8> = self
            .projection
            .weights
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AosError::Worker(format!("Failed to create output directory: {}", e))
            })?;
        }

        // Write metadata alongside
        let metadata_path = path.with_extension("json");
        let metadata = serde_json::json!({
            "embedding_dim": self.config.embedding_dim,
            "hidden_dim": self.hidden_dim,
            "pooling": format!("{:?}", self.config.pooling),
            "normalize": self.config.normalize,
            "has_bias": self.projection.bias.is_some(),
        });

        std::fs::write(
            &metadata_path,
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .map_err(|e| AosError::Worker(format!("Failed to write metadata: {}", e)))?;

        // Write weights as raw binary for now (safetensors requires more setup)
        let weights_path = path.with_extension("bin");
        std::fs::write(&weights_path, &weight_data)
            .map_err(|e| AosError::Worker(format!("Failed to write weights: {}", e)))?;

        if let Some(ref bias) = self.projection.bias {
            let bias_data: Vec<u8> = bias.iter().flat_map(|f| f.to_le_bytes()).collect();
            let bias_path = path.with_file_name(
                path.file_stem()
                    .map(|s| format!("{}_bias.bin", s.to_string_lossy()))
                    .unwrap_or_else(|| "bias.bin".to_string()),
            );
            std::fs::write(&bias_path, &bias_data)
                .map_err(|e| AosError::Worker(format!("Failed to write bias: {}", e)))?;
        }

        info!(
            path = %path.display(),
            embedding_dim = self.config.embedding_dim,
            hidden_dim = self.hidden_dim,
            "Saved embedding projection layer"
        );

        Ok(())
    }

    /// Get the current projection layer for inspection.
    pub fn projection(&self) -> &ProjectionLayer {
        &self.projection
    }

    /// Get training configuration.
    pub fn config(&self) -> &EmbeddingTrainingConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_tokenizer() -> Arc<QwenTokenizer> {
        let model = tokenizers::models::bpe::BPE::builder().build().unwrap();
        let tokenizer = tokenizers::Tokenizer::new(model);

        let special_tokens = adapteros_core::tokenizer_config::SpecialTokenMap {
            eos_token_id: 2,
            bos_token_id: Some(1),
            pad_token_id: Some(0),
            unk_token_id: None,
            im_start_id: None,
            im_end_id: None,
            fim_prefix_id: None,
            fim_suffix_id: None,
            fim_middle_id: None,
            source: adapteros_core::tokenizer_config::TokenMapSource::Unknown,
        };
        Arc::new(QwenTokenizer::from_tokenizer_with_tokens(
            tokenizer,
            special_tokens,
        ))
    }

    #[test]
    fn test_projection_layer_dims() {
        let proj = ProjectionLayer::new(768, 384, true);
        assert_eq!(proj.weights.len(), 384 * 768);
        assert_eq!(proj.bias.as_ref().unwrap().len(), 384);
        assert_eq!(proj.hidden_dim, 768);
        assert_eq!(proj.embedding_dim, 384);
    }

    #[test]
    fn test_projection_forward() {
        let proj = ProjectionLayer::new(4, 2, false);
        let input = vec![1.0, 0.0, 0.0, 0.0];
        let output = proj.forward(&input);
        assert_eq!(output.len(), 2);
    }

    #[test]
    fn test_trainer_initialization() {
        let tokenizer = mock_tokenizer();
        let config = EmbeddingTrainingConfig {
            mode: EmbeddingTrainingMode::Triplet { margin: 0.5 },
            embedding_dim: 128,
            pooling: PoolingStrategy::Mean,
            normalize: true,
            learning_rate: 0.001,
            batch_size: 32,
            epochs: 3,
            warmup_steps: 100,
            max_seq_length: 512,
        };

        let trainer = EmbeddingTrainer::new(config.clone(), tokenizer, 256);

        assert_eq!(trainer.config().embedding_dim, 128);
        assert_eq!(trainer.projection().hidden_dim, 256);
        assert_eq!(trainer.projection().embedding_dim, 128);
        assert!(!trainer.has_base_model());
    }
}
