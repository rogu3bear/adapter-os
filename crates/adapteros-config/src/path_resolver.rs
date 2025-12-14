use adapteros_core::paths::AOS_ADAPTERS_DIR_ENV;
use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};

/// Absolute prefixes that are forbidden for system/local sockets.
const FORBIDDEN_TMP_PREFIXES: [&str; 2] = ["/tmp", "/private/tmp"];

/// Dev-only fixture path for the default local Qwen2.5-7B-Instruct-4bit model.
pub const DEV_MODEL_PATH: &str = "./var/models/Qwen2.5-7B-Instruct-4bit";

/// Dev-only fixture path for the default local Qwen2.5-7B-Instruct-4bit manifest (config.json).
pub const DEV_MANIFEST_PATH: &str = "./var/models/Qwen2.5-7B-Instruct-4bit/config.json";

/// Default cache root for base models (can be overridden via AOS_MODEL_CACHE_DIR).
pub const DEFAULT_MODEL_CACHE_ROOT: &str = "./var/model-cache/models";

/// Default embedding model path (can be overridden via AOS_EMBEDDING_MODEL_PATH).
pub const DEFAULT_EMBEDDING_MODEL_PATH: &str = "./var/model-cache/models/bge-small-en-v1.5";

/// Default base model identifier (can be overridden via AOS_BASE_MODEL_ID).
pub const DEFAULT_BASE_MODEL_ID: &str = "qwen2.5-7b-mlx";

/// Default telemetry directory.
pub const DEFAULT_TELEMETRY_DIR: &str = "./var/telemetry";

/// Default index root directory (per-tenant subdirs will be appended).
pub const DEFAULT_INDEX_ROOT: &str = "./var/indices";

/// Default manifest cache directory.
pub const DEFAULT_MANIFEST_CACHE_DIR: &str = "./var/manifest-cache";

/// Default adapters root directory.
pub const DEFAULT_ADAPTERS_ROOT: &str = "./var/adapters";

/// Production worker socket root.
pub const DEFAULT_WORKER_SOCKET_PROD_ROOT: &str = "/var/run/aos";

/// Development worker socket path.
pub const DEFAULT_WORKER_SOCKET_DEV: &str = "./var/run/worker.sock";

/// Control plane worker socket default (training cancel path).
pub const DEFAULT_CP_WORKER_SOCKET: &str = "/var/run/adapteros.sock";

/// Default status file path consumed by the menu bar app.
pub const DEFAULT_STATUS_PATH: &str = "/var/run/adapteros_status.json";

/// Default SQLite URL for the control plane database.
pub const DEFAULT_DB_PATH: &str = "sqlite://var/aos-cp.sqlite3";

/// Primary adapters root environment variable.
pub const AOS_ADAPTERS_ROOT_ENV: &str = "AOS_ADAPTERS_ROOT";

/// Source describing where a resolved path originated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSource {
    Env(&'static str),
    Cli,
    Config(&'static str),
    DevFallback(&'static str),
    Default(&'static str),
}

impl std::fmt::Display for PathSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathSource::Env(var) => write!(f, "env:{}", var),
            PathSource::Cli => write!(f, "cli"),
            PathSource::Config(key) => write!(f, "config:{}", key),
            PathSource::DevFallback(label) => write!(f, "dev-fallback:{}", label),
            PathSource::Default(label) => write!(f, "default:{}", label),
        }
    }
}

/// Result of resolving a path with provenance metadata.
#[derive(Debug, Clone)]
pub struct ResolvedPath {
    pub path: PathBuf,
    pub source: PathSource,
    pub used_dev_fallback: bool,
}

/// Resolved base model location with provenance.
#[derive(Debug, Clone)]
pub struct BaseModelLocation {
    /// Canonical base model identifier.
    pub id: String,
    /// Cache root directory containing base models.
    pub cache_root: PathBuf,
    /// Fully qualified model path = cache_root/id.
    pub full_path: PathBuf,
}

