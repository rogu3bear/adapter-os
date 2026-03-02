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
pub use adapteros_core::{AosError, B3Hash, Result};

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

/// Result type alias for model hub operations
pub type HubResult<T> = std::result::Result<T, ModelHubError>;

// Manifest and registry modules
pub mod manifest;
pub mod registry;
