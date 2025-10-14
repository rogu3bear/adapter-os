//! Mock implementations for testing

use crate::{LoRAAdapter, LoRAConfig, ModelConfig};
use adapteros_core::Result;
use std::collections::HashMap;

/// Mock MLX FFI model for testing
pub struct MockMLXFFIModel {
    config: ModelConfig,
}

impl MockMLXFFIModel {
    /// Create a mock model
    pub fn new(config: ModelConfig) -> Self {
        Self { config }
    }

    /// Mock forward pass
    pub fn forward(&self, token_ids: &[u32], _position: usize) -> Result<Vec<f32>> {
        // Return mock logits based on input
        let vocab_size = self.config.vocab_size;
        let mut logits = vec![0.0; vocab_size];

        // Set some mock values
        for &token_id in token_ids {
            if (token_id as usize) < vocab_size {
                logits[token_id as usize] = 1.0;
            }
        }

        Ok(logits)
    }

    /// Mock forward with hidden states
    pub fn forward_with_hidden_states(
        &self,
        token_ids: &[u32],
    ) -> Result<(Vec<f32>, HashMap<String, Vec<f32>>)> {
        let logits = self.forward(token_ids, 0)?;
        let mut hidden_states = HashMap::new();

        // Add mock hidden states
        hidden_states.insert("q_proj".to_string(), vec![1.0; 128]);
        hidden_states.insert("k_proj".to_string(), vec![2.0; 128]);
        hidden_states.insert("v_proj".to_string(), vec![3.0; 128]);
        hidden_states.insert("o_proj".to_string(), vec![4.0; 128]);

        Ok((logits, hidden_states))
    }

    /// Get config
    pub fn config(&self) -> &ModelConfig {
        &self.config
    }
}

/// Create a mock LoRA adapter
pub fn create_mock_adapter(id: &str, rank: usize) -> LoRAAdapter {
    let config = LoRAConfig {
        rank,
        alpha: 16.0,
        target_modules: vec![
            "q_proj".to_string(),
            "k_proj".to_string(),
            "v_proj".to_string(),
            "o_proj".to_string(),
        ],
        dropout: 0.1,
    };

    let mut adapter = LoRAAdapter::new(id.to_string(), config);

    // Add mock weights for each target module
    let target_modules = adapter.config().target_modules.clone();
    for module_name in &target_modules {
        let lora_a = vec![vec![1.0; 128]; rank];
        let lora_b = vec![vec![2.0; rank]; 128];
        adapter.add_module_weights(module_name, lora_a, lora_b);
    }

    adapter
}

/// Create a mock model config
pub fn create_mock_config() -> ModelConfig {
    ModelConfig {
        hidden_size: 4096,
        num_hidden_layers: 32,
        num_attention_heads: 32,
        num_key_value_heads: 8,
        intermediate_size: 11008,
        vocab_size: 32000,
        max_position_embeddings: 32768,
        rope_theta: 10000.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_model() {
        let config = create_mock_config();
        let model = MockMLXFFIModel::new(config);

        let token_ids = vec![1, 2, 3];
        let logits = model.forward(&token_ids, 0).unwrap();

        assert_eq!(logits.len(), 32000);
        assert!(logits[1] > 0.0);
        assert!(logits[2] > 0.0);
        assert!(logits[3] > 0.0);
    }

    #[test]
    fn test_mock_adapter() {
        let adapter = create_mock_adapter("test", 4);

        assert_eq!(adapter.id(), "test");
        assert_eq!(adapter.config().rank, 4);
        assert_eq!(adapter.config().target_modules.len(), 4);
        assert!(adapter.has_module("q_proj"));
        assert!(adapter.has_module("k_proj"));
        assert!(adapter.has_module("v_proj"));
        assert!(adapter.has_module("o_proj"));
    }

    #[test]
    fn test_mock_config() {
        let config = create_mock_config();

        assert_eq!(config.hidden_size, 4096);
        assert_eq!(config.num_hidden_layers, 32);
        assert_eq!(config.vocab_size, 32000);
        assert_eq!(config.rope_theta, 10000.0);
    }
}
