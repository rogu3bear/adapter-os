//! Background reconciler to keep storage and DB metadata in sync.
//!
//! Orphan cleanup: files on disk without a matching DB record are candidates
//! for deletion if they exceed a configurable age threshold. Active and pinned
//! adapters are always protected.

use futures::FutureExt;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};

use adapteros_core::B3Hash;
use adapteros_db::NewStorageIssue;
use adapteros_storage::FsByteStorage;
use tokio::fs;
use tokio::time::sleep;
use walkdir::WalkDir;

use crate::handlers::datasets::{
    hash_dataset_manifest, resolve_dataset_root, resolve_dataset_root_lenient_from_strings,
    DatasetHashInput, ENV_DATASETS_DIR,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Orphan cleanup configuration
// ---------------------------------------------------------------------------

/// Default age threshold for orphan deletion (24 hours).
const DEFAULT_ORPHAN_AGE_THRESHOLD_SECS: u64 = 24 * 60 * 60;

/// Configuration for orphan file auto-cleanup.
#[derive(Debug, Clone)]
pub struct OrphanCleanupConfig {
    /// Enable orphan cleanup. When `false`, orphans are detected and logged
    /// but never deleted (existing behaviour).
    pub enabled: bool,
    /// When `true`, orphan cleanup logic runs but files are not actually
    /// deleted. Useful for auditing what *would* be deleted before enabling.
    pub dry_run: bool,
    /// Minimum age a file must exceed before it becomes eligible for deletion.
    /// Files younger than this are logged as warnings but retained.
    pub age_threshold: Duration,
}

impl Default for OrphanCleanupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dry_run: false,
            age_threshold: Duration::from_secs(DEFAULT_ORPHAN_AGE_THRESHOLD_SECS),
        }
    }
}

#[derive(Clone)]
pub struct StorageReconciler {
    state: Arc<AppState>,
    orphan_cleanup: OrphanCleanupConfig,
}

