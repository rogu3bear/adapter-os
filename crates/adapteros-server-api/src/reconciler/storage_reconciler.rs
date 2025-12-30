//! Enhanced storage reconciler for dataset files.
//!
//! Provides comprehensive checks for dataset file integrity:
//! - Missing file detection
//! - Orphaned file detection
//! - Hash verification
//! - Size mismatch detection
//! - Empty file detection
//! - Stale upload detection
//! - Dataset version verification

use adapteros_core::Result;
use adapteros_db::storage_reconciliation::StorageIssueParams;
use adapteros_db::{training_datasets::DatasetFile, Db};
use blake3::Hasher;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::{debug, info, warn};

/// Default threshold for considering an upload as stale (24 hours)
const STALE_UPLOAD_THRESHOLD_SECS: u64 = 24 * 60 * 60;

/// Minimum expected file size for non-empty dataset files
const MIN_DATASET_FILE_SIZE: u64 = 2;

/// Report of reconciliation results
#[derive(Debug, Default, Clone)]
pub struct ReconcileReport {
    /// Number of files missing from storage
    pub missing: usize,
    /// Number of orphaned files (on disk but not in DB)
    pub orphaned: usize,
    /// Number of files with hash mismatches
    pub hash_mismatch: usize,
    /// Number of files with size mismatches
    pub size_mismatch: usize,
    /// Number of empty or near-empty files
    pub empty_files: usize,
    /// Number of inaccessible files (permission errors, etc.)
    pub inaccessible: usize,
    /// Number of stale upload files detected
    pub stale_uploads: usize,
    /// Number of dataset versions checked
    pub dataset_versions_checked: usize,
    /// Number of dataset files checked
    pub dataset_files_checked: usize,
    /// Number of adapter artifacts checked
    pub adapter_artifacts_checked: usize,
}

impl ReconcileReport {
    /// Returns true if any issues were detected
    pub fn has_issues(&self) -> bool {
        self.missing > 0
            || self.orphaned > 0
            || self.hash_mismatch > 0
            || self.size_mismatch > 0
            || self.empty_files > 0
            || self.inaccessible > 0
            || self.stale_uploads > 0
    }

    /// Returns a summary string for logging
    pub fn summary(&self) -> String {
        format!(
            "missing={}, orphaned={}, hash_mismatch={}, size_mismatch={}, empty={}, inaccessible={}, stale_uploads={}",
            self.missing,
            self.orphaned,
            self.hash_mismatch,
            self.size_mismatch,
            self.empty_files,
            self.inaccessible,
            self.stale_uploads
        )
    }

    /// Merge another report into this one
    pub fn merge(&mut self, other: &ReconcileReport) {
        self.missing += other.missing;
        self.orphaned += other.orphaned;
        self.hash_mismatch += other.hash_mismatch;
        self.size_mismatch += other.size_mismatch;
        self.empty_files += other.empty_files;
        self.inaccessible += other.inaccessible;
        self.stale_uploads += other.stale_uploads;
        self.dataset_versions_checked += other.dataset_versions_checked;
        self.dataset_files_checked += other.dataset_files_checked;
        self.adapter_artifacts_checked += other.adapter_artifacts_checked;
    }
}

/// Configuration options for the storage reconciler
#[derive(Debug, Clone)]
pub struct ReconcilerConfig {
    /// Whether to verify file hashes (can be expensive for large files)
    pub verify_hashes: bool,
    /// Whether to check for stale/abandoned uploads
    pub detect_stale_uploads: bool,
    /// Threshold in seconds for considering an upload as stale
    pub stale_threshold_secs: u64,
    /// Whether to check file sizes match DB records
    pub verify_sizes: bool,
    /// Whether to detect empty files
    pub detect_empty_files: bool,
    /// Maximum number of datasets to check per run (0 = unlimited)
    pub max_datasets_per_run: usize,
    /// Whether to check dataset versions
    pub check_dataset_versions: bool,
}

impl Default for ReconcilerConfig {
    fn default() -> Self {
        Self {
            verify_hashes: true,
            detect_stale_uploads: true,
            stale_threshold_secs: STALE_UPLOAD_THRESHOLD_SECS,
            verify_sizes: true,
            detect_empty_files: true,
            max_datasets_per_run: 0,
            check_dataset_versions: true,
        }
    }
}

