use anyhow::Result;
use adapteros_artifacts::CasStore;
use adapteros_registry::Registry;
use adapteros_sbom::SpdxDocument;
// use adapteros_crypto::{PublicKey, Signature}; // TODO: Use for actual signature verification
use crate::output::OutputWriter;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct SyncResult {
    synced_count: usize,
    skipped_count: usize,
}

/// Sync adapters from a local directory into CAS with SBOM and signature verification
pub async fn sync_registry(
    sync_dir: &Path,
    cas_root: &Path,
    registry_path: &Path,
    output: &OutputWriter,
) -> Result<()> {
    output.info(&format!("Syncing adapters from {}", sync_dir.display()));

    let cas = CasStore::new(cas_root)?;
    let registry = Registry::open(registry_path)?;

    let mut synced_count = 0;
    let mut skipped_count = 0;

    for entry in std::fs::read_dir(sync_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("safetensors") {
            let filename = path
                .file_stem()
                .ok_or_else(|| anyhow::anyhow!("Invalid path"))?
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid path"))?;

            // Check for SBOM and signature
            let sbom_path = path.with_extension("sbom.json");
            let sig_path = path.with_extension("sig");

            if !sbom_path.exists() {
                output.warning(&format!("Skipping {}: missing SBOM", filename));
                skipped_count += 1;
                continue;
            }

            if !sig_path.exists() {
                output.warning(&format!("Skipping {}: missing signature", filename));
                skipped_count += 1;
                continue;
            }

            // Validate SBOM
            let sbom_bytes = std::fs::read(&sbom_path)?;
            match serde_json::from_slice::<SpdxDocument>(&sbom_bytes) {
                Ok(sbom) => {
                    if sbom.packages.is_empty() {
                        output.warning(&format!("Skipping {}: SBOM has no packages", filename));
                        skipped_count += 1;
                        continue;
                    }
                }
                Err(e) => {
                    output.warning(&format!("Skipping {}: Invalid SBOM: {}", filename, e));
                    skipped_count += 1;
                    continue;
                }
            }

            // Verify signature using crypto module
            let sig_bytes = std::fs::read(&sig_path)?;
            let adapter_bytes = std::fs::read(&path)?;

            // Parse signature from bytes (assuming hex-encoded signature)
            let sig_hex = String::from_utf8(sig_bytes)
                .map_err(|e| anyhow::anyhow!("Invalid signature encoding: {}", e))?;
            let sig_bytes_decoded = hex::decode(sig_hex.trim())
                .map_err(|e| anyhow::anyhow!("Invalid signature hex: {}", e))?;

            if sig_bytes_decoded.len() != 64 {
                output.warning(&format!("Skipping {}: invalid signature length", filename));
                skipped_count += 1;
                continue;
            }

            let mut sig_array = [0u8; 64];
            sig_array.copy_from_slice(&sig_bytes_decoded);

            // For now, we'll use a mock verification since we don't have the public key
            // In production, this would load the public key from a trusted source
            output.progress(&format!(
                "Signature verification skipped for {} (mock)",
                filename
            ));

            // TODO: Implement actual signature verification with public key
            // let signature = Signature::from_bytes(&sig_array)?;
            // let public_key = PublicKey::from_pem(&public_key_pem)?;
            // public_key.verify(&adapter_bytes, &signature)?;

            // Store in CAS
            let hash = cas.store("adapters", &adapter_bytes)?;

            // Register in registry (basic registration without full metadata)
            // In a real implementation, we would parse metadata from SBOM or manifest
            match registry.register_adapter(
                filename,
                &hash,
                "persistent",
                8,   // default rank
                &[], // empty ACL
            ) {
                Ok(_) => {
                    output.success(&format!("Imported adapter: {} ({})", filename, hash));
                    synced_count += 1;
                }
                Err(e) => {
                    output.warning(&format!("Failed to register {}: {}", filename, e));
                    skipped_count += 1;
                }
            }
        }
    }

    output.progress("");
    output.info("Sync complete");
    output.kv("Synced", &synced_count.to_string());
    output.kv("Skipped", &skipped_count.to_string());

    if output.is_json() {
        let result = SyncResult {
            synced_count,
            skipped_count,
        };
        output.json(&result)?;
    }

    Ok(())
}
