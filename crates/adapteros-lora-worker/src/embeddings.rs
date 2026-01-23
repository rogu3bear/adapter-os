//! Embedding model for query encoding
//!
//! Provides CPU-based embedding computation using averaged token embeddings.
//! Future: Can be optimized with Metal-accelerated embedding model.

use crate::tokenizer::QwenTokenizer;
use adapteros_config::{resolve_embedding_model_path, PathSource};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_rag::EmbeddingModel as RagEmbeddingModel;
use memmap2::Mmap;
use safetensors::SafeTensors;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Embedding model for computing query vectors
pub struct EmbeddingModel {
    model_type: EmbeddingType,
    dimension: usize,
    /// Optional tokenizer for proper text encoding.
    /// When present, encode_text uses BPE tokenization.
    /// When absent, falls back to char codes (degraded accuracy).
    tokenizer: Option<Arc<QwenTokenizer>>,
}

/// Embedding model type
///
/// Currently only TokenAverage is implemented. Dedicated embedding model support
/// is reserved for future high-performance semantic search capabilities.
#[allow(dead_code)] // Dedicated variant reserved for future implementation
enum EmbeddingType {
    /// Simple averaged token embeddings from base model
    TokenAverage { embedding_matrix: Vec<f32> },
    /// Future: dedicated embedding model
    Dedicated,
}

impl EmbeddingModel {
    /// Load embedding model with tokenizer for proper text encoding.
    ///
    /// This is the preferred constructor - it enables accurate BPE tokenization
    /// for RAG queries and semantic search.
    pub fn from_model_path_with_tokenizer<P: AsRef<Path>>(
        path: P,
        vocab_size: usize,
        hidden_dim: usize,
        tokenizer: Arc<QwenTokenizer>,
    ) -> Result<Self> {
        let embedding_matrix = Self::load_embedding_matrix(path, vocab_size, hidden_dim)?;

        Ok(Self {
            model_type: EmbeddingType::TokenAverage { embedding_matrix },
            dimension: hidden_dim,
            tokenizer: Some(tokenizer),
        })
    }

    /// Load embedding model from path without tokenizer (legacy).
    ///
    /// **Warning**: Without a tokenizer, `encode_text` falls back to using
    /// character codes instead of proper BPE tokenization, resulting in
    /// semantically invalid embeddings. Use `from_model_path_with_tokenizer`
    /// when possible.
    pub fn from_model_path<P: AsRef<Path>>(
        path: P,
        vocab_size: usize,
        hidden_dim: usize,
    ) -> Result<Self> {
        tracing::warn!(
            "EmbeddingModel created without tokenizer - encode_text will use char codes (degraded accuracy)"
        );

        let embedding_matrix = Self::load_embedding_matrix(path, vocab_size, hidden_dim)?;

        Ok(Self {
            model_type: EmbeddingType::TokenAverage { embedding_matrix },
            dimension: hidden_dim,
            tokenizer: None,
        })
    }

    fn load_embedding_matrix<P: AsRef<Path>>(
        path: P,
        vocab_size: usize,
        hidden_dim: usize,
    ) -> Result<Vec<f32>> {
        let resolve_safetensors_path = |base_path: &Path| -> Result<Option<PathBuf>> {
            let single_model_path = base_path.join("model.safetensors");
            if single_model_path.exists() {
                return Ok(Some(single_model_path));
            }

            let index_path = base_path.join("model.safetensors.index.json");
            if index_path.exists() {
                // Parse the sharded model index to find the embeddings shard
                let index_content = std::fs::read_to_string(&index_path)
                    .map_err(|e| AosError::Worker(format!("Failed to read index file: {}", e)))?;
                let index: serde_json::Value = serde_json::from_str(&index_content)
                    .map_err(|e| AosError::Worker(format!("Failed to parse index JSON: {}", e)))?;

                // Look for embedding tensor in weight_map
                let weight_map = index
                    .get("weight_map")
                    .ok_or_else(|| AosError::Worker("Index missing weight_map".into()))?;

                let shard_file = weight_map
                    .get("model.embed_tokens.weight")
                    .or_else(|| weight_map.get("transformer.wte.weight"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AosError::Worker("Could not find embedding tensor in index".into())
                    })?;

                tracing::info!(shard = %shard_file, "Loading embeddings from sharded model");
                return Ok(Some(base_path.join(shard_file)));
            }

            Ok(None)
        };

        // Load embedding layer from safetensors
        // Supports both single file (model.safetensors) and sharded models (model-XXXXX-of-YYYYY.safetensors)
        let mut base_path = path.as_ref().to_path_buf();
        let mut model_path = resolve_safetensors_path(&base_path)?;

        if model_path.is_none() {
            if let Ok(resolved) = resolve_embedding_model_path() {
                let candidate_path = resolved.path;
                if candidate_path != base_path {
                    if let Some(candidate_model) = resolve_safetensors_path(&candidate_path)? {
                        tracing::info!(
                            path = %candidate_path.display(),
                            source = %resolved.source,
                            "Using embedding model override"
                        );
                        base_path = candidate_path;
                        model_path = Some(candidate_model);
                    } else if !matches!(resolved.source, PathSource::Default(_)) {
                        return Err(AosError::Worker(format!(
                            "Embedding model override {} missing model.safetensors or model.safetensors.index.json",
                            candidate_path.display()
                        )));
                    }
                }
            }
        }

        let model_path = match model_path {
            Some(path) => path,
            None => {
                // No model found - RAG operations require real embeddings
                tracing::error!(
                    "No model.safetensors or model.safetensors.index.json found in {:?}. \
                     RAG and semantic operations require a valid embedding model.",
                    base_path
                );
                return Err(AosError::Worker(
                    "Embedding model not found. Set AOS_EMBEDDING_MODEL_PATH to a safetensors directory for RAG/semantic operations."
                        .into(),
                ));
            }
        };

        let file = File::open(&model_path)
            .map_err(|e| AosError::Worker(format!("Failed to open model: {}", e)))?;

        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| AosError::Worker(format!("Failed to mmap model: {}", e)))?;

