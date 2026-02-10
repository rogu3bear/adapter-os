//! Single-file adapter format definition

use super::training::{TrainingConfig, TrainingExample};
use adapteros_core::{AosError, B3Hash, IntegrityMode, Result};
use adapteros_crypto::{Keypair, PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Current .aos format version
///
/// Version history:
/// - v1: Initial format with manifest, weights, training_data, config, lineage, signature
/// - v2: Added separate positive/negative weight groups for better LoRA training
pub const AOS_FORMAT_VERSION: u8 = 2;

/// Current manifest schema version (semantic versioning)
///
/// This is distinct from AOS_FORMAT_VERSION which tracks binary format.
/// Schema version tracks the JSON manifest structure for backward compatibility.
pub const MANIFEST_SCHEMA_VERSION: &str = "1.0.0";

/// Minimum supported manifest schema version for import
pub const MIN_SUPPORTED_SCHEMA_VERSION: &str = "1.0.0";

/// Single-file adapter container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleFileAdapter {
    pub manifest: AdapterManifest,
    pub weights: AdapterWeights,
    pub training_data: Vec<TrainingExample>,
    pub config: TrainingConfig,
    pub lineage: LineageInfo,
    pub signature: Option<AosSignature>,
}

/// LoRA weight groups for positive/negative training
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdapterWeights {
    /// Positive reinforcement weights (learned from +1.0 examples)
    pub positive: WeightGroup,
    /// Negative reinforcement weights (learned from -1.0 examples)
    pub negative: WeightGroup,
    /// Combined weights for inference (positive - negative)
    pub combined: Option<WeightGroup>,
}

/// Individual weight group
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeightGroup {
    /// LoRA A matrix weights (rank × hidden_dim)
    pub lora_a: Vec<Vec<f32>>,
    /// LoRA B matrix weights (hidden_dim × rank)
    pub lora_b: Vec<Vec<f32>>,
    /// Weight metadata
    pub metadata: WeightMetadata,
}

/// Metadata for weight groups
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeightMetadata {
    /// Number of training examples used
    pub example_count: usize,
    /// Average loss during training
    pub avg_loss: f32,
    /// Training duration in milliseconds
    pub training_time_ms: u64,
    /// Weight group type
    pub group_type: WeightGroupType,
    /// Creation timestamp
    pub created_at: String,
}

/// Type of weight group
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WeightGroupType {
    Positive,
    Negative,
    Combined,
}

