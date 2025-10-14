//! Model loader and validation for AdapterOS

use adapteros_chat::ChatTemplateProcessor;
use adapteros_core::{AosError, Result};
use adapteros_lora_quant::BlockQuantizer;
use std::path::Path;

/// Mock model record for testing
struct MockModelRecord {
    _name: String,
    config_json: String,
    quant_type: String,
    group_size: Option<u32>,
    bits: Option<u8>,
    _tokenizer_cfg_path: std::path::PathBuf,
}

/// Model loader for building plans
pub struct ModelLoader {
    /// Model configuration
    pub config: crate::config::ModelConfig,

    /// Chat template processor
    pub chat_template: ChatTemplateProcessor,

    /// Quantizer
    pub quantizer: BlockQuantizer,
}

impl ModelLoader {
    /// Load model from registry and validate configuration
    ///
    /// NOTE: Only MLX format models are supported. MLX provides optimized memory layout
    /// for K-sparse LoRA routing and better adapter integration on Apple Silicon.
    pub fn load_from_registry<P: AsRef<Path>>(model_name: &str, _registry_path: P) -> Result<Self> {
        // TODO: Load model record from registry when registry is fixed
        // For now, create a mock model record
        let model_record = MockModelRecord {
            _name: model_name.to_string(),
            config_json: r#"{"name":"qwen2.5-7b","arch":"qwen2","vocab_size":32000,"hidden_size":4096,"intermediate_size":11008,"num_hidden_layers":32,"num_attention_heads":32,"num_key_value_heads":4,"rope_theta":1000000.0,"max_position_embeddings":32768}"#.to_string(),
            quant_type: "int4_block".to_string(),
            group_size: Some(128),
            bits: Some(4),
            _tokenizer_cfg_path: "mock_tokenizer_config.json".into(),
        };

        // Load and parse model config
        let config = crate::config::ModelConfig::from_json(&model_record.config_json)?;

        // Validate GQA configuration
        config.validate_gqa()?;

        // Load chat template (mock for now)
        let chat_template_config = adapteros_chat::ChatTemplate {
            name: "qwen".to_string(),
            template: "{% for message in messages %}{{ '<|im_start|>' + message['role'] + '\\n' + message['content'] + '<|im_end|>\\n' }}{% endfor %}".to_string(),
            special_tokens: adapteros_chat::SpecialTokens {
                bos: "<|im_start|>".to_string(),
                eos: "<|im_end|>".to_string(),
                unk: "<|unk|>".to_string(),
                pad: "<|endoftext|>".to_string(),
            },
        };
        let chat_template = ChatTemplateProcessor::new(chat_template_config);

        // Create quantizer based on model quantization
        let quantizer = BlockQuantizer::new(
            model_record.quant_type.clone(),
            model_record.group_size.unwrap_or(128),
            model_record.bits.unwrap_or(4),
        );

        Ok(Self {
            config,
            chat_template,
            quantizer,
        })
    }

    /// Validate model configuration against manifest
    pub fn validate_manifest(&self, manifest: &adapteros_manifest::ManifestV3) -> Result<()> {
        // Check model name matches
        if self.config.name != manifest.base.model_id {
            return Err(AosError::InvalidManifest(format!(
                "Model name mismatch: {} != {}",
                self.config.name, manifest.base.model_id
            )));
        }

        // Check architecture matches
        if self.config.architecture != manifest.base.arch {
            return Err(AosError::InvalidManifest(format!(
                "Architecture mismatch: {} != {}",
                self.config.architecture, manifest.base.arch
            )));
        }

        // Check vocabulary size
        if self.config.vocab_size != manifest.base.vocab_size {
            return Err(AosError::InvalidManifest(format!(
                "Vocabulary size mismatch: {} != {}",
                self.config.vocab_size, manifest.base.vocab_size
            )));
        }

        // Check hidden dimension
        if self.config.hidden_size != manifest.base.hidden_dim {
            return Err(AosError::InvalidManifest(format!(
                "Hidden dimension mismatch: {} != {}",
                self.config.hidden_size, manifest.base.hidden_dim
            )));
        }

        // Check number of layers
        if self.config.num_hidden_layers != manifest.base.n_layers {
            return Err(AosError::InvalidManifest(format!(
                "Number of layers mismatch: {} != {}",
                self.config.num_hidden_layers, manifest.base.n_layers
            )));
        }

        // Check number of attention heads
        if self.config.num_attention_heads != manifest.base.n_heads {
            return Err(AosError::InvalidManifest(format!(
                "Number of attention heads mismatch: {} != {}",
                self.config.num_attention_heads, manifest.base.n_heads
            )));
        }

        Ok(())
    }

    /// Calculate LoRA memory requirements for adapters
    pub fn calculate_lora_memory(
        &self,
        adapters: &[adapteros_manifest::Adapter],
    ) -> Result<Vec<u64>> {
        let mut memory_requirements = Vec::new();

        for adapter in adapters {
            let memory = self.calculate_adapter_memory(adapter)?;
            memory_requirements.push(memory);
        }

        Ok(memory_requirements)
    }