        let tensors = SafeTensors::deserialize(&mmap)
            .map_err(|e| AosError::Worker(format!("Failed to parse safetensors: {}", e)))?;

        // Look for embedding tensor (model-specific name)
        let embedding_tensor = tensors
            .tensor("model.embed_tokens.weight")
            .or_else(|_| tensors.tensor("transformer.wte.weight"))
            .map_err(|e| AosError::Worker(format!("Embedding tensor not found: {}", e)))?;

        // Detect if this is a quantized model by checking tensor dtype or size
        let data = embedding_tensor.data();
        let expected_fp32_size = vocab_size * hidden_dim * 4; // 4 bytes per f32

        // Check if model is quantized (data size much smaller than expected fp32 size)
        if data.len() < expected_fp32_size / 4 {
            // This is likely a quantized model (4-bit or 8-bit)
            // For quantized models, we need to dequantize the embeddings
            // Check for MLX quantized format with scales and biases

            // MLX 4-bit quantization packs weights with group_size=64
            // Try to load scales tensor if available
            let scales_result = tensors.tensor("model.embed_tokens.scales");
            let biases_result = tensors.tensor("model.embed_tokens.biases");

            if let (Ok(scales_tensor), Ok(biases_tensor)) = (scales_result, biases_result) {
                // Dequantize using MLX format: dequantized = scales * (packed_weights - biases)
                return Self::dequantize_mlx_4bit(
                    data,
                    scales_tensor.data(),
                    biases_tensor.data(),
                    vocab_size,
                    hidden_dim,
                    64, // MLX default group_size
                );
            }

            // If no scales/biases found, use mean-initialized embeddings as fallback
            // This allows the worker to start while RAG functionality is degraded
            tracing::warn!(
                "Quantized model detected without dequantization metadata. \
                 Using initialized embeddings for RAG. For full accuracy, use non-quantized model."
            );
            return Ok(Self::init_random_embeddings(vocab_size, hidden_dim));
        }

        // Standard fp32, fp16, or bf16 model
        let float_data: Vec<f32> = if data.len() == vocab_size * hidden_dim * 4 {
            // fp32 format: 4 bytes per element
            data.chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()
        } else if data.len() == vocab_size * hidden_dim * 2 {
            // fp16/bf16 format: 2 bytes per element
            // Detect bf16 vs fp16 by checking the tensor dtype from safetensors
            // bf16: exponent bits are like f32 (8 bits), mantissa is truncated (7 bits)
            // fp16: exponent is 5 bits, mantissa is 10 bits
            // We'll use bf16 interpretation as it's more common for modern models
            data.chunks_exact(2)
                .map(|chunk| {
                    let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                    // Convert bf16 to f32 by shifting left 16 bits (bf16 is top 16 bits of f32)
                    let f32_bits = (bits as u32) << 16;
                    f32::from_bits(f32_bits)
                })
                .collect()
        } else {
            return Err(AosError::Worker(format!(
                "Embedding size mismatch: expected {} (fp32) or {} (fp16/bf16) bytes, got {}",
                expected_fp32_size,
                vocab_size * hidden_dim * 2,
                data.len()
            )));
        };

