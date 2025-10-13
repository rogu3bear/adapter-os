//! Golden run archive creation and management

use crate::{epsilon::EpsilonStatistics, metadata::GoldenRunMetadata, VerifyError, VerifyResult};
use adapteros_core::B3Hash;
use adapteros_crypto::{sign_bytes, Keypair};
use adapteros_telemetry::replay::load_replay_bundle;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{debug, info};

/// Golden run archive containing all necessary data for verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenRunArchive {
    /// Metadata about the run
    pub metadata: GoldenRunMetadata,
    /// Epsilon statistics for floating-point verification
    pub epsilon_stats: EpsilonStatistics,
    /// Hash of the complete event bundle
    pub bundle_hash: B3Hash,
    /// Signature over the archive (optional, for audit trail)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

impl GoldenRunArchive {
    /// Create a new golden run archive
    pub fn new(
        metadata: GoldenRunMetadata,
        epsilon_stats: EpsilonStatistics,
        bundle_hash: B3Hash,
    ) -> Self {
        Self {
            metadata,
            epsilon_stats,
            bundle_hash,
            signature: None,
        }
    }

    /// Sign the archive with a keypair
    pub fn sign(&mut self, keypair: &Keypair) -> VerifyResult<()> {
        // Serialize archive for signing (excluding signature field)
        let mut archive_for_signing = self.clone();
        archive_for_signing.signature = None;

        let serialized =
            serde_json::to_vec(&archive_for_signing).map_err(VerifyError::Serialization)?;

        let signature = sign_bytes(keypair, &serialized);

        self.signature = Some(hex::encode(signature.to_bytes()));
        Ok(())
    }

    /// Save the archive to disk
    pub fn save<P: AsRef<Path>>(&self, dir_path: P) -> VerifyResult<()> {
        let dir_path = dir_path.as_ref();
        fs::create_dir_all(dir_path)?;

        // Write manifest
        let manifest_path = dir_path.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&self.metadata)?;
        fs::write(&manifest_path, manifest_json)?;
        info!("Wrote manifest: {}", manifest_path.display());

        // Write epsilon stats
        let epsilon_path = dir_path.join("epsilon_stats.json");
        let epsilon_json = serde_json::to_string_pretty(&self.epsilon_stats)?;
        fs::write(&epsilon_path, epsilon_json)?;
        info!("Wrote epsilon stats: {}", epsilon_path.display());

        // Write bundle hash
        let hash_path = dir_path.join("bundle_hash.txt");
        fs::write(&hash_path, self.bundle_hash.to_string())?;
        debug!("Wrote bundle hash: {}", hash_path.display());

        // Write signature if present
        if let Some(ref sig) = self.signature {
            let sig_path = dir_path.join("signature.sig");
            fs::write(&sig_path, sig)?;
            info!("Wrote signature: {}", sig_path.display());
        }

        Ok(())
    }

    /// Load an archive from disk
    pub fn load<P: AsRef<Path>>(dir_path: P) -> VerifyResult<Self> {
        let dir_path = dir_path.as_ref();

        if !dir_path.exists() {
            return Err(VerifyError::GoldenRunNotFound {
                path: dir_path.display().to_string(),
            });
        }

        // Load manifest
        let manifest_path = dir_path.join("manifest.json");
        let manifest_json =
            fs::read_to_string(&manifest_path).map_err(|_| VerifyError::ArchiveCorrupted {
                reason: "Missing manifest.json".to_string(),
            })?;
        let metadata: GoldenRunMetadata = serde_json::from_str(&manifest_json)?;

        // Load epsilon stats
        let epsilon_path = dir_path.join("epsilon_stats.json");
        let epsilon_json =
            fs::read_to_string(&epsilon_path).map_err(|_| VerifyError::ArchiveCorrupted {
                reason: "Missing epsilon_stats.json".to_string(),
            })?;
        let epsilon_stats: EpsilonStatistics = serde_json::from_str(&epsilon_json)?;

        // Load bundle hash
        let hash_path = dir_path.join("bundle_hash.txt");
        let hash_str =
            fs::read_to_string(&hash_path).map_err(|_| VerifyError::ArchiveCorrupted {
                reason: "Missing bundle_hash.txt".to_string(),
            })?;
        let bundle_hash = hash_str.trim().to_string();

        // Load signature if present
        let sig_path = dir_path.join("signature.sig");
        let signature = if sig_path.exists() {
            Some(fs::read_to_string(&sig_path)?)
        } else {
            None
        };

        Ok(Self {
            metadata,
            epsilon_stats,
            bundle_hash: B3Hash::from_hex(&bundle_hash).map_err(|_| {
                VerifyError::ArchiveCorrupted {
                    reason: format!("Invalid bundle hash: {}", bundle_hash),
                }
            })?,
            signature,
        })
    }
}

