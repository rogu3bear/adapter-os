//! Training data generation from ingested documents
//!
//! Converts document chunks into training examples for adapter fine-tuning.

#![allow(clippy::default_constructed_unit_structs)]

use crate::types::{DocumentChunk, IngestedDocument};
use adapteros_core::{AosError, B3Hash, Result};
pub use adapteros_types::training::TrainingExampleV1 as TrainingExample;
use adapteros_types::training::{provenance_from_map, ExampleMetadataV1};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokenizers::Tokenizer;
use tracing::{debug, info, warn};

fn deterministic_created_at_unix_ms(dataset_id: &str, row_id: u64, source_hash: &str) -> u64 {
    let row_bytes = row_id.to_le_bytes();
    let hash = B3Hash::hash_multi(&[
        dataset_id.as_bytes(),
        b"\0",
        &row_bytes,
        b"\0",
        source_hash.as_bytes(),
    ]);
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&hash.as_bytes()[0..8]);
    u64::from_le_bytes(buf)
}

fn hash_source_text(text: &str) -> String {
    B3Hash::hash(text.as_bytes()).to_hex()
}

fn hash_prompt_completion(prompt: &str, completion: &str) -> String {
    B3Hash::hash_multi(&[prompt.as_bytes(), b"\0", completion.as_bytes()]).to_hex()
}

pub fn resolve_pad_token_id(tokenizer: &Tokenizer) -> Result<u32> {
    // Standard pad tokens
    if let Some(id) = tokenizer.token_to_id("<|pad|>") {
        return Ok(id);
    }
    if let Some(id) = tokenizer.token_to_id("<pad>") {
        return Ok(id);
    }
    if let Some(id) = tokenizer.token_to_id("[PAD]") {
        return Ok(id);
    }
    // Qwen-style: uses endoftext as pad (per tokenizer_config.json)
    if let Some(id) = tokenizer.token_to_id("<|endoftext|>") {
        return Ok(id);
    }
    // Llama-style EOS tokens as fallback
    if let Some(id) = tokenizer.token_to_id("<|end_of_text|>") {
        return Ok(id);
    }
    if let Some(id) = tokenizer.token_to_id("</s>") {
        return Ok(id);
    }
    // Mistral/general EOS
    if let Some(id) = tokenizer.token_to_id("<eos>") {
        return Ok(id);
    }
    Err(AosError::Validation(
        "Tokenizer missing pad token id; configure a pad token".to_string(),
    ))
}

/// Wrapper for batch of training examples (JSONL format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingData {
    pub examples: Vec<TrainingExample>,
}

/// Strategy for generating training examples from documents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingStrategy {
    /// Identity mapping: input = target (memorization)
    Identity,
    /// Question-answer pairs (requires prompt engineering)
    QuestionAnswer,
    /// Model-based synthesis with determinism controls (routed to SynthesisEngine)
    Synthesis,
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
        TrainingStrategy::Synthesis => Err(AosError::Validation(
            "Synthesis strategy is handled by SynthesisEngine, not training_gen".to_string(),
        )),
    }
}

