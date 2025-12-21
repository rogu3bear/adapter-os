//! Model configuration parsing and validation

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Model configuration parsed from config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model name
    pub name: String,

    /// Model architecture identifier
    pub architecture: String,

    /// Hidden dimension size
    pub hidden_size: u32,

    /// Intermediate dimension size (for MLP)
    pub intermediate_size: u32,

    /// Number of transformer layers
    pub num_hidden_layers: u32,

    /// Number of attention heads
    pub num_attention_heads: u32,

    /// Number of key-value heads (for GQA)
    pub num_key_value_heads: u32,

    /// Maximum position embeddings
    pub max_position_embeddings: u32,

    /// RoPE theta parameter
    pub rope_theta: f32,

    /// RoPE scaling factor (optional)
    pub rope_scaling_factor: Option<f32>,

    /// Vocabulary size
    pub vocab_size: u32,

    /// Optional RoPE scaling configuration
    pub rope_scaling: Option<RopeScaling>,

    /// Additional configuration fields
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// RoPE scaling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RopeScaling {
    /// Scaling factor
    pub factor: f32,

    /// Original max position embeddings
    pub original_max_position_embeddings: u32,

    /// Scaling type (e.g., "yarn")
    pub scaling_type: String,
}

/// Derived model dimensions
#[derive(Debug, Clone)]
pub struct ModelDimensions {
    /// Head dimension
    pub head_dim: u32,

    /// Key-value width (for GQA)
    pub kv_width: u32,

    /// Total parameters estimate
    pub total_params: u64,
}

impl ModelConfig {
    /// Parse model config from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        let config: Self = serde_json::from_str(json)
            .map_err(|e| AosError::Plan(format!("Failed to parse model config: {}", e)))?;
        Ok(config)
    }

    /// Load model configuration from JSON file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        let contents = std::fs::read_to_string(path)
            .map_err(|e| AosError::Plan(format!("Failed to read config file: {}", e)))?;

        let config: ModelConfig = serde_json::from_str(&contents)
            .map_err(|e| AosError::Plan(format!("Failed to parse config JSON: {}", e)))?;

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Validate GQA configuration
    pub fn validate_gqa(&self) -> Result<()> {
        // Check that hidden_size is divisible by num_attention_heads
        if !self.hidden_size.is_multiple_of(self.num_attention_heads) {
            return Err(AosError::Plan(format!(
                "hidden_size ({}) must be divisible by num_attention_heads ({})",
                self.hidden_size, self.num_attention_heads
            )));
        }

        // Check that num_key_value_heads divides num_attention_heads
        if !self
            .num_attention_heads
            .is_multiple_of(self.num_key_value_heads)
        {
            return Err(AosError::Plan(format!(
                "num_attention_heads ({}) must be divisible by num_key_value_heads ({})",
                self.num_attention_heads, self.num_key_value_heads
            )));
        }

        Ok(())
    }

    /// Validate model configuration
    pub fn validate(&self) -> Result<()> {
        // Check that hidden_size is divisible by num_attention_heads
        if !self.hidden_size.is_multiple_of(self.num_attention_heads) {
            return Err(AosError::Plan(format!(
                "hidden_size ({}) must be divisible by num_attention_heads ({})",
                self.hidden_size, self.num_attention_heads
            )));
        }

        // Check that num_key_value_heads divides num_attention_heads
        if !self
            .num_attention_heads
            .is_multiple_of(self.num_key_value_heads)
        {
            return Err(AosError::Plan(format!(
                "num_attention_heads ({}) must be divisible by num_key_value_heads ({})",
                self.num_attention_heads, self.num_key_value_heads
            )));
        }

        // Validate RoPE scaling if present
        if let Some(ref scaling) = self.rope_scaling {
            if scaling.factor <= 0.0 {
                return Err(AosError::Plan(format!(
                    "RoPE scaling factor must be positive, got {}",
                    scaling.factor
                )));
            }

            if scaling.original_max_position_embeddings == 0 {
                return Err(AosError::Plan(
                    "RoPE scaling original_max_position_embeddings must be non-zero".to_string(),
                ));
            }
        }

        // Validate vocabulary size
        if self.vocab_size == 0 {
            return Err(AosError::Plan("vocab_size must be non-zero".to_string()));
        }

        Ok(())
    }

    /// Compute derived dimensions
    pub fn dimensions(&self) -> ModelDimensions {
        let head_dim = self.hidden_size / self.num_attention_heads;
        let kv_width = self.num_key_value_heads * head_dim;

        // Rough parameter count estimate
        let params_per_layer =
            // Attention: Q, K, V, O projections
            (self.hidden_size * self.hidden_size) * 4 +
            // MLP: gate, up, down projections  
            (self.hidden_size * self.intermediate_size) * 3;

        let total_params = params_per_layer as u64 * self.num_hidden_layers as u64 +
            // Embedding and output layers
            (self.vocab_size as u64 * self.hidden_size as u64) * 2;

        ModelDimensions {
            head_dim,
            kv_width,
            total_params,
        }
    }

    /// Check if this is a Qwen2.5 model
    pub fn is_qwen2_5(&self) -> bool {
        self.architecture == "Qwen2.5ForCausalLM" || self.architecture.contains("Qwen2.5")
    }

    /// Get effective context length (considering RoPE scaling)
    pub fn effective_context_length(&self) -> u32 {
        match &self.rope_scaling {
            Some(scaling) => {
                (scaling.original_max_position_embeddings as f32 * scaling.factor) as u32
            }
            None => self.max_position_embeddings,
        }
    }
}

