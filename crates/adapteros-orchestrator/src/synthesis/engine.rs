//! Synthesis engine for generating training data from documents
//!
//! The SynthesisEngine wraps a GPU-accelerated inference backend to run
//! the synthesis model locally on Apple Silicon using MLX/CoreML/Metal.

use super::parser::SynthesisOutputParser;
use super::types::{SynthesisBatchStats, SynthesisRequest, SynthesisResult};
use adapteros_config::{BackendPreference, ModelConfig};
use adapteros_core::{AosError, Result};
use adapteros_lora_worker::backend_factory::{create_backend_from_config, KernelBox};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration for the synthesis engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisEngineConfig {
    /// Path to the CoreML model package
    pub model_path: PathBuf,
    /// Maximum tokens to generate per chunk
    pub max_new_tokens: usize,
    /// Temperature for generation (higher = more diverse)
    pub temperature: f32,
    /// Top-p (nucleus) sampling
    pub top_p: f32,
    /// Whether to use ANE acceleration
    pub use_ane: bool,
    /// System prompt for synthesis
    pub system_prompt: String,
    /// User prompt template (use {chunk} placeholder)
    pub user_template: String,
}

impl Default for SynthesisEngineConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("var/models/synthesis_model.mlpackage"),
            max_new_tokens: 1024,
            temperature: 0.7,
            top_p: 0.9,
            use_ane: true,
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            user_template: DEFAULT_USER_TEMPLATE.to_string(),
        }
    }
}

const DEFAULT_SYSTEM_PROMPT: &str = r#"You are a training data synthesis expert. Given a document chunk, generate high-quality training examples in JSON format.

Output format:
{
  "qa_pairs": [{"question": "...", "answer": "...", "relevance": 0.0-1.0}],
  "instructions": [{"instruction": "...", "response": "..."}],
  "completions": [{"context": "...", "continuation": "..."}]
}

CRITICAL: Include a "relevance" score (0.0-1.0) for each Q&A pair:
- 0.8-1.0: High confidence - answer is clearly supported by the text
- 0.5-0.8: Medium confidence - answer is partially supported
- 0.2-0.5: Low confidence - answer requires inference beyond the text
- 0.0-0.2: Very low confidence - cannot reliably answer from this text

Guidelines:
- Generate 2-4 Q&A pairs that test understanding of the content
- Generate 1-2 instruction-following examples (explain, summarize, compare, etc.)
- Generate 1-2 completion examples (context + continuation)
- Ensure answers are grounded in the source text
- Use varied question types and instruction verbs
- Be honest about relevance - low scores are valuable for teaching uncertainty
- If content is unclear/incomplete, still generate Q&A but with LOW relevance
- Output valid JSON only"#;

const DEFAULT_USER_TEMPLATE: &str = r#"Document chunk:
```
{chunk}
```

Generate training examples:"#;

/// Engine for synthesizing training data from document chunks
pub struct SynthesisEngine {
    config: SynthesisEngineConfig,
    parser: SynthesisOutputParser,
    /// Inference backend for text generation (MLX/CoreML/Metal)
    backend: Option<Arc<RwLock<KernelBox>>>,
}

