//! Integration test: README injection through existing ingestion pipeline
//!
//! This test verifies that the repo's README.md can be ingested and used
//! to train a LoRA adapter using the existing training pipeline.
//!
//! ## Requirements
//!
//! - Tokenizer file at canonical location (see AOS_TOKENIZER_PATH env var)
//! - README.md at repository root
//!
//! ## Running
//!
//! ```bash
//! # Run with tokenizer at default location
//! cargo test -p adapteros-orchestrator --test readme_ingestion_test -- --ignored --nocapture
//!
//! # Or with explicit tokenizer path
//! AOS_TOKENIZER_PATH=/var/models/Llama-3.2-3B-Instruct-4bit/tokenizer.json \
//!   cargo test -p adapteros-orchestrator --test readme_ingestion_test -- --ignored --nocapture
//! ```

use adapteros_config::{DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT};
use adapteros_ingest_docs::{
    generate_training_data_from_documents, load_tokenizer, ChunkingOptions, DocumentIngestor,
    TrainingGenConfig, TrainingStrategy,
};
use adapteros_lora_worker::training::{
    AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use std::path::PathBuf;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-").expect("create temp dir")
}

fn canonical_tokenizer_path() -> PathBuf {
    PathBuf::from(DEFAULT_MODEL_CACHE_ROOT)
        .join(DEFAULT_BASE_MODEL_ID)
        .join("tokenizer.json")
}

/// Find the repo root by walking up from CARGO_MANIFEST_DIR
fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // adapteros-orchestrator is in crates/adapteros-orchestrator
    manifest_dir
        .parent()
        .expect("crates dir")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