/// Resolve manifest path with precedence: env > CLI > config > dev fallback.
///
/// In release builds (`debug_assertions` off), dev fallback is rejected and an error is returned
/// when no env/CLI/config value is provided.
pub fn resolve_manifest_path(
    cli_override: Option<&PathBuf>,
    config_path: Option<&PathBuf>,
) -> Result<ResolvedPath> {
    resolve_path(
        "manifest",
        "AOS_MANIFEST_PATH",
        cli_override,
        config_path,
        Some(DEV_MANIFEST_PATH),
        cfg!(debug_assertions),
    )
}

/// Resolve the base model location using a single source of truth.
///
/// Precedence (id + cache root independently):
/// 1. Explicit overrides
/// 2. Environment: AOS_BASE_MODEL_ID / AOS_MODEL_CACHE_DIR
/// 3. Effective config (base_model.id / base_model.cache_root) if initialized
/// 4. Defaults: DEFAULT_BASE_MODEL_ID / DEFAULT_MODEL_CACHE_ROOT
pub fn resolve_base_model_location(
    id_override: Option<&str>,
    cache_root_override: Option<&Path>,
    require_existing: bool,
) -> Result<BaseModelLocation> {
    crate::model::load_dotenv();
    let effective = crate::effective::try_effective_config();

    let id = id_override
        .map(|s| s.to_string())
        .or_else(|| std::env::var("AOS_BASE_MODEL_ID").ok())
        .or_else(|| {
            effective
                .and_then(|cfg| cfg.model.base_id.as_ref())
                .cloned()
        })
        .unwrap_or_else(|| DEFAULT_BASE_MODEL_ID.to_string());

    let cache_root = cache_root_override
        .map(PathBuf::from)
        .or_else(|| std::env::var("AOS_MODEL_CACHE_DIR").ok().map(PathBuf::from))
        .or_else(|| {
            effective
                .and_then(|cfg| cfg.model.cache_root.as_ref())
                .cloned()
        })
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL_CACHE_ROOT));

    let full_path = cache_root.join(&id);

    if require_existing && !full_path.exists() {
        return Err(AosError::Config(format!(
            "Model path does not exist: {}. Configure AOS_MODEL_CACHE_DIR and AOS_BASE_MODEL_ID or set base_model.cache_root/base_model.id in config.",
            full_path.display()
        )));
    }

    Ok(BaseModelLocation {
        id,
        cache_root,
        full_path,
    })
}

/// Resolve embedding model path with env/default provenance.
pub fn resolve_embedding_model_path() -> ResolvedPath {
    resolve_embedding_model_path_with_override(None)
}

/// Resolve embedding model path with CLI/env/default precedence.
///
/// Precedence:
/// 1) CLI override (validated)
/// 2) AOS_EMBEDDING_MODEL_PATH
/// 3) Default: DEFAULT_EMBEDDING_MODEL_PATH
pub fn resolve_embedding_model_path_with_override(cli_override: Option<&Path>) -> ResolvedPath {
    crate::model::load_dotenv();

    if let Some(path) = cli_override {
        let path = path.to_path_buf();
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Cli,
            kind = %"embedding-model",
            "Resolved embedding model path from CLI override"
        );
        return ResolvedPath {
            path,
            source: PathSource::Cli,
            used_dev_fallback: false,
        };
    }

    resolve_env_or_default(
        "AOS_EMBEDDING_MODEL_PATH",
        DEFAULT_EMBEDDING_MODEL_PATH,
        "embedding-model",
    )
}

/// Resolve telemetry directory with env/default provenance.
pub fn resolve_telemetry_dir() -> Result<ResolvedPath> {
    resolve_env_or_default_no_tmp("AOS_TELEMETRY_DIR", DEFAULT_TELEMETRY_DIR, "telemetry-dir")
}

/// Resolve index root directory with env/default provenance.
/// Rejects /tmp paths for security.
pub fn resolve_index_root() -> Result<ResolvedPath> {
    resolve_env_or_default_no_tmp("AOS_INDEX_DIR", DEFAULT_INDEX_ROOT, "index-root")
}

/// Resolve manifest cache directory with env/default provenance.
pub fn resolve_manifest_cache_dir() -> Result<ResolvedPath> {
    resolve_env_or_default_no_tmp(
        "AOS_MANIFEST_CACHE_DIR",
        DEFAULT_MANIFEST_CACHE_DIR,
        "manifest-cache",
    )
}