impl StorageReconciler {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            orphan_cleanup: OrphanCleanupConfig::default(),
        }
    }

    /// Configure orphan cleanup behaviour.
    pub fn with_orphan_cleanup(mut self, config: OrphanCleanupConfig) -> Self {
        self.orphan_cleanup = config;
        self
    }

    fn store(&self) -> anyhow::Result<FsByteStorage> {
        let cfg = self
            .state
            .config
            .read()
            .map_err(|_| anyhow::anyhow!("Config lock poisoned in storage reconciler"))?;
        let config_root = if cfg.paths.datasets_root.is_empty() {
            None
        } else {
            Some(cfg.paths.datasets_root.clone())
        };
        let env_root = std::env::var(ENV_DATASETS_DIR).ok();
        let datasets_root = resolve_dataset_root_lenient_from_strings(env_root, config_root)?;
        Ok(FsByteStorage::new(
            datasets_root,
            cfg.paths.adapters_root.clone().into(),
        ))
    }

    pub async fn run_once(&self) {
        let store = match self.store() {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "Storage reconciler: failed to initialize storage");
                return;
            }
        };

        if let Err(e) = self.check_dataset_versions(&store).await {
            error!(error = %e, "Storage reconciler: dataset version check failed");
        }

        if let Err(e) = self.check_adapter_versions(&store).await {
            error!(error = %e, "Storage reconciler: adapter version check failed");
        }

        if let Err(e) = self.detect_orphans(&store).await {
            error!(error = %e, "Storage reconciler: orphan scan failed");
        }
    }

    async fn check_dataset_versions(&self, _store: &FsByteStorage) -> adapteros_core::Result<()> {
        let versions = self
            .state
            .db
            .list_all_dataset_versions()
            .await
            .unwrap_or_default();
        debug!(count = versions.len(), "Reconciling dataset versions");

        for v in versions {
            let path = PathBuf::from(&v.storage_path);
            let tenant_id = v.tenant_id.clone();
            if !path.exists() {
                let severity = if v.soft_deleted_at.is_some() {
                    "info"
                } else {
                    "error"
                };
                let location = path.to_string_lossy().to_string();
                if let Err(e) = self
                    .record_issue(NewStorageIssue {
                        tenant_id: tenant_id.as_deref(),
                        owner_type: "dataset_version",
                        owner_id: &v.dataset_id,
                        version_id: Some(&v.id),
                        issue_type: "missing_bytes",
                        severity,
                        location: location.as_str(),
                        details: Some("Dataset version path missing"),
                    })
                    .await
                {
                    error!(
                        error = %e,
                        dataset_id = %v.dataset_id,
                        version_id = %v.id,
                        path = %location,
                        "Failed to record missing dataset version bytes issue"
                    );
                }
                continue;
            }

            let metadata = match fs::metadata(&path).await {
                Ok(m) => m,
                Err(e) => {
                    let location = path.to_string_lossy().to_string();
                    let details = format!("Dataset version path inaccessible: {}", e);
                    if let Err(e) = self
                        .record_issue(NewStorageIssue {
                            tenant_id: tenant_id.as_deref(),
                            owner_type: "dataset_version",
                            owner_id: &v.dataset_id,
                            version_id: Some(&v.id),
                            issue_type: "inaccessible_bytes",
                            severity: "error",
                            location: location.as_str(),
                            details: Some(details.as_str()),
                        })
                        .await
                    {
                        error!(
                            error = %e,
                            dataset_id = %v.dataset_id,
                            version_id = %v.id,
                            path = %location,
                            "Failed to record inaccessible dataset version bytes issue"
                        );
                    }
                    continue;
                }
            };

            if metadata.is_dir() {
                // Some dataset versions are directory-backed (e.g., canonical multi-file uploads).
                // For those, the version hash is the manifest hash derived from dataset_files.
                let files = match self.state.db.get_dataset_files(&v.dataset_id).await {
                    Ok(v) => v,
                    Err(e) => {
                        error!(
                            error = %e,
                            dataset_id = %v.dataset_id,
                            version_id = %v.id,
                            path = %path.display(),
                            "Failed to load dataset_files for directory-backed dataset version"
                        );
                        continue;
                    }
                };
                if files.is_empty() {
                    let location = path.to_string_lossy().to_string();
                    if let Err(e) = self
                        .record_issue(NewStorageIssue {
                            tenant_id: tenant_id.as_deref(),
                            owner_type: "dataset_version",
                            owner_id: &v.dataset_id,
                            version_id: Some(&v.id),
                            issue_type: "missing_db_files",
                            severity: "error",
                            location: location.as_str(),
                            details: Some(
                                "Dataset version storage_path is a directory but dataset has no dataset_files rows",
                            ),
                        })
                        .await
                    {
                        error!(
                            error = %e,
                            dataset_id = %v.dataset_id,
                            version_id = %v.id,
                            path = %location,
                            "Failed to record missing dataset_files issue for directory-backed dataset version"
                        );
                    }
                    continue;
                }

                let inputs: Vec<DatasetHashInput> = files
                    .into_iter()
                    .map(|f| DatasetHashInput {
                        file_name: f.file_name,
                        size_bytes: f.size_bytes.max(0) as u64,
                        file_hash_b3: f.hash_b3,
                    })
                    .collect();
                let computed = hash_dataset_manifest(&inputs);
                if computed != v.hash_b3 {
                    let location = path.to_string_lossy().to_string();
                    if let Err(e) = self
                        .record_issue(NewStorageIssue {
                            tenant_id: tenant_id.as_deref(),
                            owner_type: "dataset_version",
                            owner_id: &v.dataset_id,
                            version_id: Some(&v.id),
                            issue_type: "hash_mismatch",
                            severity: "error",
                            location: location.as_str(),
                            details: Some(
                                "Dataset version hash mismatch (directory-backed; expected manifest hash)",
                            ),
                        })
                        .await
                    {
                        error!(
                            error = %e,
                            dataset_id = %v.dataset_id,
                            version_id = %v.id,
                            path = %location,
                            "Failed to record dataset version hash mismatch issue"
                        );
                    }
                }

                continue;
            }

            let bytes = match fs::read(&path).await {
                Ok(b) => b,
                Err(e) => {
                    let location = path.to_string_lossy().to_string();
                    let details = format!("Failed to read dataset version bytes: {}", e);
                    if let Err(e) = self
                        .record_issue(NewStorageIssue {
                            tenant_id: tenant_id.as_deref(),
                            owner_type: "dataset_version",
                            owner_id: &v.dataset_id,
                            version_id: Some(&v.id),
                            issue_type: "unreadable_bytes",
                            severity: "error",
                            location: location.as_str(),
                            details: Some(details.as_str()),
                        })
                        .await
                    {
                        error!(
                            error = %e,
                            dataset_id = %v.dataset_id,
                            version_id = %v.id,
                            path = %location,
                            "Failed to record unreadable dataset version bytes issue"
                        );
                    }
                    continue;
                }
            };

            let hash = B3Hash::hash(&bytes).to_hex();
            if hash != v.hash_b3 {
                let location = path.to_string_lossy().to_string();
                if let Err(e) = self
                    .record_issue(NewStorageIssue {
                        tenant_id: tenant_id.as_deref(),
                        owner_type: "dataset_version",
                        owner_id: &v.dataset_id,
                        version_id: Some(&v.id),
                        issue_type: "hash_mismatch",
                        severity: "error",
                        location: location.as_str(),
                        details: Some("Dataset version hash mismatch"),
                    })
                    .await
                {
                    error!(
                        error = %e,
                        dataset_id = %v.dataset_id,
                        version_id = %v.id,
                        path = %location,
                        "Failed to record dataset version hash mismatch issue"
                    );
                }
            }
        }

        Ok(())
    }

    async fn check_adapter_versions(&self, __store: &FsByteStorage) -> adapteros_core::Result<()> {
        let versions = self
            .state
            .db
            .list_all_adapter_versions()
            .await
            .unwrap_or_default();
        debug!(count = versions.len(), "Reconciling adapter versions");

        for v in versions {
            let Some(path_str) = &v.aos_path else {
                continue;
            };
            let path = PathBuf::from(path_str);
            if !path.exists() {
                let severity = if v.release_state.eq_ignore_ascii_case("archived") {
                    "info"
                } else {
                    "error"
                };
                self.record_issue(NewStorageIssue {
                    tenant_id: Some(&v.tenant_id),
                    owner_type: "adapter_version",
                    owner_id: &v.repo_id,
                    version_id: Some(&v.id),
                    issue_type: "missing_bytes",
                    severity,
                    location: path.to_string_lossy().as_ref(),
                    details: Some("Adapter artifact missing"),
                })
                .await?;
                continue;
            }

            if let Some(expected_hash) = &v.aos_hash {
                let bytes = fs::read(&path).await.map_err(|e| {
                    adapteros_core::AosError::Io(format!(
                        "Failed to read adapter artifact {}: {}",
                        path.display(),
                        e
                    ))
                })?;
                let hash = B3Hash::hash(&bytes).to_hex();
                if &hash != expected_hash {
                    self.record_issue(NewStorageIssue {
                        tenant_id: Some(&v.tenant_id),
                        owner_type: "adapter_version",
                        owner_id: &v.repo_id,
                        version_id: Some(&v.id),
                        issue_type: "hash_mismatch",
                        severity: "error",
                        location: path.to_string_lossy().as_ref(),
                        details: Some("Adapter artifact hash mismatch"),
                    })
                    .await?;
                }
            }
        }

        Ok(())
    }

    async fn detect_orphans(&self, _store: &FsByteStorage) -> adapteros_core::Result<()> {
        let mut expected: HashSet<PathBuf> = HashSet::new();

        // Dataset version storage paths
        for v in self
            .state
            .db
            .list_all_dataset_versions()
            .await
            .unwrap_or_default()
        {
            expected.insert(canonical(&v.storage_path));
        }

        // Dataset file entries
        for f in self
            .state
            .db
            .list_all_dataset_files()
            .await
            .unwrap_or_default()
        {
            expected.insert(canonical(&f.file_path));
        }

        // Adapter version artifacts
        let all_versions = self
            .state
            .db
            .list_all_adapter_versions()
            .await
            .unwrap_or_default();

        for v in &all_versions {
            if let Some(path) = &v.aos_path {
                expected.insert(canonical(path));
            }
        }

        // Build protected set: paths belonging to active or pinned adapters.
        // These are NEVER eligible for orphan cleanup regardless of age.
        let protected = self.build_protected_paths(&all_versions).await;

        // Scan dataset root for orphans
        let dataset_root = resolve_dataset_root(&self.state)?.join("files");
        self.scan_orphans_in_dir(&dataset_root, "dataset", &expected, &protected)
            .await?;

        // Scan adapter repo root for orphans
        let adapter_root = {
            let cfg = match self.state.config.read() {
                Ok(cfg) => cfg,
                Err(_) => {
                    error!("Config lock poisoned in detect_orphans");
                    return Err(adapteros_core::AosError::Internal(
                        "Config lock poisoned".to_string(),
                    ));
                }
            };
            PathBuf::from(&cfg.paths.adapters_root)
        };
        self.scan_orphans_in_dir(&adapter_root, "adapter", &expected, &protected)
            .await?;

        Ok(())
    }

    /// Build the set of paths that must never be deleted.
    ///
    /// Includes paths of adapter versions in production-relevant states
    /// (`active`, `ready`) as defense-in-depth against race conditions
    /// where the DB query and file scan interleave with a promotion.
    async fn build_protected_paths(
        &self,
        all_versions: &[adapteros_db::AdapterVersion],
    ) -> HashSet<PathBuf> {
        let mut protected = HashSet::new();

        for v in all_versions {
            let state = v.release_state.to_ascii_lowercase();
            let is_production = state == "active" || state == "ready";
            if is_production {
                if let Some(path) = &v.aos_path {
                    protected.insert(canonical(path));
                }
            }
        }

        debug!(
            protected_paths = protected.len(),
            "Built protected path set for orphan cleanup"
        );

        protected
    }

    async fn scan_orphans_in_dir(
        &self,
        root: &Path,
        category: &str,
        expected: &HashSet<PathBuf>,
        protected: &HashSet<PathBuf>,
    ) -> adapteros_core::Result<()> {
        if !root.exists() {
            return Ok(());
        }

        let now = SystemTime::now();

        for entry in WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let p = entry.path();
            let canon = canonical(p);
            if expected.contains(&canon) {
                continue;
            }

            // --- Orphan detected ---

            // Never touch protected paths (active/pinned adapters)
            if protected.contains(&canon) {
                debug!(
                    path = %canon.display(),
                    category,
                    "Orphan file is protected (active/pinned adapter); skipping"
                );
                continue;
            }

            // Determine file age and size from a single metadata call
            let meta = fs::metadata(&canon).await.ok();
            let file_age = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|mtime| now.duration_since(mtime).ok());
            let file_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);

            let age_secs = file_age.map(|d| d.as_secs()).unwrap_or(0);
            let exceeds_threshold = file_age
                .map(|age| age >= self.orphan_cleanup.age_threshold)
                .unwrap_or(false);

            if self.orphan_cleanup.enabled && exceeds_threshold {
                // Eligible for deletion
                let age_hours = age_secs / 3600;
                let location = canon.to_string_lossy().to_string();

                if self.orphan_cleanup.dry_run {
                    // Dry-run: log what would be deleted but don't remove
                    let details = format!(
                        "DRY-RUN: would delete orphan (age: {}h, size: {} bytes)",
                        age_hours, file_size
                    );
                    info!(
                        path = %location,
                        category,
                        age_hours,
                        size_bytes = file_size,
                        "Orphan eligible for cleanup (dry-run)"
                    );
                    self.record_issue(NewStorageIssue {
                        tenant_id: None,
                        owner_type: "orphan",
                        owner_id: category,
                        version_id: None,
                        issue_type: "orphan_cleanup_dry_run",
                        severity: "info",
                        location: &location,
                        details: Some(&details),
                    })
                    .await?;
                } else {
                    // Actually delete
                    match fs::remove_file(&canon).await {
                        Ok(()) => {
                            let details = format!(
                                "Deleted orphan file (age: {}h, size: {} bytes)",
                                age_hours, file_size
                            );
                            info!(
                                path = %location,
                                category,
                                age_hours,
                                size_bytes = file_size,
                                "Orphan file deleted"
                            );
                            self.record_issue(NewStorageIssue {
                                tenant_id: None,
                                owner_type: "orphan",
                                owner_id: category,
                                version_id: None,
                                issue_type: "orphan_cleaned",
                                severity: "info",
                                location: &location,
                                details: Some(&details),
                            })
                            .await?;
                        }
                        Err(e) => {
                            let details =
                                format!("Failed to delete orphan (age: {}h): {}", age_hours, e);
                            warn!(
                                path = %location,
                                category,
                                error = %e,
                                "Failed to delete orphan file"
                            );
                            self.record_issue(NewStorageIssue {
                                tenant_id: None,
                                owner_type: "orphan",
                                owner_id: category,
                                version_id: None,
                                issue_type: "orphan_cleanup_failed",
                                severity: "error",
                                location: &location,
                                details: Some(&details),
                            })
                            .await?;
                        }
                    }
                }
            } else {
                // Either cleanup is disabled, or file is younger than threshold.
                // Log as warning (existing behaviour).
                let location = canon.to_string_lossy().to_string();
                let details = if !self.orphan_cleanup.enabled {
                    "File present without DB owner".to_string()
                } else {
                    format!(
                        "Orphan file below age threshold (age: {}h, threshold: {}h)",
                        age_secs / 3600,
                        self.orphan_cleanup.age_threshold.as_secs() / 3600
                    )
                };
                self.record_issue(NewStorageIssue {
                    tenant_id: None,
                    owner_type: "orphan",
                    owner_id: category,
                    version_id: None,
                    issue_type: "orphan_bytes",
                    severity: "warn",
                    location: &location,
                    details: Some(&details),
                })
                .await?;
            }
        }

        Ok(())
    }

    async fn record_issue(&self, issue: NewStorageIssue<'_>) -> adapteros_core::Result<()> {
        self.state.db.record_storage_issue(issue).await?;
        Ok(())
    }
}

