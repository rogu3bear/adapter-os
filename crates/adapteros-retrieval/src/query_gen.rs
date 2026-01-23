//! Query generation for benchmark evaluation
//!
//! This module provides automated generation of evaluation queries from document chunks.
//! Generated queries can be used to benchmark retrieval quality without manual annotation.
//!
//! # Strategies
//!
//! - `HeaderQuestions`: Extracts markdown headers and converts them to questions
//! - `DefinitionQueries`: Finds "X is..." patterns and generates "What is X?" queries
//! - `FirstSentence`: Uses the first sentence of each chunk as the query
//!
//! # Example
//!
//! ```ignore
//! use adapteros_retrieval::query_gen::{generate_queries, QueryStrategy};
//!
//! let queries = generate_queries(&chunks, QueryStrategy::HeaderQuestions, 100)?;
//! ```

use crate::corpus::Chunk;
use crate::eval::{EvalQuery, QuerySource};
use adapteros_core::B3Hash;

/// Strategy for generating queries from chunks
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryStrategy {
    /// Extract questions from markdown headers
    /// Converts headers like "## Authentication" to "What is authentication?"
    HeaderQuestions,
    /// Generate "what is X" queries from definitions
    /// Finds patterns like "X is a..." and generates "What is X?"
    DefinitionQueries,
    /// Use first sentence as query
    /// Takes the first sentence from each chunk as a search query
    FirstSentence,
}

/// Generate evaluation queries from corpus chunks
///
/// # Arguments
/// * `chunks` - Corpus chunks to generate queries from
/// * `strategy` - Query generation strategy to use
/// * `max_queries` - Maximum number of queries to generate
///
/// # Returns
/// Vector of evaluation queries with deterministic IDs and ground truth relevance
pub fn generate_queries(
    chunks: &[Chunk],
    strategy: QueryStrategy,
    max_queries: usize,
) -> Vec<EvalQuery> {
    let mut queries = Vec::new();

    for chunk in chunks {
        if queries.len() >= max_queries {
            break;
        }

        if let Some(query) = extract_query(chunk, &strategy) {
            // Skip very short or empty queries
            if query.len() < 3 {
                continue;
            }

            queries.push(EvalQuery {
                query_id: generate_query_id(&query),
                query_text: query,
                relevant_chunk_ids: vec![chunk.chunk_id.clone()],
                hard_negatives: None,
                source: QuerySource::Generated {
                    from_doc: chunk.source_path.clone(),
                },
            });
        }
    }

    queries
}

/// Extract a query from a chunk using the specified strategy
fn extract_query(chunk: &Chunk, strategy: &QueryStrategy) -> Option<String> {
    match strategy {
        QueryStrategy::HeaderQuestions => extract_header_question(&chunk.content),
        QueryStrategy::DefinitionQueries => extract_definition_query(&chunk.content),
        QueryStrategy::FirstSentence => extract_first_sentence(&chunk.content),
    }
}

/// Generate a deterministic query ID from query text using BLAKE3
///
/// Returns the first 16 hex characters of the hash for a compact but unique ID.
fn generate_query_id(query: &str) -> String {
    B3Hash::hash(query.as_bytes()).to_hex()[..16].to_string()
}

/// Extract a question from a markdown header
///
/// Converts headers like:
/// - "## Authentication" -> "What is authentication?"
/// - "# Getting Started" -> "What is getting started?"
/// - "### API Reference" -> "What is API reference?"
fn extract_header_question(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();

        // Match markdown headers (# to ######)
        if let Some(header_text) = trimmed.strip_prefix('#') {
            // Strip additional # characters and whitespace
            let header_text = header_text.trim_start_matches('#').trim();

            // Skip empty headers
            if header_text.is_empty() {
                continue;
            }

            // Convert to question, lowercasing the header text
            let question = format!("What is {}?", header_text.to_lowercase());
            return Some(question);
        }
    }
    None
}

/// Extract a query from a definition pattern
///
/// Finds patterns like:
/// - "LoRA is a low-rank adaptation technique" -> "What is LoRA?"
/// - "The router is responsible for..." -> "What is the router?"
/// - "Adapters are small neural networks" -> "What are adapters?"
fn extract_definition_query(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and headers
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Look for "X is " or "X are " patterns
        if let Some(pos) = trimmed.find(" is ") {
            let subject = &trimmed[..pos];
            // Validate subject: should be reasonably short and not start with punctuation
            if is_valid_subject(subject) {
                return Some(format!("What is {}?", subject));
            }
        }

        if let Some(pos) = trimmed.find(" are ") {
            let subject = &trimmed[..pos];
            if is_valid_subject(subject) {
                return Some(format!("What are {}?", subject));
            }
        }
    }
    None
}

