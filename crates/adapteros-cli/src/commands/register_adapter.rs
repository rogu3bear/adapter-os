//! Register LoRA adapter command (canonical .aos path)

#![allow(deprecated)]

use crate::output::OutputWriter;
use adapteros_aos::{compute_scope_hash, open_aos, BackendTag};
use adapteros_core::{AosError, B3Hash};
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;
use adapteros_single_file_adapter::SingleFileAdapterValidator;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Error message for rejecting legacy AOS bundles
const LEGACY_REJECTION_MSG: &str = "Legacy AOS bundle format is no longer supported. Please regenerate using the current toolchain.";

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

    // Ensure schema compatibility for legacy/in-memory DBs.
    ensure_adapter_schema(db).await?;

    let file_view = open_aos(&data)?;
    let manifest: Value = serde_json::from_slice(file_view.manifest_bytes).map_err(|e| {
        AosError::Validation(format!(
            "Failed to parse manifest JSON from {}: {}",
            canonical_path.display(),
            e
        ))
    })?;

    let metadata_obj = manifest.get("metadata").and_then(|m| m.as_object());
    let scope_path = metadata_obj
        .and_then(|m| m.get("scope_path"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AosError::Validation("Missing scope_path in manifest metadata".to_string())
        })?;
    if scope_path.trim().is_empty() {
        return Err(AosError::Validation("scope_path cannot be empty".to_string()).into());
    }
    let scope_hash = compute_scope_hash(scope_path);

    let domain = metadata_obj
        .and_then(|m| m.get("domain").and_then(|v| v.as_str()))
        .unwrap_or("unspecified")
        .to_string();
    let group = metadata_obj
        .and_then(|m| m.get("group").and_then(|v| v.as_str()))
        .unwrap_or("unspecified")
        .to_string();
    let _operation = metadata_obj
        .and_then(|m| m.get("operation").and_then(|v| v.as_str()))
        .unwrap_or("unspecified")
        .to_string();
    let effective_scope = manifest
        .get("scope")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            metadata_obj
                .and_then(|m| m.get("scope").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
        });
    let metadata_json = metadata_obj
        .map(|m| {
            serde_json::to_string(m).map_err(|e| {
                AosError::Validation(format!("Failed to serialize manifest metadata: {}", e))
            })
        })
        .transpose()?;

    let canonical_segment = file_view
        .segments
        .iter()
        .filter(|seg| seg.backend_tag == BackendTag::Canonical)
        .find(|seg| seg.scope_hash == scope_hash)
        .or_else(|| {
            file_view
                .segments
                .iter()
                .find(|seg| seg.backend_tag == BackendTag::Canonical)
        })
        .ok_or_else(|| AosError::Validation("Missing canonical segment".to_string()))?;

    let weights_data = canonical_segment.payload;
    let manifest_bytes = file_view.manifest_bytes;

    let manifest_version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let manifest_base_model = manifest
        .get("base_model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Track provenance for legacy bundles that lack base_model.
    let mut provenance_json: Option<String> = None;

    if let Some(manifest_model) = &manifest_base_model {
        if manifest_model != &req.base_model_id {
            return Err(AosError::Validation(
                "Bundle base_model does not match requested base model".to_string(),
            )
            .into());
        }
    } else {
        warn!(
            adapter_id = %req.adapter_id,
            tenant_id = %req.tenant_id,
            "Manifest missing base_model; accepting as legacy bundle"
        );
        provenance_json = Some(
            serde_json::json!({
                "legacy_base_model": true,
                "reason": "manifest_missing_base_model"
            })
            .to_string(),
        );
    }

    let validation = SingleFileAdapterValidator::validate(&canonical_path)
        .await
        .map_err(|e| {
            AosError::Validation(format!(
                "Adapter validation failed for {}: {}",
                canonical_path.display(),
                e
            ))
        })?;

    // Check for legacy format errors in validation
    let has_legacy_error = validation.errors.iter().any(|msg| {
        let lower = msg.to_ascii_lowercase();
        lower.contains("legacy aos") || lower.contains("invalid aos magic")
    });

    // Debug: log all validation errors
    for (i, err) in validation.errors.iter().enumerate() {
        tracing::debug!(error_idx = i, error_msg = %err, "Validation error");
    }

    if has_legacy_error {
        warn!(
            code = "LEGACY_AOS_REJECTED",
            adapter_id = %req.adapter_id,
            tenant_id = %req.tenant_id,
            path = %canonical_path.display(),
            "Rejecting legacy AOS bundle detected during validation"
        );
        return Err(AosError::Validation(LEGACY_REJECTION_MSG.to_string()).into());
    }

    if !validation.is_valid {
        let detail = if validation.errors.is_empty() {
            "adapter failed validation".to_string()
        } else {
            validation.errors.join("; ")
        };
        return Err(AosError::Validation(format!("Adapter validation failed: {}", detail)).into());
    }

    let weights_hash = blake3::hash(weights_data).to_hex().to_string();
    let aos_file_hash = blake3::hash(&data).to_hex().to_string();
    let content_hash_b3 = B3Hash::hash_multi(&[manifest_bytes, weights_data])
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
        .scope(effective_scope.as_deref().unwrap_or("global"))
        .domain(Some(domain))
        .purpose(Some(group))
        .metadata_json(metadata_json.clone())
        .aos_file_path(Some(canonical_path.to_string_lossy().to_string()))
        .aos_file_hash(Some(&aos_file_hash))
        .base_model_id(Some(&req.base_model_id))
        .manifest_schema_version(manifest_version)
        .content_hash_b3(Some(&content_hash_b3))
        .revision(req.revision.clone())
        .provenance_json(provenance_json)
        .build()
        .map_err(|e| AosError::Validation(format!("Failed to build registration params: {}", e)))?;

    // Upsert-like: if the adapter exists with same hash, treat as no-op; otherwise error.
    if let Some(existing) = db
        .get_adapter_for_tenant(&req.tenant_id, &req.adapter_id)
        .await?
    {
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

async fn ensure_adapter_schema(db: &Db) -> Result<()> {
    let has_lora_strength: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('adapters') WHERE name = 'lora_strength'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap_or(1);

    if has_lora_strength == 0 {
        sqlx::query("ALTER TABLE adapters ADD COLUMN lora_strength REAL")
            .execute(db.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to add lora_strength column: {}", e))
            })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_aos::{AosWriter, BackendTag};
    use adapteros_db::models::ModelRegistrationBuilder;
    use adapteros_db::Db;
    use adapteros_platform::common::PlatformUtils;
    use serde_json::json;
    use serial_test::serial;
    use std::fs::File;
    use std::io::Write;
    use uuid::Uuid;

    fn build_aos_file(manifest: &serde_json::Value, weights: &[u8], magic: [u8; 4]) -> Vec<u8> {
        let mut writer = AosWriter::new();
        writer
            .add_segment(BackendTag::Canonical, None, weights)
            .expect("add canonical segment");
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        let temp = tempfile::NamedTempFile::new_in(&root).expect("tempfile");
        writer
            .write_archive(temp.path(), manifest)
            .expect("write aos");
        let mut bytes = std::fs::read(temp.path()).expect("read aos");
        bytes[0..4].copy_from_slice(&magic);
        bytes
    }

    fn new_test_tempdir() -> tempfile::TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        tempfile::tempdir_in(&root).expect("tempdir")
    }

    /// Create minimal valid safetensors payload with dummy LoRA weights
    fn create_dummy_lora_safetensors() -> Vec<u8> {
        // Create minimal safetensors format:
        // 8 bytes: header_size (u64 little-endian)
        // header_size bytes: JSON header
        // remaining: tensor data

        // Dummy LoRA weights: rank=4, in=16, out=16
        // Note: Using F16 (half precision) as the loader expects it
        let lora_a: Vec<u16> = (0..64).map(|i| half::f16::from_f32((i as f32) * 0.01).to_bits()).collect();
        let lora_b: Vec<u16> = (0..64).map(|i| half::f16::from_f32((i as f32) * -0.01).to_bits()).collect();

        // Tensor names must be "lora_a" and "lora_b" (or "lora.a" / "lora.b")
        let header = json!({
            "lora_a": {
                "dtype": "F16",
                "shape": [4, 16],
                "data_offsets": [0, 128]
            },
            "lora_b": {
                "dtype": "F16",
                "shape": [16, 4],
                "data_offsets": [128, 256]
            }
        });

        let header_json = serde_json::to_string(&header).unwrap();
        let header_bytes = header_json.as_bytes();
        let header_size = header_bytes.len() as u64;

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&header_size.to_le_bytes());
        buffer.extend_from_slice(header_bytes);

        // Write tensor data (F16 = 2 bytes each)
        for val in &lora_a {
            buffer.extend_from_slice(&val.to_le_bytes());
        }
        for val in &lora_b {
            buffer.extend_from_slice(&val.to_le_bytes());
        }

        buffer
    }

    #[tokio::test]
    #[serial]
    async fn aos_magic_bundle_is_accepted() {
        // Migration signatures are not available in CI/unit tests; skip verification.
        std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

        // Use an in-memory SQLite instance and apply migrations explicitly.
        let db = Db::connect(":memory:")
            .await
            .expect("connect in-memory db for test");
        db.migrate()
            .await
            .expect("apply migrations to in-memory db");

        // Create tenant first, then model with tenant_id
        // This order ensures the model is associated with the tenant for trigger validation
        let tenant_id = db
            .create_tenant("Test Tenant", false)
            .await
            .expect("insert tenant");

        let model_name = format!("test-model-{}", Uuid::now_v7());
        let model_id = db
            .register_model(
                ModelRegistrationBuilder::new()
                    .name(&model_name)
                    .hash_b3("model-hash")
                    .config_hash_b3("config-hash")
                    .tokenizer_hash_b3("tok-hash")
                    .tokenizer_cfg_hash_b3("tok-cfg-hash")
                    .build()
                    .expect("model params"),
            )
            .await
            .expect("insert model");

        // Associate model with tenant (required by trg_adapters_base_model_tenant_check trigger)
        sqlx::query("UPDATE models SET tenant_id = ? WHERE id = ?")
            .bind(&tenant_id)
            .bind(&model_id)
            .execute(db.pool())
            .await
            .expect("update model tenant_id");
        let tmp = new_test_tempdir();
        let aos_path = tmp.path().join("valid.aos");

        let manifest = json!({
            "adapter_id": "valid-adapter",
            "name": "Valid Adapter",
            "version": "1.0.0",
            "metadata": { "scope_path": "test/domain/scope/op" }
            // base_model intentionally omitted
        });
        let weights = create_dummy_lora_safetensors();

        // AOS\0 is now the current format magic
        let aos_bytes = build_aos_file(&manifest, &weights, *b"AOS\x00");
        let mut file = File::create(&aos_path).expect("create aos");
        file.write_all(&aos_bytes).expect("write aos");

        let req = RegisterAosRequest {
            adapter_id: "valid-adapter".to_string(),
            aos_path: aos_path.clone(),
            tenant_id: tenant_id.clone(),
            base_model_id: model_id.clone(),
            tier: "warm".to_string(),
            rank: 4,
            name: None,
            revision: None,
        };

        // With AOS\0 as the current format, registration should succeed
        register_aos_with_db(&db, req)
            .await
            .expect("AOS magic should be accepted");

        let adapter = db.get_adapter("valid-adapter").await.expect("query ok");
        assert!(
            adapter.is_some(),
            "adapter should be registered when using AOS magic"
        );
    }
}
