//! Runtime app-managed config store (file + DB mirror).
//!
//! This module provides a canonical runtime settings document persisted to:
//! - `var/config/runtime_config.v1.json` (file)
//! - `runtime_config_snapshots` SQL table (DB mirror)
//!
//! It also applies live-safe fields to `ApiConfig` and emits effective-source metadata.

use crate::model_roots::default_model_discovery_roots;
use crate::state::{ApiConfig, GeneralConfig};
use adapteros_api_types::{
    EffectiveSettingsEntry, EffectiveSettingsResponse, GeneralSettings, ModelSettings,
    PerformanceSettings, SecuritySettings, ServerSettings, SettingsReconcileResponse,
    SystemSettings, UpdateSettingsRequest,
};
use adapteros_core::defaults::DEFAULT_SERVER_URL;
use adapteros_db::{Db, NewRuntimeConfigSnapshot, RuntimeConfigSnapshotRecord};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const RUNTIME_CONFIG_SCHEMA_VERSION: &str = "runtime-config/v1";
const RUNTIME_CONFIG_REL_PATH: &str = "config/runtime_config.v1.json";

const MANAGED_KEYS: &[&str] = &[
    "general.system_name",
    "general.environment",
    "general.api_base_url",
    "models.discovery_roots",
    "models.selected_model_path",
    "models.selected_manifest_path",
    "server.http_port",
    "server.https_port",
    "server.uds_socket_path",
    "server.production_mode",
    "security.jwt_mode",
    "security.token_ttl_seconds",
    "security.require_mfa",
    "security.require_pf_deny",
    "performance.max_adapters",
    "performance.max_workers",
    "performance.memory_threshold_pct",
    "performance.cache_size_mb",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeConfigDocument {
    pub schema_version: String,
    pub version: i64,
    pub source: String,
    pub checksum_b3: String,
    pub updated_by: Option<String>,
    pub updated_at: String,
    pub settings: UpdateSettingsRequest,
    #[serde(default)]
    pub pending_restart_fields: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ApplyReport {
    pub applied_live: Vec<String>,
    pub queued_for_restart: Vec<String>,
    pub rejected: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LoadedRuntimeConfig {
    pub document: RuntimeConfigDocument,
    pub effective_source: String,
}

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
        .unwrap_or(default)
}

pub fn app_config_enabled() -> bool {
    env_flag("AOS_APP_CONFIG_ENABLED", true)
}

fn strict_precedence_enabled() -> bool {
    env_flag("AOS_APP_CONFIG_STRICT_PRECEDENCE", false)
}

fn dualwrite_required() -> bool {
    env_flag("AOS_APP_CONFIG_DUALWRITE_REQUIRED", false)
}

fn runtime_var_root() -> PathBuf {
    std::env::var("AOS_VAR_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var"))
}

fn runtime_config_path() -> PathBuf {
    runtime_var_root().join(RUNTIME_CONFIG_REL_PATH)
}

fn checksum_for_document(doc: &RuntimeConfigDocument) -> String {
    let payload = serde_json::json!({
        "schema_version": doc.schema_version,
        "version": doc.version,
        "source": doc.source,
        "settings": doc.settings,
        "pending_restart_fields": doc.pending_restart_fields,
    });
    blake3::hash(payload.to_string().as_bytes())
        .to_hex()
        .to_string()
}

fn verify_checksum(doc: &RuntimeConfigDocument) -> bool {
    checksum_for_document(doc) == doc.checksum_b3
}

fn parse_timestamp_key(ts: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|d| d.timestamp())
        .unwrap_or(0)
}

fn newer_doc<'a>(
    left: &'a RuntimeConfigDocument,
    right: &'a RuntimeConfigDocument,
) -> &'a RuntimeConfigDocument {
    if left.version != right.version {
        if left.version > right.version {
            left
        } else {
            right
        }
    } else if parse_timestamp_key(&left.updated_at) >= parse_timestamp_key(&right.updated_at) {
        left
    } else {
        right
    }
}

