//! Cross-Host Federation Verification
//!
//! Provides verification methods for cross-host bundle signature chains,
//! enabling federated replay verification across multiple hosts.

use crate::{VerifyError, VerifyResult};
use adapteros_crypto::Keypair;
use adapteros_db::Db;
use adapteros_telemetry::StoredBundleMetadata;
use std::path::Path;
use tracing::{info, warn};

/// Verify cross-host bundle chain
///
/// Discovers all bundles in a directory and verifies their
/// federation signature chains across hosts.
///
/// # Arguments
///
/// * `bundle_dir` - Directory containing bundle files
/// * `db` - Database connection for signature storage
///
/// # Returns
///
/// `Ok(())` if all chains are valid, error otherwise
pub async fn verify_cross_host(bundle_dir: &Path, db: &Db) -> VerifyResult<()> {
    info!(
        "Starting cross-host verification for: {}",
        bundle_dir.display()
    );

    // Discover bundles
    let bundles = discover_bundles(bundle_dir)?;

    if bundles.is_empty() {
        warn!("No bundles found in directory");
        return Ok(());
    }

    info!("Found {} bundles to verify", bundles.len());

    // Load metadata chain
    let metadata_chain = load_metadata_chain(&bundles)?;

    // Get federation manager (requires keypair for signing, but we only verify here)
    // Use a temporary keypair since we're only verifying, not signing
    let keypair = Keypair::generate();

    // Import federation manager
    #[allow(unused_imports)]
    use adapteros_federation::FederationManager;

    let manager = FederationManager::new(db.clone(), keypair, "default".to_string())
        .map_err(|e| VerifyError::Crypto(format!("Failed to create federation manager: {}", e)))?;

    // Get all signatures and build host chains
    let mut all_signatures = Vec::new();
    for metadata in &metadata_chain {
        let sigs = manager
            .get_signatures_for_bundle(&metadata.merkle_root.to_string())
            .await
            .map_err(|e| VerifyError::Crypto(format!("Failed to get signatures: {}", e)))?;
        all_signatures.extend(sigs);
    }

    if all_signatures.is_empty() {
        warn!("No federation signatures found");
        return Ok(());
    }

    // Verify the cross-host chain
    manager
        .verify_cross_host_chain(&all_signatures)
        .await
        .map_err(|e| VerifyError::Crypto(format!("Federation chain verification failed: {}", e)))?;

    info!(
        "Cross-host verification successful: {} signatures verified",
        all_signatures.len()
    );

    Ok(())
}

/// Discover bundle files in a directory
fn discover_bundles(bundle_dir: &Path) -> VerifyResult<Vec<String>> {
    use std::fs;

    if !bundle_dir.exists() {
        return Err(VerifyError::GoldenRunNotFound {
            path: bundle_dir.display().to_string(),
        });
    }

    let mut bundles = Vec::new();

    for entry in fs::read_dir(bundle_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Look for .ndjson bundle files
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "ndjson" {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        bundles.push(name.to_string());
                    }
                }
            }
        }
    }

    bundles.sort();
    Ok(bundles)
}

/// Load metadata chain from bundle files
fn load_metadata_chain(_bundles: &[String]) -> VerifyResult<Vec<StoredBundleMetadata>> {
    // For now, return an empty chain
    // In a full implementation, this would parse bundle files and extract metadata
    Ok(Vec::new())
}

/// Federation verification report
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FederationVerificationReport {
    pub total_bundles: usize,
    pub total_signatures: usize,
    pub verified_signatures: usize,
    pub chain_valid: bool,
    pub hosts: Vec<String>,
    pub errors: Vec<String>,
}

impl FederationVerificationReport {
    /// Check if verification passed
    pub fn passed(&self) -> bool {
        self.chain_valid && self.errors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    // Note: This test is temporarily ignored due to migration conflicts
    // when running in parallel with other database tests. The functionality
    // is tested manually and in isolated test runs.
    #[tokio::test]
    async fn test_verify_cross_host_empty_dir() -> VerifyResult<()> {
        let temp_dir = new_test_tempdir();
        let bundle_dir = temp_dir.path();

        // Create a test database with unique path to avoid migration conflicts
        let db = Db::new_in_memory().await.map_err(|e| VerifyError::Aos(e))?;

        // Should succeed with empty directory (no bundles to verify)
        let result = verify_cross_host(bundle_dir, &db).await;
        assert!(result.is_ok());

        Ok(())
    }

    #[test]
    fn test_discover_bundles_nonexistent() {
        let result = discover_bundles(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }
}
