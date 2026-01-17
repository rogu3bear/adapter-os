//! Import bundle

use crate::output::OutputWriter;
use adapteros_artifacts::bundle;
use adapteros_core::B3Hash;
use adapteros_db::Db;
use adapteros_platform::common::PlatformUtils;
use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Serialize)]
struct ImportResult {
    artifacts_count: usize,
    location: String,
}

pub async fn run(bundle_path: &Path, verify: bool, output: &OutputWriter) -> Result<()> {
    output.info(format!("Importing bundle: {}", bundle_path.display()));

    // Verify bundle if requested
    if verify {
        output.info("Verifying bundle...");
        super::verify::run(bundle_path, output).await?;
    }

    // Create temporary directory for extraction
    let temp_root = PlatformUtils::temp_dir();
    fs::create_dir_all(&temp_root).with_context(|| {
        format!(
            "Failed to create AdapterOS temp directory {}",
            temp_root.display()
        )
    })?;
    let temp_dir = tempfile::Builder::new()
        .prefix("adapteros-import-")
        .tempdir_in(&temp_root)
        .context("Failed to create temporary directory")?;

    output.progress("Extracting bundle");

    // Extract bundle
    bundle::extract_bundle(bundle_path, temp_dir.path()).context("Failed to extract bundle")?;

    output.progress_done(true);

    // Load SBOM to get artifact metadata
    let sbom_path = temp_dir.path().join("sbom.json");
    let sbom_content = fs::read_to_string(&sbom_path).context("Failed to read SBOM file")?;

    let sbom: serde_json::Value =
        serde_json::from_str(&sbom_content).context("Failed to parse SBOM JSON")?;

    let sbom_hash_b3 = B3Hash::hash(sbom_content.as_bytes()).to_hex().to_string();
    let signature_b64 = load_signature_b64(temp_dir.path(), verify, output)?;

    let artifacts = sbom["artifacts"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("SBOM missing artifacts array"))?;

    output.info(format!("Processing {} artifacts", artifacts.len()));

    // Connect to database
    let db = Db::connect_env()
        .await
        .context("Failed to connect to database")?;

    // Create artifacts directory (CAS storage)
    let cas_dir = Path::new("./artifacts");
    fs::create_dir_all(cas_dir).context("Failed to create artifacts directory")?;

    // Import each artifact
    let mut imported_count = 0;
    for artifact in artifacts {
        let path = artifact["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Artifact missing path"))?;

        let hash = artifact["hash"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Artifact missing hash"))?;

        let kind = artifact["kind"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Artifact missing kind"))?;

        let artifact_path = temp_dir.path().join(path);
        let content = fs::read(&artifact_path)
            .with_context(|| format!("Failed to read artifact: {}", path))?;

        // Copy to CAS storage (hash-addressed)
        let cas_path = cas_dir.join(hash);
        fs::write(&cas_path, &content)
            .with_context(|| format!("Failed to write to CAS: {}", hash))?;

        // Get size
        let size_bytes = content.len() as i64;

        // Insert into database
        db.create_artifact(
            hash,
            kind,
            signature_b64.as_str(),
            Some(sbom_hash_b3.as_str()),
            size_bytes,
            cas_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        )
        .await
        .with_context(|| format!("Failed to create artifact record: {}", hash))?;

        imported_count += 1;
        output.verbose(format!("Imported: {}", path));
    }

    output.success(format!("{} artifacts imported to CAS", imported_count));
    output.kv("Location", &cas_dir.display().to_string());

    if output.is_json() {
        let result = ImportResult {
            artifacts_count: imported_count,
            location: cas_dir.display().to_string(),
        };
        output.json(&result)?;
    }

    Ok(())
}

fn load_signature_b64(bundle_root: &Path, verify: bool, output: &OutputWriter) -> Result<String> {
    let sig_path = bundle_root.join("signature.sig");
    if !sig_path.exists() {
        if verify {
            return Err(anyhow::anyhow!("Signature file not found in bundle"));
        }
        output.warning("Signature file not found; recording unsigned bundle");
        return Ok("unsigned".to_string());
    }

    let sig_data = fs::read(&sig_path).context("Failed to read signature file")?;
    let sig_bytes = decode_signature_bytes(&sig_data)?;
    Ok(STANDARD.encode(sig_bytes))
}

fn decode_signature_bytes(sig_data: &[u8]) -> Result<Vec<u8>> {
    let bytes = if let Ok(sig_hex) = std::str::from_utf8(sig_data) {
        let trimmed = sig_hex.trim();
        match hex::decode(trimmed) {
            Ok(decoded) => decoded,
            Err(_) => sig_data.to_vec(),
        }
    } else {
        sig_data.to_vec()
    };

    if bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "Invalid signature length: expected 64 bytes, got {}",
            bytes.len()
        ));
    }

    Ok(bytes)
}