/// Generate identity mapping: input = target (for memorization)
fn generate_identity_example(
    chunk: &DocumentChunk,
    document: &IngestedDocument,
    tokenizer: &Arc<Tokenizer>,
    config: &TrainingGenConfig,
) -> Result<Vec<TrainingExample>> {
    let pad_token_id = resolve_pad_token_id(tokenizer)?;
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

    let mut provenance = BTreeMap::new();
    provenance.insert(
        "source".to_string(),
        serde_json::Value::String(document.source_name.clone()),
    );
    provenance.insert(
        "chunk_index".to_string(),
        serde_json::Value::String(chunk.chunk_index.to_string()),
    );
    provenance.insert(
        "source_type".to_string(),
        serde_json::Value::String(document.source.as_str().to_string()),
    );
    if let Some(page) = chunk.page_number {
        provenance.insert(
            "page".to_string(),
            serde_json::Value::String(page.to_string()),
        );
    }

    let source_hash = hash_source_text(&chunk.text);
    let created_at_unix_ms = deterministic_created_at_unix_ms(
        &document.source_name,
        chunk.chunk_index as u64,
        &source_hash,
    );
    let metadata = ExampleMetadataV1::new(
        document.source_name.clone(),
        chunk.chunk_index as u64,
        source_hash,
        provenance_from_map(&provenance)
            .map_err(|e| AosError::Validation(format!("Metadata error: {e}")))?,
        created_at_unix_ms,
    );
    let attention_mask = TrainingExample::attention_mask_from_tokens(&token_ids, pad_token_id);

    Ok(vec![TrainingExample::new(
        token_ids.clone(),
        token_ids,
        attention_mask,
        metadata,
    )])
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
    let pad_token_id = resolve_pad_token_id(tokenizer)?;
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

        let mut provenance_map = BTreeMap::new();
        provenance_map.insert(
            "source".to_string(),
            serde_json::Value::String(document.source_name.clone()),
        );
        provenance_map.insert(
            "chunk_index".to_string(),
            serde_json::Value::String(chunk.chunk_index.to_string()),
        );
        provenance_map.insert(
            "qa_index".to_string(),
            serde_json::Value::String(idx.to_string()),
        );
        provenance_map.insert(
            "strategy".to_string(),
            serde_json::Value::String("qa".to_string()),
        );
        provenance_map.insert(
            "qa_question_text".to_string(),
            serde_json::Value::String(question.clone()),
        );
        provenance_map.insert(
            "qa_answer_text".to_string(),
            serde_json::Value::String(sentence.to_string()),
        );

        let row_id = (chunk.chunk_index as u64) * 1000 + idx as u64;
        let source_hash = hash_prompt_completion(&question, sentence);
        let created_at_unix_ms =
            deterministic_created_at_unix_ms(&document.source_name, row_id, &source_hash);
        let metadata = ExampleMetadataV1::new(
            document.source_name.clone(),
            row_id,
            source_hash,
            provenance_from_map(&provenance_map)
                .map_err(|e| AosError::Validation(format!("Metadata error: {e}")))?,
            created_at_unix_ms,
        );
        let attention_mask = TrainingExample::attention_mask_from_tokens(&input_ids, pad_token_id);

        examples.push(TrainingExample::new(
            input_ids,
            target_ids,
            attention_mask,
            metadata,
        ));
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
            ocr_fingerprint: None,
            normalized_text_hash: None,
            normalized_text_len: None,
            byte_len: 100,
            page_count: Some(1),
            chunks: vec![chunk.clone()],
        };

        let config = TrainingGenConfig::default();
        let examples = generate_examples_from_chunk(&chunk, &document, &tokenizer, &config)
            .expect("Failed to generate examples");

        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].input_tokens, examples[0].target_tokens);
        assert!(!examples[0].input_tokens.is_empty());

        let metadata = &examples[0].metadata;
        assert_eq!(metadata.dataset_id, "test.pdf");
        let provenance: serde_json::Value =
            serde_json::from_str(&metadata.provenance).expect("provenance json");
        assert_eq!(
            provenance.get("source").and_then(|v| v.as_str()),
            Some("test.pdf")
        );
        assert_eq!(
            provenance.get("chunk_index").and_then(|v| v.as_str()),
            Some("0")
        );
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
            ocr_fingerprint: None,
            normalized_text_hash: None,
            normalized_text_len: None,
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
            assert!(!example.input_tokens.is_empty());
            assert!(!example.target_tokens.is_empty());
            let metadata = &example.metadata;
            let provenance: serde_json::Value =
                serde_json::from_str(&metadata.provenance).expect("provenance json");
            assert_eq!(
                provenance.get("strategy").and_then(|v| v.as_str()),
                Some("qa")
            );
            assert!(
                provenance
                    .get("qa_question_text")
                    .and_then(|q| q.as_str())
                    .map(|q| !q.is_empty())
                    .unwrap_or(false),
                "Question metadata should be populated"
            );
            assert!(
                provenance
                    .get("qa_answer_text")
                    .and_then(|a| a.as_str())
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
        tokenizer.with_pre_tokenizer(Some(Whitespace));
        Arc::new(tokenizer)
    }
}