/// Resolve adapters root with support for both AOS_ADAPTERS_ROOT (preferred) and AOS_ADAPTERS_DIR (legacy).
pub fn resolve_adapters_root() -> Result<ResolvedPath> {
    crate::model::load_dotenv();
    if let Ok(env_path) = std::env::var(AOS_ADAPTERS_ROOT_ENV) {
        let path = PathBuf::from(&env_path);
        reject_tmp_persistent_path(&path, "adapters-root")?;
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Env(AOS_ADAPTERS_ROOT_ENV),
            "Resolved adapters root from environment"
        );
        return Ok(ResolvedPath {
            path,
            source: PathSource::Env(AOS_ADAPTERS_ROOT_ENV),
            used_dev_fallback: false,
        });
    }

    if let Ok(env_path) = std::env::var(AOS_ADAPTERS_DIR_ENV) {
        let path = PathBuf::from(&env_path);
        reject_tmp_persistent_path(&path, "adapters-root")?;
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Env(AOS_ADAPTERS_DIR_ENV),
            "Resolved adapters root from legacy environment variable"
        );
        return Ok(ResolvedPath {
            path,
            source: PathSource::Env(AOS_ADAPTERS_DIR_ENV),
            used_dev_fallback: false,
        });
    }

    let path = PathBuf::from(DEFAULT_ADAPTERS_ROOT);
    reject_tmp_persistent_path(&path, "adapters-root")?;
    tracing::info!(
        path = %path.display(),
        source = %PathSource::Default("adapters-root"),
        "Using default adapters root"
    );

    Ok(ResolvedPath {
        path,
        source: PathSource::Default("adapters-root"),
        used_dev_fallback: false,
    })
}

/// Resolve database URL with env/default provenance.
pub fn resolve_database_url() -> Result<ResolvedPath> {
    crate::model::load_dotenv();
    if let Ok(url) = std::env::var("AOS_DATABASE_URL") {
        tracing::info!(
            database_url = %url,
            source = %PathSource::Env("AOS_DATABASE_URL"),
            "Resolved database URL from environment"
        );
        let resolved = ResolvedPath {
            path: PathBuf::from(url),
            source: PathSource::Env("AOS_DATABASE_URL"),
            used_dev_fallback: false,
        };
        reject_tmp_persistent_path(&resolved.path, "database-url")?;
        return Ok(resolved);
    }

    if let Ok(url) = std::env::var("DATABASE_URL") {
        tracing::info!(
            database_url = %url,
            source = %PathSource::Env("DATABASE_URL"),
            "Resolved database URL from legacy environment variable"
        );
        let resolved = ResolvedPath {
            path: PathBuf::from(url),
            source: PathSource::Env("DATABASE_URL"),
            used_dev_fallback: false,
        };
        reject_tmp_persistent_path(&resolved.path, "database-url")?;
        return Ok(resolved);
    }

    let path = PathBuf::from(DEFAULT_DB_PATH);
    reject_tmp_persistent_path(&path, "database-url")?;
    tracing::info!(
        database_url = %path.display(),
        source = %PathSource::Default("database-url"),
        "Using default database URL"
    );

    Ok(ResolvedPath {
        path,
        source: PathSource::Default("database-url"),
        used_dev_fallback: false,
    })
}

