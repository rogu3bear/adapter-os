//! Hugging Face Hub API client implementation
//!
//! This module provides the core client for interacting with the Hugging Face Hub API.
//! It supports fetching model metadata, listing repository files, and constructing
//! download URLs for model files.
//!
//! ## Architecture
//!
//! The client uses reqwest for HTTP operations and follows adapterOS error handling
//! patterns using `Result<T, ModelHubError>`.
//!
//! ## API Endpoints
//!
//! - Model info: `https://huggingface.co/api/models/{repo_id}/revision/{revision}`
//! - File list: `https://huggingface.co/api/models/{repo_id}/tree/{revision}`
//! - Download: `https://huggingface.co/{repo_id}/resolve/{revision}/{filename}`

use crate::{HubResult, ModelHubError};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info};

/// Default base URL for the Hugging Face Hub API
const DEFAULT_BASE_URL: &str = "https://huggingface.co";

/// Default request timeout in seconds
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Hugging Face Hub API client
///
/// Provides methods to interact with the Hugging Face Hub API for fetching
/// model metadata, listing files, and constructing download URLs.
///
/// ## Example
///
/// ```rust,no_run
/// use adapteros_model_hub::hf_client::{HubClient, ModelInfo};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let client = HubClient::new(None);
///     let model_info = client.get_model_info("meta-llama/Llama-2-7b-hf", None).await?;
///     println!("Model: {} (SHA: {})", model_info.id, model_info.sha);
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug)]
pub struct HubClient {
    /// Base URL for the Hugging Face Hub API
    base_url: String,
    /// Optional authentication token for private repositories
    token: Option<String>,
    /// HTTP client for making requests
    client: reqwest::Client,
}

/// Model metadata from the Hugging Face Hub
///
/// Contains information about a model including its ID, commit SHA,
/// pipeline tag, library, and list of files.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Repository ID (e.g., "meta-llama/Llama-2-7b-hf")
    #[serde(rename = "modelId")]
    pub id: String,
    /// Git commit SHA for this revision
    pub sha: String,
    /// Optional pipeline tag (e.g., "text-generation", "text-classification")
    #[serde(rename = "pipeline_tag")]
    pub pipeline_tag: Option<String>,
    /// Library name (e.g., "transformers", "diffusers")
    #[serde(rename = "library_name")]
    pub library_name: Option<String>,
    /// List of files in the repository
    pub siblings: Vec<RepoFile>,
}

/// File metadata from a Hugging Face repository
///
/// Represents a single file in a model repository with its path,
/// size, and blob ID. Supports both model info API (uses `rfilename`)
/// and tree API (uses `path`) field names.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepoFile {
    /// File path relative to repository root
    /// Tree API uses "path", Model Info API uses "rfilename"
    #[serde(alias = "path", alias = "rfilename")]
    pub rfilename: String,
    /// File size in bytes (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Git blob ID for this file
    /// Tree API uses "oid", Model Info API uses "blobId"
    #[serde(
        alias = "oid",
        alias = "blobId",
        skip_serializing_if = "Option::is_none"
    )]
    pub blob_id: Option<String>,
    /// Entry type: "file" or "directory" (only present in tree API response)
    #[serde(rename = "type", default)]
    pub entry_type: String,
}

impl HubClient {
    /// Creates a new Hugging Face Hub client
    ///
    /// ## Arguments
    ///
    /// * `token` - Optional authentication token for accessing private repositories
    ///
    /// ## Example
    ///
    /// ```rust
    /// use adapteros_model_hub::hf_client::HubClient;
    ///
    /// // Public repositories
    /// let client = HubClient::new(None);
    ///
    /// // Private repositories
    /// let client = HubClient::new(Some("hf_xxxxx".to_string()));
    /// ```
    pub fn new(token: Option<String>) -> Self {
        Self::with_base_url(DEFAULT_BASE_URL.to_string(), token)
    }

