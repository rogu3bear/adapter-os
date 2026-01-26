//! Framework adapter construction utilities.
//!
//! These helpers translate the output of the framework detector into
//! concrete LoRA adapters that can be loaded by the worker.  The goal is
//! to maintain deterministic behaviour so that the same detection result
//! always yields the same adapter identifier and weight matrices.

use adapteros_retrieval::codegraph::DetectedFramework;
use adapteros_core::{B3Hash, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use tracing::info;

use crate::training::LoRAWeights;

/// Prepared framework adapter along with metadata needed by the router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkAdapterSpec {
    pub adapter_id: String,
    pub tenant_id: String,
    pub framework: DetectedFramework,
    pub rank: u8,
    pub activation_threshold: f32,
    pub target_layers: Vec<usize>,
    pub weights: LoRAWeights,
    pub metadata: serde_json::Value,
}

/// Manager that keeps framework adapters per tenant.
#[derive(Default)]
pub struct FrameworkAdapterManager {
    adapters: BTreeMap<String, FrameworkAdapterSpec>,
    tenant_index: BTreeMap<String, BTreeSet<String>>,
}

impl FrameworkAdapterManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create or refresh an adapter from the supplied detection record.
    pub fn upsert_adapter(
        &mut self,
        tenant_id: &str,
        detection: &DetectedFramework,
    ) -> Result<String> {
        let adapter_id = build_adapter_id(tenant_id, detection);
        let rank = map_rank_from_framework(detection.rank);
        let activation_threshold = 0.35 + detection.confidence * 0.5;
        let target_layers = derive_target_layers(rank);
        let weights = generate_weights(&adapter_id, rank as usize, 16);
        let metadata = json!({
            "confidence": detection.confidence,
            "framework": detection.name,
            "version": detection.version,
            "evidence": detection.evidence,
            "rank": rank,
            "activation_threshold": activation_threshold,
        });

        let spec = FrameworkAdapterSpec {
            adapter_id: adapter_id.clone(),
            tenant_id: tenant_id.to_string(),
            framework: detection.clone(),
            rank,
            activation_threshold: activation_threshold.min(0.95),
            target_layers,
            weights,
            metadata,
        };

        self.adapters.insert(adapter_id.clone(), spec);
        self.tenant_index
            .entry(tenant_id.to_string())
            .or_default()
            .insert(adapter_id.clone());

        info!(
            tenant = tenant_id,
            framework = detection.name.as_str(),
            adapter_id = adapter_id.as_str(),
            "framework adapter upserted"
        );

        Ok(adapter_id)
    }

    /// Return adapters for a tenant in deterministic order.
    pub fn adapters_for_tenant(&self, tenant_id: &str) -> Vec<&FrameworkAdapterSpec> {
        self.tenant_index
            .get(tenant_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.adapters.get(id))
            .collect()
    }

    /// Remove adapters that are no longer required.
    pub fn remove_adapter(&mut self, adapter_id: &str) {
        if let Some(spec) = self.adapters.remove(adapter_id) {
            if let Some(index) = self.tenant_index.get_mut(&spec.tenant_id) {
                index.remove(adapter_id);
                if index.is_empty() {
                    self.tenant_index.remove(&spec.tenant_id);
                }
            }
        }
    }
}

fn build_adapter_id(tenant_id: &str, detection: &DetectedFramework) -> String {
    let version = detection.version.clone().unwrap_or_else(|| "*".to_string());
    let hash = B3Hash::hash_multi(&[
        tenant_id.as_bytes(),
        detection.name.as_bytes(),
        version.as_bytes(),
        detection.confidence.to_le_bytes().as_slice(),
    ]);
    format!(
        "framework::{tenant_id}::{}::{}",
        detection.name,
        hash.to_short_hex()
    )
}

fn map_rank_from_framework(framework_rank: u8) -> u8 {
    match framework_rank {
        0..=9 => 8,
        10..=12 => 12,
        _ => 16,
    }
}

fn derive_target_layers(rank: u8) -> Vec<usize> {
    let layer_count = match rank {
        8 => 4,
        12 => 6,
        _ => 8,
    };
    (0..layer_count).map(|i| 2 * (i as usize + 1)).collect()
}

fn generate_weights(adapter_id: &str, rank: usize, hidden_dim: usize) -> LoRAWeights {
    let hash = B3Hash::hash(adapter_id.as_bytes());
    let seed = hash.to_bytes();

    let mut lora_a = Vec::with_capacity(rank);
    for r in 0..rank {
        let mut row = Vec::with_capacity(hidden_dim);
        for c in 0..hidden_dim {
            let idx = (r * 7 + c * 13) % seed.len();
            let raw = seed[idx] as f32 / 255.0;
            row.push((raw - 0.5) * 0.2);
        }
        lora_a.push(row);
    }

    let mut lora_b = Vec::with_capacity(hidden_dim);
    for c in 0..hidden_dim {
        let mut row = Vec::with_capacity(rank);
        for r in 0..rank {
            let idx = (c * 11 + r * 5) % seed.len();
            let raw = seed[idx] as f32 / 255.0;
            row.push((raw - 0.5) * 0.2);
        }
        lora_b.push(row);
    }

    LoRAWeights {
        lora_a,
        lora_b,
        modules: HashMap::new(),
        moe_config: None,
        precomputed_delta: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_detection(name: &str, confidence: f32, rank: u8) -> DetectedFramework {
        DetectedFramework {
            name: name.to_string(),
            version: Some("1.0".into()),
            confidence,
            rank,
            evidence: vec!["config:package.json".into()],
        }
    }

    #[test]
    fn adapter_id_deterministic() {
        let detection = sample_detection("React", 0.8, 9);
        let adapter_id = build_adapter_id("tenant", &detection);
        assert_eq!(adapter_id, build_adapter_id("tenant", &detection));
    }

    #[test]
    fn manager_stores_per_tenant() {
        let detection = sample_detection("Django", 0.9, 8);
        let mut manager = FrameworkAdapterManager::new();
        let id = manager.upsert_adapter("tenant", &detection).unwrap();
        assert!(manager
            .adapters_for_tenant("tenant")
            .iter()
            .any(|a| a.adapter_id == id));
    }
}
