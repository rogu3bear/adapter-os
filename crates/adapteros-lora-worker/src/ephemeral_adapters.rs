//! Ephemeral adapter orchestration.
//!
//! Ephemeral adapters are short-lived LoRA weights that capture the
//! impact of recent changes.  They are generated from the change detector
//! and automatically expire after their TTL window.

use adapteros_codegraph::DetectedChange;

// Re-export for consumers
pub use adapteros_codegraph::DetectedChangeType;
use adapteros_core::{B3Hash, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use tracing::info;

use crate::training::LoRAWeights;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralAdapterSpec {
    pub adapter_id: String,
    pub tenant_id: String,
    pub change: DetectedChange,
    pub rank: u8,
    pub expires_at: DateTime<Utc>,
    pub weights: LoRAWeights,
    pub metadata: serde_json::Value,
}

#[derive(Default)]
pub struct EphemeralAdapterManager {
    adapters: BTreeMap<String, EphemeralAdapterSpec>,
    tenant_index: BTreeMap<String, BTreeSet<String>>,
    change_history: BTreeMap<String, Vec<DetectedChange>>,
}

impl EphemeralAdapterManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_from_change(
        &mut self,
        tenant_id: &str,
        change: &DetectedChange,
    ) -> Result<String> {
        let adapter_id = build_adapter_id(tenant_id, change);
        let rank = change.suggested_rank.clamp(4, 8);
        let ttl_hours = change.ttl_hours.clamp(24, 72);
        let expires_at = Utc::now() + Duration::hours(ttl_hours as i64);
        let weights = generate_weights(&adapter_id, rank as usize, 8);
        let metadata = json!({
            "path": change.path,
            "change_type": change.change_type,
            "impact_score": change.impact_score,
            "commit_id": change.commit_id,
            "impacted_symbols": change.impacted_symbols,
            "ttl_hours": ttl_hours,
        });

        let spec = EphemeralAdapterSpec {
            adapter_id: adapter_id.clone(),
            tenant_id: tenant_id.to_string(),
            change: change.clone(),
            rank,
            expires_at,
            weights,
            metadata,
        };

        self.adapters.insert(adapter_id.clone(), spec);
        self.tenant_index
            .entry(tenant_id.to_string())
            .or_default()
            .insert(adapter_id.clone());
        self.change_history
            .entry(tenant_id.to_string())
            .or_default()
            .push(change.clone());

        info!(
            tenant = tenant_id,
            path = %change.path.display(),
            adapter_id = adapter_id.as_str(),
            ttl_hours = ttl_hours,
            "ephemeral adapter created"
        );

        Ok(adapter_id)
    }

    pub fn adapters_for_tenant(&self, tenant_id: &str) -> Vec<&EphemeralAdapterSpec> {
        self.tenant_index
            .get(tenant_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.adapters.get(id))
            .collect()
    }

    pub fn evict_expired(&mut self, now: DateTime<Utc>) -> Vec<String> {
        let expired: Vec<String> = self
            .adapters
            .iter()
            .filter(|(_, spec)| spec.expires_at <= now)
            .map(|(id, _)| id.clone())
            .collect();

        for id in &expired {
            if let Some(spec) = self.adapters.remove(id) {
                if let Some(index) = self.tenant_index.get_mut(&spec.tenant_id) {
                    index.remove(id);
                    if index.is_empty() {
                        self.tenant_index.remove(&spec.tenant_id);
                    }
                }
            }
        }

        expired
    }

    pub fn extend_ttl(&mut self, adapter_id: &str, additional_hours: u64) {
        if let Some(spec) = self.adapters.get_mut(adapter_id) {
            let max_expiry = Utc::now() + Duration::hours(72);
            let new_expiry = spec.expires_at + Duration::hours(additional_hours as i64);
            spec.expires_at = std::cmp::min(new_expiry, max_expiry);
        }
    }

    pub fn history_for_tenant(&self, tenant_id: &str) -> Vec<DetectedChange> {
        self.change_history
            .get(tenant_id)
            .cloned()
            .unwrap_or_default()
    }
}

fn build_adapter_id(tenant_id: &str, change: &DetectedChange) -> String {
    let hash = B3Hash::hash_multi(&[
        tenant_id.as_bytes(),
        change.commit_id.as_bytes(),
        change.path.to_string_lossy().as_bytes(),
    ]);
    format!(
        "ephemeral::{tenant_id}::{}::{}",
        change.commit_id,
        hash.to_short_hex()
    )
}

fn generate_weights(adapter_id: &str, rank: usize, hidden_dim: usize) -> LoRAWeights {
    let seed = B3Hash::hash(adapter_id.as_bytes()).to_bytes();
    let mut lora_a = Vec::with_capacity(rank);
    for r in 0..rank {
        let mut row = Vec::with_capacity(hidden_dim);
        for c in 0..hidden_dim {
            let idx = (r * 5 + c * 7) % seed.len();
            let raw = seed[idx] as f32 / 255.0;
            row.push((raw - 0.5) * 0.25);
        }
        lora_a.push(row);
    }

    let mut lora_b = Vec::with_capacity(hidden_dim);
    for c in 0..hidden_dim {
        let mut row = Vec::with_capacity(rank);
        for r in 0..rank {
            let idx = (c * 13 + r * 3) % seed.len();
            let raw = seed[idx] as f32 / 255.0;
            row.push((raw - 0.5) * 0.25);
        }
        lora_b.push(row);
    }

    LoRAWeights { lora_a, lora_b }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_change(path: &str) -> DetectedChange {
        DetectedChange {
            path: PathBuf::from(path),
            change_type: DetectedChangeType::Modified,
            impacted_symbols: vec!["handler".into()],
            impact_score: 0.8,
            suggested_rank: 6,
            ttl_hours: 36,
            commit_id: "abc123".into(),
        }
    }

    #[test]
    fn create_and_evict_ephemeral_adapter() {
        let mut manager = EphemeralAdapterManager::new();
        let change = sample_change("src/lib.rs");
        let id = manager.create_from_change("tenant", &change).unwrap();
        assert_eq!(manager.adapters_for_tenant("tenant").len(), 1);

        let expired = manager.evict_expired(Utc::now() + Duration::hours(100));
        assert!(expired.contains(&id));
    }

    #[test]
    fn extend_ttl_caps_at_72h() {
        let mut manager = EphemeralAdapterManager::new();
        let change = sample_change("src/lib.rs");
        let id = manager.create_from_change("tenant", &change).unwrap();
        let before = manager.adapters.get(&id).unwrap().expires_at;
        manager.extend_ttl(&id, 100);
        let after = manager.adapters.get(&id).unwrap().expires_at;
        assert!(after - before <= Duration::hours(72));
    }
}
