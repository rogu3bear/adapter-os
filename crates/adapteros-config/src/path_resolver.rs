use adapteros_core::defaults::DEFAULT_SUPERVISOR_SIGNING_KEY_PATH;
pub use adapteros_core::defaults::{
    DEFAULT_ADAPTERS_ROOT, DEFAULT_BASE_MODEL_ID, DEFAULT_CP_WORKER_SOCKET, DEFAULT_DB_PATH,
    DEFAULT_EMBEDDING_MODEL_PATH, DEFAULT_INDEX_ROOT, DEFAULT_MANIFEST_CACHE_DIR,
    DEFAULT_MODEL_CACHE_ROOT, DEFAULT_QWEN_INT4_MANIFEST_DIR, DEFAULT_STATUS_PATH,
    DEFAULT_TELEMETRY_DIR, DEFAULT_WORKER_SOCKET_DEV, DEFAULT_WORKER_SOCKET_PROD_ROOT,
    DEV_MANIFEST_PATH, DEV_MODEL_PATH,
};
use adapteros_core::paths::AOS_ADAPTERS_DIR_ENV;
use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};

/// Absolute prefixes that are forbidden for system/local sockets.
const FORBIDDEN_TMP_PREFIXES: [&str; 2] = ["/tmp", "/private/tmp"];

/// Environment variable to disable symlink validation (testing/debug only).
const AOS_SKIP_SYMLINK_CHECK_ENV: &str = "AOS_SKIP_SYMLINK_CHECK";

// Default values are centralized in adapteros-core defaults to prevent drift.

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
/// 1. Explicit overrides (id_override, cache_root_override)
/// 2. Environment: AOS_BASE_MODEL_ID / AOS_MODEL_CACHE_DIR (canonical)
/// 3. Legacy: AOS_MODEL_PATH (deprecated, only if no canonical env vars set)
/// 4. Effective config (base_model.id / base_model.cache_root) if initialized
/// 5. Defaults: DEFAULT_BASE_MODEL_ID / DEFAULT_MODEL_CACHE_ROOT
pub fn resolve_base_model_location(
    id_override: Option<&str>,
    cache_root_override: Option<&Path>,
    require_existing: bool,
) -> Result<BaseModelLocation> {
    crate::model::load_dotenv();
    let effective = crate::effective::try_effective_config();

    // Check if canonical env vars are set
    let has_canonical_env =
        std::env::var("AOS_MODEL_CACHE_DIR").is_ok() || std::env::var("AOS_BASE_MODEL_ID").is_ok();

    // Legacy AOS_MODEL_PATH fallback (deprecated)
    // Only use if no canonical env vars and no overrides are provided
    if !has_canonical_env && id_override.is_none() && cache_root_override.is_none() {
        if let Ok(legacy_path) = std::env::var("AOS_MODEL_PATH") {
            let full_path = PathBuf::from(&legacy_path);
            reject_tmp_persistent_path(&full_path, "model-path")?;
            tracing::warn!(
                legacy_var = "AOS_MODEL_PATH",
                path = %full_path.display(),
                "Using deprecated AOS_MODEL_PATH. Please migrate to AOS_MODEL_CACHE_DIR and AOS_BASE_MODEL_ID."
            );

            // Extract model ID from path (last component)
            let id = full_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| DEFAULT_BASE_MODEL_ID.to_string());

            // Extract cache root (parent directory)
            let cache_root = full_path
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL_CACHE_ROOT));
            reject_tmp_persistent_path(&cache_root, "model-cache-root")?;

            if require_existing && !full_path.exists() {
                return Err(AosError::Config(format!(
                    "Model path does not exist: {}. Configure AOS_MODEL_CACHE_DIR and AOS_BASE_MODEL_ID or set base_model.cache_root/base_model.id in config.",
                    full_path.display()
                )));
            }

            return Ok(BaseModelLocation {
                id,
                cache_root,
                full_path,
            });
        }
    }

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

    reject_tmp_persistent_path(&cache_root, "model-cache-root")?;
    let full_path = cache_root.join(&id);
    reject_tmp_persistent_path(&full_path, "model-path")?;

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
pub fn resolve_embedding_model_path() -> Result<ResolvedPath> {
    resolve_embedding_model_path_with_override(None)
}

