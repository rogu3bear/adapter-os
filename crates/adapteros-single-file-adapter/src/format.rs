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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeightGroupType {
    Positive,
    Negative,
    Combined,
}

/// Compression level for .aos packaging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    /// No compression (fastest)
    Store,
    /// Fast compression (default for most files)
    Fast,
    /// Best compression (slowest, smallest)
    Best,
}

impl Default for CompressionLevel {
    fn default() -> Self {
        Self::Fast
    }
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    /// .aos format version (for compatibility checks)
    pub format_version: u8,
    pub adapter_id: String,
    pub version: String,
    pub rank: u32,
    pub alpha: f32,
    pub base_model: String,
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
    pub metadata: HashMap<String, String>,
}

/// Configuration for weight groups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightGroupConfig {
    /// Whether to use separate positive/negative weights
    pub use_separate_weights: bool,
    /// Weight combination strategy for inference
    pub combination_strategy: CombinationStrategy,
    /// Positive weight scaling factor
    pub positive_scale: f32,
    /// Negative weight scaling factor
    pub negative_scale: f32,
}

/// Strategy for combining positive and negative weights
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombinationStrategy {
    /// Simple difference: combined = positive - negative
    Difference,
    /// Weighted difference: combined = (positive * pos_scale) - (negative * neg_scale)
    WeightedDifference,
    /// Separate inference: use positive and negative weights independently
    Separate,
}

impl Default for WeightGroupConfig {
    fn default() -> Self {
        Self {
            use_separate_weights: true,
            combination_strategy: CombinationStrategy::WeightedDifference,
            positive_scale: 1.0,
            negative_scale: 1.0,
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

impl SingleFileAdapter {
    /// Create a new single-file adapter
    pub fn create(
        adapter_id: String,
        weights: AdapterWeights,
        training_data: Vec<TrainingExample>,
        config: TrainingConfig,
        lineage: LineageInfo,
    ) -> Result<Self> {
        let weights_hash = B3Hash::hash(&serde_json::to_vec(&weights)?).to_hex();
        let training_data_hash = B3Hash::hash(&serde_json::to_vec(&training_data)?).to_hex();

        let manifest = AdapterManifest {
            format_version: AOS_FORMAT_VERSION,
            adapter_id: adapter_id.clone(),
            version: lineage.version.clone(),
            rank: config.rank as u32,
            alpha: config.alpha,
            base_model: "qwen2.5-7b".to_string(),
            category: "code".to_string(),
            scope: "global".to_string(),
            tier: "persistent".to_string(),
            target_modules: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
            ],
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash,
            training_data_hash,
            compression_method: "deflate-fast".to_string(),
            weight_groups: WeightGroupConfig::default(),
            metadata: HashMap::new(),
        };

        Ok(Self {
            manifest,
            weights,
            training_data,
            config,
            lineage,
            signature: None,
        })
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
        let config = &self.manifest.weight_groups;

        match config.combination_strategy {
            CombinationStrategy::Difference => self.compute_weight_difference(
                &self.weights.positive,
                &self.weights.negative,
                1.0,
                1.0,
            ),
            CombinationStrategy::WeightedDifference => self.compute_weight_difference(
                &self.weights.positive,
                &self.weights.negative,
                config.positive_scale,
                config.negative_scale,
            ),
            CombinationStrategy::Separate => {
                // For separate inference, return positive weights as default
                Ok(self.weights.positive.clone())
            }
        }
    }

    /// Compute weight difference: result = (pos * pos_scale) - (neg * neg_scale)
    fn compute_weight_difference(
        &self,
        positive: &WeightGroup,
        negative: &WeightGroup,
        pos_scale: f32,
        neg_scale: f32,
    ) -> Result<WeightGroup> {
        // Ensure dimensions match
        if positive.lora_a.len() != negative.lora_a.len()
            || positive.lora_b.len() != negative.lora_b.len()
        {
            return Err(AosError::Training(
                "Weight group dimensions don't match".to_string(),
            ));
        }

        let mut combined_lora_a = Vec::new();
        let mut combined_lora_b = Vec::new();

        // Combine LoRA A matrices
        for (pos_row, neg_row) in positive.lora_a.iter().zip(negative.lora_a.iter()) {
            let mut combined_row = Vec::new();
            for (pos_val, neg_val) in pos_row.iter().zip(neg_row.iter()) {
                combined_row.push((pos_val * pos_scale) - (neg_val * neg_scale));
            }
            combined_lora_a.push(combined_row);
        }

        // Combine LoRA B matrices
        for (pos_row, neg_row) in positive.lora_b.iter().zip(negative.lora_b.iter()) {
            let mut combined_row = Vec::new();
            for (pos_val, neg_val) in pos_row.iter().zip(neg_row.iter()) {
                combined_row.push((pos_val * pos_scale) - (neg_val * neg_scale));
            }
            combined_lora_b.push(combined_row);
        }

        Ok(WeightGroup {
            lora_a: combined_lora_a,
            lora_b: combined_lora_b,
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
    } else if file_version < current_version {
        if file_version < 2 {
            warnings.push("File uses old format without separate weight groups".to_string());
        }
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
