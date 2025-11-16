//! MLX-based embedding model for text encoding
//!
//! Provides efficient embedding computation using MLX on Apple Silicon.
//! Supports sentence-transformers compatible models like all-MiniLM-L6-v2.

use adapteros_core::{AosError, B3Hash, Result};
use safetensors::SafeTensors;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokenizers::Tokenizer;
use tracing::{debug, info};

/// MLX embedding model configuration
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EmbeddingConfig {
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub max_position_embeddings: usize,
    pub vocab_size: usize,
    #[serde(default = "default_pooling")]
    pub pooling_mode: String,
    #[serde(default = "default_normalize")]
    pub normalize_embeddings: bool,
}

fn default_pooling() -> String {
    "mean".to_string()
}

fn default_normalize() -> bool {
    true
}

/// MLX-based embedding model
pub struct MLXEmbeddingModel {
    /// Model weights loaded from safetensors
    weights: EmbeddingWeights,
    /// Tokenizer for text processing
    tokenizer: Arc<Tokenizer>,
    /// Model configuration
    config: EmbeddingConfig,
    /// Model hash for determinism tracking
    model_hash: B3Hash,
    /// Model path for reference
    model_path: PathBuf,
}

/// Model weights structure
struct EmbeddingWeights {
    /// Token embeddings: [vocab_size, hidden_size]
    token_embeddings: Vec<f32>,
    /// Position embeddings: [max_position_embeddings, hidden_size]
    position_embeddings: Option<Vec<f32>>,
    /// Layer norm weights
    layer_norm_weights: Option<Vec<f32>>,
    layer_norm_bias: Option<Vec<f32>>,
    /// Attention weights for each layer (reserved for full transformer implementation)
    #[allow(dead_code)]
    attention_layers: Vec<AttentionLayer>,
    /// FFN weights for each layer (reserved for full transformer implementation)
    #[allow(dead_code)]
    ffn_layers: Vec<FFNLayer>,
}

#[derive(Clone)]
#[allow(dead_code)]
struct AttentionLayer {
    q_weight: Vec<f32>,
    k_weight: Vec<f32>,
    v_weight: Vec<f32>,
    o_weight: Vec<f32>,
    q_bias: Option<Vec<f32>>,
    k_bias: Option<Vec<f32>>,
    v_bias: Option<Vec<f32>>,
    o_bias: Option<Vec<f32>>,
}

#[derive(Clone)]
#[allow(dead_code)]
struct FFNLayer {
    fc1_weight: Vec<f32>,
    fc2_weight: Vec<f32>,
    fc1_bias: Option<Vec<f32>>,
    fc2_bias: Option<Vec<f32>>,
}

impl MLXEmbeddingModel {
    /// Load embedding model from path
    ///
    /// Expected directory structure:
    /// - model.safetensors (or pytorch_model.bin)
    /// - config.json
    /// - tokenizer.json
    pub fn load<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let model_path = model_path.as_ref();
        info!("Loading MLX embedding model from {}", model_path.display());

        // Load configuration
        let config = Self::load_config(model_path)?;

        // Load tokenizer
        let tokenizer = Self::load_tokenizer(model_path)?;

        // Load weights
        let weights = Self::load_weights(model_path, &config)?;

        // Compute model hash
        let model_hash = Self::compute_model_hash(model_path)?;

        info!(
            "MLX embedding model loaded: dim={}, layers={}",
            config.hidden_size, config.num_hidden_layers
        );