        if float_data.len() != vocab_size * hidden_dim {
            return Err(AosError::Worker(format!(
                "Embedding count mismatch: expected {}, got {}",
                vocab_size * hidden_dim,
                float_data.len()
            )));
        }

        Ok(float_data)
    }

    /// Dequantize MLX 4-bit quantized embeddings
    fn dequantize_mlx_4bit(
        packed_data: &[u8],
        scales_data: &[u8],
        biases_data: &[u8],
        vocab_size: usize,
        hidden_dim: usize,
        group_size: usize,
    ) -> Result<Vec<f32>> {
        let num_groups = hidden_dim / group_size;
        let mut result = Vec::with_capacity(vocab_size * hidden_dim);

        // Parse scales and biases as fp16
        let scales: Vec<f32> = scales_data
            .chunks_exact(2)
            .map(|chunk| {
                let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                half::f16::from_bits(bits).to_f32()
            })
            .collect();

        let biases: Vec<f32> = biases_data
            .chunks_exact(2)
            .map(|chunk| {
                let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                half::f16::from_bits(bits).to_f32()
            })
            .collect();

        // 4-bit packing: 2 values per byte
        let packed_per_row = hidden_dim / 2;

        for vocab_idx in 0..vocab_size {
            let row_offset = vocab_idx * packed_per_row;
            let scale_offset = vocab_idx * num_groups;

            for group_idx in 0..num_groups {
                let scale = scales.get(scale_offset + group_idx).copied().unwrap_or(1.0);
                let bias = biases.get(scale_offset + group_idx).copied().unwrap_or(0.0);

                for elem_in_group in 0..group_size {
                    let elem_idx = group_idx * group_size + elem_in_group;
                    let packed_idx = row_offset + elem_idx / 2;

                    if packed_idx >= packed_data.len() {
                        result.push(0.0);
                        continue;
                    }

                    let packed_byte = packed_data[packed_idx];
                    let value = if elem_idx.is_multiple_of(2) {
                        (packed_byte & 0x0F) as f32
                    } else {
                        ((packed_byte >> 4) & 0x0F) as f32
                    };

                    // Dequantize: scale * (value - 8) + bias (center around 8 for 4-bit)
                    let dequantized = scale * (value - 8.0) + bias;
                    result.push(dequantized);
                }
            }
        }

        Ok(result)
    }

    /// Initialize random embeddings for when model is quantized without metadata
    fn init_random_embeddings(vocab_size: usize, hidden_dim: usize) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut result = Vec::with_capacity(vocab_size * hidden_dim);

        // Use deterministic pseudo-random initialization based on vocab position
        for vocab_idx in 0..vocab_size {
            for dim_idx in 0..hidden_dim {
                let mut hasher = DefaultHasher::new();
                (vocab_idx, dim_idx).hash(&mut hasher);
                let hash = hasher.finish();

                // Convert hash to float in range [-0.02, 0.02] (typical embedding init range)
                let normalized = (hash as f64 / u64::MAX as f64) * 0.04 - 0.02;
                result.push(normalized as f32);
            }
        }

        result
    }

    /// Compute embedding for text tokens
    pub fn encode_tokens(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        match &self.model_type {
            EmbeddingType::TokenAverage { embedding_matrix } => {
                // Guard: empty tokens would cause division by zero
                if token_ids.is_empty() {
                    return Ok(vec![0.0f32; self.dimension]);
                }

                // Average token embeddings
                let mut result = vec![0.0f32; self.dimension];

                for &token_id in token_ids {
                    let start_idx = (token_id as usize) * self.dimension;
                    let end_idx = start_idx + self.dimension;

                    if end_idx > embedding_matrix.len() {
                        return Err(AosError::Worker(format!(
                            "Token ID {} out of bounds",
                            token_id
                        )));
                    }

                    for (i, val) in embedding_matrix[start_idx..end_idx].iter().enumerate() {
                        result[i] += val;
                    }
                }

                // Normalize by number of tokens
                let norm_factor = 1.0 / (token_ids.len() as f32);
                for val in &mut result {
                    *val *= norm_factor;
                }

                // L2 normalize
                let l2_norm: f32 = result.iter().map(|x| x * x).sum::<f32>().sqrt();
                if l2_norm > 0.0 {
                    for val in &mut result {
                        *val /= l2_norm;
                    }
                }

                Ok(result)
            }
            EmbeddingType::Dedicated => {
                // Future: run dedicated embedding model
                Err(AosError::Worker(
                    "Dedicated embedding model not yet implemented".to_string(),
                ))
            }
        }
    }

    /// Get embedding dimension
    pub fn dimension(&self) -> usize {
        self.dimension
    }
}

