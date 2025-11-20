/// PRD-02: Robust Temporary File Cleanup
///
/// This module implements reliable cleanup of orphaned .tmp files with verification,
/// retry logic, and comprehensive metrics. The cleanup process runs periodically in
/// the background and performs on-error cleanup in the upload handler.
///
/// Features:
/// - Registry of tracked temporary files
/// - Periodic background cleanup task (default: 5 minutes)
/// - Orphaned file detection (files older than 1 hour by default)
/// - Cleanup verification with retry logic (up to 3 attempts)
/// - Detailed metrics: scanned, deleted, failed, retried
/// - Proper logging at all levels (trace, debug, info, warn, error)
use adapteros_core::AosError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration, Instant};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

/// Metrics for cleanup operations
#[derive(Debug, Clone, Default)]
pub struct CleanupMetrics {
    /// Total number of files scanned
    pub total_scanned: u64,
    /// Number of files successfully deleted
    pub total_deleted: u64,
    /// Number of files that failed to delete
    pub total_failed: u64,
    /// Number of files retried
    pub total_retried: u64,
    /// Timestamp of last cleanup run
    pub last_cleanup_at: Option<Instant>,
    /// Number of errors encountered
    pub total_errors: u64,
}

/// Tracking information for a temporary file
#[derive(Debug, Clone)]
pub struct TempFileEntry {
    /// Unique identifier for this temp file
    pub id: String,
    /// Full path to the temporary file
    pub path: PathBuf,
    /// When this file was registered
    pub created_at: Instant,
    /// How many times we've attempted cleanup
    pub cleanup_attempts: u32,
    /// Last error when trying to cleanup
    pub last_error: Option<String>,
}

/// Registry and manager for temporary files with cleanup capabilities
pub struct TempFileRegistry {
    /// Map of temp file ID -> entry
    files: Arc<RwLock<HashMap<String, TempFileEntry>>>,
    /// Cleanup metrics
    metrics: Arc<RwLock<CleanupMetrics>>,
    /// Threshold age for orphaned files (files older than this without cleanup attempt)
    orphan_threshold: Duration,
    /// Maximum attempts before giving up on cleanup
    max_cleanup_attempts: u32,
}