impl ReconcilerConfig {
    /// Create a fast config that skips expensive hash verification
    pub fn fast() -> Self {
        Self {
            verify_hashes: false,
            detect_stale_uploads: true,
            stale_threshold_secs: STALE_UPLOAD_THRESHOLD_SECS,
            verify_sizes: true,
            detect_empty_files: true,
            max_datasets_per_run: 1000,
            check_dataset_versions: false,
        }
    }
}

/// Storage reconciler for verifying dataset file integrity
pub struct StorageReconciler {
    db: Db,
    datasets_root: PathBuf,
    adapters_root: PathBuf,
    config: ReconcilerConfig,
}

impl StorageReconciler {
    /// Create a new storage reconciler with default configuration
    pub fn new(db: Db, datasets_root: PathBuf, adapters_root: PathBuf) -> Self {
        Self {
            db,
            datasets_root,
            adapters_root,
            config: ReconcilerConfig::default(),
        }
    }

    /// Set custom configuration
    pub fn with_config(mut self, config: ReconcilerConfig) -> Self {
        self.config = config;
        self
    }

    /// Run all reconciliation checks once
    pub async fn run_once(&self) -> Result<ReconcileReport> {
        let mut report = ReconcileReport::default();

        // Reconcile dataset files (main check)
        self.reconcile_datasets(&mut report).await?;

        // Reconcile dataset versions (versioned storage paths)
        if self.config.check_dataset_versions {
            self.reconcile_dataset_versions(&mut report).await?;
        }

        // Check for stale uploads in temp/chunked directories
        if self.config.detect_stale_uploads {
            self.detect_stale_uploads(&mut report).await?;
        }

        // Reconcile adapter artifacts
        self.reconcile_adapters(&mut report).await?;

        if report.has_issues() {
            warn!(
                summary = %report.summary(),
                datasets_checked = report.dataset_files_checked,
                versions_checked = report.dataset_versions_checked,
                adapters_checked = report.adapter_artifacts_checked,
                "Storage reconciliation completed with issues"
            );
        } else {
            info!(
                datasets_checked = report.dataset_files_checked,
                versions_checked = report.dataset_versions_checked,
                adapters_checked = report.adapter_artifacts_checked,
                "Storage reconciliation completed successfully"
            );
        }

        Ok(report)
    }

    /// Run only dataset file checks (subset of full reconciliation)
    pub async fn check_dataset_files_only(&self) -> Result<ReconcileReport> {
        let mut report = ReconcileReport::default();
        self.reconcile_datasets(&mut report).await?;
        Ok(report)
    }

    /// Check a specific dataset's files for integrity
    pub async fn check_dataset(&self, dataset_id: &str) -> Result<ReconcileReport> {
        let mut report = ReconcileReport::default();

        let dataset = match self.db.get_training_dataset(dataset_id).await? {
            Some(ds) => ds,
            None => return Ok(report),
        };

        let files: Vec<DatasetFile> = self.db.get_dataset_files(dataset_id).await?;

        for f in files {
            report.dataset_files_checked += 1;
            let path = PathBuf::from(&f.file_path);

            self.check_file_integrity(
                &path,
                &f.hash_b3,
                Some(f.size_bytes),
                dataset.tenant_id.as_deref(),
                "dataset_file",
                dataset_id,
                None,
                &mut report,
            )
            .await?;
        }

        Ok(report)
    }

