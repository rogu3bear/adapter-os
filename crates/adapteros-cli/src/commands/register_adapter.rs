//! Register LoRA adapter command (canonical .aos path)

use crate::output::OutputWriter;
use adapteros_aos::{compute_scope_hash, open_aos, BackendTag, AOS_MAGIC};
use adapteros_core::{AosError, B3Hash};
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;
use adapteros_single_file_adapter::SingleFileAdapterValidator;
use anyhow::Result;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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

static LEGACY_BUNDLES_SEEN: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));
static LEGACY_BUNDLES_ACCEPTED: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));

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
    let mut data = fs::read(&canonical_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read .aos file {}: {}",
            canonical_path.display(),
            e
        ))
    })?;

    const LEGACY_REJECTION_MSG: &str =
        "Unsupported legacy AOS 1.x bundle; please repackage as AOS2";

    // TODO(v0.15.0): remove AOS_ALLOW_LEGACY_AOS after legacy bundles are repackaged (temporary
    // escape hatch). Tracked in docs/issues/AOS_ALLOW_LEGACY_AOS_REMOVAL.md.
    let allow_legacy = std::env::var("AOS_ALLOW_LEGACY_AOS")
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    let mut legacy_seen_count: Option<u64> = None;
    let mut legacy_accepted_recorded = false;

    // Ensure schema compatibility for legacy/in-memory DBs.
    ensure_adapter_schema(db).await?;

    let legacy_magic_header = data.len() >= 4 && &data[0..4] == b"AOS\x00";
    if legacy_magic_header {
        let seen = LEGACY_BUNDLES_SEEN.fetch_add(1, Ordering::Relaxed) + 1;
        legacy_seen_count = Some(seen);
        if !allow_legacy {
            warn!(
                code = "LEGACY_AOS_REJECTED",
                adapter_id = %req.adapter_id,
                tenant_id = %req.tenant_id,
                path = %canonical_path.display(),
                legacy_seen = seen,
                "Rejecting legacy AOS bundle (AOS_ALLOW_LEGACY_AOS not enabled)"
            );
            return Err(AosError::Validation(LEGACY_REJECTION_MSG.to_string()).into());
        }

        let accepted = LEGACY_BUNDLES_ACCEPTED.fetch_add(1, Ordering::Relaxed) + 1;
        warn!(
            code = "LEGACY_AOS_ACCEPTED",
            adapter_id = %req.adapter_id,
            tenant_id = %req.tenant_id,
            path = %canonical_path.display(),
            legacy_seen = seen,
            legacy_accepted = accepted,
            "DEPRECATED: legacy AOS bundle accepted; upgrade required"
        );
        legacy_accepted_recorded = true;
        // Patch legacy magic to current magic so downstream parser succeeds.
        data[0..4].copy_from_slice(&AOS_MAGIC);
    }

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

    let legacy_magic_error_from_validation = validation
        .errors
        .iter()
        .any(|msg| msg.to_ascii_lowercase().contains("invalid aos magic"));
    let mut legacy_magic_error = legacy_magic_error_from_validation || legacy_magic_header;

    let legacy_marker = |msg: &str| {
        let lower = msg.to_ascii_lowercase();
        lower.contains("legacy aos") || lower.contains("invalid aos magic")
    };

    let mut legacy_validation_error =
        validation.errors.iter().any(|msg| legacy_marker(msg)) || legacy_magic_error;
    let mut has_non_legacy_errors = validation.errors.iter().any(|msg| !legacy_marker(msg));

    if legacy_validation_error {
        let seen = legacy_seen_count
            .unwrap_or_else(|| LEGACY_BUNDLES_SEEN.fetch_add(1, Ordering::Relaxed) + 1);
        legacy_seen_count = Some(seen);
        warn!(
            code = "LEGACY_AOS_SEEN",
            adapter_id = %req.adapter_id,
            tenant_id = %req.tenant_id,
            path = %canonical_path.display(),
            legacy_seen = seen,
            allow_legacy,
            "Legacy AOS bundle detected during registration"
        );
    }

    if legacy_validation_error && !allow_legacy {
        warn!(
            code = "LEGACY_AOS_REJECTED",
            adapter_id = %req.adapter_id,
            tenant_id = %req.tenant_id,
            path = %canonical_path.display(),
            legacy_seen = legacy_seen_count.unwrap_or_else(|| LEGACY_BUNDLES_SEEN.load(Ordering::Relaxed)),
            "Rejecting legacy AOS bundle (AOS_ALLOW_LEGACY_AOS not enabled)"
        );
        return Err(AosError::Validation(LEGACY_REJECTION_MSG.to_string()).into());
    }

    if legacy_validation_error && allow_legacy && !legacy_accepted_recorded {
        let accepted = LEGACY_BUNDLES_ACCEPTED.fetch_add(1, Ordering::Relaxed) + 1;
        warn!(
            code = "LEGACY_AOS_ACCEPTED",
            adapter_id = %req.adapter_id,
            tenant_id = %req.tenant_id,
            path = %canonical_path.display(),
            legacy_seen = legacy_seen_count.unwrap_or_else(|| LEGACY_BUNDLES_SEEN.load(Ordering::Relaxed)),
            legacy_accepted = accepted,
            "DEPRECATED: legacy AOS bundle accepted; upgrade required"
        );
        legacy_accepted_recorded = true;
    }

    let validation_has_only_legacy = legacy_validation_error && !has_non_legacy_errors;

    if legacy_magic_error && !allow_legacy {
        warn!(
            code = "LEGACY_AOS_REJECTED",
            adapter_id = %req.adapter_id,
            tenant_id = %req.tenant_id,
            path = %canonical_path.display(),
            legacy_seen = legacy_seen_count.unwrap_or_else(|| LEGACY_BUNDLES_SEEN.load(Ordering::Relaxed)),
            "Rejecting legacy AOS bundle (AOS_ALLOW_LEGACY_AOS not enabled)"
        );
        return Err(AosError::Validation(LEGACY_REJECTION_MSG.to_string()).into());
    }

    let validation_passed = if legacy_magic_error && allow_legacy {
        true
    } else {
        validation.is_valid || (validation_has_only_legacy && allow_legacy)
    };

    if !validation_passed {
        let detail = if validation.errors.is_empty() {
            "adapter failed validation".to_string()
        } else {
            validation.errors.join("; ")
        };
        let detail_lower = detail.to_ascii_lowercase();
        legacy_magic_error = legacy_magic_error || detail_lower.contains("invalid aos magic");

        if legacy_magic_error {
            let seen = LEGACY_BUNDLES_SEEN.fetch_add(1, Ordering::Relaxed) + 1;
            if !allow_legacy {
                warn!(
                    code = "LEGACY_AOS_REJECTED",
                    adapter_id = %req.adapter_id,
                    tenant_id = %req.tenant_id,
                    path = %canonical_path.display(),
                    legacy_seen = seen,
                    "Rejecting legacy AOS bundle (AOS_ALLOW_LEGACY_AOS not enabled)"
                );
                return Err(AosError::Validation(LEGACY_REJECTION_MSG.to_string()).into());
            }

            if !legacy_accepted_recorded {
                let accepted = LEGACY_BUNDLES_ACCEPTED.fetch_add(1, Ordering::Relaxed) + 1;
                warn!(
                    code = "LEGACY_AOS_ACCEPTED",
                    adapter_id = %req.adapter_id,
                    tenant_id = %req.tenant_id,
                    path = %canonical_path.display(),
                    legacy_seen = seen,
                    legacy_accepted = accepted,
                    "DEPRECATED: legacy AOS bundle accepted; upgrade required"
                );
                legacy_accepted_recorded = true;
            }
            // Allow continuation for legacy magic when explicitly permitted.
        } else {
            return Err(
                AosError::Validation(format!("Adapter validation failed: {}", detail)).into(),
            );
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

async fn ensure_adapter_schema(db: &Db) -> Result<()> {
    let has_lora_strength: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('adapters') WHERE name = 'lora_strength'",
    )
    .fetch_one(&*db.pool())
    .await
    .unwrap_or(1);

    if has_lora_strength == 0 {
        sqlx::query("ALTER TABLE adapters ADD COLUMN lora_strength REAL")
            .execute(&*db.pool())
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
    use serde_json::json;
    use serial_test::serial;
    use std::fs::File;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing::subscriber::DefaultGuard;
    use uuid::Uuid;

    fn build_aos_file(manifest: &serde_json::Value, weights: &[u8], magic: [u8; 4]) -> Vec<u8> {
        let mut writer = AosWriter::new();
        writer
            .add_segment(BackendTag::Canonical, None, weights)
            .expect("add canonical segment");
        let temp = tempfile::NamedTempFile::new().expect("tempfile");
        writer
            .write_archive(temp.path(), manifest)
            .expect("write aos");
        let mut bytes = std::fs::read(temp.path()).expect("read aos");
        bytes[0..4].copy_from_slice(&magic);
        bytes
    }

    #[derive(Clone)]
    struct BufferingWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for BufferingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut guard = self.0.lock().unwrap();
            guard.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn install_buffering_subscriber(buffer: Arc<Mutex<Vec<u8>>>) -> DefaultGuard {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || BufferingWriter(buffer.clone()))
            .with_max_level(tracing::Level::WARN)
            .with_ansi(false)
            .finish();
        tracing::subscriber::set_default(subscriber)
    }

    fn reset_legacy_counters() {
        LEGACY_BUNDLES_SEEN.store(0, Ordering::Relaxed);
        LEGACY_BUNDLES_ACCEPTED.store(0, Ordering::Relaxed);
    }

    fn legacy_counter_snapshot() -> (u64, u64) {
        (
            LEGACY_BUNDLES_SEEN.load(Ordering::Relaxed),
            LEGACY_BUNDLES_ACCEPTED.load(Ordering::Relaxed),
        )
    }

    #[tokio::test]
    #[serial]
    async fn legacy_magic_bundle_is_rejected_when_env_unset() {
        // Migration signatures are not available in CI/unit tests; skip verification.
        std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
        std::env::remove_var("AOS_ALLOW_LEGACY_AOS");
        reset_legacy_counters();
        let log_buffer = Arc::new(Mutex::new(Vec::new()));
        let _guard = install_buffering_subscriber(log_buffer.clone());

        // Use an in-memory SQLite instance and apply migrations explicitly.
        let db = Db::connect(":memory:")
            .await
            .expect("connect in-memory db for test");
        db.migrate()
            .await
            .expect("apply migrations to in-memory db");

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
        let tenant_id = db
            .create_tenant("Test Tenant", false)
            .await
            .expect("insert tenant");
        let tmp = tempfile::tempdir().expect("tempdir");
        let aos_path = tmp.path().join("legacy.aos");

        let manifest = json!({
            "adapter_id": "legacy-adapter",
            "name": "Legacy Adapter",
            "version": "1.0.0",
            "metadata": { "scope_path": "test/domain/scope/op" }
            // base_model intentionally omitted
        });
        let weights = vec![0u8; 16];

        let aos_bytes = build_aos_file(&manifest, &weights, *b"AOS\x00");
        let mut file = File::create(&aos_path).expect("create aos");
        file.write_all(&aos_bytes).expect("write aos");

        let req = RegisterAosRequest {
            adapter_id: "legacy-adapter".to_string(),
            aos_path: aos_path.clone(),
            tenant_id: tenant_id.clone(),
            base_model_id: model_id.clone(),
            tier: "warm".to_string(),
            rank: 4,
            name: None,
            revision: None,
        };

        let err = register_aos_with_db(&db, req)
            .await
            .expect_err("legacy magic should be rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("Unsupported legacy AOS 1.x bundle; please repackage as AOS2"),
            "error message should instruct to repackage; got {msg}"
        );

        let adapter = db.get_adapter("legacy-adapter").await.expect("query ok");
        assert!(
            adapter.is_none(),
            "legacy adapter should not be registered when legacy magic is present"
        );

        let (seen, accepted) = legacy_counter_snapshot();
        assert_eq!(seen, 1, "legacy detection counter should increment");
        assert_eq!(
            accepted, 0,
            "accept counter should not increment on rejection"
        );
        let logs = String::from_utf8(log_buffer.lock().unwrap().clone()).unwrap();
        assert!(
            logs.contains("LEGACY_AOS_REJECTED"),
            "rejection log should be emitted for legacy bundles"
        );
    }

    #[tokio::test]
    #[serial]
    async fn legacy_magic_bundle_is_accepted_when_env_enabled() {
        std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
        std::env::set_var("AOS_ALLOW_LEGACY_AOS", "1");
        reset_legacy_counters();
        let log_buffer = Arc::new(Mutex::new(Vec::new()));
        let _guard = install_buffering_subscriber(log_buffer.clone());

        // Use an in-memory SQLite instance and apply migrations explicitly.
        let db = Db::connect(":memory:")
            .await
            .expect("connect in-memory db for test");
        db.migrate()
            .await
            .expect("apply migrations to in-memory db");

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
        let tenant_id = db
            .create_tenant("Test Tenant", false)
            .await
            .expect("insert tenant");
        let tmp = tempfile::tempdir().expect("tempdir");
        let aos_path = tmp.path().join("legacy-accepted.aos");

        let manifest = json!({
            "adapter_id": "legacy-adapter",
            "name": "Legacy Adapter",
            "version": "1.0.0",
            "metadata": { "scope_path": "test/domain/scope/op" }
            // base_model intentionally omitted
        });
        let weights = vec![0u8; 16];

        let aos_bytes = build_aos_file(&manifest, &weights, *b"AOS\x00");
        let mut file = File::create(&aos_path).expect("create aos");
        file.write_all(&aos_bytes).expect("write aos");

        let req = RegisterAosRequest {
            adapter_id: "legacy-adapter".to_string(),
            aos_path: aos_path.clone(),
            tenant_id: tenant_id.clone(),
            base_model_id: model_id.clone(),
            tier: "warm".to_string(),
            rank: 4,
            name: None,
            revision: None,
        };

        let result = register_aos_with_db(&db, req)
            .await
            .expect("legacy magic should be accepted when env is enabled");

        assert_eq!(result.status, "registered");
        let adapter = db
            .get_adapter("legacy-adapter")
            .await
            .expect("query ok")
            .expect("adapter should be present after acceptance");
        assert_eq!(adapter.hash_b3, result.weights_hash);

        let (seen, accepted) = legacy_counter_snapshot();
        assert_eq!(seen, 1, "legacy detection counter should increment");
        assert_eq!(
            accepted, 1,
            "accept counter should increment when env allows"
        );
        let logs = String::from_utf8(log_buffer.lock().unwrap().clone()).unwrap();
        assert!(
            logs.contains("LEGACY_AOS_ACCEPTED"),
            "acceptance log should include LEGACY_AOS_ACCEPTED code"
        );
        assert!(
            logs.contains("DEPRECATED: legacy AOS bundle accepted; upgrade required"),
            "acceptance log should carry explicit deprecation warning"
        );
        std::env::remove_var("AOS_ALLOW_LEGACY_AOS");
    }
}