    /// Creates a new client with a custom base URL
    ///
    /// This is useful for testing or using alternative Hugging Face Hub instances.
    ///
    /// ## Arguments
    ///
    /// * `base_url` - Base URL for the Hugging Face Hub API
    /// * `token` - Optional authentication token
    pub fn with_base_url(base_url: String, token: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .user_agent(format!("adapteros-model-hub/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            base_url,
            token,
            client,
        }
    }

    /// Fetches model metadata from the Hugging Face Hub
    ///
    /// Retrieves detailed information about a model including its commit SHA,
    /// pipeline tag, library, and list of files.
    ///
    /// ## Arguments
    ///
    /// * `repo_id` - Repository ID (e.g., "meta-llama/Llama-2-7b-hf")
    /// * `revision` - Optional revision (branch, tag, or commit). Defaults to "main"
    ///
    /// ## Returns
    ///
    /// Returns `ModelInfo` containing model metadata and file list.
    ///
    /// ## Errors
    ///
    /// Returns `ModelHubError::Network` if the request fails or times out.
    /// Returns `ModelHubError::ModelNotFound` if the model doesn't exist.
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// # use adapteros_model_hub::hf_client::HubClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = HubClient::new(None);
    ///
    /// // Get main revision
    /// let info = client.get_model_info("gpt2", None).await?;
    ///
    /// // Get specific revision
    /// let info = client.get_model_info("gpt2", Some("v1.0")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_model_info(
        &self,
        repo_id: &str,
        revision: Option<&str>,
    ) -> HubResult<ModelInfo> {
        let revision = revision.unwrap_or("main");
        let url = format!(
            "{}/api/models/{}/revision/{}",
            self.base_url, repo_id, revision
        );

        info!(
            repo_id = %repo_id,
            revision = %revision,
            "Fetching model info from Hugging Face Hub"
        );

        self.execute_with_retry(|| async {
            let mut request = self.client.get(&url);

            if let Some(ref token) = self.token {
                request = request.header("Authorization", format!("Bearer {}", token));
            }

            let response = request.send().await.map_err(ModelHubError::Network)?;

            let status = response.status();
            if !status.is_success() {
                return Err(self.handle_error_response(status, repo_id));
            }

            let model_info = response.json::<ModelInfo>().await.map_err(|e| {
                ModelHubError::DownloadFailed(format!(
                    "Failed to parse model info response for {}: {}",
                    repo_id, e
                ))
            })?;

            debug!(
                repo_id = %repo_id,
                sha = %model_info.sha,
                files_count = model_info.siblings.len(),
                "Successfully fetched model info"
            );

            Ok(model_info)
        })
        .await
    }

    /// Lists all files in a repository
    ///
    /// Retrieves the complete file tree for a model repository at a specific revision.
    ///
    /// ## Arguments
    ///
    /// * `repo_id` - Repository ID (e.g., "meta-llama/Llama-2-7b-hf")
    /// * `revision` - Optional revision (branch, tag, or commit). Defaults to "main"
    ///
    /// ## Returns
    ///
    /// Returns a vector of `RepoFile` entries representing all files in the repository.
    ///
    /// ## Errors
    ///
    /// Returns `ModelHubError::Network` if the request fails or times out.
    /// Returns `ModelHubError::ModelNotFound` if the model doesn't exist.
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// # use adapteros_model_hub::hf_client::HubClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = HubClient::new(None);
    /// let files = client.list_files("gpt2", None).await?;
    ///
    /// for file in files {
    ///     println!("File: {} ({} bytes)", file.rfilename, file.size.unwrap_or(0));
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_files(
        &self,
        repo_id: &str,
        revision: Option<&str>,
    ) -> HubResult<Vec<RepoFile>> {
        let revision = revision.unwrap_or("main");
        let url = format!("{}/api/models/{}/tree/{}", self.base_url, repo_id, revision);

        info!(
            repo_id = %repo_id,
            revision = %revision,
            "Listing files from Hugging Face Hub"
        );

        self.execute_with_retry(|| async {
            let mut request = self.client.get(&url);

            if let Some(ref token) = self.token {
                request = request.header("Authorization", format!("Bearer {}", token));
            }

            let response = request.send().await.map_err(ModelHubError::Network)?;

            let status = response.status();
            if !status.is_success() {
                return Err(self.handle_error_response(status, repo_id));
            }

            // HF tree API returns a flat array of files/directories
            let all_entries = response.json::<Vec<RepoFile>>().await.map_err(|e| {
                ModelHubError::DownloadFailed(format!(
                    "Failed to parse file list response for {}: {}",
                    repo_id, e
                ))
            })?;

            // Filter to only files (exclude directories)
            let files: Vec<RepoFile> = all_entries
                .into_iter()
                .filter(|f| f.entry_type == "file")
                .collect();

            debug!(
                repo_id = %repo_id,
                files_count = files.len(),
                "Successfully listed files"
            );

            Ok(files)
        })
        .await
    }