impl RagEmbeddingModel for EmbeddingModel {
    fn encode_text(&self, text: &str) -> Result<Vec<f32>> {
        let token_ids = match &self.tokenizer {
            Some(tokenizer) => {
                // Proper BPE tokenization using the model's tokenizer
                tokenizer.encode(text)?
            }
            None => {
                // Fallback: char codes (degraded accuracy, kept for backwards compat)
                // This produces semantically invalid embeddings but allows startup
                text.chars().map(|c| c as u32).collect()
            }
        };

        self.encode_tokens(&token_ids)
    }

    fn model_hash(&self) -> B3Hash {
        // Compute hash from embedding matrix for determinism validation
        match &self.model_type {
            EmbeddingType::TokenAverage { embedding_matrix } => {
                // Hash first 1MB of embedding data for fingerprint
                let bytes_to_hash =
                    std::cmp::min(embedding_matrix.len() * std::mem::size_of::<f32>(), 1024 * 1024);
                let byte_slice = unsafe {
                    std::slice::from_raw_parts(
                        embedding_matrix.as_ptr() as *const u8,
                        bytes_to_hash,
                    )
                };
                let hash = blake3::hash(byte_slice);
                B3Hash::from_bytes(*hash.as_bytes())
            }
            EmbeddingType::Dedicated => {
                // Future: compute from dedicated model
                B3Hash::from_hex(
                    "0000000000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap()
            }
        }
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_normalization() {
        let model = EmbeddingModel {
            model_type: EmbeddingType::TokenAverage {
                embedding_matrix: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], // 2 tokens, dim=3
            },
            dimension: 3,
            tokenizer: None,
        };

        let embedding = model
            .encode_tokens(&[0, 1])
            .expect("Test token encoding should succeed");

        // Check L2 norm is 1.0
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_embedding_average() {
        let model = EmbeddingModel {
            model_type: EmbeddingType::TokenAverage {
                embedding_matrix: vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0], // 2 tokens, dim=3
            },
            dimension: 3,
            tokenizer: None,
        };

        let embedding = model
            .encode_tokens(&[0, 1])
            .expect("Test token encoding should succeed");

        // After averaging and normalization, should be diagonal
        assert!(embedding[0] > 0.6); // Roughly 1/sqrt(2)
        assert!(embedding[1] > 0.6); // Roughly 1/sqrt(2)
        assert!(embedding[2].abs() < 0.1);
    }

    #[test]
    fn test_encode_text_without_tokenizer_uses_char_codes() {
        // Create model with enough vocab to handle char codes
        let vocab_size = 256;
        let dim = 4;
        let mut embedding_matrix = vec![0.0; vocab_size * dim];

        // Set specific embeddings for 'a' (97) and 'b' (98)
        let a_idx = 97 * dim;
        embedding_matrix[a_idx] = 1.0;
        embedding_matrix[a_idx + 1] = 0.0;
        embedding_matrix[a_idx + 2] = 0.0;
        embedding_matrix[a_idx + 3] = 0.0;

        let b_idx = 98 * dim;
        embedding_matrix[b_idx] = 0.0;
        embedding_matrix[b_idx + 1] = 1.0;
        embedding_matrix[b_idx + 2] = 0.0;
        embedding_matrix[b_idx + 3] = 0.0;

        let model = EmbeddingModel {
            model_type: EmbeddingType::TokenAverage { embedding_matrix },
            dimension: dim,
            tokenizer: None,
        };

        // encode_text without tokenizer should use char codes
        let embedding = model.encode_text("ab").expect("Encoding should succeed");

        // Should be average of [1,0,0,0] and [0,1,0,0] = [0.5, 0.5, 0, 0], normalized
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "Embedding should be L2 normalized");
        assert!(embedding[0] > 0.6, "First component should be ~0.707");
        assert!(embedding[1] > 0.6, "Second component should be ~0.707");
    }

    #[test]
    fn test_model_hash_computes_blake3() {
        let embedding_matrix = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let model = EmbeddingModel {
            model_type: EmbeddingType::TokenAverage {
                embedding_matrix: embedding_matrix.clone(),
            },
            dimension: 3,
            tokenizer: None,
        };

        let hash = model.model_hash();

        // Hash should not be all zeros (that was the old broken behavior)
        let zero_hash = B3Hash::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        assert_ne!(hash, zero_hash, "model_hash should compute real BLAKE3 hash");

        // Same model should produce same hash (determinism)
        let hash2 = model.model_hash();
        assert_eq!(hash, hash2, "model_hash should be deterministic");
    }
}
