//! Tensor layout with padding calculations

use adapteros_core::Result;
use adapteros_manifest::ManifestV3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorLayout {
    pub base_layers: Vec<LayerLayout>,
    pub adapter_layouts: Vec<AdapterLayout>,
    pub kv_cache_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerLayout {
    pub layer_idx: usize,
    pub qkv_offset: usize,
    pub mlp_offset: usize,
    pub qkv_size: usize,
    pub mlp_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterLayout {
    pub id: String,
    pub rank: u32,
    pub rank_padded: u32,
    pub lora_a_offset: usize,
    pub lora_b_offset: usize,
}

impl TensorLayout {
    /// Compute layout from manifest
    pub fn from_manifest(manifest: &ManifestV3) -> Result<Self> {
        let mut base_layers = Vec::new();
        let mut offset = 0;

        // Base model layers
        for i in 0..manifest.base.n_layers {
            let qkv_size = manifest.base.hidden_dim * manifest.base.hidden_dim * 3;
            let mlp_size = manifest.base.hidden_dim * manifest.base.hidden_dim * 4;

            base_layers.push(LayerLayout {
                layer_idx: i as usize,
                qkv_offset: offset,
                mlp_offset: offset + qkv_size as usize,
                qkv_size: qkv_size as usize,
                mlp_size: mlp_size as usize,
            });

            offset += (qkv_size + mlp_size) as usize;
        }

        // Adapter layouts with rank padding
        let mut adapter_layouts = Vec::new();
        for adapter in &manifest.adapters {
            // Pad rank to next multiple of 16 for vectorization
            let rank_padded = adapter.rank.div_ceil(16) * 16;

            adapter_layouts.push(AdapterLayout {
                id: adapter.id.clone(),
                rank: adapter.rank,
                rank_padded,
                lora_a_offset: offset,
                lora_b_offset: offset + (rank_padded * manifest.base.hidden_dim) as usize,
            });

            offset += (rank_padded * manifest.base.hidden_dim * 2) as usize;
        }

        // KV cache sizing
        let kv_cache_size = manifest.base.n_layers as usize
            * manifest.base.hidden_dim as usize
            * 2048 // max seq len
            * 2; // K and V

        Ok(Self {
            base_layers,
            adapter_layouts,
            kv_cache_size,
        })
    }
}
