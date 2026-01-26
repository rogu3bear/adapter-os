//! Weight serialization and combination helpers
//!
//! Provides utilities for encoding weight groups to on-disk payloads and
//! recombining separated positive/negative LoRA weights for inference.

use super::format::{CombinationStrategy, WeightGroup, WeightGroupType, WeightMetadata};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// Serializable representation of a weight group (LoRA matrices only)
#[derive(Debug, Serialize, Deserialize)]
pub struct WeightGroupPayload {
    pub lora_a: Vec<Vec<f32>>,
    pub lora_b: Vec<Vec<f32>>,
}

impl WeightGroupPayload {
    pub fn from_group(group: &WeightGroup) -> Self {
        Self {
            lora_a: group.lora_a.clone(),
            lora_b: group.lora_b.clone(),
        }
    }
}

/// Serialize a weight group into JSON payload (stored as `.safetensors`)
pub fn serialize_weight_group(weight_group: &WeightGroup) -> Result<Vec<u8>> {
    serde_json::to_vec(&WeightGroupPayload::from_group(weight_group))
        .map_err(|e| AosError::Training(format!("Failed to serialize weight group payload: {}", e)))
}

/// Deserialize weight group payload and attach metadata
pub fn deserialize_weight_group(bytes: &[u8], metadata: WeightMetadata) -> Result<WeightGroup> {
    let payload: WeightGroupPayload = serde_json::from_slice(bytes)
        .map_err(|e| AosError::Parse(format!("Failed to parse weight group payload: {}", e)))?;

    Ok(WeightGroup {
        lora_a: payload.lora_a,
        lora_b: payload.lora_b,
        metadata,
    })
}

/// Combine positive/negative weights according to strategy
pub fn combine_weight_groups(
    positive: &WeightGroup,
    negative: &WeightGroup,
    strategy: &CombinationStrategy,
) -> Result<WeightGroup> {
    match strategy {
        CombinationStrategy::Difference => combine_with_scales(positive, negative, 1.0, 1.0),
        CombinationStrategy::WeightedDifference {
            positive_scale,
            negative_scale,
        } => combine_with_scales(positive, negative, *positive_scale, *negative_scale),
        CombinationStrategy::Separate => Ok(positive.clone()),
    }
}

fn combine_with_scales(
    positive: &WeightGroup,
    negative: &WeightGroup,
    pos_scale: f32,
    neg_scale: f32,
) -> Result<WeightGroup> {
    if positive.lora_a.len() != negative.lora_a.len()
        || positive.lora_b.len() != negative.lora_b.len()
    {
        return Err(AosError::Training(
            "Weight group dimensions do not match".to_string(),
        ));
    }

    let mut combined_a = Vec::with_capacity(positive.lora_a.len());
    for (pos_row, neg_row) in positive.lora_a.iter().zip(negative.lora_a.iter()) {
        if pos_row.len() != neg_row.len() {
            return Err(AosError::Training(
                "Weight group row dimensions do not match".to_string(),
            ));
        }
        let row = pos_row
            .iter()
            .zip(neg_row.iter())
            .map(|(p, n)| (p * pos_scale) - (n * neg_scale))
            .collect();
        combined_a.push(row);
    }

    let mut combined_b = Vec::with_capacity(positive.lora_b.len());
    for (pos_row, neg_row) in positive.lora_b.iter().zip(negative.lora_b.iter()) {
        if pos_row.len() != neg_row.len() {
            return Err(AosError::Training(
                "Weight group column dimensions do not match".to_string(),
            ));
        }
        let row = pos_row
            .iter()
            .zip(neg_row.iter())
            .map(|(p, n)| (p * pos_scale) - (n * neg_scale))
            .collect();
        combined_b.push(row);
    }

    Ok(WeightGroup {
        lora_a: combined_a,
        lora_b: combined_b,
        metadata: WeightMetadata {
            example_count: positive.metadata.example_count + negative.metadata.example_count,
            avg_loss: (positive.metadata.avg_loss + negative.metadata.avg_loss) / 2.0,
            training_time_ms: positive.metadata.training_time_ms
                + negative.metadata.training_time_ms,
            group_type: WeightGroupType::Combined,
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    })
}

/// Minimal metadata persisted alongside on-disk weight payloads
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WeightGroupDiskInfo {
    pub example_count: usize,
    pub avg_loss: f32,
    pub training_time_ms: u64,
    pub created_at: String,
}

/// Metadata manifest for all weight groups stored in an .aos file
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WeightGroupsManifest {
    pub positive: WeightGroupDiskInfo,
    pub negative: WeightGroupDiskInfo,
    pub combined: Option<WeightGroupDiskInfo>,
    pub combination_strategy: CombinationStrategy,
    pub use_separate_weights: bool,
}