impl TempFileRegistry {
    /// Create a new temporary file registry
    ///
    /// # Arguments
    /// * `orphan_threshold` - Duration after which a temp file is considered orphaned
    /// * `max_cleanup_attempts` - Maximum number of cleanup retries
    pub fn new(orphan_threshold: Duration, max_cleanup_attempts: u32) -> Self {
        info!(
            orphan_threshold_secs = orphan_threshold.as_secs(),
            max_cleanup_attempts = max_cleanup_attempts,
            "Initializing temp file registry"
        );
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(CleanupMetrics::default())),
            orphan_threshold,
            max_cleanup_attempts,
        }
    }

    /// Register a temporary file for tracking
    pub async fn register(&self, path: impl AsRef<Path>) -> String {
        let path = path.as_ref();
        let id = Uuid::now_v7().to_string();

        let entry = TempFileEntry {
            id: id.clone(),
            path: path.to_path_buf(),
            created_at: Instant::now(),
            cleanup_attempts: 0,
            last_error: None,
        };

        debug!(
            temp_file_id = %id,
            path = %path.display(),
            "Registering temporary file"
        );

        let mut files = self.files.write().await;
        files.insert(id.clone(), entry);

        id
    }

    /// Unregister a temporary file (typically called on success)
    pub async fn unregister(&self, id: &str) {
        let mut files = self.files.write().await;
        if files.remove(id).is_some() {
            trace!(temp_file_id = %id, "Unregistered temporary file");
        }
    }

    /// Get current cleanup metrics
    pub async fn metrics(&self) -> CleanupMetrics {
        self.metrics.read().await.clone()
    }

    /// Scan directory for orphaned .tmp files
    async fn scan_orphaned_files(&self, directory: &Path) -> Result<Vec<PathBuf>, AosError> {
        trace!(directory = %directory.display(), "Scanning for orphaned temp files");

        let mut orphaned = Vec::new();

        // Create directory if it doesn't exist (to avoid errors on first run)
        if !directory.exists() {
            debug!(
                directory = %directory.display(),
                "Directory doesn't exist, skipping scan"
            );
            return Ok(orphaned);
        }

        let mut read_dir = fs::read_dir(directory)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read directory: {}", e)))?;

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();

            // Only process .tmp files
            if !path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".tmp"))
                .unwrap_or(false)
            {
                continue;
            }

            // Check if file is in our registry (we don't want to delete actively tracked files)
            let files = self.files.read().await;
            if files.values().any(|e| e.path == path) {
                trace!(path = %path.display(), "File is actively tracked, skipping");
                continue;
            }
            drop(files);

            // Check file age
            match entry.metadata().await {
                Ok(metadata) => {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed > self.orphan_threshold {
                                trace!(
                                    path = %path.display(),
                                    age_secs = elapsed.as_secs(),
                                    threshold_secs = self.orphan_threshold.as_secs(),
                                    "Found orphaned temp file"
                                );
                                orphaned.push(path);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to get metadata for temp file"
                    );
                }
            }
        }

        info!(
            directory = %directory.display(),
            count = orphaned.len(),
            "Scan complete"
        );

        Ok(orphaned)
    }

    /// Perform a single cleanup attempt on a file with verification
    async fn attempt_cleanup(&self, path: &Path) -> Result<bool, String> {
        trace!(path = %path.display(), "Attempting to remove file");

        // Attempt removal
        match fs::remove_file(path).await {
            Ok(_) => {
                // Verify file is actually gone
                match fs::try_exists(path).await {
                    Ok(false) => {
                        trace!(path = %path.display(), "File successfully removed and verified");
                        Ok(true)
                    }
                    Ok(true) => {
                        warn!(
                            path = %path.display(),
                            "File still exists after removal attempt"
                        );
                        Err("File still exists after removal".to_string())
                    }
                    Err(e) => {
                        warn!(
                            path = %path.display(),
                            error = %e,
                            "Failed to verify file removal"
                        );
                        Err(format!("Verification failed: {}", e))
                    }
                }
            }
            Err(e) => {
                // Check if file already doesn't exist
                match fs::try_exists(path).await {
                    Ok(false) => {
                        trace!(
                            path = %path.display(),
                            "File doesn't exist (already cleaned up)"
                        );
                        Ok(true)
                    }
                    _ => {
                        warn!(path = %path.display(), error = %e, "Failed to remove file");
                        Err(format!("Removal failed: {}", e))
                    }
                }
            }
        }
    }

    /// Cleanup a single file with retry logic
    pub async fn cleanup_with_retry(&self, path: &Path) -> Result<bool, String> {
        debug!(path = %path.display(), "Starting cleanup with retry logic");

        for attempt in 1..=self.max_cleanup_attempts {
            match self.attempt_cleanup(path).await {
                Ok(true) => {
                    info!(
                        path = %path.display(),
                        attempt = attempt,
                        "File cleaned up successfully"
                    );
                    let mut metrics = self.metrics.write().await;
                    metrics.total_deleted += 1;
                    return Ok(true);
                }
                Ok(false) => {
                    unreachable!("attempt_cleanup always returns true on success or error");
                }
                Err(e) => {
                    if attempt < self.max_cleanup_attempts {
                        warn!(
                            path = %path.display(),
                            attempt = attempt,
                            max_attempts = self.max_cleanup_attempts,
                            error = %e,
                            "Cleanup attempt failed, retrying"
                        );
                        let mut metrics = self.metrics.write().await;
                        metrics.total_retried += 1;
                        // Exponential backoff: 100ms, 200ms, 400ms, etc.
                        let backoff =
                            Duration::from_millis(100 * 2_u64.saturating_pow(attempt - 1));
                        tokio::time::sleep(backoff).await;
                    } else {
                        error!(
                            path = %path.display(),
                            attempts = attempt,
                            error = %e,
                            "Failed to cleanup file after max retries"
                        );
                        let mut metrics = self.metrics.write().await;
                        metrics.total_failed += 1;
                        metrics.total_errors += 1;
                        return Err(e);
                    }
                }
            }
        }

        Err("Exhausted cleanup attempts".to_string())
    }

    /// Run a single cleanup cycle (scans for orphaned files and cleans them up)
    pub async fn run_cleanup_cycle(&self, directory: &Path) -> Result<(), AosError> {
        debug!(directory = %directory.display(), "Starting cleanup cycle");

        let mut metrics = self.metrics.write().await;
        let start_time = Instant::now();

        // Scan for orphaned files
        drop(metrics); // Release lock before scanning
        let orphaned_files = self.scan_orphaned_files(directory).await?;

        let mut metrics = self.metrics.write().await;
        metrics.total_scanned = metrics
            .total_scanned
            .saturating_add(orphaned_files.len() as u64);

        // Attempt cleanup on each orphaned file
        drop(metrics); // Release lock before cleanup
        for path in orphaned_files {
            let _ = self.cleanup_with_retry(&path).await; // Log errors, don't fail entire cycle
        }

        let mut metrics = self.metrics.write().await;
        metrics.last_cleanup_at = Some(Instant::now());

        let duration = start_time.elapsed();
        info!(
            duration_ms = duration.as_millis(),
            scanned = metrics.total_scanned,
            deleted = metrics.total_deleted,
            failed = metrics.total_failed,
            "Cleanup cycle completed"
        );

        Ok(())
    }
}

