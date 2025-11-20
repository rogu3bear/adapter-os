//! Weight Mapping Utilities for Transformer Architectures
//!
//! This module provides utilities to map transformer layer names between different
//! model architectures (Qwen, LLaMA, GPT-NeoX, etc.) and their CoreML equivalents.
//!
//! ## Supported Architectures
//!
//! - Qwen2.5 (default)
//! - LLaMA 2/3
//! - Mistral
//! - GPT-NeoX
//!
//! ## Layer Types
//!
//! - **Attention**: Q/K/V/O projections
//! - **Feed-Forward**: Gate/Up/Down projections (SwiGLU, GELU)
//! - **Normalization**: Input/Post-attention LayerNorm
//! - **Embeddings**: Token/Position embeddings
//! - **Output**: LM head projection

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// Transformer architecture type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchitectureType {
    /// Qwen2.5 architecture
    Qwen25,
    /// LLaMA 2/3 architecture
    LLaMA,
    /// Mistral architecture
    Mistral,
    /// GPT-NeoX architecture
    GPTNeoX,
}

impl ArchitectureType {
    /// Detect architecture from layer names
    pub fn detect_from_keys(keys: &[String]) -> Result<Self> {
        if keys.iter().any(|k| k.contains("qwen")) {
            Ok(Self::Qwen25)
        } else if keys.iter().any(|k| k.contains("llama")) {
            Ok(Self::LLaMA)
        } else if keys.iter().any(|k| k.contains("mistral")) {
            Ok(Self::Mistral)
        } else if keys.iter().any(|k| k.contains("gpt_neox")) {
            Ok(Self::GPTNeoX)
        } else {
            // Default to Qwen2.5 if unknown
            debug!("Unknown architecture, defaulting to Qwen2.5");
            Ok(Self::Qwen25)
        }
    }
}

/// Layer component type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayerComponent {
    // Attention components
    AttentionQProj,
    AttentionKProj,
    AttentionVProj,
    AttentionOProj,
    AttentionQKVFused, // Fused QKV projection (some architectures)

    // Feed-forward components
    FFNGateProj,
    FFNUpProj,
    FFNDownProj,

    // Normalization
    InputLayerNorm,
    PostAttentionLayerNorm,
    FinalLayerNorm,

    // Embeddings
    TokenEmbedding,
    PositionEmbedding,

    // Output
    LMHead,

    // Bias terms (optional)
    AttentionQBias,
    AttentionKBias,
    AttentionVBias,
    AttentionOBias,
    FFNGateBias,
    FFNUpBias,
    FFNDownBias,
}

impl LayerComponent {
    /// Check if component is a weight matrix (vs bias)
    pub fn is_weight(&self) -> bool {
        !matches!(
            self,
            Self::AttentionQBias
                | Self::AttentionKBias
                | Self::AttentionVBias
                | Self::AttentionOBias
                | Self::FFNGateBias
                | Self::FFNUpBias
                | Self::FFNDownBias
        )
    }

    /// Check if component is part of attention module
    pub fn is_attention(&self) -> bool {
        matches!(
            self,
            Self::AttentionQProj
                | Self::AttentionKProj
                | Self::AttentionVProj
                | Self::AttentionOProj
                | Self::AttentionQKVFused
                | Self::AttentionQBias
                | Self::AttentionKBias
                | Self::AttentionVBias
                | Self::AttentionOBias
        )
    }

    /// Check if component is part of feed-forward module
    pub fn is_ffn(&self) -> bool {
        matches!(
            self,
            Self::FFNGateProj
                | Self::FFNUpProj
                | Self::FFNDownProj
                | Self::FFNGateBias
                | Self::FFNUpBias
                | Self::FFNDownBias
        )
    }

    /// Check if component is normalization
    pub fn is_norm(&self) -> bool {
        matches!(
            self,
            Self::InputLayerNorm | Self::PostAttentionLayerNorm | Self::FinalLayerNorm
        )
    }
}

/// Weight mapping for a specific architecture
#[derive(Debug, Clone)]
pub struct WeightMapper {
    architecture: ArchitectureType,
    layer_templates: HashMap<LayerComponent, String>,
    layer_prefix: String,
}

