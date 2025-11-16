//! Integration tests for document ingestion pipeline

use adapteros_ingest_docs::{
    default_ingest_options, generate_training_data, load_tokenizer, DocumentIngestor,
    DocumentSource, EmbeddingModel, SimpleEmbeddingModel, TrainingGenConfig, TrainingStrategy,
};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokenizers::Tokenizer;

fn fixture_tokenizer() -> Arc<Tokenizer> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to locate workspace root");
    let tokenizer_path = repo_root.join("models/test-model/tokenizer.json");
    load_tokenizer(&tokenizer_path).expect("Failed to load tokenizer")
}

#[test]
fn test_markdown_ingestion() {
    let markdown_content = r#"
# Test Document

This is a test document for ingestion.

## Section 1

This section contains some important information about the product.

## Section 2

This section provides additional details and specifications.
"#;

    // Write to temp file
    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    temp_file
        .write_all(markdown_content.as_bytes())
        .expect("Failed to write markdown");

    // Create ingestor
    let options = default_ingest_options();
    let ingestor = DocumentIngestor::new(options, None);

    // Ingest markdown
    let document = ingestor
        .ingest_markdown_path(temp_file.path())
        .expect("Failed to ingest markdown");

    assert_eq!(document.source, DocumentSource::Markdown);
    assert!(
        !document.chunks.is_empty(),
        "Should have at least one chunk"
    );
    assert!(document.doc_hash.to_hex().len() > 0);
}

#[test]
fn test_training_data_generation() {
    // Create a simple test document
    let markdown_content = "This is a test document. It contains multiple sentences. Each sentence provides some information.";

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    temp_file
        .write_all(markdown_content.as_bytes())
        .expect("Failed to write markdown");

    let options = default_ingest_options();
    let ingestor = DocumentIngestor::new(options, None);

    let document = ingestor
        .ingest_markdown_path(temp_file.path())
        .expect("Failed to ingest markdown");

    // Load tokenizer from fixtures
    let tokenizer = fixture_tokenizer();

    // Generate training data with identity strategy
    let config = TrainingGenConfig {
        strategy: TrainingStrategy::Identity,
        max_seq_length: 512,
        add_special_tokens: true,
    };

    let training_data = generate_training_data(&document, &tokenizer, &config)
        .expect("Failed to generate training data");

    assert!(
        !training_data.examples.is_empty(),
        "Should have at least one training example"
    );

    // Verify the example structure
    let example = &training_data.examples[0];
    assert!(!example.input.is_empty());
    assert!(!example.target.is_empty());
    assert_eq!(
        example.input, example.target,
        "For identity strategy, input should equal target"
    );
}

#[test]
fn test_embedding_generation() {
    let tokenizer = fixture_tokenizer();

    let embedding_model = SimpleEmbeddingModel::new(tokenizer);

    let text = "This is a test sentence for embedding generation.";

    let embedding = embedding_model
        .encode_text(text)
        .expect("Failed to generate embedding");

    assert_eq!(embedding.len(), adapteros_ingest_docs::EMBEDDING_DIMENSION);

    // Check normalization (should be approximately 1.0)
    let magnitude: f32 = embedding.iter().map(|&x| x * x).sum::<f32>().sqrt();
    assert!(
        (magnitude - 1.0).abs() < 1e-4,
        "Embedding should be normalized, got magnitude {}",
        magnitude
    );

    // Test determinism
    let embedding2 = embedding_model
        .encode_text(text)
        .expect("Failed to generate embedding");

    assert_eq!(embedding, embedding2, "Embeddings should be deterministic");
}

#[tokio::test]
async fn test_rag_preparation() {
    let markdown_content = "# RAG Test\n\nThis document will be indexed for RAG retrieval.";

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    temp_file
        .write_all(markdown_content.as_bytes())
        .expect("Failed to write markdown");

    let options = default_ingest_options();
    let ingestor = DocumentIngestor::new(options, None);

    let document = ingestor
        .ingest_markdown_path(temp_file.path())
        .expect("Failed to ingest markdown");

    // Create embedding model
    let tokenizer = fixture_tokenizer();
    let embedding_model = Arc::new(SimpleEmbeddingModel::new(tokenizer))
        as Arc<dyn adapteros_ingest_docs::EmbeddingModel>;

    // Prepare for RAG
    let rag_params = adapteros_ingest_docs::prepare_document_for_rag(
        "test-tenant",
        &document,
        &embedding_model,
        Some("v1"),
    )
    .await
    .expect("Failed to prepare document for RAG");

    assert!(!rag_params.is_empty(), "Should have RAG parameters");

    for params in &rag_params {
        assert_eq!(params.tenant_id, "test-tenant");
        assert!(params.doc_id.contains("chunk"));
        assert!(!params.text.is_empty());
        assert_eq!(
            params.embedding.len(),
            adapteros_ingest_docs::EMBEDDING_DIMENSION
        );
        assert_eq!(params.rev, "v1");
        assert_eq!(params.source_type, "markdown");
    }
}
