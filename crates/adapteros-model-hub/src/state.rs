//! Download state persistence for crash recovery
//!
//! This module provides state tracking and persistence for model downloads,
//! enabling crash recovery and resumable downloads.
//!
//! ## Architecture
//!
//! - **DownloadState**: Top-level state for a model download
//! - **FileDownloadState**: Per-file download progress
//! - **StateManager**: Handles atomic state persistence to disk
//!
//! ## State File Format
//!
//! State files are JSON documents stored as `download-{model_id}.state.json`
//! in the state directory. The format includes a version field for future
//! migrations.
//!
//! ## Atomic Writes
//!
//! All state updates use atomic write-via-rename to ensure consistency:
//! 1. Write to temporary file: `{filename}.tmp`
//! 2. Sync to disk
//! 3. Rename to final path (atomic on POSIX)
//!
//! [source: crates/adapteros-model-hub/src/state.rs]
//! [policy: Determinism, Artifacts]

use adapteros_core::error::{AosError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

/// Current state format version
const STATE_VERSION: u32 = 1;

/// Top-level download state for a model
///
/// Tracks overall progress and metadata for a model download operation.
/// Contains a list of individual file download states.
///
/// [source: crates/adapteros-model-hub/src/state.rs]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadState {
    /// Unique model identifier
    pub model_id: String,

    /// Repository identifier (e.g., "meta-llama/Llama-2-7b")
    pub repo_id: String,

    /// Git revision/tag/branch
    pub revision: String,

    /// Per-file download states
    pub files: Vec<FileDownloadState>,

    /// Total bytes across all files
    pub total_bytes: u64,

    /// Total downloaded bytes across all files
    pub downloaded_bytes: u64,

    /// Download start timestamp
    pub started_at: DateTime<Utc>,

    /// Last activity timestamp (updated on progress)
    pub last_activity: DateTime<Utc>,

    /// State format version for migrations
    pub version: u32,
}

impl DownloadState {
    /// Create a new download state
    pub fn new(model_id: String, repo_id: String, revision: String) -> Self {
        let now = Utc::now();
        Self {
            model_id,
            repo_id,
            revision,
            files: Vec::new(),
            total_bytes: 0,
            downloaded_bytes: 0,
            started_at: now,
            last_activity: now,
            version: STATE_VERSION,
        }
    }

    /// Add a file to the download state
    pub fn add_file(&mut self, file: FileDownloadState) {
        self.total_bytes += file.total_bytes;
        self.downloaded_bytes += file.downloaded_bytes;
        self.files.push(file);
        self.last_activity = Utc::now();
    }

    /// Update progress for a specific file
    pub fn update_file_progress(&mut self, filename: &str, downloaded: u64) -> Result<()> {
        let file = self
            .files
            .iter_mut()
            .find(|f| f.filename == filename)
            .ok_or_else(|| AosError::NotFound(format!("File not found: {}", filename)))?;

        let delta = downloaded.saturating_sub(file.downloaded_bytes);
        file.downloaded_bytes = downloaded;
        self.downloaded_bytes = self.downloaded_bytes.saturating_add(delta);
        self.last_activity = Utc::now();

        Ok(())
    }

    /// Mark a file as completed
    pub fn complete_file(&mut self, filename: &str, hash: String) -> Result<()> {
        let file = self
            .files
            .iter_mut()
            .find(|f| f.filename == filename)
            .ok_or_else(|| AosError::NotFound(format!("File not found: {}", filename)))?;

        file.status = FileStatus::Completed { hash };
        self.last_activity = Utc::now();

        Ok(())
    }

    /// Mark a file as failed
    pub fn fail_file(&mut self, filename: &str, error: String) -> Result<()> {
        let file = self
            .files
            .iter_mut()
            .find(|f| f.filename == filename)
            .ok_or_else(|| AosError::NotFound(format!("File not found: {}", filename)))?;

        let retry_count = match &file.status {
            FileStatus::Failed { retry_count, .. } => retry_count + 1,
            _ => 1,
        };

        file.status = FileStatus::Failed { error, retry_count };
        self.last_activity = Utc::now();

        Ok(())
    }

