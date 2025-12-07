use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};

/// Dev-only fixture path for the default local Qwen2.5-7B-Instruct-4bit model.
pub const DEV_MODEL_PATH: &str = "./var/models/Qwen2.5-7B-Instruct-4bit";

/// Dev-only fixture path for the default local Qwen2.5-7B-Instruct-4bit manifest (config.json).
pub const DEV_MANIFEST_PATH: &str = "./var/models/Qwen2.5-7B-Instruct-4bit/config.json";

/// Source describing where a resolved path originated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSource {
    Env(&'static str),
    Cli,
    Config(&'static str),
    DevFallback(&'static str),
}

impl std::fmt::Display for PathSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathSource::Env(var) => write!(f, "env:{}", var),
            PathSource::Cli => write!(f, "cli"),
            PathSource::Config(key) => write!(f, "config:{}", key),
            PathSource::DevFallback(label) => write!(f, "dev-fallback:{}", label),
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
    }
}