/// Resolve worker socket for worker processes.
///
/// Precedence:
/// 1) CLI override (already parsed into `override_path`)
/// 2) AOS_WORKER_SOCKET
/// 3) /var/run/aos/{tenant}/worker.sock (attempts to create parent)
/// 4) ./var/run/worker.sock (dev fallback)
pub fn resolve_worker_socket_for_worker(
    tenant_id: &str,
    override_path: Option<&Path>,
) -> Result<ResolvedPath> {
    crate::model::load_dotenv();
    if let Some(path) = override_path {
        reject_tmp_socket(path, "worker")?;
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Cli,
            "Resolved worker socket from CLI/env override"
        );
        return Ok(ResolvedPath {
            path: path.to_path_buf(),
            source: PathSource::Cli,
            used_dev_fallback: false,
        });
    }

    if let Ok(env_path) = std::env::var("AOS_WORKER_SOCKET") {
        let path = PathBuf::from(&env_path);
        reject_tmp_socket(&path, "worker")?;
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Env("AOS_WORKER_SOCKET"),
            "Resolved worker socket from environment"
        );
        return Ok(ResolvedPath {
            path,
            source: PathSource::Env("AOS_WORKER_SOCKET"),
            used_dev_fallback: false,
        });
    }

    let prod_path = PathBuf::from(format!(
        "{}/{}/worker.sock",
        DEFAULT_WORKER_SOCKET_PROD_ROOT.trim_end_matches('/'),
        tenant_id
    ));
    reject_tmp_socket(&prod_path, "worker")?;
    if let Some(parent) = prod_path.parent() {
        if std::fs::create_dir_all(parent).is_ok() {
            tracing::info!(
                path = %prod_path.display(),
                source = %PathSource::Default("worker-socket-prod"),
                "Using per-tenant worker socket path"
            );
            return Ok(ResolvedPath {
                path: prod_path,
                source: PathSource::Default("worker-socket-prod"),
                used_dev_fallback: false,
            });
        } else {
            tracing::warn!(
                path = %prod_path.display(),
                "Failed to create production worker socket directory, falling back to dev path"
            );
        }
    }

    let dev_path = PathBuf::from(DEFAULT_WORKER_SOCKET_DEV);
    reject_tmp_socket(&dev_path, "worker")?;
    if let Some(parent) = dev_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    tracing::info!(
        path = %dev_path.display(),
        source = %PathSource::DevFallback(DEFAULT_WORKER_SOCKET_DEV),
        "Using dev worker socket fallback"
    );

    Ok(ResolvedPath {
        path: dev_path,
        source: PathSource::DevFallback(DEFAULT_WORKER_SOCKET_DEV),
        used_dev_fallback: true,
    })
}

/// Resolve worker socket for control plane clients (training cancel, owner chat).
///
/// Precedence:
/// 1) AOS_WORKER_SOCKET
/// 2) /var/run/adapteros.sock
pub fn resolve_worker_socket_for_cp() -> Result<ResolvedPath> {
    crate::model::load_dotenv();
    if let Ok(env_path) = std::env::var("AOS_WORKER_SOCKET") {
        let path = PathBuf::from(&env_path);
        reject_tmp_socket(&path, "control-plane")?;
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Env("AOS_WORKER_SOCKET"),
            "Resolved control-plane worker socket from environment"
        );
        return Ok(ResolvedPath {
            path,
            source: PathSource::Env("AOS_WORKER_SOCKET"),
            used_dev_fallback: false,
        });
    }

    let path = PathBuf::from(DEFAULT_CP_WORKER_SOCKET);
    reject_tmp_socket(&path, "control-plane")?;
    tracing::info!(
        path = %path.display(),
        source = %PathSource::Default("worker-socket-cp"),
        "Using default control-plane worker socket"
    );
    Ok(ResolvedPath {
        path,
        source: PathSource::Default("worker-socket-cp"),
        used_dev_fallback: false,
    })
}

/// Remove stale socket file (if present) and ensure parent directory exists.
///
/// Also rejects sockets under `/tmp` (or `/private/tmp`) to enforce system-local invariants.
pub fn prepare_socket_path(path: &Path, kind: &str) -> Result<()> {
    reject_tmp_socket(path, kind)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AosError::Config(format!(
                "Failed to create {} socket directory {}: {}",
                kind,
                parent.display(),
                e
            ))
        })?;
    }

    if path.exists() {
        std::fs::remove_file(path).map_err(|e| {
            AosError::Config(format!(
                "Failed to remove stale {} socket {}: {}",
                kind,
                path.display(),
                e
            ))
        })?;
    }

    Ok(())
}

fn reject_tmp_socket(path: &Path, kind: &str) -> Result<()> {
    let path_str = path.display().to_string();
    if FORBIDDEN_TMP_PREFIXES
        .iter()
        .any(|prefix| path_str.starts_with(prefix))
    {
        return Err(AosError::Config(format!(
            "{} socket path must not be under /tmp: {}",
            kind,
            path.display()
        )));
    }
    Ok(())
}

