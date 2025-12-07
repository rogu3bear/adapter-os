//! Register LoRA adapter command (canonical .aos path)

use crate::output::OutputWriter;
use adapteros_aos::writer::{AOS_MAGIC, HEADER_SIZE};
use adapteros_core::{AosError, B3Hash};
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAosRequest {
    pub adapter_id: String,
    pub aos_path: PathBuf,
    pub tenant_id: String,
    pub base_model_id: String,
    pub tier: String,
    pub rank: u32,
    pub name: Option<String>,
    pub revision: Option<String>,
}

#[derive(Serialize)]
struct AdapterRegistration {
    id: String,
    hash: String,
    tier: String,
    rank: u32,
    status: String,
    aos_path: String,
}

pub async fn run(
    adapter_id: &str,
    aos_path: &Path,
    tenant_id: &str,
    base_model_id: &str,
    tier: &str,
    rank: u32,
    output: &OutputWriter,
) -> Result<()> {
    output.info("Registering LoRA adapter from .aos bundle");
    output.kv("ID", adapter_id);
    output.kv("AOS", &aos_path.display().to_string());
    output.kv("Tier", tier);
    output.kv("Rank", &rank.to_string());
    output.kv("Tenant", tenant_id);
    output.kv("Base Model", base_model_id);
    output.progress("Verifying bundle...");

    let db = Db::connect_env()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    let request = RegisterAosRequest {
        adapter_id: adapter_id.to_string(),
        aos_path: aos_path.to_path_buf(),
        tenant_id: tenant_id.to_string(),
        base_model_id: base_model_id.to_string(),
        tier: tier.to_string(),
        rank,
        name: None,
        revision: None,
    };

    let registration = register_aos_with_db(&db, request.clone())
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    output.success("Adapter registered successfully");

    if output.is_json() {
        let registration = AdapterRegistration {
            id: adapter_id.to_string(),
            hash: registration.weights_hash.clone(),
            tier: tier.to_string(),
            rank,
            status: registration.status,
            aos_path: registration.aos_path.clone(),
        };
        output.json(&registration)?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct RegistrationResult {
    pub weights_hash: String,
    pub aos_path: String,
    pub status: String,
}

pub async fn register_aos_with_db(db: &Db, req: RegisterAosRequest) -> Result<RegistrationResult> {
    let canonical_path = fs::canonicalize(&req.aos_path).unwrap_or(req.aos_path.clone());
    let data = fs::read(&canonical_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read .aos file {}: {}",
            canonical_path.display(),
            e
        ))
    })?;

    if data.len() < HEADER_SIZE {
        return Err(
            AosError::Validation(format!("AOS file too small ({} bytes)", data.len())).into(),
        );
    }

    if &data[0..4] != &AOS_MAGIC {
        return Err(AosError::Validation(format!(
            "Invalid AOS magic bytes in {}",
            canonical_path.display()
        ))
        .into());
    }

    let weights_offset = u64::from_le_bytes(data[8..16].try_into().unwrap()) as usize;
    let weights_size = u64::from_le_bytes(data[16..24].try_into().unwrap()) as usize;
    let manifest_offset = u64::from_le_bytes(data[24..32].try_into().unwrap()) as usize;
    let manifest_size = u64::from_le_bytes(data[32..40].try_into().unwrap()) as usize;

    if weights_offset + weights_size > data.len() || manifest_offset + manifest_size > data.len() {
        return Err(AosError::Validation(format!(
            "AOS header ranges exceed file size: {} bytes",
            data.len()
        ))
        .into());
    }

    let weights_data = &data[weights_offset..weights_offset + weights_size];
    let manifest_bytes = &data[manifest_offset..manifest_offset + manifest_size];

    let manifest: Value = serde_json::from_slice(manifest_bytes).map_err(|e| {
        AosError::Validation(format!(
            "Failed to parse manifest JSON from {}: {}",
            canonical_path.display(),
            e
        ))
    })?;

    let manifest_version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let manifest_base_model = manifest
        .get("base_model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if let Some(manifest_model) = &manifest_base_model {
        if manifest_model != &req.base_model_id {
            return Err(AosError::Validation(format!(
                "Base model mismatch: manifest={}, provided={}",
                manifest_model, req.base_model_id
            ))
            .into());
        }
    }

    let weights_hash = blake3::hash(weights_data).to_hex().to_string();
    let aos_file_hash = blake3::hash(&data).to_hex().to_string();
    let content_hash_b3 = B3Hash::hash_multi(&[&manifest_bytes[..], weights_data])
        .to_hex()
        .to_string();

    let name = req
        .name
        .clone()
        .or_else(|| {
            manifest
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| req.adapter_id.clone());

    let params = AdapterRegistrationBuilder::new()
        .tenant_id(&req.tenant_id)
        .adapter_id(&req.adapter_id)
        .name(name)
        .hash_b3(&weights_hash)
        .rank(req.rank as i32)
        .tier(&req.tier)
        .aos_file_path(Some(canonical_path.to_string_lossy().to_string()))
        .aos_file_hash(Some(&aos_file_hash))
        .base_model_id(Some(&req.base_model_id))
        .manifest_schema_version(manifest_version)
        .content_hash_b3(Some(&content_hash_b3))
        .revision(req.revision.clone())
        .build()
        .map_err(|e| AosError::Validation(format!("Failed to build registration params: {}", e)))?;

    // Upsert-like: if the adapter exists with same hash, treat as no-op; otherwise error.
    if let Some(existing) = db.get_adapter(&req.adapter_id).await? {
        if existing.hash_b3 != weights_hash {
            return Err(AosError::Validation(format!(
                "Adapter {} already exists with different hash",
                req.adapter_id
            ))
            .into());
        }
        warn!(
            adapter_id = %req.adapter_id,
            "Adapter already registered with same hash; skipping insert"
        );
        return Ok(RegistrationResult {
            weights_hash,
            aos_path: canonical_path.to_string_lossy().to_string(),
            status: "exists".to_string(),
        });
    }

    let _id = db.register_adapter_extended(params).await?;

    info!(
        adapter_id = %req.adapter_id,
        tenant_id = %req.tenant_id,
        aos_path = %canonical_path.display(),
        "Registered adapter via .aos bundle"
    );

    Ok(RegistrationResult {
        weights_hash,
        aos_path: canonical_path.to_string_lossy().to_string(),
        status: "registered".to_string(),
    })
}
