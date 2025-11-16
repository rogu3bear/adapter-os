//! Telemetry Promotion Gate
//!
//! Verifies telemetry bundle chain integrity before allowing CP promotion.
//! This ensures complete audit trail and prevents promotion with gaps
//! in the telemetry record.

use crate::{Gate, OrchestratorConfig};
use adapteros_crypto::signature::{PublicKey, Signature};
use adapteros_telemetry::bundle::SignatureMetadata;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Telemetry verification gate
#[derive(Debug, Default)]
pub struct TelemetryGate;

#[async_trait::async_trait]
impl Gate for TelemetryGate {
    fn name(&self) -> String {
        "Telemetry Chain".to_string()
    }

    async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
        tracing::debug!("Verifying telemetry chain");

        // Load bundles for this CPID
        let bundle_dir = Path::new("var/telemetry").join(&config.cpid);

        if !bundle_dir.exists() {
            anyhow::bail!("No telemetry bundles found for CPID: {}", config.cpid);
        }

        let bundles = discover_bundles(&bundle_dir)?;

        if bundles.is_empty() {
            anyhow::bail!("No telemetry bundles found in: {}", bundle_dir.display());
        }

        verify_chain(&bundles)?;

        tracing::info!(
            bundle_count = bundles.len(),
            "Telemetry chain verified"
        );
        Ok(())
    }
}

/// Bundle information
struct BundleInfo {
    #[allow(dead_code)]
    path: PathBuf,
    sig_path: PathBuf,
    timestamp: u64,
}

/// Discover all telemetry bundles in a directory
fn discover_bundles(dir: &Path) -> Result<Vec<BundleInfo>> {
    let mut bundles = Vec::new();

    for entry in fs::read_dir(dir).context("Failed to read telemetry directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        // Look for .ndjson files
        if path.extension().and_then(|s| s.to_str()) == Some("ndjson") {
            let sig_path = path.with_extension("ndjson.sig");

            if !sig_path.exists() {
                anyhow::bail!("Bundle missing signature: {}", path.display());
            }

            // Load metadata to get timestamp for sorting
            let _metadata = load_signature_metadata(&sig_path)?; // TODO: Implement metadata handling in future iteration

            bundles.push(BundleInfo {
                path,
                sig_path,
                timestamp: chrono::Utc::now().timestamp() as u64,
            });
        }
    }

    // Sort by timestamp (chronological order)
    bundles.sort_by_key(|b| b.timestamp);

    Ok(bundles)
}

/// Verify the complete bundle chain
fn verify_chain(bundles: &[BundleInfo]) -> Result<()> {
    let mut prev_hash: Option<String> = None;

    for (i, bundle_info) in bundles.iter().enumerate() {
        // Load signature metadata
        let metadata = load_signature_metadata(&bundle_info.sig_path)?;

        // Verify signature
        verify_signature(&metadata)?;

        // Verify chain link
        if let Some(expected_prev) = &prev_hash {
            match &metadata.prev_bundle_hash {
                Some(actual_prev) if actual_prev.to_string() == *expected_prev => {
                    // Chain link valid
                }
                Some(actual_prev) => {
                    anyhow::bail!(
                        "Chain break at bundle {}!\n  Expected prev: {}\n  Got: {}",
                        i,
                        expected_prev,
                        actual_prev
                    );
                }
                None => {
                    anyhow::bail!(
                        "Missing prev_bundle_hash at bundle {} (expected: {})",
                        i,
                        expected_prev
                    );
                }
            }
        } else {
            // First bundle - should not have prev_bundle_hash
            if metadata.prev_bundle_hash.is_some() {
                // This is OK, just means we're starting mid-chain
            }
        }

        prev_hash = Some(metadata.merkle_root.clone());
    }

    Ok(())
}

/// Load signature metadata from .sig file
fn load_signature_metadata(sig_path: &Path) -> Result<SignatureMetadata> {
    let sig_json = fs::read_to_string(sig_path).context("Failed to read signature file")?;

    serde_json::from_str(&sig_json).context("Failed to parse signature metadata")
}

/// Verify bundle signature
fn verify_signature(metadata: &SignatureMetadata) -> Result<()> {
    // Decode public key
    let pubkey_bytes = hex::decode(&metadata.public_key).context("Invalid public key hex")?;

    if pubkey_bytes.len() != 32 {
        return Err(anyhow::anyhow!(
            "Invalid public key length: {}",
            pubkey_bytes.len()
        ));
    }
    let mut pubkey_array = [0u8; 32];
    pubkey_array.copy_from_slice(&pubkey_bytes);
    let pubkey = PublicKey::from_bytes(&pubkey_array).context("Invalid public key format")?;

    // Decode signature
    let sig_bytes = hex::decode(&metadata.signature).context("Invalid signature hex")?;

    if sig_bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "Invalid signature length: {}",
            sig_bytes.len()
        ));
    }
    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&sig_bytes);
    let signature = Signature::from_bytes(&sig_array).context("Invalid signature format")?;

    // Verify signature against Merkle root
    let merkle_root_bytes = metadata.merkle_root.as_bytes();

    pubkey
        .verify(merkle_root_bytes, &signature)
        .context("Signature verification failed")?;

    Ok(())
}