fn reject_tmp_persistent_path(path: &Path, kind: &str) -> Result<()> {
    let path_str = path.display().to_string();
    let mut candidate = path_str.as_str();

    for prefix in ["sqlite://", "sqlite:", "file://", "file:"] {
        if let Some(stripped) = candidate.strip_prefix(prefix) {
            candidate = stripped;
            break;
        }
    }

    while candidate.starts_with("//") {
        candidate = &candidate[1..];
    }

    if FORBIDDEN_TMP_PREFIXES
        .iter()
        .any(|prefix| candidate.starts_with(prefix))
    {
        return Err(AosError::Config(format!(
            "{} path must not be under /tmp: {}",
            kind,
            path.display()
        )));
    }

    Ok(())
}

/// Resolve status file path for menu bar integration.
pub fn resolve_status_path() -> ResolvedPath {
    resolve_env_or_default("AOS_STATUS_PATH", DEFAULT_STATUS_PATH, "status-path")
}

/// Resolve model path with precedence: env > CLI > config > dev fallback.
///
/// In release builds (`debug_assertions` off), dev fallback is rejected and an error is returned
/// when no env/CLI/config value is provided.
pub fn resolve_model_path(
    cli_override: Option<&PathBuf>,
    config_path: Option<&PathBuf>,
) -> Result<ResolvedPath> {
    resolve_path(
        "model",
        "AOS_MODEL_PATH",
        cli_override,
        config_path,
        Some(DEV_MODEL_PATH),
        cfg!(debug_assertions),
    )
}

fn resolve_path(
    kind: &str,
    env_var: &'static str,
    cli_override: Option<&PathBuf>,
    config_path: Option<&PathBuf>,
    dev_fallback: Option<&'static str>,
    allow_dev_fallback: bool,
) -> Result<ResolvedPath> {
    // 1) Environment
    if let Ok(val) = std::env::var(env_var) {
        if !val.is_empty() {
            let path = PathBuf::from(&val);
            validate_path_exists(&path, kind, env_var)?;
            tracing::info!(path = %path.display(), source = %PathSource::Env(env_var), kind, "Resolved {} path from environment", kind);
            return Ok(ResolvedPath {
                path,
                source: PathSource::Env(env_var),
                used_dev_fallback: false,
            });
        }
    }

    // 2) CLI override
    if let Some(path) = cli_override {
        validate_path_exists(path, kind, "CLI")?;
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Cli,
            kind,
            "Resolved {} path from CLI",
            kind
        );
        return Ok(ResolvedPath {
            path: path.clone(),
            source: PathSource::Cli,
            used_dev_fallback: false,
        });
    }

    // 3) Config file
    if let Some(path) = config_path {
        validate_path_exists(path, kind, "config")?;
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Config("config-file"),
            kind,
            "Resolved {} path from config",
            kind
        );
        return Ok(ResolvedPath {
            path: path.clone(),
            source: PathSource::Config("config-file"),
            used_dev_fallback: false,
        });
    }

    // 4) Dev fallback (debug-only)
    if let Some(fallback) = dev_fallback {
        if !allow_dev_fallback {
            return Err(AosError::Config(format!(
                "{} path not configured. Set {} or provide a CLI/config value; dev fallback '{}' is disabled in release builds.",
                kind, env_var, fallback
            )));
        }

        let path = PathBuf::from(fallback);
        if !path.exists() {
            tracing::warn!(
                path = %path.display(),
                kind,
                "Dev fallback {} path does not exist; continuing (debug build only)",
                kind
            );
        } else {
            tracing::info!(
                path = %path.display(),
                source = %PathSource::DevFallback(fallback),
                kind,
                "Using dev fallback {} path",
                kind
            );
        }

        return Ok(ResolvedPath {
            path,
            source: PathSource::DevFallback(fallback),
            used_dev_fallback: true,
        });
    }

    Err(AosError::Config(format!(
        "{} path not configured. Set {} or provide a CLI/config value.",
        kind, env_var
    )))
}

