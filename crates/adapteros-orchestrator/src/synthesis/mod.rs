//! Training data synthesis from documents
//!
//! This module provides the infrastructure to convert document chunks into
//! high-quality training examples (Q&A pairs, instruction-following, completions)
//! using a small locally-running synthesis model on CoreML/ANE.
//!
//! # Architecture
//!
//! ```text
//! Document → Chunk → SynthesisEngine → JSON Output → Parser → TrainingExamples
//! ```
//!
//! The synthesis model is a fine-tuned Qwen2.5-1.5B that takes document chunks
//! and outputs structured JSON with multiple training example types.
//!
//! # Automatic Sample Role Classification
//!
//! Training examples are automatically classified as positive or negative based
//! on the model's confidence (relevance score):
//!
//! - **Positive** (relevance >= 0.6): High confidence, teaches correct behavior
//! - **Negative** (relevance < 0.3): Low confidence, teaches abstention
//!
//! Users don't need to manually label examples - the system handles it automatically.

mod engine;
mod parser;
mod types;

pub use engine::{
    create_synthesis_request, create_synthesis_request_with_provenance, SynthesisEngine,
    SynthesisEngineConfig,
};
pub use parser::{parse_synthesis_output, SynthesisOutputParser};
pub use types::{
    CompletionExample, ExampleProvenance, ExampleType, InstructionExample, QAPair, SampleRole,
    SynthesisBatchStats, SynthesisOutput, SynthesisRequest, SynthesisResult, TrainingExample,
    ABSTENTION_THRESHOLD, POSITIVE_THRESHOLD,
};
