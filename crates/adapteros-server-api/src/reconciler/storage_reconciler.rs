use adapteros_core::Result;
use adapteros_db::storage_reconciliation::StorageIssueParams;
use adapteros_db::{training_datasets::DatasetFile, Db};
use blake3::Hasher;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::info;

#[derive(Debug, Default)]
pub struct ReconcileReport {
    pub missing: usize,
    pub orphaned: usize,
    pub hash_mismatch: usize,
}

pub struct StorageReconciler {
    db: Db,
    datasets_root: PathBuf,
    adapters_root: PathBuf,
}

impl StorageReconciler {
    pub fn new(db: Db, datasets_root: PathBuf, adapters_root: PathBuf) -> Self {
        Self {
            db,
            datasets_root,
            adapters_root,
        }
    }

    pub async fn run_once(&self) -> Result<ReconcileReport> {
        let mut report = ReconcileReport::default();
        self.reconcile_datasets(&mut report).await?;
        self.reconcile_adapters(&mut report).await?;
        info!(
            missing = report.missing,
            orphaned = report.orphaned,
            hash_mismatch = report.hash_mismatch,
            "Storage reconciliation completed"
        );
        Ok(report)
    }

    async fn reconcile_datasets(&self, report: &mut ReconcileReport) -> Result<()> {
        let datasets = self.db.list_all_training_datasets_system(10_000).await?;

        let mut db_paths = HashSet::new();

        for ds in datasets {
            let files: Vec<DatasetFile> = self.db.get_dataset_files(&ds.id).await?;
            for f in files {
                db_paths.insert(PathBuf::from(&f.file_path));
                let path = PathBuf::from(&f.file_path);
                if !path.exists() {
                    report.missing += 1;
                    self.db
                        .record_storage_reconciliation_issue(StorageIssueParams {
                            tenant_id: ds.tenant_id.as_deref(),
                            owner_type: "dataset",
                            owner_id: Some(ds.id.as_str()),
                            version_id: None,
                            issue_type: "missing_file",
                            severity: "error",
                            path: path.to_string_lossy().as_ref(),
                            expected_hash: Some(f.hash_b3.as_str()),
                            actual_hash: None,
                            message: Some("Dataset file missing from storage"),
                        })
                        .await?;
                    continue;
                }

                let hash = self.hash_path(&path).await?;
                if hash != f.hash_b3 {
                    report.hash_mismatch += 1;
                    self.db
                        .record_storage_reconciliation_issue(StorageIssueParams {
                            tenant_id: ds.tenant_id.as_deref(),
                            owner_type: "dataset",
                            owner_id: Some(ds.id.as_str()),
                            version_id: None,
                            issue_type: "hash_mismatch",
                            severity: "warning",
                            path: path.to_string_lossy().as_ref(),
                            expected_hash: Some(f.hash_b3.as_str()),
                            actual_hash: Some(hash.as_str()),
                            message: Some("Dataset file hash mismatch"),
                        })
                        .await?;
                }
            }
        }

        // Orphans: anything on disk not in db_paths
        let files_on_disk = self.collect_files(self.datasets_root.join("files")).await?;
        for path in files_on_disk {
            if !db_paths.contains(&path) {
                report.orphaned += 1;
                self.db
                    .record_storage_reconciliation_issue(StorageIssueParams {
                        tenant_id: None,
                        owner_type: "dataset",
                        owner_id: None,
                        version_id: None,
                        issue_type: "orphan_file",
                        severity: "warning",
                        path: path.to_string_lossy().as_ref(),
                        expected_hash: None,
                        actual_hash: None,
                        message: Some(
                            "Dataset file present without DB owner; soft-retained pending archival policy",
                        ),
                    })
                    .await?;
                if self.should_delete_physical(&path).await {
                    let _ = fs::remove_file(&path).await;
                }
            }
        }

        Ok(())
    }

    async fn reconcile_adapters(&self, report: &mut ReconcileReport) -> Result<()> {
        let mut db_paths = HashSet::new();

        // NOTE: This lists artifacts per tenant; scope is intentionally broad to catch drift.
        let tenants = self.db.list_tenants().await.unwrap_or_default();
        for tenant in tenants {
            let artifacts = self
                .db
                .list_adapter_artifacts_for_tenant(&tenant.id)
                .await
                .unwrap_or_default();
            for (path_str, aos_hash) in artifacts {
                let path = PathBuf::from(&path_str);
                db_paths.insert(path.clone());
                if !path.exists() {
                    report.missing += 1;
                    self.db
                        .record_storage_reconciliation_issue(StorageIssueParams {
                            tenant_id: Some(tenant.id.as_str()),
                            owner_type: "adapter",
                            owner_id: None,
                            version_id: None,
                            issue_type: "missing_file",
                            severity: "error",
                            path: path.to_string_lossy().as_ref(),
                            expected_hash: Some(aos_hash.as_str()),
                            actual_hash: None,
                            message: Some("Adapter artifact missing from storage"),
                        })
                        .await?;
                    continue;
                }
                let hash = self.hash_path(&path).await?;
                if hash != aos_hash {
                    report.hash_mismatch += 1;
                    self.db
                        .record_storage_reconciliation_issue(StorageIssueParams {
                            tenant_id: Some(tenant.id.as_str()),
                            owner_type: "adapter",
                            owner_id: None,
                            version_id: None,
                            issue_type: "hash_mismatch",
                            severity: "warning",
                            path: path.to_string_lossy().as_ref(),
                            expected_hash: Some(aos_hash.as_str()),
                            actual_hash: Some(hash.as_str()),
                            message: Some("Adapter artifact hash mismatch"),
                        })
                        .await?;
                }
            }
        }

        // Orphans
        let files_on_disk = self.collect_files(self.adapters_root.clone()).await?;
        for path in files_on_disk {
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("aos"))
                && !db_paths.contains(&path)
            {
                report.orphaned += 1;
                self.db
                    .record_storage_reconciliation_issue(StorageIssueParams {
                        tenant_id: None,
                        owner_type: "adapter",
                        owner_id: None,
                        version_id: None,
                        issue_type: "orphan_file",
                        severity: "warning",
                        path: path.to_string_lossy().as_ref(),
                        expected_hash: None,
                        actual_hash: None,
                        message: Some(
                            "Adapter artifact present without DB owner; soft-retained pending archival policy",
                        ),
                    })
                    .await?;
                if self.should_delete_physical(&path).await {
                    let _ = fs::remove_file(&path).await;
                }
            }
        }

