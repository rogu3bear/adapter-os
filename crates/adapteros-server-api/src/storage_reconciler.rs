//! Background reconciler to keep storage and DB metadata in sync.

use tracing::{debug, error};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use adapteros_core::B3Hash;
use adapteros_db::NewStorageIssue;
use adapteros_storage::FsByteStorage;
use tokio::fs;
use tokio::time::sleep;
use walkdir::WalkDir;

use crate::handlers::datasets::resolve_dataset_root;
use crate::state::AppState;

#[derive(Clone)]
pub struct StorageReconciler {
    state: Arc<AppState>,
}

impl StorageReconciler {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    fn store(&self) -> FsByteStorage {
        let cfg = self.state.config.read().expect("Config lock poisoned");
        FsByteStorage::new(
            cfg.paths.datasets_root.clone().into(),
            cfg.paths.adapters_root.clone().into(),
        )
    }

    pub async fn run_once(&self) {
        let store = self.store();

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
                self.record_issue(NewStorageIssue {
                    tenant_id: tenant_id.as_deref(),
                    owner_type: "dataset_version",
                    owner_id: &v.dataset_id,
                    version_id: Some(&v.id),
                    issue_type: "missing_bytes",
                    severity,
                    location: path.to_string_lossy().as_ref(),
                    details: Some("Dataset version file missing"),
                })
                .await?;
                continue;
            }

            let bytes = fs::read(&path).await.map_err(|e| {
                adapteros_core::AosError::Io(format!(
                    "Failed to read dataset {}: {}",
                    path.display(),
                    e
                ))
            })?;
            let hash = B3Hash::hash(&bytes).to_hex();
            if hash != v.hash_b3 {
                self.record_issue(NewStorageIssue {
                    tenant_id: tenant_id.as_deref(),
                    owner_type: "dataset_version",
                    owner_id: &v.dataset_id,
                    version_id: Some(&v.id),
                    issue_type: "hash_mismatch",
                    severity: "error",
                    location: path.to_string_lossy().as_ref(),
                    details: Some("Dataset version hash mismatch"),
                })
                .await?;
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
                let severity = if v.release_state.to_ascii_lowercase() == "archived" {
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
        for v in self
            .state
            .db
            .list_all_adapter_versions()
            .await
            .unwrap_or_default()
        {
            if let Some(path) = &v.aos_path {
                expected.insert(canonical(path));
            }
        }

        // Scan dataset root for orphans
        let dataset_root = resolve_dataset_root(&self.state).join("files");
        self.scan_orphans_in_dir(&dataset_root, "dataset", &expected)
            .await?;

        // Scan adapter repo root for orphans
        let adapter_root = {
            let cfg = self.state.config.read().expect("Config lock poisoned");
            PathBuf::from(&cfg.paths.adapters_root)
        };
        self.scan_orphans_in_dir(&adapter_root, "adapter", &expected)
            .await?;

        Ok(())
    }

    async fn scan_orphans_in_dir(
        &self,
        root: &Path,
        category: &str,
        expected: &HashSet<PathBuf>,
    ) -> adapteros_core::Result<()> {
        if !root.exists() {
            return Ok(());
        }

        for entry in WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let p = entry.path();
            let canon = canonical(p);
            if !expected.contains(&canon) {
                self.record_issue(NewStorageIssue {
                    tenant_id: None,
                    owner_type: "orphan",
                    owner_id: category,
                    version_id: None,
                    issue_type: "orphan_bytes",
                    severity: "warn",
                    location: canon.to_string_lossy().as_ref(),
                    details: Some("File present without DB owner"),
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
    let reconciler = StorageReconciler::new(state);
    tokio::spawn(async move {
        loop {
            reconciler.run_once().await;
            sleep(Duration::from_secs(900)).await; // 15 min cadence
        }
    });
}

fn canonical<P: AsRef<Path>>(p: P) -> PathBuf {
    match p.as_ref().canonicalize() {
        Ok(c) => c,
        Err(_) => p.as_ref().to_path_buf(),
    }
}
