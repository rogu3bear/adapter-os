//! Model loader and validation for AdapterOS

use adapteros_chat::{ChatTemplate, ChatTemplateProcessor, SpecialTokens};
use adapteros_core::{AosError, Result};
use adapteros_lora_quant::BlockQuantizer;
use std::path::Path;
use tracing::{debug, info, warn};

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
    pub fn load_from_registry<P: AsRef<Path>>(model_name: &str, registry_path: P) -> Result<Self> {
        let registry_path = registry_path.as_ref();
        info!(model_name = %model_name, registry_path = %registry_path.display(), "Loading model from registry");

        // Open registry connection
        let conn = rusqlite::Connection::open(registry_path).map_err(|e| {
            AosError::Registry(format!(
                "Failed to open registry at {}: {}",
                registry_path.display(),
                e
            ))
        })?;

        let model_registry = adapteros_registry::models::ModelRegistry::new(conn);

        // Load model record from registry
        let model_record = model_registry.get_model(model_name)?.ok_or_else(|| {
            AosError::NotFound(format!("Model '{}' not found in registry", model_name))
        })?;

        debug!(
            model_name = %model_name,
            config_hash = %model_record.config_hash,
            "Found model record in registry"
        );

        // Load config.json from model directory
        let model_dir = registry_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("models")
            .join(model_name);

        let config_path = model_dir.join("config.json");
        let config_json = std::fs::read_to_string(&config_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read model config at {}: {}",
                config_path.display(),
                e
            ))
        })?;

        // Parse and validate model config
        let config = crate::config::ModelConfig::from_json(&config_json)?;
        config.validate_gqa()?;

        debug!(
            model_name = %config.name,
            architecture = %config.architecture,
            hidden_size = config.hidden_size,
            num_layers = config.num_hidden_layers,
            "Parsed model configuration"
        );

        // Load chat template from tokenizer_config.json
        let chat_template = Self::load_chat_template(&model_dir, &config.architecture)?;

        // Determine quantization from model metadata or config
        let (quant_type, group_size, bits) = Self::detect_quantization(&model_dir)?;

        // Create quantizer
        let quantizer = BlockQuantizer::new(quant_type, group_size, bits);

        info!(
            model_name = %model_name,
            architecture = %config.architecture,
            "Successfully loaded model from registry"
        );

        Ok(Self {
            config,
            chat_template,
            quantizer,
        })
    }

    /// Load chat template from tokenizer_config.json or use architecture defaults
    fn load_chat_template(model_dir: &Path, architecture: &str) -> Result<ChatTemplateProcessor> {
        let tokenizer_config_path = model_dir.join("tokenizer_config.json");

        if tokenizer_config_path.exists() {
            debug!(path = %tokenizer_config_path.display(), "Loading chat template from tokenizer_config.json");

            let content = std::fs::read_to_string(&tokenizer_config_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to read tokenizer config at {}: {}",
                    tokenizer_config_path.display(),
                    e
                ))
            })?;

            // Parse tokenizer_config.json
            let tokenizer_cfg: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| AosError::Config(format!("Invalid tokenizer config JSON: {}", e)))?;

            // Extract chat_template if present
            let template_str = tokenizer_cfg
                .get("chat_template")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Extract special tokens
            let bos = tokenizer_cfg
                .get("bos_token")
                .and_then(|v| v.as_str())
                .unwrap_or("<|im_start|>")
                .to_string();

            let eos = tokenizer_cfg
                .get("eos_token")
                .and_then(|v| v.as_str())
                .unwrap_or("<|im_end|>")
                .to_string();

            let unk = tokenizer_cfg
                .get("unk_token")
                .and_then(|v| v.as_str())
                .unwrap_or("<|unk|>")
                .to_string();

            let pad = tokenizer_cfg
                .get("pad_token")
                .and_then(|v| v.as_str())
                .unwrap_or("<|endoftext|>")
                .to_string();

            let chat_template_config = ChatTemplate {
                name: architecture.to_string(),
                template: template_str
                    .unwrap_or_else(|| Self::default_template_for_arch(architecture)),
                special_tokens: SpecialTokens { bos, eos, unk, pad },
            };

            debug!(
                architecture = %architecture,
                bos = %chat_template_config.special_tokens.bos,
                eos = %chat_template_config.special_tokens.eos,
                "Loaded chat template configuration"
            );

            Ok(ChatTemplateProcessor::new(chat_template_config))
        } else {
            warn!(
                path = %tokenizer_config_path.display(),
                architecture = %architecture,
                "tokenizer_config.json not found, using architecture defaults"
            );

            Ok(ChatTemplateProcessor::new(Self::default_chat_template(
                architecture,
            )))
        }
    }

    /// Get default chat template based on model architecture
    fn default_chat_template(architecture: &str) -> ChatTemplate {
        let (name, template, special_tokens) = match architecture {
            "qwen2" | "qwen" => (
                "qwen".to_string(),
                Self::default_template_for_arch(architecture),
                SpecialTokens {
                    bos: "<|im_start|>".to_string(),
                    eos: "<|im_end|>".to_string(),
                    unk: "<|unk|>".to_string(),
                    pad: "<|endoftext|>".to_string(),
                },
            ),
            "llama" | "llama2" => (
                "llama".to_string(),
                Self::default_template_for_arch(architecture),
                SpecialTokens {
                    bos: "<s>".to_string(),
                    eos: "</s>".to_string(),
                    unk: "<unk>".to_string(),
                    pad: "<pad>".to_string(),
                },
            ),
            "mistral" => (
                "mistral".to_string(),
                Self::default_template_for_arch(architecture),
                SpecialTokens {
                    bos: "<s>".to_string(),
                    eos: "</s>".to_string(),
                    unk: "<unk>".to_string(),
                    pad: "<pad>".to_string(),
                },
            ),
            _ => (
                "chatml".to_string(),
                Self::default_template_for_arch("chatml"),
                SpecialTokens::default(),
            ),
        };

        ChatTemplate {
            name,
            template,
            special_tokens,
        }
    }

    /// Get default template string for architecture
    fn default_template_for_arch(architecture: &str) -> String {
        match architecture {
            "qwen2" | "qwen" | "chatml" => {
                "{% for message in messages %}{{ '<|im_start|>' + message['role'] + '\\n' + message['content'] + '<|im_end|>\\n' }}{% endfor %}".to_string()
            }
            "llama" | "llama2" => {
                "{% for message in messages %}{% if message['role'] == 'system' %}<<SYS>>\\n{{ message['content'] }}\\n<</SYS>>\\n\\n{% elif message['role'] == 'user' %}[INST] {{ message['content'] }} [/INST]{% else %}{{ message['content'] }}{% endif %}{% endfor %}".to_string()
            }
            "mistral" => {
                "{% for message in messages %}{% if message['role'] == 'user' %}[INST] {{ message['content'] }} [/INST]{% else %}{{ message['content'] }}{% endif %}{% endfor %}".to_string()
            }
            _ => ChatTemplate::default().template,
        }
    }

    /// Detect quantization settings from model directory
    fn detect_quantization(model_dir: &Path) -> Result<(String, u32, u8)> {
        // Try to read quantization info from config.json or weights
        let config_path = model_dir.join("config.json");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to read config for quantization detection: {}",
                    e
                ))
            })?;

            let config: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| AosError::Config(format!("Invalid config JSON: {}", e)))?;

            // Check for quantization config
            if let Some(quant_config) = config.get("quantization_config") {
                let quant_type = quant_config
                    .get("quant_method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("int4_block")
                    .to_string();

                let group_size = quant_config
                    .get("group_size")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(128) as u32;

                let bits = quant_config
                    .get("bits")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(4) as u8;

                debug!(
                    quant_type = %quant_type,
                    group_size = group_size,
                    bits = bits,
                    "Detected quantization from config"
                );

                return Ok((quant_type, group_size, bits));
            }
        }

        // Default to int4 block quantization (most common for MLX models)
        debug!("Using default quantization: int4_block, group_size=128, bits=4");
        Ok(("int4_block".to_string(), 128, 4))
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
            lora_strength: None,
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
