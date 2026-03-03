pub mod cache;
pub mod client;
pub mod download;
pub mod hf_client;
pub mod integrity;
pub mod state;

pub use cache::{CacheConfig, CacheStats, FileLock, GcStats, ModelCache};
pub use client::{ModelHubClient, ModelHubConfig};
pub use download::{DownloadManager, DownloadProgress, DownloadResult, DownloadTask};
pub use hf_client::{HubClient, ModelInfo, RepoFile};
pub use integrity::{extract_hash_from_filename, IntegrityChecker, StreamingHasher};
pub use state::{DownloadState, FileDownloadState, FileStatus, StateManager};

// Re-export core types for convenience
use adapteros_core::recovery::RecoveryClassifier;
pub use adapteros_core::{AosError, B3Hash, Result};
use std::time::Duration;

use thiserror::Error;

/// Model hub specific errors
#[derive(Error, Debug)]
pub enum ModelHubError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Integrity check failed: {0}")]
    IntegrityFailure(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Invalid model ID: {0}")]
    InvalidModelId(String),

    #[error("Download failed: {0}")]
    DownloadFailed(String),

    #[error("adapterOS error: {0}")]
    AosError(#[from] AosError),
}

impl RecoveryClassifier for ModelHubError {
    fn is_retryable(&self) -> bool {
        match self {
            // Network/connectivity errors are typically transient
            ModelHubError::Network(_) => true,

            // Re-use logic for nested AosErrors
            ModelHubError::AosError(e) => e.is_retryable(),

            // Check for rate limits or server errors in DownloadFailed strings
            ModelHubError::DownloadFailed(msg) => {
                let lower = msg.to_lowercase();
                lower.contains("rate limit")
                    || lower.contains("server error")
                    || lower.contains("timeout")
            }

            _ => false,
        }
    }

    fn counts_as_failure(&self) -> bool {
        match self {
            ModelHubError::Network(_) => true,
            ModelHubError::AosError(e) => e.counts_as_failure(),
            ModelHubError::DownloadFailed(msg) => {
                let lower = msg.to_lowercase();
                lower.contains("rate limit")
                    || lower.contains("server error")
                    || lower.contains("timeout")
            }
            _ => false,
        }
    }

    fn recommended_delay(&self) -> Option<Duration> {
        match self {
            ModelHubError::AosError(e) => e.recommended_delay(),
            _ => None,
        }
    }

    fn should_fallback(&self) -> bool {
        self.is_retryable()
    }
}

/// Result type alias for model hub operations
pub type HubResult<T> = std::result::Result<T, ModelHubError>;

// Manifest and registry modules
pub mod manifest;
pub mod registry;
