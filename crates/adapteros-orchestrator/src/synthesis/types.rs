//! Types for training data synthesis
//!
//! This module provides automatic sample_role classification based on
//! model confidence (relevance scores). Users don't need to manually
//! label positive/negative - the synthesis engine handles it automatically.
//!
//! # AARA Lifecycle Support
//!
//! These types support the Anchor-Audit-Rectify-Act lifecycle:
//! - **Anchor**: `ExampleProvenance` tracks source document, chunk, and line numbers
//! - **Audit**: Provenance enables tracing from inference back to source
//! - **Rectify**: Source hashes enable detecting when documents change
//! - **Act**: `SampleRole` and relevance enable confidence-aware abstention

use serde::{Deserialize, Serialize};

// =============================================================================
// Confidence Thresholds for Auto-Classification
// =============================================================================

/// Threshold above which examples are classified as positive (high confidence)
pub const POSITIVE_THRESHOLD: f32 = 0.6;

/// Threshold below which examples generate abstention responses (low confidence)
pub const ABSTENTION_THRESHOLD: f32 = 0.3;

/// Sample role for training - determines how the example is used
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SampleRole {
    /// Positive examples teach the model correct behavior
    #[default]
    Positive,
    /// Negative examples teach the model to abstain when uncertain
    Negative,
}

impl SampleRole {
    /// Classify based on relevance/confidence score
    ///
    /// - relevance >= 0.6 → Positive (confident, teach this)
    /// - relevance < 0.3 → Negative (uncertain, teach abstention)
    /// - 0.3 <= relevance < 0.6 → Positive with lower weight
    pub fn from_relevance(relevance: f32) -> Self {
        if relevance >= POSITIVE_THRESHOLD {
            SampleRole::Positive
        } else if relevance < ABSTENTION_THRESHOLD {
            SampleRole::Negative
        } else {
            // Middle ground - still positive but will have lower weight
            SampleRole::Positive
        }
    }

    /// Get the string representation for metadata
    pub fn as_str(&self) -> &'static str {
        match self {
            SampleRole::Positive => "positive",
            SampleRole::Negative => "negative",
        }
    }
}

impl std::fmt::Display for SampleRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =============================================================================
// Provenance Tracking (ANCHOR)
// =============================================================================

/// Full provenance information for a training example
///
/// This struct enables the ANCHOR phase of the AARA lifecycle by tracking
/// exactly where each training example came from, including:
/// - Source document identity and content hash
/// - Chunk location within the document
/// - Line numbers for precise citation
/// - Synthesis model information for reproducibility
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExampleProvenance {
    /// Path or identifier of the source document
    pub source_file: String,
    /// BLAKE3 hash of the entire source document
    pub source_hash_b3: String,
    /// Index of the chunk within the document
    pub chunk_index: usize,
    /// BLAKE3 hash of the chunk text
    pub chunk_hash_b3: String,
    /// Starting line number in the source document (1-indexed)
    #[serde(default)]
    pub line_start: Option<u32>,
    /// Ending line number in the source document (1-indexed)
    #[serde(default)]
    pub line_end: Option<u32>,
    /// Character offset start within the document
    #[serde(default)]
    pub char_start: Option<usize>,
    /// Character offset end within the document
    #[serde(default)]
    pub char_end: Option<usize>,
    /// ID of the synthesis model that generated this example
    #[serde(default)]
    pub synthesis_model_id: Option<String>,
    /// Seed used for deterministic synthesis
    #[serde(default)]
    pub synthesis_seed: Option<u64>,
    /// Timestamp when the example was synthesized (Unix ms)
    #[serde(default)]
    pub synthesized_at_ms: Option<u64>,
}

impl ExampleProvenance {
    /// Create a new provenance with minimal required fields
    pub fn new(source_file: impl Into<String>, chunk_index: usize) -> Self {
        Self {
            source_file: source_file.into(),
            chunk_index,
            ..Default::default()
        }
    }

    /// Builder: set source hash
    pub fn with_source_hash(mut self, hash: impl Into<String>) -> Self {
        self.source_hash_b3 = hash.into();
        self
    }

    /// Builder: set chunk hash
    pub fn with_chunk_hash(mut self, hash: impl Into<String>) -> Self {
        self.chunk_hash_b3 = hash.into();
        self
    }

    /// Builder: set line range
    pub fn with_lines(mut self, start: u32, end: u32) -> Self {
        self.line_start = Some(start);
        self.line_end = Some(end);
        self
    }

