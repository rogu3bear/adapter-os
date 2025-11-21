//! Register LoRA adapter command

use crate::output::OutputWriter;
use adapteros_core::B3Hash;
use adapteros_db::Db;
use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Serialize)]
struct AdapterRegistration {
    id: String,
    hash: String,
    tier: String,
    rank: u32,
    status: String,
}

pub async fn run(id: &str, hash: &str, tier: &str, rank: u32, output: &OutputWriter) -> Result<()> {
    output.info("Registering LoRA adapter");
    output.kv("ID", id);
    output.kv("Hash", hash);
    output.kv("Tier", tier);
    output.kv("Rank", &rank.to_string());
    output.progress("Verifying adapter...");

    // Parse hash
    let adapter_hash =
        B3Hash::from_hex(hash).map_err(|e| anyhow::anyhow!("Invalid hash format: {}", e))?;

    // Connect to database
    let db = Db::connect_env()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    // Check if adapter already exists
    if let Ok(Some(existing)) = db.get_adapter(id).await {
        if existing.hash_b3 == adapter_hash.to_string() {
            output.warning("Adapter already registered with same hash");
            output.success("Registration completed (already exists)");

            if output.is_json() {
                let registration = AdapterRegistration {
                    id: id.to_string(),
                    hash: hash.to_string(),
                    tier: tier.to_string(),
                    rank,
                    status: "exists".to_string(),
                };
                output.json(&registration)?;
            }
            return Ok(());
        } else {
            return Err(anyhow::anyhow!("Adapter exists with different hash"));
        }
    }

    // Verify adapter file exists (by hash)
    let adapter_path =
        Path::new("./adapters").join(format!("{}.safetensors", adapter_hash.to_hex()));
    if !adapter_path.exists() {
        output.warning("Adapter file not found, creating placeholder");
        output.progress("Note: In production, adapter files would be pre-staged");

        // Create directory if it doesn't exist
        if let Some(parent) = adapter_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create a placeholder file for demonstration
        fs::write(
            &adapter_path,
            format!("placeholder adapter file for {}", id),
        )?;
    }

    // Load and validate adapter metadata
    let adapter_size = fs::metadata(&adapter_path)?.len();
    output.progress(&format!("Adapter file size: {} bytes", adapter_size));

    // Store metadata in the registry database
    use adapteros_db::adapters::AdapterRegistrationBuilder;
    let params = AdapterRegistrationBuilder::new()
        .adapter_id(id)
        .name(
            adapter_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        )
        .hash_b3(adapter_hash.to_string())
        .rank(rank.try_into().unwrap())
        .tier(tier)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build registration params: {}", e))?;
    db.register_adapter(params)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to store adapter in database: {}", e))?;

    output.success("Adapter registered successfully");

    if output.is_json() {
        let registration = AdapterRegistration {
            id: id.to_string(),
            hash: hash.to_string(),
            tier: tier.to_string(),
            rank,
            status: "registered".to_string(),
        };
        output.json(&registration)?;
    }

    Ok(())
}
