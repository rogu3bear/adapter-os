//! Model hub client for discovering and downloading models

use crate::cache::ModelCache;
use crate::download::{DownloadManager, DownloadTask};
use crate::hf_client::HubClient;
use crate::HubResult;
use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};

/// Configuration for the model hub client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHubConfig {
    /// Base URL for the model registry
    pub registry_url: String,
    /// Local cache directory
    pub cache_dir: PathBuf,
    /// Maximum concurrent downloads
    pub max_concurrent_downloads: usize,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Optional HF authentication token
    pub hf_token: Option<String>,
}

impl Default for ModelHubConfig {
    fn default() -> Self {
        // Use AOS_MODEL_CACHE_DIR env var if set, otherwise default to var/model-cache
        // This aligns with the server's default in main.rs and cp.toml paths configuration
        let cache_dir = std::env::var("AOS_MODEL_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/model-cache"));

        Self {
            registry_url: "https://huggingface.co".to_string(),
            cache_dir,
            max_concurrent_downloads: 4,
            timeout_secs: 300,
            hf_token: None,
        }
    }
}

/// Main client for interacting with the model hub
pub struct ModelHubClient {
    _config: ModelHubConfig,
    cache: ModelCache,
    download_manager: DownloadManager,
    hf_client: HubClient,
}

impl ModelHubClient {
    /// Create a new model hub client
    pub fn new(config: ModelHubConfig) -> HubResult<Self> {
        let cache = ModelCache::new(config.cache_dir.join("models"))?;
        let download_manager = DownloadManager::new(
            config.cache_dir.join("downloads"),
            config.max_concurrent_downloads,
        )?;
        let hf_client =
            HubClient::with_base_url(config.registry_url.clone(), config.hf_token.clone());

        Ok(Self {
            _config: config,
            cache,
            download_manager,
            hf_client,
        })
    }

    /// List available models from the registry
    ///
    /// Note: HuggingFace Hub doesn't have a simple "list all models" endpoint.
    /// This method returns an empty list. To search for models, you need to
    /// use the HuggingFace Hub search API separately or query specific model IDs.
    pub async fn list_models(&self) -> HubResult<Vec<ModelInfo>> {
        info!("list_models called - HuggingFace Hub doesn't support listing all models");
        // HF Hub doesn't have a simple list endpoint without pagination
        // Users should query specific models via download_model
        Ok(vec![])
    }

    /// Download a model by repository ID
    ///
    /// Downloads all .safetensors and .json files from the specified HuggingFace
    /// repository and stores them in the local cache.
    ///
    /// # Arguments
    ///
    /// * `repo_id` - HuggingFace repository ID (e.g., "meta-llama/Llama-2-7b-hf")
    ///
    /// # Returns
    ///
    /// Returns the path to the downloaded model directory in the cache.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use adapteros_model_hub::client::{ModelHubClient, ModelHubConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ModelHubClient::new(ModelHubConfig::default())?;
    /// let model_path = client.download_model("gpt2").await?;
    /// println!("Model downloaded to: {}", model_path.display());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn download_model(&self, repo_id: &str) -> HubResult<PathBuf> {
        info!(repo_id = %repo_id, "Starting model download");

        // Check if model is already cached
        let model_path = self.cache.get_model_path(repo_id);

        // Get file list from HuggingFace Hub
        let files = self.get_model_files(repo_id).await?;

        if files.is_empty() {
            return Err(crate::ModelHubError::ModelNotFound(format!(
                "No downloadable files found for model: {}",
                repo_id
            )));
        }

        // Check if all required files are already cached
        let required_filenames: Vec<&str> = files.iter().map(|f| f.rfilename.as_str()).collect();
        if self.cache.is_model_complete(repo_id, &required_filenames) {
            info!(repo_id = %repo_id, "Model already cached");
            return Ok(model_path);
        }

        // Acquire lock to prevent concurrent downloads of the same model
        let _lock = self.cache.acquire_lock(repo_id)?;