    /// Check a single file's existence and integrity
    #[allow(clippy::too_many_arguments)]
    async fn check_file_integrity(
        &self,
        path: &Path,
        expected_hash: &str,
        expected_size: Option<i64>,
        tenant_id: Option<&str>,
        owner_type: &str,
        owner_id: &str,
        version_id: Option<&str>,
        report: &mut ReconcileReport,
    ) -> Result<bool> {
        // Check if file exists
        if !path.exists() {
            report.missing += 1;
            self.db
                .record_storage_reconciliation_issue(StorageIssueParams {
                    tenant_id,
                    owner_type,
                    owner_id: Some(owner_id),
                    version_id,
                    issue_type: "missing_file",
                    severity: "error",
                    path: &path.to_string_lossy(),
                    expected_hash: Some(expected_hash),
                    actual_hash: None,
                    message: Some("File missing from storage"),
                })
                .await?;
            return Ok(false);
        }

        // Check file accessibility and get metadata
        let metadata = match fs::metadata(path).await {
            Ok(m) => m,
            Err(e) => {
                report.inaccessible += 1;
                self.db
                    .record_storage_reconciliation_issue(StorageIssueParams {
                        tenant_id,
                        owner_type,
                        owner_id: Some(owner_id),
                        version_id,
                        issue_type: "inaccessible",
                        severity: "error",
                        path: &path.to_string_lossy(),
                        expected_hash: Some(expected_hash),
                        actual_hash: None,
                        message: Some(&format!("File inaccessible: {}", e)),
                    })
                    .await?;
                return Ok(false);
            }
        };

        // Check for empty files
        if self.config.detect_empty_files && metadata.len() < MIN_DATASET_FILE_SIZE {
            report.empty_files += 1;
            self.db
                .record_storage_reconciliation_issue(StorageIssueParams {
                    tenant_id,
                    owner_type,
                    owner_id: Some(owner_id),
                    version_id,
                    issue_type: "empty_file",
                    severity: "warning",
                    path: &path.to_string_lossy(),
                    expected_hash: Some(expected_hash),
                    actual_hash: None,
                    message: Some(&format!(
                        "File is empty or too small ({} bytes)",
                        metadata.len()
                    )),
                })
                .await?;
        }

        // Check size mismatch
        if self.config.verify_sizes {
            if let Some(expected) = expected_size {
                let actual_size = metadata.len() as i64;
                if actual_size != expected {
                    report.size_mismatch += 1;
                    self.db
                        .record_storage_reconciliation_issue(StorageIssueParams {
                            tenant_id,
                            owner_type,
                            owner_id: Some(owner_id),
                            version_id,
                            issue_type: "size_mismatch",
                            severity: "warning",
                            path: &path.to_string_lossy(),
                            expected_hash: Some(expected_hash),
                            actual_hash: None,
                            message: Some(&format!(
                                "File size mismatch: expected {} bytes, found {} bytes",
                                expected, actual_size
                            )),
                        })
                        .await?;
                }
            }
        }

        // Verify hash
        if self.config.verify_hashes {
            match self.hash_path(path).await {
                Ok(actual_hash) => {
                    if actual_hash != expected_hash {
                        report.hash_mismatch += 1;
                        self.db
                            .record_storage_reconciliation_issue(StorageIssueParams {
                                tenant_id,
                                owner_type,
                                owner_id: Some(owner_id),
                                version_id,
                                issue_type: "hash_mismatch",
                                severity: "error",
                                path: &path.to_string_lossy(),
                                expected_hash: Some(expected_hash),
                                actual_hash: Some(&actual_hash),
                                message: Some("File hash mismatch - content may be corrupted"),
                            })
                            .await?;
                        return Ok(false);
                    }
                }
                Err(e) => {
                    report.inaccessible += 1;
                    self.db
                        .record_storage_reconciliation_issue(StorageIssueParams {
                            tenant_id,
                            owner_type,
                            owner_id: Some(owner_id),
                            version_id,
                            issue_type: "hash_error",
                            severity: "error",
                            path: &path.to_string_lossy(),
                            expected_hash: Some(expected_hash),
                            actual_hash: None,
                            message: Some(&format!("Failed to compute hash: {}", e)),
                        })
                        .await?;
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Reconcile all dataset files
    async fn reconcile_datasets(&self, report: &mut ReconcileReport) -> Result<()> {
        let limit = if self.config.max_datasets_per_run > 0 {
            self.config.max_datasets_per_run as i64
        } else {
            10_000
        };

        let datasets = self.db.list_all_training_datasets_system(limit).await?;
        debug!(count = datasets.len(), "Reconciling dataset files");

        // Track all known paths for orphan detection
        let mut db_paths: HashSet<PathBuf> = HashSet::new();
        // Track paths by tenant for tenant-specific orphan detection
        let mut _paths_by_tenant: HashMap<String, HashSet<PathBuf>> = HashMap::new();

        for ds in datasets {
            let files: Vec<DatasetFile> = self.db.get_dataset_files(&ds.id).await?;

            for f in &files {
                report.dataset_files_checked += 1;
                let path = PathBuf::from(&f.file_path);
                db_paths.insert(path.clone());

                // Track by tenant for isolation checks
                if let Some(ref tid) = ds.tenant_id {
                    _paths_by_tenant
                        .entry(tid.clone())
                        .or_default()
                        .insert(path.clone());
                }

                self.check_file_integrity(
                    &path,
                    &f.hash_b3,
                    Some(f.size_bytes),
                    ds.tenant_id.as_deref(),
                    "dataset_file",
                    &ds.id,
                    None,
                    report,
                )
                .await?;
            }

            // Also check the dataset's storage_path directory exists
            if !ds.storage_path.is_empty() {
                let storage_dir = PathBuf::from(&ds.storage_path);
                if !storage_dir.exists() && !files.is_empty() {
                    // Directory missing but files should exist
                    debug!(
                        dataset_id = %ds.id,
                        path = %ds.storage_path,
                        "Dataset storage directory missing"
                    );
                }
            }
        }

        // Detect orphan files: files on disk not in any DB record
        self.detect_orphan_files(&db_paths, report).await?;

        Ok(())
    }

    /// Reconcile dataset version storage paths
    async fn reconcile_dataset_versions(&self, report: &mut ReconcileReport) -> Result<()> {
        let versions = self.db.list_all_dataset_versions().await?;
        debug!(count = versions.len(), "Reconciling dataset versions");

        for v in versions {
            report.dataset_versions_checked += 1;

            // Skip soft-deleted versions with relaxed checks
            let is_deleted = v.soft_deleted_at.is_some();

            let path = PathBuf::from(&v.storage_path);
            if is_deleted {
                if !path.exists() {
                    debug!(
                        version_id = %v.id,
                        "Soft-deleted dataset version file missing (expected)"
                    );
                }
                continue;
            }

            self.check_file_integrity(
                &path,
                &v.hash_b3,
                None,
                v.tenant_id.as_deref(),
                "dataset_version",
                &v.dataset_id,
                Some(&v.id),
                report,
            )
            .await?;
        }

        Ok(())
    }

    /// Detect stale uploads in temp and chunked directories
    async fn detect_stale_uploads(&self, report: &mut ReconcileReport) -> Result<()> {
        let threshold = Duration::from_secs(self.config.stale_threshold_secs);
        let now = SystemTime::now();

        // Check temp directory
        let temp_dir = self.datasets_root.join("temp");
        self.scan_stale_in_dir(&temp_dir, threshold, now, report)
            .await?;

        // Check chunked uploads directory
        let chunked_dir = self.datasets_root.join("chunked");
        self.scan_stale_in_dir(&chunked_dir, threshold, now, report)
            .await?;

        Ok(())
    }

    async fn scan_stale_in_dir(
        &self,
        dir: &Path,
        threshold: Duration,
        now: SystemTime,
        report: &mut ReconcileReport,
    ) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        let files = self.collect_files(dir.to_path_buf()).await?;
        for path in files {
            if let Ok(metadata) = fs::metadata(&path).await {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age > threshold {
                            report.stale_uploads += 1;
                            self.db
                                .record_storage_reconciliation_issue(StorageIssueParams {
                                    tenant_id: None,
                                    owner_type: "stale_upload",
                                    owner_id: None,
                                    version_id: None,
                                    issue_type: "stale_upload",
                                    severity: "warning",
                                    path: &path.to_string_lossy(),
                                    expected_hash: None,
                                    actual_hash: None,
                                    message: Some(&format!(
                                        "Stale upload file (age: {} hours)",
                                        age.as_secs() / 3600
                                    )),
                                })
                                .await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Detect orphan files in the datasets directory
    async fn detect_orphan_files(
        &self,
        known_paths: &HashSet<PathBuf>,
        report: &mut ReconcileReport,
    ) -> Result<()> {
        let files_dir = self.datasets_root.join("files");
        if !files_dir.exists() {
            return Ok(());
        }

        let files_on_disk = self.collect_files(files_dir).await?;

        for path in files_on_disk {
            // Normalize path for comparison
            let normalized = match path.canonicalize() {
                Ok(p) => p,
                Err(_) => path.clone(),
            };

            let is_known = known_paths.iter().any(|known| {
                known
                    .canonicalize()
                    .map(|k| k == normalized)
                    .unwrap_or(known == &path)
            });

            if !is_known {
                report.orphaned += 1;
                self.db
                    .record_storage_reconciliation_issue(StorageIssueParams {
                        tenant_id: None,
                        owner_type: "dataset",
                        owner_id: None,
                        version_id: None,
                        issue_type: "orphan_file",
                        severity: "warning",
                        path: &path.to_string_lossy(),
                        expected_hash: None,
                        actual_hash: None,
                        message: Some(
                            "Dataset file present without DB owner; soft-retained pending archival policy",
                        ),
                    })
                    .await?;

                if self.should_delete_physical(&path).await {
                    if let Err(e) = fs::remove_file(&path).await {
                        warn!(path = %path.display(), error = %e, "Failed to delete orphan file");
                    }
                }
            }
        }

        Ok(())
    }

    /// Reconcile adapter artifacts
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
                report.adapter_artifacts_checked += 1;
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

                if self.config.verify_hashes {
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

    /// Compute BLAKE3 hash of a file
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

    /// Recursively collect all files in a directory
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

    /// Check if a file should be physically deleted (policy-driven)
    async fn should_delete_physical(&self, _path: &Path) -> bool {
        // Placeholder for policy-driven archival checks; retain by default.
        // Future: check archival policy, retention settings, etc.
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
            .create_training_dataset_with_id(
                "ds1",
                "present",
                None,
                "jsonl",
                "hash",
                "",
                None,
                None,
                Some("ready"),
                Some("hash"),
                None,
            )
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
            .create_training_dataset_with_id(
                "ds2",
                "missing",
                None,
                "jsonl",
                "hash",
                "",
                None,
                None,
                Some("ready"),
                Some("hash"),
                None,
            )
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

    #[tokio::test]
    async fn reconciler_detects_size_mismatch() {
        let tmp = tempdir().unwrap();
        let datasets_root = tmp.path().join("datasets");
        let adapters_root = tmp.path().join("adapters");
        fs::create_dir_all(&datasets_root).await.unwrap();
        fs::create_dir_all(&adapters_root).await.unwrap();

        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = db.create_tenant("Test Tenant", false).await.unwrap();

        let ds = db
            .create_training_dataset_with_id(
                "ds_size",
                "size_test",
                None,
                "jsonl",
                "hash",
                "",
                None,
                None,
                Some("ready"),
                Some("hash"),
                None,
            )
            .await
            .unwrap();

        adapteros_db::sqlx::query("UPDATE training_datasets SET tenant_id = ? WHERE id = ?")
            .bind(&tenant_id)
            .bind(&ds)
            .execute(db.pool())
            .await
            .unwrap();

        let file_path = datasets_root.join("files").join(&ds).join("data.jsonl");
        fs::create_dir_all(file_path.parent().unwrap())
            .await
            .unwrap();

        // Write a file with different size than recorded
        let actual_bytes = b"{\"text\":\"actual content here\"}";
        fs::write(&file_path, actual_bytes).await.unwrap();
        let file_hash = blake3::hash(actual_bytes).to_hex().to_string();

        // Record with wrong size (smaller than actual)
        db.add_dataset_file(
            &ds,
            "data.jsonl",
            &file_path.to_string_lossy(),
            5, // Wrong size
            &file_hash,
            Some("application/json"),
        )
        .await
        .unwrap();

        let reconciler = StorageReconciler::new(db.clone(), datasets_root, adapters_root);
        let report = reconciler.run_once().await.unwrap();

        assert!(report.size_mismatch >= 1, "Should detect size mismatch");
    }

    #[tokio::test]
    async fn reconciler_detects_empty_files() {
        let tmp = tempdir().unwrap();
        let datasets_root = tmp.path().join("datasets");
        let adapters_root = tmp.path().join("adapters");
        fs::create_dir_all(&datasets_root).await.unwrap();
        fs::create_dir_all(&adapters_root).await.unwrap();

        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = db.create_tenant("Test Tenant", false).await.unwrap();

        let ds = db
            .create_training_dataset_with_id(
                "ds_empty",
                "empty_test",
                None,
                "jsonl",
                "hash",
                "",
                None,
                None,
                Some("ready"),
                Some("hash"),
                None,
            )
            .await
            .unwrap();

        adapteros_db::sqlx::query("UPDATE training_datasets SET tenant_id = ? WHERE id = ?")
            .bind(&tenant_id)
            .bind(&ds)
            .execute(db.pool())
            .await
            .unwrap();

        let file_path = datasets_root.join("files").join(&ds).join("empty.jsonl");
        fs::create_dir_all(file_path.parent().unwrap())
            .await
            .unwrap();

        // Write an empty file
        fs::write(&file_path, b"").await.unwrap();
        let empty_hash = blake3::hash(b"").to_hex().to_string();

        db.add_dataset_file(
            &ds,
            "empty.jsonl",
            &file_path.to_string_lossy(),
            0,
            &empty_hash,
            Some("application/json"),
        )
        .await
        .unwrap();

        let reconciler = StorageReconciler::new(db.clone(), datasets_root, adapters_root);
        let report = reconciler.run_once().await.unwrap();

        assert!(report.empty_files >= 1, "Should detect empty file");
    }

    #[tokio::test]
    async fn reconciler_config_fast_skips_hashes() {
        let tmp = tempdir().unwrap();
        let datasets_root = tmp.path().join("datasets");
        let adapters_root = tmp.path().join("adapters");
        fs::create_dir_all(&datasets_root).await.unwrap();
        fs::create_dir_all(&adapters_root).await.unwrap();

        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = db.create_tenant("Test Tenant", false).await.unwrap();

        let ds = db
            .create_training_dataset_with_id(
                "ds_fast",
                "fast_test",
                None,
                "jsonl",
                "hash",
                "",
                None,
                None,
                Some("ready"),
                Some("hash"),
                None,
            )
            .await
            .unwrap();

        adapteros_db::sqlx::query("UPDATE training_datasets SET tenant_id = ? WHERE id = ?")
            .bind(&tenant_id)
            .bind(&ds)
            .execute(db.pool())
            .await
            .unwrap();

        let file_path = datasets_root.join("files").join(&ds).join("data.jsonl");
        fs::create_dir_all(file_path.parent().unwrap())
            .await
            .unwrap();

        let content = b"{\"text\":\"test\"}";
        fs::write(&file_path, content).await.unwrap();

        // Record with WRONG hash - but fast mode should not detect it
        db.add_dataset_file(
            &ds,
            "data.jsonl",
            &file_path.to_string_lossy(),
            content.len() as i64,
            "wrong_hash",
            Some("application/json"),
        )
        .await
        .unwrap();

        let reconciler = StorageReconciler::new(db.clone(), datasets_root, adapters_root)
            .with_config(ReconcilerConfig::fast());
        let report = reconciler.run_once().await.unwrap();

        // Fast mode skips hash verification, so no hash_mismatch should be detected
        assert_eq!(
            report.hash_mismatch, 0,
            "Fast mode should skip hash verification"
        );
    }

    #[tokio::test]
    async fn report_has_issues_returns_correct_value() {
        let empty_report = ReconcileReport::default();
        assert!(!empty_report.has_issues());

        let mut report_with_missing = ReconcileReport::default();
        report_with_missing.missing = 1;
        assert!(report_with_missing.has_issues());

        let mut report_with_orphaned = ReconcileReport::default();
        report_with_orphaned.orphaned = 1;
        assert!(report_with_orphaned.has_issues());
    }

    #[tokio::test]
    async fn report_merge_combines_counts() {
        let mut report1 = ReconcileReport {
            missing: 1,
            orphaned: 2,
            hash_mismatch: 3,
            ..Default::default()
        };

        let report2 = ReconcileReport {
            missing: 4,
            orphaned: 5,
            hash_mismatch: 6,
            size_mismatch: 1,
            ..Default::default()
        };

        report1.merge(&report2);

        assert_eq!(report1.missing, 5);
        assert_eq!(report1.orphaned, 7);
        assert_eq!(report1.hash_mismatch, 9);
        assert_eq!(report1.size_mismatch, 1);
    }
}
