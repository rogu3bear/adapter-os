//! Adapter manifest validation and building logic

use super::metadata::{
    default_category, default_determinism_mode, default_recommended_for_moe, default_scope,
    default_strength_for_tier, default_tier, expected_repo_slug_from_metadata,
    is_valid_graph_target, metadata_has_scan_root_keys, metadata_indicates_codebase,
    parse_bool_strict, parse_scan_roots_from_metadata, parse_scan_roots_strict,
    resolve_scan_root_from_metadata,
};
use super::types::{
    AdapterPlacement, CoremlPlacementSpec, CoremlTrainingMetadata, LayerHash, ScanRootMetadata,
};
use crate::training::trainer::{MoETrainingConfig, TrainingConfig};
use crate::training::{
    LORA_Q15_DENOM, LORA_Q15_QUANTIZATION, LORA_Q15_VERSION, LORA_STRENGTH_DEFAULTS_VERSION,
};
use adapteros_core::{AosError, Result};
use adapteros_types::training::LoraTier;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// Adapter manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub version: String,
    pub rank: usize,
    pub base_model: String,
    /// BLAKE3 hash of the base model weights/config
    #[serde(default)]
    pub base_model_hash: Option<String>,
    pub training_config: TrainingConfig,
    /// BLAKE3 hash of the training configuration for reproducibility verification
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_config_hash: Option<String>,
    /// BLAKE3 hash of the tokenizer configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokenizer_hash: Option<String>,
    pub created_at: String,
    pub weights_hash: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_tier")]
    pub tier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_layer_hashes: Option<HashMap<String, LayerHash>>,
    #[serde(default)]
    pub training_backend: Option<String>,
    #[serde(default = "default_determinism_mode")]
    pub determinism: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lora_tier: Option<LoraTier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lora_strength: Option<f32>,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default = "default_recommended_for_moe")]
    pub recommended_for_moe: bool,
    #[serde(default)]
    pub quantization: Option<String>,
    #[serde(default)]
    pub gate_q15_denominator: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml: Option<CoremlTrainingMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<AdapterPlacement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_placement: Option<CoremlPlacementSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_backend_details: Option<String>,
    /// Primary dataset ID used for training
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,
    /// BLAKE3 hash of the primary dataset content
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_version_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_spec_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_lineage_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthetic_mode: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_slice_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,
    /// MoE (Mixture of Experts) training configuration (if trained for MoE model)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moe_config: Option<MoETrainingConfig>,
    /// Codebase scope: repository name or identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_repo: Option<String>,
    /// Codebase scope: branch name (e.g., "main", "feature/xyz")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_branch: Option<String>,
    /// Codebase scope: commit SHA at training time
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_commit: Option<String>,
    /// Codebase scope: primary scan root path used during ingestion
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_scan_root: Option<String>,
    /// Codebase scope: remote URL of the repository
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_remote_url: Option<String>,
    /// Codebase scope: normalized repository slug (e.g., "my_project" from "My-Project")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_slug: Option<String>,
    /// All scan root paths used during training package creation.
    /// Captures multiple roots when training combines content from different directories.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scan_roots: Vec<ScanRootMetadata>,
    /// Session identifier for correlating ingestion workflows
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Human-readable session name for ingestion workflows
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
    /// Session tags for categorization (comma or JSON array in metadata)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_tags: Option<Vec<String>>,
    /// Whether stream mode was enabled during training/ingestion.
    /// Stream mode enables real-time progress updates during the training pipeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_mode: Option<bool>,
    /// Integrity hash covering all critical manifest fields.
    /// BLAKE3(base_model_hash || training_config_hash || tokenizer_hash || dataset_hash || weights_hash)
    /// Used for tamper detection and validation on load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity_hash: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl AdapterManifest {
    pub(crate) fn validate(&self) -> Result<()> {
        if let Some(coreml) = &self.coreml {
            if coreml.coreml_used {
                let backend = self
                    .training_backend
                    .as_deref()
                    .or(self.training_backend_details.as_deref())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if !backend.contains("coreml") {
                    return Err(AosError::InvalidManifest(
                        "coreml_used requires training_backend coreml".to_string(),
                    ));
                }
            }

            if let Some(device) = &coreml.coreml_device_type {
                let device_lc = device.to_ascii_lowercase();
                if !matches!(device_lc.as_str(), "ane" | "gpu" | "cpu" | "unknown") {
                    return Err(AosError::InvalidManifest(format!(
                        "Invalid coreml_device_type: {}",
                        device
                    )));
                }
            }
        }

        let scope_id_ok = self
            .metadata
            .get("adapter_scope_id")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let scope_type_ok = self
            .metadata
            .get("adapter_scope_type")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let scope_visibility_ok = self
            .metadata
            .get("adapter_scope_visibility")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let scope_required = matches!(self.category.as_str(), "code" | "codebase");
        if (scope_required || scope_id_ok || scope_type_ok || scope_visibility_ok)
            && !(scope_id_ok && scope_type_ok && scope_visibility_ok)
        {
            return Err(AosError::InvalidManifest(
                "adapter_scope_id, adapter_scope_type, and adapter_scope_visibility are required for code adapters"
                    .to_string(),
            ));
        }

        let synthetic_mode = self
            .metadata
            .get("synthetic_mode")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let lineage_mode = self
            .metadata
            .get("data_lineage_mode")
            .map(|s| s.as_str())
            .unwrap_or("versioned");
        let dataset_ids_present = self
            .dataset_version_ids
            .as_ref()
            .map(|ids| !ids.is_empty())
            .unwrap_or(false);

        debug!(
            lineage_mode = lineage_mode,
            dataset_ids_present = dataset_ids_present,
            "evaluated lineage mode"
        );

        if !synthetic_mode {
            if let Some(ids) = &self.dataset_version_ids {
                if ids.is_empty() {
                    return Err(AosError::InvalidManifest(
                        "dataset_version_ids cannot be empty".to_string(),
                    ));
                }
            }

            if !dataset_ids_present && lineage_mode != "legacy_unpinned" {
                return Err(AosError::InvalidManifest(
                    "dataset_version_ids required unless synthetic_mode=true or lineage_mode=legacy_unpinned"
                        .to_string(),
                ));
            }

            if dataset_ids_present && self.data_spec_hash.is_none() {
                return Err(AosError::InvalidManifest(
                    "data_spec_hash is required when dataset_version_ids are present".to_string(),
                ));
            }
        }

        if let Some(placement) = &self.placement {
            for record in &placement.records {
                if record.rank == 0 {
                    return Err(AosError::InvalidManifest(
                        "Placement record rank must be > 0".to_string(),
                    ));
                }

                if !is_valid_graph_target(&record.graph_target) {
                    return Err(AosError::InvalidManifest(format!(
                        "Invalid placement graph_target: {}",
                        record.graph_target
                    )));
                }

                if record.direction.trim().is_empty() {
                    return Err(AosError::InvalidManifest(
                        "Placement direction must be non-empty".to_string(),
                    ));
                }

                if let Some(alpha) = record.alpha_override {
                    if alpha <= 0.0 {
                        return Err(AosError::InvalidManifest(
                            "Placement alpha_override must be positive".to_string(),
                        ));
                    }
                }
            }
        }

        if let Some(spec) = &self.coreml_placement {
            if spec.version == 0 {
                return Err(AosError::InvalidManifest(
                    "coreml_placement.version must be > 0".to_string(),
                ));
            }
            for binding in &spec.bindings {
                if binding.rank == 0 {
                    return Err(AosError::InvalidManifest(format!(
                        "coreml_placement binding {} has rank 0",
                        binding.binding_id
                    )));
                }
                if binding.shape.input_dim == 0 || binding.shape.output_dim == 0 {
                    return Err(AosError::InvalidManifest(format!(
                        "coreml_placement binding {} has zero-dimension shape",
                        binding.binding_id
                    )));
                }
                if binding.target.layer.trim().is_empty() {
                    return Err(AosError::InvalidManifest(
                        "coreml_placement binding missing target layer".to_string(),
                    ));
                }
            }
        }

        self.validate_scope_metadata()?;
        self.validate_quantization_metadata()?;

        Ok(())
    }

    fn validate_scope_metadata(&self) -> Result<()> {
        let is_codebase = metadata_indicates_codebase(&self.metadata);
        let expected_slug = expected_repo_slug_from_metadata(&self.metadata);

        if is_codebase {
            let has_repo_slug_metadata = self
                .metadata
                .get("repo_slug")
                .map(|slug| !slug.trim().is_empty())
                .unwrap_or(false)
                || self
                    .metadata
                    .get("scope_repo_slug")
                    .map(|slug| !slug.trim().is_empty())
                    .unwrap_or(false);
            if !has_repo_slug_metadata {
                return Err(AosError::InvalidManifest(
                    "repo_slug missing from ingestion metadata for codebase adapter".to_string(),
                ));
            }
        }

        if is_codebase
            && self
                .repo_slug
                .as_deref()
                .map(|slug| slug.trim().is_empty())
                .unwrap_or(true)
        {
            return Err(AosError::InvalidManifest(
                "repo_slug missing for codebase adapter".to_string(),
            ));
        }

        if let Some(expected_slug) = expected_slug {
            match self.repo_slug.as_deref() {
                Some(actual) if actual == expected_slug => {}
                Some(actual) => {
                    return Err(AosError::InvalidManifest(format!(
                        "repo_slug '{}' does not match ingestion metadata '{}'",
                        actual, expected_slug
                    )));
                }
                None => {
                    return Err(AosError::InvalidManifest(
                        "repo_slug missing but ingestion metadata provides one".to_string(),
                    ));
                }
            }
        }

        if is_codebase && !metadata_has_scan_root_keys(&self.metadata) {
            return Err(AosError::InvalidManifest(
                "scan_roots missing from ingestion metadata for codebase adapter".to_string(),
            ));
        }

        if metadata_has_scan_root_keys(&self.metadata) {
            let expected_roots = parse_scan_roots_strict(&self.metadata)?
                .unwrap_or_else(|| parse_scan_roots_from_metadata(&self.metadata));
            if expected_roots.is_empty() {
                return Err(AosError::InvalidManifest(
                    "scan_roots metadata provided but no scan roots parsed".to_string(),
                ));
            }
            if self.scan_roots.is_empty() {
                return Err(AosError::InvalidManifest(
                    "manifest missing scan_roots for codebase ingestion".to_string(),
                ));
            }
            if self.scan_roots != expected_roots {
                return Err(AosError::InvalidManifest(
                    "manifest scan_roots do not match ingestion metadata".to_string(),
                ));
            }

            if let Some(expected_root) = resolve_scan_root_from_metadata(&self.metadata) {
                if self.scope_scan_root.as_deref() != Some(expected_root.as_str()) {
                    return Err(AosError::InvalidManifest(format!(
                        "scope_scan_root '{}' does not match ingestion metadata '{}'",
                        self.scope_scan_root.as_deref().unwrap_or("<missing>"),
                        expected_root
                    )));
                }
            }
        }

        if is_codebase && !self.metadata.contains_key("stream_mode") {
            return Err(AosError::InvalidManifest(
                "stream_mode missing from ingestion metadata for codebase adapter".to_string(),
            ));
        }

        if let Some(raw) = self.metadata.get("stream_mode") {
            let expected = parse_bool_strict(raw).ok_or_else(|| {
                AosError::InvalidManifest(format!("Invalid stream_mode value '{}'", raw))
            })?;
            if self.stream_mode != Some(expected) {
                return Err(AosError::InvalidManifest(
                    "stream_mode does not match ingestion metadata".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn validate_quantization_metadata(&self) -> Result<()> {
        let quant = self.quantization.as_deref().unwrap_or_default();
        if quant.is_empty() {
            return Err(AosError::InvalidManifest(
                "quantization is required for packaged adapters".to_string(),
            ));
        }
        if quant != LORA_Q15_QUANTIZATION {
            return Err(AosError::InvalidManifest(format!(
                "Unsupported quantization '{}': expected {}",
                quant, LORA_Q15_QUANTIZATION
            )));
        }

        let version = self
            .metadata
            .get("quantization_version")
            .map(|v| v.as_str())
            .unwrap_or("");
        if version != LORA_Q15_VERSION {
            return Err(AosError::InvalidManifest(format!(
                "quantization_version '{}' does not match expected {}",
                version, LORA_Q15_VERSION
            )));
        }

        let strength_version = self
            .metadata
            .get("lora_strength_defaults_version")
            .map(|v| v.as_str())
            .unwrap_or("");
        if strength_version != LORA_STRENGTH_DEFAULTS_VERSION {
            return Err(AosError::InvalidManifest(format!(
                "lora_strength_defaults_version '{}' does not match expected {}",
                strength_version, LORA_STRENGTH_DEFAULTS_VERSION
            )));
        }

        let denom_raw = self.metadata.get("lora_q15_denom").ok_or_else(|| {
            AosError::InvalidManifest("lora_q15_denom missing from metadata".to_string())
        })?;
        let denom = denom_raw.parse::<f32>().map_err(|_| {
            AosError::InvalidManifest(format!(
                "lora_q15_denom '{}' is not a valid number",
                denom_raw
            ))
        })?;
        if (denom - LORA_Q15_DENOM).abs() > f32::EPSILON {
            return Err(AosError::InvalidManifest(format!(
                "lora_q15_denom {} does not match expected {}",
                denom, LORA_Q15_DENOM
            )));
        }

        if self.lora_tier.is_some() && self.lora_strength.is_none() {
            return Err(AosError::InvalidManifest(
                "lora_strength missing for tiered adapter".to_string(),
            ));
        }

        if !self.metadata.contains_key("lora_strength") {
            if let Some(tier) = self.lora_tier {
                if let Some(expected) = default_strength_for_tier(Some(tier)) {
                    if self
                        .lora_strength
                        .map(|strength| (strength - expected).abs() > f32::EPSILON)
                        .unwrap_or(true)
                    {
                        return Err(AosError::InvalidManifest(
                            "lora_strength default does not match expected tier strength"
                                .to_string(),
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Compute the integrity hash from critical manifest fields.
    /// This covers: base_model_hash, training_config_hash, tokenizer_hash, dataset_hash, weights_hash.
    pub fn compute_integrity_hash(&self) -> String {
        use blake3::Hasher;
        let mut hasher = Hasher::new();

        // Order matters: fields are concatenated in deterministic order
        if let Some(ref h) = self.base_model_hash {
            hasher.update(b"base_model_hash:");
            hasher.update(h.as_bytes());
            hasher.update(b"|");
        }
        if let Some(ref h) = self.training_config_hash {
            hasher.update(b"training_config_hash:");
            hasher.update(h.as_bytes());
            hasher.update(b"|");
        }
        if let Some(ref h) = self.tokenizer_hash {
            hasher.update(b"tokenizer_hash:");
            hasher.update(h.as_bytes());
            hasher.update(b"|");
        }
        if let Some(ref h) = self.dataset_hash {
            hasher.update(b"dataset_hash:");
            hasher.update(h.as_bytes());
            hasher.update(b"|");
        }
        hasher.update(b"weights_hash:");
        hasher.update(self.weights_hash.as_bytes());

        hasher.finalize().to_hex().to_string()
    }

    /// Verify the integrity hash matches the computed value.
    /// Returns Ok(()) if valid or if no integrity_hash is set (backward compatibility).
    /// Returns an error with actionable message if verification fails.
    pub fn verify_integrity(&self) -> Result<()> {
        let Some(ref stored_hash) = self.integrity_hash else {
            // Backward compatibility: older .aos files may not have integrity_hash
            debug!(
                adapter_version = %self.version,
                "No integrity_hash present in manifest (legacy .aos file)"
            );
            return Ok(());
        };

        let computed = self.compute_integrity_hash();
        if stored_hash != &computed {
            return Err(AosError::IntegrityViolation(format!(
                "Adapter integrity verification failed: stored hash '{}' does not match computed '{}'. \
                 This may indicate tampering or corruption. \
                 Action: Re-export the adapter from the original training run, or verify the source .aos file.",
                stored_hash, computed
            )));
        }

        debug!(
            integrity_hash = %stored_hash,
            "Adapter integrity verification passed"
        );
        Ok(())
    }

    /// Seal the manifest by computing and storing the integrity hash.
    /// Call this after all fields are set, before serializing to .aos.
    pub fn seal_integrity(&mut self) {
        self.integrity_hash = Some(self.compute_integrity_hash());
    }
}