impl WeightMapper {
    /// Create weight mapper for specified architecture
    pub fn new(architecture: ArchitectureType) -> Self {
        let (layer_templates, layer_prefix) = match architecture {
            ArchitectureType::Qwen25 => (Self::qwen25_templates(), "model.layers"),
            ArchitectureType::LLaMA => (Self::llama_templates(), "model.layers"),
            ArchitectureType::Mistral => (Self::mistral_templates(), "model.layers"),
            ArchitectureType::GPTNeoX => (Self::gpt_neox_templates(), "gpt_neox.layers"),
        };

        Self {
            architecture,
            layer_templates,
            layer_prefix: layer_prefix.to_string(),
        }
    }

    /// Get layer name for specific component and layer index
    pub fn get_layer_name(&self, component: LayerComponent, layer_idx: usize) -> String {
        let template = self
            .layer_templates
            .get(&component)
            .expect("Component not in template");

        format!("{}.{}.{}", self.layer_prefix, layer_idx, template)
    }

    /// Parse layer name and extract component + index
    pub fn parse_layer_name(&self, name: &str) -> Option<(LayerComponent, usize)> {
        if !name.starts_with(&self.layer_prefix) {
            return None;
        }

        // Extract layer index
        let parts: Vec<&str> = name.split('.').collect();
        if parts.len() < 3 {
            return None;
        }

        let layer_idx = parts[2].parse::<usize>().ok()?;

        // Match component
        for (component, template) in &self.layer_templates {
            let expected_name = format!("{}.{}.{}", self.layer_prefix, layer_idx, template);
            if name == expected_name {
                return Some((*component, layer_idx));
            }
        }

        None
    }

    /// Map layer name to CoreML-friendly name
    pub fn to_coreml_name(&self, component: LayerComponent, layer_idx: usize) -> String {
        match component {
            LayerComponent::AttentionQProj => format!("layer{}_attn_q", layer_idx),
            LayerComponent::AttentionKProj => format!("layer{}_attn_k", layer_idx),
            LayerComponent::AttentionVProj => format!("layer{}_attn_v", layer_idx),
            LayerComponent::AttentionOProj => format!("layer{}_attn_o", layer_idx),
            LayerComponent::AttentionQKVFused => format!("layer{}_attn_qkv", layer_idx),
            LayerComponent::FFNGateProj => format!("layer{}_ffn_gate", layer_idx),
            LayerComponent::FFNUpProj => format!("layer{}_ffn_up", layer_idx),
            LayerComponent::FFNDownProj => format!("layer{}_ffn_down", layer_idx),
            LayerComponent::InputLayerNorm => format!("layer{}_norm_input", layer_idx),
            LayerComponent::PostAttentionLayerNorm => format!("layer{}_norm_post_attn", layer_idx),
            LayerComponent::FinalLayerNorm => "final_norm".to_string(),
            LayerComponent::TokenEmbedding => "token_embedding".to_string(),
            LayerComponent::PositionEmbedding => "position_embedding".to_string(),
            LayerComponent::LMHead => "lm_head".to_string(),
            LayerComponent::AttentionQBias => format!("layer{}_attn_q_bias", layer_idx),
            LayerComponent::AttentionKBias => format!("layer{}_attn_k_bias", layer_idx),
            LayerComponent::AttentionVBias => format!("layer{}_attn_v_bias", layer_idx),
            LayerComponent::AttentionOBias => format!("layer{}_attn_o_bias", layer_idx),
            LayerComponent::FFNGateBias => format!("layer{}_ffn_gate_bias", layer_idx),
            LayerComponent::FFNUpBias => format!("layer{}_ffn_up_bias", layer_idx),
            LayerComponent::FFNDownBias => format!("layer{}_ffn_down_bias", layer_idx),
        }
    }

    /// Qwen2.5 layer templates
    fn qwen25_templates() -> HashMap<LayerComponent, String> {
        let mut templates = HashMap::new();

        // Attention
        templates.insert(
            LayerComponent::AttentionQProj,
            "self_attn.q_proj.weight".to_string(),
        );
        templates.insert(
            LayerComponent::AttentionKProj,
            "self_attn.k_proj.weight".to_string(),
        );
        templates.insert(
            LayerComponent::AttentionVProj,
            "self_attn.v_proj.weight".to_string(),
        );
        templates.insert(
            LayerComponent::AttentionOProj,
            "self_attn.o_proj.weight".to_string(),
        );

        // Feed-forward
        templates.insert(LayerComponent::FFNGateProj, "mlp.gate_proj.weight".to_string());
        templates.insert(LayerComponent::FFNUpProj, "mlp.up_proj.weight".to_string());
        templates.insert(LayerComponent::FFNDownProj, "mlp.down_proj.weight".to_string());

        // Normalization
        templates.insert(
            LayerComponent::InputLayerNorm,
            "input_layernorm.weight".to_string(),
        );
        templates.insert(
            LayerComponent::PostAttentionLayerNorm,
            "post_attention_layernorm.weight".to_string(),
        );

        templates
    }

