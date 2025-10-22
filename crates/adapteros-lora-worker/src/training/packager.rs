//! Adapter packaging with safetensors and manifest generation
//!
//! Packages trained LoRA adapters into a format compatible with mplora-artifacts.

use super::quantizer::{LoRAQuantizer, QuantizedLoRAWeights};
use super::trainer::TrainingConfig;
use adapteros_core::{AosError, Result};
use safetensors::tensor::TensorView;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

/// Adapter packager
#[derive(Debug)]
pub struct AdapterPackager {
    output_dir: PathBuf,
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
    pub metadata: std::collections::HashMap<String, String>,
}

impl AdapterPackager {
    /// Create a new packager with output directory
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
        }
    }

    /// Package adapter with weights and manifest
    pub async fn package(
        &self,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
    ) -> Result<PackagedAdapter> {
        info!("Packaging adapter: {}", adapter_id);

        // Create adapter directory
        let adapter_dir = self.output_dir.join(adapter_id);
        tokio::fs::create_dir_all(&adapter_dir).await.map_err(|e| {
            AosError::Training(format!("Failed to create adapter directory: {}", e))
        })?;

        // Serialize weights to safetensors format
        let weights_path = adapter_dir.join("weights.safetensors");
        self.save_weights_safetensors(&weights_path, weights)
            .await?;

        // Compute BLAKE3 hash of weights
        let hash_b3 = self.compute_hash(&weights_path).await?;

        // Create manifest
        let manifest = AdapterManifest {
            version: "1.0.0".to_string(),
            rank: config.rank,
            base_model: base_model.to_string(),
            training_config: config.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: hash_b3.clone(),
            metadata: std::collections::HashMap::new(),
        };

        // Save manifest
        let manifest_path = adapter_dir.join("manifest.json");
        self.save_manifest(&manifest_path, &manifest).await?;

        // Sign the adapter (using mplora-crypto)
        self.sign_adapter(&adapter_dir).await?;

        info!("Adapter packaged successfully: {}", adapter_id);

        Ok(PackagedAdapter {
            adapter_id: adapter_id.to_string(),
            manifest,
            weights_path,
            hash_b3,
        })
    }

    /// Save weights in safetensors format
    async fn save_weights_safetensors(
        &self,
        path: &Path,
        weights: &QuantizedLoRAWeights,
    ) -> Result<()> {
        // Dequantize to f32 for runtime backends
        let deq = LoRAQuantizer::dequantize_from_q15(weights);

        // Default module list; future: make configurable
        let modules = ["q_proj", "k_proj", "v_proj", "o_proj"];

        // Build tensor views by reusing the same weights for each module
        let mut tensors: Vec<(String, TensorView)> = Vec::new();

        // Flatten helpers
        fn flatten_2d(m: &Vec<Vec<f32>>) -> Vec<u8> {
            let mut out = Vec::with_capacity(m.len() * m.get(0).map(|r| r.len()).unwrap_or(0) * 4);
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

        // Note: scalar config (alpha, dropout) can be embedded later if needed.

        let data = safetensors::serialize(tensors, &Default::default())
            .map_err(|e| AosError::Training(format!("safetensors serialize error: {}", e)))?;

        tokio::fs::write(path, data)
            .await
            .map_err(|e| AosError::Training(format!("Failed to write weights: {}", e)))?;

        Ok(())
    }

    /// Save manifest as JSON
    async fn save_manifest(&self, path: &Path, manifest: &AdapterManifest) -> Result<()> {
        let serialized = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| AosError::Training(format!("Failed to serialize manifest: {}", e)))?;

        tokio::fs::write(path, serialized)
            .await
            .map_err(|e| AosError::Training(format!("Failed to write manifest: {}", e)))?;

        Ok(())
    }

    /// Compute BLAKE3 hash of file
    async fn compute_hash(&self, path: &Path) -> Result<String> {
        let data = tokio::fs::read(path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read file for hashing: {}", e)))?;

        let hash = blake3::hash(&data);
        Ok(hash.to_hex().to_string())
    }

    /// Sign adapter directory with Ed25519
    async fn sign_adapter(&self, adapter_dir: &Path) -> Result<()> {
        // Generate or load signing keypair
        let keypair = adapteros_crypto::Keypair::generate();

        // Read manifest for signing
        let manifest_path = adapter_dir.join("manifest.json");
        let manifest_data = tokio::fs::read(&manifest_path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read manifest: {}", e)))?;

        // Sign manifest
        let signature = keypair.sign(&manifest_data);

        // Save signature
        let sig_path = adapter_dir.join("signature.sig");
        tokio::fs::write(sig_path, signature.to_bytes())
            .await
            .map_err(|e| AosError::Training(format!("Failed to write signature: {}", e)))?;

        // Save public key
        let pubkey_path = adapter_dir.join("public_key.pem");
        let pubkey_hex = hex::encode(keypair.public_key().to_bytes());
        tokio::fs::write(pubkey_path, pubkey_hex)
            .await
            .map_err(|e| AosError::Training(format!("Failed to write public key: {}", e)))?;

        info!("Adapter signed successfully");
        Ok(())
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
    pub async fn load(&self, adapter_id: &str) -> Result<PackagedAdapter> {
        let adapter_dir = self.output_dir.join(adapter_id);

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
            metadata: std::collections::HashMap::new(),
        };

        let packager = AdapterPackager::new(temp_dir.path());
        packager
            .save_manifest(&manifest_path, &manifest)
            .await
            .unwrap();

        // Load and verify
        let loaded_data = tokio::fs::read(&manifest_path).await.unwrap();
        let loaded_manifest: AdapterManifest = serde_json::from_slice(&loaded_data).unwrap();

        assert_eq!(loaded_manifest.rank, 4);
        assert_eq!(loaded_manifest.base_model, "test-model");
    }
}
