use crate::runtime_config_store;
use adapteros_config::resolve_base_model_location;
use adapteros_db::Db;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const DISCOVERY_ROOTS_ENV: &str = "AOS_MODEL_DISCOVERY_ROOTS";

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    PathBuf::from(path)
}

pub fn parse_discovery_roots(raw: &str) -> Vec<PathBuf> {
    raw.split([',', ';', '\n'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(expand_tilde)
        .collect()
}

fn push_if_valid_root(roots: &mut Vec<PathBuf>, path: PathBuf) {
    if path.exists() && path.is_dir() {
        roots.push(path);
    }
}

fn canonicalize_or_keep(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn dedupe_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for root in roots {
        let canonical = canonicalize_or_keep(&root);
        if seen.insert(canonical.clone()) {
            deduped.push(canonical);
        }
    }
    deduped
}

/// Resolve baseline discovery roots from canonical env/default lanes.
///
/// This is used by both runtime path enforcement and Settings UI defaults.
pub fn default_model_discovery_roots() -> Result<Vec<PathBuf>, String> {
    let location = resolve_base_model_location(None, None, false).map_err(|e| e.to_string())?;

    if !location.cache_root.exists() {
        std::fs::create_dir_all(&location.cache_root).map_err(|e| {
            format!(
                "Failed to create model cache root {}: {}",
                location.cache_root.display(),
                e
            )
        })?;
    }

    let mut roots = vec![location.cache_root.clone()];

    if let Ok(raw) = std::env::var(DISCOVERY_ROOTS_ENV) {
        for path in parse_discovery_roots(&raw) {
            push_if_valid_root(&mut roots, path);
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let lmstudio_root = PathBuf::from(home).join(".lmstudio/models");
        push_if_valid_root(&mut roots, lmstudio_root);
    }

    Ok(dedupe_roots(roots))
}

fn settings_model_roots(raw_roots: &[String]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for raw in raw_roots {
        for path in parse_discovery_roots(raw) {
            push_if_valid_root(&mut roots, path);
        }
    }
    dedupe_roots(roots)
}

/// Resolve model roots allowed for discovery, setup import, and runtime model path validation.
///
/// Priority:
/// 1. Canonical base model cache root (AOS_MODEL_CACHE_DIR + AOS_BASE_MODEL_ID lane)
/// 2. Runtime settings override (`settings.models.discovery_roots`) when configured
/// 3. Environment discovery roots / LM Studio fallback defaults
pub async fn resolve_model_allowed_roots(db: Option<&Db>) -> Result<Vec<PathBuf>, String> {
    let mut roots = default_model_discovery_roots()?;

    if let Some(db) = db {
        if let Some(loaded) = runtime_config_store::load_runtime_config(db).await? {
            if let Some(models) = loaded.document.settings.models {
                let mut override_roots = settings_model_roots(&models.discovery_roots);
                roots.append(&mut override_roots);
            }
        }
    }

    Ok(dedupe_roots(roots))
}