impl SynthesisEngine {
    /// Create a new synthesis engine with the given configuration
    pub fn new(config: SynthesisEngineConfig) -> Self {
        Self {
            config,
            parser: SynthesisOutputParser::default(),
            backend: None,
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(SynthesisEngineConfig::default())
    }

    /// Load the synthesis model
    ///
    /// This initializes the inference backend and loads the model.
    /// Must be called before `synthesize()`.
    pub async fn load_model(&mut self) -> Result<()> {
        if !self.config.model_path.exists() {
            return Err(AosError::NotFound(format!(
                "Synthesis model not found at: {}",
                self.config.model_path.display()
            )));
        }

        info!(
            model_path = %self.config.model_path.display(),
            use_ane = self.config.use_ane,
            "Loading synthesis model"
        );

        // Create model configuration
        let mut model_config = ModelConfig::new(self.config.model_path.clone());
        model_config.backend = if self.config.use_ane {
            BackendPreference::CoreML
        } else {
            BackendPreference::Mlx
        };

        // Create the backend
        let backend = create_backend_from_config(&model_config).map_err(|e| {
            AosError::config(format!(
                "Failed to create inference backend for synthesis: {}",
                e
            ))
        })?;

        // Verify the backend supports text generation
        if !backend.supports_streaming_text_generation() {
            warn!("Backend does not support streaming text generation, falling back to batch mode");
        }

        info!("Synthesis model loaded successfully");

        self.backend = Some(Arc::new(RwLock::new(backend)));
        Ok(())
    }

    /// Check if the model is loaded
    pub fn is_loaded(&self) -> bool {
        self.backend.is_some()
    }

    /// Synthesize training data from a single chunk
    pub async fn synthesize(&self, request: SynthesisRequest) -> Result<SynthesisResult> {
        let start = Instant::now();

        // Build prompt
        let prompt = self.build_prompt(&request);

        // Generate output
        let raw_output = self.generate(&prompt).await?;

        let latency_ms = start.elapsed().as_millis() as u64;

        // Parse output
        match self.parser.parse(&raw_output) {
            Ok(output) => {
                debug!(
                    source = %request.source,
                    examples = output.total_examples(),
                    latency_ms = latency_ms,
                    "Synthesis successful"
                );
                Ok(SynthesisResult::success(
                    request, output, raw_output, latency_ms,
                ))
            }
            Err(e) => {
                warn!(
                    source = %request.source,
                    error = %e,
                    latency_ms = latency_ms,
                    "Synthesis parse failed"
                );
                Ok(SynthesisResult::parse_failure(
                    request, raw_output, latency_ms,
                ))
            }
        }
    }

    /// Synthesize training data from multiple chunks
    pub async fn synthesize_batch(
        &self,
        requests: Vec<SynthesisRequest>,
    ) -> Result<(Vec<SynthesisResult>, SynthesisBatchStats)> {
        let mut results = Vec::with_capacity(requests.len());
        let mut stats = SynthesisBatchStats::default();

        for (i, request) in requests.into_iter().enumerate() {
            debug!(
                chunk = i + 1,
                source = %request.source,
                "Processing chunk"
            );

            let result = self.synthesize(request).await?;
            stats.add_result(&result);
            results.push(result);
        }

        info!(
            chunks = stats.chunks_processed,
            examples = stats.total_examples(),
            success_rate = format!("{:.1}%", stats.success_rate() * 100.0),
            avg_latency_ms = stats.avg_latency_ms(),
            "Batch synthesis complete"
        );

        Ok((results, stats))
    }

    /// Build the full prompt for synthesis
    fn build_prompt(&self, request: &SynthesisRequest) -> String {
        let user_content = self.config.user_template.replace("{chunk}", &request.chunk);

        // For chat models, format as messages
        // This is a simplified version; actual implementation would use tokenizer's
        // chat template
        format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            self.config.system_prompt, user_content
        )
    }

    /// Generate text from the model
    async fn generate(&self, prompt: &str) -> Result<String> {
        let backend = self.backend.as_ref().ok_or_else(|| {
            AosError::config("Synthesis model not loaded. Call load_model() first.")
        })?;

        // Run inference on the backend
        let backend_guard: tokio::sync::RwLockReadGuard<'_, KernelBox> = backend.read().await;

        debug!(
            prompt_len = prompt.len(),
            max_tokens = self.config.max_new_tokens,
            temperature = self.config.temperature,
            "Running synthesis inference"
        );

        let result = backend_guard.generate_text_complete(
            prompt,
            self.config.max_new_tokens,
            self.config.temperature,
            self.config.top_p,
        )?;

        debug!(
            output_len = result.text.len(),
            tokens = result.tokens_generated,
            "Synthesis generation complete"
        );

        Ok(result.text)
    }

