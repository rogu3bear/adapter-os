//! Directory specific adapter management.
//!
//! Directory adapters allow the router to specialise behaviour for
//! tightly-scoped parts of a repository.  They are derived from the
//! directory fingerprints produced by the codegraph crate.

use adapteros_codegraph::DirectoryAnalysis;
use adapteros_core::{B3Hash, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use tracing::info;

use crate::training::LoRAWeights;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathActivationRule {
    pub prefix: String,
    pub max_depth: usize,
    pub allow_descendants: bool,
}

impl PathActivationRule {
    fn matches(&self, path: &str) -> bool {
        if !path.starts_with(&self.prefix) {
            return false;
        }
        if !self.allow_descendants {
            return path == self.prefix;
        }
        let depth = depth_of(path);
        let base = depth_of(&self.prefix);
        depth >= base && depth - base <= self.max_depth
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryAdapterSpec {
    pub adapter_id: String,
    pub tenant_id: String,
    pub directory: PathBuf,
    pub fingerprint: B3Hash,
    pub rank: u8,
    pub activation_rules: Vec<PathActivationRule>,
    pub weights: LoRAWeights,
    pub metadata: serde_json::Value,
}

#[derive(Default)]
pub struct DirectoryAdapterManager {
    adapters: BTreeMap<String, DirectoryAdapterSpec>,
    tenant_index: BTreeMap<String, BTreeSet<String>>,
}

impl DirectoryAdapterManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert_from_analysis(
        &mut self,
        tenant_id: &str,
        analysis: &DirectoryAnalysis,
    ) -> Result<String> {
        let adapter_id = build_adapter_id(tenant_id, &analysis.fingerprint);
        let rank = map_rank_from_directory(analysis);
        let activation_rules = vec![PathActivationRule {
            prefix: normalize_path(&analysis.path),
            max_depth: 2,
            allow_descendants: true,
        }];
        let weights = generate_weights(&analysis.fingerprint, rank as usize, 12);
        let metadata = json!({
            "languages": analysis.language_stats,
            "patterns": analysis.pattern_counts,
            "architectural_styles": analysis.architectural_styles,
            "total_files": analysis.total_files,
            "total_lines": analysis.total_lines,
        });

        let spec = DirectoryAdapterSpec {
            adapter_id: adapter_id.clone(),
            tenant_id: tenant_id.to_string(),
            directory: analysis.path.clone(),
            fingerprint: analysis.fingerprint,
            rank,
            activation_rules,
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
            directory = %analysis.path.display(),
            adapter_id = adapter_id.as_str(),
            "directory adapter upserted"
        );

        Ok(adapter_id)
    }

    pub fn adapters_for_tenant(&self, tenant_id: &str) -> Vec<&DirectoryAdapterSpec> {
        self.tenant_index
            .get(tenant_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.adapters.get(id))
            .collect()
    }

    pub fn adapters_for_path<'a>(
        &'a self,
        tenant_id: &str,
        path: &str,
    ) -> Vec<&'a DirectoryAdapterSpec> {
        let normalized = path.replace('\\', "/");
        self.adapters_for_tenant(tenant_id)
            .into_iter()
            .filter(|adapter| {
                adapter
                    .activation_rules
                    .iter()
                    .any(|rule| rule.matches(&normalized))
            })
            .collect()
    }
}

fn build_adapter_id(tenant_id: &str, fingerprint: &B3Hash) -> String {
    format!("directory::{tenant_id}::{}", fingerprint.to_short_hex())
}

fn map_rank_from_directory(analysis: &DirectoryAnalysis) -> u8 {
    let symbol_bonus = (analysis.symbols.len() / 8) as u8;
    let pattern_bonus = analysis.pattern_counts.len() as u8;
    let mut rank = 16 + symbol_bonus + pattern_bonus;
    if rank > 32 {
        rank = 32;
    }
    rank
}

fn generate_weights(fingerprint: &B3Hash, rank: usize, hidden_dim: usize) -> LoRAWeights {
    let seed = fingerprint.to_bytes();
    let mut lora_a = Vec::with_capacity(rank);
    for r in 0..rank {
        let mut row = Vec::with_capacity(hidden_dim);
        for c in 0..hidden_dim {
            let idx = (r * 17 + c * 5) % seed.len();
            let raw = seed[idx] as f32 / 255.0;
            row.push((raw - 0.5) * 0.15);
        }
        lora_a.push(row);
    }

    let mut lora_b = Vec::with_capacity(hidden_dim);
    for c in 0..hidden_dim {
        let mut row = Vec::with_capacity(rank);
        for r in 0..rank {
            let idx = (c * 3 + r * 19) % seed.len();
            let raw = seed[idx] as f32 / 255.0;
            row.push((raw - 0.5) * 0.15);
        }
        lora_b.push(row);
    }

    LoRAWeights { lora_a, lora_b }
}

fn normalize_path(path: &PathBuf) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn depth_of(path: &str) -> usize {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_codegraph::{DirectorySymbol, DirectorySymbolKind};

    fn sample_analysis(path: &str) -> DirectoryAnalysis {
        DirectoryAnalysis {
            path: PathBuf::from(path),
            symbols: vec![sample_symbol("handler")],
            language_stats: BTreeMap::new(),
            pattern_counts: BTreeMap::new(),
            architectural_styles: BTreeSet::new(),
            fingerprint: B3Hash::hash(path.as_bytes()),
            total_files: 3,
            total_lines: 100,
        }
    }

    #[test]
    fn rule_matching_respects_depth() {
        let rule = PathActivationRule {
            prefix: "src/api".into(),
            max_depth: 1,
            allow_descendants: true,
        };
        assert!(rule.matches("src/api"));
        assert!(rule.matches("src/api/routes"));
        assert!(!rule.matches("src/api/routes/v1"));
    }

    #[test]
    fn manager_returns_path_matches() {
        let mut manager = DirectoryAdapterManager::new();
        let mut analysis = sample_analysis("src/api");
        analysis.pattern_counts.insert("http".into(), 1);
        manager.upsert_from_analysis("tenant", &analysis).unwrap();

        let matches = manager.adapters_for_path("tenant", "src/api/routes/mod.rs");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn adapters_are_deterministic() {
        let analysis = sample_analysis("components/buttons");
        let mut manager = DirectoryAdapterManager::new();
        let id1 = manager.upsert_from_analysis("tenant", &analysis).unwrap();
        let id2 = manager.upsert_from_analysis("tenant", &analysis).unwrap();
        assert_eq!(id1, id2);
    }

    fn sample_symbol(name: &str) -> DirectorySymbol {
        DirectorySymbol {
            name: name.to_string(),
            kind: DirectorySymbolKind::Function,
            file: PathBuf::from("src/lib.rs"),
            language: "rust".into(),
        }
    }
}
