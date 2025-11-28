//! Resumable download manager for model files
//!
//! Provides HTTP-based resumable downloads with progress tracking, integrity verification,
//! and concurrent download support.
//!
//! ## Features
//!
//! - **Resumable downloads**: Uses HTTP Range headers to resume interrupted downloads
//! - **Progress tracking**: Real-time download progress via broadcast channel
//! - **Integrity verification**: BLAKE3 hash verification on completion
//! - **Concurrent downloads**: Support for multiple simultaneous downloads
//! - **Speed calculation**: Rolling average download speed with ETA
//!
//! ## Usage
//!
//! ```rust,no_run
//! use adapteros_model_hub::download::{DownloadManager, DownloadTask};
//! use std::path::PathBuf;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = DownloadManager::new(PathBuf::from("/tmp/downloads"), 3)?;
//!
//! let task = DownloadTask {
//!     model_id: "llama-2-7b".to_string(),
//!     url: "https://example.com/model.safetensors".to_string(),
//!     filename: "model.safetensors".to_string(),
//!     expected_hash: None,
//!     total_bytes: 0, // Will be determined from Content-Length
//! };
//!
//! let result = manager.download_file(task).await?;
//! # Ok(())
//! # }
//! ```

use adapteros_core::{AosError, B3Hash, Result};
use futures_util::StreamExt;
use reqwest::{header, StatusCode};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use tracing::{debug, info};

const PROGRESS_UPDATE_INTERVAL_BYTES: u64 = 10_485_760; // 10MB
const SPEED_WINDOW_SAMPLES: usize = 10;

/// Download manager for model files with resumable download support
pub struct DownloadManager {
    client: reqwest::Client,
    downloads_dir: PathBuf,
    max_concurrent: usize,
    progress_tx: broadcast::Sender<DownloadProgress>,
}

/// A download task specification
#[derive(Debug, Clone)]
pub struct DownloadTask {
    /// Model identifier
    pub model_id: String,
    /// Download URL
    pub url: String,
    /// Target filename
    pub filename: String,
    /// Expected BLAKE3 hash for verification (optional)
    pub expected_hash: Option<B3Hash>,
    /// Total bytes (0 = determine from Content-Length)
    pub total_bytes: u64,
}

/// Real-time download progress information
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// Model identifier
    pub model_id: String,
    /// Filename being downloaded
    pub filename: String,
    /// Bytes downloaded so far
    pub downloaded_bytes: u64,
    /// Total bytes to download
    pub total_bytes: u64,
    /// Download speed in bytes per second
    pub speed_bytes_per_sec: f64,
    /// Estimated time remaining in seconds
    pub eta_seconds: Option<u64>,
}

/// Result of a download operation
#[derive(Debug)]
pub enum DownloadResult {
    /// Download completed successfully
    Complete {
        /// Path to downloaded file
        path: PathBuf,
        /// Computed BLAKE3 hash
        hash: B3Hash,
    },
    /// Download was resumed from a previous attempt
    Resumed {
        /// Path to downloaded file
        path: PathBuf,
        /// Bytes downloaded in this session
        bytes_downloaded: u64,
    },
    /// Download failed
    Failed {
        /// Reason for failure
        reason: String,
        /// Whether the download can be resumed
        is_resumable: bool,
    },
}

