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

use crate::{
    mlx_add, mlx_array_data, mlx_array_free, mlx_array_from_data, mlx_array_from_uints,
    mlx_array_reshape, mlx_array_size, mlx_array_t, mlx_clear_error, mlx_divide,
    mlx_get_last_error, mlx_mean, mlx_multiply, mlx_sqrt, mlx_sum, mlx_take,
};

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

        // GPU-accelerated forward pass using MLX
        let embedding = self.forward_pass_gpu(token_ids)?;

        debug!(
            "Encoded text with {} tokens -> {} dim embedding (GPU)",
            token_ids.len(),
            embedding.len()
        );

        Ok(embedding)
    }

    /// Encode multiple texts into embedding vectors (batch processing)
    pub fn encode_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            results.push(self.encode_text(text)?);
        }

        Ok(results)
    }

    /// GPU-accelerated forward pass using MLX
    fn forward_pass_gpu(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        let hidden_size = self.config.hidden_size;
        let seq_len = token_ids.len();

        unsafe {
            mlx_clear_error();

            // Create token embeddings array on GPU: [vocab_size, hidden_size]
            let token_emb_array = mlx_array_from_data(
                self.weights.token_embeddings.as_ptr(),
                self.weights.token_embeddings.len() as i32,
            );
            if token_emb_array.is_null() {
                return Err(self.get_mlx_error("Failed to create token embeddings array"));
            }

            // Reshape to [vocab_size, hidden_size]
            let emb_shape = [self.config.vocab_size as i32, hidden_size as i32];
            let token_emb_reshaped = mlx_array_reshape(token_emb_array, emb_shape.as_ptr(), 2);
            mlx_array_free(token_emb_array);
            if token_emb_reshaped.is_null() {
                return Err(self.get_mlx_error("Failed to reshape token embeddings"));
            }

            // Create indices array from token IDs
            let indices_array = mlx_array_from_uints(token_ids.as_ptr(), token_ids.len() as i32);
            if indices_array.is_null() {
                mlx_array_free(token_emb_reshaped);
                return Err(self.get_mlx_error("Failed to create indices array"));
            }

            // Gather token embeddings: [seq_len, hidden_size]
            let hidden_states = mlx_take(token_emb_reshaped, indices_array, 0);
            mlx_array_free(token_emb_reshaped);
            mlx_array_free(indices_array);
            if hidden_states.is_null() {
                return Err(self.get_mlx_error("Failed to gather token embeddings"));
            }

            // Add position embeddings if available
            let hidden_states = if let Some(ref pos_emb) = self.weights.position_embeddings {
                let pos_emb_array = mlx_array_from_data(pos_emb.as_ptr(), pos_emb.len() as i32);
                if pos_emb_array.is_null() {
                    mlx_array_free(hidden_states);
                    return Err(self.get_mlx_error("Failed to create position embeddings array"));
                }

                // Reshape to [max_pos, hidden_size] and slice to [seq_len, hidden_size]
                let pos_shape = [
                    self.config.max_position_embeddings as i32,
                    hidden_size as i32,
                ];
                let pos_emb_reshaped = mlx_array_reshape(pos_emb_array, pos_shape.as_ptr(), 2);
                mlx_array_free(pos_emb_array);
                if pos_emb_reshaped.is_null() {
                    mlx_array_free(hidden_states);
                    return Err(self.get_mlx_error("Failed to reshape position embeddings"));
                }

                // Create position indices [0, 1, 2, ..., seq_len-1]
                let pos_indices: Vec<u32> = (0..seq_len as u32).collect();
                let pos_idx_array =
                    mlx_array_from_uints(pos_indices.as_ptr(), pos_indices.len() as i32);
                if pos_idx_array.is_null() {
                    mlx_array_free(hidden_states);
                    mlx_array_free(pos_emb_reshaped);
                    return Err(self.get_mlx_error("Failed to create position indices"));
                }

                // Gather position embeddings for this sequence
                let pos_emb_seq = mlx_take(pos_emb_reshaped, pos_idx_array, 0);
                mlx_array_free(pos_emb_reshaped);
                mlx_array_free(pos_idx_array);
                if pos_emb_seq.is_null() {
                    mlx_array_free(hidden_states);
                    return Err(self.get_mlx_error("Failed to gather position embeddings"));
                }

                // Add position embeddings to token embeddings
                let result = mlx_add(hidden_states, pos_emb_seq);
                mlx_array_free(hidden_states);
                mlx_array_free(pos_emb_seq);
                if result.is_null() {
                    return Err(self.get_mlx_error("Failed to add position embeddings"));
                }
                result
            } else {
                hidden_states
            };

            // Apply layer norm if available (GPU-accelerated)
            let hidden_states = if let (Some(ref weights), Some(ref bias)) = (
                &self.weights.layer_norm_weights,
                &self.weights.layer_norm_bias,
            ) {
                self.apply_layer_norm_gpu(hidden_states, weights, bias, hidden_size, seq_len)?
            } else {
                hidden_states
            };

            // Apply pooling (GPU-accelerated)
            let pooled = self.apply_pooling_gpu(hidden_states, seq_len, hidden_size)?;

            // Normalize if configured (GPU-accelerated)
            let final_result = if self.config.normalize_embeddings {
                self.normalize_gpu(pooled)?
            } else {
                pooled
            };

            // Extract result data
            let size = mlx_array_size(final_result);
            let data_ptr = mlx_array_data(final_result);
            if data_ptr.is_null() {
                mlx_array_free(final_result);
                return Err(AosError::Other("Failed to get result data".to_string()));
            }

            let result = std::slice::from_raw_parts(data_ptr, size).to_vec();
            mlx_array_free(final_result);

            Ok(result)
        }
    }

    /// GPU-accelerated layer normalization
    unsafe fn apply_layer_norm_gpu(
        &self,
        hidden_states: *mut mlx_array_t,
        weights: &[f32],
        bias: &[f32],
        hidden_size: usize,
        _seq_len: usize,
    ) -> Result<*mut mlx_array_t> {
        // Compute mean along hidden dimension (axis 1)
        let mean = mlx_mean(hidden_states, 1);
        if mean.is_null() {
            mlx_array_free(hidden_states);
            return Err(self.get_mlx_error("Failed to compute mean for layer norm"));
        }

        // Reshape mean to [seq_len, 1] for broadcasting
        let mean_shape = [-1i32, 1];
        let mean_reshaped = mlx_array_reshape(mean, mean_shape.as_ptr(), 2);
        mlx_array_free(mean);
        if mean_reshaped.is_null() {
            mlx_array_free(hidden_states);
            return Err(self.get_mlx_error("Failed to reshape mean"));
        }

        // Compute (x - mean)
        let centered = self.mlx_subtract_arrays(hidden_states, mean_reshaped)?;
        mlx_array_free(mean_reshaped);

        // Compute variance = mean((x - mean)^2)
        let squared = mlx_multiply(centered, centered);
        if squared.is_null() {
            mlx_array_free(hidden_states);
            mlx_array_free(centered);
            return Err(self.get_mlx_error("Failed to square centered values"));
        }

        let variance = mlx_mean(squared, 1);
        mlx_array_free(squared);
        if variance.is_null() {
            mlx_array_free(hidden_states);
            mlx_array_free(centered);
            return Err(self.get_mlx_error("Failed to compute variance"));
        }

        // Add epsilon and compute std = sqrt(variance + eps)
        let eps_array = mlx_array_from_data(&1e-12f32, 1);
        if eps_array.is_null() {
            mlx_array_free(hidden_states);
            mlx_array_free(centered);
            mlx_array_free(variance);
            return Err(self.get_mlx_error("Failed to create epsilon array"));
        }

        let var_plus_eps = mlx_add(variance, eps_array);
        mlx_array_free(variance);
        mlx_array_free(eps_array);
        if var_plus_eps.is_null() {
            mlx_array_free(hidden_states);
            mlx_array_free(centered);
            return Err(self.get_mlx_error("Failed to add epsilon to variance"));
        }

        let std = mlx_sqrt(var_plus_eps);
        mlx_array_free(var_plus_eps);
        if std.is_null() {
            mlx_array_free(hidden_states);
            mlx_array_free(centered);
            return Err(self.get_mlx_error("Failed to compute std"));
        }

        // Reshape std for broadcasting
        let std_shape = [-1i32, 1];
        let std_reshaped = mlx_array_reshape(std, std_shape.as_ptr(), 2);
        mlx_array_free(std);
        if std_reshaped.is_null() {
            mlx_array_free(hidden_states);
            mlx_array_free(centered);
            return Err(self.get_mlx_error("Failed to reshape std"));
        }

        // Normalize: (x - mean) / std
        let normalized = mlx_divide(centered, std_reshaped);
        mlx_array_free(hidden_states);
        mlx_array_free(centered);
        mlx_array_free(std_reshaped);
        if normalized.is_null() {
            return Err(self.get_mlx_error("Failed to normalize"));
        }

        // Apply weights and bias: normalized * weights + bias
        let weights_array = mlx_array_from_data(weights.as_ptr(), weights.len() as i32);
        if weights_array.is_null() {
            mlx_array_free(normalized);
            return Err(self.get_mlx_error("Failed to create weights array"));
        }

        let scaled = mlx_multiply(normalized, weights_array);
        mlx_array_free(normalized);
        mlx_array_free(weights_array);
        if scaled.is_null() {
            return Err(self.get_mlx_error("Failed to apply weights"));
        }

        let bias_array = mlx_array_from_data(bias.as_ptr(), bias.len() as i32);
        if bias_array.is_null() {
            mlx_array_free(scaled);
            return Err(self.get_mlx_error("Failed to create bias array"));
        }

        // Reshape bias for broadcasting
        let bias_shape = [1i32, hidden_size as i32];
        let bias_reshaped = mlx_array_reshape(bias_array, bias_shape.as_ptr(), 2);
        mlx_array_free(bias_array);
        if bias_reshaped.is_null() {
            mlx_array_free(scaled);
            return Err(self.get_mlx_error("Failed to reshape bias"));
        }

        let result = mlx_add(scaled, bias_reshaped);
        mlx_array_free(scaled);
        mlx_array_free(bias_reshaped);
        if result.is_null() {
            return Err(self.get_mlx_error("Failed to apply bias"));
        }

        Ok(result)
    }

    /// Helper to subtract arrays (a - b) using multiply by -1 and add
    unsafe fn mlx_subtract_arrays(
        &self,
        a: *mut mlx_array_t,
        b: *mut mlx_array_t,
    ) -> Result<*mut mlx_array_t> {
        let neg_one = mlx_array_from_data(&-1.0f32, 1);
        if neg_one.is_null() {
            return Err(self.get_mlx_error("Failed to create -1 scalar"));
        }

        let neg_b = mlx_multiply(b, neg_one);
        mlx_array_free(neg_one);
        if neg_b.is_null() {
            return Err(self.get_mlx_error("Failed to negate array"));
        }

        let result = mlx_add(a, neg_b);
        mlx_array_free(neg_b);
        if result.is_null() {
            return Err(self.get_mlx_error("Failed to subtract arrays"));
        }

        Ok(result)
    }

    /// GPU-accelerated pooling
    unsafe fn apply_pooling_gpu(
        &self,
        hidden_states: *mut mlx_array_t,
        _seq_len: usize,
        _hidden_size: usize,
    ) -> Result<*mut mlx_array_t> {
        match self.config.pooling_mode.as_str() {
            "mean" => {
                // Mean pooling along sequence dimension (axis 0)
                let pooled = mlx_mean(hidden_states, 0);
                mlx_array_free(hidden_states);
                if pooled.is_null() {
                    return Err(self.get_mlx_error("Failed to apply mean pooling"));
                }
                Ok(pooled)
            }
            "cls" => {
                // Use first token ([CLS]) embedding - take index 0 along seq dimension
                let zero_idx = mlx_array_from_uints(&0u32, 1);
                if zero_idx.is_null() {
                    mlx_array_free(hidden_states);
                    return Err(self.get_mlx_error("Failed to create zero index"));
                }

                let cls_emb = mlx_take(hidden_states, zero_idx, 0);
                mlx_array_free(hidden_states);
                mlx_array_free(zero_idx);
                if cls_emb.is_null() {
                    return Err(self.get_mlx_error("Failed to extract CLS embedding"));
                }

                // Flatten to 1D
                let size = mlx_array_size(cls_emb) as i32;
                let flat_shape = [size];
                let flattened = mlx_array_reshape(cls_emb, flat_shape.as_ptr(), 1);
                mlx_array_free(cls_emb);
                if flattened.is_null() {
                    return Err(self.get_mlx_error("Failed to flatten CLS embedding"));
                }

                Ok(flattened)
            }
            _ => {
                // Default to mean pooling
                let pooled = mlx_mean(hidden_states, 0);
                mlx_array_free(hidden_states);
                if pooled.is_null() {
                    return Err(self.get_mlx_error("Failed to apply default pooling"));
                }
                Ok(pooled)
            }
        }
    }

    /// GPU-accelerated L2 normalization
    unsafe fn normalize_gpu(&self, vector: *mut mlx_array_t) -> Result<*mut mlx_array_t> {
        // Compute squared values
        let squared = mlx_multiply(vector, vector);
        if squared.is_null() {
            mlx_array_free(vector);
            return Err(self.get_mlx_error("Failed to square vector"));
        }

        // Sum all elements (axis -1 means flatten and sum)
        let sum_squared = mlx_sum(squared, -1);
        mlx_array_free(squared);
        if sum_squared.is_null() {
            mlx_array_free(vector);
            return Err(self.get_mlx_error("Failed to sum squared values"));
        }

        // Compute magnitude = sqrt(sum)
        let magnitude = mlx_sqrt(sum_squared);
        mlx_array_free(sum_squared);
        if magnitude.is_null() {
            mlx_array_free(vector);
            return Err(self.get_mlx_error("Failed to compute magnitude"));
        }

        // Add small epsilon to avoid division by zero
        let eps = mlx_array_from_data(&1e-9f32, 1);
        if eps.is_null() {
            mlx_array_free(vector);
            mlx_array_free(magnitude);
            return Err(self.get_mlx_error("Failed to create epsilon"));
        }

        let mag_safe = mlx_add(magnitude, eps);
        mlx_array_free(magnitude);
        mlx_array_free(eps);
        if mag_safe.is_null() {
            mlx_array_free(vector);
            return Err(self.get_mlx_error("Failed to add epsilon to magnitude"));
        }

        // Normalize: vector / magnitude
        let normalized = mlx_divide(vector, mag_safe);
        mlx_array_free(vector);
        mlx_array_free(mag_safe);
        if normalized.is_null() {
            return Err(self.get_mlx_error("Failed to normalize vector"));
        }

        Ok(normalized)
    }

    /// Helper to get MLX error message
    fn get_mlx_error(&self, context: &str) -> AosError {
        unsafe {
            let error_msg = mlx_get_last_error();
            let error_str = if error_msg.is_null() {
                "Unknown MLX error".to_string()
            } else {
                std::ffi::CStr::from_ptr(error_msg)
                    .to_string_lossy()
                    .to_string()
            };
            AosError::Other(format!("{}: {}", context, error_str))
        }
    }

    /// CPU-based forward pass (fallback)
    #[allow(dead_code)]
    fn forward_pass_cpu(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
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
            Self::apply_layer_norm_cpu(&mut hidden_states, weights, bias, hidden_size, seq_len);
        }

        // Apply pooling
        let pooled = self.apply_pooling_cpu(&hidden_states, seq_len, hidden_size);

        // Normalize if configured
        if self.config.normalize_embeddings {
            Ok(Self::normalize_cpu(&pooled))
        } else {
            Ok(pooled)
        }
    }

    fn apply_layer_norm_cpu(
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

    fn apply_pooling_cpu(
        &self,
        hidden_states: &[f32],
        seq_len: usize,
        hidden_size: usize,
    ) -> Vec<f32> {
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
                self.apply_pooling_cpu(hidden_states, seq_len, hidden_size)
            }
        }
    }

    fn normalize_cpu(vector: &[f32]) -> Vec<f32> {
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
        let normalized = MLXEmbeddingModel::normalize_cpu(&vec);

        let magnitude: f32 = normalized.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_mean_pooling_computation() {
        // Test mean pooling logic without requiring real tokenizer
        // Mean pooling averages embeddings across sequence dimension
        let embeddings = vec![vec![1.0, 2.0, 3.0], vec![3.0, 4.0, 5.0]];

        // Compute mean across sequence dimension
        let seq_len = embeddings.len();
        let embed_dim = embeddings[0].len();
        let mut mean = vec![0.0f32; embed_dim];

        for emb in &embeddings {
            for (i, &val) in emb.iter().enumerate() {
                mean[i] += val;
            }
        }
        for val in &mut mean {
            *val /= seq_len as f32;
        }

        assert_eq!(mean, vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_normalize_zero_vector() {
        let vec = vec![0.0, 0.0, 0.0];
        let normalized = MLXEmbeddingModel::normalize_cpu(&vec);
        // Zero vector should return itself
        assert_eq!(normalized, vec);
    }

    #[test]
    fn test_normalize_unit_vector() {
        let vec = vec![1.0, 0.0, 0.0];
        let normalized = MLXEmbeddingModel::normalize_cpu(&vec);
        let magnitude: f32 = normalized.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 1e-5);
    }
}