    /// Builder: set character range
    pub fn with_char_range(mut self, start: usize, end: usize) -> Self {
        self.char_start = Some(start);
        self.char_end = Some(end);
        self
    }

    /// Builder: set synthesis model info
    pub fn with_synthesis_info(mut self, model_id: impl Into<String>, seed: u64) -> Self {
        self.synthesis_model_id = Some(model_id.into());
        self.synthesis_seed = Some(seed);
        self
    }

    /// Builder: set synthesis timestamp
    pub fn with_timestamp(mut self, timestamp_ms: u64) -> Self {
        self.synthesized_at_ms = Some(timestamp_ms);
        self
    }

    /// Convert to canonical JSON string for storage
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Parse from JSON string
    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }

    /// Check if this provenance has sufficient information for audit
    pub fn is_auditable(&self) -> bool {
        !self.source_file.is_empty() && !self.source_hash_b3.is_empty()
    }

    /// Get a human-readable citation string
    pub fn citation(&self) -> String {
        let mut citation = self.source_file.clone();
        if let (Some(start), Some(end)) = (self.line_start, self.line_end) {
            citation.push_str(&format!(":L{}-{}", start, end));
        }
        citation.push_str(&format!(" (chunk {})", self.chunk_index));
        citation
    }
}

/// Request for synthesizing training data from a document chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisRequest {
    /// The document chunk text to synthesize from
    pub chunk: String,
    /// Source metadata (file path, chunk index, etc.)
    pub source: String,
    /// Optional additional context
    pub context: Option<String>,
    /// Full provenance information for ANCHOR tracking
    #[serde(default)]
    pub provenance: Option<ExampleProvenance>,
}

impl SynthesisRequest {
    /// Create a basic request without provenance
    pub fn new(chunk: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            chunk: chunk.into(),
            source: source.into(),
            context: None,
            provenance: None,
        }
    }

    /// Create a request with full provenance
    pub fn with_provenance(
        chunk: impl Into<String>,
        source: impl Into<String>,
        provenance: ExampleProvenance,
    ) -> Self {
        Self {
            chunk: chunk.into(),
            source: source.into(),
            context: None,
            provenance: Some(provenance),
        }
    }

    /// Builder: add context
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// A question-answer pair extracted from the document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QAPair {
    /// The question about the document content
    pub question: String,
    /// The answer grounded in the document
    pub answer: String,
    /// Optional relevance score (0.0 - 1.0)
    #[serde(default)]
    pub relevance: Option<f32>,
}

/// An instruction-following example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionExample {
    /// The instruction/task to perform
    pub instruction: String,
    /// The response to the instruction
    pub response: String,
    /// Type of instruction (explain, summarize, compare, etc.)
    #[serde(default)]
    pub instruction_type: Option<String>,
}

/// A completion example (context → continuation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionExample {
    /// The context/prefix
    pub context: String,
    /// The expected continuation
    pub continuation: String,
}

/// Structured output from the synthesis model
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SynthesisOutput {
    /// Question-answer pairs
    #[serde(default)]
    pub qa_pairs: Vec<QAPair>,
    /// Instruction-following examples
    #[serde(default)]
    pub instructions: Vec<InstructionExample>,
    /// Completion examples
    #[serde(default)]
    pub completions: Vec<CompletionExample>,
}

impl SynthesisOutput {
    /// Total number of examples across all types
    pub fn total_examples(&self) -> usize {
        self.qa_pairs.len() + self.instructions.len() + self.completions.len()
    }

    /// Check if the output is empty
    pub fn is_empty(&self) -> bool {
        self.qa_pairs.is_empty() && self.instructions.is_empty() && self.completions.is_empty()
    }