    /// Check if all files are completed
    pub fn is_complete(&self) -> bool {
        !self.files.is_empty()
            && self
                .files
                .iter()
                .all(|f| matches!(f.status, FileStatus::Completed { .. }))
    }

    /// Get completion percentage (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.downloaded_bytes as f64 / self.total_bytes as f64
        }
    }
}

/// Per-file download state
///
/// Tracks progress, status, and metadata for a single file within
/// a model download operation.
///
/// [source: crates/adapteros-model-hub/src/state.rs]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDownloadState {
    /// Filename (relative to model directory)
    pub filename: String,

    /// Download URL
    pub url: String,

    /// Expected hash (for verification)
    pub expected_hash: Option<String>,

    /// Bytes downloaded so far
    pub downloaded_bytes: u64,

    /// Total file size in bytes
    pub total_bytes: u64,

    /// Current status
    pub status: FileStatus,

    /// Path to partial download file
    pub partial_path: String,

    /// Final destination path
    pub final_path: String,

    /// HTTP ETag for resumable downloads
    pub etag: Option<String>,
}

impl FileDownloadState {
    /// Create a new file download state
    pub fn new(
        filename: String,
        url: String,
        total_bytes: u64,
        partial_path: String,
        final_path: String,
    ) -> Self {
        Self {
            filename,
            url,
            expected_hash: None,
            downloaded_bytes: 0,
            total_bytes,
            status: FileStatus::Pending,
            partial_path,
            final_path,
            etag: None,
        }
    }

    /// Get completion percentage (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.downloaded_bytes as f64 / self.total_bytes as f64
        }
    }
}

/// File download status
///
/// Represents the current state of a file download operation.
///
/// [source: crates/adapteros-model-hub/src/state.rs]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum FileStatus {
    /// Waiting to start
    Pending,

    /// Download in progress
    InProgress,

    /// Successfully completed
    Completed {
        /// Verified hash of the downloaded file
        hash: String,
    },

    /// Failed with error
    Failed {
        /// Error message
        error: String,
        /// Number of retry attempts
        retry_count: u32,
    },
}

/// Manages download state persistence
///
/// Handles atomic writes, state loading, and cleanup operations
/// for download state files.
///
/// ## Thread Safety
///
/// StateManager is Send + Sync and can be safely shared across tasks.
/// State file operations are atomic via write-and-rename.
///
/// [source: crates/adapteros-model-hub/src/state.rs]
pub struct StateManager {
    state_dir: PathBuf,
}