        // Download each file
        for file in files {
            let url = self
                .hf_client
                .get_download_url(repo_id, &file.rfilename, None);

            debug!(
                repo_id = %repo_id,
                filename = %file.rfilename,
                size = ?file.size,
                url = %url,
                "Downloading file"
            );

            let task = DownloadTask {
                model_id: repo_id.to_string(),
                url,
                filename: file.rfilename.clone(),
                expected_hash: None, // HF doesn't provide B3 hashes
                total_bytes: file.size.unwrap_or(0),
            };

            let download_result = self.download_manager.download_file(task).await?;

            // Store the downloaded file in the cache
            match download_result {
                crate::download::DownloadResult::Complete { path, hash } => {
                    // Read the downloaded file
                    let data = std::fs::read(&path).map_err(|e| {
                        crate::ModelHubError::Io(std::io::Error::other(format!(
                            "Failed to read downloaded file: {}",
                            e
                        )))
                    })?;

                    // Determine file extension
                    let extension = std::path::Path::new(&file.rfilename)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("bin");

                    // Store in cache
                    self.cache.store_blob_with_hash(&data, &hash, extension)?;

                    // Create symlink in model directory
                    self.cache
                        .create_model_symlink(repo_id, &file.rfilename, &hash)?;

                    info!(
                        repo_id = %repo_id,
                        filename = %file.rfilename,
                        hash = %hash.to_hex(),
                        "File downloaded and cached"
                    );
                }
                crate::download::DownloadResult::Resumed {
                    path,
                    bytes_downloaded,
                } => {
                    debug!(
                        repo_id = %repo_id,
                        filename = %file.rfilename,
                        bytes_downloaded = bytes_downloaded,
                        "Download resumed"
                    );

                    // Read and store the resumed file
                    let data = std::fs::read(&path).map_err(|e| {
                        crate::ModelHubError::Io(std::io::Error::other(format!(
                            "Failed to read resumed file: {}",
                            e
                        )))
                    })?;

                    let hash = B3Hash::hash(&data);
                    let extension = std::path::Path::new(&file.rfilename)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("bin");

                    self.cache.store_blob_with_hash(&data, &hash, extension)?;
                    self.cache
                        .create_model_symlink(repo_id, &file.rfilename, &hash)?;
                }
                crate::download::DownloadResult::Failed {
                    reason,
                    is_resumable,
                } => {
                    return Err(crate::ModelHubError::DownloadFailed(format!(
                        "Download failed for {}: {} (resumable: {})",
                        file.rfilename, reason, is_resumable
                    )));
                }
            }
        }

        info!(
            repo_id = %repo_id,
            path = %model_path.display(),
            "Model download complete"
        );

        Ok(model_path)
    }

    /// Get list of downloadable files from a model repository
    ///
    /// Fetches the file tree from HuggingFace Hub and filters to only
    /// .safetensors and .json files.
    async fn get_model_files(&self, repo_id: &str) -> HubResult<Vec<crate::hf_client::RepoFile>> {
        let all_files = self.hf_client.list_files(repo_id, None).await?;

        // Filter to only .safetensors and .json files
        let filtered_files: Vec<_> = all_files
            .into_iter()
            .filter(|f| {
                let path = f.rfilename.to_lowercase();
                path.ends_with(".safetensors") || path.ends_with(".json")
            })
            .collect();

        debug!(
            repo_id = %repo_id,
            total_files = filtered_files.len(),
            "Filtered model files"
        );

        Ok(filtered_files)
    }

    /// Get local cache path for a model
    pub fn get_model_path(&self, model_id: &str) -> PathBuf {
        self.cache.get_model_path(model_id)
    }

    /// Check if a model is fully cached locally
    pub fn is_model_cached(&self, repo_id: &str) -> bool {
        let model_path = self.cache.get_model_path(repo_id);
        model_path.exists()
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> HubResult<crate::cache::CacheStats> {
        Ok(self.cache.stats()?)
    }

    /// Run garbage collection to remove unreferenced blobs
    pub fn garbage_collect(&self) -> HubResult<crate::cache::GcStats> {
        Ok(self.cache.garbage_collect()?)
    }
}

/// Information about a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub size_bytes: u64,
    pub checksum: String,
}
