//! Real LLM backend implementations for patch generation

use crate::patch_generator::{LlmBackend, PatchContext};
use adapteros_config::ModelConfig;
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, warn};

/// Configuration for local LLM backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLlmConfig {
    pub model_path: PathBuf,
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: usize,
    pub stop_tokens: Vec<String>,
}

impl Default for LocalLlmConfig {
    fn default() -> Self {
        // Use unified model config from environment, falling back to default path
        let model_path = ModelConfig::from_env()
            .map(|c| c.path)
            .unwrap_or_else(|_| PathBuf::from("models/default"));

        Self {
            model_path,
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: 2048,
            stop_tokens: vec!["</patch>".to_string(), "<|endoftext|>".to_string()],
        }
    }
}

impl LocalLlmConfig {
    /// Create a LocalLlmConfig from a ModelConfig
    ///
    /// Uses the model path from ModelConfig and applies default generation parameters.
    pub fn from_model_config(config: &ModelConfig) -> Self {
        Self {
            model_path: config.path.clone(),
            ..Default::default()
        }
    }
}

/// Local LLM backend using MLX (temporarily disabled)
pub struct LocalLlmBackend {
    /// Configuration for local LLM (reserved for MLX backend reactivation)
    pub config: LocalLlmConfig,
    // model: Option<adapteros_lora_mlx::MLXModel>,
}

impl LocalLlmBackend {
    /// Create a new local LLM backend
    pub fn new(_config: LocalLlmConfig) -> Result<Self> {
        // MLX backend temporarily disabled - requires MLX C++ library
        Err(AosError::Mlx(
            "MLX backend temporarily disabled".to_string(),
        ))
    }

    /// Create a prompt for patch generation
    fn create_patch_prompt(&self, context: &PatchContext) -> String {
        crate::patch_generator::create_patch_prompt(context)
    }

    /// Parse rationale and patch from generated text
    fn parse_response(&self, response: &str) -> (String, String) {
        crate::patch_generator::parse_patch_response(response)
    }

    /// Generate text using MLX model
    async fn generate_text(&self, _prompt: &str) -> Result<String> {
        // MLX backend temporarily disabled
        Err(AosError::Mlx(
            "MLX backend temporarily disabled".to_string(),
        ))
    }
}

#[async_trait]
impl LlmBackend for LocalLlmBackend {
    async fn generate_patch(&self, context: &PatchContext) -> Result<String> {
        let prompt = self.create_patch_prompt(context);
        debug!("Generated prompt length: {} chars", prompt.len());

        let response = self.generate_text(&prompt).await?;
        debug!("Generated response length: {} chars", response.len());

        let (_rationale, patch) = self.parse_response(&response);

        if patch.is_empty() {
            warn!("No patch found in generated response");
            return Err(AosError::Worker(
                "Failed to generate valid patch from LLM response".to_string(),
            ));
        }

        Ok(patch)
    }

    async fn extract_rationale(&self, patch_text: &str) -> Result<String> {
        // Try to extract rationale from the full text
        let lines: Vec<&str> = patch_text.lines().collect();

        let mut rationale_lines = Vec::new();
        for line in lines {
            // Stop when we hit the actual patch
            if line.starts_with("---") || line.starts_with("diff") || line.starts_with("@@") {
                break;
            }
            if !line.trim().is_empty() {
                rationale_lines.push(line);
            }
        }

        if rationale_lines.is_empty() {
            Ok("Generated patch based on provided context and evidence.".to_string())
        } else {
            Ok(rationale_lines.join("\n"))
        }
    }
}

/// Remote LLM backend (for external API services)
///
/// NOTE: This backend is only available when the `remote-llm` feature is enabled.
/// Zero-egress builds disable this feature by default to prevent network access.
#[cfg(feature = "remote-llm")]
pub struct RemoteLlmBackend {
    api_endpoint: String,
    api_key: Option<String>,
    config: LocalLlmConfig,
    client: reqwest::Client,
}

