//! Synthesis engine for generating training data from documents
//!
//! The SynthesisEngine supports two backend paths for inference:
//!
//! - **CoreML/KernelBox** (default): Runs through the `FusedKernels` trait via
//!   `create_backend_from_config`. Uses ANE acceleration when `use_ane` is true.
//!   Non-deterministic (hardware-dependent scheduling on ANE).
//!
//! - **MLX direct** (feature `mlx-synthesis`): Loads `MLXFFIModel` directly and
//!   calls `generate_with_config()` with `GenerationConfig.seed` set to an
//!   explicit 32-byte seed for fully deterministic output. Selected when
//!   `enrichment_mode` is `EnrichmentMode::StrictReplay`.

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

// =============================================================================
// Enrichment Mode
// =============================================================================

/// Controls which inference backend the synthesis engine uses.
///
/// - `PowerSaveCoreml`: Uses KernelBox via CoreML/ANE -- power-efficient
///   but not bit-exact reproducible across runs.
/// - `StrictReplay`: Uses MLX FFI model directly with an explicit 32-byte seed
///   for fully deterministic output. Requires feature `mlx-synthesis`.
/// - `Off`: No enrichment -- pass-through without synthesis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentMode {
    /// Power-efficient CoreML/ANE path via KernelBox (non-deterministic)
    PowerSaveCoreml,
    /// Deterministic MLX path with explicit seed (requires `mlx-synthesis` feature)
    StrictReplay,
    /// No enrichment -- pass-through without synthesis
    #[default]
    Off,
}

impl std::fmt::Display for EnrichmentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StrictReplay => write!(f, "strict_replay"),
            Self::PowerSaveCoreml => write!(f, "power_save_coreml"),
            Self::Off => write!(f, "off"),
        }
    }
}

impl std::str::FromStr for EnrichmentMode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "strict_replay" => Ok(Self::StrictReplay),
            "power_save_coreml" => Ok(Self::PowerSaveCoreml),
            "off" => Ok(Self::Off),
            other => Err(format!(
                "Invalid enrichment mode: '{}'. Must be one of: strict_replay, power_save_coreml, off",
                other
            )),
        }
    }
}

// =============================================================================
// Backend Mode (internal)
// =============================================================================

/// Internal enum holding the loaded backend.
enum SynthesisBackendMode {
    /// CoreML / generic KernelBox backend
    CoremlKernelBox(Arc<RwLock<KernelBox>>),
    /// MLX FFI model loaded directly (feature-gated)
    #[cfg(feature = "mlx-synthesis")]
    MlxModelDirect(Arc<adapteros_lora_mlx_ffi::MLXFFIModel>),
}

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for the synthesis engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisEngineConfig {
    /// Path to the model package
    pub model_path: PathBuf,
    /// Maximum tokens to generate per chunk
    pub max_new_tokens: usize,
    /// Temperature for generation (higher = more diverse)
    pub temperature: f32,
    /// Top-p (nucleus) sampling
    pub top_p: f32,
    /// Whether to use ANE acceleration (only relevant for CoreML path)
    pub use_ane: bool,
    /// Enrichment mode -- selects the backend path
    pub enrichment_mode: EnrichmentMode,
    /// System prompt for synthesis
    pub system_prompt: String,
    /// User prompt template (use {chunk} placeholder)
    pub user_template: String,
}

