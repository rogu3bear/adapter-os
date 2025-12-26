//! Pure Rust model implementation using mlx-lm
//!
//! This module provides a transformer model implementation using mlx-rs and mlx-lm.
//! It replaces the C++ FFI-based model when the `mlx-rs-backend` feature is enabled.

use adapteros_core::{AosError, Result};
use std::path::Path;

#[cfg(feature = "mlx-rs-backend")]
use crate::array::MlxArray;

/// Model configuration compatible with Qwen2.5/Qwen3 models
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MlxRsModelConfig {
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub intermediate_size: usize,
    pub vocab_size: usize,
    pub max_position_embeddings: usize,
    #[serde(default = "default_rope_theta")]
    pub rope_theta: f32,
    #[serde(default)]
    pub rope_scaling: Option<RoPEScaling>,
    #[serde(default = "default_rms_norm_eps")]
    pub rms_norm_eps: f32,
    #[serde(default)]
    pub tie_word_embeddings: bool,
}

fn default_rope_theta() -> f32 {
    10000.0
}

fn default_rms_norm_eps() -> f32 {
    1e-6
}

/// RoPE scaling configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename = "RopeScaling")]
pub struct RoPEScaling {
    #[serde(rename = "type")]
    pub scaling_type: String,
    pub factor: f32,
}

/// Deprecated alias for backwards compatibility
#[deprecated(
    since = "0.12.0",
    note = "Use `RoPEScaling` instead (correct RoPE casing)"
)]
pub type RopeScaling = RoPEScaling;

/// Forward pass output with hidden states
#[cfg(feature = "mlx-rs-backend")]
pub struct ForwardOutput {
    pub logits: Vec<f32>,
    pub hidden_states: std::collections::HashMap<String, Vec<f32>>,
}

/// RoPE cache structure for efficient rotary position embeddings
#[cfg(feature = "mlx-rs-backend")]
pub struct RoPECache {
    cos: MlxArray,
    sin: MlxArray,
}

/// Model health tracking with circuit breaker pattern
#[cfg(feature = "mlx-rs-backend")]
pub struct ModelHealth {
    pub consecutive_failures: u32,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub circuit_open: bool,
    pub last_failure: Option<std::time::Instant>,
}

#[cfg(feature = "mlx-rs-backend")]
impl Default for ModelHealth {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            total_requests: 0,
            failed_requests: 0,
            circuit_open: false,
            last_failure: None,
        }
    }
}

/// Pure Rust MLX model using mlx-lm
#[cfg(feature = "mlx-rs-backend")]
pub struct MlxRsModel {
    /// Model weights as MlxArray tensors
    weights: std::collections::HashMap<String, MlxArray>,
    /// Model configuration
    pub config: MlxRsModelConfig,
    /// Embedding table
    embed_tokens: MlxArray,
    /// LM head (may be tied to embeddings)
    lm_head: Option<MlxArray>,
    /// RoPE cache for rotary position embeddings
    rope_cache: RoPECache,
    /// Health tracking with circuit breaker
    health: std::sync::Mutex<ModelHealth>,
    // Note: Layer norms and other weights are stored in the weights map
}

#[cfg(feature = "mlx-rs-backend")]
impl MlxRsModel {
    /// Initialize RoPE cache for rotary position embeddings
    fn init_rope_cache(config: &MlxRsModelConfig) -> Result<RoPECache> {
        let head_dim = config.hidden_size / config.num_attention_heads;
        let half_dim = head_dim / 2;
        let theta = config.rope_theta;

        let inv_freq: Vec<f32> = (0..half_dim)
            .map(|i| 1.0 / theta.powf(2.0 * i as f32 / head_dim as f32))
            .collect();

        let max_seq = config.max_position_embeddings.min(8192); // Cap for memory
        let mut cos_data = Vec::with_capacity(max_seq * half_dim);
        let mut sin_data = Vec::with_capacity(max_seq * half_dim);

        for pos in 0..max_seq {
            for &freq in &inv_freq {
                let angle = pos as f32 * freq;
                cos_data.push(angle.cos());
                sin_data.push(angle.sin());
            }
        }

        Ok(RoPECache {
            cos: MlxArray::from_slice_f32(&cos_data, &[max_seq as i32, half_dim as i32])?,
            sin: MlxArray::from_slice_f32(&sin_data, &[max_seq as i32, half_dim as i32])?,
        })
    }

