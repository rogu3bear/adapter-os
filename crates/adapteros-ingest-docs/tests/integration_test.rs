//! Integration tests for document ingestion pipeline

use adapteros_ingest_docs::{
    default_ingest_options, generate_training_data, pdf_render, DocumentIngestor, DocumentSource,
    EmbeddingModel, ExtractedImage, SimpleEmbeddingModel, TrainingGenConfig, TrainingStrategy,
};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokenizers::models::wordlevel::WordLevel;
use tokenizers::pre_tokenizers::whitespace::Whitespace;
use tokenizers::Tokenizer;

fn new_temp_file() -> NamedTempFile {
    NamedTempFile::with_prefix("aos-test-").expect("Failed to create temp file")
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
    let mut temp_file = new_temp_file();
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
    assert!(!document.doc_hash.to_hex().is_empty());
}

#[test]
fn test_text_ingestion_deterministic() {
    let text_content = "Line one.  \r\n\r\n   Line two.\n\nLine three.";
    let options = default_ingest_options();
    let ingestor = DocumentIngestor::new(options, None);

    let doc1 = ingestor
        .ingest_text_bytes(text_content.as_bytes(), "notes.txt")
        .expect("Failed to ingest text");
    let doc2 = ingestor
        .ingest_text_bytes(text_content.as_bytes(), "notes.txt")
        .expect("Failed to ingest text");

    assert_eq!(doc1.source, DocumentSource::Text);
    assert_eq!(doc1.normalized_text_hash, doc2.normalized_text_hash);
    assert_eq!(doc1.normalized_text_len, doc2.normalized_text_len);
    assert_eq!(doc1.chunks.len(), doc2.chunks.len());
    assert_eq!(doc1.chunks[0].text, doc2.chunks[0].text);
}

#[test]
fn test_pdf_no_text_layer_rejected() {
    let options = default_ingest_options();
    let ingestor = DocumentIngestor::new(options, None);

    // Minimal PDF with a single empty page and no text content.
    let empty_pdf: &[u8] = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>\nendobj\n4 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\nxref\n0 5\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \n0000000115 00000 n \n0000000202 00000 n \ntrailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n251\n%%EOF\n";

    let err = ingestor
        .ingest_pdf_bytes(empty_pdf, "empty-text.pdf")
        .expect_err("Expected PDF with no text layer to fail");
    let msg = err.to_string();
    assert!(msg.contains("contains no text layer"));
}

#[test]
fn test_pdf_toxic_empty_rejected() {
    let options = default_ingest_options();
    let ingestor = DocumentIngestor::new(options, None);
    let path = PathBuf::from("tests/data/toxic_docs/empty.pdf");

    let result = ingestor.ingest_pdf_path(&path);
    assert!(result.is_err(), "Empty PDF should be rejected");
}

#[test]
fn test_pdf_toxic_recursion_rejected() {
    let options = default_ingest_options();
    let ingestor = DocumentIngestor::new(options, None);
    let path = PathBuf::from("tests/data/toxic_docs/recursive.pdf");

    let result = ingestor.ingest_pdf_path(&path);
    assert!(
        result.is_err(),
        "Recursive or oversized PDF should be rejected"
    );
}

#[test]
fn test_training_data_generation() {
    // Create a simple test document
    let markdown_content = "This is a test document. It contains multiple sentences. Each sentence provides some information.";

    let mut temp_file = new_temp_file();
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
    assert!(!example.input_tokens.is_empty());
    assert!(!example.target_tokens.is_empty());
    assert_eq!(
        example.input_tokens, example.target_tokens,
        "For identity strategy, input should equal target"
    );
}

#[test]
fn test_generate_training_data_question_answer_strategy_produces_qa_pairs() {
    let markdown_content = "AdapterOS supports deterministic routing for CoreML reasoning workloads. The system applies policy hooks before inference.";

    let mut temp_file = new_temp_file();
    temp_file
        .write_all(markdown_content.as_bytes())
        .expect("Failed to write markdown");

    let options = default_ingest_options();
    let ingestor = DocumentIngestor::new(options, None);
    let document = ingestor
        .ingest_markdown_path(temp_file.path())
        .expect("Failed to ingest markdown");

    let tokenizer = fixture_tokenizer();
    let config = TrainingGenConfig {
        strategy: TrainingStrategy::QuestionAnswer,
        max_seq_length: 512,
        add_special_tokens: true,
    };

    let training_data = generate_training_data(&document, &tokenizer, &config)
        .expect("Failed to generate training data");

    assert!(
        !training_data.examples.is_empty(),
        "Should have at least one Q&A example"
    );

    let example = &training_data.examples[0];
    assert!(!example.input_tokens.is_empty());
    assert!(!example.target_tokens.is_empty());
    assert_ne!(
        example.input_tokens, example.target_tokens,
        "Q&A strategy should produce distinct input/target"
    );

    let provenance: serde_json::Value =
        serde_json::from_str(&example.metadata.provenance).expect("provenance json");
    assert!(
        provenance.get("qa_question_text").is_some(),
        "expected qa_question_text in provenance"
    );
    assert!(
        provenance.get("qa_answer_text").is_some(),
        "expected qa_answer_text in provenance"
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

    let mut temp_file = new_temp_file();
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

// ============================================================================
// Visual Content Extraction Tests
// ============================================================================

#[test]
fn test_vision_prompt_generation_single_image() {
    let prompt = pdf_render::generate_vision_prompt(1, None);
    assert!(prompt.contains("Describe this image"));
    assert!(prompt.contains("chart") || prompt.contains("graph") || prompt.contains("diagram"));
}

#[test]
fn test_vision_prompt_generation_multiple_images() {
    let prompt = pdf_render::generate_vision_prompt(3, None);
    assert!(prompt.contains("Describe these images"));
}

#[test]
fn test_vision_prompt_with_context() {
    let prompt = pdf_render::generate_vision_prompt(1, Some("Financial report Q4 2024"));
    assert!(prompt.contains("Context: Financial report Q4 2024"));
}

#[test]
fn test_images_to_base64_empty() {
    let images: Vec<ExtractedImage> = vec![];
    let base64_images = pdf_render::images_to_base64(&images);
    assert!(base64_images.is_empty());
}

#[test]
fn test_images_to_base64_encoding() {
    // Create a minimal test image (1x1 pixel PNG)
    let test_png_bytes = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77,
        0x53, 0xDE, // IHDR data
    ];

    let images = vec![ExtractedImage {
        page_number: 1,
        image_name: "test_image".to_string(),
        image_bytes: test_png_bytes.clone(),
        width: 1,
        height: 1,
    }];

    let base64_images = pdf_render::images_to_base64(&images);
    assert_eq!(base64_images.len(), 1);

    // Verify base64 encoding
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&base64_images[0])
        .expect("Should be valid base64");
    assert_eq!(decoded, test_png_bytes);
}

#[test]
fn test_extracted_image_type() {
    let image = ExtractedImage {
        page_number: 5,
        image_name: "chart_001".to_string(),
        image_bytes: vec![1, 2, 3, 4],
        width: 800,
        height: 600,
    };

    assert_eq!(image.page_number, 5);
    assert_eq!(image.image_name, "chart_001");
    assert_eq!(image.image_bytes.len(), 4);
    assert_eq!(image.width, 800);
    assert_eq!(image.height, 600);
}