    /// Constructs a download URL for a specific file
    ///
    /// Generates the direct download URL for a file in a Hugging Face repository.
    /// This URL can be used with standard HTTP clients to download the file.
    ///
    /// ## Arguments
    ///
    /// * `repo_id` - Repository ID (e.g., "meta-llama/Llama-2-7b-hf")
    /// * `filename` - File path relative to repository root
    /// * `revision` - Optional revision (branch, tag, or commit). Defaults to "main"
    ///
    /// ## Returns
    ///
    /// Returns a URL string that can be used to download the file.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use adapteros_model_hub::hf_client::HubClient;
    /// let client = HubClient::new(None);
    /// let url = client.get_download_url("gpt2", "config.json", None);
    /// println!("Download URL: {}", url);
    /// // https://huggingface.co/gpt2/resolve/main/config.json
    /// ```
    pub fn get_download_url(
        &self,
        repo_id: &str,
        filename: &str,
        revision: Option<&str>,
    ) -> String {
        let revision = revision.unwrap_or("main");
        format!(
            "{}/{}/resolve/{}/{}",
            self.base_url, repo_id, revision, filename
        )
    }

    /// Executes a request with exponential backoff retry logic
    ///
    /// Retries failed requests using the adapterOS `RecoveryOrchestrator`.
    /// This helps handle transient network issues and rate limiting.
    async fn execute_with_retry<F, Fut, T>(&self, f: F) -> HubResult<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = HubResult<T>> + Send,
        T: Send,
    {
        use adapteros_core::recovery::RecoveryOrchestratorBuilder;
        use adapteros_core::retry_policy::RetryPolicy;

        let orchestrator = RecoveryOrchestratorBuilder::new("hf-client")
            .with_retry_policy(RetryPolicy::network("hf-client"))
            .build();

        let outcome = orchestrator.execute(f).await;

        match outcome.result {
            Ok(val) => Ok(val),
            Err(e) => {
                let err_msg = e.to_string();
                if let Some(source) = e.into_source() {
                    Err(source)
                } else {
                    Err(crate::ModelHubError::DownloadFailed(err_msg))
                }
            }
        }
    }