async fn read_file_document() -> Result<Option<RuntimeConfigDocument>, String> {
    let path = runtime_config_path();
    if !path.exists() {
        return Ok(None);
    }

    let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
        format!(
            "failed to read runtime config file {}: {}",
            path.display(),
            e
        )
    })?;
    let doc: RuntimeConfigDocument = serde_json::from_str(&content).map_err(|e| {
        format!(
            "failed to parse runtime config file {}: {}",
            path.display(),
            e
        )
    })?;

    if !verify_checksum(&doc) {
        return Err(format!(
            "runtime config file checksum mismatch at {}",
            path.display()
        ));
    }

    Ok(Some(doc))
}

fn db_record_to_document(
    row: &RuntimeConfigSnapshotRecord,
) -> Result<RuntimeConfigDocument, String> {
    let settings: UpdateSettingsRequest =
        serde_json::from_str(&row.settings_json).map_err(|e| {
            format!(
                "failed to parse runtime config snapshot settings_json: {}",
                e
            )
        })?;
    let pending_restart_fields: Vec<String> = row
        .pending_restart_fields_json
        .as_ref()
        .map(|raw| serde_json::from_str(raw))
        .transpose()
        .map_err(|e| {
            format!(
                "failed to parse runtime config snapshot pending_restart_fields_json: {}",
                e
            )
        })?
        .unwrap_or_default();

    let doc = RuntimeConfigDocument {
        schema_version: row.schema_version.clone(),
        version: row.version,
        source: row.source.clone(),
        checksum_b3: row.checksum_b3.clone(),
        updated_by: row.updated_by.clone(),
        updated_at: row.updated_at.clone(),
        settings,
        pending_restart_fields,
    };

    if !verify_checksum(&doc) {
        return Err("runtime config DB snapshot checksum mismatch".to_string());
    }

    Ok(doc)
}

async fn read_db_document(db: &Db) -> Result<Option<RuntimeConfigDocument>, String> {
    let row = db
        .get_latest_runtime_config_snapshot()
        .await
        .map_err(|e| format!("failed to read runtime config snapshot from DB: {}", e))?;

    match row {
        Some(row) => Ok(Some(db_record_to_document(&row)?)),
        None => Ok(None),
    }
}

async fn write_file_document(doc: &RuntimeConfigDocument) -> Result<(), String> {
    let path = runtime_config_path();
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid runtime config path: {}", path.display()))?;
    tokio::fs::create_dir_all(parent).await.map_err(|e| {
        format!(
            "failed to create runtime config directory {}: {}",
            parent.display(),
            e
        )
    })?;

    let tmp_path = parent.join(format!(
        ".runtime_config.v1.json.{}.tmp",
        chrono::Utc::now().timestamp_millis()
    ));

    let bytes = serde_json::to_vec_pretty(doc)
        .map_err(|e| format!("failed to serialize runtime config document: {}", e))?;

    tokio::fs::write(&tmp_path, bytes).await.map_err(|e| {
        format!(
            "failed to write runtime config temp file {}: {}",
            tmp_path.display(),
            e
        )
    })?;
    tokio::fs::rename(&tmp_path, &path).await.map_err(|e| {
        format!(
            "failed to atomically move runtime config file {}: {}",
            path.display(),
            e
        )
    })?;

    Ok(())
}

async fn write_db_document(db: &Db, doc: &RuntimeConfigDocument) -> Result<(), String> {
    let settings_json = serde_json::to_string(&doc.settings)
        .map_err(|e| format!("failed to serialize runtime settings for DB: {}", e))?;

    db.upsert_runtime_config_snapshot(&NewRuntimeConfigSnapshot {
        version: doc.version,
        schema_version: doc.schema_version.clone(),
        source: doc.source.clone(),
        checksum_b3: doc.checksum_b3.clone(),
        settings_json,
        pending_restart_fields: doc.pending_restart_fields.clone(),
        updated_by: doc.updated_by.clone(),
    })
    .await
    .map_err(|e| format!("failed to upsert runtime config DB snapshot: {}", e))?;

    Ok(())
}