    /// Convert to training examples with automatic sample_role classification
    ///
    /// Examples are automatically classified as positive or negative based on
    /// the relevance scores from the synthesis model. Low-confidence examples
    /// are converted to abstention training (negative sample_role).
    pub fn to_training_examples(&self, source: &str) -> Vec<TrainingExample> {
        let mut examples = Vec::with_capacity(self.total_examples() * 2); // Extra space for abstention

        // Convert Q&A pairs with auto-classification
        for (i, qa) in self.qa_pairs.iter().enumerate() {
            let relevance = qa.relevance;

            // Create the main example (auto-classified)
            examples.push(TrainingExample::new(
                qa.question.clone(),
                qa.answer.clone(),
                ExampleType::QuestionAnswer,
                source.to_string(),
                i,
                relevance,
            ));

            // For low-confidence Q&A, also generate explicit abstention example
            if relevance.map(|r| r < ABSTENTION_THRESHOLD).unwrap_or(false) {
                examples.push(TrainingExample::new(
                    qa.question.clone(),
                    format!(
                        "I don't have enough information to answer this question confidently based on the available documentation."
                    ),
                    ExampleType::QuestionAnswer,
                    source.to_string(),
                    i,
                    Some(0.1), // Very low relevance → negative
                ));
            }
        }

        // Convert instructions (assume high confidence unless model says otherwise)
        for (i, inst) in self.instructions.iter().enumerate() {
            examples.push(TrainingExample::new(
                inst.instruction.clone(),
                inst.response.clone(),
                ExampleType::Instruction,
                source.to_string(),
                self.qa_pairs.len() + i,
                Some(0.8), // Default high confidence for instructions
            ));
        }

        // Convert completions (assume high confidence)
        for (i, comp) in self.completions.iter().enumerate() {
            examples.push(TrainingExample::new(
                comp.context.clone(),
                comp.continuation.clone(),
                ExampleType::Completion,
                source.to_string(),
                self.qa_pairs.len() + self.instructions.len() + i,
                Some(0.8), // Default high confidence for completions
            ));
        }

        examples
    }

    /// Get counts of positive and negative examples
    pub fn count_by_role(&self, source: &str) -> (usize, usize) {
        let examples = self.to_training_examples(source);
        let positive = examples.iter().filter(|e| e.is_positive()).count();
        let negative = examples.iter().filter(|e| e.is_negative()).count();
        (positive, negative)
    }
}

/// Type of training example
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExampleType {
    /// Question-answer pair
    QuestionAnswer,
    /// Instruction-following
    Instruction,
    /// Completion/continuation
    Completion,
}

impl std::fmt::Display for ExampleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExampleType::QuestionAnswer => write!(f, "qa"),
            ExampleType::Instruction => write!(f, "instruction"),
            ExampleType::Completion => write!(f, "completion"),
        }
    }
}

/// A training example ready for JSONL output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingExample {
    /// Input prompt
    pub prompt: String,
    /// Target response
    pub response: String,
    /// Type of example
    pub example_type: ExampleType,
    /// Source document/chunk (legacy field, use provenance for full info)
    pub source: String,
    /// Index within the source
    pub index: usize,
    /// Weight for training (default 1.0)
    #[serde(default = "default_weight")]
    pub weight: f32,
    /// Sample role - auto-classified from relevance score
    #[serde(default)]
    pub sample_role: SampleRole,
    /// Original relevance/confidence score from synthesis
    #[serde(default)]
    pub relevance: Option<f32>,
    /// Full provenance for ANCHOR tracking and AUDIT queries
    #[serde(default)]
    pub provenance: Option<ExampleProvenance>,
}

fn default_weight() -> f32 {
    1.0
}

impl TrainingExample {
    /// Create a new training example with auto-classification
    pub fn new(
        prompt: String,
        response: String,
        example_type: ExampleType,
        source: String,
        index: usize,
        relevance: Option<f32>,
    ) -> Self {
        let rel = relevance.unwrap_or(1.0);
        let sample_role = SampleRole::from_relevance(rel);

        // Weight is based on relevance - lower relevance = lower weight
        let weight = if sample_role == SampleRole::Negative {
            -0.5 // Negative weight for abstention examples
        } else {
            rel.clamp(0.3, 1.0) // Scale weight by confidence
        };

        Self {
            prompt,
            response,
            example_type,
            source,
            index,
            weight,
            sample_role,
            relevance,
            provenance: None,
        }
    }

    /// Create a new training example with full provenance
    pub fn with_provenance(
        prompt: String,
        response: String,
        example_type: ExampleType,
        provenance: ExampleProvenance,
        index: usize,
        relevance: Option<f32>,
    ) -> Self {
        let source = provenance.source_file.clone();
        let mut example = Self::new(prompt, response, example_type, source, index, relevance);
        example.provenance = Some(provenance);
        example
    }

