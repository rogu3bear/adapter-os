//! AOS format packaging (AdapterPackager impl)

#![allow(clippy::useless_vec)]

use super::coreml::{
    build_coreml_sections, resolve_coreml_placement_spec, validate_quantized_shapes,
};
use super::manifest::AdapterManifest;
use super::metadata::{
    apply_branch_metadata_defaults, apply_codebase_scope_defaults, canonicalize_backend_label,
    default_determinism_mode, default_scope, extract_manifest_fields, normalize_commit_metadata,
    persist_scope_metadata,
};
use super::types::{
    AdapterPlacement, BranchMetadata, CoremlPlacementSpec, CoremlTrainingMetadata, LayerHash,
    PackagedAdapter, PlacementRecord,
};
use crate::training::quantizer::{LoRAQuantizer, QuantizedLoRAWeights};
use crate::training::trainer::{MoETrainingConfig, TrainingConfig};
use crate::training::{
    LORA_Q15_DENOM, LORA_Q15_QUANTIZATION, LORA_Q15_VERSION, LORA_STRENGTH_DEFAULTS_VERSION,
};
use adapteros_aos::{AosWriter, BackendTag};
use adapteros_core::{AosError, RepoAdapterPaths, Result};
use adapteros_crypto::Keypair;
use adapteros_lora_router::ROUTER_GATE_Q15_DENOM;
use adapteros_storage::{
    AdapterLayout, AdapterName, AdapterVersion, ByteStorage, FsByteStorage, FsRefStore, RefStore,
    StorageKey, TrainingMetrics,
};
use safetensors::tensor::TensorView;
use safetensors::SafeTensors;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use walkdir::WalkDir;

const DEFAULT_ARTIFACT_HARD_QUOTA_BYTES: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB
const DEFAULT_ARTIFACT_SOFT_PCT: f64 = 0.8;

impl super::types::AdapterPackager {
    /// Create a new packager with output directory
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Self {
        Self {
            repo_root: output_dir.as_ref().to_path_buf(),
        }
    }

    /// Create a packager using the default adapters directory
    ///
    /// Uses the centralized path from `adapteros_core::RepoAdapterPaths`,
    /// which resolves from environment variable `AOS_ADAPTERS_ROOT`
    /// (with `AOS_ADAPTERS_DIR` compatibility) or defaults to `var/adapters/repo`.
    pub fn with_default_path() -> Self {
        Self {
            repo_root: RepoAdapterPaths::from_env_and_config(None)
                .repo_root
                .to_path_buf(),
        }
    }

    /// Create a packager from config value, falling back to default
    pub fn from_config(adapters_root: Option<&str>) -> Self {
        Self {
            repo_root: RepoAdapterPaths::from_env_and_config(adapters_root.map(|s| s.to_string()))
                .repo_root
                .to_path_buf(),
        }
    }