/// Resolve embedding model path with CLI/env/default precedence.
///
/// Precedence:
/// 1) CLI override (validated)
/// 2) AOS_EMBEDDING_MODEL_PATH
/// 3) Default: DEFAULT_EMBEDDING_MODEL_PATH
pub fn resolve_embedding_model_path_with_override(
    cli_override: Option<&Path>,
) -> Result<ResolvedPath> {
    crate::model::load_dotenv();

    if let Some(path) = cli_override {
        let path = path.to_path_buf();
        reject_tmp_persistent_path(&path, "embedding-model")?;
        tracing::info!(
            path = %path.display(),
            source = %PathSource::Cli,
            kind = %"embedding-model",
            "Resolved embedding model path from CLI override"
        );
        return Ok(ResolvedPath {
            path,
            source: PathSource::Cli,
            used_dev_fallback: false,
        });
    }

    resolve_env_or_default_no_tmp(
        "AOS_EMBEDDING_MODEL_PATH",
        DEFAULT_EMBEDDING_MODEL_PATH,
        "embedding-model",
    )
}

/// Resolve Qwen2.5 int4 manifest directory for MLX FFI.
///
/// Precedence:
/// 1) AOS_QWEN_INT4_DIR
/// 2) DEFAULT_QWEN_INT4_MANIFEST_DIR (if present)
pub fn resolve_qwen_int4_manifest_dir() -> Result<ResolvedPath> {
    crate::model::load_dotenv();
    if let Ok(val) = std::env::var("AOS_QWEN_INT4_DIR") {
        if !val.is_empty() {
            let path = PathBuf::from(&val);
            reject_tmp_persistent_path(&path, "qwen-int4-manifest-dir")?;
            tracing::info!(
                path = %path.display(),
                source = %PathSource::Env("AOS_QWEN_INT4_DIR"),
                "Resolved Qwen int4 manifest dir from environment"
            );
            return Ok(ResolvedPath {
                path,
                source: PathSource::Env("AOS_QWEN_INT4_DIR"),
                used_dev_fallback: false,
            });
        }
    }

    let default_path = PathBuf::from(DEFAULT_QWEN_INT4_MANIFEST_DIR);
    if default_path.exists() {
        reject_tmp_persistent_path(&default_path, "qwen-int4-manifest-dir")?;
        tracing::info!(
            path = %default_path.display(),
            source = %PathSource::Default("qwen-int4-manifest-dir"),
            "Using default Qwen int4 manifest dir"
        );
        return Ok(ResolvedPath {
            path: default_path,
            source: PathSource::Default("qwen-int4-manifest-dir"),
            used_dev_fallback: false,
        });
    }

    Err(AosError::Config(format!(
        "AOS_QWEN_INT4_DIR not set and default {} not found",
        DEFAULT_QWEN_INT4_MANIFEST_DIR
    )))
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
/// 3) ./var/run/aos/{tenant}/worker.sock (attempts to create parent)
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
/// 2) ./var/run/adapteros.sock
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

/// Check if symlink validation should be skipped (debug builds only).
fn should_skip_symlink_check() -> bool {
    cfg!(debug_assertions) && std::env::var(AOS_SKIP_SYMLINK_CHECK_ENV).is_ok()
}

/// String-based literal check for /tmp prefixes (fast path).
fn reject_tmp_literal(path_str: &str, kind: &str, original_path: &Path) -> Result<()> {
    if FORBIDDEN_TMP_PREFIXES
        .iter()
        .any(|prefix| path_str.starts_with(prefix))
    {
        return Err(AosError::Config(format!(
            "{} path must not be under /tmp: {}",
            kind,
            original_path.display()
        )));
    }
    Ok(())
}

/// Validate that a path does not resolve to /tmp, even through symlinks.
///
/// For existing paths: canonicalizes to resolve symlinks before checking.
/// For new paths: validates the parent directory if it exists.
fn validate_path_not_in_tmp(path: &Path, kind: &str) -> Result<()> {
    let path_str = path.display().to_string();

    // Fast path: check literal string first
    reject_tmp_literal(&path_str, kind, path)?;

    // Skip symlink resolution if explicitly disabled (testing only)
    if should_skip_symlink_check() {
        return Ok(());
    }

    // For existing paths, canonicalize to detect symlink attacks
    if path.exists() {
        match std::fs::canonicalize(path) {
            Ok(canonical) => {
                let canonical_str = canonical.display().to_string();
                if FORBIDDEN_TMP_PREFIXES
                    .iter()
                    .any(|prefix| canonical_str.starts_with(prefix))
                {
                    return Err(AosError::Config(format!(
                        "{} path must not resolve to /tmp: {} (symlink detected pointing to {})",
                        kind,
                        path.display(),
                        canonical.display()
                    )));
                }
            }
            Err(e) => {
                // Log warning but don't fail - path exists but can't canonicalize
                // This can happen with permission issues
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to canonicalize {} path for symlink check",
                    kind
                );
            }
        }
    } else if let Some(parent) = path.parent() {
        // For new paths, validate the parent if it exists
        if parent.exists() && !parent.as_os_str().is_empty() {
            if let Ok(canonical_parent) = std::fs::canonicalize(parent) {
                // Reconstruct path with canonical parent
                if let Some(file_name) = path.file_name() {
                    let canonical = canonical_parent.join(file_name);
                    let canonical_str = canonical.display().to_string();
                    if FORBIDDEN_TMP_PREFIXES
                        .iter()
                        .any(|prefix| canonical_str.starts_with(prefix))
                    {
                        return Err(AosError::Config(format!(
                            "{} path must not resolve to /tmp: {} (parent symlink detected pointing to {})",
                            kind,
                            path.display(),
                            canonical.display()
                        )));
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn reject_tmp_socket(path: &Path, kind: &str) -> Result<()> {
    validate_path_not_in_tmp(path, kind)
}

/// Reject paths under /tmp or /private/tmp for persistent storage.
/// This prevents accidental loss of critical runtime state on system restart.
/// Also detects symlink attacks where a path under /var resolves to /tmp.
pub fn reject_tmp_persistent_path(path: &Path, kind: &str) -> Result<()> {
    let path_str = path.display().to_string();
    let mut candidate = path_str.as_str();

    // Strip URL prefixes for database URLs
    for prefix in ["sqlite://", "sqlite:", "file://", "file:"] {
        if let Some(stripped) = candidate.strip_prefix(prefix) {
            candidate = stripped;
            break;
        }
    }

    while candidate.starts_with("//") {
        candidate = &candidate[1..];
    }

    // Fast path: check literal string
    reject_tmp_literal(candidate, kind, path)?;

    // Skip symlink resolution if explicitly disabled (testing only)
    if should_skip_symlink_check() {
        return Ok(());
    }

    // Canonicalize the cleaned path to detect symlink attacks
    let clean_path = Path::new(candidate);
    if clean_path.exists() {
        if let Ok(canonical) = std::fs::canonicalize(clean_path) {
            let canonical_str = canonical.display().to_string();
            if FORBIDDEN_TMP_PREFIXES
                .iter()
                .any(|prefix| canonical_str.starts_with(prefix))
            {
                return Err(AosError::Config(format!(
                    "{} path must not resolve to /tmp: {} (symlink detected pointing to {})",
                    kind,
                    path.display(),
                    canonical.display()
                )));
            }
        }
    } else if let Some(parent) = clean_path.parent() {
        // For new paths, validate parent
        if parent.exists() && !parent.as_os_str().is_empty() {
            if let Ok(canonical_parent) = std::fs::canonicalize(parent) {
                if let Some(file_name) = clean_path.file_name() {
                    let canonical = canonical_parent.join(file_name);
                    let canonical_str = canonical.display().to_string();
                    if FORBIDDEN_TMP_PREFIXES
                        .iter()
                        .any(|prefix| canonical_str.starts_with(prefix))
                    {
                        return Err(AosError::Config(format!(
                            "{} path must not resolve to /tmp: {} (parent symlink detected pointing to {})",
                            kind,
                            path.display(),
                            canonical.display()
                        )));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Resolve status file path for menu bar integration.
pub fn resolve_status_path() -> Result<ResolvedPath> {
    resolve_env_or_default_no_tmp("AOS_STATUS_PATH", DEFAULT_STATUS_PATH, "status-path")
}

/// Resolve supervisor signing key path with env/default provenance.
/// Rejects /tmp paths for security.
pub fn resolve_supervisor_signing_key_path() -> Result<ResolvedPath> {
    resolve_env_or_default_no_tmp(
        "AOS_SUPERVISOR_SIGNING_KEY_PATH",
        DEFAULT_SUPERVISOR_SIGNING_KEY_PATH,
        "supervisor-signing-key",
    )
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
            reject_tmp_persistent_path(&path, kind)?;
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
        reject_tmp_persistent_path(path, kind)?;
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
        reject_tmp_persistent_path(path, kind)?;
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
        reject_tmp_persistent_path(&path, kind)?;
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
    use crate::test_support::TestEnvGuard;
    use adapteros_core::defaults as core_defaults;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    #[test]
    fn path_defaults_match_core_defaults() {
        assert_eq!(DEV_MODEL_PATH, core_defaults::DEV_MODEL_PATH);
        assert_eq!(DEV_MANIFEST_PATH, core_defaults::DEV_MANIFEST_PATH);
        assert_eq!(
            DEFAULT_MODEL_CACHE_ROOT,
            core_defaults::DEFAULT_MODEL_CACHE_ROOT
        );
        assert_eq!(
            DEFAULT_EMBEDDING_MODEL_PATH,
            core_defaults::DEFAULT_EMBEDDING_MODEL_PATH
        );
        assert_eq!(DEFAULT_BASE_MODEL_ID, core_defaults::DEFAULT_BASE_MODEL_ID);
        assert_eq!(
            DEFAULT_QWEN_INT4_MANIFEST_DIR,
            core_defaults::DEFAULT_QWEN_INT4_MANIFEST_DIR
        );
        assert_eq!(DEFAULT_TELEMETRY_DIR, core_defaults::DEFAULT_TELEMETRY_DIR);
        assert_eq!(DEFAULT_INDEX_ROOT, core_defaults::DEFAULT_INDEX_ROOT);
        assert_eq!(
            DEFAULT_MANIFEST_CACHE_DIR,
            core_defaults::DEFAULT_MANIFEST_CACHE_DIR
        );
        assert_eq!(DEFAULT_ADAPTERS_ROOT, core_defaults::DEFAULT_ADAPTERS_ROOT);
        assert_eq!(
            DEFAULT_WORKER_SOCKET_PROD_ROOT,
            core_defaults::DEFAULT_WORKER_SOCKET_PROD_ROOT
        );
        assert_eq!(
            DEFAULT_WORKER_SOCKET_DEV,
            core_defaults::DEFAULT_WORKER_SOCKET_DEV
        );
        assert_eq!(
            DEFAULT_CP_WORKER_SOCKET,
            core_defaults::DEFAULT_CP_WORKER_SOCKET
        );
        assert_eq!(DEFAULT_STATUS_PATH, core_defaults::DEFAULT_STATUS_PATH);
        assert_eq!(
            DEFAULT_SUPERVISOR_SIGNING_KEY_PATH,
            core_defaults::DEFAULT_SUPERVISOR_SIGNING_KEY_PATH
        );
        assert_eq!(DEFAULT_DB_PATH, core_defaults::DEFAULT_DB_PATH);
    }

    #[test]
    fn env_beats_cli_and_config() {
        let _env = TestEnvGuard::new();
        let tmp = new_test_tempdir();
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
        let _env = TestEnvGuard::new();
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
        let _env = TestEnvGuard::new();
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
        let _env = TestEnvGuard::new();
        let tmp_root = new_test_tempdir();
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
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_MODEL_CACHE_DIR", "/tmp/cache-root");
        let err = resolve_base_model_location(None, None, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("model-cache-root"));
        std::env::remove_var("AOS_MODEL_CACHE_DIR");
    }

    #[test]
    fn base_model_rejects_private_tmp_env_cache_root() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_MODEL_CACHE_DIR", "/private/tmp/cache-root");
        let err = resolve_base_model_location(None, None, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("model-cache-root"));
        std::env::remove_var("AOS_MODEL_CACHE_DIR");
    }

    #[test]
    fn telemetry_dir_prefers_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_TELEMETRY_DIR", "./var/aos-telemetry-env");
        let resolved = resolve_telemetry_dir().unwrap();
        assert_eq!(resolved.path, PathBuf::from("./var/aos-telemetry-env"));
        assert_eq!(resolved.source, PathSource::Env("AOS_TELEMETRY_DIR"));
        std::env::remove_var("AOS_TELEMETRY_DIR");
    }

    #[test]
    fn telemetry_dir_rejects_tmp_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_TELEMETRY_DIR", "/tmp/aos-telemetry");
        let err = resolve_telemetry_dir().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("telemetry-dir"));
        std::env::remove_var("AOS_TELEMETRY_DIR");
    }

    #[test]
    fn telemetry_dir_rejects_private_tmp_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_TELEMETRY_DIR", "/private/tmp/aos-telemetry");
        let err = resolve_telemetry_dir().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("telemetry-dir"));
        std::env::remove_var("AOS_TELEMETRY_DIR");
    }

    #[test]
    fn manifest_cache_rejects_tmp_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_MANIFEST_CACHE_DIR", "/tmp/manifest-cache");
        let err = resolve_manifest_cache_dir().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("manifest-cache"));
        std::env::remove_var("AOS_MANIFEST_CACHE_DIR");
    }

    #[test]
    fn manifest_cache_rejects_tmp_uri_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_MANIFEST_CACHE_DIR", "file:///tmp/manifest-cache");
        let err = resolve_manifest_cache_dir().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("manifest-cache"));
        std::env::remove_var("AOS_MANIFEST_CACHE_DIR");
    }

    #[test]
    fn qwen_int4_manifest_dir_rejects_tmp_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_QWEN_INT4_DIR", "/tmp/qwen-int4");
        let err = resolve_qwen_int4_manifest_dir().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("qwen-int4-manifest-dir"));
        std::env::remove_var("AOS_QWEN_INT4_DIR");
    }

    #[test]
    fn qwen_int4_manifest_dir_rejects_private_tmp_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_QWEN_INT4_DIR", "/private/tmp/qwen-int4");
        let err = resolve_qwen_int4_manifest_dir().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("qwen-int4-manifest-dir"));
        std::env::remove_var("AOS_QWEN_INT4_DIR");
    }

    #[test]
    fn adapters_root_rejects_tmp_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var(AOS_ADAPTERS_ROOT_ENV, "/tmp/adapters");
        let err = resolve_adapters_root().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("adapters-root"));
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
    }

    #[test]
    fn adapters_root_rejects_tmp_legacy_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var(AOS_ADAPTERS_DIR_ENV, "/tmp/adapters");
        let err = resolve_adapters_root().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("adapters-root"));
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
    }

    #[test]
    fn index_root_rejects_tmp() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_INDEX_DIR", "/tmp/indices");
        let err = resolve_index_root().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("index-root"));
        std::env::remove_var("AOS_INDEX_DIR");
    }

    #[test]
    fn database_url_defaults_when_unset() {
        let _env = TestEnvGuard::new();
        std::env::remove_var("DATABASE_URL");
        std::env::remove_var("AOS_DATABASE_URL");
        let resolved = resolve_database_url().unwrap();
        assert_eq!(resolved.path, PathBuf::from(DEFAULT_DB_PATH));
        assert_eq!(resolved.source, PathSource::Default("database-url"));
    }

    #[test]
    fn database_url_prefers_primary_env() {
        let _env = TestEnvGuard::new();
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
        let _env = TestEnvGuard::new();
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
        let _env = TestEnvGuard::new();
        std::env::set_var("DATABASE_URL", "/tmp/db.sqlite");
        let err = resolve_database_url().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("database-url"));
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    fn worker_socket_uses_env_override() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_WORKER_SOCKET", "/tmp/worker.sock");
        let err = resolve_worker_socket_for_worker("tenant-x", None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be under /tmp"));
        std::env::remove_var("AOS_WORKER_SOCKET");
    }

    #[test]
    fn worker_socket_prefers_cli_over_env() {
        let _env = TestEnvGuard::new();
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
        let _env = TestEnvGuard::new();
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
        let _env = TestEnvGuard::new();
        std::env::remove_var("DATABASE_URL");
        std::env::set_var("AOS_DATABASE_URL", "sqlite:///tmp/cp.db");
        let err = resolve_database_url().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("database-url"));
        std::env::remove_var("AOS_DATABASE_URL");
    }

    #[test]
    fn database_url_rejects_tmp_sqlite_single_slash_scheme() {
        let _env = TestEnvGuard::new();
        std::env::remove_var("DATABASE_URL");
        std::env::set_var("AOS_DATABASE_URL", "sqlite:/tmp/cp.db");
        let err = resolve_database_url().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("database-url"));
        std::env::remove_var("AOS_DATABASE_URL");
    }

    #[test]
    fn database_url_rejects_tmp_file_scheme() {
        let _env = TestEnvGuard::new();
        std::env::remove_var("DATABASE_URL");
        std::env::set_var("AOS_DATABASE_URL", "file:///private/tmp/cp.db");
        let err = resolve_database_url().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("database-url"));
        std::env::remove_var("AOS_DATABASE_URL");
    }

    #[test]
    fn control_plane_socket_rejects_tmp() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_WORKER_SOCKET", "/tmp/cp.sock");
        let err = resolve_worker_socket_for_cp().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        std::env::remove_var("AOS_WORKER_SOCKET");
    }

    #[test]
    fn prepare_socket_removes_stale_and_creates_parent() {
        let _env = TestEnvGuard::new();
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
        let _env = TestEnvGuard::new();
        let tmp = new_test_tempdir();
        let embed_path = tmp.path().join("embed-model");
        fs::create_dir_all(&embed_path).unwrap();

        std::env::set_var("AOS_EMBEDDING_MODEL_PATH", &embed_path);
        let resolved = resolve_embedding_model_path().unwrap();
        assert_eq!(resolved.path, embed_path);
        assert_eq!(resolved.source, PathSource::Env("AOS_EMBEDDING_MODEL_PATH"));
        std::env::remove_var("AOS_EMBEDDING_MODEL_PATH");
    }

    #[test]
    fn embedding_model_defaults_when_env_unset() {
        let _env = TestEnvGuard::new();
        std::env::remove_var("AOS_EMBEDDING_MODEL_PATH");
        let resolved = resolve_embedding_model_path().unwrap();
        assert_eq!(resolved.path, PathBuf::from(DEFAULT_EMBEDDING_MODEL_PATH));
        assert_eq!(resolved.source, PathSource::Default("embedding-model"));
    }

    #[test]
    fn status_path_rejects_tmp_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_STATUS_PATH", "/tmp/adapteros_status.json");
        let err = resolve_status_path().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("status-path"));
        std::env::remove_var("AOS_STATUS_PATH");
    }

    #[test]
    fn status_path_rejects_private_tmp_env() {
        let _env = TestEnvGuard::new();
        std::env::set_var("AOS_STATUS_PATH", "/private/tmp/adapteros_status.json");
        let err = resolve_status_path().unwrap_err().to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("status-path"));
        std::env::remove_var("AOS_STATUS_PATH");
    }

    #[test]
    fn worker_manifest_rejects_tmp_path() {
        let path = PathBuf::from("/tmp/worker-manifest.json");
        let err = reject_tmp_persistent_path(&path, "worker-manifest")
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("worker-manifest"));
    }

    #[test]
    fn worker_manifest_rejects_private_tmp_path() {
        let path = PathBuf::from("/private/tmp/worker-manifest.json");
        let err = reject_tmp_persistent_path(&path, "worker-manifest")
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("worker-manifest"));
    }

    #[test]
    fn config_toml_rejects_tmp_path() {
        let path = PathBuf::from("/tmp/cp.toml");
        let err = reject_tmp_persistent_path(&path, "config-toml")
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("config-toml"));
    }

    #[test]
    fn config_toml_rejects_private_tmp_path() {
        let path = PathBuf::from("/private/tmp/cp.toml");
        let err = reject_tmp_persistent_path(&path, "config-toml")
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be under /tmp"));
        assert!(err.contains("config-toml"));
    }

    #[test]
    fn symlink_to_tmp_detected_for_socket() {
        let _env = TestEnvGuard::new();
        // Create a symlink from a safe-looking path to /tmp
        let tmp = new_test_tempdir();
        let link_path = tmp.path().join("safe-looking.sock");
        let target = PathBuf::from("/tmp/malicious.sock");

        // Create the target file so the symlink is valid
        if std::fs::write(&target, b"target").is_ok() {
            // Create symlink
            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                if symlink(&target, &link_path).is_ok() {
                    // The symlink should be detected and rejected
                    let err = reject_tmp_socket(&link_path, "test-socket")
                        .unwrap_err()
                        .to_string();
                    assert!(
                        err.contains("symlink detected")
                            || err.contains("must not resolve to /tmp")
                    );
                    assert!(err.contains("/tmp"));

                    // Cleanup
                    let _ = std::fs::remove_file(&link_path);
                }
            }
            let _ = std::fs::remove_file(&target);
        }
    }

    #[test]
    fn symlink_to_private_tmp_detected_for_persistent_path() {
        let _env = TestEnvGuard::new();
        // Create a symlink from a safe-looking path to /private/tmp
        let tmp = new_test_tempdir();
        let link_path = tmp.path().join("safe-db.sqlite");
        let target = PathBuf::from("/private/tmp/malicious.db");

        // Create the target file so the symlink is valid
        if std::fs::write(&target, b"target").is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                if symlink(&target, &link_path).is_ok() {
                    // The symlink should be detected and rejected
                    let err = reject_tmp_persistent_path(&link_path, "test-db")
                        .unwrap_err()
                        .to_string();
                    assert!(
                        err.contains("symlink detected")
                            || err.contains("must not resolve to /tmp")
                    );
                    assert!(err.contains("/tmp") || err.contains("/private/tmp"));

                    // Cleanup
                    let _ = std::fs::remove_file(&link_path);
                }
            }
            let _ = std::fs::remove_file(&target);
        }
    }

    #[test]
    fn parent_symlink_to_tmp_detected() {
        let _env = TestEnvGuard::new();
        // Test case: parent directory is a symlink to /tmp
        // Create /tmp/test-parent as target, symlink from var/tmp/xxx/parent -> /tmp/test-parent
        let tmp = new_test_tempdir();
        let target_parent = PathBuf::from("/tmp/symlink-test-parent");
        let link_parent = tmp.path().join("fake-parent");

        // Create target directory
        if std::fs::create_dir_all(&target_parent).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                if symlink(&target_parent, &link_parent).is_ok() {
                    // Try to use a path through the symlinked parent
                    let bad_path = link_parent.join("new-file.sock");

                    // The parent symlink should be detected
                    let err = reject_tmp_socket(&bad_path, "test-socket")
                        .unwrap_err()
                        .to_string();
                    assert!(
                        err.contains("symlink detected")
                            || err.contains("must not resolve to /tmp")
                    );

                    // Cleanup
                    let _ = std::fs::remove_file(&link_parent);
                }
            }
            let _ = std::fs::remove_dir_all(&target_parent);
        }
    }

    #[test]
    fn non_symlink_path_allowed() {
        let _env = TestEnvGuard::new();
        // A regular path that doesn't involve symlinks should pass
        let tmp = new_test_tempdir();
        let regular_path = tmp.path().join("regular.sock");

        // Should pass without error
        reject_tmp_socket(&regular_path, "test-socket")
            .expect("non-symlink path should be allowed");
    }

    #[test]
    fn skip_symlink_check_env_works() {
        let _env = TestEnvGuard::new();
        // When AOS_SKIP_SYMLINK_CHECK is set in debug builds, symlink checks should be skipped
        std::env::set_var("AOS_SKIP_SYMLINK_CHECK", "1");

        // The literal /tmp check should still work
        let tmp_path = PathBuf::from("/tmp/direct.sock");
        let err = reject_tmp_socket(&tmp_path, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be under /tmp"));

        std::env::remove_var("AOS_SKIP_SYMLINK_CHECK");
    }
}