/// Check if a subject is valid for generating a definition query
fn is_valid_subject(subject: &str) -> bool {
    let subject = subject.trim();

    // Must have reasonable length
    if subject.is_empty() || subject.len() > 50 {
        return false;
    }

    // Should not be or start with common pronouns that indicate mid-sentence
    let lower = subject.to_lowercase();

    // Check for single-word pronouns
    if matches!(lower.as_str(), "it" | "this" | "that" | "which" | "there") {
        return false;
    }

    // Check for phrases starting with pronouns
    if lower.starts_with("it ")
        || lower.starts_with("this ")
        || lower.starts_with("that ")
        || lower.starts_with("which ")
        || lower.starts_with("there ")
    {
        return false;
    }

    // First character should be alphanumeric (not punctuation)
    subject
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric())
}

/// Extract the first sentence from content
///
/// Returns the first non-empty, non-header line trimmed to sentence boundaries.
fn extract_first_sentence(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and headers
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Find sentence boundary (., !, ?)
        let sentence = if let Some(pos) = trimmed.find(|c| c == '.' || c == '!' || c == '?') {
            // Include the punctuation
            trimmed[..=pos].to_string()
        } else {
            // No sentence boundary, use the whole line
            trimmed.to_string()
        };

        // Only return if we have meaningful content
        if sentence.split_whitespace().count() >= 2 {
            return Some(sentence);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::ChunkType;

    fn make_chunk(content: &str, source_path: &str) -> Chunk {
        Chunk::new(
            source_path.to_string(),
            content.to_string(),
            0,
            content.len(),
            ChunkType::Document {
                format: "markdown".to_string(),
            },
        )
    }

    #[test]
    fn test_generate_queries_header() {
        let chunks = vec![
            make_chunk("## Authentication\nThis section covers auth.", "auth.md"),
            make_chunk("# Getting Started\nWelcome to the guide.", "intro.md"),
            make_chunk("### API Reference\nEndpoints listed here.", "api.md"),
        ];

        let queries = generate_queries(&chunks, QueryStrategy::HeaderQuestions, 10);

        assert_eq!(queries.len(), 3);
        assert_eq!(queries[0].query_text, "What is authentication?");
        assert_eq!(queries[1].query_text, "What is getting started?");
        assert_eq!(queries[2].query_text, "What is api reference?");

        // Verify relevance labels
        assert!(queries[0]
            .relevant_chunk_ids
            .contains(&chunks[0].chunk_id));
        assert!(queries[1]
            .relevant_chunk_ids
            .contains(&chunks[1].chunk_id));
    }

    #[test]
    fn test_generate_queries_definition() {
        let chunks = vec![
            make_chunk("LoRA is a low-rank adaptation technique.", "lora.md"),
            make_chunk("Adapters are small neural networks.", "adapters.md"),
            make_chunk(
                "No definition here, just regular text about things.",
                "other.md",
            ),
        ];

        let queries = generate_queries(&chunks, QueryStrategy::DefinitionQueries, 10);

        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0].query_text, "What is LoRA?");
        assert_eq!(queries[1].query_text, "What are Adapters?");
    }

    #[test]
    fn test_generate_queries_first_sentence() {
        let chunks = vec![
            make_chunk(
                "This is the first sentence. This is the second.",
                "doc1.md",
            ),
            make_chunk("Welcome to the guide!", "doc2.md"),
            make_chunk("Short.", "doc3.md"), // Too short, should be skipped
        ];

        let queries = generate_queries(&chunks, QueryStrategy::FirstSentence, 10);

        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0].query_text, "This is the first sentence.");
        assert_eq!(queries[1].query_text, "Welcome to the guide!");
    }

    #[test]
    fn test_query_id_deterministic() {
        let query1 = "What is authentication?";
        let query2 = "What is authentication?";
        let query3 = "What is authorization?";

        let id1 = generate_query_id(query1);
        let id2 = generate_query_id(query2);
        let id3 = generate_query_id(query3);

        // Same query should produce same ID
        assert_eq!(id1, id2);

        // Different queries should produce different IDs
        assert_ne!(id1, id3);

        // ID should be 16 hex characters
        assert_eq!(id1.len(), 16);
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_max_queries_limit() {
        let chunks: Vec<Chunk> = (0..10)
            .map(|i| make_chunk(&format!("## Header {}", i), &format!("doc{}.md", i)))
            .collect();

        let queries = generate_queries(&chunks, QueryStrategy::HeaderQuestions, 3);

        assert_eq!(queries.len(), 3);
    }

    #[test]
    fn test_query_source_generated() {
        let chunks = vec![make_chunk("## Test Header", "test.md")];
        let queries = generate_queries(&chunks, QueryStrategy::HeaderQuestions, 10);

        assert_eq!(queries.len(), 1);
        match &queries[0].source {
            QuerySource::Generated { from_doc } => {
                assert_eq!(from_doc, "test.md");
            }
            _ => panic!("Expected Generated source"),
        }
    }

    #[test]
    fn test_empty_chunks() {
        let chunks: Vec<Chunk> = vec![];
        let queries = generate_queries(&chunks, QueryStrategy::HeaderQuestions, 10);
        assert!(queries.is_empty());
    }

    #[test]
    fn test_header_extraction_variants() {
        // Test different header levels
        assert_eq!(
            extract_header_question("# Title"),
            Some("What is title?".to_string())
        );
        assert_eq!(
            extract_header_question("## Subtitle"),
            Some("What is subtitle?".to_string())
        );
        assert_eq!(
            extract_header_question("### Deep Header"),
            Some("What is deep header?".to_string())
        );
        assert_eq!(
            extract_header_question("###### Very Deep"),
            Some("What is very deep?".to_string())
        );

        // Empty header should be skipped
        assert_eq!(extract_header_question("# "), None);
        assert_eq!(extract_header_question("##"), None);

        // No header at all
        assert_eq!(extract_header_question("Just regular text"), None);
    }

    #[test]
    fn test_definition_extraction_edge_cases() {
        // Skip if subject starts with pronouns
        assert_eq!(extract_definition_query("It is a test."), None);
        assert_eq!(extract_definition_query("This is important."), None);
        assert_eq!(extract_definition_query("There is a problem."), None);

        // Valid definitions
        assert_eq!(
            extract_definition_query("A cache is a storage mechanism."),
            Some("What is A cache?".to_string())
        );

        // Subject too long (over 50 chars) should be rejected
        let long_subject =
            "This is a very long subject that definitely exceeds fifty characters in length";
        assert_eq!(
            extract_definition_query(&format!("{} is something.", long_subject)),
            None
        );
    }

    #[test]
    fn test_first_sentence_extraction_edge_cases() {
        // Multiple sentences
        assert_eq!(
            extract_first_sentence("First sentence. Second sentence."),
            Some("First sentence.".to_string())
        );

        // Question mark
        assert_eq!(
            extract_first_sentence("Is this a question? Yes it is."),
            Some("Is this a question?".to_string())
        );

        // Exclamation mark with multiple words
        assert_eq!(
            extract_first_sentence("Welcome everyone! Enjoy your stay."),
            Some("Welcome everyone!".to_string())
        );

        // Single-word exclamation should be skipped (less than 2 words)
        assert_eq!(extract_first_sentence("Welcome!"), None);

        // Skip headers
        assert_eq!(
            extract_first_sentence("# Header\nActual content here."),
            Some("Actual content here.".to_string())
        );

        // Very short content (less than 2 words) should be skipped
        assert_eq!(extract_first_sentence("OK."), None);
    }

    #[test]
    fn test_hard_negatives_none() {
        let chunks = vec![make_chunk("## Test", "test.md")];
        let queries = generate_queries(&chunks, QueryStrategy::HeaderQuestions, 10);

        assert!(queries[0].hard_negatives.is_none());
    }

    #[test]
    fn test_chunks_without_extractable_content() {
        // Chunks that don't have headers for HeaderQuestions strategy
        let chunks = vec![
            make_chunk("Just regular text without headers.", "doc1.md"),
            make_chunk("More text here.", "doc2.md"),
        ];

        let queries = generate_queries(&chunks, QueryStrategy::HeaderQuestions, 10);
        assert!(queries.is_empty());
    }

    #[test]
    fn test_skips_very_short_queries() {
        // Create chunks that would generate very short queries
        let chunks = vec![
            make_chunk("# A", "doc1.md"), // Would generate "What is a?" - too short
            make_chunk("# OK", "doc2.md"), // Would generate "What is ok?" - borderline
            make_chunk("# Authentication", "doc3.md"), // Valid
        ];

        let queries = generate_queries(&chunks, QueryStrategy::HeaderQuestions, 10);

        // Should have at least the valid one
        assert!(!queries.is_empty());
        // The query text should be the valid one
        assert!(queries.iter().any(|q| q.query_text == "What is authentication?"));
    }
}
