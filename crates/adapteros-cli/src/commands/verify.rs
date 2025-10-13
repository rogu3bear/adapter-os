//! Verify bundle

use crate::output::OutputWriter;
use anyhow::{Context, Result};
use adapteros_artifacts::bundle;
use adapteros_core::B3Hash;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Serialize)]
struct VerificationResult {
    signature_verified: bool,
    sbom_complete: bool,
    artifacts_verified: usize,
    artifacts_total: usize,
    bundle_hash: String,
}

pub async fn run(bundle_path: &Path, output: &OutputWriter) -> Result<()> {
    output.info(format!("Verifying bundle: {}", bundle_path.display()));

    // Create temporary directory for extraction
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;

    output.progress("Extracting bundle");

    // Extract bundle
    bundle::extract_bundle(bundle_path, temp_dir.path()).context("Failed to extract bundle")?;

    output.progress_done(true);

    // Load signature file
    let signature_path = temp_dir.path().join("signature.sig");
    if !signature_path.exists() {
        return Err(anyhow::anyhow!("Signature file not found in bundle"));
    }

    let signature_data = fs::read(&signature_path).context("Failed to read signature file")?;

    output.success("Signature file found");

    // Load public key from metadata (stored in bundle for verification)
    let pubkey_path = temp_dir.path().join("public_key.hex");
    let public_key_hex = if pubkey_path.exists() {
        fs::read_to_string(&pubkey_path).context("Failed to read public key")?
    } else {
        output.warning("No public key found in bundle, skipping signature verification");
        output.verbose("(Public key should be in public_key.hex)");
        return Ok(());
    };

    // Decode hex-encoded public key
    let public_key_bytes =
        hex::decode(public_key_hex.trim()).context("Failed to decode public key hex")?;
    if public_key_bytes.len() != 32 {
        return Err(anyhow::anyhow!(
            "Invalid public key length: expected 32 bytes, got {}",
            public_key_bytes.len()
        ));
    }
    let mut pk_array = [0u8; 32];
    pk_array.copy_from_slice(&public_key_bytes);
    let public_key =
        adapteros_crypto::PublicKey::from_bytes(&pk_array).context("Failed to parse public key")?;

    // Decode hex-encoded signature
    let signature_hex =
        String::from_utf8(signature_data).context("Failed to parse signature as UTF-8")?;
    let signature_bytes =
        hex::decode(signature_hex.trim()).context("Failed to decode signature hex")?;
    if signature_bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "Invalid signature length: expected 64 bytes, got {}",
            signature_bytes.len()
        ));
    }
    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&signature_bytes);
    let signature =
        adapteros_crypto::Signature::from_bytes(&sig_array).context("Failed to parse signature")?;

    // Load SBOM file
    let sbom_path = temp_dir.path().join("sbom.json");
    if !sbom_path.exists() {
        return Err(anyhow::anyhow!("SBOM file not found in bundle"));
    }

    let sbom_content = fs::read_to_string(&sbom_path).context("Failed to read SBOM file")?;

    // Verify signature against SBOM content
    adapteros_crypto::verify_signature(&public_key, sbom_content.as_bytes(), &signature)
        .context("Signature verification failed")?;

    output.success("Signature verified successfully");

    let sbom: serde_json::Value =
        serde_json::from_str(&sbom_content).context("Failed to parse SBOM JSON")?;

    output.success("SBOM file found");

    // Verify SBOM completeness
    let artifacts = sbom["artifacts"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("SBOM missing artifacts array"))?;

    output.info(format!("SBOM lists {} artifacts", artifacts.len()));

    // Verify hashes for all artifacts
    let mut verified_count = 0;
    for artifact in artifacts {
        let path = artifact["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Artifact missing path"))?;

        let expected_hash = artifact["hash"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Artifact missing hash"))?;

        let artifact_path = temp_dir.path().join(path);
        if !artifact_path.exists() {
            return Err(anyhow::anyhow!("Artifact not found: {}", path));
        }

        // Compute hash
        let content = fs::read(&artifact_path)
            .with_context(|| format!("Failed to read artifact: {}", path))?;

        let computed_hash = B3Hash::hash(&content);

        if computed_hash.to_string() != expected_hash {
            return Err(anyhow::anyhow!(
                "Hash mismatch for {}: expected {}, got {}",
                path,
                expected_hash,
                computed_hash
            ));
        }

        verified_count += 1;
        output.verbose(format!("Verified: {}", path));
    }

    output.success(format!("All {} artifact hashes verified", verified_count));

    // Compute bundle hash for determinism verification
    let bundle_content =
        fs::read(bundle_path).context("Failed to read bundle for hash computation")?;
    let bundle_hash = adapteros_core::B3Hash::hash(&bundle_content);

    output.blank();
    output.success("Bundle verification complete");
    output.kv("Bundle hash (deterministic)", &bundle_hash.to_string());
    output.kv("Signature", "verified");
    output.kv("SBOM", "complete");
    output.kv(
        "Artifacts",
        &format!("{}/{} verified", verified_count, artifacts.len()),
    );

    if output.is_json() {
        let result = VerificationResult {
            signature_verified: true,
            sbom_complete: true,
            artifacts_verified: verified_count,
            artifacts_total: artifacts.len(),
            bundle_hash: bundle_hash.to_string(),
        };
        output.json(&result)?;
    }

    Ok(())
}
