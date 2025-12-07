//! Shared adapter repository path helpers (repo + cache roots).
//!
//! Centralizes adapter bundle layout for both control plane (import) and
//! worker-side loading. Paths are always absolutized and follow the same
//! precedence rules:
//!
//! 1. `AOS_ADAPTERS_ROOT` (primary env)
//! 2. `AOS_ADAPTERS_DIR` (compat alias)
//! 3. Config-provided root (if any)
//! 4. Default `var/adapters/repo`
//!
//! Cache root uses `AOS_ADAPTER_CACHE_DIR` or defaults to `var/adapters/cache`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Primary environment variable for adapter repo root
pub const ENV_ADAPTERS_ROOT: &str = "AOS_ADAPTERS_ROOT";
/// Compatibility alias (legacy)
pub const ENV_ADAPTERS_DIR_COMPAT: &str = "AOS_ADAPTERS_DIR";
/// Environment variable for adapter cache root
pub const ENV_ADAPTER_CACHE_DIR: &str = "AOS_ADAPTER_CACHE_DIR";

pub const DEFAULT_REPO_DIR: &str = "var/adapters/repo";
pub const DEFAULT_CACHE_DIR: &str = "var/adapters/cache";

#[derive(Debug, Clone)]
pub struct AdapterPaths {
    pub repo_root: PathBuf,
    pub cache_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionStrategy {
    ExactOrError,
    LatestSemver,
    LatestLex,
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum ResolveError {
    #[error("invalid segment: {0}")]
    InvalidSegment(String),
    #[error("missing version for ExactOrError")]
    MissingVersion,
    #[error("no adapter bundle found")]
    NotFound,
    #[error("semver parse error: {0}")]
    Semver(String),
}

impl AdapterPaths {
    /// Versioned bundle path: {repo_root}/{tenant}/{adapter}/{version}.aos
    pub fn bundle_path(
        &self,
        tenant_id: &str,
        adapter_name: &str,
        version: &str,
    ) -> Result<PathBuf, ResolveError> {
        validate_segment(version)?;
        adapter_fs_path_with_root(&self.repo_root, tenant_id, adapter_name)
            .map(|base| base.join(format!("{}.aos", version)))
    }

    /// Flat bundle path: {repo_root}/{adapter}.aos (legacy layout)
    pub fn flat_bundle_path(&self, adapter_name: &str) -> PathBuf {
        self.repo_root.join(format!("{adapter_name}.aos"))
    }

    /// Cache path keyed by manifest hash with prefix directory sharding.
    pub fn cache_path(&self, manifest_hash: &str) -> PathBuf {
        let prefix = manifest_hash.get(0..2).unwrap_or("xx");
        self.cache_root
            .join(prefix)
            .join(format!("{manifest_hash}.aos"))
    }

    /// Resolve an existing bundle path with explicit version strategy.
    ///
    /// When `version` is `None`:
    /// - `LatestLex`: choose the lexicographically last `.aos` file in the tenant/adapter dir.
    /// - `LatestSemver`: choose the highest parsed `major.minor.patch` in the dir.
    /// - `ExactOrError`: return `MissingVersion`.
    pub fn resolve_existing_bundle(
        &self,
        tenant_id: &str,
        adapter_name: &str,
        version: Option<&str>,
        strategy: VersionStrategy,
    ) -> Result<PathBuf, ResolveError> {
        validate_segment(tenant_id)?;
        validate_segment(adapter_name)?;

        if let Some(ver) = version {
            return self
                .bundle_path(tenant_id, adapter_name, ver)
                .and_then(|p| validate_exists_or_not(&p));
        }

        match strategy {
            VersionStrategy::ExactOrError => Err(ResolveError::MissingVersion),
            VersionStrategy::LatestSemver => self.pick_latest_semver(tenant_id, adapter_name),
            VersionStrategy::LatestLex => self.pick_latest_lex(tenant_id, adapter_name),
        }
    }

    /// Runtime-safe resolution helper: requires explicit version and fails fast otherwise.
    ///
    /// Use this for runtime / worker loads to avoid accidental "latest" resolution.
    pub fn resolve_bundle_for_runtime(
        &self,
        tenant_id: &str,
        adapter_name: &str,
        version: Option<&str>,
    ) -> Result<PathBuf, ResolveError> {
        if version.is_none() {
            return Err(ResolveError::MissingVersion);
        }

        self.resolve_existing_bundle(
            tenant_id,
            adapter_name,
            version,
            VersionStrategy::ExactOrError,
        )
    }

    /// CLI convenience helper: resolve latest SemVer for operator tooling only.
    ///
    /// Do not use in the runtime hot path; runtime loads must call `resolve_bundle_for_runtime`.
    pub fn resolve_latest_semver_for_cli(
        &self,
        tenant_id: &str,
        adapter_name: &str,
    ) -> Result<PathBuf, ResolveError> {
        self.resolve_existing_bundle(tenant_id, adapter_name, None, VersionStrategy::LatestSemver)
    }

    /// Resolve adapter roots from environment variables (with compat alias) and
    /// optional config-provided root.
    pub fn from_env_and_config(config_root: Option<String>) -> Self {
        resolve_adapter_roots_from_strings(
            env::var(ENV_ADAPTERS_ROOT)
                .ok()
                .or_else(|| env::var(ENV_ADAPTERS_DIR_COMPAT).ok()),
            env::var(ENV_ADAPTER_CACHE_DIR).ok(),
            config_root,
        )
    }
}

fn absolutize(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    env::current_dir()
        .unwrap_or_else(|_| Path::new("/").to_path_buf())
        .join(path)
}

pub fn resolve_adapter_roots_from_strings(
    repo_env: Option<String>,
    cache_env: Option<String>,
    config_root: Option<String>,
) -> AdapterPaths {
    let repo_root = repo_env
        .or(config_root)
        .unwrap_or_else(|| DEFAULT_REPO_DIR.to_string());
    let cache_root = cache_env.unwrap_or_else(|| DEFAULT_CACHE_DIR.to_string());

    let repo_abs = absolutize(PathBuf::from(repo_root));
    let cache_abs = absolutize(PathBuf::from(cache_root));

    debug_assert!(repo_abs.is_absolute());
    debug_assert!(cache_abs.is_absolute());

    AdapterPaths {
        repo_root: repo_abs,
        cache_root: cache_abs,
    }
}

/// Canonical on-disk directory for an adapter (per-tenant)
pub fn adapter_fs_path(tenant_id: &str, adapter_id: &str) -> Result<PathBuf, ResolveError> {
    let roots = AdapterPaths::from_env_and_config(None);
    adapter_fs_path_with_root(roots.repo_root, tenant_id, adapter_id)
}

/// Canonical on-disk directory for an adapter with explicit repo root
pub fn adapter_fs_path_with_root(
    repo_root: impl AsRef<Path>,
    tenant_id: &str,
    adapter_id: &str,
) -> Result<PathBuf, ResolveError> {
    validate_segment(tenant_id)?;
    validate_segment(adapter_id)?;
    Ok(repo_root.as_ref().join(tenant_id).join(adapter_id))
}

fn validate_segment(segment: &str) -> Result<(), ResolveError> {
    if segment.is_empty()
        || segment.contains('/')
        || segment.contains('\\')
        || segment == "."
        || segment == ".."
    {
        return Err(ResolveError::InvalidSegment(segment.to_string()));
    }
    Ok(())
}

fn validate_exists_or_not(path: &Path) -> Result<PathBuf, ResolveError> {
    if path.exists() {
        Ok(path.to_path_buf())
    } else {
        Err(ResolveError::NotFound)
    }
}

impl AdapterPaths {
    fn pick_latest_lex(
        &self,
        tenant_id: &str,
        adapter_name: &str,
    ) -> Result<PathBuf, ResolveError> {
        let adapter_dir = self.repo_root.join(tenant_id).join(adapter_name);
        if let Ok(entries) = fs::read_dir(&adapter_dir) {
            let mut candidates: Vec<PathBuf> = entries
                .flatten()
                .filter_map(|e| {
                    let path = e.path();
                    (path.extension().is_some_and(|ext| ext == "aos")).then_some(path)
                })
                .collect();
            candidates.sort();
            if let Some(last) = candidates.into_iter().last() {
                return Ok(last);
            }
        }

        let flat = self.flat_bundle_path(adapter_name);
        validate_exists_or_not(&flat)
    }

    fn pick_latest_semver(
        &self,
        tenant_id: &str,
        adapter_name: &str,
    ) -> Result<PathBuf, ResolveError> {
        let adapter_dir = self.repo_root.join(tenant_id).join(adapter_name);
        let mut best: Option<(PathBuf, (u64, u64, u64))> = None;

        if let Ok(entries) = fs::read_dir(&adapter_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.extension().is_some_and(|ext| ext == "aos") {
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Some(vers) = parse_semver(stem) {
                        match best {
                            None => best = Some((path.clone(), vers)),
                            Some((_, ref current)) if vers > *current => {
                                best = Some((path.clone(), vers))
                            }
                            _ => {}
                        }
                    } else {
                        return Err(ResolveError::Semver(stem.to_string()));
                    }
                }
            }
        }

        if let Some((path, _)) = best {
            return Ok(path);
        }

        let flat = self.flat_bundle_path(adapter_name);
        validate_exists_or_not(&flat)
    }
}

fn parse_semver(input: &str) -> Option<(u64, u64, u64)> {
    let mut parts = input.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_overrides_are_absolute() {
        let repo = "/tmp/a-repo".to_string();
        let cache = "/tmp/a-cache".to_string();
        let paths =
            resolve_adapter_roots_from_strings(Some(repo.clone()), Some(cache.clone()), None);
        assert_eq!(paths.repo_root, PathBuf::from(repo));
        assert_eq!(paths.cache_root, PathBuf::from(cache));
        assert!(paths.repo_root.is_absolute());
        assert!(paths.cache_root.is_absolute());
    }

    #[test]
    fn config_fallback_used_when_env_missing() {
        let config_root = "/opt/aos/adapters".to_string();
        let paths = resolve_adapter_roots_from_strings(None, None, Some(config_root.clone()));
        assert_eq!(paths.repo_root, PathBuf::from(config_root));
        assert!(paths.cache_root.ends_with(DEFAULT_CACHE_DIR));
        assert!(paths.repo_root.is_absolute());
        assert!(paths.cache_root.is_absolute());
    }

    #[test]
    fn relative_paths_are_absolutized() {
        let paths = resolve_adapter_roots_from_strings(
            Some("relative/repo".into()),
            Some("c/cache".into()),
            None,
        );
        assert!(paths.repo_root.is_absolute());
        assert!(paths.cache_root.is_absolute());
    }

    #[test]
    fn bundle_path_layout_is_deterministic() {
        let base = PathBuf::from("/var/a");
        let paths = AdapterPaths {
            repo_root: base.clone(),
            cache_root: base.join("cache"),
        };
        let bundle = paths
            .bundle_path("t-spirit", "qwen-helper", "1.0.0")
            .unwrap();
        assert!(bundle.ends_with("t-spirit/qwen-helper/1.0.0.aos"));
    }

    #[test]
    fn compat_env_alias_is_honored() {
        unsafe {
            std::env::set_var(ENV_ADAPTERS_DIR_COMPAT, "/env/compat");
        }
        let paths = AdapterPaths::from_env_and_config(None);
        assert_eq!(paths.repo_root, PathBuf::from("/env/compat"));
        unsafe {
            std::env::remove_var(ENV_ADAPTERS_DIR_COMPAT);
        }
    }

    #[test]
    fn resolve_existing_lex_picks_latest() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let cache = temp.path().join("cache");
        fs::create_dir_all(repo.join("tenant").join("adapter")).unwrap();
        fs::write(repo.join("tenant").join("adapter").join("1.0.0.aos"), b"v1").unwrap();
        fs::write(
            repo.join("tenant").join("adapter").join("1.10.0.aos"),
            b"v110",
        )
        .unwrap();

        let paths = AdapterPaths {
            repo_root: repo,
            cache_root: cache,
        };

        let resolved = paths
            .resolve_existing_bundle("tenant", "adapter", None, VersionStrategy::LatestLex)
            .unwrap();
        assert!(resolved
            .to_string_lossy()
            .ends_with("tenant/adapter/1.10.0.aos"));
    }