    /// Convert to JSONL format for training
    pub fn to_jsonl(&self) -> String {
        let mut metadata = serde_json::json!({
            "type": self.example_type.to_string(),
            "source": self.source,
            "index": self.index,
            "sample_role": self.sample_role.as_str(),
            "relevance": self.relevance,
        });

        // Include provenance if available
        if let Some(ref prov) = self.provenance {
            metadata["provenance"] = serde_json::json!({
                "source_file": prov.source_file,
                "source_hash_b3": prov.source_hash_b3,
                "chunk_index": prov.chunk_index,
                "chunk_hash_b3": prov.chunk_hash_b3,
                "line_start": prov.line_start,
                "line_end": prov.line_end,
            });
        }

        serde_json::json!({
            "prompt": self.prompt,
            "response": self.response,
            "weight": self.weight,
            "metadata": metadata
        })
        .to_string()
    }

    /// Check if this is a positive example
    pub fn is_positive(&self) -> bool {
        self.sample_role == SampleRole::Positive
    }

    /// Check if this is a negative/abstention example
    pub fn is_negative(&self) -> bool {
        self.sample_role == SampleRole::Negative
    }

    /// Check if this example has auditable provenance
    pub fn is_auditable(&self) -> bool {
        self.provenance
            .as_ref()
            .map(|p| p.is_auditable())
            .unwrap_or(false)
    }

    /// Get citation string if provenance available
    pub fn citation(&self) -> Option<String> {
        self.provenance.as_ref().map(|p| p.citation())
    }
}

/// Result of synthesis for a single chunk
#[derive(Debug, Clone)]
pub struct SynthesisResult {
    /// The synthesis request
    pub request: SynthesisRequest,
    /// Parsed synthesis output
    pub output: SynthesisOutput,
    /// Raw model output (for debugging)
    pub raw_output: String,
    /// Generation latency in milliseconds
    pub latency_ms: u64,
    /// Whether JSON parsing succeeded
    pub parse_success: bool,
}

impl SynthesisResult {
    /// Create a new successful result
    pub fn success(
        request: SynthesisRequest,
        output: SynthesisOutput,
        raw_output: String,
        latency_ms: u64,
    ) -> Self {
        Self {
            request,
            output,
            raw_output,
            latency_ms,
            parse_success: true,
        }
    }

    /// Create a failed result (could not parse JSON)
    pub fn parse_failure(request: SynthesisRequest, raw_output: String, latency_ms: u64) -> Self {
        Self {
            request,
            output: SynthesisOutput::default(),
            raw_output,
            latency_ms,
            parse_success: false,
        }
    }
}

/// Statistics about a synthesis batch
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SynthesisBatchStats {
    /// Number of chunks processed
    pub chunks_processed: usize,
    /// Number of successful JSON parses
    pub parse_successes: usize,
    /// Number of parse failures
    pub parse_failures: usize,
    /// Total Q&A pairs generated
    pub qa_pairs: usize,
    /// Total instructions generated
    pub instructions: usize,
    /// Total completions generated
    pub completions: usize,
    /// Total generation time in milliseconds
    pub total_latency_ms: u64,
    /// Positive examples (high confidence)
    pub positive_examples: usize,
    /// Negative examples (abstention/low confidence)
    pub negative_examples: usize,
}

impl SynthesisBatchStats {
    /// Add a synthesis result to the stats
    pub fn add_result(&mut self, result: &SynthesisResult) {
        self.chunks_processed += 1;
        self.total_latency_ms += result.latency_ms;

        if result.parse_success {
            self.parse_successes += 1;
            self.qa_pairs += result.output.qa_pairs.len();
            self.instructions += result.output.instructions.len();
            self.completions += result.output.completions.len();

            // Count by sample role
            let (pos, neg) = result.output.count_by_role(&result.request.source);
            self.positive_examples += pos;
            self.negative_examples += neg;
        } else {
            self.parse_failures += 1;
        }
    }

    /// Total examples generated
    pub fn total_examples(&self) -> usize {
        self.positive_examples + self.negative_examples
    }

    /// Parse success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f32 {
        if self.chunks_processed == 0 {
            0.0
        } else {
            self.parse_successes as f32 / self.chunks_processed as f32
        }
    }

    /// Average latency per chunk in milliseconds
    pub fn avg_latency_ms(&self) -> u64 {
        if self.chunks_processed == 0 {
            0
        } else {
            self.total_latency_ms / self.chunks_processed as u64
        }
    }

    /// Ratio of positive to total examples
    pub fn positive_ratio(&self) -> f32 {
        let total = self.total_examples();
        if total == 0 {
            0.0
        } else {
            self.positive_examples as f32 / total as f32
        }
    }
}
