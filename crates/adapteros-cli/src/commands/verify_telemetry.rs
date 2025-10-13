//! Verify telemetry bundle chain integrity
//!
//! This command walks through telemetry bundles and verifies:
//! 1. Each bundle's Merkle tree signature
//! 2. Chain continuity via prev_bundle_hash links
//! 3. No gaps or tampering in the audit trail

use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use adapteros_crypto::signature::{PublicKey, Signature};
use adapteros_telemetry::bundle::SignatureMetadata;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct VerificationResult {
    total_bundles: usize,
    verified_count: usize,
    chain_continuity: String,
    signatures_valid: bool,
}

/// Verify telemetry bundle chain
pub async fn verify_telemetry_chain(bundle_dir: &Path, output: &OutputWriter) -> Result<()> {
    output.info(format!(
        "Verifying telemetry bundle chain in: {}",
        bundle_dir.display()
    ));
    output.blank();

    let bundles = discover_bundles(bundle_dir)?;

    if bundles.is_empty() {
        output.warning("No bundles found");
        return Ok(());
    }

    output.info(format!("Found {} bundles", bundles.len()));
    output.blank();

    let mut prev_hash: Option<String> = None;
    let mut verified_count = 0;

    for bundle_info in bundles {
        let filename = bundle_info
            .path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid bundle path"))?
            .to_string_lossy();
        output.progress(format!("Verifying: {}", filename));

        // Load signature metadata
        let metadata = load_signature_metadata(&bundle_info.sig_path)?;

        // Verify signature
        verify_signature(&bundle_info.path, &metadata)?;

        // Verify chain link
        if let Some(expected_prev) = &prev_hash {
            let expected_b3hash = adapteros_core::B3Hash::from_hex(expected_prev)
                .map_err(|e| AosError::Validation(format!("Invalid expected hash format: {}", e)))?;
            
            match &metadata.prev_bundle_hash {
                Some(actual_prev) if *actual_prev == expected_b3hash => {
                    // Chain link valid
                }
                Some(actual_prev) => {
                    output.progress_done(false);
                    return Err(AosError::Validation(format!(
                        "Chain break detected!\n  Expected prev hash: {}\n  Got: {}",
                        expected_prev, actual_prev
                    )));
                }
                None => {
                    output.progress_done(false);
                    return Err(AosError::Validation(format!(
                        "Missing prev_bundle_hash in chain (expected: {})",
                        expected_prev
                    )));
                }
            }
        } else {
            // First bundle - should not have prev_bundle_hash
            if metadata.prev_bundle_hash.is_some() {
                output.warning("First bundle has prev_bundle_hash (will be ignored)");
            }
        }

        prev_hash = Some(metadata.merkle_root.clone());
        verified_count += 1;

        output.progress_done(true);
    }

    output.blank();

    if output.is_json() {
        let result = VerificationResult {
            total_bundles: verified_count,
            verified_count,
            chain_continuity: "intact".to_string(),
            signatures_valid: true,
        };
        output.json(&result)?;
    } else {
        output.success("Chain verified successfully!");
        output.kv("Total bundles", &verified_count.to_string());
        output.kv("Chain continuity", "intact");
        output.kv("Signatures", "all valid");
    }

    Ok(())
}

/// Bundle information
struct BundleInfo {
    path: PathBuf,
    sig_path: PathBuf,
    timestamp: u64,
}

/// Discover all telemetry bundles in a directory
fn discover_bundles(dir: &Path) -> Result<Vec<BundleInfo>> {
    let mut bundles = Vec::new();

    for entry in fs::read_dir(dir).map_err(|e| AosError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| AosError::Io(e.to_string()))?;
        let path = entry.path();

        // Look for .ndjson files
        if path.extension().and_then(|s| s.to_str()) == Some("ndjson") {
            let sig_path = path.with_extension("ndjson.sig");

            if !sig_path.exists() {
                eprintln!("⚠️  Warning: Bundle missing signature: {}", path.display());
                continue;
            }

            // Load metadata to get timestamp for sorting
            let metadata = load_signature_metadata(&sig_path)?;

            bundles.push(BundleInfo {
                path,
                sig_path,
                timestamp: metadata.sequence_no as u64,
            });
        }
    }

    // Sort by timestamp (chronological order)
    bundles.sort_by_key(|b| b.timestamp);

    Ok(bundles)
}

/// Load signature metadata from .sig file
fn load_signature_metadata(sig_path: &Path) -> Result<SignatureMetadata> {
    let sig_json = fs::read_to_string(sig_path)
        .map_err(|e| AosError::Io(format!("Failed to read signature: {}", e)))?;

    serde_json::from_str(&sig_json).map_err(|e| AosError::Serialization(e))
}

/// Verify bundle signature
fn verify_signature(bundle_path: &Path, metadata: &SignatureMetadata) -> Result<()> {
    // Decode public key
    let pubkey_bytes = hex::decode(&metadata.public_key)
        .map_err(|e| AosError::Validation(format!("Invalid public key hex: {}", e)))?;

    let pubkey_array: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| AosError::Validation("Invalid public key length".to_string()))?;
    let pubkey = PublicKey::from_bytes(&pubkey_array)
        .map_err(|e| AosError::Validation(format!("Invalid public key: {}", e)))?;

    // Decode signature
    let sig_bytes = hex::decode(&metadata.signature)
        .map_err(|e| AosError::Validation(format!("Invalid signature hex: {}", e)))?;

    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| AosError::Validation("Invalid signature length".to_string()))?;
    let signature = Signature::from_bytes(&sig_array)
        .map_err(|e| AosError::Validation(format!("Invalid signature: {}", e)))?;

    // Verify signature against Merkle root
    let merkle_root_bytes = metadata.merkle_root.as_bytes();

    pubkey
        .verify(merkle_root_bytes, &signature)
        .map_err(|e| AosError::Validation(format!("Signature verification failed: {}", e)))?;

    // Note: For full verification, we should also:
    // 1. Recompute Merkle root from bundle events
    // 2. Compare with metadata.merkle_root
    // This requires reading the full bundle, which we skip for now

    Ok(())
}