    /// LLaMA layer templates
    fn llama_templates() -> HashMap<LayerComponent, String> {
        // LLaMA has same structure as Qwen2.5
        Self::qwen25_templates()
    }

    /// Mistral layer templates
    fn mistral_templates() -> HashMap<LayerComponent, String> {
        // Mistral has same structure as Qwen2.5
        Self::qwen25_templates()
    }

    /// GPT-NeoX layer templates
    fn gpt_neox_templates() -> HashMap<LayerComponent, String> {
        let mut templates = HashMap::new();

        // GPT-NeoX uses fused QKV projection
        templates.insert(
            LayerComponent::AttentionQKVFused,
            "attention.query_key_value.weight".to_string(),
        );
        templates.insert(
            LayerComponent::AttentionOProj,
            "attention.dense.weight".to_string(),
        );

        // Feed-forward
        templates.insert(LayerComponent::FFNUpProj, "mlp.dense_h_to_4h.weight".to_string());
        templates.insert(
            LayerComponent::FFNDownProj,
            "mlp.dense_4h_to_h.weight".to_string(),
        );

        // Normalization
        templates.insert(LayerComponent::InputLayerNorm, "input_layernorm.weight".to_string());
        templates.insert(
            LayerComponent::PostAttentionLayerNorm,
            "post_attention_layernorm.weight".to_string(),
        );

        templates
    }

    /// Get architecture type
    pub fn architecture(&self) -> ArchitectureType {
        self.architecture
    }

    /// Get layer prefix
    pub fn layer_prefix(&self) -> &str {
        &self.layer_prefix
    }
}

/// Weight mapping table for entire model
#[derive(Debug, Clone)]
pub struct WeightMappingTable {
    mapper: WeightMapper,
    mappings: HashMap<String, String>,
}

impl WeightMappingTable {
    /// Create mapping table from source layer names
    pub fn build(source_names: &[String], architecture: ArchitectureType) -> Result<Self> {
        let mapper = WeightMapper::new(architecture);
        let mut mappings = HashMap::new();

        for source_name in source_names {
            if let Some((component, layer_idx)) = mapper.parse_layer_name(source_name) {
                let coreml_name = mapper.to_coreml_name(component, layer_idx);
                mappings.insert(source_name.clone(), coreml_name);
            } else {
                // Handle non-layer weights (embeddings, lm_head, etc.)
                let coreml_name = if source_name.contains("embed_tokens") {
                    "token_embedding".to_string()
                } else if source_name.contains("lm_head") {
                    "lm_head".to_string()
                } else if source_name.contains("norm") && !source_name.contains("layers") {
                    "final_norm".to_string()
                } else {
                    // Keep original name if no mapping found
                    source_name.clone()
                };

                mappings.insert(source_name.clone(), coreml_name);
            }
        }

        debug!("Built mapping table with {} entries", mappings.len());

        Ok(Self { mapper, mappings })
    }

    /// Get CoreML name for source name
    pub fn get_coreml_name(&self, source_name: &str) -> Option<&str> {
        self.mappings.get(source_name).map(|s| s.as_str())
    }

    /// Get all mappings
    pub fn mappings(&self) -> &HashMap<String, String> {
        &self.mappings
    }

    /// Export mapping table as JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.mappings).map_err(|e| {
            AosError::Validation(format!("Failed to serialize mapping table: {}", e))
        })
    }

    /// Import mapping table from JSON
    pub fn from_json(json: &str, architecture: ArchitectureType) -> Result<Self> {
        let mappings: HashMap<String, String> = serde_json::from_str(json).map_err(|e| {
            AosError::Validation(format!("Failed to deserialize mapping table: {}", e))
        })?;

        Ok(Self {
            mapper: WeightMapper::new(architecture),
            mappings,
        })
    }
}

/// LoRA weight mapping (for LoRA adapters)
#[derive(Debug, Clone)]
pub struct LoRAMapping {
    base_mapper: WeightMapper,
}