    /// Generate text from the model (stub mode for testing without model)
    #[cfg(test)]
    async fn generate_stub(&self, _prompt: &str) -> Result<String> {
        // Return stub JSON for testing
        let stub_output = r#"{
  "qa_pairs": [
    {
      "question": "What does this document describe?",
      "answer": "This is a placeholder answer for testing."
    }
  ],
  "instructions": [
    {
      "instruction": "Summarize the key points of this document.",
      "response": "This is a placeholder summary for testing."
    }
  ],
  "completions": [
    {
      "context": "The document discusses",
      "continuation": "placeholder content for testing."
    }
  ]
}"#;
        Ok(stub_output.to_string())
    }

    /// Get configuration
    pub fn config(&self) -> &SynthesisEngineConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: SynthesisEngineConfig) {
        self.config = config;
    }
}

/// Create a synthesis request from a document chunk
pub fn create_synthesis_request(chunk: &str, source: &str) -> SynthesisRequest {
    SynthesisRequest::new(chunk, source)
}

/// Create a synthesis request with full provenance tracking
pub fn create_synthesis_request_with_provenance(
    chunk: &str,
    provenance: super::types::ExampleProvenance,
) -> SynthesisRequest {
    let source = provenance.source_file.clone();
    SynthesisRequest::with_provenance(chunk, source, provenance)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test engine that uses stub generation (no model required)
    struct TestSynthesisEngine {
        inner: SynthesisEngine,
    }

    impl TestSynthesisEngine {
        fn new() -> Self {
            Self {
                inner: SynthesisEngine::with_defaults(),
            }
        }

        async fn synthesize(&self, request: SynthesisRequest) -> Result<SynthesisResult> {
            let start = Instant::now();
            let prompt = self.inner.build_prompt(&request);
            let raw_output = self.inner.generate_stub(&prompt).await?;
            let latency_ms = start.elapsed().as_millis() as u64;

            match self.inner.parser.parse(&raw_output) {
                Ok(output) => Ok(SynthesisResult::success(
                    request, output, raw_output, latency_ms,
                )),
                Err(_) => Ok(SynthesisResult::parse_failure(
                    request, raw_output, latency_ms,
                )),
            }
        }

        async fn synthesize_batch(
            &self,
            requests: Vec<SynthesisRequest>,
        ) -> Result<(Vec<SynthesisResult>, SynthesisBatchStats)> {
            let mut results = Vec::with_capacity(requests.len());
            let mut stats = SynthesisBatchStats::default();

            for request in requests {
                let result = self.synthesize(request).await?;
                stats.add_result(&result);
                results.push(result);
            }

            Ok((results, stats))
        }
    }

    #[tokio::test]
    async fn test_synthesis_engine_stub() {
        let engine = TestSynthesisEngine::new();

        let request = create_synthesis_request(
            "AdapterOS uses BLAKE3 hashing for content integrity.",
            "test/doc.md:chunk_0",
        );

        let result = engine.synthesize(request).await.unwrap();

        assert!(result.parse_success);
        assert!(!result.output.is_empty());
        assert!(!result.output.qa_pairs.is_empty());
    }

    #[tokio::test]
    async fn test_synthesis_batch() {
        let engine = TestSynthesisEngine::new();

        let requests = vec![
            create_synthesis_request("First chunk content.", "doc.md:0"),
            create_synthesis_request("Second chunk content.", "doc.md:1"),
        ];

        let (results, stats) = engine.synthesize_batch(requests).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(stats.chunks_processed, 2);
        assert!(stats.success_rate() > 0.0);
    }

    #[test]
    fn test_build_prompt() {
        let engine = SynthesisEngine::with_defaults();
        let request = create_synthesis_request("Test content", "test.md");

        let prompt = engine.build_prompt(&request);

        assert!(prompt.contains("system"));
        assert!(prompt.contains("Test content"));
        assert!(prompt.contains("assistant"));
    }

    #[test]
    fn test_is_loaded_false_initially() {
        let engine = SynthesisEngine::with_defaults();
        assert!(!engine.is_loaded());
    }
}
