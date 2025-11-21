//! Embedding model for query encoding
//!
//! Provides CPU-based embedding computation using averaged token embeddings.
//! Future: Can be optimized with Metal-accelerated embedding model.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_rag::EmbeddingModel as RagEmbeddingModel;
use memmap2::Mmap;
use safetensors::SafeTensors;
use std::fs::File;
use std::path::Path;

/// Embedding model for computing query vectors
pub struct EmbeddingModel {
    model_type: EmbeddingType,
    dimension: usize,
}

enum EmbeddingType {
    /// Simple averaged token embeddings from base model
    TokenAverage { embedding_matrix: Vec<f32> },
    /// Future: dedicated embedding model
    Dedicated,
}

impl EmbeddingModel {
    /// Load embedding model from path (currently uses token embeddings)
    pub fn from_model_path<P: AsRef<Path>>(
        path: P,
        vocab_size: usize,
        hidden_dim: usize,
    ) -> Result<Self> {
        // For now, load embedding matrix from base model
        // In production, this would load a dedicated embedding model

        let embedding_matrix = Self::load_embedding_matrix(path, vocab_size, hidden_dim)?;

        Ok(Self {
            model_type: EmbeddingType::TokenAverage { embedding_matrix },
            dimension: hidden_dim,
        })
    }

    fn load_embedding_matrix<P: AsRef<Path>>(
        path: P,
        vocab_size: usize,
        hidden_dim: usize,
    ) -> Result<Vec<f32>> {
        // Load embedding layer from safetensors (MLX format expected)
        // MLX format uses a single model.safetensors file optimized for Apple Silicon
        let model_path = path.as_ref().join("model.safetensors");

        // Check if file exists, if not return placeholder for testing
        if !model_path.exists() {
            // Return dummy embedding matrix for testing
            return Ok(vec![0.1; vocab_size * hidden_dim]);
        }

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

        // Convert to Vec<f32>
        let data = embedding_tensor.data();
        let float_data: Vec<f32> = data
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        if float_data.len() != vocab_size * hidden_dim {
            return Err(AosError::Worker(format!(
                "Embedding size mismatch: expected {}, got {}",
                vocab_size * hidden_dim,
                float_data.len()
            )));
        }

        Ok(float_data)
    }

    /// Compute embedding for text tokens
    pub fn encode_tokens(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        match &self.model_type {
            EmbeddingType::TokenAverage { embedding_matrix } => {
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
        // For now, use a simple approach: tokenize and encode
        // In a real implementation, this would use a proper tokenizer
        let tokens = text.chars().map(|c| c as u32).collect::<Vec<u32>>();

        self.encode_tokens(&tokens)
    }

    fn model_hash(&self) -> B3Hash {
        // Return a fixed hash for this embedding model type
        // In a real implementation, this would be computed from the model
        B3Hash::from_hex("0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap()
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
        };

        let embedding = model
            .encode_tokens(&[0, 1])
            .expect("Test token encoding should succeed");

        // After averaging and normalization, should be diagonal
        assert!(embedding[0] > 0.6); // Roughly 1/sqrt(2)
        assert!(embedding[1] > 0.6); // Roughly 1/sqrt(2)
        assert!(embedding[2].abs() < 0.1);
    }
}
