//! Training data generation from ingested documents
//!
//! Converts document chunks into training examples for adapter fine-tuning.

use crate::types::{DocumentChunk, IngestedDocument};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokenizers::Tokenizer;
use tracing::{debug, info, warn};

/// Training example in the format expected by AdapterOS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingExample {
    /// Input token IDs
    pub input: Vec<u32>,
    /// Target token IDs
    pub target: Vec<u32>,
    /// Example metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Wrapper for batch of training examples (JSONL format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingData {
    pub examples: Vec<TrainingExample>,
}

/// Strategy for generating training examples from documents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingStrategy {
    /// Identity mapping: input = target (memorization)
    Identity,
    /// Question-answer pairs (requires prompt engineering)
    QuestionAnswer,
}

/// Configuration for training data generation
#[derive(Debug, Clone)]
pub struct TrainingGenConfig {
    pub strategy: TrainingStrategy,
    pub max_seq_length: usize,
    pub add_special_tokens: bool,
}

impl Default for TrainingGenConfig {
    fn default() -> Self {
        Self {
            strategy: TrainingStrategy::Identity,
            max_seq_length: 512,
            add_special_tokens: true,
        }
    }
}

/// Generate training examples from a document chunk
pub fn generate_examples_from_chunk(
    chunk: &DocumentChunk,
    document: &IngestedDocument,
    tokenizer: &Arc<Tokenizer>,
    config: &TrainingGenConfig,
) -> Result<Vec<TrainingExample>> {
    match config.strategy {
        TrainingStrategy::Identity => generate_identity_example(chunk, document, tokenizer, config),
        TrainingStrategy::QuestionAnswer => {
            generate_qa_examples(chunk, document, tokenizer, config)
        }
    }
}

/// Generate identity mapping: input = target (for memorization)
fn generate_identity_example(
    chunk: &DocumentChunk,
    document: &IngestedDocument,
    tokenizer: &Arc<Tokenizer>,
    config: &TrainingGenConfig,
) -> Result<Vec<TrainingExample>> {
    let encoding = tokenizer
        .encode(chunk.text.as_str(), config.add_special_tokens)
        .map_err(|e| AosError::Validation(format!("Failed to tokenize chunk: {e}")))?;

    let mut token_ids = encoding.get_ids().to_vec();

    // Truncate if necessary
    if token_ids.len() > config.max_seq_length {
        token_ids.truncate(config.max_seq_length);
        debug!(
            "Truncated chunk {} to {} tokens",
            chunk.chunk_index, config.max_seq_length
        );
    }

    let mut metadata = HashMap::new();
    metadata.insert("source".to_string(), document.source_name.clone());
    metadata.insert("chunk_index".to_string(), chunk.chunk_index.to_string());
    metadata.insert(
        "source_type".to_string(),
        document.source.as_str().to_string(),
    );
    if let Some(page) = chunk.page_number {
        metadata.insert("page".to_string(), page.to_string());
    }

    Ok(vec![TrainingExample {
        input: token_ids.clone(),
        target: token_ids,
        metadata: Some(metadata),
    }])
}

/// Generate question-answer pairs (basic implementation)
fn generate_qa_examples(
    chunk: &DocumentChunk,
    document: &IngestedDocument,
    tokenizer: &Arc<Tokenizer>,
    config: &TrainingGenConfig,
) -> Result<Vec<TrainingExample>> {
    // For a basic implementation, we'll create simple Q&A pairs
    // A more sophisticated implementation would use an LLM to generate questions

    let text = &chunk.text;

    // Split into sentences (simple heuristic)
    let sentences: Vec<&str> = text
        .split(['.', '?', '!'])
        .filter(|s| s.trim().len() > 20)
        .collect();

    if sentences.is_empty() {
        warn!(
            "No suitable sentences found in chunk {} for Q&A generation",
            chunk.chunk_index
        );
        return Ok(Vec::new());
    }

    let mut examples = Vec::new();

    // For each sentence, create a simple Q&A pair
    for (idx, sentence) in sentences.iter().take(3).enumerate() {
        let sentence = sentence.trim();
        if sentence.is_empty() {
            continue;
        }

        // Simple question template: "What does the document say about [topic]?"
        let question = format!("What does the document say? Answer: {}", sentence);

        let question_enc = tokenizer
            .encode(question.as_str(), config.add_special_tokens)
            .map_err(|e| AosError::Validation(format!("Failed to tokenize question: {e}")))?;

        let answer_enc = tokenizer
            .encode(sentence, config.add_special_tokens)
            .map_err(|e| AosError::Validation(format!("Failed to tokenize answer: {e}")))?;

        let mut input_ids = question_enc.get_ids().to_vec();
        let mut target_ids = answer_enc.get_ids().to_vec();

        // Truncate if necessary
        if input_ids.len() > config.max_seq_length {
            input_ids.truncate(config.max_seq_length);
        }
        if target_ids.len() > config.max_seq_length {
            target_ids.truncate(config.max_seq_length);
        }

        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), document.source_name.clone());
        metadata.insert("chunk_index".to_string(), chunk.chunk_index.to_string());
        metadata.insert("qa_index".to_string(), idx.to_string());
        metadata.insert("strategy".to_string(), "qa".to_string());
        metadata.insert("qa_question_text".to_string(), question.clone());
        metadata.insert("qa_answer_text".to_string(), sentence.to_string());

        examples.push(TrainingExample {
            input: input_ids,
            target: target_ids,
            metadata: Some(metadata),
        });
    }

    debug!(
        "Generated {} Q&A examples from chunk {}",
        examples.len(),
        chunk.chunk_index
    );

    Ok(examples)
}