/// Background cleanup manager that runs periodic cleanup tasks
pub struct CleanupManager {
    registry: Arc<TempFileRegistry>,
    cleanup_interval: Duration,
    adapters_dir: PathBuf,
}

impl CleanupManager {
    /// Create a new cleanup manager
    pub fn new(
        registry: Arc<TempFileRegistry>,
        cleanup_interval: Duration,
        adapters_dir: impl AsRef<Path>,
    ) -> Self {
        info!(
            interval_secs = cleanup_interval.as_secs(),
            directory = %adapters_dir.as_ref().display(),
            "Creating cleanup manager"
        );
        Self {
            registry,
            cleanup_interval,
            adapters_dir: adapters_dir.as_ref().to_path_buf(),
        }
    }

    /// Start the background cleanup task
    ///
    /// This spawns a tokio task that periodically scans for and removes orphaned temp files.
    /// Returns a handle that can be used to monitor/control the task.
    pub fn start(&self) -> tokio::task::JoinHandle<()> {
        let registry = Arc::clone(&self.registry);
        let interval_duration = self.cleanup_interval;
        let adapters_dir = self.adapters_dir.clone();

        tokio::spawn(async move {
            let mut cleanup_interval = interval(interval_duration);

            loop {
                cleanup_interval.tick().await;

                trace!("Triggering scheduled cleanup cycle");
                match registry.run_cleanup_cycle(&adapters_dir).await {
                    Ok(_) => {
                        debug!("Cleanup cycle completed successfully");
                    }
                    Err(e) => {
                        error!(error = %e, "Cleanup cycle failed");
                    }
                }
            }
        })
    }
}