impl LoRAMapping {
    /// Create LoRA mapping
    pub fn new(architecture: ArchitectureType) -> Self {
        Self {
            base_mapper: WeightMapper::new(architecture),
        }
    }

    /// Map LoRA layer name to CoreML name
    pub fn map_lora_name(&self, lora_name: &str) -> Option<(String, LoRAComponent)> {
        // Parse LoRA name: "model.layers.0.self_attn.q_proj.lora_A"
        if let Some(base_name_end) = lora_name.rfind(".lora_") {
            let base_name = &lora_name[..base_name_end];
            let lora_component = &lora_name[base_name_end + 1..];

            let component = match lora_component {
                "lora_A" => LoRAComponent::A,
                "lora_B" => LoRAComponent::B,
                _ => return None,
            };

            if let Some((layer_component, layer_idx)) =
                self.base_mapper.parse_layer_name(base_name)
            {
                let coreml_base = self.base_mapper.to_coreml_name(layer_component, layer_idx);
                Some((coreml_base, component))
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// LoRA component (A or B matrix)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoRAComponent {
    A, // Down projection
    B, // Up projection
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qwen25_mapping() {
        let mapper = WeightMapper::new(ArchitectureType::Qwen25);

        let name = mapper.get_layer_name(LayerComponent::AttentionQProj, 0);
        assert_eq!(name, "model.layers.0.self_attn.q_proj.weight");

        let coreml_name = mapper.to_coreml_name(LayerComponent::AttentionQProj, 0);
        assert_eq!(coreml_name, "layer0_attn_q");
    }

    #[test]
    fn test_parse_layer_name() {
        let mapper = WeightMapper::new(ArchitectureType::Qwen25);

        let parsed = mapper.parse_layer_name("model.layers.5.self_attn.q_proj.weight");
        assert_eq!(parsed, Some((LayerComponent::AttentionQProj, 5)));

        let parsed_ffn = mapper.parse_layer_name("model.layers.10.mlp.gate_proj.weight");
        assert_eq!(parsed_ffn, Some((LayerComponent::FFNGateProj, 10)));
    }

    #[test]
    fn test_layer_component_checks() {
        assert!(LayerComponent::AttentionQProj.is_weight());
        assert!(LayerComponent::AttentionQProj.is_attention());
        assert!(!LayerComponent::AttentionQProj.is_ffn());

        assert!(LayerComponent::FFNGateProj.is_weight());
        assert!(LayerComponent::FFNGateProj.is_ffn());
        assert!(!LayerComponent::FFNGateProj.is_attention());

        assert!(!LayerComponent::AttentionQBias.is_weight());
    }

    #[test]
    fn test_architecture_detection() {
        let keys = vec![
            "model.layers.0.self_attn.q_proj.weight".to_string(),
            "model.embed_tokens.weight".to_string(),
        ];

        let arch = ArchitectureType::detect_from_keys(&keys);
        assert!(arch.is_ok());
    }

    #[test]
    fn test_mapping_table() {
        let source_names = vec![
            "model.layers.0.self_attn.q_proj.weight".to_string(),
            "model.layers.0.mlp.gate_proj.weight".to_string(),
            "lm_head.weight".to_string(),
        ];

        let table = WeightMappingTable::build(&source_names, ArchitectureType::Qwen25).unwrap();

        assert_eq!(
            table.get_coreml_name("model.layers.0.self_attn.q_proj.weight"),
            Some("layer0_attn_q")
        );
        assert_eq!(
            table.get_coreml_name("model.layers.0.mlp.gate_proj.weight"),
            Some("layer0_ffn_gate")
        );
        assert_eq!(table.get_coreml_name("lm_head.weight"), Some("lm_head"));
    }

    #[test]
    fn test_lora_mapping() {
        let lora_mapper = LoRAMapping::new(ArchitectureType::Qwen25);

        let result = lora_mapper.map_lora_name("model.layers.0.self_attn.q_proj.lora_A");
        assert_eq!(result, Some(("layer0_attn_q".to_string(), LoRAComponent::A)));

        let result_b = lora_mapper.map_lora_name("model.layers.5.mlp.gate_proj.lora_B");
        assert_eq!(
            result_b,
            Some(("layer5_ffn_gate".to_string(), LoRAComponent::B))
        );
    }
}
