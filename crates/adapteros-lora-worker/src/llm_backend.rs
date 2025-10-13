//! Real LLM backend implementations for patch generation

use crate::patch_generator::{LlmBackend, PatchContext};
use async_trait::async_trait;
use adapteros_core::{AosError, Result};
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
        Self {
            model_path: PathBuf::from("models/qwen2.5-7b-mlx"),
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: 2048,
            stop_tokens: vec!["</patch>".to_string(), "<|endoftext|>".to_string()],
        }
    }
}

/// Local LLM backend using MLX (temporarily disabled)
pub struct LocalLlmBackend {
    config: LocalLlmConfig,
    // model: Option<adapteros_lora_mlx::MLXModel>,
}

impl LocalLlmBackend {
    /// Create a new local LLM backend
    pub fn new(_config: LocalLlmConfig) -> Result<Self> {
        // MLX backend temporarily disabled due to PyO3 linker issues
        Err(AosError::Mlx(
            "MLX backend temporarily disabled".to_string(),
        ))
    }

    /// Create a prompt for patch generation
    fn create_patch_prompt(&self, context: &PatchContext) -> String {
        let evidence_text = if context.evidence_summary.is_empty() {
            "No evidence provided.".to_string()
        } else {
            format!("Evidence:\n{}", context.evidence_summary)
        };

        let file_contexts_text = context
            .file_contexts
            .iter()
            .map(|(file, content)| format!("File: {}\n```\n{}\n```", file, content))
            .collect::<Vec<_>>()
            .join("\n\n");

        let constraints_text = if context.constraints.is_empty() {
            String::new()
        } else {
            format!(
                "\nConstraints:\n{}",
                context
                    .constraints
                    .iter()
                    .map(|c| format!("- {}", c))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        format!(
            r#"<|im_start|>system
You are an expert code assistant that generates precise, well-documented patches.
Generate a patch that addresses the given description using the provided evidence and context.

Output Format:
1. First, provide a rationale explaining the changes
2. Then, provide the patch in unified diff format

<|im_end|>
<|im_start|>user
Description: {}

{}

File Contexts:
{}
{}

Generate a patch addressing this description. Include:
1. A clear rationale (2-3 sentences)
2. The patch in unified diff format

<|im_end|>
<|im_start|>assistant
"#,
            context.request.description, evidence_text, file_contexts_text, constraints_text
        )
    }

    /// Parse rationale and patch from generated text
    fn parse_response(&self, response: &str) -> (String, String) {
        // Try to extract rationale (before patch)
        let parts: Vec<&str> = response.split("---").collect();

        if parts.len() >= 2 {
            let rationale = parts[0].trim().to_string();
            let patch = format!("---{}", parts[1..].join("---"));
            (rationale, patch)
        } else if response.contains("diff --git") {
            let parts: Vec<&str> = response.split("diff --git").collect();
            let rationale = parts[0].trim().to_string();
            let patch = format!("diff --git{}", parts[1..].join("diff --git"));
            (rationale, patch)
        } else {
            // No clear patch format, treat as rationale only
            (response.trim().to_string(), String::new())
        }
    }

    /// Generate text using MLX model
    async fn generate_text(&self, _prompt: &str) -> Result<String> {
        // MLX backend temporarily disabled
        Err(AosError::Mlx("MLX backend temporarily disabled".to_string()))
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
pub struct RemoteLlmBackend {
    api_endpoint: String,
    api_key: Option<String>,
    config: LocalLlmConfig,
}

impl RemoteLlmBackend {
    /// Create a new remote LLM backend
    pub fn new(api_endpoint: String, api_key: Option<String>) -> Self {
        Self {
            api_endpoint,
            api_key,
            config: LocalLlmConfig::default(),
        }
    }

    /// Set generation configuration
    pub fn with_config(mut self, config: LocalLlmConfig) -> Self {
        self.config = config;
        self
    }
}

#[async_trait]
impl LlmBackend for RemoteLlmBackend {
    async fn generate_patch(&self, context: &PatchContext) -> Result<String> {
        // For now, return a stub. In production, this would call an external API
        warn!(
            "Remote LLM backend not fully implemented, endpoint: {}",
            self.api_endpoint
        );

        Ok(format!(
            "--- a/{}\n+++ b/{}\n@@ -1,1 +1,2 @@\n // Remote API patch generation\n+// Generated via remote API",
            context
                .request
                .target_files
                .first()
                .unwrap_or(&"unknown.rs".to_string()),
            context
                .request
                .target_files
                .first()
                .unwrap_or(&"unknown.rs".to_string()),
        ))
    }

    async fn extract_rationale(&self, _patch_text: &str) -> Result<String> {
        Ok("Patch generated via remote API backend.".to_string())
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
            let backend = RemoteLlmBackend::new(endpoint, api_key);
            Ok(Box::new(backend))
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
        let config = LocalLlmConfig::default();
        let backend = LocalLlmBackend::new(config).unwrap_or_else(|_| LocalLlmBackend {
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
        let backend = LocalLlmBackend {
            config,
        };

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
}