impl Default for SynthesisEngineConfig {
    fn default() -> Self {
        Self {
            model_path: adapteros_core::rebase_var_path("var/models/synthesis_model.mlpackage"),
            max_new_tokens: 1024,
            temperature: 0.7,
            top_p: 0.9,
            use_ane: true,
            enrichment_mode: EnrichmentMode::default(),
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

// =============================================================================
// Engine
// =============================================================================

/// Engine for synthesizing training data from document chunks
pub struct SynthesisEngine {
    config: SynthesisEngineConfig,
    parser: SynthesisOutputParser,
    /// Loaded inference backend (populated by `load_model()`)
    backend: Option<SynthesisBackendMode>,
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

    /// Load the synthesis model.
    ///
    /// Dispatches to the appropriate backend based on `enrichment_mode`:
    /// - `PowerSaveCoreml` -> loads via `create_backend_from_config` (KernelBox)
    /// - `StrictReplay` -> loads `MLXFFIModel` directly (requires `mlx-synthesis`)
    /// - `Off` -> skips model loading entirely
    ///
    /// Must be called before `synthesize()`.
    pub async fn load_model(&mut self) -> Result<()> {
        if self.config.enrichment_mode == EnrichmentMode::Off {
            info!("Enrichment mode is Off -- skipping model load");
            return Ok(());
        }

        if !self.config.model_path.exists() {
            return Err(AosError::NotFound(format!(
                "Synthesis model not found at: {}",
                self.config.model_path.display()
            )));
        }

        match self.config.enrichment_mode {
            EnrichmentMode::StrictReplay => {
                self.load_mlx_direct().await?;
            }
            EnrichmentMode::PowerSaveCoreml => {
                self.load_kernelbox().await?;
            }
            EnrichmentMode::Off => unreachable!(),
        }

        Ok(())
    }

    /// Load the CoreML/KernelBox backend
    async fn load_kernelbox(&mut self) -> Result<()> {
        info!(
            model_path = %self.config.model_path.display(),
            use_ane = self.config.use_ane,
            "Loading synthesis model via KernelBox (CoreML path)"
        );

        let mut model_config = ModelConfig::new(self.config.model_path.clone());
        model_config.backend = if self.config.use_ane {
            BackendPreference::CoreML
        } else {
            BackendPreference::Mlx
        };

        let backend = create_backend_from_config(&model_config).map_err(|e| {
            AosError::config(format!(
                "Failed to create inference backend for synthesis: {}",
                e
            ))
        })?;

        if !backend.supports_streaming_text_generation() {
            warn!("Backend does not support streaming text generation, falling back to batch mode");
        }

        info!("Synthesis model loaded successfully (KernelBox)");
        self.backend = Some(SynthesisBackendMode::CoremlKernelBox(Arc::new(
            RwLock::new(backend),
        )));
        Ok(())
    }

    /// Load the MLX FFI model directly for deterministic inference.
    ///
    /// Only available when the `mlx-synthesis` feature is enabled.
    #[cfg(feature = "mlx-synthesis")]
    async fn load_mlx_direct(&mut self) -> Result<()> {
        info!(
            model_path = %self.config.model_path.display(),
            "Loading synthesis model via MLXFFIModel (StrictReplay path)"
        );

        let model =
            adapteros_lora_mlx_ffi::MLXFFIModel::load(&self.config.model_path).map_err(|e| {
                AosError::config(format!(
                    "Failed to load MLX model for strict-replay synthesis: {}",
                    e
                ))
            })?;

        info!("Synthesis model loaded successfully (MLX direct)");
        self.backend = Some(SynthesisBackendMode::MlxModelDirect(Arc::new(model)));
        Ok(())
    }

    #[cfg(not(feature = "mlx-synthesis"))]
    async fn load_mlx_direct(&mut self) -> Result<()> {
        Err(AosError::config(
            "StrictReplay enrichment mode requires the `mlx-synthesis` feature. \
             Rebuild with `--features mlx-synthesis`."
                .to_string(),
        ))
    }

    /// Check if the model is loaded
    pub fn is_loaded(&self) -> bool {
        self.backend.is_some()
    }

    /// Synthesize training data from a single chunk.
    ///
    /// Pass `seed` for deterministic generation (StrictReplay mode). The seed
    /// is ignored when using the KernelBox path.
    pub async fn synthesize(
        &self,
        request: SynthesisRequest,
        seed: Option<[u8; 32]>,
    ) -> Result<SynthesisResult> {
        let start = Instant::now();

        let prompt = self.build_prompt(&request);
        let raw_output = self.generate(&prompt, seed).await?;
        let latency_ms = start.elapsed().as_millis() as u64;

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

    /// Synthesize training data from multiple chunks (unseeded).
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

            let result = self.synthesize(request, None).await?;
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

    /// Synthesize training data from multiple chunks with per-chunk seeds.
    ///
    /// Each request gets its own 32-byte seed for deterministic generation.
    /// `seeds` must be the same length as `requests`.
    pub async fn synthesize_batch_with_seeds(
        &self,
        requests: Vec<SynthesisRequest>,
        seeds: Vec<[u8; 32]>,
    ) -> Result<(Vec<SynthesisResult>, SynthesisBatchStats)> {
        if requests.len() != seeds.len() {
            return Err(AosError::Validation(format!(
                "synthesize_batch_with_seeds: requests ({}) and seeds ({}) length mismatch",
                requests.len(),
                seeds.len(),
            )));
        }

        let mut results = Vec::with_capacity(requests.len());
        let mut stats = SynthesisBatchStats::default();

        for (i, (request, seed)) in requests.into_iter().zip(seeds.into_iter()).enumerate() {
            debug!(
                chunk = i + 1,
                source = %request.source,
                seed_prefix = hex::encode(&seed[..4]),
                "Processing chunk (seeded)"
            );

            let result = self.synthesize(request, Some(seed)).await?;
            stats.add_result(&result);
            results.push(result);
        }

        info!(
            chunks = stats.chunks_processed,
            examples = stats.total_examples(),
            success_rate = format!("{:.1}%", stats.success_rate() * 100.0),
            avg_latency_ms = stats.avg_latency_ms(),
            "Seeded batch synthesis complete"
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

    /// Generate text from the loaded backend.
    ///
    /// Dispatches to KernelBox or MLX direct depending on which backend was loaded.
    /// The `seed` parameter is only used by the MLX direct path; the KernelBox
    /// path ignores it (ANE scheduling is not seed-controllable).
    async fn generate(&self, prompt: &str, seed: Option<[u8; 32]>) -> Result<String> {
        let backend = self.backend.as_ref().ok_or_else(|| {
            AosError::config("Synthesis model not loaded. Call load_model() first.")
        })?;

        debug!(
            prompt_len = prompt.len(),
            max_tokens = self.config.max_new_tokens,
            temperature = self.config.temperature,
            has_seed = seed.is_some(),
            "Running synthesis inference"
        );

        match backend {
            SynthesisBackendMode::CoremlKernelBox(kb) => {
                let backend_guard = kb.read().await;
                let result = backend_guard.generate_text_complete(
                    prompt,
                    self.config.max_new_tokens,
                    self.config.temperature,
                    self.config.top_p,
                )?;

                debug!(
                    output_len = result.text.len(),
                    tokens = result.tokens_generated,
                    "Synthesis generation complete (KernelBox)"
                );

                Ok(result.text)
            }

            #[cfg(feature = "mlx-synthesis")]
            SynthesisBackendMode::MlxModelDirect(model) => {
                use adapteros_lora_mlx_ffi::generation::GenerationConfig;

                let gen_config = GenerationConfig {
                    max_tokens: self.config.max_new_tokens,
                    temperature: self.config.temperature,
                    top_p: Some(self.config.top_p),
                    seed,
                    ..Default::default()
                };

                let text = model.generate_with_config(prompt, gen_config)?;

                debug!(
                    output_len = text.len(),
                    "Synthesis generation complete (MLX direct)"
                );

                Ok(text)
            }
        }
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
            "adapterOS uses BLAKE3 hashing for content integrity.",
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

    #[test]
    fn test_enrichment_mode_default() {
        let config = SynthesisEngineConfig::default();
        assert_eq!(config.enrichment_mode, EnrichmentMode::Off);
    }

    #[test]
    fn test_enrichment_mode_serde_roundtrip() {
        let json = serde_json::to_string(&EnrichmentMode::StrictReplay).unwrap();
        assert_eq!(json, "\"strict_replay\"");
        let back: EnrichmentMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, EnrichmentMode::StrictReplay);

        let json2 = serde_json::to_string(&EnrichmentMode::PowerSaveCoreml).unwrap();
        assert_eq!(json2, "\"power_save_coreml\"");

        let json3 = serde_json::to_string(&EnrichmentMode::Off).unwrap();
        assert_eq!(json3, "\"off\"");
    }
}