/// Generate training data from an entire document
pub fn generate_training_data(
    document: &IngestedDocument,
    tokenizer: &Arc<Tokenizer>,
    config: &TrainingGenConfig,
) -> Result<TrainingData> {
    info!(
        "Generating training data from document {} using strategy {:?}",
        document.source_name, config.strategy
    );

    let mut all_examples = Vec::new();

    for chunk in &document.chunks {
        let examples = generate_examples_from_chunk(chunk, document, tokenizer, config)?;
        all_examples.extend(examples);
    }

    info!(
        "Generated {} training examples from {} chunks",
        all_examples.len(),
        document.chunks.len()
    );

    Ok(TrainingData {
        examples: all_examples,
    })
}

/// Generate training data from multiple documents
pub fn generate_training_data_from_documents(
    documents: &[IngestedDocument],
    tokenizer: &Arc<Tokenizer>,
    config: &TrainingGenConfig,
) -> Result<TrainingData> {
    let mut all_examples = Vec::new();

    for document in documents {
        let data = generate_training_data(document, tokenizer, config)?;
        all_examples.extend(data.examples);
    }

    info!(
        "Generated {} total training examples from {} documents",
        all_examples.len(),
        documents.len()
    );

    Ok(TrainingData {
        examples: all_examples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DocumentSource;
    use adapteros_core::B3Hash;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokenizers::models::wordlevel::WordLevel;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;
    use tokenizers::Tokenizer;

    #[test]
    fn test_generate_identity_example() {
        let tokenizer = fixture_tokenizer();

        let chunk = DocumentChunk::new(
            0,
            Some(1),
            0,
            50,
            "This is a test document for training data generation.".to_string(),
        )
        .with_total(1);

        let document = IngestedDocument {
            source: DocumentSource::Pdf,
            source_name: "test.pdf".to_string(),
            source_path: Some(PathBuf::from("var/test.pdf")),
            doc_hash: B3Hash::hash(b"test"),
            byte_len: 100,
            page_count: Some(1),
            chunks: vec![chunk.clone()],
        };

        let config = TrainingGenConfig::default();
        let examples = generate_examples_from_chunk(&chunk, &document, &tokenizer, &config)
            .expect("Failed to generate examples");

        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].input, examples[0].target);
        assert!(!examples[0].input.is_empty());

        let metadata = examples[0].metadata.as_ref().unwrap();
        assert_eq!(metadata.get("source").unwrap(), "test.pdf");
        assert_eq!(metadata.get("chunk_index").unwrap(), "0");
    }

    #[test]
    fn test_generate_qa_examples() {
        let tokenizer = fixture_tokenizer();

        let chunk = DocumentChunk::new(
            0,
            Some(1),
            0,
            100,
            "The capital of France is Paris. It is a beautiful city. Many tourists visit every year.".to_string(),
        )
        .with_total(1);

        let document = IngestedDocument {
            source: DocumentSource::Markdown,
            source_name: "facts.md".to_string(),
            source_path: None,
            doc_hash: B3Hash::hash(b"facts"),
            byte_len: 100,
            page_count: None,
            chunks: vec![chunk.clone()],
        };

        let config = TrainingGenConfig {
            strategy: TrainingStrategy::QuestionAnswer,
            ..Default::default()
        };

        let examples = generate_examples_from_chunk(&chunk, &document, &tokenizer, &config)
            .expect("Failed to generate Q&A examples");

        assert!(
            !examples.is_empty(),
            "Should generate at least one Q&A pair"
        );

        for example in &examples {
            assert!(!example.input.is_empty());
            assert!(!example.target.is_empty());
            let metadata = example.metadata.as_ref().unwrap();
            assert_eq!(metadata.get("strategy").unwrap(), "qa");
            assert!(
                metadata
                    .get("qa_question_text")
                    .map(|q| !q.is_empty())
                    .unwrap_or(false),
                "Question metadata should be populated"
            );
            assert!(
                metadata
                    .get("qa_answer_text")
                    .map(|a| !a.is_empty())
                    .unwrap_or(false),
                "Answer metadata should be populated"
            );
        }
    }

    fn fixture_tokenizer() -> Arc<Tokenizer> {
        let vocab = [("[UNK]".to_string(), 0u32), ("[PAD]".to_string(), 1u32)]
            .into_iter()
            .collect();
        let model = WordLevel::builder()
            .vocab(vocab)
            .unk_token("[UNK]".to_string())
            .build()
            .expect("wordlevel model");
        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Whitespace);
        Arc::new(tokenizer)
    }
}