impl StateManager {
    /// Create a new state manager
    ///
    /// # Arguments
    ///
    /// * `state_dir` - Directory to store state files
    ///
    /// # Example
    ///
    /// ```ignore
    /// let manager = StateManager::new("/var/lib/adapteros/download-states".into());
    /// ```
    pub fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }

    /// Save download state to disk
    ///
    /// Uses atomic write-and-rename to ensure consistency.
    /// State is first written to a temporary file, synced to disk,
    /// then renamed to the final path.
    ///
    /// # Arguments
    ///
    /// * `state` - Download state to persist
    ///
    /// # Errors
    ///
    /// Returns error if directory creation, serialization, or I/O fails.
    ///
    /// [source: crates/adapteros-model-hub/src/state.rs]
    /// [policy: Artifacts, Determinism]
    pub async fn save_state(&self, state: &DownloadState) -> Result<()> {
        // Ensure state directory exists
        fs::create_dir_all(&self.state_dir)
            .await
            .map_err(|e| AosError::Io(format!("Failed to create state directory: {}", e)))?;

        let filename = format!("download-{}.state.json", state.model_id);
        let final_path = self.state_dir.join(&filename);
        let temp_path = self.state_dir.join(format!("{}.tmp", filename));

        // Serialize state
        let json = serde_json::to_string_pretty(state).map_err(|e| AosError::Serialization(e))?;

        // Write to temporary file
        let mut file = fs::File::create(&temp_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to create temp state file: {}", e)))?;

        file.write_all(json.as_bytes())
            .await
            .map_err(|e| AosError::Io(format!("Failed to write state data: {}", e)))?;

        // Sync to disk
        file.sync_all()
            .await
            .map_err(|e| AosError::Io(format!("Failed to sync state file: {}", e)))?;

        drop(file);

        // Atomic rename
        fs::rename(&temp_path, &final_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to rename state file: {}", e)))?;

        debug!(
            model_id = %state.model_id,
            path = %final_path.display(),
            progress = %state.progress(),
            "Saved download state"
        );

        Ok(())
    }

    /// Load download state from disk
    ///
    /// # Arguments
    ///
    /// * `model_id` - Model identifier
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(state))` if state exists, `Ok(None)` if not found.
    ///
    /// # Errors
    ///
    /// Returns error if deserialization or I/O fails.
    ///
    /// [source: crates/adapteros-model-hub/src/state.rs]
    pub async fn load_state(&self, model_id: &str) -> Result<Option<DownloadState>> {
        let filename = format!("download-{}.state.json", model_id);
        let path = self.state_dir.join(filename);

        // Check if file exists
        if !path.exists() {
            return Ok(None);
        }

        // Read and deserialize
        let contents = fs::read_to_string(&path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read state file: {}", e)))?;

        let state: DownloadState =
            serde_json::from_str(&contents).map_err(|e| AosError::Serialization(e))?;

        debug!(
            model_id = %model_id,
            path = %path.display(),
            progress = %state.progress(),
            "Loaded download state"
        );

        Ok(Some(state))
    }

    /// Delete download state from disk
    ///
    /// # Arguments
    ///
    /// * `model_id` - Model identifier
    ///
    /// # Errors
    ///
    /// Returns error if deletion fails (except for NotFound).
    ///
    /// [source: crates/adapteros-model-hub/src/state.rs]
    pub async fn delete_state(&self, model_id: &str) -> Result<()> {
        let filename = format!("download-{}.state.json", model_id);
        let path = self.state_dir.join(filename);

        match fs::remove_file(&path).await {
            Ok(_) => {
                info!(model_id = %model_id, "Deleted download state");
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug!(model_id = %model_id, "State file not found (already deleted)");
                Ok(())
            }
            Err(e) => Err(AosError::Io(format!("Failed to delete state file: {}", e))),
        }
    }

    /// List all incomplete downloads
    ///
    /// Scans the state directory and returns all download states
    /// that are not fully completed. Useful for crash recovery.
    ///
    /// # Returns
    ///
    /// Vector of incomplete download states, sorted by last activity
    /// (most recent first).
    ///
    /// # Errors
    ///
    /// Returns error if directory reading or state loading fails.
    ///
    /// [source: crates/adapteros-model-hub/src/state.rs]
    pub async fn list_incomplete_downloads(&self) -> Result<Vec<DownloadState>> {
        // Ensure directory exists
        if !self.state_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(&self.state_dir)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read state directory: {}", e)))?;

        let mut incomplete_states = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();

            // Skip if not a state file
            if !path.extension().map_or(false, |ext| ext == "json") {
                continue;
            }

            // Skip temporary files
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |n| n.ends_with(".tmp"))
            {
                continue;
            }

            // Read and parse state
            let contents = match fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to read state file");
                    continue;
                }
            };

            let state: DownloadState = match serde_json::from_str(&contents) {
                Ok(s) => s,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to parse state file");
                    continue;
                }
            };

            // Only include incomplete downloads
            if !state.is_complete() {
                incomplete_states.push(state);
            }
        }

        // Sort by last activity (most recent first)
        incomplete_states.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

        info!(
            count = incomplete_states.len(),
            "Found incomplete downloads"
        );

        Ok(incomplete_states)
    }

    /// Clean up stale state files
    ///
    /// Removes state files that haven't been updated within the
    /// specified maximum age.
    ///
    /// # Arguments
    ///
    /// * `max_age` - Maximum age for state files
    ///
    /// # Returns
    ///
    /// Number of state files deleted.
    ///
    /// # Errors
    ///
    /// Returns error if directory reading or deletion fails.
    ///
    /// [source: crates/adapteros-model-hub/src/state.rs]
    pub async fn cleanup_stale_states(&self, max_age: Duration) -> Result<usize> {
        // Ensure directory exists
        if !self.state_dir.exists() {
            return Ok(0);
        }

        let now = Utc::now();
        let max_age_chrono = chrono::Duration::from_std(max_age)
            .map_err(|e| AosError::Internal(format!("Invalid duration: {}", e)))?;

        let mut entries = fs::read_dir(&self.state_dir)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read state directory: {}", e)))?;

        let mut deleted_count = 0;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();

            // Skip if not a state file
            if !path.extension().map_or(false, |ext| ext == "json") {
                continue;
            }

            // Skip temporary files
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |n| n.ends_with(".tmp"))
            {
                continue;
            }

            // Read and parse state
            let contents = match fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to read state file");
                    continue;
                }
            };

            let state: DownloadState = match serde_json::from_str(&contents) {
                Ok(s) => s,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to parse state file");
                    continue;
                }
            };

            // Check if stale
            let age = now - state.last_activity;
            if age > max_age_chrono {
                match fs::remove_file(&path).await {
                    Ok(_) => {
                        info!(
                            model_id = %state.model_id,
                            age_days = %age.num_days(),
                            "Deleted stale state file"
                        );
                        deleted_count += 1;
                    }
                    Err(e) => {
                        warn!(
                            path = %path.display(),
                            error = %e,
                            "Failed to delete stale state file"
                        );
                    }
                }
            }
        }

        info!(deleted = deleted_count, "Cleaned up stale state files");

        Ok(deleted_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_save_and_load_state() {
        let temp_dir = TempDir::new().unwrap();
        let manager = StateManager::new(temp_dir.path().to_path_buf());

        let mut state = DownloadState::new(
            "test-model".to_string(),
            "org/model".to_string(),
            "main".to_string(),
        );

        state.add_file(FileDownloadState::new(
            "model.safetensors".to_string(),
            "https://example.com/model.safetensors".to_string(),
            1000,
            "/tmp/model.partial".to_string(),
            "/models/model.safetensors".to_string(),
        ));

        // Save state
        manager.save_state(&state).await.unwrap();

        // Load state
        let loaded = manager.load_state("test-model").await.unwrap();
        assert!(loaded.is_some());

        let loaded_state = loaded.unwrap();
        assert_eq!(loaded_state.model_id, state.model_id);
        assert_eq!(loaded_state.repo_id, state.repo_id);
        assert_eq!(loaded_state.files.len(), 1);
    }

    #[tokio::test]
    async fn test_load_nonexistent_state() {
        let temp_dir = TempDir::new().unwrap();
        let manager = StateManager::new(temp_dir.path().to_path_buf());

        let result = manager.load_state("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_state() {
        let temp_dir = TempDir::new().unwrap();
        let manager = StateManager::new(temp_dir.path().to_path_buf());

        let state = DownloadState::new(
            "test-model".to_string(),
            "org/model".to_string(),
            "main".to_string(),
        );

        // Save and verify
        manager.save_state(&state).await.unwrap();
        assert!(manager.load_state("test-model").await.unwrap().is_some());

        // Delete and verify
        manager.delete_state("test-model").await.unwrap();
        assert!(manager.load_state("test-model").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_update_file_progress() {
        let mut state = DownloadState::new(
            "test-model".to_string(),
            "org/model".to_string(),
            "main".to_string(),
        );

        state.add_file(FileDownloadState::new(
            "model.safetensors".to_string(),
            "https://example.com/model.safetensors".to_string(),
            1000,
            "/tmp/model.partial".to_string(),
            "/models/model.safetensors".to_string(),
        ));

        // Update progress
        state
            .update_file_progress("model.safetensors", 500)
            .unwrap();
        assert_eq!(state.downloaded_bytes, 500);
        assert_eq!(state.files[0].downloaded_bytes, 500);

        // Update again
        state
            .update_file_progress("model.safetensors", 1000)
            .unwrap();
        assert_eq!(state.downloaded_bytes, 1000);
        assert_eq!(state.files[0].downloaded_bytes, 1000);
    }

    #[tokio::test]
    async fn test_complete_file() {
        let mut state = DownloadState::new(
            "test-model".to_string(),
            "org/model".to_string(),
            "main".to_string(),
        );

        state.add_file(FileDownloadState::new(
            "model.safetensors".to_string(),
            "https://example.com/model.safetensors".to_string(),
            1000,
            "/tmp/model.partial".to_string(),
            "/models/model.safetensors".to_string(),
        ));

        state
            .complete_file("model.safetensors", "abc123".to_string())
            .unwrap();

        match &state.files[0].status {
            FileStatus::Completed { hash } => assert_eq!(hash, "abc123"),
            _ => panic!("Expected Completed status"),
        }
    }

    #[tokio::test]
    async fn test_fail_file() {
        let mut state = DownloadState::new(
            "test-model".to_string(),
            "org/model".to_string(),
            "main".to_string(),
        );

        state.add_file(FileDownloadState::new(
            "model.safetensors".to_string(),
            "https://example.com/model.safetensors".to_string(),
            1000,
            "/tmp/model.partial".to_string(),
            "/models/model.safetensors".to_string(),
        ));

        state
            .fail_file("model.safetensors", "Network error".to_string())
            .unwrap();

        match &state.files[0].status {
            FileStatus::Failed { error, retry_count } => {
                assert_eq!(error, "Network error");
                assert_eq!(*retry_count, 1);
            }
            _ => panic!("Expected Failed status"),
        }

        // Fail again to increment retry count
        state
            .fail_file("model.safetensors", "Network error".to_string())
            .unwrap();

        match &state.files[0].status {
            FileStatus::Failed { retry_count, .. } => {
                assert_eq!(*retry_count, 2);
            }
            _ => panic!("Expected Failed status"),
        }
    }

    #[tokio::test]
    async fn test_is_complete() {
        let mut state = DownloadState::new(
            "test-model".to_string(),
            "org/model".to_string(),
            "main".to_string(),
        );

        // Empty state is not complete
        assert!(!state.is_complete());

        state.add_file(FileDownloadState::new(
            "file1.bin".to_string(),
            "https://example.com/file1.bin".to_string(),
            1000,
            "/tmp/file1.partial".to_string(),
            "/models/file1.bin".to_string(),
        ));

        state.add_file(FileDownloadState::new(
            "file2.bin".to_string(),
            "https://example.com/file2.bin".to_string(),
            2000,
            "/tmp/file2.partial".to_string(),
            "/models/file2.bin".to_string(),
        ));

        // Not complete yet
        assert!(!state.is_complete());

        // Complete first file
        state
            .complete_file("file1.bin", "hash1".to_string())
            .unwrap();
        assert!(!state.is_complete());

        // Complete second file
        state
            .complete_file("file2.bin", "hash2".to_string())
            .unwrap();
        assert!(state.is_complete());
    }

    #[tokio::test]
    async fn test_progress() {
        let mut state = DownloadState::new(
            "test-model".to_string(),
            "org/model".to_string(),
            "main".to_string(),
        );

        state.add_file(FileDownloadState::new(
            "model.safetensors".to_string(),
            "https://example.com/model.safetensors".to_string(),
            1000,
            "/tmp/model.partial".to_string(),
            "/models/model.safetensors".to_string(),
        ));

        assert_eq!(state.progress(), 0.0);

        state
            .update_file_progress("model.safetensors", 250)
            .unwrap();
        assert_eq!(state.progress(), 0.25);

        state
            .update_file_progress("model.safetensors", 500)
            .unwrap();
        assert_eq!(state.progress(), 0.5);

        state
            .update_file_progress("model.safetensors", 1000)
            .unwrap();
        assert_eq!(state.progress(), 1.0);
    }

    #[tokio::test]
    async fn test_list_incomplete_downloads() {
        let temp_dir = TempDir::new().unwrap();
        let manager = StateManager::new(temp_dir.path().to_path_buf());

        // Create incomplete state
        let mut incomplete_state = DownloadState::new(
            "incomplete-model".to_string(),
            "org/incomplete".to_string(),
            "main".to_string(),
        );
        incomplete_state.add_file(FileDownloadState::new(
            "model.bin".to_string(),
            "https://example.com/model.bin".to_string(),
            1000,
            "/tmp/model.partial".to_string(),
            "/models/model.bin".to_string(),
        ));
        manager.save_state(&incomplete_state).await.unwrap();

        // Create complete state
        let mut complete_state = DownloadState::new(
            "complete-model".to_string(),
            "org/complete".to_string(),
            "main".to_string(),
        );
        complete_state.add_file(FileDownloadState::new(
            "model.bin".to_string(),
            "https://example.com/model.bin".to_string(),
            1000,
            "/tmp/model.partial".to_string(),
            "/models/model.bin".to_string(),
        ));
        complete_state
            .complete_file("model.bin", "hash".to_string())
            .unwrap();
        manager.save_state(&complete_state).await.unwrap();

        // List incomplete downloads
        let incomplete = manager.list_incomplete_downloads().await.unwrap();
        assert_eq!(incomplete.len(), 1);
        assert_eq!(incomplete[0].model_id, "incomplete-model");
    }

    #[tokio::test]
    async fn test_cleanup_stale_states() {
        let temp_dir = TempDir::new().unwrap();
        let manager = StateManager::new(temp_dir.path().to_path_buf());

        // Create state with old timestamp
        let mut old_state = DownloadState::new(
            "old-model".to_string(),
            "org/old".to_string(),
            "main".to_string(),
        );
        old_state.last_activity = Utc::now() - chrono::Duration::days(10);
        manager.save_state(&old_state).await.unwrap();

        // Create recent state
        let recent_state = DownloadState::new(
            "recent-model".to_string(),
            "org/recent".to_string(),
            "main".to_string(),
        );
        manager.save_state(&recent_state).await.unwrap();

        // Clean up states older than 7 days
        let deleted = manager
            .cleanup_stale_states(Duration::from_secs(7 * 24 * 3600))
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        // Verify old state is deleted
        assert!(manager.load_state("old-model").await.unwrap().is_none());

        // Verify recent state still exists
        assert!(manager.load_state("recent-model").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_atomic_write() {
        let temp_dir = TempDir::new().unwrap();
        let manager = StateManager::new(temp_dir.path().to_path_buf());

        let state = DownloadState::new(
            "test-model".to_string(),
            "org/model".to_string(),
            "main".to_string(),
        );

        // Save state
        manager.save_state(&state).await.unwrap();

        // Verify temp file was removed
        let temp_file = temp_dir.path().join("download-test-model.state.json.tmp");
        assert!(!temp_file.exists());

        // Verify final file exists
        let final_file = temp_dir.path().join("download-test-model.state.json");
        assert!(final_file.exists());
    }
}