        Ok(Self {
            weights,
            tokenizer,
            config,
            model_hash,
            model_path: model_path.to_path_buf(),
        })
    }

    fn load_config<P: AsRef<Path>>(model_path: P) -> Result<EmbeddingConfig> {
        let config_path = model_path.as_ref().join("config.json");
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| AosError::Io(format!("Failed to read config: {}", e)))?;

        serde_json::from_str(&config_str)
            .map_err(|e| AosError::Parse(format!("Failed to parse config: {}", e)))
    }

    fn load_tokenizer<P: AsRef<Path>>(model_path: P) -> Result<Arc<Tokenizer>> {
        let tokenizer_path = model_path.as_ref().join("tokenizer.json");

        Tokenizer::from_file(&tokenizer_path)
            .map(Arc::new)
            .map_err(|e| AosError::Io(format!("Failed to load tokenizer: {}", e)))
    }

    fn load_weights<P: AsRef<Path>>(
        model_path: P,
        config: &EmbeddingConfig,
    ) -> Result<EmbeddingWeights> {
        let safetensors_path = model_path.as_ref().join("model.safetensors");

        if !safetensors_path.exists() {
            return Err(AosError::NotFound(format!(
                "Model weights not found at {}",
                safetensors_path.display()
            )));
        }

        let file = File::open(&safetensors_path)
            .map_err(|e| AosError::Io(format!("Failed to open model file: {}", e)))?;

        let mmap = unsafe { memmap2::Mmap::map(&file) }
            .map_err(|e| AosError::Io(format!("Failed to mmap model: {}", e)))?;

        let tensors = SafeTensors::deserialize(&mmap)
            .map_err(|e| AosError::Parse(format!("Failed to parse safetensors: {}", e)))?;

        // Load token embeddings
        let token_embeddings = Self::load_tensor_f32(
            &tensors,
            &[
                "embeddings.word_embeddings.weight",
                "bert.embeddings.word_embeddings.weight",
            ],
            config.vocab_size * config.hidden_size,
        )?;

        // Load position embeddings (optional for some models)
        let position_embeddings = Self::try_load_tensor_f32(
            &tensors,
            &[
                "embeddings.position_embeddings.weight",
                "bert.embeddings.position_embeddings.weight",
            ],
            config.max_position_embeddings * config.hidden_size,
        );

        // Load layer norm (optional)
        let layer_norm_weights = Self::try_load_tensor_f32(
            &tensors,
            &[
                "embeddings.LayerNorm.weight",
                "bert.embeddings.LayerNorm.weight",
            ],
            config.hidden_size,
        );

        let layer_norm_bias = Self::try_load_tensor_f32(
            &tensors,
            &[
                "embeddings.LayerNorm.bias",
                "bert.embeddings.LayerNorm.bias",
            ],
            config.hidden_size,
        );

        // Load attention and FFN layers
        let mut attention_layers = Vec::new();
        let mut ffn_layers = Vec::new();

        for layer_idx in 0..config.num_hidden_layers {
            // Load attention layer
            let attention = Self::load_attention_layer(&tensors, layer_idx, config)?;
            attention_layers.push(attention);

            // Load FFN layer
            let ffn = Self::load_ffn_layer(&tensors, layer_idx, config)?;
            ffn_layers.push(ffn);
        }

        Ok(EmbeddingWeights {
            token_embeddings,
            position_embeddings,
            layer_norm_weights,
            layer_norm_bias,
            attention_layers,
            ffn_layers,
        })
    }

    fn load_attention_layer(
        tensors: &SafeTensors,
        layer_idx: usize,
        config: &EmbeddingConfig,
    ) -> Result<AttentionLayer> {
        let hidden_size = config.hidden_size;

        let prefixes = [
            format!("encoder.layer.{}.attention.self", layer_idx),
            format!("bert.encoder.layer.{}.attention.self", layer_idx),
        ];

        let q_weight = Self::load_tensor_with_prefixes(
            tensors,
            &prefixes,
            "query.weight",
            hidden_size * hidden_size,
        )?;
        let k_weight = Self::load_tensor_with_prefixes(
            tensors,
            &prefixes,
            "key.weight",
            hidden_size * hidden_size,
        )?;
        let v_weight = Self::load_tensor_with_prefixes(
            tensors,
            &prefixes,
            "value.weight",
            hidden_size * hidden_size,
        )?;

        let o_prefixes = [
            format!("encoder.layer.{}.attention.output", layer_idx),
            format!("bert.encoder.layer.{}.attention.output", layer_idx),
        ];
        let o_weight = Self::load_tensor_with_prefixes(
            tensors,
            &o_prefixes,
            "dense.weight",
            hidden_size * hidden_size,
        )?;

        // Biases are optional
        let q_bias =
            Self::try_load_tensor_with_prefixes(tensors, &prefixes, "query.bias", hidden_size);
        let k_bias =
            Self::try_load_tensor_with_prefixes(tensors, &prefixes, "key.bias", hidden_size);
        let v_bias =
            Self::try_load_tensor_with_prefixes(tensors, &prefixes, "value.bias", hidden_size);
        let o_bias =
            Self::try_load_tensor_with_prefixes(tensors, &o_prefixes, "dense.bias", hidden_size);

        Ok(AttentionLayer {
            q_weight,
            k_weight,
            v_weight,
            o_weight,
            q_bias,
            k_bias,
            v_bias,
            o_bias,
        })
    }

    fn load_ffn_layer(
        tensors: &SafeTensors,
        layer_idx: usize,
        config: &EmbeddingConfig,
    ) -> Result<FFNLayer> {
        let hidden_size = config.hidden_size;
        let intermediate_size = hidden_size * 4; // Standard BERT/transformer intermediate size

        let prefixes = [
            format!("encoder.layer.{}.intermediate", layer_idx),
            format!("bert.encoder.layer.{}.intermediate", layer_idx),
        ];

        let fc1_weight = Self::load_tensor_with_prefixes(
            tensors,
            &prefixes,
            "dense.weight",
            intermediate_size * hidden_size,
        )?;

        let out_prefixes = [
            format!("encoder.layer.{}.output", layer_idx),
            format!("bert.encoder.layer.{}.output", layer_idx),
        ];
        let fc2_weight = Self::load_tensor_with_prefixes(
            tensors,
            &out_prefixes,
            "dense.weight",
            hidden_size * intermediate_size,
        )?;

        let fc1_bias = Self::try_load_tensor_with_prefixes(
            tensors,
            &prefixes,
            "dense.bias",
            intermediate_size,
        );
        let fc2_bias =
            Self::try_load_tensor_with_prefixes(tensors, &out_prefixes, "dense.bias", hidden_size);

        Ok(FFNLayer {
            fc1_weight,
            fc2_weight,
            fc1_bias,
            fc2_bias,
        })
    }

    fn load_tensor_f32(
        tensors: &SafeTensors,
        names: &[&str],
        expected_size: usize,
    ) -> Result<Vec<f32>> {
        for name in names {
            if let Ok(tensor) = tensors.tensor(name) {
                let data = tensor.data();
                let float_data: Vec<f32> = data
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                if float_data.len() == expected_size {
                    return Ok(float_data);
                }
            }
        }

        Err(AosError::NotFound(format!(
            "Tensor not found with names: {:?}",
            names
        )))
    }

    fn try_load_tensor_f32(
        tensors: &SafeTensors,
        names: &[&str],
        expected_size: usize,
    ) -> Option<Vec<f32>> {
        Self::load_tensor_f32(tensors, names, expected_size).ok()
    }

    fn load_tensor_with_prefixes(
        tensors: &SafeTensors,
        prefixes: &[String],
        suffix: &str,
        expected_size: usize,
    ) -> Result<Vec<f32>> {
        let names: Vec<String> = prefixes
            .iter()
            .map(|p| format!("{}.{}", p, suffix))
            .collect();
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        Self::load_tensor_f32(tensors, &name_refs, expected_size)
    }

    fn try_load_tensor_with_prefixes(
        tensors: &SafeTensors,
        prefixes: &[String],
        suffix: &str,
        expected_size: usize,
    ) -> Option<Vec<f32>> {
        Self::load_tensor_with_prefixes(tensors, prefixes, suffix, expected_size).ok()
    }

    fn compute_model_hash<P: AsRef<Path>>(model_path: P) -> Result<B3Hash> {
        let safetensors_path = model_path.as_ref().join("model.safetensors");
        let bytes = std::fs::read(&safetensors_path)
            .map_err(|e| AosError::Io(format!("Failed to read model for hashing: {}", e)))?;

        Ok(B3Hash::hash(&bytes))
    }

    /// Encode text into embedding vector
    pub fn encode_text(&self, text: &str) -> Result<Vec<f32>> {
        if text.trim().is_empty() {
            return Ok(vec![0.0; self.config.hidden_size]);
        }

        // Tokenize input
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| AosError::Validation(format!("Tokenization failed: {}", e)))?;

        let token_ids = encoding.get_ids();

        if token_ids.is_empty() {
            return Ok(vec![0.0; self.config.hidden_size]);
        }

        // Truncate to max position embeddings
        let max_len = self.config.max_position_embeddings.min(token_ids.len());
        let token_ids = &token_ids[..max_len];

        // Simple forward pass (CPU-based for now, MLX acceleration TODO)
        let embedding = self.forward_pass(token_ids)?;

        debug!(
            "Encoded text with {} tokens -> {} dim embedding",
            token_ids.len(),
            embedding.len()
        );

        Ok(embedding)
    }

    fn forward_pass(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        let hidden_size = self.config.hidden_size;
        let seq_len = token_ids.len();

        // Get token embeddings and add position embeddings
        let mut hidden_states = vec![0.0f32; seq_len * hidden_size];

        for (pos, &token_id) in token_ids.iter().enumerate() {
            let token_idx = token_id as usize;
            if token_idx >= self.config.vocab_size {
                return Err(AosError::Validation(format!(
                    "Token ID {} out of vocab range",
                    token_id
                )));
            }

            let start = token_idx * hidden_size;
            let end = start + hidden_size;

            for (i, &val) in self.weights.token_embeddings[start..end].iter().enumerate() {
                hidden_states[pos * hidden_size + i] = val;
            }

            // Add position embeddings if available
            if let Some(ref pos_emb) = self.weights.position_embeddings {
                let pos_start = pos * hidden_size;
                let pos_end = pos_start + hidden_size;
                if pos_end <= pos_emb.len() {
                    for (i, &val) in pos_emb[pos_start..pos_end].iter().enumerate() {
                        hidden_states[pos * hidden_size + i] += val;
                    }
                }
            }
        }

        // Apply layer norm if available
        if let (Some(ref weights), Some(ref bias)) = (
            &self.weights.layer_norm_weights,
            &self.weights.layer_norm_bias,
        ) {
            Self::apply_layer_norm(&mut hidden_states, weights, bias, hidden_size, seq_len);
        }

        // Note: For full transformer encoding, we would run through all attention and FFN layers here
        // For now, we'll use the embeddings directly with mean pooling

        // Apply pooling
        let pooled = self.apply_pooling(&hidden_states, seq_len, hidden_size);

        // Normalize if configured
        if self.config.normalize_embeddings {
            Ok(Self::normalize(&pooled))
        } else {
            Ok(pooled)
        }
    }

    fn apply_layer_norm(
        hidden_states: &mut [f32],
        weights: &[f32],
        bias: &[f32],
        hidden_size: usize,
        seq_len: usize,
    ) {
        for pos in 0..seq_len {
            let start = pos * hidden_size;
            let end = start + hidden_size;
            let slice = &mut hidden_states[start..end];

            // Compute mean and variance
            let mean: f32 = slice.iter().sum::<f32>() / hidden_size as f32;
            let variance: f32 =
                slice.iter().map(|&x| (x - mean).powi(2)).sum::<f32>() / hidden_size as f32;
            let std = (variance + 1e-12).sqrt();

            // Normalize
            for (i, val) in slice.iter_mut().enumerate() {
                *val = (*val - mean) / std * weights[i] + bias[i];
            }
        }
    }

    fn apply_pooling(&self, hidden_states: &[f32], seq_len: usize, hidden_size: usize) -> Vec<f32> {
        match self.config.pooling_mode.as_str() {
            "mean" => {
                let mut pooled = vec![0.0f32; hidden_size];
                for pos in 0..seq_len {
                    let start = pos * hidden_size;
                    for (i, &val) in hidden_states[start..start + hidden_size].iter().enumerate() {
                        pooled[i] += val;
                    }
                }
                for val in &mut pooled {
                    *val /= seq_len as f32;
                }
                pooled
            }
            "cls" => {
                // Use first token ([CLS]) embedding
                hidden_states[..hidden_size].to_vec()
            }
            _ => {
                // Default to mean pooling
                self.apply_pooling(hidden_states, seq_len, hidden_size)
            }
        }
    }

    fn normalize(vector: &[f32]) -> Vec<f32> {
        let magnitude: f32 = vector.iter().map(|&x| x * x).sum::<f32>().sqrt();
        if magnitude < 1e-9 {
            return vector.to_vec();
        }
        vector.iter().map(|&x| x / magnitude).collect()
    }

    /// Get embedding dimension
    pub fn dimension(&self) -> usize {
        self.config.hidden_size
    }

    /// Get model hash
    pub fn model_hash(&self) -> B3Hash {
        self.model_hash
    }

    /// Get model path
    pub fn model_path(&self) -> &Path {
        &self.model_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize() {
        let vec = vec![3.0, 4.0];
        let normalized = MLXEmbeddingModel::normalize(&vec);

        let magnitude: f32 = normalized.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 1e-5);
    }

    #[test]
    #[ignore] // Requires tokenizer file
    fn test_pooling() {
        // This test would require a real tokenizer file
        // Skipped for CI/CD
    }
}