pub async fn load_runtime_config(db: &Db) -> Result<Option<LoadedRuntimeConfig>, String> {
    if !app_config_enabled() {
        return Ok(None);
    }

    let file_doc = read_file_document().await?;
    let db_doc = read_db_document(db).await?;

    match (file_doc, db_doc) {
        (None, None) => Ok(None),
        (Some(file_doc), None) => Ok(Some(LoadedRuntimeConfig {
            document: file_doc,
            effective_source: "app:file".to_string(),
        })),
        (None, Some(db_doc)) => Ok(Some(LoadedRuntimeConfig {
            document: db_doc,
            effective_source: "app:db".to_string(),
        })),
        (Some(file_doc), Some(db_doc)) => {
            let selected = newer_doc(&file_doc, &db_doc).clone();
            let source = if selected.version == file_doc.version
                && selected.updated_at == file_doc.updated_at
            {
                "app:file"
            } else {
                "app:db"
            };
            Ok(Some(LoadedRuntimeConfig {
                document: selected,
                effective_source: source.to_string(),
            }))
        }
    }
}

pub async fn reconcile_runtime_config(db: &Db) -> Result<SettingsReconcileResponse, String> {
    if !app_config_enabled() {
        return Ok(SettingsReconcileResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            success: true,
            status: "disabled".to_string(),
            message: "App-managed config is disabled by AOS_APP_CONFIG_ENABLED".to_string(),
            effective_source: None,
            applied_at: None,
        });
    }

    let file_doc = read_file_document().await?;
    let db_doc = read_db_document(db).await?;

    let (selected, selected_source, action) = match (file_doc, db_doc) {
        (None, None) => {
            return Ok(SettingsReconcileResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                success: true,
                status: "empty".to_string(),
                message: "No runtime config present in file or DB".to_string(),
                effective_source: None,
                applied_at: None,
            })
        }
        (Some(file_doc), None) => (file_doc, "app:file", "mirrored file -> db"),
        (None, Some(db_doc)) => (db_doc, "app:db", "mirrored db -> file"),
        (Some(file_doc), Some(db_doc)) => {
            let selected = newer_doc(&file_doc, &db_doc).clone();
            let source = if selected.version == file_doc.version
                && selected.updated_at == file_doc.updated_at
            {
                "app:file"
            } else {
                "app:db"
            };
            (
                selected,
                source,
                "reconciled by deterministic newest(version,updated_at)",
            )
        }
    };

    write_file_document(&selected).await?;
    write_db_document(db, &selected).await?;

    Ok(SettingsReconcileResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        success: true,
        status: "reconciled".to_string(),
        message: action.to_string(),
        effective_source: Some(selected_source.to_string()),
        applied_at: Some(selected.updated_at),
    })
}

pub async fn persist_runtime_update(
    db: &Db,
    request: &UpdateSettingsRequest,
    pending_restart_fields: Vec<String>,
    updated_by: Option<String>,
) -> Result<LoadedRuntimeConfig, String> {
    if !app_config_enabled() {
        return Err("app-managed config is disabled (AOS_APP_CONFIG_ENABLED=0)".to_string());
    }

    let current = load_runtime_config(db).await?;
    let next_version = current
        .as_ref()
        .map(|c| c.document.version + 1)
        .unwrap_or(1);

    let mut doc = RuntimeConfigDocument {
        schema_version: RUNTIME_CONFIG_SCHEMA_VERSION.to_string(),
        version: next_version,
        source: "settings_api".to_string(),
        checksum_b3: String::new(),
        updated_by,
        updated_at: chrono::Utc::now().to_rfc3339(),
        settings: request.clone(),
        pending_restart_fields,
    };
    doc.checksum_b3 = checksum_for_document(&doc);

    let mut file_err: Option<String> = None;
    let mut db_err: Option<String> = None;

    if let Err(e) = write_file_document(&doc).await {
        file_err = Some(e);
    }
    if let Err(e) = write_db_document(db, &doc).await {
        db_err = Some(e);
    }

    if dualwrite_required() && (file_err.is_some() || db_err.is_some()) {
        return Err(format!(
            "runtime config dual-write failed (required): file={:?}, db={:?}",
            file_err, db_err
        ));
    }

    if let Some(err) = &file_err {
        tracing::warn!(error = %err, "Runtime config file write failed; continuing");
    }
    if let Some(err) = &db_err {
        tracing::warn!(error = %err, "Runtime config DB write failed; continuing");
    }

    let effective_source = if db_err.is_none() {
        "app:db"
    } else {
        "app:file"
    }
    .to_string();

    Ok(LoadedRuntimeConfig {
        document: doc,
        effective_source,
    })
}