/// Create a golden run archive from a replay bundle
pub async fn create_golden_run<P: AsRef<Path>>(
    bundle_path: P,
    toolchain_version: &str,
    adapter_ids: &[&str],
) -> VerifyResult<GoldenRunArchive> {
    let bundle_path = bundle_path.as_ref();

    info!("Creating golden run from bundle: {}", bundle_path.display());

    // Load the replay bundle
    let bundle = load_replay_bundle(bundle_path).map_err(|e| VerifyError::ArchiveCorrupted {
        reason: format!("Failed to load replay bundle: {}", e),
    })?;

    // Compute bundle hash
    let bundle_content = fs::read(bundle_path)?;
    let bundle_hash = B3Hash::hash(&bundle_content);

    debug!("Bundle hash: {}", bundle_hash);

    // Extract epsilon statistics from events
    let epsilon_stats = EpsilonStatistics::from_replay_bundle(&bundle)?;

    info!(
        "Extracted epsilon stats: {} layers, max_ε={:.6e}",
        epsilon_stats.layer_stats.len(),
        epsilon_stats.max_epsilon()
    );

    // Create metadata
    let metadata = GoldenRunMetadata::new(
        bundle.cpid.clone(),
        bundle.plan_id.clone(),
        toolchain_version.to_string(),
        adapter_ids.iter().map(|s| s.to_string()).collect(),
        bundle.seed_global,
    );

    Ok(GoldenRunArchive::new(metadata, epsilon_stats, bundle_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epsilon::EpsilonStats;
    use crate::metadata::DeviceFingerprint;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_archive() -> GoldenRunArchive {
        let metadata = GoldenRunMetadata {
            run_id: "test-001".to_string(),
            cpid: "test-cpid".to_string(),
            plan_id: "test-plan".to_string(),
            created_at: chrono::Utc::now(),
            toolchain: crate::metadata::ToolchainMetadata {
                rustc_version: "1.75.0".to_string(),
                metal_version: "3.1".to_string(),
                kernel_hash: B3Hash::from_hex(
                    "b3:0000000000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap(),
            },
            adapters: vec!["adapter-001".to_string()],
            device: DeviceFingerprint {
                schema_version: 1,
                device_model: "Apple M2 Max".to_string(),
                soc_id: "M2 Max".to_string(),
                gpu_pci_id: "0x0000".to_string(),
                os_version: "14.0".to_string(),
                os_build: "23A344".to_string(),
                metal_family: "Apple9".to_string(),
                gpu_driver_version: "3.1.0".to_string(),
                path_hash: B3Hash::from_hex(
                    "b3:0000000000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap(),
                env_hash: B3Hash::from_hex(
                    "b3:0000000000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap(),
                cpu_features: vec!["neon".to_string(), "fp".to_string()],
                firmware_hash: None,
                boot_version_hash: None,
            },
            global_seed: B3Hash::from_hex(
                "b3:1111111111111111111111111111111111111111111111111111111111111111",
            )
            .unwrap(),
        };

        let mut layer_stats = HashMap::new();
        layer_stats.insert(
            "layer_0".to_string(),
            EpsilonStats {
                l2_error: 1e-7,
                max_error: 5e-7,
                mean_error: 2e-7,
                element_count: 1000,
            },
        );

        let epsilon_stats = EpsilonStatistics { layer_stats };

        let bundle_hash =
            B3Hash::from_hex("b3:2222222222222222222222222222222222222222222222222222222222222222")
                .unwrap();

        GoldenRunArchive::new(metadata, epsilon_stats, bundle_hash)
    }

    #[test]
    fn test_archive_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let archive_dir = temp_dir.path().join("test-archive");

        let archive = create_test_archive();
        archive.save(&archive_dir).unwrap();

        // Verify files exist
        assert!(archive_dir.join("manifest.json").exists());
        assert!(archive_dir.join("epsilon_stats.json").exists());
        assert!(archive_dir.join("bundle_hash.txt").exists());

        // Load and verify
        let loaded = GoldenRunArchive::load(&archive_dir).unwrap();
        assert_eq!(loaded.metadata.run_id, "test-001");
        assert_eq!(loaded.epsilon_stats.layer_stats.len(), 1);
        assert_eq!(loaded.bundle_hash, archive.bundle_hash);
    }

    #[test]
    fn test_archive_load_missing() {
        let temp_dir = TempDir::new().unwrap();
        let missing_path = temp_dir.path().join("nonexistent");

        let result = GoldenRunArchive::load(&missing_path);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            VerifyError::GoldenRunNotFound { .. }
        ));
    }
}