/// Adapter manifest (enhanced version of existing manifest)
///
/// PRD-ART-01: This manifest is the source of truth for adapter identity and portability.
/// The `schema_version` field enables backward-compatible evolution of this structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    /// .aos format version (for binary compatibility checks)
    #[serde(default = "default_format_version")]
    pub format_version: u8,
    /// Manifest schema version (semantic versioning, e.g., "1.0.0")
    /// Used for backward-compatible JSON structure evolution
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    #[serde(default)]
    pub adapter_id: String,
    /// Content hash: BLAKE3(manifest_json + weights_bytes)
    /// This is the canonical identity for the adapter across systems
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_rank")]
    pub rank: u32,
    #[serde(default = "default_alpha")]
    pub alpha: f32,
    /// Base model name (e.g., "qwen2.5-7b")
    #[serde(default)]
    pub base_model: String,
    /// Base model ID (FK reference to models.id for validation)
    #[serde(default)]
    pub base_model_id: Option<String>,
    /// Backend family: "metal", "coreml", "mlx", or "auto"
    #[serde(default)]
    pub backend_family: Option<String>,
    /// Quantization format: "q15", "f16", "f32", etc.
    #[serde(default)]
    pub quantization: Option<String>,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub tier: String,
    #[serde(default = "default_recommended_for_moe")]
    pub recommended_for_moe: bool,
    #[serde(default)]
    pub target_modules: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub weights_hash: String,
    #[serde(default)]
    pub training_data_hash: String,
    /// Optional per-layer BLAKE3 hashes keyed by canonical logical layer path
    /// (e.g., "transformer.layer_12.attn.q_proj.lora_A"). Backward-compatible:
    /// absent for older manifests. Accepts either string hashes or {hash, tensor_name} objects.
    #[serde(default)]
    pub per_layer_hashes: Option<HashMap<String, serde_json::Value>>,
    /// Compression method used for weights
    #[serde(default)]
    pub compression_method: String,
    /// Weight group configuration
    #[serde(default)]
    pub weight_groups: WeightGroupConfig,
    /// Full training provenance (embedded for portability)
    #[serde(default)]
    pub provenance: Option<ProvenanceData>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Training configuration (from packager)
    #[serde(default)]
    pub training_config: Option<serde_json::Value>,
    /// CoreML placement configuration
    #[serde(default)]
    pub coreml_placement: Option<serde_json::Value>,
    /// Synthetic mode flag
    #[serde(default)]
    pub synthetic_mode: Option<bool>,
    /// Determinism level
    #[serde(default)]
    pub determinism: Option<String>,
    /// Gate Q15 denominator
    #[serde(default)]
    pub gate_q15_denominator: Option<u32>,
    /// Kernel version
    #[serde(default)]
    pub kernel_version: Option<String>,
    /// Training backend
    #[serde(default)]
    pub training_backend: Option<String>,
    /// Base model hash (BLAKE3)
    #[serde(default)]
    pub base_model_hash: Option<String>,
    /// Training configuration hash (BLAKE3) for reproducibility verification
    #[serde(default)]
    pub training_config_hash: Option<String>,
    /// Tokenizer hash (BLAKE3) for input processing verification
    #[serde(default)]
    pub tokenizer_hash: Option<String>,
    /// Primary dataset ID used for training
    #[serde(default)]
    pub dataset_id: Option<String>,
    /// Dataset content hash (BLAKE3)
    #[serde(default)]
    pub dataset_hash: Option<String>,
    /// Integrity hash covering all critical fields for tamper detection.
    /// BLAKE3(base_model_hash || training_config_hash || tokenizer_hash || dataset_hash || weights_hash)
    #[serde(default)]
    pub integrity_hash: Option<String>,
}

impl AdapterManifest {
    /// Compute the integrity hash from critical manifest fields.
    /// This covers: base_model_hash, training_config_hash, tokenizer_hash, dataset_hash, weights_hash.
    pub fn compute_integrity_hash(&self) -> String {
        // Build input buffer in deterministic order
        let mut input = Vec::new();

        if let Some(ref h) = self.base_model_hash {
            input.extend_from_slice(b"base_model_hash:");
            input.extend_from_slice(h.as_bytes());
            input.push(b'|');
        }
        if let Some(ref h) = self.training_config_hash {
            input.extend_from_slice(b"training_config_hash:");
            input.extend_from_slice(h.as_bytes());
            input.push(b'|');
        }
        if let Some(ref h) = self.tokenizer_hash {
            input.extend_from_slice(b"tokenizer_hash:");
            input.extend_from_slice(h.as_bytes());
            input.push(b'|');
        }
        if let Some(ref h) = self.dataset_hash {
            input.extend_from_slice(b"dataset_hash:");
            input.extend_from_slice(h.as_bytes());
            input.push(b'|');
        }
        input.extend_from_slice(b"weights_hash:");
        input.extend_from_slice(self.weights_hash.as_bytes());

        B3Hash::hash(&input).to_hex()
    }