/// Request body for remote LLM API
#[cfg(feature = "remote-llm")]
#[derive(Debug, Serialize)]
struct RemoteLlmRequest {
    prompt: String,
    max_tokens: usize,
    temperature: f32,
    top_p: f32,
    stop: Vec<String>,
}

/// Response from remote LLM API
#[cfg(feature = "remote-llm")]
#[derive(Debug, Deserialize)]
struct RemoteLlmResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    choices: Vec<RemoteLlmChoice>,
}

#[cfg(feature = "remote-llm")]
#[derive(Debug, Deserialize)]
struct RemoteLlmChoice {
    #[serde(default)]
    text: String,
    #[serde(default)]
    message: Option<RemoteLlmMessage>,
}

#[cfg(feature = "remote-llm")]
#[derive(Debug, Deserialize)]
struct RemoteLlmMessage {
    #[serde(default)]
    content: String,
}

#[cfg(feature = "remote-llm")]
impl RemoteLlmBackend {
    /// Create a new remote LLM backend
    ///
    /// # Security
    ///
    /// In release builds, this will reject any remote endpoint configuration
    /// to enforce zero-egress policy. Remote endpoints are only allowed in
    /// debug builds for development and testing purposes.
    pub fn new(api_endpoint: String, api_key: Option<String>) -> Result<Self> {
        // In release builds, reject remote endpoints to enforce zero-egress
        #[cfg(not(debug_assertions))]
        {
            return Err(AosError::EgressViolation(format!(
                "Remote LLM backend is disabled in release builds (endpoint: {}). Zero-egress policy prevents network access to external LLM services",
                api_endpoint
            )));
        }

        #[cfg(debug_assertions)]
        {
            debug!(
                "Creating remote LLM backend for endpoint: {} (debug build only)",
                api_endpoint
            );
            Ok(Self {
                api_endpoint,
                api_key,
                config: LocalLlmConfig::default(),
                client: reqwest::Client::new(),
            })
        }
    }

    /// Set generation configuration
    pub fn with_config(mut self, config: LocalLlmConfig) -> Self {
        self.config = config;
        self
    }

    /// Create a prompt for patch generation
    fn create_patch_prompt(&self, context: &PatchContext) -> String {
        crate::patch_generator::create_patch_prompt(context)
    }

    /// Parse rationale and patch from generated text
    fn parse_response(&self, response: &str) -> (String, String) {
        crate::patch_generator::parse_patch_response(response)
    }

    /// Call the remote LLM API
    async fn call_api(&self, prompt: &str) -> Result<String> {
        let request_body = RemoteLlmRequest {
            prompt: prompt.to_string(),
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
            top_p: self.config.top_p,
            stop: self.config.stop_tokens.clone(),
        };

        let mut request = self
            .client
            .post(&self.api_endpoint)
            .header("Content-Type", "application/json")
            .json(&request_body);

        if let Some(ref api_key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        debug!(endpoint = %self.api_endpoint, "Calling remote LLM API");

        let response = request
            .send()
            .await
            .map_err(|e| AosError::Network(format!("Failed to call LLM API: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AosError::Network(format!(
                "LLM API returned error {}: {}",
                status, error_text
            )));
        }

        let api_response: RemoteLlmResponse = response.json().await.map_err(|e| {
            AosError::Validation(format!("Failed to parse LLM API response: {}", e))
        })?;

        // Extract text from response (handle different API formats)
        let text = if !api_response.text.is_empty() {
            api_response.text
        } else if let Some(choice) = api_response.choices.first() {
            if let Some(ref message) = choice.message {
                message.content.clone()
            } else {
                choice.text.clone()
            }
        } else {
            return Err(AosError::Validation(
                "Empty response from LLM API".to_string(),
            ));
        };

        debug!(response_len = text.len(), "Received response from LLM API");
        Ok(text)
    }
}