/// Helper function to trigger immediate cleanup on error
///
/// This is called from the upload handler when an error occurs to immediately
/// clean up the temporary file that was being written.
pub async fn cleanup_temp_file_on_error(registry: &Arc<TempFileRegistry>, path: &Path) {
    debug!(path = %path.display(), "Triggering immediate cleanup on error");
    match registry.cleanup_with_retry(path).await {
        Ok(true) => {
            info!(path = %path.display(), "Temp file cleaned up on error");
        }
        Err(e) => {
            warn!(
                path = %path.display(),
                error = %e,
                "Failed to clean up temp file on error (will be cleaned by background task)"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_registry_register_and_unregister() {
        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);

        let temp_path = PathBuf::from("/tmp/test.tmp");
        let id = registry.register(&temp_path).await;

        assert!(!id.is_empty());
        assert_eq!(id.len(), 36); // UUID string length

        registry.unregister(&id).await;

        let files = registry.files.read().await;
        assert!(!files.contains_key(&id));
    }

    #[tokio::test]
    async fn test_cleanup_existing_file() {
        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("test.tmp");

        // Create a temporary file
        fs::write(&temp_file, "test content").unwrap();
        assert!(temp_file.exists());

        // Cleanup should succeed
        let result = registry.cleanup_with_retry(&temp_file).await;
        assert!(result.is_ok());
        assert!(!temp_file.exists());

        // Check metrics
        let metrics = registry.metrics().await;
        assert_eq!(metrics.total_deleted, 1);
        assert_eq!(metrics.total_failed, 0);
    }

    #[tokio::test]
    async fn test_cleanup_nonexistent_file() {
        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);
        let temp_file = PathBuf::from("/tmp/nonexistent_test_file_12345.tmp");

        // Cleanup should succeed (file doesn't exist)
        let result = registry.cleanup_with_retry(&temp_file).await;
        assert!(result.is_ok());

        let metrics = registry.metrics().await;
        assert_eq!(metrics.total_deleted, 1);
    }

    #[tokio::test]
    async fn test_scan_orphaned_files() {
        let registry = TempFileRegistry::new(Duration::from_millis(100), 3);
        let temp_dir = TempDir::new().unwrap();

        // Create a fresh temp file (not orphaned)
        let fresh_file = temp_dir.path().join("fresh.tmp");
        fs::write(&fresh_file, "fresh").unwrap();

        // Create an old temp file (orphaned)
        let old_file = temp_dir.path().join("old.tmp");
        fs::write(&old_file, "old").unwrap();
        // We can't easily make it old, but we can wait and use a short threshold
        tokio::time::sleep(Duration::from_millis(200)).await;

        let orphaned = registry.scan_orphaned_files(temp_dir.path()).await.unwrap();

        // Should find the old file
        assert!(orphaned.iter().any(|p| p.file_name().unwrap() == "old.tmp"));

        // Cleanup
        for file in orphaned {
            let _ = registry.cleanup_with_retry(&file).await;
        }
    }

    #[tokio::test]
    async fn test_cleanup_metrics() {
        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);
        let temp_dir = TempDir::new().unwrap();

        // Create and cleanup multiple files
        for i in 0..3 {
            let temp_file = temp_dir.path().join(format!("test_{}.tmp", i));
            fs::write(&temp_file, "content").unwrap();
            let _ = registry.cleanup_with_retry(&temp_file).await;
        }

        let metrics = registry.metrics().await;
        assert_eq!(metrics.total_deleted, 3);
        assert_eq!(metrics.total_failed, 0);
    }

    #[tokio::test]
    async fn test_cleanup_manager_creation() {
        let registry = Arc::new(TempFileRegistry::new(Duration::from_secs(60), 3));
        let temp_dir = TempDir::new().unwrap();

        let manager = CleanupManager::new(registry, Duration::from_secs(5), temp_dir.path());
        assert_eq!(manager.cleanup_interval, Duration::from_secs(5));
    }

    #[test]
    fn test_metrics_default() {
        let metrics = CleanupMetrics::default();
        assert_eq!(metrics.total_scanned, 0);
        assert_eq!(metrics.total_deleted, 0);
        assert_eq!(metrics.total_failed, 0);
        assert_eq!(metrics.total_retried, 0);
        assert_eq!(metrics.total_errors, 0);
        assert!(metrics.last_cleanup_at.is_none());
    }
}