        Ok(())
    }

    async fn hash_path(&self, path: &Path) -> Result<String> {
        let mut file = fs::File::open(path).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to open {}: {}", path.display(), e))
        })?;
        let mut hasher = Hasher::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf).await.map_err(|e| {
                adapteros_core::AosError::Io(format!("Failed to read {}: {}", path.display(), e))
            })?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    async fn collect_files(&self, root: PathBuf) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let mut entries = match fs::read_dir(&dir).await {
                Ok(v) => v,
                Err(_) => continue,
            };
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                match entry.metadata().await {
                    Ok(meta) if meta.is_dir() => stack.push(path),
                    Ok(meta) if meta.is_file() => files.push(path),
                    _ => {}
                }
            }
        }
        Ok(files)
    }

    async fn should_delete_physical(&self, _path: &Path) -> bool {
        // Placeholder for policy-driven archival checks; retain by default.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_db::Db;
    use tempfile::tempdir;

    #[tokio::test]
    async fn reconciler_detects_missing_and_orphan_dataset_files() {
        let tmp = tempdir().unwrap();
        let datasets_root = tmp.path().join("datasets");
        let adapters_root = tmp.path().join("adapters");
        fs::create_dir_all(&datasets_root).await.unwrap();
        fs::create_dir_all(&adapters_root).await.unwrap();

        let db = Db::new_in_memory().await.unwrap();

        // Create tenant first (FK constraint on dataset_files requires tenant_id)
        let tenant_id = db.create_tenant("Test Tenant", false).await.unwrap();

        // Dataset with a present file
        let ds1 = db
            .create_training_dataset_with_id("ds1", "present", None, "jsonl", "hash", "", None)
            .await
            .unwrap();
        // Set tenant_id on dataset (required by dataset_files trigger)
        adapteros_db::sqlx::query("UPDATE training_datasets SET tenant_id = ? WHERE id = ?")
            .bind(&tenant_id)
            .bind(&ds1)
            .execute(db.pool())
            .await
            .unwrap();
        let file1_path = datasets_root.join("files").join(&ds1).join("file.jsonl");
        fs::create_dir_all(file1_path.parent().unwrap())
            .await
            .unwrap();
        let file1_bytes = b"{\"text\":\"hello\"}";
        fs::write(&file1_path, file1_bytes).await.unwrap();
        let file1_hash = blake3::hash(file1_bytes).to_hex().to_string();
        db.add_dataset_file(
            &ds1,
            "file.jsonl",
            &file1_path.to_string_lossy(),
            file1_bytes.len() as i64,
            &file1_hash,
            Some("application/json"),
        )
        .await
        .unwrap();

        // Dataset with missing file
        let ds2 = db
            .create_training_dataset_with_id("ds2", "missing", None, "jsonl", "hash", "", None)
            .await
            .unwrap();
        // Set tenant_id on dataset (required by dataset_files trigger)
        adapteros_db::sqlx::query("UPDATE training_datasets SET tenant_id = ? WHERE id = ?")
            .bind(&tenant_id)
            .bind(&ds2)
            .execute(db.pool())
            .await
            .unwrap();
        let missing_path = datasets_root.join("files").join(&ds2).join("missing.jsonl");
        db.add_dataset_file(
            &ds2,
            "missing.jsonl",
            &missing_path.to_string_lossy(),
            5,
            "deadbeef",
            Some("application/json"),
        )
        .await
        .unwrap();

        // Orphan file
        let orphan_path = datasets_root.join("files").join("orphan.bin");
        fs::create_dir_all(orphan_path.parent().unwrap())
            .await
            .unwrap();
        fs::write(&orphan_path, b"orphan").await.unwrap();

        let reconciler = StorageReconciler::new(db.clone(), datasets_root, adapters_root);
        let report = reconciler.run_once().await.unwrap();
        assert!(report.missing >= 1);
        assert!(report.orphaned >= 1);

        let issues = db.list_storage_reconciliation_issues(10).await.unwrap();
        assert!(issues.iter().any(|i| i.issue_type == "missing_file"));
        assert!(issues.iter().any(|i| i.issue_type == "orphan_file"));
    }
}
