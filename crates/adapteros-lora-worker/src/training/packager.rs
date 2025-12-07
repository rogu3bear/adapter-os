//! Adapter packaging with safetensors and manifest generation
//!
//! Packages trained LoRA adapters into a format compatible with mplora-artifacts.

use super::quantizer::{LoRAQuantizer, QuantizedLoRAWeights};
use super::trainer::TrainingConfig;
use adapteros_aos::AosWriter;
use adapteros_core::{AosError, RepoAdapterPaths, Result};
use adapteros_crypto::Keypair;
use safetensors::tensor::TensorView;
use safetensors::SafeTensors;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;

/// Adapter packager.
/// Adapter-only invariant: only LoRA deltas are ever exported; base model
/// weights remain outside the package boundary.
#[derive(Debug)]
pub struct AdapterPackager {
    repo_root: PathBuf,
}

/// Packaged adapter with all metadata
#[derive(Debug, Clone)]
pub struct PackagedAdapter {
    pub adapter_id: String,
    pub manifest: AdapterManifest,
    pub weights_path: PathBuf,
    pub hash_b3: String,
}

/// Adapter manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub version: String,
    pub rank: usize,
    pub base_model: String,
    pub training_config: TrainingConfig,
    pub created_at: String,
    pub weights_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_layer_hashes: Option<std::collections::HashMap<String, LayerHash>>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Per-layer hash entry keyed by canonical logical layer path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerHash {
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tensor_name: Option<String>,
}

impl AdapterPackager {
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

    fn adapter_dir(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> std::result::Result<PathBuf, adapteros_core::ResolveError> {
        adapteros_core::adapter_fs_path_with_root(&self.repo_root, tenant_id, adapter_id)
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

        // Create manifest
        let manifest = AdapterManifest {
            version: "1.0.0".to_string(),
            rank: config.rank,
            base_model: base_model.to_string(),
            training_config: config.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: hash_b3.clone(),
            per_layer_hashes: Some(per_layer_hashes),
            metadata,
        };

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

    /// Package adapter as single .aos archive file
    ///
    /// Creates a single-file .aos archive containing manifest + weights.
    /// This is the preferred format for distribution and loading into Worker.
    pub async fn package_aos(
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

        // Compute BLAKE3 hash of weights and per-layer hashes
        let hash_b3 = blake3::hash(&weights_data).to_hex().to_string();
        let per_layer_hashes = Self::compute_per_layer_hashes_from_bytes(&weights_data)?;

        // Create manifest
        let manifest = AdapterManifest {
            version: "2.0".to_string(), // AOS 2.0 format
            rank: config.rank,
            base_model: base_model.to_string(),
            training_config: config.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: hash_b3.clone(),
            per_layer_hashes: Some(per_layer_hashes),
            metadata,
        };

        // Write .aos archive
        let aos_path = adapter_dir.join(format!("{}.aos", adapter_id));
        let writer = AosWriter::new();
        writer.write_archive(&aos_path, &manifest, &weights_data)?;

        // Deterministic signature for the archive to allow reproducible verification
        self.sign_archive(&aos_path, adapter_id).await?;

        info!(
            path = %aos_path.display(),
            size_kb = weights_data.len() / 1024,
            "AOS archive created successfully"
        );

        Ok(PackagedAdapter {
            adapter_id: adapter_id.to_string(),
            manifest,
            weights_path: aos_path,
            hash_b3,
        })
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
        let mut iter = tensor_name.split(|c| c == '.' || c == '/').peekable();

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
    fn build_safetensors_bytes(weights: &QuantizedLoRAWeights) -> Result<Vec<u8>> {
        // Dequantize to f32 for runtime backends
        let deq = LoRAQuantizer::dequantize_from_q15(weights);

        // Default module list; future: make configurable
        let modules = ["q_proj", "k_proj", "v_proj", "o_proj"];

        // Build tensor views by reusing the same weights for each module
        let mut tensors: Vec<(String, TensorView)> = Vec::new();

        // Flatten helpers
        fn flatten_2d(m: &Vec<Vec<f32>>) -> Vec<u8> {
            let mut out = Vec::with_capacity(m.len() * m.first().map(|r| r.len()).unwrap_or(0) * 4);
            for row in m {
                for &v in row {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
            out
        }

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

    #[tokio::test]
    async fn test_compute_hash() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        tokio::fs::write(&test_file, b"hello world").await.unwrap();

        let packager = AdapterPackager::new(temp_dir.path());
        let hash = packager.compute_hash(&test_file).await.unwrap();

        assert_eq!(hash.len(), 64); // BLAKE3 produces 256-bit hash (64 hex chars)
    }

    #[tokio::test]
    async fn test_save_load_manifest() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manifest_path = temp_dir.path().join("manifest.json");

        let manifest = AdapterManifest {
            version: "1.0.0".to_string(),
            rank: 4,
            base_model: "test-model".to_string(),
            training_config: TrainingConfig::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: "test_hash".to_string(),
            per_layer_hashes: None,
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
        let hashes =
            AdapterPackager::compute_per_layer_hashes_from_bytes(&serialized).expect("hashing ok");

        let canonical = "transformer.layer_0.attn.q_proj.lora_A";
        let entry = hashes
            .get(canonical)
            .expect("canonical layer entry should exist");
        assert_eq!(
            entry.tensor_name.as_deref(),
            Some("model.layers.0.attn.q_proj.lora_A.weight")
        );
        assert!(!entry.hash.is_empty());
    }
}