#[cfg(feature = "remote-llm")]
#[async_trait]
impl LlmBackend for RemoteLlmBackend {
    async fn generate_patch(&self, context: &PatchContext) -> Result<String> {
        let prompt = self.create_patch_prompt(context);
        debug!("Generated prompt length: {} chars", prompt.len());

        let response = self.call_api(&prompt).await?;
        debug!("Generated response length: {} chars", response.len());

        let (_rationale, patch) = self.parse_response(&response);

        if patch.is_empty() {
            warn!("No patch found in generated response from remote API");
            return Err(AosError::Worker(
                "Failed to generate valid patch from remote LLM API response".to_string(),
            ));
        }

        Ok(patch)
    }

    async fn extract_rationale(&self, patch_text: &str) -> Result<String> {
        // Try to extract rationale from the full text
        let lines: Vec<&str> = patch_text.lines().collect();

        let mut rationale_lines = Vec::new();
        for line in lines {
            // Stop when we hit the actual patch
            if line.starts_with("---") || line.starts_with("diff") || line.starts_with("@@") {
                break;
            }
            if !line.trim().is_empty() {
                rationale_lines.push(line);
            }
        }

        if rationale_lines.is_empty() {
            Ok("Generated patch via remote API backend.".to_string())
        } else {
            Ok(rationale_lines.join("\n"))
        }
    }
}

/// Backend selection based on configuration
pub enum LlmBackendType {
    Local(LocalLlmConfig),
    Remote {
        endpoint: String,
        api_key: Option<String>,
    },
    Mock,
}

/// Create an LLM backend based on configuration
pub fn create_llm_backend(backend_type: LlmBackendType) -> Result<Box<dyn LlmBackend>> {
    match backend_type {
        LlmBackendType::Local(config) => {
            let backend = LocalLlmBackend::new(config)?;
            Ok(Box::new(backend))
        }
        LlmBackendType::Remote { endpoint, api_key } => {
            #[cfg(feature = "remote-llm")]
            {
                let backend = RemoteLlmBackend::new(endpoint, api_key)?;
                return Ok(Box::new(backend));
            }
            #[cfg(not(feature = "remote-llm"))]
            {
                let _ = (endpoint, api_key);
                return Err(AosError::FeatureDisabled {
                    feature: "Remote LLM backend".to_string(),
                    reason: "Zero-egress mode: remote-llm feature is disabled".to_string(),
                    alternative: Some(
                        "Use local backend or enable 'remote-llm' feature if egress is permitted"
                            .to_string(),
                    ),
                });
            }
        }
        LlmBackendType::Mock => {
            use crate::patch_generator::MockLlmBackend;
            Ok(Box::new(MockLlmBackend))
        }
    }
}