    /// Verify the integrity hash matches the computed value.
    ///
    /// Behavior depends on `mode`:
    /// - `Permissive`: returns `Ok(())` when `integrity_hash` is `None` (backward
    ///   compatibility with legacy `.aos` files). Present hashes are still verified.
    /// - `Strict`: returns an error when `integrity_hash` is `None`.
    pub fn verify_integrity_with_mode(&self, mode: IntegrityMode) -> Result<()> {
        let Some(ref stored_hash) = self.integrity_hash else {
            if mode.is_strict() {
                return Err(AosError::IntegrityViolation(
                    "Missing integrity_hash in adapter manifest. \
                     Strict integrity mode requires all adapters to carry an integrity hash. \
                     Action: Re-package the adapter with `seal_integrity()` before deployment."
                        .to_string(),
                ));
            }
            // Backward compatibility: older .aos files may not have integrity_hash
            tracing::debug!(
                schema_version = %self.schema_version,
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

        tracing::debug!(
            integrity_hash = %stored_hash,
            "Adapter integrity verification passed"
        );
        Ok(())
    }

    /// Verify the integrity hash in permissive mode (backward compatible).
    ///
    /// Equivalent to `verify_integrity_with_mode(IntegrityMode::Permissive)`.
    pub fn verify_integrity(&self) -> Result<()> {
        self.verify_integrity_with_mode(IntegrityMode::Permissive)
    }

    /// Seal the manifest by computing and storing the integrity hash.
    /// Call this after all fields are set, before serializing to .aos.
    pub fn seal_integrity(&mut self) {
        self.integrity_hash = Some(self.compute_integrity_hash());
    }
}

fn default_format_version() -> u8 {
    AOS_FORMAT_VERSION
}

fn default_schema_version() -> String {
    MANIFEST_SCHEMA_VERSION.to_string()
}

fn default_version() -> String {
    "1.0.0".to_string()
}

fn default_rank() -> u32 {
    16
}

fn default_alpha() -> f32 {
    32.0
}

fn default_recommended_for_moe() -> bool {
    true
}

fn ensure_scope_metadata(metadata: &mut HashMap<String, String>, scope: &str) -> String {
    let domain = metadata
        .entry("domain".to_string())
        .or_insert_with(|| "unspecified".to_string())
        .clone();
    let group = metadata
        .entry("group".to_string())
        .or_insert_with(|| "unspecified".to_string())
        .clone();
    let operation = metadata
        .entry("operation".to_string())
        .or_insert_with(|| "unspecified".to_string())
        .clone();
    let scope_path = format!("{}/{}/{}/{}", domain, group, scope, operation);
    metadata
        .entry("scope_path".to_string())
        .or_insert_with(|| scope_path.clone());
    scope_path
}

/// Full training provenance data for portable adapters
///
/// PRD-ART-01: Embedded in .aos files for complete audit trail across systems
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvenanceData {
    /// Training job ID that produced this adapter
    pub training_job_id: Option<String>,
    /// Dataset ID used for training
    pub dataset_id: Option<String>,
    /// BLAKE3 hash of the dataset
    pub dataset_hash: Option<String>,
    /// Training configuration (hyperparameters, etc.)
    pub training_config: Option<serde_json::Value>,
    /// Documents used in training with their content hashes
    pub documents: Vec<ProvenanceDocument>,
    /// Timestamp when provenance was captured
    pub export_timestamp: String,
    /// BLAKE3 hash of the provenance data for integrity
    pub export_hash: String,
}

/// Document reference with content hash for provenance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceDocument {
    /// Document ID
    pub id: String,
    /// Document name
    pub name: String,
    /// BLAKE3 hash of document content
    pub content_hash: String,
}

// Re-export types from crate::types for backward compatibility
pub use crate::types::{CombinationStrategy, WeightGroupConfig};

/// Evolution lineage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageInfo {
    pub adapter_id: String,
    pub version: String,
    pub parent_version: Option<String>,
    pub parent_hash: Option<String>,
    pub mutations: Vec<Mutation>,
    pub quality_delta: f32,
    pub created_at: String,
}

/// Training mutation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutation {
    pub mutation_type: String,
    pub examples_added: usize,
    pub examples_removed: usize,
    pub quality_impact: f32,
    pub applied_at: String,
}

/// Cryptographic signature metadata for .aos files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AosSignature {
    /// Ed25519 signature over manifest hash
    pub signature: Signature,
    /// Public key for verification
    pub public_key: PublicKey,
    /// Unix timestamp (microseconds)
    pub timestamp: u64,
    /// Key ID (deterministic: blake3(pubkey))
    pub key_id: String,
}

/// Options for creating a single-file adapter with extended metadata
///
/// PRD-ART-01: These options enable portable adapters with full provenance
#[derive(Debug, Clone)]
pub struct CreateOptions {
    /// Base model name (e.g., "qwen2.5-7b")
    pub base_model: Option<String>,
    /// Base model ID (FK reference to models.id)
    pub base_model_id: Option<String>,
    /// Backend family: "metal", "coreml", "mlx", or "auto"
    pub backend_family: Option<String>,
    /// Quantization format: "q15", "f16", "f32"
    pub quantization: Option<String>,
    /// Full training provenance for portability
    pub provenance: Option<ProvenanceData>,
    /// Whether the adapter is recommended for MoE base models
    pub recommended_for_moe: bool,
}