/// End-to-end test: README.md -> ingest -> train -> .aos artifact
///
/// Uses the exact same functions as the disabled train-docs command:
/// 1. DocumentIngestor::ingest_markdown_path() - ingests README
/// 2. generate_training_data_from_documents() - creates training examples
/// 3. MicroLoRATrainer::train() - trains LoRA adapter
/// 4. AdapterPackager::package_aos_with_metadata() - creates .aos artifact
#[tokio::test]
#[ignore = "Requires tokenizer and model files not available in CI [tracking: README-INJECT-001]"]
async fn test_readme_ingestion_end_to_end() {
    // === Step 1: Check tokenizer availability ===
    let tokenizer_path = std::env::var("AOS_TOKENIZER_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| canonical_tokenizer_path());

    if !tokenizer_path.exists() {
        tracing::warn!("Skipping test: tokenizer not found at {:?}", tokenizer_path);
        return;
    }

    // === Step 2: Locate README.md at repo root ===
    let readme_path = repo_root().join("README.md");
    assert!(
        readme_path.exists(),
        "README.md must exist at repo root: {}",
        readme_path.display()
    );

    tracing::info!("=== README Injection Test ===");
    tracing::info!("README path: {}", readme_path.display());
    tracing::info!("Tokenizer path: {}", tokenizer_path.display());

    // === Step 3: Ingest README using existing DocumentIngestor ===
    tracing::info!("Step 1/4: Ingesting README.md...");
    let tokenizer = load_tokenizer(&tokenizer_path).expect("load tokenizer");
    let chunking_options = ChunkingOptions {
        chunk_tokens: 512,
        overlap_tokens: 128,
        min_chunk_chars: 160,
    };
    let ingestor = DocumentIngestor::new(chunking_options, Some(tokenizer.clone()));
    let doc = ingestor
        .ingest_markdown_path(&readme_path)
        .expect("ingest README");

    // Verify ingestion succeeded
    assert!(
        !doc.chunks.is_empty(),
        "README should produce at least one chunk"
    );
    assert!(
        doc.source_name.contains("README"),
        "Source name should contain README"
    );
    assert!(doc.byte_len > 0, "README should have content");

    tracing::info!(
        "  Ingested: {} ({} bytes, {} chunks)",
        doc.source_name,
        doc.byte_len,
        doc.chunks.len()
    );

    // === Step 4: Generate training examples using existing function ===
    tracing::info!("Step 2/4: Generating training examples...");
    let gen_config = TrainingGenConfig {
        strategy: TrainingStrategy::Identity,
        max_seq_length: 512,
        add_special_tokens: true,
    };
    let training_data =
        generate_training_data_from_documents(std::slice::from_ref(&doc), &tokenizer, &gen_config)
            .expect("generate training data");

    // Verify examples were generated
    assert!(
        !training_data.examples.is_empty(),
        "Should generate at least one training example"
    );

    // Verify example structure
    for (i, example) in training_data.examples.iter().enumerate() {
        assert!(
            !example.input_tokens.is_empty(),
            "Example {} should have input tokens",
            i
        );
        assert!(
            !example.target_tokens.is_empty(),
            "Example {} should have target tokens",
            i
        );
        // Identity strategy: input == target
        assert_eq!(
            example.input_tokens, example.target_tokens,
            "Identity strategy should have matching input/target for example {}",
            i
        );
    }

    tracing::info!(
        "  Generated {} training examples from {} chunks",
        training_data.examples.len(),
        doc.chunks.len()
    );

    // === Step 5: Train using existing MicroLoRATrainer ===
    tracing::info!("Step 3/4: Training LoRA adapter...");
    let output_dir = new_test_tempdir();

    // Resolve base model path from tokenizer path
    let base_model_path = tokenizer_path.parent().map(|p| p.to_path_buf());

    // Use minimal config for fast test execution
    let train_config = TrainingConfig {
        rank: 4,
        alpha: 16.0,
        learning_rate: 0.001,
        batch_size: 1,
        epochs: 1,
        hidden_dim: 3584, // Qwen2.5-7B hidden dim
        vocab_size: tokenizer.get_vocab_size(true),
        pad_token_id: resolve_pad_token_id(&tokenizer),
        ignore_index: resolve_pad_token_id(&tokenizer) as i32,
        use_gpu_backward: false, // CPU training for CI compatibility
        validation_split: 0.0,   // Disable validation to avoid base_model requirement
        base_model_path,
        ..TrainingConfig::default()
    };

    let mut trainer = MicroLoRATrainer::new(train_config.clone()).expect("create trainer");

    // Convert examples to the format expected by trainer
    let examples: Vec<TrainingExample> = training_data.examples;

    let result = trainer.train(&examples).await.expect("train");

    // Verify training completed successfully
    assert!(result.final_loss >= 0.0, "Loss should be non-negative");
    assert!(result.final_loss.is_finite(), "Loss should be finite");
    assert!(
        result.training_time_us > 0,
        "Training time should be positive"
    );

    tracing::info!(
        "  Training complete: loss={:.6}, time={}ms",
        result.final_loss,
        result.training_time_us / 1000
    );

    // === Step 6: Package to .aos using existing packager ===
    tracing::info!("Step 4/4: Packaging adapter to .aos...");
    let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
    let packager = AdapterPackager::new(output_dir.path());

    // Mark as synthetic since training data was generated from docs (same as train_docs.rs)
    let mut pkg_metadata = std::collections::HashMap::new();
    pkg_metadata.insert("synthetic_mode".to_string(), "true".to_string());

    let packaged = packager
        .package_aos_with_metadata(
            "default",
            "readme_adapter",
            &quantized,
            &train_config,
            DEFAULT_BASE_MODEL_ID,
            pkg_metadata,
        )
        .await
        .expect("package adapter");

    // === Step 7: Verify artifact exists ===
    assert!(
        packaged.weights_path.exists(),
        ".aos artifact must exist at {}",
        packaged.weights_path.display()
    );

    let artifact_size = std::fs::metadata(&packaged.weights_path)
        .map(|m| m.len())
        .unwrap_or(0);
    assert!(artifact_size > 0, ".aos artifact should not be empty");

    tracing::info!(
        "  Packaged: {} ({} bytes)",
        packaged.weights_path.display(),
        artifact_size
    );

    // === Summary ===
    tracing::info!("=== README Injection Test PASSED ===");
    tracing::info!("  README bytes ingested: {}", doc.byte_len);
    tracing::info!("  Chunks created: {}", doc.chunks.len());
    tracing::info!("  Training examples: {}", examples.len());
    tracing::info!("  Final loss: {:.6}", result.final_loss);
    tracing::info!("  Training time: {}ms", result.training_time_us / 1000);
    tracing::info!("  Artifact size: {} bytes", artifact_size);
    tracing::info!("  Artifact path: {}", packaged.weights_path.display());
}

/// Resolve pad token ID from tokenizer (same logic as train_docs.rs)
fn resolve_pad_token_id(tokenizer: &tokenizers::Tokenizer) -> u32 {
    // Standard pad tokens
    if let Some(id) = tokenizer.token_to_id("<|pad|>") {
        return id;
    }
    if let Some(id) = tokenizer.token_to_id("<pad>") {
        return id;
    }
    if let Some(id) = tokenizer.token_to_id("[PAD]") {
        return id;
    }
    // Qwen-style: uses endoftext as pad
    if let Some(id) = tokenizer.token_to_id("<|endoftext|>") {
        return id;
    }
    // Llama-style EOS tokens
    if let Some(id) = tokenizer.token_to_id("<|end_of_text|>") {
        return id;
    }
    if let Some(id) = tokenizer.token_to_id("</s>") {
        return id;
    }
    // Mistral/general EOS
    if let Some(id) = tokenizer.token_to_id("<eos>") {
        return id;
    }
    // Ultimate fallback
    0
}

/// Test that README exists and can be located
#[test]
fn test_readme_exists_at_repo_root() {
    let readme_path = repo_root().join("README.md");
    assert!(
        readme_path.exists(),
        "README.md should exist at repo root: {}",
        readme_path.display()
    );
}

/// Test ingestion-only (no training) - faster, no tokenizer required for basic check
#[test]
fn test_readme_can_be_chunked_without_tokenizer() {
    let readme_path = repo_root().join("README.md");
    if !readme_path.exists() {
        return;
    }

    // Create ingestor without tokenizer (character-based chunking)
    let ingestor = DocumentIngestor::new(ChunkingOptions::default(), None);
    let doc = ingestor
        .ingest_markdown_path(&readme_path)
        .expect("ingest README without tokenizer");

    assert!(!doc.chunks.is_empty(), "Should produce chunks");
    assert!(doc.byte_len > 0, "Should have content");

    // Verify chunk structure
    for chunk in &doc.chunks {
        assert!(!chunk.text.is_empty(), "Chunk text should not be empty");
        assert!(chunk.chunk_index < doc.chunks.len(), "Chunk index valid");
    }
}