    /// Load a model from a directory containing safetensors weights
    pub fn load<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let model_path = model_path.as_ref();

        if !model_path.exists() {
            return Err(AosError::NotFound(format!(
                "Model path does not exist: {}",
                model_path.display()
            )));
        }

        // Load config
        let config_path = model_path.join("config.json");
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| AosError::Io(format!("Failed to read config: {}", e)))?;
        let config: MlxRsModelConfig = serde_json::from_str(&config_str)
            .map_err(|e| AosError::Parse(format!("Failed to parse config: {}", e)))?;

        // Load weights using UnifiedSafeTensorsLoader
        let weights = Self::load_weights(model_path, &config)?;

        // Extract embedding table
        let embed_tokens = weights
            .get("model.embed_tokens.weight")
            .or_else(|| weights.get("embed_tokens.weight"))
            .ok_or_else(|| AosError::NotFound("Embedding weights not found".to_string()))?
            .clone();

        // Extract LM head (may be None if tied to embeddings)
        let lm_head = if config.tie_word_embeddings {
            None
        } else {
            weights
                .get("lm_head.weight")
                .or_else(|| weights.get("model.lm_head.weight"))
                .cloned()
        };

        // Initialize RoPE cache
        let rope_cache = Self::init_rope_cache(&config)?;

        tracing::info!(
            path = %model_path.display(),
            num_layers = config.num_hidden_layers,
            hidden_size = config.hidden_size,
            vocab_size = config.vocab_size,
            num_weights = weights.len(),
            "Loaded MlxRsModel"
        );

        Ok(Self {
            weights,
            config,
            embed_tokens,
            lm_head,
            rope_cache,
            health: std::sync::Mutex::new(ModelHealth::default()),
        })
    }

    /// Load weights from safetensors files
    fn load_weights<P: AsRef<Path>>(
        model_path: P,
        _config: &MlxRsModelConfig,
    ) -> Result<std::collections::HashMap<String, MlxArray>> {
        use crate::unified_loader::{LoadStrategy, UnifiedSafeTensorsLoader};

        let model_path = model_path.as_ref();
        let mut weights = std::collections::HashMap::new();

        // Find all safetensors files
        let safetensors_files: Vec<_> = std::fs::read_dir(model_path)
            .map_err(|e| AosError::Io(format!("Failed to read model directory: {}", e)))?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "safetensors")
                    .unwrap_or(false)
            })
            .collect();

        if safetensors_files.is_empty() {
            return Err(AosError::NotFound(
                "No safetensors files found in model directory".to_string(),
            ));
        }

        // Load each safetensors file
        for entry in safetensors_files {
            let path = entry.path();
            tracing::debug!(path = %path.display(), "Loading safetensors file");

            let loader = UnifiedSafeTensorsLoader::load(&path, LoadStrategy::RustOnly)?;

            for name in loader.tensor_names() {
                match loader.get_tensor_mlx(&name) {
                    Ok(array) => {
                        weights.insert(name.clone(), array);
                    }
                    Err(e) => {
                        tracing::warn!(tensor = %name, error = %e, "Failed to load tensor");
                    }
                }
            }
        }

        tracing::info!(num_tensors = weights.len(), "Loaded all weights");
        Ok(weights)
    }

    /// Run forward pass
    ///
    /// # Arguments
    /// * `token_ids` - Input token IDs
    /// * `position` - Current position in sequence (for KV cache, currently unused)
    ///
    /// # Returns
    /// Logits for next token prediction
    pub fn forward(&self, token_ids: &[u32], _position: usize) -> Result<Vec<f32>> {
        // Convert token IDs to MlxArray
        let token_array = {
            let tokens_i32: Vec<i32> = token_ids.iter().map(|&t| t as i32).collect();
            let shape = vec![1, tokens_i32.len() as i32]; // [batch, seq_len]
            MlxArray::from_slice_i32(&tokens_i32, &shape)?
        };

        // Embedding lookup
        let hidden_states = self.embed_lookup(&token_array)?;

        // Process through transformer layers
        let mut x = hidden_states;
        for layer_idx in 0..self.config.num_hidden_layers {
            x = self.transformer_layer_forward(layer_idx, x)?;
        }

        // Final layer norm
        x = self.final_norm(x)?;

        // LM head projection
        let logits = self.lm_head_forward(x)?;

        // Get last token's logits
        let logits_vec = logits.to_vec_f32()?;
        let vocab_size = self.config.vocab_size;
        let seq_len = token_ids.len();

        // Extract logits for the last position
        let start_idx = (seq_len - 1) * vocab_size;
        let end_idx = start_idx + vocab_size;

        if end_idx > logits_vec.len() {
            return Err(AosError::Internal(format!(
                "Logits size mismatch: expected at least {} elements, got {}",
                end_idx,
                logits_vec.len()
            )));
        }

        Ok(logits_vec[start_idx..end_idx].to_vec())
    }

    /// Embedding lookup
    fn embed_lookup(&self, token_ids: &MlxArray) -> Result<MlxArray> {
        // Use take_axis operation for embedding lookup along axis 0
        // token_ids: [batch, seq_len] -> hidden_states: [batch, seq_len, hidden_size]
        let flat_tokens = token_ids.reshape(&[-1])?; // Flatten to 1D
        let embedded = self.embed_tokens.take_axis(&flat_tokens, 0)?;

        // Reshape back to [batch, seq_len, hidden_size]
        let batch_size = 1i32;
        let seq_len = token_ids.size() as i32;
        let hidden_size = self.config.hidden_size as i32;
        embedded.reshape(&[batch_size, seq_len, hidden_size])
    }

    /// Single transformer layer forward pass
    fn transformer_layer_forward(&self, layer_idx: usize, x: MlxArray) -> Result<MlxArray> {
        // Get layer weights
        let prefix = format!("model.layers.{}", layer_idx);

        // Pre-attention RMSNorm
        let normed = self.rms_norm(&x, &format!("{}.input_layernorm", prefix))?;

        // Self-attention (simplified - full implementation would need KV cache)
        let attn_out = self.self_attention(layer_idx, &normed)?;

        // Residual connection
        let x = x.add(&attn_out)?;

        // Post-attention RMSNorm
        let normed = self.rms_norm(&x, &format!("{}.post_attention_layernorm", prefix))?;

        // MLP
        let mlp_out = self.mlp_forward(layer_idx, &normed)?;

        // Residual connection
        x.add(&mlp_out)
    }

    /// Apply rotary position embeddings to query or key tensors
    fn apply_rope(&self, x: &MlxArray, start_pos: usize) -> Result<MlxArray> {
        let seq_len = x.shape()[1] as usize;
        let half_dim = x.shape()[3] as usize / 2;

        // Split x into two halves along the last dimension
        let (x1, x2) = x.split_at_dim(-1, half_dim)?;

        // Get cos and sin for the current position range
        let cos = self.rope_cache.cos.slice(start_pos, start_pos + seq_len)?;
        let sin = self.rope_cache.sin.slice(start_pos, start_pos + seq_len)?;

        // Broadcast cos/sin to match x shape: [batch, seq, num_heads, half_dim]
        let cos = cos.reshape(&[1, seq_len as i32, 1, half_dim as i32])?;
        let sin = sin.reshape(&[1, seq_len as i32, 1, half_dim as i32])?;

        // Apply rotation: x1 * cos - x2 * sin, x1 * sin + x2 * cos
        let rotated_x1 = x1.mul(&cos)?.sub(&x2.mul(&sin)?)?;
        let rotated_x2 = x1.mul(&sin)?.add(&x2.mul(&cos)?)?;

        // Concatenate back
        MlxArray::concat_axis(&[&rotated_x1, &rotated_x2], -1)
    }

    /// RMSNorm implementation
    fn rms_norm(&self, x: &MlxArray, weight_key: &str) -> Result<MlxArray> {
        let weight = self
            .weights
            .get(&format!("{}.weight", weight_key))
            .ok_or_else(|| {
                AosError::NotFound(format!("RMSNorm weight not found: {}", weight_key))
            })?;

        // Compute RMSNorm: x * weight / sqrt(mean(x^2) + eps)
        let x_squared = x.mul(x)?;
        let mean_squared = x_squared.mean(Some(-1), true)?;

        // Add epsilon for numerical stability
        let eps_array = MlxArray::from_slice_f32(&[self.config.rms_norm_eps], &[1])?;
        let variance = mean_squared.add(&eps_array)?;

        // rsqrt = 1/sqrt(variance)
        let rsqrt = variance.rsqrt()?;

        // Normalize and scale
        let normalized = x.mul(&rsqrt)?;
        normalized.mul(weight)
    }

    /// Create causal attention mask
    /// Returns a mask with shape [1, 1, seq_len, seq_len] where upper triangle is -inf
    #[cfg(feature = "mlx-rs-backend")]
    fn create_causal_mask(&self, seq_len: i32) -> Result<MlxArray> {
        let mut mask_data = vec![0.0f32; (seq_len * seq_len) as usize];
        for i in 0..seq_len {
            for j in (i + 1)..seq_len {
                mask_data[(i * seq_len + j) as usize] = f32::NEG_INFINITY;
            }
        }
        MlxArray::from_slice_f32(&mask_data, &[1, 1, seq_len, seq_len])
    }

    /// Repeat KV heads for GQA (Grouped Query Attention)
    /// Expands [batch, num_heads, seq, head_dim] -> [batch, num_heads * n_rep, seq, head_dim]
    #[cfg(feature = "mlx-rs-backend")]
    fn repeat_kv_heads(&self, x: &MlxArray, n_rep: usize) -> Result<MlxArray> {
        if n_rep == 1 {
            return Ok(x.clone());
        }

        let shape = x.shape();
        // shape: [batch, num_kv_heads, seq, head_dim]
        let expanded = x.expand_dims(3)?; // [batch, num_kv_heads, seq, 1, head_dim]
        let tiled = expanded.tile(&[1, 1, 1, n_rep as i32, 1])?; // [batch, num_kv_heads, seq, n_rep, head_dim]
        tiled.reshape(&[shape[0], shape[1] * n_rep as i32, shape[2], shape[3]])
    }

    /// Self-attention with causal masking and GQA support
    fn self_attention(&self, layer_idx: usize, x: &MlxArray) -> Result<MlxArray> {
        let prefix = format!("model.layers.{}.self_attn", layer_idx);

        // Get projection weights
        let q_proj = self.get_weight(&format!("{}.q_proj.weight", prefix))?;
        let k_proj = self.get_weight(&format!("{}.k_proj.weight", prefix))?;
        let v_proj = self.get_weight(&format!("{}.v_proj.weight", prefix))?;
        let o_proj = self.get_weight(&format!("{}.o_proj.weight", prefix))?;

        // Compute Q, K, V projections
        let q = x.matmul(&q_proj.transpose()?)?;
        let k = x.matmul(&k_proj.transpose()?)?;
        let v = x.matmul(&v_proj.transpose()?)?;

        // Reshape for multi-head attention
        let shape = x.shape();
        let batch_size = shape[0];
        let seq_len = shape[1];
        let num_heads = self.config.num_attention_heads as i32;
        let num_kv_heads = self.config.num_key_value_heads as i32;
        let head_dim = (self.config.hidden_size / self.config.num_attention_heads) as i32;

        // [batch, seq, hidden] -> [batch, seq, num_heads, head_dim]
        let mut q = q.reshape(&[batch_size, seq_len, num_heads, head_dim])?;
        let mut k = k.reshape(&[batch_size, seq_len, num_kv_heads, head_dim])?;
        let v = v.reshape(&[batch_size, seq_len, num_kv_heads, head_dim])?;

        // Apply RoPE to Q and K (position 0 for now, will be updated with KV cache)
        q = self.apply_rope(&q, 0)?;
        k = self.apply_rope(&k, 0)?;

        // Transpose to [batch, num_heads, seq, head_dim]
        let q = q.transpose_axes(&[0, 2, 1, 3])?;
        let mut k = k.transpose_axes(&[0, 2, 1, 3])?;
        let mut v = v.transpose_axes(&[0, 2, 1, 3])?;

        // Expand K and V heads if using GQA
        #[cfg(feature = "mlx-rs-backend")]
        if num_kv_heads < num_heads {
            let n_rep = (num_heads / num_kv_heads) as usize;
            k = self.repeat_kv_heads(&k, n_rep)?;
            v = self.repeat_kv_heads(&v, n_rep)?;
        }

        // Compute attention scores: Q @ K^T / sqrt(d_k)
        let k_t = k.transpose_axes(&[0, 1, 3, 2])?; // Transpose last two dims
        let scores = q.matmul(&k_t)?;

        // Scale
        let scale = 1.0 / (head_dim as f32).sqrt();
        let scale_arr = MlxArray::from_slice_f32(&[scale], &[1])?;
        let scores = scores.mul(&scale_arr)?;

        // Apply causal mask
        #[cfg(feature = "mlx-rs-backend")]
        let scores = {
            let mask = self.create_causal_mask(seq_len)?;
            scores.add(&mask)?
        };

        let attn_weights = scores.softmax(-1)?;

        // Apply attention to values
        let attn_out = attn_weights.matmul(&v)?;

        // Reshape back: [batch, num_heads, seq, head_dim] -> [batch, seq, hidden]
        let attn_out = attn_out.transpose_axes(&[0, 2, 1, 3])?;
        let hidden_size = self.config.hidden_size as i32;
        let attn_out = attn_out.reshape(&[batch_size, seq_len, hidden_size])?;

        // Output projection
        attn_out.matmul(&o_proj.transpose()?)
    }

    /// MLP forward pass (SwiGLU)
    fn mlp_forward(&self, layer_idx: usize, x: &MlxArray) -> Result<MlxArray> {
        let prefix = format!("model.layers.{}.mlp", layer_idx);

        let gate_proj = self.get_weight(&format!("{}.gate_proj.weight", prefix))?;
        let up_proj = self.get_weight(&format!("{}.up_proj.weight", prefix))?;
        let down_proj = self.get_weight(&format!("{}.down_proj.weight", prefix))?;

        // gate = silu(x @ gate_proj.T)
        let gate = x.matmul(&gate_proj.transpose()?)?;
        let gate = gate.silu()?;

        // up = x @ up_proj.T
        let up = x.matmul(&up_proj.transpose()?)?;

        // hidden = gate * up
        let hidden = gate.mul(&up)?;

        // output = hidden @ down_proj.T
        hidden.matmul(&down_proj.transpose()?)
    }

    /// Final layer norm
    fn final_norm(&self, x: MlxArray) -> Result<MlxArray> {
        self.rms_norm(&x, "model.norm")
    }

    /// LM head forward pass
    fn lm_head_forward(&self, x: MlxArray) -> Result<MlxArray> {
        let lm_head = self.lm_head.as_ref().unwrap_or(&self.embed_tokens);
        x.matmul(&lm_head.transpose()?)
    }

    /// Get a weight tensor by name
    fn get_weight(&self, name: &str) -> Result<&MlxArray> {
        self.weights
            .get(name)
            .ok_or_else(|| AosError::NotFound(format!("Weight not found: {}", name)))
    }

    /// Get model configuration
    pub fn config(&self) -> &MlxRsModelConfig {
        &self.config
    }

    /// Get number of layers
    pub fn num_layers(&self) -> usize {
        self.config.num_hidden_layers
    }

    /// Get vocab size
    pub fn vocab_size(&self) -> usize {
        self.config.vocab_size
    }

    /// Check if the model is healthy (circuit breaker not tripped)
    pub fn is_healthy(&self) -> bool {
        let health = self.health.lock().unwrap();
        !health.circuit_open && health.consecutive_failures < 3
    }

    /// Record a successful inference
    pub fn record_success(&self) {
        let mut health = self.health.lock().unwrap();
        health.consecutive_failures = 0;
        health.total_requests += 1;
        if health.circuit_open {
            health.circuit_open = false;
        }
    }

    /// Record a failed inference
    pub fn record_failure(&self) {
        let mut health = self.health.lock().unwrap();
        health.consecutive_failures += 1;
        health.failed_requests += 1;
        health.total_requests += 1;
        health.last_failure = Some(std::time::Instant::now());

        if health.consecutive_failures >= 3 {
            health.circuit_open = true;
        }
    }

    /// Get health statistics
    /// Returns (total_requests, failed_requests, circuit_open)
    pub fn health_stats(&self) -> (u64, u64, bool) {
        let health = self.health.lock().unwrap();
        (
            health.total_requests,
            health.failed_requests,
            health.circuit_open,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        let json = r#"{
            "hidden_size": 896,
            "num_hidden_layers": 24,
            "num_attention_heads": 14,
            "num_key_value_heads": 2,
            "intermediate_size": 4864,
            "vocab_size": 151936,
            "max_position_embeddings": 131072,
            "rope_theta": 1000000.0,
            "rms_norm_eps": 1e-06,
            "tie_word_embeddings": true
        }"#;

        let config: MlxRsModelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.hidden_size, 896);
        assert_eq!(config.num_hidden_layers, 24);
        assert_eq!(config.vocab_size, 151936);
        assert!(config.tie_word_embeddings);
    }
}
