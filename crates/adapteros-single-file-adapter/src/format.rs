//! Single-file adapter format definition

use super::training::{TrainingConfig, TrainingExample};
use adapteros_core::{AosError, B3Hash, Result};
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

/// Compression level for .aos packaging
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompressionLevel {
    /// No compression (fastest)
    Store,
    /// Fast compression (default for most files)
    #[default]
    Fast,
    /// Best compression (slowest, smallest)
    Best,
}

impl CompressionLevel {
    /// Convert to zip compression method and level
    pub fn to_zip_options(self) -> (zip::CompressionMethod, Option<i64>) {
        use zip::CompressionMethod;
        match self {
            Self::Store => (CompressionMethod::Stored, None),
            Self::Fast => (CompressionMethod::Deflated, Some(3)),
            Self::Best => (CompressionMethod::Deflated, Some(9)),
        }
    }

    /// Get compression method name for manifest
    pub fn method_name(self) -> &'static str {
        match self {
            Self::Store => "stored",
            Self::Fast => "deflate-fast",
            Self::Best => "deflate-best",
        }
    }
}

/// Adapter manifest (enhanced version of existing manifest)
///
/// PRD-ART-01: This manifest is the source of truth for adapter identity and portability.
/// The `schema_version` field enables backward-compatible evolution of this structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    /// .aos format version (for binary compatibility checks)
    pub format_version: u8,
    /// Manifest schema version (semantic versioning, e.g., "1.0.0")
    /// Used for backward-compatible JSON structure evolution
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub adapter_id: String,
    /// Content hash: BLAKE3(manifest_json + weights_bytes)
    /// This is the canonical identity for the adapter across systems
    #[serde(default)]
    pub content_hash: Option<String>,
    pub version: String,
    pub rank: u32,
    pub alpha: f32,
    /// Base model name (e.g., "qwen2.5-7b")
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
    pub category: String,
    pub scope: String,
    pub tier: String,
    pub target_modules: Vec<String>,
    pub created_at: String,
    pub weights_hash: String,
    pub training_data_hash: String,
    /// Compression method used for weights
    #[serde(default)]
    pub compression_method: String,
    /// Weight group configuration
    pub weight_groups: WeightGroupConfig,
    /// Full training provenance (embedded for portability)
    #[serde(default)]
    pub provenance: Option<ProvenanceData>,
    pub metadata: HashMap<String, String>,
}

fn default_schema_version() -> String {
    MANIFEST_SCHEMA_VERSION.to_string()
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

/// Configuration for weight groups
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeightGroupConfig {
    /// Whether to use separate positive/negative weights
    pub use_separate_weights: bool,
    /// Weight combination strategy for inference
    pub combination_strategy: CombinationStrategy,
}

/// Strategy for combining positive and negative weights
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CombinationStrategy {
    /// Simple difference: combined = positive - negative
    Difference,
    /// Weighted difference: combined = (positive * pos_scale) - (negative * neg_scale)
    WeightedDifference {
        /// Scaling factor applied to positive weights
        positive_scale: f32,
        /// Scaling factor applied to negative weights
        negative_scale: f32,
    },
    /// Separate inference: use positive and negative weights independently
    Separate,
}

impl Default for WeightGroupConfig {
    fn default() -> Self {
        Self {
            use_separate_weights: true,
            combination_strategy: CombinationStrategy::WeightedDifference {
                positive_scale: 1.0,
                negative_scale: 1.0,
            },
        }
    }
}

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
#[derive(Debug, Clone, Default)]
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
}

impl CreateOptions {
    /// Create options with base model information
    pub fn with_base_model(mut self, name: &str, id: Option<String>) -> Self {
        self.base_model = Some(name.to_string());
        self.base_model_id = id;
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

        let manifest = AdapterManifest {
            format_version: AOS_FORMAT_VERSION,
            schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
            adapter_id: adapter_id.clone(),
            content_hash: None, // Computed after manifest is finalized
            version: lineage.version.clone(),
            rank: config.rank as u32,
            alpha: config.alpha,
            base_model: options.base_model.unwrap_or_else(|| "qwen2.5-7b".to_string()),
            base_model_id: options.base_model_id,
            backend_family: options.backend_family,
            quantization: options.quantization,
            category: "code".to_string(),
            scope: "global".to_string(),
            tier: "persistent".to_string(),
            target_modules: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
            ],
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: weights_hash.clone(),
            training_data_hash,
            compression_method: "deflate-fast".to_string(),
            weight_groups: WeightGroupConfig::default(),
            provenance: options.provenance,
            metadata: HashMap::new(),
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
        crate::weights::combine_weight_groups(
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