    /// Handles HTTP error responses and converts them to appropriate ModelHubError variants
    fn handle_error_response(&self, status: reqwest::StatusCode, repo_id: &str) -> ModelHubError {
        match status.as_u16() {
            401 | 403 => ModelHubError::DownloadFailed(format!(
                "Authentication failed for repository: {} (repository may be private or token invalid)",
                repo_id
            )),
            404 => ModelHubError::ModelNotFound(repo_id.to_string()),
            429 => ModelHubError::DownloadFailed(format!(
                "Rate limit exceeded for Hugging Face Hub API (repository: {})",
                repo_id
            )),
            500..=599 => ModelHubError::DownloadFailed(format!(
                "Hugging Face Hub server error ({}) for repository: {}",
                status, repo_id
            )),
            _ => ModelHubError::DownloadFailed(format!(
                "HTTP error {} for repository: {}",
                status, repo_id
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = HubClient::new(None);
        assert_eq!(client.base_url, DEFAULT_BASE_URL);
        assert!(client.token.is_none());
    }

    #[test]
    fn test_client_with_token() {
        let token = "hf_test_token".to_string();
        let client = HubClient::new(Some(token.clone()));
        assert_eq!(client.token, Some(token));
    }

    #[test]
    fn test_client_with_custom_base_url() {
        let base_url = "https://custom-hub.example.com".to_string();
        let client = HubClient::with_base_url(base_url.clone(), None);
        assert_eq!(client.base_url, base_url);
    }

    #[test]
    fn test_download_url_construction() {
        let client = HubClient::new(None);

        // Test with default revision
        let url = client.get_download_url("gpt2", "config.json", None);
        assert_eq!(url, "https://huggingface.co/gpt2/resolve/main/config.json");

        // Test with specific revision
        let url = client.get_download_url("gpt2", "pytorch_model.bin", Some("v1.0"));
        assert_eq!(
            url,
            "https://huggingface.co/gpt2/resolve/v1.0/pytorch_model.bin"
        );

        // Test with nested path
        let url = client.get_download_url("meta-llama/Llama-2-7b", "model/weights.bin", None);
        assert_eq!(
            url,
            "https://huggingface.co/meta-llama/Llama-2-7b/resolve/main/model/weights.bin"
        );
    }

    #[test]
    fn test_download_url_custom_base() {
        let base_url = "https://custom-hub.example.com".to_string();
        let client = HubClient::with_base_url(base_url, None);

        let url = client.get_download_url("my-org/my-model", "config.json", None);
        assert_eq!(
            url,
            "https://custom-hub.example.com/my-org/my-model/resolve/main/config.json"
        );
    }

    #[test]
    fn test_repo_file_serialization() {
        let file = RepoFile {
            rfilename: "config.json".to_string(),
            size: Some(1024),
            blob_id: Some("abc123".to_string()),
            entry_type: "file".to_string(),
        };

        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("config.json"));
        assert!(json.contains("1024"));
        assert!(json.contains("abc123"));
    }

    #[test]
    fn test_repo_file_deserialization_tree_api() {
        // Tree API uses "path" and "oid"
        let json =
            r#"{"path": "model.safetensors", "size": 2048, "oid": "xyz789", "type": "file"}"#;
        let file: RepoFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.rfilename, "model.safetensors");
        assert_eq!(file.size, Some(2048));
        assert_eq!(file.blob_id, Some("xyz789".to_string()));
        assert_eq!(file.entry_type, "file");
    }

    #[test]
    fn test_repo_file_deserialization_model_info_api() {
        // Model Info API uses "rfilename" and "blobId"
        let json = r#"{"rfilename": "config.json", "size": 1024, "blobId": "abc123"}"#;
        let file: RepoFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.rfilename, "config.json");
        assert_eq!(file.size, Some(1024));
        assert_eq!(file.blob_id, Some("abc123".to_string()));
    }

    #[test]
    fn test_model_info_deserialization() {
        let json = r#"{
            "modelId": "gpt2",
            "sha": "abc123def456",
            "pipeline_tag": "text-generation",
            "library_name": "transformers",
            "siblings": [
                {
                    "rfilename": "config.json",
                    "size": 1024,
                    "blobId": "blob123"
                }
            ]
        }"#;

        let info: ModelInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.id, "gpt2");
        assert_eq!(info.sha, "abc123def456");
        assert_eq!(info.pipeline_tag, Some("text-generation".to_string()));
        assert_eq!(info.library_name, Some("transformers".to_string()));
        assert_eq!(info.siblings.len(), 1);
        assert_eq!(info.siblings[0].rfilename, "config.json");
    }
}