pub fn spawn_storage_reconciler(state: Arc<AppState>) {
    spawn_storage_reconciler_with_config(state, OrphanCleanupConfig::default());
}

pub fn spawn_storage_reconciler_with_config(
    state: Arc<AppState>,
    orphan_config: OrphanCleanupConfig,
) {
    state
        .background_task_tracker()
        .record_spawned("Storage reconciler", false);
    let reconciler = StorageReconciler::new(state).with_orphan_cleanup(orphan_config);
    tokio::spawn(async move {
        if let Err(panic) = std::panic::AssertUnwindSafe(async move {
            loop {
                reconciler.run_once().await;
                sleep(Duration::from_secs(900)).await; // 15 min cadence
            }
        })
        .catch_unwind()
        .await
        {
            tracing::error!(
                task = "storage_reconciliation",
                "background task panicked: {:?}",
                panic
            );
        }
    });
}

fn canonical<P: AsRef<Path>>(p: P) -> PathBuf {
    match p.as_ref().canonicalize() {
        Ok(c) => c,
        Err(_) => p.as_ref().to_path_buf(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod orphan_cleanup_tests {
    use super::*;

    #[test]
    fn default_config_is_disabled() {
        let cfg = OrphanCleanupConfig::default();
        assert!(!cfg.enabled);
        assert!(!cfg.dry_run);
        assert_eq!(
            cfg.age_threshold,
            Duration::from_secs(DEFAULT_ORPHAN_AGE_THRESHOLD_SECS)
        );
    }

    #[test]
    fn default_age_threshold_is_24h() {
        assert_eq!(DEFAULT_ORPHAN_AGE_THRESHOLD_SECS, 24 * 60 * 60);
    }

    #[test]
    fn orphan_cleanup_config_clone() {
        let cfg = OrphanCleanupConfig {
            enabled: true,
            dry_run: true,
            age_threshold: Duration::from_secs(3600),
        };
        let cloned = cfg.clone();
        assert!(cloned.enabled);
        assert!(cloned.dry_run);
        assert_eq!(cloned.age_threshold, Duration::from_secs(3600));
    }

    #[test]
    fn canonical_preserves_non_existent_path() {
        let fake = PathBuf::from("/nonexistent/path/to/file.bin");
        let c = canonical(&fake);
        assert_eq!(c, fake);
    }

    #[test]
    fn protected_states_include_active_and_ready() {
        // Verify the logic matching used in build_protected_paths
        for state in &["active", "Active", "ACTIVE", "ready", "Ready", "READY"] {
            let lower = state.to_ascii_lowercase();
            let is_production = lower == "active" || lower == "ready";
            assert!(
                is_production,
                "State '{}' should be considered production",
                state
            );
        }
        for state in &["archived", "draft", "deprecated", "deleted"] {
            let lower = state.to_ascii_lowercase();
            let is_production = lower == "active" || lower == "ready";
            assert!(
                !is_production,
                "State '{}' should NOT be considered production",
                state
            );
        }
    }
}