pub fn managed_keys() -> Vec<String> {
    MANAGED_KEYS.iter().map(|k| (*k).to_string()).collect()
}

pub fn warn_or_fail_on_managed_env_collisions(doc: &RuntimeConfigDocument) -> Result<(), String> {
    let strict = strict_precedence_enabled();

    let collisions = [
        (
            "general.api_base_url",
            "AOS_API_BASE_URL",
            doc.settings
                .general
                .as_ref()
                .map(|g| g.api_base_url.clone()),
        ),
        (
            "models.discovery_roots",
            "AOS_MODEL_DISCOVERY_ROOTS",
            doc.settings
                .models
                .as_ref()
                .map(|m| m.discovery_roots.join(",")),
        ),
        (
            "models.selected_model_path",
            "AOS_MODEL_PATH",
            doc.settings
                .models
                .as_ref()
                .and_then(|m| m.selected_model_path.clone()),
        ),
        (
            "models.selected_manifest_path",
            "AOS_WORKER_MANIFEST",
            doc.settings
                .models
                .as_ref()
                .and_then(|m| m.selected_manifest_path.clone()),
        ),
        (
            "models.selected_manifest_path",
            "AOS_MANIFEST_PATH",
            doc.settings
                .models
                .as_ref()
                .and_then(|m| m.selected_manifest_path.clone()),
        ),
        (
            "server.http_port",
            "AOS_SERVER_PORT",
            doc.settings
                .server
                .as_ref()
                .map(|s| s.http_port.to_string()),
        ),
        (
            "security.jwt_mode",
            "AOS_JWT_MODE",
            doc.settings.security.as_ref().map(|s| s.jwt_mode.clone()),
        ),
        (
            "performance.max_workers",
            "AOS_MAX_WORKERS",
            doc.settings
                .performance
                .as_ref()
                .map(|p| p.max_workers.to_string()),
        ),
    ];

    let mut msgs = Vec::new();
    for (key, env_var, app_value) in collisions {
        if app_value.is_none() {
            continue;
        }
        if let Ok(env_val) = std::env::var(env_var) {
            msgs.push(format!(
                "managed key '{}' has app-managed value and colliding env {}={}",
                key, env_var, env_val
            ));
        }
    }

    if msgs.is_empty() {
        return Ok(());
    }

    let joined = msgs.join("; ");
    if strict {
        Err(format!("managed env collisions (strict mode): {}", joined))
    } else {
        tracing::warn!(details = %joined, "Managed env collisions detected; app config precedence retained");
        Ok(())
    }
}