/// Calculate LoRA adapter size for given configuration
pub fn calculate_lora_size(config: &ModelConfig, rank: u32, targets: &[String]) -> Result<usize> {
    let mut total_params = 0u64;

    for target in targets {
        let params = match target.as_str() {
            // Attention projections
            "q_proj" | "o_proj" => {
                // Q and O: [hidden_size, hidden_size]
                rank as u64 * (config.hidden_size as u64 + config.hidden_size as u64)
            }
            "k_proj" | "v_proj" => {
                // K and V: [hidden_size, kv_width] for GQA
                let kv_width =
                    config.num_key_value_heads * (config.hidden_size / config.num_attention_heads);
                rank as u64 * (config.hidden_size as u64 + kv_width as u64)
            }
            // MLP projections
            "gate_proj" | "up_proj" => {
                // Gate and Up: [hidden_size, intermediate_size]
                rank as u64 * (config.hidden_size as u64 + config.intermediate_size as u64)
            }
            "down_proj" => {
                // Down: [intermediate_size, hidden_size]
                rank as u64 * (config.intermediate_size as u64 + config.hidden_size as u64)
            }
            _ => {
                return Err(AosError::Plan(format!(
                    "Unknown LoRA target: {}. Valid targets: q_proj, k_proj, v_proj, o_proj, gate_proj, up_proj, down_proj",
                    target
                )));
            }
        };

        total_params += params;
    }

    // Multiply by number of layers
    total_params *= config.num_hidden_layers as u64;

    // Convert to bytes (assuming fp16 = 2 bytes per parameter)
    let bytes = total_params * 2;

    Ok(bytes as usize)
}

/// Validate that all LoRA targets exist in the model
pub fn validate_lora_targets(_config: &ModelConfig, targets: &[String]) -> Result<()> {
    let valid_targets = [
        "q_proj",
        "k_proj",
        "v_proj",
        "o_proj",
        "gate_proj",
        "up_proj",
        "down_proj",
    ];

    for target in targets {
        if !valid_targets.contains(&target.as_str()) {
            return Err(AosError::Plan(format!(
                "Invalid LoRA target: {}. Valid targets: {:?}",
                target, valid_targets
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qwen2_5_config_parsing() {
        let config_json = r#"{
            "architecture": "Qwen2.5ForCausalLM",
            "hidden_size": 3584,
            "intermediate_size": 18944,
            "num_hidden_layers": 28,
            "num_attention_heads": 28,
            "num_key_value_heads": 4,
            "max_position_embeddings": 32768,
            "rope_theta": 1000000.0,
            "vocab_size": 151936
        }"#;

        let config: ModelConfig =
            serde_json::from_str(config_json).expect("Test config JSON should parse");

        assert_eq!(config.hidden_size, 3584);
        assert_eq!(config.num_attention_heads, 28);
        assert_eq!(config.num_key_value_heads, 4);
        assert!(config.is_qwen2_5());

        let dims = config.dimensions();
        assert_eq!(dims.head_dim, 128); // 3584 / 28
        assert_eq!(dims.kv_width, 512); // 4 * 128
    }

    #[test]
    fn test_lora_size_calculation() {
        let config_json = r#"{
            "architecture": "Qwen2.5ForCausalLM",
            "hidden_size": 3584,
            "intermediate_size": 18944,
            "num_hidden_layers": 28,
            "num_attention_heads": 28,
            "num_key_value_heads": 4,
            "max_position_embeddings": 32768,
            "rope_theta": 1000000.0,
            "vocab_size": 151936
        }"#;

        let config: ModelConfig =
            serde_json::from_str(config_json).expect("Test config JSON should parse");

        let targets = vec![
            "q_proj".to_string(),
            "k_proj".to_string(),
            "v_proj".to_string(),
            "o_proj".to_string(),
            "gate_proj".to_string(),
            "up_proj".to_string(),
            "down_proj".to_string(),
        ];

        let size_bytes = calculate_lora_size(&config, 16, &targets)
            .expect("LoRA size calculation should succeed");
        let size_mb = size_bytes / (1024 * 1024);

        // Expected: ~77 MB for rank 16 across all targets
        assert!(
            (70..=85).contains(&size_mb),
            "LoRA size should be ~77 MB, got {} MB",
            size_mb
        );
    }

    #[test]
    fn test_config_validation() {
        let config_json = r#"{
            "architecture": "Qwen2.5ForCausalLM",
            "hidden_size": 3584,
            "intermediate_size": 18944,
            "num_hidden_layers": 28,
            "num_attention_heads": 28,
            "num_key_value_heads": 4,
            "max_position_embeddings": 32768,
            "rope_theta": 1000000.0,
            "vocab_size": 151936
        }"#;

        // Valid config should pass
        let config: ModelConfig =
            serde_json::from_str(config_json).expect("Test config JSON should parse");
        assert!(config.validate().is_ok());

        // Invalid config: hidden_size not divisible by num_attention_heads
        let invalid_json = r#"{
            "architecture": "Qwen2.5ForCausalLM",
            "hidden_size": 3585,
            "intermediate_size": 18944,
            "num_hidden_layers": 28,
            "num_attention_heads": 28,
            "num_key_value_heads": 4,
            "max_position_embeddings": 32768,
            "rope_theta": 1000000.0,
            "vocab_size": 151936
        }"#;

        let invalid_config: ModelConfig = serde_json::from_str(invalid_json)
            .expect("Test invalid config JSON should parse (will fail validation)");
        assert!(invalid_config.validate().is_err());
    }
}