impl Default for CreateOptions {
    fn default() -> Self {
        Self {
            base_model: None,
            base_model_id: None,
            backend_family: None,
            quantization: None,
            provenance: None,
            recommended_for_moe: true,
        }
    }
}

impl CreateOptions {
    /// Create options with base model information
    pub fn with_base_model(mut self, name: &str, id: Option<String>) -> Self {
        self.base_model = Some(name.to_string());
        self.base_model_id = id;
        self
    }

    /// Mark whether the adapter should be recommended for MoE base models
    pub fn with_recommended_for_moe(mut self, recommended: bool) -> Self {
        self.recommended_for_moe = recommended;
        self
    }

    /// Set backend family
    pub fn with_backend(mut self, backend: &str) -> Self {
        self.backend_family = Some(backend.to_string());
        self
    }

    /// Set quantization format
    pub fn with_quantization(mut self, quant: &str) -> Self {
        self.quantization = Some(quant.to_string());
        self
    }

    /// Set provenance data
    pub fn with_provenance(mut self, provenance: ProvenanceData) -> Self {
        self.provenance = Some(provenance);
        self
    }
}

impl SingleFileAdapter {
    /// Create a new single-file adapter
    pub fn create(
        adapter_id: String,
        weights: AdapterWeights,
        training_data: Vec<TrainingExample>,
        config: TrainingConfig,
        lineage: LineageInfo,
    ) -> Result<Self> {
        Self::create_with_options(
            adapter_id,
            weights,
            training_data,
            config,
            lineage,
            CreateOptions::default(),
        )
    }

    /// Create a new single-file adapter with extended options
    pub fn create_with_options(
        adapter_id: String,
        weights: AdapterWeights,
        training_data: Vec<TrainingExample>,
        config: TrainingConfig,
        lineage: LineageInfo,
        options: CreateOptions,
    ) -> Result<Self> {
        let weights_hash = B3Hash::hash(&serde_json::to_vec(&weights)?).to_hex();
        let training_data_hash = B3Hash::hash(&serde_json::to_vec(&training_data)?).to_hex();

        let mut metadata = HashMap::new();
        let scope_value = "global".to_string();
        ensure_scope_metadata(&mut metadata, &scope_value);

        let manifest = AdapterManifest {
            format_version: AOS_FORMAT_VERSION,
            schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
            adapter_id: adapter_id.clone(),
            content_hash: None, // Computed after manifest is finalized
            version: lineage.version.clone(),
            rank: config.rank as u32,
            alpha: config.alpha,
            base_model: options
                .base_model
                .unwrap_or_else(|| "qwen2.5-7b".to_string()),
            base_model_id: options.base_model_id,
            backend_family: options.backend_family,
            quantization: options.quantization,
            category: "code".to_string(),
            scope: scope_value,
            tier: "persistent".to_string(),
            recommended_for_moe: options.recommended_for_moe,
            target_modules: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
            ],
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: weights_hash.clone(),
            training_data_hash,
            per_layer_hashes: None,
            compression_method: "deflate-fast".to_string(),
            weight_groups: WeightGroupConfig::default(),
            provenance: options.provenance,
            metadata,
            // Optional fields from packager manifest
            training_config: None,
            coreml_placement: None,
            synthetic_mode: None,
            determinism: None,
            gate_q15_denominator: None,
            kernel_version: None,
            training_backend: None,
            base_model_hash: None,
            training_config_hash: None,
            tokenizer_hash: None,
            dataset_id: None,
            dataset_hash: None,
            integrity_hash: None,
        };

        let mut adapter = Self {
            manifest,
            weights,
            training_data,
            config,
            lineage,
            signature: None,
        };

        // Compute content hash for identity
        adapter.compute_and_set_content_hash()?;