fn resolve_env_or_default(
    env_var: &'static str,
    default: &'static str,
    label: &'static str,
) -> ResolvedPath {
    crate::model::load_dotenv();
    if let Ok(val) = std::env::var(env_var) {
        if !val.is_empty() {
            let path = PathBuf::from(&val);
            tracing::info!(
                path = %path.display(),
                source = %PathSource::Env(env_var),
                kind = %label,
                "Resolved {} from environment",
                label
            );
            return ResolvedPath {
                path,
                source: PathSource::Env(env_var),
                used_dev_fallback: false,
            };
        }
    }

    let path = PathBuf::from(default);
    tracing::info!(
        path = %path.display(),
        source = %PathSource::Default(label),
        kind = %label,
        "Using default {} path",
        label
    );
    ResolvedPath {
        path,
        source: PathSource::Default(label),
        used_dev_fallback: false,
    }
}

fn resolve_env_or_default_no_tmp(
    env_var: &'static str,
    default: &'static str,
    label: &'static str,
) -> Result<ResolvedPath> {
    let resolved = resolve_env_or_default(env_var, default, label);
    reject_tmp_persistent_path(&resolved.path, label)?;
    Ok(resolved)
}

fn validate_path_exists(path: &Path, kind: &str, source: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    Err(AosError::Config(format!(
        "{} path from {} does not exist: {}",
        kind,
        source,
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::default_schema;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn env_beats_cli_and_config() {
        let tmp = TempDir::new().unwrap();
        let env_path = tmp.path().join("env_manifest.json");
        fs::write(&env_path, "{}").unwrap();

        std::env::set_var("AOS_MANIFEST_PATH", env_path.to_str().unwrap());
        let cli_path = tmp.path().join("cli_manifest.json");
        fs::write(&cli_path, "{}").unwrap();
        let cfg_path = tmp.path().join("cfg_manifest.json");
        fs::write(&cfg_path, "{}").unwrap();

        let resolved =
            resolve_manifest_path(Some(&cli_path), Some(&cfg_path)).expect("env should win");
        assert_eq!(resolved.path, env_path);
        assert_eq!(resolved.source, PathSource::Env("AOS_MANIFEST_PATH"));

        std::env::remove_var("AOS_MANIFEST_PATH");
    }

    #[test]
    fn rejects_dev_fallback_when_disabled() {
        let err = resolve_path(
            "manifest",
            "AOS_MANIFEST_PATH",
            None,
            None,
            Some(DEV_MANIFEST_PATH),
            false,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("dev fallback"));
        assert!(err.contains("AOS_MANIFEST_PATH"));
    }

    #[test]
    fn schema_defaults_reference_dev_constants() {
        let schema = default_schema();
        let model_var = schema.get_variable("AOS_MODEL_PATH").unwrap();
        assert_eq!(model_var.default.as_deref(), Some(DEV_MODEL_PATH));

        let manifest_var = schema.get_variable("AOS_MANIFEST_PATH").unwrap();
        assert_eq!(manifest_var.default.as_deref(), Some(DEV_MANIFEST_PATH));

        let cache_root = schema.get_variable("AOS_MODEL_CACHE_DIR").unwrap();
        assert_eq!(
            cache_root.default.as_deref(),
            Some(DEFAULT_MODEL_CACHE_ROOT)
        );

        let base_model = schema.get_variable("AOS_BASE_MODEL_ID").unwrap();
        assert_eq!(base_model.default.as_deref(), Some(DEFAULT_BASE_MODEL_ID));
    }

    #[test]
    fn base_model_resolves_with_overrides_and_defaults() {
        let tmp_root = TempDir::new().unwrap();
        let id_override = "custom-id";
        let cache_root_override = tmp_root.path();
        let resolved =
            resolve_base_model_location(Some(id_override), Some(cache_root_override), false)
                .expect("resolver should succeed");

        assert_eq!(resolved.id, id_override);
        assert_eq!(resolved.cache_root, cache_root_override);
        assert_eq!(resolved.full_path, cache_root_override.join(id_override));
    }

    #[test]
    fn base_model_uses_env_cache_root() {
        std::env::set_var("AOS_MODEL_CACHE_DIR", "/tmp/cache-root");
        let resolved = resolve_base_model_location(None, None, false).unwrap();
        assert_eq!(resolved.cache_root, PathBuf::from("/tmp/cache-root"));
        std::env::remove_var("AOS_MODEL_CACHE_DIR");
    }

    #[test]
    fn telemetry_dir_prefers_env() {
        std::env::set_var("AOS_TELEMETRY_DIR", "./var/aos-telemetry-env");
        let resolved = resolve_telemetry_dir().unwrap();
        assert_eq!(resolved.path, PathBuf::from("./var/aos-telemetry-env"));
        assert_eq!(resolved.source, PathSource::Env("AOS_TELEMETRY_DIR"));
        std::env::remove_var("AOS_TELEMETRY_DIR");
    }

    #[test]
    fn telemetry_dir_rejects_tmp_env() {
        std::env::set_var("AOS_TELEMETRY_DIR", "/tmp/aos-telemetry");
        let err = resolve_telemetry_dir().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("telemetry-dir"));
        std::env::remove_var("AOS_TELEMETRY_DIR");
    }

    #[test]
    fn telemetry_dir_rejects_private_tmp_env() {
        std::env::set_var("AOS_TELEMETRY_DIR", "/private/tmp/aos-telemetry");
        let err = resolve_telemetry_dir().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("telemetry-dir"));
        std::env::remove_var("AOS_TELEMETRY_DIR");
    }

    #[test]
    fn manifest_cache_rejects_tmp_env() {
        std::env::set_var("AOS_MANIFEST_CACHE_DIR", "/tmp/manifest-cache");
        let err = resolve_manifest_cache_dir().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("manifest-cache"));
        std::env::remove_var("AOS_MANIFEST_CACHE_DIR");
    }

    #[test]
    fn manifest_cache_rejects_tmp_uri_env() {
        std::env::set_var("AOS_MANIFEST_CACHE_DIR", "file:///tmp/manifest-cache");
        let err = resolve_manifest_cache_dir().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("manifest-cache"));
        std::env::remove_var("AOS_MANIFEST_CACHE_DIR");
    }

    #[test]
    fn adapters_root_rejects_tmp_env() {
        std::env::set_var(AOS_ADAPTERS_ROOT_ENV, "/tmp/adapters");
        let err = resolve_adapters_root().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("adapters-root"));
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
    }

    #[test]
    fn adapters_root_rejects_tmp_legacy_env() {
        std::env::set_var(AOS_ADAPTERS_DIR_ENV, "/tmp/adapters");
        let err = resolve_adapters_root().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("adapters-root"));
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
    }

    #[test]
    fn index_root_rejects_tmp() {
        std::env::set_var("AOS_INDEX_DIR", "/tmp/indices");
        let err = resolve_index_root().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("index-root"));
        std::env::remove_var("AOS_INDEX_DIR");
    }

    #[test]
    fn database_url_defaults_when_unset() {
        std::env::remove_var("DATABASE_URL");
        std::env::remove_var("AOS_DATABASE_URL");
        let resolved = resolve_database_url().unwrap();
        assert_eq!(resolved.path, PathBuf::from(DEFAULT_DB_PATH));
        assert_eq!(resolved.source, PathSource::Default("database-url"));
    }

    #[test]
    fn database_url_prefers_primary_env() {
        std::env::set_var("AOS_DATABASE_URL", "sqlite://var/db-primary.sqlite3");
        std::env::set_var("DATABASE_URL", "sqlite://var/db-legacy.sqlite3");
        let resolved = resolve_database_url().unwrap();
        assert_eq!(
            resolved.path,
            PathBuf::from("sqlite://var/db-primary.sqlite3")
        );
        assert_eq!(resolved.source, PathSource::Env("AOS_DATABASE_URL"));
        std::env::remove_var("AOS_DATABASE_URL");
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    fn database_url_falls_back_to_legacy_env() {
        std::env::remove_var("AOS_DATABASE_URL");
        std::env::set_var("DATABASE_URL", "sqlite://var/db-legacy-only.sqlite3");
        let resolved = resolve_database_url().unwrap();
        assert_eq!(
            resolved.path,
            PathBuf::from("sqlite://var/db-legacy-only.sqlite3")
        );
        assert_eq!(resolved.source, PathSource::Env("DATABASE_URL"));
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    fn database_url_rejects_tmp_env() {
        std::env::set_var("DATABASE_URL", "/tmp/db.sqlite");
        let err = resolve_database_url().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("database-url"));
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    fn worker_socket_uses_env_override() {
        std::env::set_var("AOS_WORKER_SOCKET", "/tmp/worker.sock");
        let err = resolve_worker_socket_for_worker("tenant-x", None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be under /tmp"));
        std::env::remove_var("AOS_WORKER_SOCKET");
    }

    #[test]
    fn worker_socket_prefers_cli_over_env() {
        let cli_path = PathBuf::from("./var/test-cli.worker.sock");
        std::env::set_var("AOS_WORKER_SOCKET", "./var/test-env.worker.sock");

        let resolved =
            resolve_worker_socket_for_worker("tenant-x", Some(cli_path.as_path())).unwrap();
        assert_eq!(resolved.path, cli_path);
        assert_eq!(resolved.source, PathSource::Cli);

        std::env::remove_var("AOS_WORKER_SOCKET");
    }

    #[test]
    fn worker_socket_prefers_env_when_cli_missing() {
        std::env::set_var("AOS_WORKER_SOCKET", "./var/test-env-only.worker.sock");

        let resolved = resolve_worker_socket_for_worker("tenant-x", None).unwrap();
        assert_eq!(
            resolved.path,
            PathBuf::from("./var/test-env-only.worker.sock")
        );
        assert_eq!(resolved.source, PathSource::Env("AOS_WORKER_SOCKET"));

        std::env::remove_var("AOS_WORKER_SOCKET");
    }

    #[test]
    fn database_url_rejects_tmp_sqlite_scheme() {
        std::env::remove_var("DATABASE_URL");
        std::env::set_var("AOS_DATABASE_URL", "sqlite:///tmp/cp.db");
        let err = resolve_database_url().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("database-url"));
        std::env::remove_var("AOS_DATABASE_URL");
    }

    #[test]
    fn database_url_rejects_tmp_sqlite_single_slash_scheme() {
        std::env::remove_var("DATABASE_URL");
        std::env::set_var("AOS_DATABASE_URL", "sqlite:/tmp/cp.db");
        let err = resolve_database_url().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("database-url"));
        std::env::remove_var("AOS_DATABASE_URL");
    }

    #[test]
    fn database_url_rejects_tmp_file_scheme() {
        std::env::remove_var("DATABASE_URL");
        std::env::set_var("AOS_DATABASE_URL", "file:///private/tmp/cp.db");
        let err = resolve_database_url().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("database-url"));
        std::env::remove_var("AOS_DATABASE_URL");
    }

    #[test]
    fn control_plane_socket_rejects_tmp() {
        std::env::set_var("AOS_WORKER_SOCKET", "/tmp/cp.sock");
        let err = resolve_worker_socket_for_cp().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        std::env::remove_var("AOS_WORKER_SOCKET");
    }

    #[test]
    fn prepare_socket_removes_stale_and_creates_parent() {
        let tmp = TempDir::new_in(".").unwrap();
        let socket_path = tmp.path().join("nested/worker.sock");
        std::fs::create_dir_all(socket_path.parent().unwrap()).unwrap();
        std::fs::write(&socket_path, b"stale").unwrap();

        prepare_socket_path(&socket_path, "worker").expect("prepare should succeed");
        assert!(socket_path.parent().unwrap().exists());
        assert!(!socket_path.exists());

        // Ensure a fresh socket file can be created after cleanup.
        std::fs::File::create(&socket_path).expect("should be able to recreate socket file");
    }

    #[test]
    fn embedding_model_prefers_env() {
        let tmp = TempDir::new().unwrap();
        let embed_path = tmp.path().join("embed-model");
        fs::create_dir_all(&embed_path).unwrap();

        std::env::set_var("AOS_EMBEDDING_MODEL_PATH", &embed_path);
        let resolved = resolve_embedding_model_path();
        assert_eq!(resolved.path, embed_path);
        assert_eq!(resolved.source, PathSource::Env("AOS_EMBEDDING_MODEL_PATH"));
        std::env::remove_var("AOS_EMBEDDING_MODEL_PATH");
    }

    #[test]
    fn embedding_model_defaults_when_env_unset() {
        std::env::remove_var("AOS_EMBEDDING_MODEL_PATH");
        let resolved = resolve_embedding_model_path();
        assert_eq!(resolved.path, PathBuf::from(DEFAULT_EMBEDDING_MODEL_PATH));
        assert_eq!(resolved.source, PathSource::Default("embedding-model"));
    }
}
