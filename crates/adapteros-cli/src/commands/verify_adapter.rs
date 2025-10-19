//! Verify packaged adapter readiness

use crate::output::OutputWriter;
use adapteros_crypto::{PublicKey, Signature};
use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
struct VerifyReport {
    adapter_id: String,
    adapters_root: String,
    weights_hash_b3: String,
    manifest_hash_b3: String,
    manifest_hash_matches: bool,
    signature_valid: bool,
    ready: bool,
}

pub async fn run(adapters_root: PathBuf, adapter_id: String, output: &OutputWriter) -> Result<()> {
    let adapter_dir = adapters_root.join(&adapter_id);
    let weights_path = adapter_dir.join("weights.safetensors");
    let manifest_path = adapter_dir.join("manifest.json");
    let sig_path = adapter_dir.join("signature.sig");
    let pubkey_path = adapter_dir.join("public_key.pem");

    output.info("Verifying packaged adapter");
    output.kv("Adapter ID", &adapter_id);
    output.kv("Path", &adapter_dir.display().to_string());

    // Read files
    let weights = tokio::fs::read(&weights_path).await?;
    let manifest = tokio::fs::read(&manifest_path).await?;
    let sig_bytes = tokio::fs::read(&sig_path).await?;
    let pubkey_hex = tokio::fs::read_to_string(&pubkey_path).await?;

    // Compute BLAKE3 over weights
    let weights_hash = blake3::hash(&weights).to_hex().to_string();

    // Parse manifest weights hash
    #[derive(serde::Deserialize)]
    struct Manifest {
        weights_hash: String,
    }
    let manifest_json: Manifest = serde_json::from_slice(&manifest)?;
    let manifest_hash = manifest_json.weights_hash;
    let matches = manifest_hash == weights_hash;

    // Verify signature over manifest
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid signature length"))?;
    let signature = Signature::from_bytes(&sig_arr)?;
    let pubkey_bytes = hex::decode(pubkey_hex.trim())?;
    let pubkey_arr: [u8; 32] = pubkey_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid public key length"))?;
    let public_key = PublicKey::from_bytes(&pubkey_arr)?;
    let sig_ok = public_key.verify(&manifest, &signature).is_ok();

    let ready = matches && sig_ok;

    if output.is_json() {
        let report = VerifyReport {
            adapter_id,
            adapters_root: adapters_root.display().to_string(),
            weights_hash_b3: weights_hash.clone(),
            manifest_hash_b3: manifest_hash.clone(),
            manifest_hash_matches: matches,
            signature_valid: sig_ok,
            ready,
        };
        output.json(&report)?;
    } else {
        output.kv("Weights B3", &weights_hash);
        output.kv("Manifest B3", &manifest_hash);
        output.kv("Hash matches", &matches.to_string());
        output.kv("Signature valid", &sig_ok.to_string());
        if ready {
            output.success("Adapter is ready for registration and runtime")
        } else {
            output.warning("Adapter is not ready")
        }
    }

    Ok(())
}
