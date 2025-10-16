//! Import bundle

use crate::output::OutputWriter;
use adapteros_artifacts::bundle;
use adapteros_db::Db;
use anyhow::{Context, Result};
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
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;

    output.progress("Extracting bundle");

    // Extract bundle
    bundle::extract_bundle(bundle_path, temp_dir.path()).context("Failed to extract bundle")?;

    output.progress_done(true);

    // Load SBOM to get artifact metadata
    let sbom_path = temp_dir.path().join("sbom.json");
    let sbom_content = fs::read_to_string(&sbom_path).context("Failed to read SBOM file")?;

    let sbom: serde_json::Value =
        serde_json::from_str(&sbom_content).context("Failed to parse SBOM JSON")?;

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

        // Get signature (placeholder)
        let signature = "placeholder_signature_base64";

        // Get size
        let size_bytes = content.len() as i64;

        // Insert into database
        db.create_artifact(
            hash,
            kind,
            signature,
            None,
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