        Ok(adapter)
    }

    /// Compute and set the content hash (BLAKE3 of manifest + weights)
    ///
    /// This hash serves as the canonical identity for the adapter across systems.
    pub fn compute_and_set_content_hash(&mut self) -> Result<()> {
        let content_hash = self.compute_content_hash()?;
        self.manifest.content_hash = Some(content_hash);
        Ok(())
    }

    /// Compute content hash without setting it
    pub fn compute_content_hash(&self) -> Result<String> {
        // Serialize manifest without content_hash to avoid circular dependency
        let mut manifest_for_hash = self.manifest.clone();
        manifest_for_hash.content_hash = None;
        let manifest_bytes = serde_json::to_vec(&manifest_for_hash)?;
        let weights_bytes = serde_json::to_vec(&self.weights)?;

        let hash = B3Hash::hash_multi(&[&manifest_bytes, &weights_bytes]);
        Ok(hash.to_hex())
    }

    /// Sign the adapter with the provided keypair
    pub fn sign(&mut self, keypair: &Keypair) -> Result<()> {
        let manifest_hash = B3Hash::hash(&serde_json::to_vec(&self.manifest)?);
        let signature = keypair.sign(&manifest_hash.to_bytes());
        let key_id = B3Hash::hash(&keypair.public_key().to_bytes()).to_hex();

        self.signature = Some(AosSignature {
            signature,
            public_key: keypair.public_key(),
            timestamp: chrono::Utc::now().timestamp_micros() as u64,
            key_id,
        });

        Ok(())
    }

    /// Check whether the adapter carries a valid signature
    pub fn is_signed(&self) -> bool {
        self.signature.is_some()
    }

    /// Convenience accessor for signature details (key id, timestamp)
    pub fn signature_info(&self) -> Option<(String, u64)> {
        self.signature
            .as_ref()
            .map(|sig| (sig.key_id.clone(), sig.timestamp))
    }

    /// Verify adapter integrity (hashes and optional signature)
    pub fn verify(&self) -> Result<bool> {
        verify_format_version(self.manifest.format_version)?;

        let weights_hash = B3Hash::hash(&serde_json::to_vec(&self.weights)?).to_hex();
        let training_hash = B3Hash::hash(&serde_json::to_vec(&self.training_data)?).to_hex();

        if weights_hash != self.manifest.weights_hash
            || training_hash != self.manifest.training_data_hash
        {
            return Ok(false);
        }

        if self.is_signed() && !self.verify_signature()? {
            return Ok(false);
        }

        Ok(true)
    }

    /// Verify the adapter signature
    pub fn verify_signature(&self) -> Result<bool> {
        match &self.signature {
            Some(sig) => {
                let manifest_hash = B3Hash::hash(&serde_json::to_vec(&self.manifest)?);
                sig.public_key
                    .verify(&manifest_hash.to_bytes(), &sig.signature)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Get combined weights for inference
    pub fn get_inference_weights(&self) -> Result<WeightGroup> {
        match &self.weights.combined {
            Some(combined) => Ok(combined.clone()),
            None => {
                // Compute combined weights on-the-fly
                let combined = self.compute_combined_weights()?;
                Ok(combined)
            }
        }
    }

    /// Compute combined weights from positive and negative
    fn compute_combined_weights(&self) -> Result<WeightGroup> {
        super::weights::combine_weight_groups(
            &self.weights.positive,
            &self.weights.negative,
            &self.manifest.weight_groups.combination_strategy,
        )
    }
}

/// Compatibility report for .aos format versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub current_version: u8,
    pub file_version: u8,
    pub is_compatible: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Check compatibility between format versions
pub fn get_compatibility_report(file_version: u8) -> CompatibilityReport {
    let current_version = AOS_FORMAT_VERSION;
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if file_version > current_version {
        errors.push(format!(
            "File format v{} is newer than supported v{}",
            file_version, current_version
        ));
    } else if file_version < current_version && file_version < 2 {
        warnings.push("File uses old format without separate weight groups".to_string());
    }

    CompatibilityReport {
        current_version,
        file_version,
        is_compatible: errors.is_empty(),
        warnings,
        errors,
    }
}

/// Verify format version compatibility
pub fn verify_format_version(file_version: u8) -> Result<()> {
    let report = get_compatibility_report(file_version);
    if !report.is_compatible {
        return Err(AosError::Training(format!(
            "Format compatibility error: {}",
            report.errors.join(", ")
        )));
    }
    Ok(())
}