pub fn apply_runtime_overrides(
    api_config: &Arc<RwLock<ApiConfig>>,
    request: &UpdateSettingsRequest,
) -> Result<ApplyReport, String> {
    let mut report = ApplyReport::default();

    let mut cfg = api_config
        .write()
        .map_err(|e| format!("api config lock poisoned: {}", e))?;

    if let Some(general) = &request.general {
        let existing_det = cfg.general.as_ref().and_then(|g| g.determinism_mode);
        cfg.general = Some(GeneralConfig {
            system_name: Some(general.system_name.clone()),
            environment: Some(general.environment.clone()),
            api_base_url: Some(general.api_base_url.clone()),
            determinism_mode: existing_det,
        });
        report.applied_live.extend([
            "general.system_name".to_string(),
            "general.environment".to_string(),
            "general.api_base_url".to_string(),
        ]);
    }

    if let Some(perf) = &request.performance {
        cfg.performance.max_adapters = Some(perf.max_adapters as usize);
        cfg.performance.max_workers = Some(perf.max_workers as usize);
        cfg.performance.memory_threshold_pct = Some(perf.memory_threshold_pct);
        cfg.performance.cache_size_mb = Some(perf.cache_size_mb as usize);

        cfg.capacity_limits.models_per_worker = Some(perf.max_adapters as usize);
        cfg.capacity_limits.models_per_tenant = Some((perf.max_adapters / 2) as usize);

        report.applied_live.extend([
            "performance.max_adapters".to_string(),
            "performance.max_workers".to_string(),
            "performance.memory_threshold_pct".to_string(),
            "performance.cache_size_mb".to_string(),
        ]);
    }

    if request.models.is_some() {
        report.applied_live.extend([
            "models.discovery_roots".to_string(),
            "models.selected_model_path".to_string(),
            "models.selected_manifest_path".to_string(),
        ]);
    }

    if request.server.is_some() {
        report.queued_for_restart.extend([
            "server.http_port".to_string(),
            "server.https_port".to_string(),
            "server.uds_socket_path".to_string(),
            "server.production_mode".to_string(),
        ]);
    }

    if request.security.is_some() {
        report.queued_for_restart.extend([
            "security.jwt_mode".to_string(),
            "security.token_ttl_seconds".to_string(),
            "security.require_mfa".to_string(),
            "security.require_pf_deny".to_string(),
        ]);
    }

    report.queued_for_restart.sort();
    report.queued_for_restart.dedup();

    Ok(report)
}

pub fn settings_from_api_config(api: &ApiConfig) -> SystemSettings {
    SystemSettings {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        general: GeneralSettings {
            system_name: api
                .general
                .as_ref()
                .and_then(|g| g.system_name.clone())
                .unwrap_or_else(|| "adapterOS".to_string()),
            environment: api
                .general
                .as_ref()
                .and_then(|g| g.environment.clone())
                .unwrap_or_else(|| "production".to_string()),
            api_base_url: api
                .general
                .as_ref()
                .and_then(|g| g.api_base_url.clone())
                .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string()),
        },
        models: ModelSettings {
            discovery_roots: default_model_discovery_roots()
                .unwrap_or_default()
                .into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            selected_model_path: std::env::var("AOS_MODEL_PATH")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            selected_manifest_path: std::env::var("AOS_WORKER_MANIFEST")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .or_else(|| {
                    std::env::var("AOS_MANIFEST_PATH")
                        .ok()
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty())
                }),
        },
        server: ServerSettings {
            http_port: api.server.http_port.unwrap_or(18080),
            https_port: api.server.https_port,
            uds_socket_path: api.server.uds_socket.clone(),
            production_mode: api.server.production_mode,
        },
        security: SecuritySettings {
            jwt_mode: api
                .security
                .jwt_mode
                .clone()
                .unwrap_or_else(|| "eddsa".to_string()),
            token_ttl_seconds: api.security.token_ttl_seconds.unwrap_or(28800) as u32,
            require_mfa: api.security.require_mfa.unwrap_or(false),
            egress_enabled: !api.security.require_pf_deny,
            require_pf_deny: api.security.require_pf_deny,
        },
        performance: PerformanceSettings {
            max_adapters: api.performance.max_adapters.unwrap_or(100) as u32,
            max_workers: api.performance.max_workers.unwrap_or(10) as u32,
            memory_threshold_pct: api.performance.memory_threshold_pct.unwrap_or(0.85),
            cache_size_mb: api.performance.cache_size_mb.unwrap_or(1024) as u64,
        },
        effective_source: None,
        applied_at: None,
        restart_required_fields: Vec::new(),
        pending_restart_fields: Vec::new(),
    }
}