/// Create an LLM backend with optional unified model configuration
///
/// This function extends `create_llm_backend` by accepting an optional `ModelConfig`
/// to override the default model path and settings for local backends.
///
/// # Arguments
/// * `backend_type` - The type of backend to create
/// * `model_config` - Optional unified model configuration (used for Local backend)
///
/// # Example
/// ```rust,ignore
/// use adapteros_config::ModelConfig;
/// use adapteros_lora_worker::llm_backend::{create_llm_backend_with_config, LlmBackendType, LocalLlmConfig};
///
/// // Using unified config from environment
/// let model_config = ModelConfig::from_env();
/// let backend = create_llm_backend_with_config(
///     LlmBackendType::Local(LocalLlmConfig::default()),
///     model_config.as_ref(),
/// )?;
/// ```
pub fn create_llm_backend_with_config(
    backend_type: LlmBackendType,
    model_config: Option<&ModelConfig>,
) -> Result<Box<dyn LlmBackend>> {
    match backend_type {
        LlmBackendType::Local(mut config) => {
            // Override model path from ModelConfig if provided
            if let Some(mc) = model_config {
                config.model_path = mc.path.clone();
            }
            let backend = LocalLlmBackend::new(config)?;
            Ok(Box::new(backend))
        }
        LlmBackendType::Remote { endpoint, api_key } => {
            #[cfg(feature = "remote-llm")]
            {
                let backend = RemoteLlmBackend::new(endpoint, api_key)?;
                return Ok(Box::new(backend));
            }
            #[cfg(not(feature = "remote-llm"))]
            {
                let _ = (endpoint, api_key);
                return Err(AosError::FeatureDisabled {
                    feature: "Remote LLM backend".to_string(),
                    reason: "Zero-egress mode: remote-llm feature is disabled".to_string(),
                    alternative: Some(
                        "Use local backend or enable 'remote-llm' feature if egress is permitted"
                            .to_string(),
                    ),
                });
            }
        }
        LlmBackendType::Mock => {
            use crate::patch_generator::MockLlmBackend;
            Ok(Box::new(MockLlmBackend))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch_generator::PatchGenerationRequest;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_local_backend_creation() {
        let config = LocalLlmConfig::default();
        // Don't fail test if MLX isn't available
        let _ = LocalLlmBackend::new(config);
    }

    #[tokio::test]
    async fn test_prompt_generation() {
        let config_val = LocalLlmConfig::default();
        let backend = LocalLlmBackend::new(config_val).unwrap_or_else(|_| LocalLlmBackend {
            config: LocalLlmConfig::default(),
        });

        let request = PatchGenerationRequest {
            repo_id: "test".to_string(),
            commit_sha: None,
            target_files: vec!["src/main.rs".to_string()],
            description: "Add error handling".to_string(),
            evidence: vec![],
            context: HashMap::new(),
        };

        let context = PatchContext {
            request,
            evidence_summary: "Function should return Result".to_string(),
            file_contexts: HashMap::new(),
            constraints: vec!["Must maintain API compatibility".to_string()],
        };

        let prompt = backend.create_patch_prompt(&context);
        assert!(prompt.contains("Add error handling"));
        assert!(prompt.contains("Result"));
        assert!(prompt.contains("API compatibility"));
    }

    #[test]
    fn test_response_parsing() {
        let config = LocalLlmConfig::default();
        let backend = LocalLlmBackend { config };

        let response = r#"This patch adds error handling to the function.

--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,5 @@
 fn main() {
+    if let Err(e) = run() {
+        eprintln!("Error: {}", e);
+    }
 }
"#;

        let (rationale, patch) = backend.parse_response(response);
        assert!(rationale.contains("error handling"));
        assert!(patch.contains("--- a/src/main.rs"));
    }

    /// Test that remote backend creation fails when feature is disabled
    #[test]
    #[cfg(not(feature = "remote-llm"))]
    fn test_remote_backend_rejected_without_feature() {
        let result = create_llm_backend(LlmBackendType::Remote {
            endpoint: "https://api.example.com/v1/completions".to_string(),
            api_key: Some("test-key".to_string()),
        });

        let Err(err) = result else {
            panic!("Remote backend should be rejected without remote-llm feature");
        };

        match err {
            AosError::FeatureDisabled {
                feature, reason, ..
            } => {
                assert!(feature.contains("Remote"), "Error should mention remote");
                assert!(
                    reason.contains("remote-llm"),
                    "Error should mention the feature flag"
                );
            }
            other => panic!("Expected FeatureDisabled error, got: {:?}", other),
        }
    }

    /// Test that remote backend with config creation fails when feature is disabled
    #[test]
    #[cfg(not(feature = "remote-llm"))]
    fn test_remote_backend_with_config_rejected_without_feature() {
        let result = create_llm_backend_with_config(
            LlmBackendType::Remote {
                endpoint: "https://api.example.com/v1/completions".to_string(),
                api_key: None,
            },
            None,
        );

        let Err(err) = result else {
            panic!("Remote backend should be rejected without remote-llm feature");
        };

        match err {
            AosError::FeatureDisabled { .. } => { /* expected */ }
            other => panic!("Expected FeatureDisabled error, got: {:?}", other),
        }
    }

    /// Test that remote backend creation succeeds when feature is enabled
    #[test]
    #[cfg(feature = "remote-llm")]
    fn test_remote_backend_creation_with_feature() {
        let result = create_llm_backend(LlmBackendType::Remote {
            endpoint: "https://api.example.com/v1/completions".to_string(),
            api_key: Some("test-key".to_string()),
        });

        assert!(
            result.is_ok(),
            "Remote backend should succeed with remote-llm feature"
        );
    }

    /// Test that remote backends are rejected in release builds
    ///
    /// This test enforces zero-egress policy by ensuring that even with the
    /// remote-llm feature enabled, remote backends cannot be created in release
    /// builds (when debug_assertions are disabled).
    #[test]
    #[cfg(all(feature = "remote-llm", not(debug_assertions)))]
    fn test_remote_backend_rejected_in_release_builds() {
        // Attempt to create a remote backend
        let result = RemoteLlmBackend::new(
            "https://api.openai.com/v1/completions".to_string(),
            Some("sk-test-key".to_string()),
        );

        // Should fail in release builds
        let Err(err) = result else {
            panic!(
                "Remote backend should be rejected in release builds, but creation succeeded"
            );
        };

        // Verify it's an EgressViolation error
        match err {
            AosError::EgressViolation(msg) => {
                assert!(
                    msg.contains("release builds"),
                    "Error should mention release builds: {}",
                    msg
                );
                assert!(
                    msg.contains("Zero-egress"),
                    "Error should mention zero-egress policy: {}",
                    msg
                );
            }
            other => panic!("Expected EgressViolation error, got: {:?}", other),
        }
    }

    /// Test that remote backends work in debug builds
    ///
    /// This test verifies that remote backends can be created in debug builds
    /// when the feature is enabled, allowing for development and testing.
    #[test]
    #[cfg(all(feature = "remote-llm", debug_assertions))]
    fn test_remote_backend_allowed_in_debug_builds() {
        // Attempt to create a remote backend
        let result = RemoteLlmBackend::new(
            "https://api.example.com/v1/completions".to_string(),
            Some("test-key".to_string()),
        );

        // Should succeed in debug builds
        assert!(
            result.is_ok(),
            "Remote backend should be allowed in debug builds with remote-llm feature"
        );

        let backend = result.unwrap();
        assert_eq!(
            backend.api_endpoint,
            "https://api.example.com/v1/completions"
        );
    }

    /// Test that create_llm_backend enforces release build restrictions
    #[test]
    #[cfg(all(feature = "remote-llm", not(debug_assertions)))]
    fn test_create_llm_backend_rejects_remote_in_release() {
        let result = create_llm_backend(LlmBackendType::Remote {
            endpoint: "https://api.example.com/v1/completions".to_string(),
            api_key: Some("test-key".to_string()),
        });

        // Should fail in release builds
        assert!(
            result.is_err(),
            "create_llm_backend should reject remote backends in release builds"
        );

        match result.unwrap_err() {
            AosError::EgressViolation(_) => { /* expected */ }
            other => panic!(
                "Expected EgressViolation error from create_llm_backend, got: {:?}",
                other
            ),
        }
    }

    /// Test that create_llm_backend_with_config enforces release build restrictions
    #[test]
    #[cfg(all(feature = "remote-llm", not(debug_assertions)))]
    fn test_create_llm_backend_with_config_rejects_remote_in_release() {
        let result = create_llm_backend_with_config(
            LlmBackendType::Remote {
                endpoint: "https://api.example.com/v1/completions".to_string(),
                api_key: None,
            },
            None,
        );

        // Should fail in release builds
        assert!(
            result.is_err(),
            "create_llm_backend_with_config should reject remote backends in release builds"
        );

        match result.unwrap_err() {
            AosError::EgressViolation(_) => { /* expected */ }
            other => panic!(
                "Expected EgressViolation error from create_llm_backend_with_config, got: {:?}",
                other
            ),
        }
    }
}