impl DownloadManager {
    /// Create a new download manager
    ///
    /// # Arguments
    ///
    /// * `downloads_dir` - Directory to store downloaded files
    /// * `max_concurrent` - Maximum number of concurrent downloads
    ///
    /// # Errors
    ///
    /// Returns error if downloads directory cannot be created
    pub fn new(downloads_dir: PathBuf, max_concurrent: usize) -> Result<Self> {
        std::fs::create_dir_all(&downloads_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to create downloads directory at {}: {}",
                downloads_dir.display(),
                e
            ))
        })?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| AosError::Network(format!("Failed to create HTTP client: {}", e)))?;

        let (progress_tx, _) = broadcast::channel(128);

        Ok(Self {
            client,
            downloads_dir,
            max_concurrent,
            progress_tx,
        })
    }

    /// Subscribe to download progress events
    ///
    /// Returns a receiver that will receive progress updates for all downloads
    pub fn subscribe_progress(&self) -> broadcast::Receiver<DownloadProgress> {
        self.progress_tx.subscribe()
    }

    /// Download a file with resumable support
    ///
    /// This method:
    /// 1. Checks if a partial file exists from a previous download
    /// 2. Sends HTTP Range header to resume from the existing position
    /// 3. Downloads the file with progress tracking
    /// 4. Verifies the hash if expected_hash is provided
    ///
    /// # Arguments
    ///
    /// * `task` - Download task specification
    ///
    /// # Errors
    ///
    /// Returns error if download fails, hash verification fails, or I/O errors occur
    pub async fn download_file(&self, task: DownloadTask) -> Result<DownloadResult> {
        let file_path = self.downloads_dir.join(&task.filename);
        let partial_path = self
            .downloads_dir
            .join(format!("{}.partial", task.filename));

        // Check if partial download exists
        let start_byte = if partial_path.exists() {
            tokio::fs::metadata(&partial_path)
                .await
                .map(|m| m.len())
                .unwrap_or(0)
        } else {
            0
        };

        info!(
            model_id = %task.model_id,
            filename = %task.filename,
            start_byte = start_byte,
            "Starting download"
        );

        // Build HTTP request with Range header if resuming
        let mut request = self.client.get(&task.url);
        if start_byte > 0 {
            request = request.header(header::RANGE, format!("bytes={}-", start_byte));
            debug!(start_byte = start_byte, "Resuming download from byte");
        }

        // Send request
        let response = request
            .send()
            .await
            .map_err(|e| AosError::Network(format!("Failed to send HTTP request: {}", e)))?;

        // Check status code
        match response.status() {
            StatusCode::OK | StatusCode::PARTIAL_CONTENT => {
                // Continue with download
            }
            StatusCode::RANGE_NOT_SATISFIABLE => {
                // File is already complete
                info!(
                    model_id = %task.model_id,
                    filename = %task.filename,
                    "File already complete (416 Range Not Satisfiable)"
                );

                // Rename partial to final if needed
                if partial_path.exists() && !file_path.exists() {
                    tokio::fs::rename(&partial_path, &file_path)
                        .await
                        .map_err(|e| {
                            AosError::Io(format!("Failed to rename completed file: {}", e))
                        })?;
                }

                // Verify hash if provided
                if let Some(expected_hash) = task.expected_hash {
                    let actual_hash = self.compute_file_hash(&file_path).await?;
                    if actual_hash != expected_hash {
                        return Err(AosError::CacheCorruption {
                            path: file_path.display().to_string(),
                            expected: expected_hash.to_hex(),
                            actual: actual_hash.to_hex(),
                        });
                    }
                }

                let hash = self.compute_file_hash(&file_path).await?;
                return Ok(DownloadResult::Complete {
                    path: file_path.clone(),
                    hash,
                });
            }
            status => {
                return Err(AosError::Network(format!(
                    "HTTP request failed with status {}: {}",
                    status,
                    response.text().await.unwrap_or_default()
                )));
            }
        }

        // Get total size from Content-Length header
        let total_bytes = if task.total_bytes > 0 {
            task.total_bytes
        } else {
            response.content_length().unwrap_or(0)
        };

        // Check if resuming
        let is_resuming = response.status() == StatusCode::PARTIAL_CONTENT;
        let total_bytes_with_offset = if is_resuming {
            start_byte + total_bytes
        } else {
            total_bytes
        };

        debug!(
            total_bytes = total_bytes_with_offset,
            is_resuming = is_resuming,
            "Download size determined"
        );

        // Open file for writing
        let mut file = if is_resuming && partial_path.exists() {
            OpenOptions::new()
                .append(true)
                .open(&partial_path)
                .await
                .map_err(|e| {
                    AosError::Io(format!("Failed to open partial file for appending: {}", e))
                })?
        } else {
            File::create(&partial_path)
                .await
                .map_err(|e| AosError::Io(format!("Failed to create partial file: {}", e)))?
        };

        // Create byte stream
        let mut stream = response.bytes_stream();
        let mut downloaded = start_byte;
        let mut last_progress_update = start_byte;
        let mut speed_samples: Vec<f64> = Vec::with_capacity(SPEED_WINDOW_SAMPLES);
        let mut last_sample_time = Instant::now();
        let mut last_sample_bytes = start_byte;

        // Download loop
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result
                .map_err(|e| AosError::Network(format!("Failed to read response chunk: {}", e)))?;

            file.write_all(&chunk)
                .await
                .map_err(|e| AosError::Io(format!("Failed to write to file: {}", e)))?;

            downloaded += chunk.len() as u64;

            // Update speed calculation
            let now = Instant::now();
            let elapsed = now.duration_since(last_sample_time).as_secs_f64();
            if elapsed >= 1.0 {
                let bytes_delta = downloaded - last_sample_bytes;
                let speed = bytes_delta as f64 / elapsed;

                speed_samples.push(speed);
                if speed_samples.len() > SPEED_WINDOW_SAMPLES {
                    speed_samples.remove(0);
                }

                last_sample_time = now;
                last_sample_bytes = downloaded;
            }

            // Emit progress update every 10MB
            if downloaded - last_progress_update >= PROGRESS_UPDATE_INTERVAL_BYTES
                || downloaded >= total_bytes_with_offset
            {
                let avg_speed = if !speed_samples.is_empty() {
                    speed_samples.iter().sum::<f64>() / speed_samples.len() as f64
                } else {
                    0.0
                };

                let eta_seconds = if avg_speed > 0.0 {
                    let remaining = total_bytes_with_offset.saturating_sub(downloaded);
                    Some((remaining as f64 / avg_speed) as u64)
                } else {
                    None
                };

                let progress = DownloadProgress {
                    model_id: task.model_id.clone(),
                    filename: task.filename.clone(),
                    downloaded_bytes: downloaded,
                    total_bytes: total_bytes_with_offset,
                    speed_bytes_per_sec: avg_speed,
                    eta_seconds,
                };

                // Ignore send errors (no active receivers)
                let _ = self.progress_tx.send(progress);
                last_progress_update = downloaded;

                debug!(
                    downloaded = downloaded,
                    total = total_bytes_with_offset,
                    speed_mbps = avg_speed / 1_048_576.0,
                    eta_seconds = ?eta_seconds,
                    "Download progress"
                );
            }
        }

        // Flush and close file
        file.flush()
            .await
            .map_err(|e| AosError::Io(format!("Failed to flush file: {}", e)))?;
        drop(file);

        info!(
            model_id = %task.model_id,
            filename = %task.filename,
            downloaded_bytes = downloaded,
            "Download completed, computing hash"
        );

        // Compute hash
        let actual_hash = self.compute_file_hash(&partial_path).await?;

        // Verify hash if provided
        if let Some(expected_hash) = task.expected_hash {
            if actual_hash != expected_hash {
                return Ok(DownloadResult::Failed {
                    reason: format!(
                        "Hash mismatch: expected {}, got {}",
                        expected_hash.to_hex(),
                        actual_hash.to_hex()
                    ),
                    is_resumable: false,
                });
            }
        }

        // Rename partial to final
        tokio::fs::rename(&partial_path, &file_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to rename completed file: {}", e)))?;

        info!(
            model_id = %task.model_id,
            filename = %task.filename,
            hash = %actual_hash.to_hex(),
            "Download and verification complete"
        );

        Ok(DownloadResult::Complete {
            path: file_path,
            hash: actual_hash,
        })
    }

    /// Compute BLAKE3 hash of a file
    async fn compute_file_hash(&self, path: &Path) -> Result<B3Hash> {
        let file_data = tokio::fs::read(path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read file for hashing: {}", e)))?;

        Ok(B3Hash::hash(&file_data))
    }

    /// Get the configured downloads directory
    pub fn downloads_dir(&self) -> &Path {
        &self.downloads_dir
    }

    /// Get the maximum concurrent downloads setting
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_download_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = DownloadManager::new(temp_dir.path().to_path_buf(), 3).unwrap();

        assert_eq!(manager.max_concurrent(), 3);
        assert!(manager.downloads_dir().exists());
    }

    #[tokio::test]
    async fn test_progress_subscription() {
        let temp_dir = TempDir::new().unwrap();
        let manager = DownloadManager::new(temp_dir.path().to_path_buf(), 3).unwrap();

        let mut rx1 = manager.subscribe_progress();
        let mut rx2 = manager.subscribe_progress();

        // Both receivers should work independently
        assert!(rx1.try_recv().is_err()); // No messages yet
        assert!(rx2.try_recv().is_err());
    }

    #[test]
    fn test_download_task_clone() {
        let task = DownloadTask {
            model_id: "test-model".to_string(),
            url: "https://example.com/model".to_string(),
            filename: "model.safetensors".to_string(),
            expected_hash: None,
            total_bytes: 1024,
        };

        let cloned = task.clone();
        assert_eq!(task.model_id, cloned.model_id);
        assert_eq!(task.url, cloned.url);
        assert_eq!(task.filename, cloned.filename);
        assert_eq!(task.total_bytes, cloned.total_bytes);
    }

    #[test]
    fn test_download_progress_clone() {
        let progress = DownloadProgress {
            model_id: "test-model".to_string(),
            filename: "model.safetensors".to_string(),
            downloaded_bytes: 512,
            total_bytes: 1024,
            speed_bytes_per_sec: 1_048_576.0,
            eta_seconds: Some(10),
        };

        let cloned = progress.clone();
        assert_eq!(progress.model_id, cloned.model_id);
        assert_eq!(progress.downloaded_bytes, cloned.downloaded_bytes);
        assert_eq!(progress.speed_bytes_per_sec, cloned.speed_bytes_per_sec);
    }
}