pub fn build_effective_settings_response(
    settings: &SystemSettings,
    loaded: Option<&LoadedRuntimeConfig>,
) -> EffectiveSettingsResponse {
    let mut entries = Vec::new();
    let mut source_map: BTreeMap<String, String> = BTreeMap::new();

    for key in managed_keys() {
        source_map.insert(key, "default".to_string());
    }

    if let Some(loaded) = loaded {
        let src = loaded.effective_source.clone();
        if loaded.document.settings.general.is_some() {
            source_map.insert("general.system_name".to_string(), src.clone());
            source_map.insert("general.environment".to_string(), src.clone());
            source_map.insert("general.api_base_url".to_string(), src.clone());
        }
        if loaded.document.settings.models.is_some() {
            source_map.insert("models.discovery_roots".to_string(), src.clone());
            source_map.insert("models.selected_model_path".to_string(), src.clone());
            source_map.insert("models.selected_manifest_path".to_string(), src.clone());
        }
        if loaded.document.settings.server.is_some() {
            source_map.insert("server.http_port".to_string(), src.clone());
            source_map.insert("server.https_port".to_string(), src.clone());
            source_map.insert("server.uds_socket_path".to_string(), src.clone());
            source_map.insert("server.production_mode".to_string(), src.clone());
        }
        if loaded.document.settings.security.is_some() {
            source_map.insert("security.jwt_mode".to_string(), src.clone());
            source_map.insert("security.token_ttl_seconds".to_string(), src.clone());
            source_map.insert("security.require_mfa".to_string(), src.clone());
            source_map.insert("security.require_pf_deny".to_string(), src.clone());
        }
        if loaded.document.settings.performance.is_some() {
            source_map.insert("performance.max_adapters".to_string(), src.clone());
            source_map.insert("performance.max_workers".to_string(), src.clone());
            source_map.insert("performance.memory_threshold_pct".to_string(), src.clone());
            source_map.insert("performance.cache_size_mb".to_string(), src.clone());
        }
    }

    let add_entry = |entries: &mut Vec<EffectiveSettingsEntry>,
                     key: &str,
                     value: serde_json::Value,
                     source_map: &BTreeMap<String, String>| {
        entries.push(EffectiveSettingsEntry {
            key: key.to_string(),
            value,
            effective_source: source_map
                .get(key)
                .cloned()
                .unwrap_or_else(|| "default".to_string()),
        });
    };

    add_entry(
        &mut entries,
        "general.system_name",
        serde_json::json!(settings.general.system_name),
        &source_map,
    );
    add_entry(
        &mut entries,
        "general.environment",
        serde_json::json!(settings.general.environment),
        &source_map,
    );
    add_entry(
        &mut entries,
        "general.api_base_url",
        serde_json::json!(settings.general.api_base_url),
        &source_map,
    );
    add_entry(
        &mut entries,
        "models.discovery_roots",
        serde_json::json!(settings.models.discovery_roots),
        &source_map,
    );
    add_entry(
        &mut entries,
        "models.selected_model_path",
        serde_json::json!(settings.models.selected_model_path),
        &source_map,
    );
    add_entry(
        &mut entries,
        "models.selected_manifest_path",
        serde_json::json!(settings.models.selected_manifest_path),
        &source_map,
    );
    add_entry(
        &mut entries,
        "server.http_port",
        serde_json::json!(settings.server.http_port),
        &source_map,
    );
    add_entry(
        &mut entries,
        "server.https_port",
        serde_json::json!(settings.server.https_port),
        &source_map,
    );
    add_entry(
        &mut entries,
        "server.uds_socket_path",
        serde_json::json!(settings.server.uds_socket_path),
        &source_map,
    );
    add_entry(
        &mut entries,
        "server.production_mode",
        serde_json::json!(settings.server.production_mode),
        &source_map,
    );
    add_entry(
        &mut entries,
        "security.jwt_mode",
        serde_json::json!(settings.security.jwt_mode),
        &source_map,
    );
    add_entry(
        &mut entries,
        "security.token_ttl_seconds",
        serde_json::json!(settings.security.token_ttl_seconds),
        &source_map,
    );
    add_entry(
        &mut entries,
        "security.require_mfa",
        serde_json::json!(settings.security.require_mfa),
        &source_map,
    );
    add_entry(
        &mut entries,
        "security.require_pf_deny",
        serde_json::json!(settings.security.require_pf_deny),
        &source_map,
    );
    add_entry(
        &mut entries,
        "performance.max_adapters",
        serde_json::json!(settings.performance.max_adapters),
        &source_map,
    );
    add_entry(
        &mut entries,
        "performance.max_workers",
        serde_json::json!(settings.performance.max_workers),
        &source_map,
    );
    add_entry(
        &mut entries,
        "performance.memory_threshold_pct",
        serde_json::json!(settings.performance.memory_threshold_pct),
        &source_map,
    );
    add_entry(
        &mut entries,
        "performance.cache_size_mb",
        serde_json::json!(settings.performance.cache_size_mb),
        &source_map,
    );

    EffectiveSettingsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        entries,
        managed_keys: managed_keys(),
    }
}