    fn artifact_quota_limits() -> (u64, u64) {
        let hard = std::env::var("AOS_ARTIFACT_HARD_QUOTA_BYTES")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_ARTIFACT_HARD_QUOTA_BYTES);
        let soft = std::env::var("AOS_ARTIFACT_SOFT_QUOTA_BYTES")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or((hard as f64 * DEFAULT_ARTIFACT_SOFT_PCT) as u64);
        (soft, hard)
    }

    async fn current_artifact_usage(&self, tenant_id: &str) -> Result<u64> {
        let tenant_dir = self.repo_root.join(tenant_id);
        if !tenant_dir.exists() {
            return Ok(0);
        }
        let mut total: u64 = 0;
        for entry in WalkDir::new(&tenant_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "aos" {
                    if let Ok(meta) = tokio::fs::metadata(path).await {
                        total = total.saturating_add(meta.len());
                    }
                }
            }
        }
        Ok(total)
    }

    async fn enforce_artifact_quota(&self, tenant_id: &str, incoming_bytes: u64) -> Result<()> {
        let (soft, hard) = Self::artifact_quota_limits();
        let current = self.current_artifact_usage(tenant_id).await?;
        let predicted = current.saturating_add(incoming_bytes);
        if predicted > hard {
            return Err(AosError::Training(format!(
                "Artifact storage quota exceeded for tenant {}: {} > {} bytes",
                tenant_id, predicted, hard
            )));
        }
        if predicted > soft {
            warn!(
                tenant_id = %tenant_id,
                predicted,
                soft,
                "Artifact storage soft quota exceeded"
            );
        }
        Ok(())
    }

    /// Enrich metadata with deterministic defaults and backend/quantization hints.
    fn build_manifest_metadata(
        metadata: HashMap<String, String>,
        config: &TrainingConfig,
        scope: &str,
    ) -> (HashMap<String, String>, Option<String>, String, String) {
        let mut manifest_metadata = metadata;
        super::metadata::apply_branch_metadata_defaults(&mut manifest_metadata);
        super::metadata::normalize_commit_metadata(&mut manifest_metadata);
        super::metadata::apply_codebase_scope_defaults(&mut manifest_metadata);

        // runtime-only knob; exclude from persisted .aos metadata
        manifest_metadata.remove("routing_determinism_mode");

        // Standard quantization + determinism annotations
        manifest_metadata
            .entry("quantization".to_string())
            .or_insert_with(|| LORA_Q15_QUANTIZATION.to_string());
        manifest_metadata
            .entry("quantization_version".to_string())
            .or_insert_with(|| LORA_Q15_VERSION.to_string());
        manifest_metadata
            .entry("lora_strength_defaults_version".to_string())
            .or_insert_with(|| LORA_STRENGTH_DEFAULTS_VERSION.to_string());
        manifest_metadata
            .entry("lora_q15_denom".to_string())
            .or_insert_with(|| LORA_Q15_DENOM.to_string());
        manifest_metadata
            .entry("gate_q15_denominator".to_string())
            .or_insert_with(|| ROUTER_GATE_Q15_DENOM.to_string());

        let determinism = manifest_metadata
            .entry("determinism".to_string())
            .or_insert_with(super::metadata::default_determinism_mode)
            .clone();

        // Prefer caller-provided backend (actual executed), otherwise derive from config preference
        let training_backend = manifest_metadata
            .get("training_backend")
            .cloned()
            .or_else(|| config.preferred_backend.map(|b| b.tag().to_string()))
            .map(|b| super::metadata::canonicalize_backend_label(&b));

        if let Some(ref backend) = training_backend {
            manifest_metadata.insert("training_backend".to_string(), backend.clone());
        }

        // Persist scope early so hierarchy defaults can use it.
        manifest_metadata
            .entry("scope".to_string())
            .or_insert_with(|| scope.to_string());

        // Hierarchical adapter metadata: derive when missing or placeholder.
        // domain <- category (adapter type), group <- scope (project/tenant), operation <- adapter_name/action.
        let domain = manifest_metadata
            .get("domain")
            .filter(|v| v != &&"unspecified".to_string())
            .cloned()
            .or_else(|| manifest_metadata.get("category").cloned())
            .unwrap_or_else(|| "unspecified".to_string());
        manifest_metadata.insert("domain".to_string(), domain.clone());

        let group = manifest_metadata
            .get("group")
            .filter(|v| v != &&"unspecified".to_string())
            .cloned()
            .or_else(|| manifest_metadata.get("project").cloned())
            .or_else(|| manifest_metadata.get("scope").cloned())
            .unwrap_or_else(|| "unspecified".to_string());
        manifest_metadata.insert("group".to_string(), group.clone());

        let operation = manifest_metadata
            .get("operation")
            .filter(|v| v != &&"unspecified".to_string())
            .cloned()
            .or_else(|| manifest_metadata.get("adapter_name").cloned())
            .or_else(|| manifest_metadata.get("training_action").cloned())
            .unwrap_or_else(|| "unspecified".to_string());
        manifest_metadata.insert("operation".to_string(), operation.clone());

        let scope_path = format!("{}/{}/{}/{}", domain, group, scope, operation);
        manifest_metadata
            .entry("scope_path".to_string())
            .or_insert_with(|| scope_path.clone());

        if !manifest_metadata.contains_key("scope_repo_id") {
            if let Some(repo_id) = manifest_metadata
                .get("repo_identifier")
                .or_else(|| manifest_metadata.get("repo_id"))
                .cloned()
            {
                manifest_metadata.insert("scope_repo_id".to_string(), repo_id);
            }
        }
        if !manifest_metadata.contains_key("repo_identifier") {
            if let Some(repo_id) = manifest_metadata
                .get("scope_repo_id")
                .or_else(|| manifest_metadata.get("repo_id"))
                .cloned()
            {
                manifest_metadata.insert("repo_identifier".to_string(), repo_id);
            }
        }

        (manifest_metadata, training_backend, determinism, scope_path)
    }

    fn adapter_dir(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> std::result::Result<PathBuf, adapteros_core::ResolveError> {
        adapteros_core::adapter_fs_path_with_root(&self.repo_root, tenant_id, adapter_id)
    }

    async fn artifact_usage_for_tenant(&self, tenant_id: &str) -> Result<u64> {
        let root = self.repo_root.join(tenant_id);
        Self::dir_size(&root).await
    }

    async fn dir_size(root: &Path) -> Result<u64> {
        let mut total: u64 = 0;
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await.map_err(|e| {
                AosError::Io(format!("Failed to read dir {}: {}", dir.display(), e))
            })?;
            while let Some(entry) = entries.next_entry().await.map_err(|e| {
                AosError::Io(format!("Failed to read dir entry {}: {}", dir.display(), e))
            })? {
                let path = entry.path();
                let meta = entry.metadata().await.map_err(|e| {
                    AosError::Io(format!("Failed to stat {}: {}", path.display(), e))
                })?;
                if meta.is_dir() {
                    stack.push(path);
                } else {
                    total = total.saturating_add(meta.len());
                }
            }
        }
        Ok(total)
    }

    /// Package adapter with weights and manifest
    pub async fn package(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
    ) -> Result<PackagedAdapter> {
        self.package_with_metadata(
            tenant_id,
            adapter_id,
            weights,
            config,
            base_model,
            HashMap::new(),
        )
        .await
    }

    /// Package adapter with weights, manifest, and metadata
    pub async fn package_with_metadata(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
        metadata: HashMap<String, String>,
    ) -> Result<PackagedAdapter> {
        info!("Packaging adapter: {}", adapter_id);

        // Create adapter directory (canonical tenant-aware path)
        let adapter_dir = self
            .adapter_dir(tenant_id, adapter_id)
            .map_err(|e| AosError::Validation(format!("Invalid adapter path: {}", e)))?;
        tokio::fs::create_dir_all(&adapter_dir).await.map_err(|e| {
            AosError::Training(format!("Failed to create adapter directory: {}", e))
        })?;

        // Serialize weights to safetensors format (adapter-only deltas)
        let weights_path = adapter_dir.join("weights.safetensors");
        let weights_bytes = self
            .save_weights_safetensors(&weights_path, weights)
            .await?;

        // Compute whole-adapter hash + per-layer hashes from the in-memory bytes
        let hash_b3 = blake3::hash(&weights_bytes).to_hex().to_string();
        let per_layer_hashes = Self::compute_per_layer_hashes_from_bytes(&weights_bytes)?;

        let modules = ["q_proj", "k_proj", "v_proj", "o_proj"];
        let (rank, hidden_dim, _, _) = super::coreml::validate_quantized_shapes(weights, config)?;
        let coreml_placement =
            super::coreml::resolve_coreml_placement_spec(&metadata, &modules, rank, hidden_dim)?;
        let base_model_hash = metadata.get("base_model_hash").cloned();

        let scope_value = metadata
            .get("scope")
            .cloned()
            .unwrap_or_else(super::metadata::default_scope);
        let (mut metadata, training_backend, determinism, _scope_path) =
            Self::build_manifest_metadata(metadata, config, &scope_value);

        // Use centralized extraction for consistent metadata parsing
        let fields = super::metadata::extract_manifest_fields(&metadata);
        super::metadata::persist_scope_metadata(&mut metadata, &fields.scope_meta);
        let (coreml, placement, training_backend_details) = super::coreml::build_coreml_sections(
            &metadata,
            training_backend.as_deref(),
            config.rank,
        )?;

        // Create manifest with consistent version format (semver)
        let mut manifest = AdapterManifest {
            version: "1.0.0".to_string(),
            rank: config.rank,
            base_model: base_model.to_string(),
            base_model_hash,
            training_config: config.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: hash_b3.clone(),
            category: fields.category,
            tier: fields.tier,
            per_layer_hashes: Some(per_layer_hashes),
            training_backend,
            determinism,
            coreml_placement: Some(coreml_placement.clone()),
            lora_tier: fields.lora_tier,
            lora_strength: fields.lora_strength,
            scope: scope_value,
            recommended_for_moe: fields.recommended_for_moe,
            quantization: Some(LORA_Q15_QUANTIZATION.to_string()),
            gate_q15_denominator: Some(ROUTER_GATE_Q15_DENOM as u32),
            coreml,
            placement,
            training_backend_details,
            dataset_version_ids: fields.dataset_version_ids,
            data_spec_hash: fields.data_spec_hash,
            data_lineage_mode: fields.data_lineage_mode,
            synthetic_mode: fields.synthetic_mode,
            training_slice_id: fields.training_slice_id,
            backend_policy: fields.backend_policy,
            kernel_version: Some(adapteros_core::version::VERSION.to_string()),
            moe_config: config.moe_config.clone(),
            scope_repo: fields.scope_meta.scope_repo,
            scope_branch: fields.scope_meta.scope_branch,
            scope_commit: fields.scope_meta.scope_commit,
            scope_scan_root: fields.scope_meta.scope_scan_root,
            scope_remote_url: fields.scope_meta.scope_remote_url,
            repo_slug: fields.scope_meta.repo_slug,
            scan_roots: fields.scope_meta.scan_roots,
            session_id: fields.scope_meta.session_id,
            session_name: fields.scope_meta.session_name,
            session_tags: fields.scope_meta.session_tags,
            stream_mode: fields.stream_mode,
            // Integrity fields for provenance tracking (extracted from metadata)
            training_config_hash: fields.training_config_hash.clone(),
            tokenizer_hash: fields.tokenizer_hash.clone(),
            dataset_id: fields.dataset_id.clone(),
            dataset_hash: fields.dataset_hash.clone(),
            integrity_hash: None, // Computed below via seal_integrity()
            metadata,
        };

        // Seal manifest with integrity hash before validation and serialization
        manifest.seal_integrity();
        manifest.validate()?;

        // Serialize manifest once for deterministic signing
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| AosError::Training(format!("Failed to serialize manifest: {}", e)))?;

        // Save manifest
        let manifest_path = adapter_dir.join("manifest.json");
        tokio::fs::write(&manifest_path, &manifest_bytes)
            .await
            .map_err(|e| AosError::Training(format!("Failed to write manifest: {}", e)))?;

        // Deterministic manifest signing (seeded by manifest bytes + adapter_id)
        self.sign_manifest(&adapter_dir, adapter_id, &manifest_bytes)
            .await?;

        info!("Adapter packaged successfully: {}", adapter_id);

        Ok(PackagedAdapter {
            adapter_id: adapter_id.to_string(),
            manifest,
            weights_path,
            hash_b3,
        })
    }

    /// Package adapter as single .aos archive file for a specific tenant.
    ///
    /// Creates a single-file .aos archive containing manifest + weights.
    /// This is the preferred format for distribution and loading into Worker.
    pub async fn package_aos_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
    ) -> Result<PackagedAdapter> {
        self.package_aos_with_metadata(
            tenant_id,
            adapter_id,
            weights,
            config,
            base_model,
            HashMap::new(),
        )
        .await
    }

    /// Package adapter as single .aos archive file (legacy wrapper).
    ///
    /// Uses the default tenant ("default") to preserve compatibility with
    /// existing call sites that are not yet tenant-aware.
    pub async fn package_aos(
        &self,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
    ) -> Result<PackagedAdapter> {
        self.package_aos_for_tenant("default", adapter_id, weights, config, base_model)
            .await
    }

    /// Package adapter as single .aos archive file with metadata
    pub async fn package_aos_with_metadata(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
        metadata: HashMap<String, String>,
    ) -> Result<PackagedAdapter> {
        info!("Packaging adapter as .aos archive: {}", adapter_id);

        let adapter_dir = self
            .adapter_dir(tenant_id, adapter_id)
            .map_err(|e| AosError::Validation(format!("Invalid adapter path: {}", e)))?;
        tokio::fs::create_dir_all(&adapter_dir).await.map_err(|e| {
            AosError::Training(format!("Failed to create adapter directory: {}", e))
        })?;

        // Serialize weights to in-memory safetensors buffer (matches loader expectations)
        let weights_data = Self::build_safetensors_bytes(weights)?;
        let estimate_bytes = weights_data.len() as u64;
        self.enforce_artifact_quota(tenant_id, estimate_bytes)
            .await?;

        // Compute BLAKE3 hash of weights and per-layer hashes
        let hash_b3 = blake3::hash(&weights_data).to_hex().to_string();
        let per_layer_hashes = Self::compute_per_layer_hashes_from_bytes(&weights_data)?;

        let modules = ["q_proj", "k_proj", "v_proj", "o_proj"];
        let (rank, hidden_dim, _, _) = super::coreml::validate_quantized_shapes(weights, config)?;
        let coreml_placement =
            super::coreml::resolve_coreml_placement_spec(&metadata, &modules, rank, hidden_dim)?;
        let base_model_hash = metadata.get("base_model_hash").cloned();

        let scope_value = metadata
            .get("scope")
            .cloned()
            .unwrap_or_else(super::metadata::default_scope);
        let (mut metadata, training_backend, determinism, scope_path) =
            Self::build_manifest_metadata(metadata, config, &scope_value);

        let fields = super::metadata::extract_manifest_fields(&metadata);
        super::metadata::persist_scope_metadata(&mut metadata, &fields.scope_meta);
        let (coreml, placement, training_backend_details) = super::coreml::build_coreml_sections(
            &metadata,
            training_backend.as_deref(),
            config.rank,
        )?;

        // Create manifest
        let mut manifest = AdapterManifest {
            version: "2.0".to_string(), // AOS 2.0 format
            rank: config.rank,
            base_model: base_model.to_string(),
            base_model_hash,
            training_config: config.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: hash_b3.clone(),
            category: fields.category,
            tier: fields.tier,
            per_layer_hashes: Some(per_layer_hashes),
            training_backend,
            determinism,
            coreml_placement: Some(coreml_placement.clone()),
            lora_tier: fields.lora_tier,
            lora_strength: fields.lora_strength,
            scope: scope_value.clone(),
            recommended_for_moe: fields.recommended_for_moe,
            quantization: Some(LORA_Q15_QUANTIZATION.to_string()),
            gate_q15_denominator: Some(ROUTER_GATE_Q15_DENOM as u32),
            coreml,
            placement,
            training_backend_details,
            dataset_version_ids: fields.dataset_version_ids,
            data_spec_hash: fields.data_spec_hash,
            data_lineage_mode: fields.data_lineage_mode,
            synthetic_mode: fields.synthetic_mode,
            training_slice_id: fields.training_slice_id,
            backend_policy: fields.backend_policy,
            kernel_version: Some(adapteros_core::version::VERSION.to_string()),
            moe_config: config.moe_config.clone(),
            scope_repo: fields.scope_meta.scope_repo,
            scope_branch: fields.scope_meta.scope_branch,
            scope_commit: fields.scope_meta.scope_commit,
            scope_scan_root: fields.scope_meta.scope_scan_root,
            scope_remote_url: fields.scope_meta.scope_remote_url,
            repo_slug: fields.scope_meta.repo_slug,
            scan_roots: fields.scope_meta.scan_roots,
            session_id: fields.scope_meta.session_id,
            session_name: fields.scope_meta.session_name,
            session_tags: fields.scope_meta.session_tags,
            stream_mode: fields.stream_mode,
            // Integrity fields for provenance tracking (extracted from metadata)
            training_config_hash: fields.training_config_hash.clone(),
            tokenizer_hash: fields.tokenizer_hash.clone(),
            dataset_id: fields.dataset_id.clone(),
            dataset_hash: fields.dataset_hash.clone(),
            integrity_hash: None, // Computed below via seal_integrity()
            metadata,
        };

        // Seal manifest with integrity hash before validation and serialization
        manifest.seal_integrity();
        manifest.validate()?;

        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| AosError::Training(format!("Failed to serialize manifest: {}", e)))?;

        let (soft_quota, hard_quota) = Self::artifact_quota_limits();
        let current_usage = self.artifact_usage_for_tenant(tenant_id).await.unwrap_or(0);
        let expected_bytes = weights_data.len() as u64 + manifest_bytes.len() as u64;
        let predicted = current_usage + expected_bytes;
        if predicted > hard_quota {
            return Err(AosError::Validation(format!(
                "Artifact storage quota exceeded: {} > {} bytes",
                predicted, hard_quota
            )));
        }
        if predicted > soft_quota {
            warn!(
                tenant_id = %tenant_id,
                predicted,
                soft_quota,
                "Artifact storage soft quota exceeded"
            );
        }

        // Build the AOS archive in memory to compute final hash
        let mut writer = AosWriter::new();
        writer.add_segment(
            BackendTag::Canonical,
            Some(scope_path.clone()),
            &weights_data,
        )?;

        // Write to a temporary location first to get the archive bytes
        let temp_dir = tempfile::tempdir().map_err(|e| {
            AosError::Training(format!("Failed to create temp directory: {}", e))
        })?;
        let temp_path = temp_dir.path().join("temp.aos");
        writer.write_archive(&temp_path, &manifest)?;

        // Read the archive and compute the final content hash
        let archive_bytes = tokio::fs::read(&temp_path).await.map_err(|e| {
            AosError::Training(format!("Failed to read temp archive: {}", e))
        })?;
        let archive_hash = blake3::hash(&archive_bytes).to_hex().to_string();

        // Set up adapter layout for content-addressed storage
        let adapter_layout = AdapterLayout::new(&self.repo_root);
        let content_addressed_path = adapter_layout.object_path(&archive_hash);

        // Create parent directories for the content-addressed path
        if let Some(parent) = content_addressed_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AosError::Training(format!(
                    "Failed to create objects directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        // Copy to content-addressed location
        tokio::fs::copy(&temp_path, &content_addressed_path)
            .await
            .map_err(|e| {
                AosError::Training(format!(
                    "Failed to copy archive to {}: {}",
                    content_addressed_path.display(),
                    e
                ))
            })?;

        // Deterministic signature for the archive to allow reproducible verification
        self.sign_archive(&content_addressed_path, adapter_id)
            .await?;

        // Create/update the draft ref pointing to the new hash
        let adapter_name = AdapterName::subject(adapter_id);
        let ref_store = FsRefStore::new(adapter_layout.clone());
        ref_store
            .update_ref(&adapter_name, tenant_id, "draft", &archive_hash)
            .await
            .map_err(|e| {
                AosError::Training(format!("Failed to update draft ref: {}", e))
            })?;

        // Create AdapterVersion record with version metadata
        let mut version = AdapterVersion::new(
            archive_hash.clone(),
            adapter_name.clone(),
            manifest.version.clone(),
        )
        .with_size(archive_bytes.len() as u64)
        .with_base_model(base_model);

        // Add tenant_id to metadata for index lookups
        version
            .metadata
            .insert("tenant_id".to_string(), tenant_id.to_string());

        // Extract training metrics if available in manifest metadata
        if let Some(final_loss) = manifest
            .metadata
            .get("final_loss")
            .and_then(|v| v.parse::<f64>().ok())
        {
            version.training_metrics = Some(TrainingMetrics {
                final_loss,
                epochs: manifest
                    .metadata
                    .get("epochs")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(config.epochs as u32),
                steps: manifest
                    .metadata
                    .get("steps")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                learning_rate: Some(config.learning_rate as f64),
                validation_loss: manifest
                    .metadata
                    .get("validation_loss")
                    .and_then(|v| v.parse().ok()),
            });
        }

        // Get parent hash from current ref if this is an update
        if let Ok(Some(current_hash)) = ref_store.get_ref(&adapter_name, tenant_id, "current").await
        {
            version.parent_hash = Some(current_hash);
        }

        info!(
            path = %content_addressed_path.display(),
            hash = %archive_hash,
            size_kb = archive_bytes.len() / 1024,
            "AOS archive stored in content-addressed location"
        );

        Ok(PackagedAdapter {
            adapter_id: adapter_id.to_string(),
            manifest,
            weights_path: content_addressed_path,
            hash_b3: archive_hash,
        })
    }

    /// Package adapter as single .aos archive with branch metadata.
    ///
    /// This method provides a more ergonomic way to include branch and commit
    /// information in the packaged training artifacts. The branch metadata is
    /// merged into the packaging metadata and preserved in the manifest.
    ///
    /// # Arguments
    ///
    /// * `tenant_id` - Tenant identifier for multi-tenant isolation
    /// * `adapter_id` - Unique identifier for this adapter
    /// * `weights` - Quantized LoRA weights to package
    /// * `config` - Training configuration used to produce these weights
    /// * `base_model` - Base model identifier this adapter is trained against
    /// * `branch_metadata` - Git branch and commit information for provenance
    /// * `metadata` - Additional metadata to include in the manifest
    ///
    /// # Example
    ///
    /// ```ignore
    /// let branch_meta = BranchMetadata::new("main", "abc123def")
    ///     .with_repo_name("my-repo")
    ///     .with_remote_url("https://github.com/org/repo");
    ///
    /// let packaged = packager
    ///     .package_aos_with_branch_metadata(
    ///         "tenant-1",
    ///         "adapter-001",
    ///         &weights,
    ///         &config,
    ///         "llama-3.2",
    ///         &branch_meta,
    ///         HashMap::new(),
    ///     )
    ///     .await?;
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub async fn package_aos_with_branch_metadata(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
        branch_metadata: &BranchMetadata,
        metadata: HashMap<String, String>,
    ) -> Result<PackagedAdapter> {
        // Merge branch metadata into the packaging metadata
        let mut enriched_metadata = metadata;
        for (key, value) in branch_metadata.to_metadata_entries() {
            // Only insert if not already present (explicit metadata takes precedence)
            enriched_metadata.entry(key).or_insert(value);
        }

        // Log branch metadata inclusion for auditability
        if branch_metadata.is_present() {
            info!(
                adapter_id = %adapter_id,
                branch = ?branch_metadata.branch,
                commit = ?branch_metadata.commit,
                repo = ?branch_metadata.repo_name,
                "Including branch metadata in packaged adapter"
            );
        }

        self.package_aos_with_metadata(
            tenant_id,
            adapter_id,
            weights,
            config,
            base_model,
            enriched_metadata,
        )
        .await
    }

    /// Save weights in safetensors format
    async fn save_weights_safetensors(
        &self,
        path: &Path,
        weights: &QuantizedLoRAWeights,
    ) -> Result<Vec<u8>> {
        let data = Self::build_safetensors_bytes(weights)?;

        tokio::fs::write(path, &data)
            .await
            .map_err(|e| AosError::Training(format!("Failed to write weights: {}", e)))?;

        Ok(data)
    }

    /// Compute BLAKE3 hash of file
    async fn compute_hash(&self, path: &Path) -> Result<String> {
        let data = tokio::fs::read(path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read file for hashing: {}", e)))?;

        let hash = blake3::hash(&data);
        Ok(hash.to_hex().to_string())
    }

    /// Canonical logical layer path for manifest keys (e.g., transformer.layer_12.attn.q_proj.lora_A)
    fn canonical_layer_id(tensor_name: &str) -> String {
        let mut segments = Vec::new();
        let mut iter = tensor_name.split(['.', '/']).peekable();

        while let Some(seg) = iter.next() {
            if seg.is_empty() {
                continue;
            }

            let lower = seg.to_lowercase();
            if lower == "weight" {
                continue;
            }

            if lower == "model" || lower == "transformer" {
                if segments.is_empty() {
                    segments.push("transformer".to_string());
                }
                continue;
            }

            if lower == "layers" || lower == "layer" {
                if let Some(next) = iter.peek() {
                    if let Ok(idx) = next.parse::<usize>() {
                        segments.push(format!("layer_{}", idx));
                        iter.next();
                        continue;
                    }
                }
            }

            let normalized = match lower.as_str() {
                "lora_a" => "lora_A".to_string(),
                "lora_b" => "lora_B".to_string(),
                other => other.to_string(),
            };

            segments.push(normalized);
        }

        if segments.is_empty() {
            return tensor_name.to_string();
        }

        if segments.first().map(|s| s.as_str()) != Some("transformer") {
            let mut prefixed = vec!["transformer".to_string()];
            prefixed.extend(segments);
            segments = prefixed;
        }

        segments.join(".")
    }

    /// Serialize quantized weights into safetensors bytes (adapter-only, no base weights)
    ///
    /// Supports both multi-module and single-module (legacy) weights:
    /// - Multi-module: Each module gets its own trained weights
    /// - Single-module: Same weights are replicated across default modules
    fn build_safetensors_bytes(weights: &QuantizedLoRAWeights) -> Result<Vec<u8>> {
        // Dequantize to f32 for runtime backends
        let deq = LoRAQuantizer::dequantize_from_q15(weights);

        // Flatten helper
        fn flatten_2d(m: &[Vec<f32>]) -> Vec<u8> {
            let mut out = Vec::with_capacity(m.len() * m.first().map(|r| r.len()).unwrap_or(0) * 4);
            for row in m {
                for &v in row {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
            out
        }

        // Check if multi-module weights are present
        if deq.is_multi_module() {
            // Multi-module: serialize each module's own trained weights
            info!(
                "Packaging multi-module adapter with {} modules",
                deq.modules.len()
            );

            // Collect all flattened bytes first (safetensors needs stable references)
            #[allow(clippy::type_complexity)]
            let mut module_bytes: Vec<(
                String,
                Vec<u8>,
                Vec<u8>,
                usize,
                usize,
                usize,
                usize,
            )> = Vec::new();
            for (name, module_weights) in &deq.modules {
                let a_rows = module_weights.lora_a.len();
                let a_cols = module_weights.lora_a.first().map(|r| r.len()).unwrap_or(0);
                let b_rows = module_weights.lora_b.len();
                let b_cols = module_weights.lora_b.first().map(|r| r.len()).unwrap_or(0);
                let a_bytes = flatten_2d(&module_weights.lora_a);
                let b_bytes = flatten_2d(&module_weights.lora_b);
                module_bytes.push((
                    name.clone(),
                    a_bytes,
                    b_bytes,
                    a_rows,
                    a_cols,
                    b_rows,
                    b_cols,
                ));
            }

            // Build tensor views
            let mut tensors: Vec<(String, TensorView)> = Vec::new();
            for (name, a_bytes, b_bytes, a_rows, a_cols, b_rows, b_cols) in &module_bytes {
                let a_view = TensorView::new(
                    safetensors::Dtype::F32,
                    vec![*a_rows, *a_cols],
                    a_bytes.as_slice(),
                )
                .map_err(|e| {
                    AosError::Training(format!("safetensors A view error for {}: {}", name, e))
                })?;
                let b_view = TensorView::new(
                    safetensors::Dtype::F32,
                    vec![*b_rows, *b_cols],
                    b_bytes.as_slice(),
                )
                .map_err(|e| {
                    AosError::Training(format!("safetensors B view error for {}: {}", name, e))
                })?;
                tensors.push((format!("lora_a.{}", name), a_view));
                tensors.push((format!("lora_b.{}", name), b_view));
            }

            debug_assert!(
                tensors
                    .iter()
                    .all(|(name, _)| name.starts_with("lora_a.") || name.starts_with("lora_b.")),
                "packager must only serialize LoRA tensors; base weights are excluded"
            );

            safetensors::serialize(tensors, &Default::default())
                .map_err(|e| AosError::Training(format!("safetensors serialize error: {}", e)))
        } else {
            // Legacy single-module: replicate same weights for default modules
            let modules = ["q_proj", "k_proj", "v_proj", "o_proj"];
            let mut tensors: Vec<(String, TensorView)> = Vec::new();

            let a_rows = deq.lora_a.len(); // rank
            let a_cols = deq.lora_a.first().map(|r| r.len()).unwrap_or(0); // hidden_dim
            let b_rows = deq.lora_b.len(); // hidden_dim
            let b_cols = deq.lora_b.first().map(|r| r.len()).unwrap_or(0); // rank

            let a_bytes = flatten_2d(&deq.lora_a);
            let b_bytes = flatten_2d(&deq.lora_b);

            for name in modules.iter() {
                let a_view = TensorView::new(
                    safetensors::Dtype::F32,
                    vec![a_rows, a_cols],
                    a_bytes.as_slice(),
                )
                .map_err(|e| AosError::Training(format!("safetensors A view error: {}", e)))?;
                let b_view = TensorView::new(
                    safetensors::Dtype::F32,
                    vec![b_rows, b_cols],
                    b_bytes.as_slice(),
                )
                .map_err(|e| AosError::Training(format!("safetensors B view error: {}", e)))?;
                tensors.push((format!("lora_a.{}", name), a_view));
                tensors.push((format!("lora_b.{}", name), b_view));
            }

            debug_assert!(
                tensors
                    .iter()
                    .all(|(name, _)| name.starts_with("lora_a.") || name.starts_with("lora_b.")),
                "packager must only serialize LoRA tensors; base weights are excluded"
            );

            safetensors::serialize(tensors, &Default::default())
                .map_err(|e| AosError::Training(format!("safetensors serialize error: {}", e)))
        }
    }

    fn compute_per_layer_hashes_from_bytes(
        weights_bytes: &[u8],
    ) -> Result<std::collections::HashMap<String, LayerHash>> {
        let tensors = SafeTensors::deserialize(weights_bytes).map_err(|e| {
            AosError::Training(format!(
                "Failed to parse safetensors for per-layer hashing: {e}"
            ))
        })?;

        let mut hashes = std::collections::HashMap::new();
        for (name, tensor) in tensors.tensors() {
            let canonical = Self::canonical_layer_id(&name);
            let hash = blake3::hash(tensor.data()).to_hex().to_string();
            if hashes
                .insert(
                    canonical.clone(),
                    LayerHash {
                        hash,
                        tensor_name: Some(name.to_string()),
                    },
                )
                .is_some()
            {
                return Err(AosError::Training(format!(
                    "Duplicate canonical layer id detected while hashing: {}",
                    canonical
                )));
            }
        }

        Ok(hashes)
    }

    /// Deterministic manifest signing using Ed25519 seeded from manifest bytes
    async fn sign_manifest(
        &self,
        adapter_dir: &Path,
        adapter_id: &str,
        manifest_bytes: &[u8],
    ) -> Result<()> {
        let keypair = Self::load_signing_keypair("manifest", adapter_id, manifest_bytes)?;
        let signature = keypair.sign(manifest_bytes);

        // Save signature
        let sig_path = adapter_dir.join("signature.sig");
        tokio::fs::write(&sig_path, signature.to_bytes())
            .await
            .map_err(|e| AosError::Training(format!("Failed to write signature: {}", e)))?;

        // Save public key (hex-encoded)
        let pubkey_path = adapter_dir.join("public_key.pem");
        let pubkey_hex = hex::encode(keypair.public_key().to_bytes());
        tokio::fs::write(&pubkey_path, pubkey_hex)
            .await
            .map_err(|e| AosError::Training(format!("Failed to write public key: {}", e)))?;

        info!("Adapter manifest signed deterministically");
        Ok(())
    }

    /// Deterministic archive signing for .aos outputs
    async fn sign_archive(&self, aos_path: &Path, adapter_id: &str) -> Result<()> {
        let archive_bytes = tokio::fs::read(aos_path).await.map_err(|e| {
            AosError::Training(format!("Failed to read archive for signing: {}", e))
        })?;
        let keypair = Self::load_signing_keypair("aos-archive", adapter_id, &archive_bytes)?;
        let signature = keypair.sign(&archive_bytes);

        let sig_path = aos_path.with_extension("aos.sig");
        tokio::fs::write(&sig_path, signature.to_bytes())
            .await
            .map_err(|e| AosError::Training(format!("Failed to write archive signature: {}", e)))?;

        let pubkey_path = aos_path.with_extension("aos.pub");
        let pubkey_hex = hex::encode(keypair.public_key().to_bytes());
        tokio::fs::write(&pubkey_path, pubkey_hex)
            .await
            .map_err(|e| {
                AosError::Training(format!("Failed to write archive public key: {}", e))
            })?;

        info!(
            path = %aos_path.display(),
            sig = %sig_path.display(),
            "AOS archive signed deterministically"
        );

        Ok(())
    }

    fn deterministic_keypair(label: &str, adapter_id: &str, material: &[u8]) -> Keypair {
        let mut hasher = blake3::Hasher::new();
        hasher.update(label.as_bytes());
        hasher.update(adapter_id.as_bytes());
        hasher.update(material);
        let hash = hasher.finalize();
        Keypair::from_bytes(hash.as_bytes())
    }

    /// Load signing keypair: prefer env-provided Ed25519 seed (32-byte hex), fall back to deterministic.
    fn load_signing_keypair(label: &str, adapter_id: &str, material: &[u8]) -> Result<Keypair> {
        if let Ok(hex_seed) = std::env::var("AOS_SIGNING_KEY_HEX") {
            let bytes = hex::decode(hex_seed.trim())
                .map_err(|e| AosError::Training(format!("Invalid AOS_SIGNING_KEY_HEX: {}", e)))?;
            if bytes.len() != 32 {
                return Err(AosError::Training(
                    "AOS_SIGNING_KEY_HEX must be 32 bytes".to_string(),
                ));
            }
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes);
            return Ok(Keypair::from_bytes(&seed));
        }

        // Deterministic fallback (test/dev only)
        Ok(Self::deterministic_keypair(label, adapter_id, material))
    }

    /// Verify adapter signature
    pub async fn verify_signature(&self, adapter_dir: &Path) -> Result<bool> {
        // Read manifest
        let manifest_path = adapter_dir.join("manifest.json");
        let manifest_data = tokio::fs::read(&manifest_path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read manifest: {}", e)))?;

        // Read signature
        let sig_path = adapter_dir.join("signature.sig");
        let sig_bytes = tokio::fs::read(&sig_path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read signature: {}", e)))?;

        let sig_array: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| AosError::Training("Invalid signature length".to_string()))?;

        let signature = adapteros_crypto::Signature::from_bytes(&sig_array)
            .map_err(|e| AosError::Training(format!("Invalid signature: {}", e)))?;

        // Read public key
        let pubkey_path = adapter_dir.join("public_key.pem");
        let pubkey_hex = tokio::fs::read_to_string(&pubkey_path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read public key: {}", e)))?;

        let pubkey_bytes = hex::decode(pubkey_hex.trim())
            .map_err(|e| AosError::Training(format!("Invalid public key hex: {}", e)))?;

        let pubkey_array: [u8; 32] = pubkey_bytes
            .try_into()
            .map_err(|_| AosError::Training("Invalid public key length".to_string()))?;

        let public_key = adapteros_crypto::PublicKey::from_bytes(&pubkey_array)
            .map_err(|e| AosError::Training(format!("Invalid public key: {}", e)))?;

        // Verify signature
        public_key
            .verify(&manifest_data, &signature)
            .map_err(|e| AosError::Training(format!("Signature verification failed: {}", e)))?;

        Ok(true)
    }

    /// Load packaged adapter
    pub async fn load(&self, tenant_id: &str, adapter_id: &str) -> Result<PackagedAdapter> {
        let adapter_dir = self
            .adapter_dir(tenant_id, adapter_id)
            .map_err(|e| AosError::Validation(format!("Invalid adapter path: {}", e)))?;

        // Verify signature first
        if !self.verify_signature(&adapter_dir).await? {
            return Err(AosError::Training(format!(
                "Signature verification failed for adapter: {}",
                adapter_id
            )));
        }

        // Load manifest
        let manifest_path = adapter_dir.join("manifest.json");
        let manifest_data = tokio::fs::read(&manifest_path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read manifest: {}", e)))?;

        let manifest: AdapterManifest = serde_json::from_slice(&manifest_data)
            .map_err(|e| AosError::Training(format!("Failed to parse manifest: {}", e)))?;

        let weights_path = adapter_dir.join("weights.safetensors");
        let hash_b3 = self.compute_hash(&weights_path).await?;

        // Verify hash matches manifest
        if hash_b3 != manifest.weights_hash {
            return Err(AosError::Training(format!(
                "Hash mismatch: expected {}, got {}",
                manifest.weights_hash, hash_b3
            )));
        }

        Ok(PackagedAdapter {
            adapter_id: adapter_id.to_string(),
            manifest,
            weights_path,
            hash_b3,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::packager::coreml as pkg_coreml;
    use crate::training::packager::metadata as pkg_metadata;
    use crate::training::packager::types::{
        AdapterPackager, BranchMetadata, CoremlPlacementSpec, ScanRootMetadata,
    };
    use crate::training::trainer::TrainingConfig;
    use crate::training::LORA_Q15_QUANTIZATION;
    use adapteros_lora_router::ROUTER_GATE_Q15_DENOM;
    use adapteros_types::coreml::{
        CoreMLOpKind, CoreMLPlacementBinding, CoreMLPlacementShape, CoreMLProjection,
        CoreMLTargetRef,
    };
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_compute_hash() {
        let temp_dir = tempfile::Builder::new()
            .prefix("aos-test-")
            .tempdir()
            .expect("failed to create temporary directory for compute_hash test - check disk space and permissions");
        let test_file = temp_dir.path().join("test.txt");
        tokio::fs::write(&test_file, b"hello world").await.unwrap();

        let packager = AdapterPackager::new(temp_dir.path());
        let hash: String = packager.compute_hash(&test_file).await.unwrap();

        assert_eq!(hash.len(), 64); // BLAKE3 produces 256-bit hash (64 hex chars)
    }

    #[tokio::test]
    async fn test_save_load_manifest() {
        let temp_dir = tempfile::Builder::new()
            .prefix("aos-test-")
            .tempdir()
            .expect("failed to create temporary directory for manifest save/load test - check disk space");
        let manifest_path = temp_dir.path().join("manifest.json");

        let manifest = AdapterManifest {
            version: "1.0.0".to_string(),
            rank: 4,
            base_model: "test-model".to_string(),
            base_model_hash: None,
            training_config: TrainingConfig::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: "test_hash".to_string(),
            category: pkg_metadata::default_category(),
            tier: pkg_metadata::default_tier(),
            per_layer_hashes: None,
            training_backend: Some("cpu".to_string()),
            determinism: pkg_metadata::default_determinism_mode(),
            lora_tier: None,
            lora_strength: None,
            scope: pkg_metadata::default_scope(),
            recommended_for_moe: true,
            quantization: Some(LORA_Q15_QUANTIZATION.to_string()),
            gate_q15_denominator: Some(ROUTER_GATE_Q15_DENOM as u32),
            coreml: None,
            placement: None,
            training_backend_details: None,
            coreml_placement: None,
            dataset_version_ids: None,
            data_spec_hash: None,
            data_lineage_mode: None,
            synthetic_mode: None,
            training_slice_id: None,
            backend_policy: None,
            kernel_version: None,
            moe_config: None,
            scope_repo: None,
            scope_branch: None,
            scope_commit: None,
            scope_scan_root: None,
            scope_remote_url: None,
            repo_slug: None,
            scan_roots: Vec::new(),
            session_id: None,
            session_name: None,
            session_tags: None,
            stream_mode: None,
            dataset_id: None,
            dataset_hash: None,
            integrity_hash: None,
            training_config_hash: None,
            tokenizer_hash: None,
            metadata: std::collections::HashMap::new(),
        };

        let _packager = AdapterPackager::new(temp_dir.path());
        let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        tokio::fs::write(&manifest_path, manifest_bytes)
            .await
            .unwrap();

        // Load and verify
        let loaded_data = tokio::fs::read(&manifest_path).await.unwrap();
        let loaded_manifest: AdapterManifest = serde_json::from_slice(&loaded_data).unwrap();

        assert_eq!(loaded_manifest.rank, 4);
        assert_eq!(loaded_manifest.base_model, "test-model");
    }

    #[tokio::test]
    async fn artifact_quota_enforces_hard_limit() {
        let temp_dir = tempfile::Builder::new()
            .prefix("aos-test-")
            .tempdir()
            .expect("failed to create temporary directory for artifact quota test - check disk space");
        let tenant_dir = temp_dir.path().join("tenant1").join("adapter");
        tokio::fs::create_dir_all(&tenant_dir).await.unwrap();
        let existing = tenant_dir.join("v1.aos");
        tokio::fs::write(&existing, vec![0u8; 8]).await.unwrap();

        std::env::set_var("AOS_ARTIFACT_HARD_QUOTA_BYTES", "10");
        std::env::set_var("AOS_ARTIFACT_SOFT_QUOTA_BYTES", "8");

        let packager = AdapterPackager::new(temp_dir.path());
        let result = packager.enforce_artifact_quota("tenant1", 5).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_per_layer_hashes_use_canonical_ids() {
        use safetensors::tensor::TensorView;

        let lora_bytes: Vec<u8> = vec![0.1f32, 0.2, 0.3, 0.4]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let tensors = [(
            "model.layers.0.attn.q_proj.lora_A.weight".to_string(),
            TensorView::new(safetensors::Dtype::F32, vec![2, 2], &lora_bytes).unwrap(),
        )];

        let serialized = safetensors::tensor::serialize(tensors, &None).unwrap();
        let hashes = AdapterPackager::compute_per_layer_hashes_from_bytes(&serialized)
            .expect("failed to compute per-layer hashes from serialized safetensors - serialization format should be valid");

        let canonical = "transformer.layer_0.attn.q_proj.lora_A";
        let entry = hashes.get(canonical).expect(
            "canonical layer entry 'transformer.layer_0.attn.q_proj.lora_A' should exist after hashing - \
             layer normalization should have created this key from 'model.layers.0.attn.q_proj.lora_A.weight'"
        );
        assert_eq!(
            entry.tensor_name.as_deref(),
            Some("model.layers.0.attn.q_proj.lora_A.weight")
        );
        assert!(!entry.hash.is_empty());
    }

    #[test]
    fn manifest_prefers_actual_backend_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("training_backend".to_string(), "mlx".to_string());
        metadata.insert(
            "training_backend_reason".to_string(),
            "coreml_unavailable".to_string(),
        );
        let (_meta, training_backend, _determinism, _scope) =
            AdapterPackager::build_manifest_metadata(
                metadata,
                &TrainingConfig::default(),
                "project",
            );

        assert_eq!(training_backend.as_deref(), Some("mlx"));
    }

    #[test]
    fn manifest_keeps_backend_reason_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("training_backend".to_string(), "cpu".to_string());
        metadata.insert(
            "training_backend_reason".to_string(),
            "coreml_unavailable".to_string(),
        );
        let config = TrainingConfig::default();
        let (meta, _backend, _determinism, _scope) =
            AdapterPackager::build_manifest_metadata(metadata, &config, "project");

        assert_eq!(
            meta.get("training_backend_reason").map(String::as_str),
            Some("coreml_unavailable")
        );
    }

    #[test]
    fn derives_domain_group_operation_from_defaults() {
        let mut metadata = HashMap::new();
        metadata.insert("adapter_name".to_string(), "my-adapter".to_string());
        metadata.insert("category".to_string(), "code".to_string());
        metadata.insert("scope".to_string(), "project".to_string());

        let (enriched, _, _, scope_path) = AdapterPackager::build_manifest_metadata(
            metadata,
            &TrainingConfig::default(),
            "tenant",
        );

        assert_eq!(enriched.get("domain").unwrap(), "code");
        assert_eq!(enriched.get("group").unwrap(), "project");
        assert_eq!(enriched.get("operation").unwrap(), "my-adapter");
        assert_eq!(scope_path, "code/project/tenant/my-adapter");
    }

    #[test]
    fn respects_provided_hierarchy_overrides() {
        let mut metadata = HashMap::new();
        metadata.insert("domain".to_string(), "custom-domain".to_string());
        metadata.insert("group".to_string(), "custom-group".to_string());
        metadata.insert("operation".to_string(), "custom-op".to_string());

        let (enriched, _, _, scope_path) = AdapterPackager::build_manifest_metadata(
            metadata,
            &TrainingConfig::default(),
            "tenant",
        );

        assert_eq!(enriched.get("domain").unwrap(), "custom-domain");
        assert_eq!(enriched.get("group").unwrap(), "custom-group");
        assert_eq!(enriched.get("operation").unwrap(), "custom-op");
        assert_eq!(scope_path, "custom-domain/custom-group/tenant/custom-op");
    }

    #[test]
    fn invalid_coreml_placement_is_rejected() {
        let mut metadata = HashMap::new();
        let bad_spec = CoremlPlacementSpec {
            version: 1,
            graph_id: Some("graph".to_string()),
            bindings: vec![CoreMLPlacementBinding {
                binding_id: "q_proj".to_string(),
                target: CoreMLTargetRef {
                    layer: "q_proj".to_string(),
                    op_kind: CoreMLOpKind::AttentionQ,
                    path_hint: None,
                },
                projection: CoreMLProjection::InputToHidden,
                rank: 8, // wrong rank/hidden_dim for this test
                alpha: None,
                scale: None,
                gating: None,
                shape: CoreMLPlacementShape {
                    input_dim: 16,
                    output_dim: 16,
                },
            }],
        };
        metadata.insert(
            "coreml_placement".to_string(),
            serde_json::to_string(&bad_spec).unwrap(),
        );

        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 32,
            ..Default::default()
        };

        let err = pkg_coreml::resolve_coreml_placement_spec(
            &metadata,
            &["q_proj"],
            config.rank,
            config.hidden_dim,
        );

        assert!(err.is_err());
    }

    #[test]
    fn default_coreml_placement_covers_modules() {
        let modules = ["q_proj", "o_proj"];
        let spec = pkg_coreml::default_coreml_placement_spec(&modules, 4, 32);
        assert_eq!(spec.version, 1);
        assert_eq!(spec.bindings.len(), modules.len());
        for binding in spec.bindings {
            assert_eq!(binding.rank, 4);
            assert_eq!(binding.shape.input_dim, 32);
            assert_eq!(binding.shape.output_dim, 32);
        }
    }

    #[test]
    fn artifact_quota_limits_respect_env() {
        std::env::set_var("AOS_ARTIFACT_HARD_QUOTA_BYTES", "1000");
        std::env::set_var("AOS_ARTIFACT_SOFT_QUOTA_BYTES", "800");
        let (soft, hard) = AdapterPackager::artifact_quota_limits();
        assert_eq!(hard, 1000);
        assert_eq!(soft, 800);
        std::env::remove_var("AOS_ARTIFACT_HARD_QUOTA_BYTES");
        std::env::remove_var("AOS_ARTIFACT_SOFT_QUOTA_BYTES");
    }

    #[test]
    fn parse_scan_roots_from_json_array() {
        let mut metadata = HashMap::new();
        let scan_roots_json = r#"[
            {"path": "src", "label": "main", "file_count": 100, "byte_count": 50000},
            {"path": "lib", "label": "library"}
        ]"#;
        metadata.insert("scan_roots".to_string(), scan_roots_json.to_string());

        let roots = pkg_metadata::parse_scan_roots_from_metadata(&metadata);
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].path, "src");
        assert_eq!(roots[0].label, Some("main".to_string()));
        assert_eq!(roots[0].file_count, Some(100));
        assert_eq!(roots[0].byte_count, Some(50000));
        assert_eq!(roots[1].path, "lib");
        assert_eq!(roots[1].label, Some("library".to_string()));
        assert_eq!(roots[1].file_count, None);
    }

    #[test]
    fn parse_scan_roots_from_scope_scan_root_fallback() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_scan_root".to_string(), "/project/src".to_string());
        metadata.insert("scan_root_label".to_string(), "primary".to_string());
        metadata.insert("scan_root_file_count".to_string(), "42".to_string());
        metadata.insert("scan_root_content_hash".to_string(), "abc123".to_string());

        let roots = pkg_metadata::parse_scan_roots_from_metadata(&metadata);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/project/src");
        assert_eq!(roots[0].label, Some("primary".to_string()));
        assert_eq!(roots[0].file_count, Some(42));
        assert_eq!(roots[0].content_hash, Some("abc123".to_string()));
    }

    #[test]
    fn parse_scan_roots_prefers_relative_paths() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "scan_root_relative".to_string(),
            "packages/core".to_string(),
        );
        metadata.insert(
            "scan_root_path".to_string(),
            "/repo/packages/core".to_string(),
        );

        let roots = pkg_metadata::parse_scan_roots_from_metadata(&metadata);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "packages/core");
    }

    #[test]
    fn parse_scan_roots_returns_empty_for_no_data() {
        let metadata = HashMap::new();
        let roots = pkg_metadata::parse_scan_roots_from_metadata(&metadata);
        assert!(roots.is_empty());
    }

    #[test]
    fn extract_scope_metadata_from_canonical_keys() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_repo".to_string(), "my-repo".to_string());
        metadata.insert("repo_slug".to_string(), "my_repo".to_string());
        metadata.insert("scope_branch".to_string(), "main".to_string());
        metadata.insert("scope_commit".to_string(), "abc123".to_string());
        metadata.insert(
            "scope_remote_url".to_string(),
            "https://github.com/org/repo".to_string(),
        );
        metadata.insert("session_id".to_string(), "session-001".to_string());
        metadata.insert("session_name".to_string(), "nightly-run".to_string());
        metadata.insert("session_tags".to_string(), "ci,nightly".to_string());

        let scope = pkg_metadata::extract_scope_metadata(&metadata);
        assert_eq!(scope.scope_repo, Some("my-repo".to_string()));
        assert_eq!(scope.repo_slug, Some("my_repo".to_string()));
        assert_eq!(scope.scope_branch, Some("main".to_string()));
        assert_eq!(scope.scope_commit, Some("abc123".to_string()));
        assert_eq!(
            scope.scope_remote_url,
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(scope.session_id, Some("session-001".to_string()));
        assert_eq!(scope.session_name, Some("nightly-run".to_string()));
        assert_eq!(
            scope.session_tags,
            Some(vec!["ci".to_string(), "nightly".to_string()])
        );
    }

    #[test]
    fn extract_scope_metadata_falls_back_to_repo_keys() {
        let mut metadata = HashMap::new();
        metadata.insert("repo_name".to_string(), "fallback-repo".to_string());
        metadata.insert("repo_branch".to_string(), "develop".to_string());
        metadata.insert("repo_commit".to_string(), "def456".to_string());
        metadata.insert("repo_path".to_string(), "/home/user/project".to_string());
        metadata.insert(
            "repo_remote".to_string(),
            "git@github.com:org/repo.git".to_string(),
        );

        let scope = pkg_metadata::extract_scope_metadata(&metadata);
        assert_eq!(scope.scope_repo, Some("fallback-repo".to_string()));
        assert_eq!(scope.scope_branch, Some("develop".to_string()));
        assert_eq!(scope.scope_commit, Some("def456".to_string()));
        assert_eq!(
            scope.scope_scan_root,
            Some("/home/user/project".to_string())
        );
        assert_eq!(
            scope.scope_remote_url,
            Some("git@github.com:org/repo.git".to_string())
        );
    }

    #[test]
    fn extract_scope_metadata_falls_back_to_commit_sha() {
        let mut metadata = HashMap::new();
        metadata.insert("commit_sha".to_string(), "abc123def4567890".to_string());

        let scope = pkg_metadata::extract_scope_metadata(&metadata);
        assert_eq!(scope.scope_commit, Some("abc123def4567890".to_string()));
    }

    #[test]
    fn branch_metadata_from_metadata_commit_sha_fallback() {
        let mut metadata = HashMap::new();
        metadata.insert("commit_sha".to_string(), "abc123def4567890".to_string());

        let meta = BranchMetadata::from_metadata(&metadata);
        assert_eq!(meta.commit, Some("abc123def4567890".to_string()));
        assert_eq!(meta.commit_full, Some("abc123def4567890".to_string()));
    }

    #[test]
    fn extract_scope_metadata_prefers_canonical_over_fallback() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_repo".to_string(), "canonical-repo".to_string());
        metadata.insert("repo_name".to_string(), "fallback-repo".to_string());

        let scope = pkg_metadata::extract_scope_metadata(&metadata);
        assert_eq!(scope.scope_repo, Some("canonical-repo".to_string()));
    }

    #[test]
    fn scan_root_metadata_serialization_roundtrip() {
        let root = ScanRootMetadata {
            path: "/project/src".to_string(),
            label: Some("main".to_string()),
            file_count: Some(100),
            byte_count: Some(50000),
            content_hash: Some("blake3hash".to_string()),
            scanned_at: Some("2024-01-15T10:30:00Z".to_string()),
        };

        let json = serde_json::to_string(&root).unwrap();
        let parsed: ScanRootMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(root, parsed);
    }

    #[test]
    fn branch_metadata_new_creates_basic_instance() {
        let meta = BranchMetadata::new("main", "abc123");
        assert_eq!(meta.branch, Some("main".to_string()));
        assert_eq!(meta.commit, Some("abc123".to_string()));
        assert!(meta.is_present());
    }

    #[test]
    fn branch_metadata_builder_pattern() {
        let meta = BranchMetadata::new("feature/xyz", "def456")
            .with_full_commit("def456789abcdef0123456789abcdef012345678")
            .with_repo_name("my-repo")
            .with_repo_slug("my_repo")
            .with_remote_url("https://github.com/org/repo")
            .with_dirty(true)
            .with_captured_at("2024-01-15T10:30:00Z");

        assert_eq!(meta.branch, Some("feature/xyz".to_string()));
        assert_eq!(meta.commit, Some("def456".to_string()));
        assert_eq!(
            meta.commit_full,
            Some("def456789abcdef0123456789abcdef012345678".to_string())
        );
        assert_eq!(meta.repo_name, Some("my-repo".to_string()));
        assert_eq!(meta.repo_slug, Some("my_repo".to_string()));
        assert_eq!(
            meta.remote_url,
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(meta.dirty, Some(true));
        assert_eq!(meta.captured_at, Some("2024-01-15T10:30:00Z".to_string()));
    }

    #[test]
    fn branch_metadata_to_metadata_entries() {
        let meta = BranchMetadata::new("main", "abc123")
            .with_repo_name("test-repo")
            .with_repo_slug("test_repo")
            .with_remote_url("git@github.com:org/repo.git");

        let entries = meta.to_metadata_entries();
        assert_eq!(entries.get("scope_branch"), Some(&"main".to_string()));
        assert_eq!(entries.get("scope_commit"), Some(&"abc123".to_string()));
        assert_eq!(entries.get("scope_repo"), Some(&"test-repo".to_string()));
        assert_eq!(entries.get("repo_slug"), Some(&"test_repo".to_string()));
        assert_eq!(
            entries.get("scope_remote_url"),
            Some(&"git@github.com:org/repo.git".to_string())
        );
    }

    #[test]
    fn branch_metadata_from_metadata_canonical_keys() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_branch".to_string(), "develop".to_string());
        metadata.insert("scope_commit".to_string(), "xyz789".to_string());
        metadata.insert("scope_repo".to_string(), "canonical-repo".to_string());
        metadata.insert("repo_slug".to_string(), "canonical_repo".to_string());
        metadata.insert(
            "scope_remote_url".to_string(),
            "https://github.com/org/repo".to_string(),
        );
        metadata.insert("scope_dirty".to_string(), "true".to_string());

        let meta = BranchMetadata::from_metadata(&metadata);
        assert_eq!(meta.branch, Some("develop".to_string()));
        assert_eq!(meta.commit, Some("xyz789".to_string()));
        assert_eq!(meta.repo_name, Some("canonical-repo".to_string()));
        assert_eq!(meta.repo_slug, Some("canonical_repo".to_string()));
        assert_eq!(
            meta.remote_url,
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(meta.dirty, Some(true));
    }

    #[test]
    fn branch_metadata_from_metadata_fallback_keys() {
        let mut metadata = HashMap::new();
        metadata.insert("repo_branch".to_string(), "fallback-branch".to_string());
        metadata.insert("repo_commit".to_string(), "fallback-commit".to_string());
        metadata.insert("repo_name".to_string(), "fallback-repo".to_string());
        metadata.insert("repo_remote".to_string(), "git@fallback.git".to_string());

        let meta = BranchMetadata::from_metadata(&metadata);
        assert_eq!(meta.branch, Some("fallback-branch".to_string()));
        assert_eq!(meta.commit, Some("fallback-commit".to_string()));
        assert_eq!(meta.repo_name, Some("fallback-repo".to_string()));
        assert_eq!(meta.remote_url, Some("git@fallback.git".to_string()));
    }

    #[test]
    fn branch_metadata_prefers_canonical_over_fallback() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_branch".to_string(), "canonical".to_string());
        metadata.insert("repo_branch".to_string(), "fallback".to_string());
        metadata.insert("branch".to_string(), "base".to_string());

        let meta = BranchMetadata::from_metadata(&metadata);
        assert_eq!(meta.branch, Some("canonical".to_string()));
    }

    #[test]
    fn branch_metadata_is_present_checks_branch_or_commit() {
        let empty = BranchMetadata::default();
        assert!(!empty.is_present());

        let with_branch = BranchMetadata {
            branch: Some("main".to_string()),
            ..Default::default()
        };
        assert!(with_branch.is_present());

        let with_commit = BranchMetadata {
            commit: Some("abc123".to_string()),
            ..Default::default()
        };
        assert!(with_commit.is_present());
    }

    #[test]
    fn branch_metadata_serialization_roundtrip() {
        let meta = BranchMetadata::new("main", "abc123")
            .with_full_commit("abc123456789")
            .with_repo_name("test-repo")
            .with_repo_slug("test_repo")
            .with_remote_url("https://github.com/org/repo")
            .with_dirty(false)
            .with_captured_at("2024-01-15T10:30:00Z");

        let json = serde_json::to_string(&meta).unwrap();
        let parsed: BranchMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(meta.branch, parsed.branch);
        assert_eq!(meta.commit, parsed.commit);
        assert_eq!(meta.commit_full, parsed.commit_full);
        assert_eq!(meta.repo_name, parsed.repo_name);
        assert_eq!(meta.repo_slug, parsed.repo_slug);
        assert_eq!(meta.remote_url, parsed.remote_url);
        assert_eq!(meta.dirty, parsed.dirty);
        assert_eq!(meta.captured_at, parsed.captured_at);
    }

    #[test]
    fn branch_metadata_entries_exclude_none_values() {
        let meta = BranchMetadata {
            branch: Some("main".to_string()),
            commit: None,
            ..Default::default()
        };

        let entries = meta.to_metadata_entries();
        assert!(entries.contains_key("scope_branch"));
        assert!(!entries.contains_key("scope_commit"));
        assert!(!entries.contains_key("scope_repo"));
    }
    #[test]
    fn extract_manifest_fields_includes_scope_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_repo".to_string(), "test-repo".to_string());
        metadata.insert("scope_branch".to_string(), "feature-branch".to_string());
        metadata.insert("scope_commit".to_string(), "abc123def456".to_string());
        metadata.insert(
            "scope_scan_root".to_string(),
            "/path/to/project".to_string(),
        );
        metadata.insert(
            "scope_remote_url".to_string(),
            "https://github.com/org/test-repo".to_string(),
        );
        metadata.insert("session_id".to_string(), "session-xyz".to_string());
        metadata.insert("session_name".to_string(), "release-run".to_string());
        metadata.insert("session_tags".to_string(), "release,prod".to_string());
        metadata.insert("category".to_string(), "code".to_string());
        metadata.insert("tier".to_string(), "warm".to_string());

        let fields = pkg_metadata::extract_manifest_fields(&metadata);

        // Verify scope metadata is extracted correctly
        assert_eq!(fields.scope_meta.scope_repo, Some("test-repo".to_string()));
        assert_eq!(
            fields.scope_meta.scope_branch,
            Some("feature-branch".to_string())
        );
        assert_eq!(
            fields.scope_meta.scope_commit,
            Some("abc123def456".to_string())
        );
        assert_eq!(
            fields.scope_meta.scope_scan_root,
            Some("/path/to/project".to_string())
        );
        assert_eq!(
            fields.scope_meta.scope_remote_url,
            Some("https://github.com/org/test-repo".to_string())
        );
        assert_eq!(
            fields.scope_meta.session_id,
            Some("session-xyz".to_string())
        );
        assert_eq!(
            fields.scope_meta.session_name,
            Some("release-run".to_string())
        );
        assert_eq!(
            fields.scope_meta.session_tags,
            Some(vec!["prod".to_string(), "release".to_string()])
        );

        // Verify other fields are also extracted
        assert_eq!(fields.category, "code");
        assert_eq!(fields.tier, "warm");
    }

    #[test]
    fn extract_manifest_fields_with_scan_roots_json() {
        let mut metadata = HashMap::new();
        let scan_roots_json = r#"[
            {"path": "src", "label": "main", "file_count": 150},
            {"path": "tests", "label": "tests", "file_count": 50}
        ]"#;
        metadata.insert("scan_roots".to_string(), scan_roots_json.to_string());
        metadata.insert("scope_repo".to_string(), "multi-root-repo".to_string());

        let fields = pkg_metadata::extract_manifest_fields(&metadata);

        assert_eq!(fields.scope_meta.scan_roots.len(), 2);
        assert_eq!(fields.scope_meta.scan_roots[0].path, "src");
        assert_eq!(
            fields.scope_meta.scan_roots[0].label,
            Some("main".to_string())
        );
        assert_eq!(fields.scope_meta.scan_roots[0].file_count, Some(150));
        assert_eq!(fields.scope_meta.scan_roots[1].path, "tests");
        assert_eq!(
            fields.scope_meta.scope_repo,
            Some("multi-root-repo".to_string())
        );
    }

    #[tokio::test]
    async fn test_package_aos_stores_in_content_addressed_path() {
        use crate::training::quantizer::QuantizedLoRAWeights;

        let temp_dir = tempfile::Builder::new()
            .prefix("aos-versioning-test-")
            .tempdir()
            .expect("failed to create temporary directory for versioning test");

        let packager = AdapterPackager::new(temp_dir.path());

        // Create minimal quantized weights for testing (Q15 format)
        let weights = QuantizedLoRAWeights {
            lora_a_q15: vec![vec![100i16; 32]; 4], // rank=4, hidden=32
            lora_b_q15: vec![vec![100i16; 4]; 32], // hidden=32, rank=4
            scale_a: vec![1.0; 4],
            scale_b: vec![1.0; 32],
            modules: HashMap::new(),
        };

        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 32,
            ..Default::default()
        };

        // Add required metadata for lineage tracking
        let mut metadata = HashMap::new();
        metadata.insert("lineage_mode".to_string(), "legacy_unpinned".to_string());

        let result = packager
            .package_aos_with_metadata(
                "test-tenant",
                "versioning-test",
                &weights,
                &config,
                "test-base-model",
                metadata,
            )
            .await;

        // Should succeed
        let packaged = result.expect("packaging should succeed");

        // Verify the weights_path is in objects directory (content-addressed)
        let path_str = packaged.weights_path.to_string_lossy();
        assert!(
            path_str.contains("objects"),
            "Archive should be in objects/ directory, got: {}",
            path_str
        );

        // Verify the file exists
        assert!(
            packaged.weights_path.exists(),
            "Archive file should exist at {}",
            packaged.weights_path.display()
        );

        // Verify the hash is in the path
        assert!(
            path_str.contains(&packaged.hash_b3[0..2]),
            "Path should contain hash prefix"
        );

        // Verify the draft ref was created
        let layout = AdapterLayout::new(temp_dir.path());
        let ref_store = FsRefStore::new(layout);
        let adapter_name = AdapterName::subject("versioning-test");

        let draft_hash = ref_store
            .get_ref(&adapter_name, "test-tenant", "draft")
            .await
            .expect("getting ref should succeed");

        assert_eq!(
            draft_hash,
            Some(packaged.hash_b3.clone()),
            "draft ref should point to the archive hash"
        );

        // Verify the archive can be resolved via the draft ref
        let resolved_path = ref_store
            .resolve_ref(&adapter_name, "test-tenant", "draft")
            .await
            .expect("resolving ref should succeed");

        assert_eq!(
            resolved_path,
            Some(packaged.weights_path.clone()),
            "resolved path should match the archive path"
        );
    }

    #[tokio::test]
    async fn test_package_aos_tracks_parent_version() {
        use crate::training::quantizer::QuantizedLoRAWeights;

        let temp_dir = tempfile::Builder::new()
            .prefix("aos-parent-test-")
            .tempdir()
            .expect("failed to create temporary directory");

        let packager = AdapterPackager::new(temp_dir.path());

        let weights = QuantizedLoRAWeights {
            lora_a_q15: vec![vec![100i16; 32]; 4],
            lora_b_q15: vec![vec![100i16; 4]; 32],
            scale_a: vec![1.0; 4],
            scale_b: vec![1.0; 32],
            modules: HashMap::new(),
        };

        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 32,
            ..Default::default()
        };

        // Add required metadata for lineage tracking
        let mut metadata = HashMap::new();
        metadata.insert("lineage_mode".to_string(), "legacy_unpinned".to_string());

        // First version
        let first = packager
            .package_aos_with_metadata(
                "test-tenant",
                "parent-test",
                &weights,
                &config,
                "test-model",
                metadata.clone(),
            )
            .await
            .expect("first packaging should succeed");

        // Promote to current (simulate deployment)
        let layout = AdapterLayout::new(temp_dir.path());
        let ref_store = FsRefStore::new(layout.clone());
        let adapter_name = AdapterName::subject("parent-test");

        ref_store
            .update_ref(&adapter_name, "test-tenant", "current", &first.hash_b3)
            .await
            .expect("promoting to current should succeed");

        // Create slightly different weights for second version
        let weights2 = QuantizedLoRAWeights {
            lora_a_q15: vec![vec![200i16; 32]; 4], // different values
            lora_b_q15: vec![vec![200i16; 4]; 32],
            scale_a: vec![1.0; 4],
            scale_b: vec![1.0; 32],
            modules: HashMap::new(),
        };

        // Second version
        let second = packager
            .package_aos_with_metadata(
                "test-tenant",
                "parent-test",
                &weights2,
                &config,
                "test-model",
                metadata,
            )
            .await
            .expect("second packaging should succeed");

        // Verify hashes are different
        assert_ne!(
            first.hash_b3, second.hash_b3,
            "Different weights should produce different hashes"
        );

        // Verify both files exist
        assert!(first.weights_path.exists(), "First archive should exist");
        assert!(second.weights_path.exists(), "Second archive should exist");

        // Verify draft ref points to second version
        let draft = ref_store
            .get_ref(&adapter_name, "test-tenant", "draft")
            .await
            .expect("getting draft ref should succeed");
        assert_eq!(draft, Some(second.hash_b3.clone()));

        // Verify current still points to first version
        let current = ref_store
            .get_ref(&adapter_name, "test-tenant", "current")
            .await
            .expect("getting current ref should succeed");
        assert_eq!(current, Some(first.hash_b3.clone()));
    }
}
