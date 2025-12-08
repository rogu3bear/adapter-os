use adapteros_core::paths::AOS_ADAPTERS_DIR_ENV;
use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};

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

/// Default SQLite path for the control plane database.
pub const DEFAULT_DB_PATH: &str = "./var/cp.db";

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
pub fn resolve_telemetry_dir() -> ResolvedPath {
    resolve_env_or_default("AOS_TELEMETRY_DIR", DEFAULT_TELEMETRY_DIR, "telemetry-dir")
}

/// Resolve index root directory with env/default provenance.
pub fn resolve_index_root() -> ResolvedPath {
    resolve_env_or_default("AOS_INDEX_DIR", DEFAULT_INDEX_ROOT, "index-root")
}

/// Resolve manifest cache directory with env/default provenance.
pub fn resolve_manifest_cache_dir() -> ResolvedPath {
    resolve_env_or_default(
        "AOS_MANIFEST_CACHE_DIR",
        DEFAULT_MANIFEST_CACHE_DIR,
        "manifest-cache",
    )
}

/// Resolve adapters root with support for both AOS_ADAPTERS_ROOT (preferred) and AOS_ADAPTERS_DIR (legacy).
pub fn resolve_adapters_root() -> ResolvedPath {
    crate::model::load_dotenv();
    if let Ok(env_path) = std::env::var(AOS_ADAPTERS_ROOT_ENV) {
        let path = PathBuf::from(&env_path);
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Env(AOS_ADAPTERS_ROOT_ENV),
            "Resolved adapters root from environment"
        );
        return ResolvedPath {
            path,
            source: PathSource::Env(AOS_ADAPTERS_ROOT_ENV),
            used_dev_fallback: false,
        };
    }

    if let Ok(env_path) = std::env::var(AOS_ADAPTERS_DIR_ENV) {
        let path = PathBuf::from(&env_path);
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Env(AOS_ADAPTERS_DIR_ENV),
            "Resolved adapters root from legacy environment variable"
        );
        return ResolvedPath {
            path,
            source: PathSource::Env(AOS_ADAPTERS_DIR_ENV),
            used_dev_fallback: false,
        };
    }

    let path = PathBuf::from(DEFAULT_ADAPTERS_ROOT);
    tracing::info!(
        path = %path.display(),
        source = %PathSource::Default("adapters-root"),
        "Using default adapters root"
    );

    ResolvedPath {
        path,
        source: PathSource::Default("adapters-root"),
        used_dev_fallback: false,
    }
}

/// Resolve database URL with env/default provenance.
pub fn resolve_database_url() -> ResolvedPath {
    crate::model::load_dotenv();
    if let Ok(url) = std::env::var("DATABASE_URL") {
        tracing::info!(
            database_url = %url,
            source = %PathSource::Env("DATABASE_URL"),
            "Resolved database URL from environment"
        );
        return ResolvedPath {
            path: PathBuf::from(url),
            source: PathSource::Env("DATABASE_URL"),
            used_dev_fallback: false,
        };
    }

    if let Ok(url) = std::env::var("AOS_DATABASE_URL") {
        tracing::info!(
            database_url = %url,
            source = %PathSource::Env("AOS_DATABASE_URL"),
            "Resolved database URL from legacy environment variable"
        );
        return ResolvedPath {
            path: PathBuf::from(url),
            source: PathSource::Env("AOS_DATABASE_URL"),
            used_dev_fallback: false,
        };
    }

    let path = PathBuf::from(DEFAULT_DB_PATH);
    tracing::info!(
        database_url = %path.display(),
        source = %PathSource::Default("database-url"),
        "Using default database URL"
    );

    ResolvedPath {
        path,
        source: PathSource::Default("database-url"),
        used_dev_fallback: false,
    }
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
) -> ResolvedPath {
    crate::model::load_dotenv();
    if let Some(path) = override_path {
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Cli,
            "Resolved worker socket from CLI/env override"
        );
        return ResolvedPath {
            path: path.to_path_buf(),
            source: PathSource::Cli,
            used_dev_fallback: false,
        };
    }

    if let Ok(env_path) = std::env::var("AOS_WORKER_SOCKET") {
        let path = PathBuf::from(&env_path);
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Env("AOS_WORKER_SOCKET"),
            "Resolved worker socket from environment"
        );
        return ResolvedPath {
            path,
            source: PathSource::Env("AOS_WORKER_SOCKET"),
            used_dev_fallback: false,
        };
    }

    let prod_path = PathBuf::from(format!(
        "{}/{}/worker.sock",
        DEFAULT_WORKER_SOCKET_PROD_ROOT.trim_end_matches('/'),
        tenant_id
    ));
    if let Some(parent) = prod_path.parent() {
        if std::fs::create_dir_all(parent).is_ok() {
            tracing::info!(
                path = %prod_path.display(),
                source = %PathSource::Default("worker-socket-prod"),
                "Using per-tenant worker socket path"
            );
            return ResolvedPath {
                path: prod_path,
                source: PathSource::Default("worker-socket-prod"),
                used_dev_fallback: false,
            };
        } else {
            tracing::warn!(
                path = %prod_path.display(),
                "Failed to create production worker socket directory, falling back to dev path"
            );
        }
    }

    let dev_path = PathBuf::from(DEFAULT_WORKER_SOCKET_DEV);
    if let Some(parent) = dev_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    tracing::info!(
        path = %dev_path.display(),
        source = %PathSource::DevFallback(DEFAULT_WORKER_SOCKET_DEV),
        "Using dev worker socket fallback"
    );

    ResolvedPath {
        path: dev_path,
        source: PathSource::DevFallback(DEFAULT_WORKER_SOCKET_DEV),
        used_dev_fallback: true,
    }
}