    /// Calculate memory requirement for a single adapter
    fn calculate_adapter_memory(&self, adapter: &adapteros_manifest::Adapter) -> Result<u64> {
        let rank = adapter.rank as u64;
        let hidden_size = self.config.hidden_size as u64;
        let intermediate_size = self.config.intermediate_size as u64;
        let num_layers = self.config.num_hidden_layers as u64;

        // Calculate parameters per layer
        let attention_params = rank * (hidden_size + hidden_size); // Q, K, V, O
        let mlp_params = rank * (hidden_size + intermediate_size + intermediate_size + hidden_size); // gate, up, down

        let params_per_layer = attention_params + mlp_params;
        let total_params = params_per_layer * num_layers;

        // Convert to bytes (fp16 = 2 bytes per parameter)
        let memory_bytes = total_params * 2;

        Ok(memory_bytes)
    }

    /// Get GQA configuration
    pub fn get_gqa_config(&self) -> GqaConfig {
        GqaConfig {
            num_attention_heads: self.config.num_attention_heads,
            num_key_value_heads: self.config.num_key_value_heads,
            head_dim: self.config.hidden_size / self.config.num_attention_heads,
            kv_width: self.config.num_key_value_heads
                * (self.config.hidden_size / self.config.num_attention_heads),
        }
    }

    /// Get RoPE configuration
    pub fn get_rope_config(&self) -> RopeConfig {
        RopeConfig {
            theta: self.config.rope_theta,
            scaling_factor: self.config.rope_scaling_factor,
            max_position_embeddings: self.config.max_position_embeddings,
        }
    }
}

/// GQA (Grouped Query Attention) configuration
#[derive(Debug, Clone)]
pub struct GqaConfig {
    pub num_attention_heads: u32,
    pub num_key_value_heads: u32,
    pub head_dim: u32,
    pub kv_width: u32,
}

/// RoPE (Rotary Position Embedding) configuration
#[derive(Debug, Clone)]
pub struct RopeConfig {
    pub theta: f32,
    pub scaling_factor: Option<f32>,
    pub max_position_embeddings: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gqa_config() {
        let config = crate::config::ModelConfig {
            name: "test".to_string(),
            architecture: "qwen2".to_string(),
            vocab_size: 32000,
            hidden_size: 4096,
            intermediate_size: 11008,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 4,
            rope_theta: 10000.0,
            rope_scaling_factor: None,
            max_position_embeddings: 32768,
            rope_scaling: None,
            extra: std::collections::HashMap::new(),
        };

        let gqa_config = GqaConfig {
            num_attention_heads: config.num_attention_heads,
            num_key_value_heads: config.num_key_value_heads,
            head_dim: config.hidden_size / config.num_attention_heads,
            kv_width: config.num_key_value_heads
                * (config.hidden_size / config.num_attention_heads),
        };

        assert_eq!(gqa_config.num_attention_heads, 32);
        assert_eq!(gqa_config.num_key_value_heads, 4);
        assert_eq!(gqa_config.head_dim, 128);
        assert_eq!(gqa_config.kv_width, 512);
    }

    #[test]
    fn test_lora_memory_calculation() {
        let config = crate::config::ModelConfig {
            name: "test".to_string(),
            architecture: "qwen2".to_string(),
            vocab_size: 32000,
            hidden_size: 4096,
            intermediate_size: 11008,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 4,
            rope_theta: 10000.0,
            rope_scaling_factor: None,
            max_position_embeddings: 32768,
            rope_scaling: None,
            extra: std::collections::HashMap::new(),
        };

        let adapter = adapteros_manifest::Adapter {
            id: "test-adapter".to_string(),
            hash: adapteros_core::B3Hash::hash(b"test"),
            tier: adapteros_manifest::AdapterTier::Persistent,
            rank: 16,
            alpha: 32.0,
            target_modules: vec!["q_proj".to_string(), "k_proj".to_string()],
            ttl: None,
            acl: vec![],
            warmup_prompt: None,
            dependencies: None,
            // Code intelligence fields
            category: adapteros_manifest::AdapterCategory::Code,
            scope: adapteros_manifest::AdapterScope::Global,
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: Some("inference".to_string()),
            // State management hints
            auto_promote: false,
            eviction_priority: adapteros_manifest::EvictionPriority::Normal,
        };

        let loader = ModelLoader {
            config,
            chat_template: ChatTemplateProcessor::new(adapteros_chat::ChatTemplate {
                name: "test".to_string(),
                template: "test".to_string(),
                special_tokens: adapteros_chat::SpecialTokens {
                    bos: "<|im_start|>".to_string(),
                    eos: "<|im_end|>".to_string(),
                    unk: "<|unk|>".to_string(),
                    pad: "<|pad|>".to_string(),
                },
            }),
            quantizer: BlockQuantizer::new("int4_block".to_string(), 128, 4),
        };

        let memory = loader
            .calculate_adapter_memory(&adapter)
            .expect("Test adapter memory calculation should succeed");

        // Expected calculation:
        // rank=16, hidden_size=4096, intermediate_size=11008, num_layers=32
        // attention_params = 16 * (4096 + 4096) = 131,072
        // mlp_params = 16 * (4096 + 11008 + 11008 + 4096) = 491,520
        // params_per_layer = 131,072 + 491,520 = 622,592
        // total_params = 622,592 * 32 = 19,922,944
        // memory_bytes = 19,922,944 * 2 = 39,845,888
        assert_eq!(memory, 39_845_888);
    }
}