pub async fn apply_loaded_runtime_config(
    db: &Db,
    api_config: &Arc<RwLock<ApiConfig>>,
) -> Result<Option<LoadedRuntimeConfig>, String> {
    let loaded = load_runtime_config(db).await?;
    let Some(loaded) = loaded else {
        return Ok(None);
    };

    warn_or_fail_on_managed_env_collisions(&loaded.document)?;
    let _ = apply_runtime_overrides(api_config, &loaded.document.settings)?;
    Ok(Some(loaded))
}

pub fn runtime_config_file_path() -> PathBuf {
    runtime_config_path()
}

pub fn runtime_config_file_exists() -> bool {
    Path::new(&runtime_config_path()).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_keys_are_non_empty_and_stable() {
        let keys = managed_keys();
        assert!(!keys.is_empty());
        assert!(keys.iter().any(|k| k == "general.system_name"));
        assert!(keys.iter().any(|k| k == "performance.max_workers"));
    }

    #[test]
    fn apply_runtime_overrides_updates_live_safe_fields_only() {
        let api_config = Arc::new(RwLock::new(ApiConfig::default()));
        let request = UpdateSettingsRequest {
            general: Some(GeneralSettings {
                system_name: "MySystem".to_string(),
                environment: "staging".to_string(),
                api_base_url: "http://127.0.0.1:18080".to_string(),
            }),
            models: None,
            server: Some(ServerSettings {
                http_port: 19080,
                https_port: None,
                uds_socket_path: None,
                production_mode: false,
            }),
            security: None,
            performance: Some(PerformanceSettings {
                max_adapters: 55,
                max_workers: 9,
                memory_threshold_pct: 0.75,
                cache_size_mb: 512,
            }),
        };

        let report = apply_runtime_overrides(&api_config, &request).expect("apply should succeed");
        assert!(report
            .applied_live
            .iter()
            .any(|f| f == "general.system_name"));
        assert!(report
            .applied_live
            .iter()
            .any(|f| f == "performance.max_workers"));
        assert!(report
            .queued_for_restart
            .iter()
            .any(|f| f == "server.http_port"));

        let cfg = api_config.read().expect("lock");
        assert_eq!(
            cfg.general.as_ref().and_then(|g| g.system_name.clone()),
            Some("MySystem".to_string())
        );
        assert_eq!(cfg.performance.max_workers, Some(9));
        assert_eq!(cfg.capacity_limits.models_per_worker, Some(55));
    }
}