/// Resolve worker socket for control plane clients (training cancel, owner chat).
///
/// Precedence:
/// 1) AOS_WORKER_SOCKET
/// 2) /var/run/adapteros.sock
pub fn resolve_worker_socket_for_cp() -> ResolvedPath {
    crate::model::load_dotenv();
    if let Ok(env_path) = std::env::var("AOS_WORKER_SOCKET") {
        let path = PathBuf::from(&env_path);
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Env("AOS_WORKER_SOCKET"),
            "Resolved control-plane worker socket from environment"
        );
        return ResolvedPath {
            path,
            source: PathSource::Env("AOS_WORKER_SOCKET"),
            used_dev_fallback: false,
        };
    }

    let path = PathBuf::from(DEFAULT_CP_WORKER_SOCKET);
    tracing::info!(
        path = %path.display(),
        source = %PathSource::Default("worker-socket-cp"),
        "Using default control-plane worker socket"
    );
    ResolvedPath {
        path,
        source: PathSource::Default("worker-socket-cp"),
        used_dev_fallback: false,
    }
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
        std::env::set_var("AOS_TELEMETRY_DIR", "/tmp/aos-telemetry");
        let resolved = resolve_telemetry_dir();
        assert_eq!(resolved.path, PathBuf::from("/tmp/aos-telemetry"));
        assert_eq!(resolved.source, PathSource::Env("AOS_TELEMETRY_DIR"));
        std::env::remove_var("AOS_TELEMETRY_DIR");
    }

    #[test]
    fn database_url_defaults_when_unset() {
        std::env::remove_var("DATABASE_URL");
        std::env::remove_var("AOS_DATABASE_URL");
        let resolved = resolve_database_url();
        assert_eq!(resolved.path, PathBuf::from(DEFAULT_DB_PATH));
        assert_eq!(resolved.source, PathSource::Default("database-url"));
    }

    #[test]
    fn database_url_prefers_primary_env() {
        std::env::set_var("DATABASE_URL", "/tmp/db.sqlite");
        let resolved = resolve_database_url();
        assert_eq!(resolved.path, PathBuf::from("/tmp/db.sqlite"));
        assert_eq!(resolved.source, PathSource::Env("DATABASE_URL"));
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    fn worker_socket_uses_env_override() {
        std::env::set_var("AOS_WORKER_SOCKET", "/tmp/worker.sock");
        let resolved = resolve_worker_socket_for_worker("tenant-x", None);
        assert_eq!(resolved.path, PathBuf::from("/tmp/worker.sock"));
        assert_eq!(resolved.source, PathSource::Env("AOS_WORKER_SOCKET"));
        std::env::remove_var("AOS_WORKER_SOCKET");
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