    #[test]
    fn resolve_existing_falls_back_to_flat() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let cache = temp.path().join("cache");
        let tenant_flat = repo.join("tenant").join("adapter");
        fs::create_dir_all(&tenant_flat).unwrap();
        let flat_inner = tenant_flat.join("adapter.aos");
        fs::write(&flat_inner, b"flat").unwrap();

        let paths = AdapterPaths {
            repo_root: repo,
            cache_root: cache,
        };

        let resolved = paths
            .resolve_existing_bundle("tenant", "adapter", None, VersionStrategy::LatestLex)
            .unwrap();
        assert_eq!(resolved, flat_inner);
    }

    #[test]
    fn resolve_existing_semver_orders_correctly() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let cache = temp.path().join("cache");
        fs::create_dir_all(repo.join("tenant").join("adapter")).unwrap();
        fs::write(
            repo.join("tenant").join("adapter").join("1.9.9.aos"),
            b"v199",
        )
        .unwrap();
        fs::write(
            repo.join("tenant").join("adapter").join("1.10.0.aos"),
            b"v1100",
        )
        .unwrap();

        let paths = AdapterPaths {
            repo_root: repo,
            cache_root: cache,
        };

        let resolved = paths
            .resolve_existing_bundle("tenant", "adapter", None, VersionStrategy::LatestSemver)
            .unwrap();
        assert!(resolved
            .to_string_lossy()
            .ends_with("tenant/adapter/1.10.0.aos"));
    }

    #[test]
    fn bundle_path_rejects_slashes() {
        let base = PathBuf::from("/var/a");
        let paths = AdapterPaths {
            repo_root: base.clone(),
            cache_root: base.join("cache"),
        };
        let err = paths.bundle_path("ten/ant", "name", "1.0.0").unwrap_err();
        assert!(matches!(err, ResolveError::InvalidSegment(_)));
    }

    #[test]
    fn resolve_existing_rejects_traversal() {
        let base = PathBuf::from("/var/a");
        let paths = AdapterPaths {
            repo_root: base.clone(),
            cache_root: base.join("cache"),
        };
        let err = paths
            .resolve_existing_bundle("../tenant", "name", None, VersionStrategy::LatestLex)
            .unwrap_err();
        assert!(matches!(err, ResolveError::InvalidSegment(_)));
    }

    #[test]
    fn missing_version_with_exact_errors() {
        let base = PathBuf::from("/var/a");
        let paths = AdapterPaths {
            repo_root: base.clone(),
            cache_root: base.join("cache"),
        };
        let err = paths
            .resolve_existing_bundle("tenant", "name", None, VersionStrategy::ExactOrError)
            .unwrap_err();
        assert!(matches!(err, ResolveError::MissingVersion));
    }

    #[test]
    fn resolve_bundle_for_runtime_requires_version() {
        let base = PathBuf::from("/var/a");
        let paths = AdapterPaths {
            repo_root: base.clone(),
            cache_root: base.join("cache"),
        };

        let err = paths.resolve_bundle_for_runtime("tenant", "name", None);
        assert!(matches!(err, Err(ResolveError::MissingVersion)));
    }

    #[test]
    fn resolve_latest_semver_for_cli_picks_highest() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let cache = temp.path().join("cache");
        fs::create_dir_all(repo.join("tenant").join("adapter")).unwrap();
        fs::write(repo.join("tenant").join("adapter").join("1.0.0.aos"), b"v1").unwrap();
        fs::write(repo.join("tenant").join("adapter").join("1.2.0.aos"), b"v2").unwrap();

        let paths = AdapterPaths {
            repo_root: repo.clone(),
            cache_root: cache,
        };

        let resolved = paths
            .resolve_latest_semver_for_cli("tenant", "adapter")
            .unwrap();
        assert_eq!(
            resolved,
            repo.join("tenant").join("adapter").join("1.2.0.aos")
        );
    }

    #[test]
    fn env_compat_alias_used_when_set() {
        unsafe {
            std::env::set_var(ENV_ADAPTERS_DIR_COMPAT, "/env/compat");
        }
        let paths = AdapterPaths::from_env_and_config(None);
        assert_eq!(paths.repo_root, PathBuf::from("/env/compat"));
        unsafe {
            std::env::remove_var(ENV_ADAPTERS_DIR_COMPAT);
        }
    }

    #[test]
    fn env_primary_adapters_dir_is_used() {
        unsafe {
            std::env::set_var(ENV_ADAPTERS_ROOT, "/env/primary");
        }
        let paths = AdapterPaths::from_env_and_config(None);
        assert_eq!(paths.repo_root, PathBuf::from("/env/primary"));
        unsafe {
            std::env::remove_var(ENV_ADAPTERS_ROOT);
        }
    }
}
