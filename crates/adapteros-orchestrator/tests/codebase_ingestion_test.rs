//! Integration tests for codebase ingestion pipeline
//!
//! These tests require external dependencies (tokenizers, models) and are marked as ignored.
//! Run with: cargo test --test codebase_ingestion_test -- --ignored

use adapteros_config::{DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT};
use adapteros_lora_worker::training::TrainingConfig;
use adapteros_orchestrator::codebase_ingestion::{CodebaseIngestion, IngestionConfig};
use adapteros_platform::common::PlatformUtils;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = PlatformUtils::temp_dir();
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

fn canonical_tokenizer_path() -> PathBuf {
    PathBuf::from(DEFAULT_MODEL_CACHE_ROOT)
        .join(DEFAULT_BASE_MODEL_ID)
        .join("tokenizer.json")
}

/// Test end-to-end codebase ingestion pipeline
/// TODO: Requires tokenizer and model files, skipped in CI
#[tokio::test]
#[ignore = "Requires tokenizer and model files not available in CI [tracking: STAB-IGN-001]"]
async fn test_codebase_ingestion_end_to_end() {
    // Skip if tokenizer is not available
    let tokenizer_path = canonical_tokenizer_path();
    if !tokenizer_path.exists() {
        eprintln!("Skipping test: tokenizer not found at {:?}", tokenizer_path);
        return;
    }

    // Create a temporary repository with documented code
    let temp_repo = new_test_tempdir();
    let repo_path = temp_repo.path();

    // Create source directory
    let src_dir = repo_path.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    // Write a documented Rust library
    let lib_rs = src_dir.join("lib.rs");
    let mut file = std::fs::File::create(&lib_rs).unwrap();
    writeln!(
        file,
        r#"
/// Add two numbers together
///
/// This function takes two integers and returns their sum.
pub fn add(a: i32, b: i32) -> i32 {{
    a + b
}}

/// Multiply two numbers
///
/// Returns the product of two integers.
pub fn multiply(x: i32, y: i32) -> i32 {{
    x * y
}}
"#
    )
    .unwrap();

    // Write README
    let readme = repo_path.join("README.md");
    let mut file = std::fs::File::create(&readme).unwrap();
    writeln!(
        file,
        r#"# Test Project

This is a test project for codebase ingestion testing.

It contains basic math utilities.
"#
    )
    .unwrap();

    // Create output directory for adapter
    let output_dir = new_test_tempdir();

    // Configure ingestion
    let config = IngestionConfig {
        training_config: TrainingConfig {
            rank: 4,
            alpha: 16.0,
            learning_rate: 0.0001,
            batch_size: 1,
            epochs: 1, // Just 1 epoch for testing
            hidden_dim: 768,
            ..Default::default()
        },
        tokenizer_path: Some(tokenizer_path),
        max_pairs_per_symbol: 2,
        include_private: false,
        min_doc_length: 15,
        generate_negative_examples: true,
        base_model: "qwen2.5-7b".to_string(),
    };

    // Run ingestion pipeline
    let ingestion = CodebaseIngestion::new(config).expect("Failed to create ingestion pipeline");

    let result = ingestion
        .ingest_and_train(repo_path, "test_adapter", output_dir.path())
        .await
        .expect("Ingestion pipeline failed");

    // Verify results
    assert_eq!(result.adapter_id, "test_adapter");
    assert!(
        !result.adapter_hash.is_empty(),
        "Adapter hash should not be empty"
    );
    assert!(
        !result.content_hash.is_empty(),
        "Content hash should not be empty"
    );
    assert!(result.symbols_count > 0, "Should have extracted symbols");
    assert!(result.examples_count > 0, "Should have generated examples");

    // Verify adapter was packaged
    let adapter_path = output_dir.path().join("test_adapter");
    assert!(adapter_path.exists(), "Adapter directory should exist");
    assert!(
        adapter_path.join("manifest.json").exists(),
        "Manifest should exist"
    );

    println!("Codebase ingestion test passed");
    println!("  Symbols extracted: {}", result.symbols_count);
    println!("  Training examples: {}", result.examples_count);
    println!("  Final loss: {:.6}", result.final_loss);
    println!("  Adapter hash: {}", result.adapter_hash);
    println!("  Content hash: {}", result.content_hash);
}

/// Test determinism: same codebase should produce same hash
/// TODO: Requires tokenizer and model files, skipped in CI
#[tokio::test]
#[ignore = "Requires tokenizer and model files not available in CI [tracking: STAB-IGN-001]"]
async fn test_determinism() {
    // Skip if tokenizer is not available
    let tokenizer_path = canonical_tokenizer_path();
    if !tokenizer_path.exists() {
        eprintln!("Skipping test: tokenizer not found at {:?}", tokenizer_path);
        return;
    }

    // Create identical repositories
    let create_test_repo = || -> TempDir {
        let temp_repo = new_test_tempdir();
        let src_dir = temp_repo.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let lib_rs = src_dir.join("lib.rs");
        let mut file = std::fs::File::create(&lib_rs).unwrap();
        writeln!(
            file,
            r#"
/// Square a number
pub fn square(n: i32) -> i32 {{
    n * n
}}
"#
        )
        .unwrap();

        temp_repo
    };

    let repo1 = create_test_repo();
    let repo2 = create_test_repo();
    let output1 = new_test_tempdir();
    let output2 = new_test_tempdir();

    let config = IngestionConfig {
        training_config: TrainingConfig {
            rank: 4,
            alpha: 16.0,
            learning_rate: 0.0001,
            batch_size: 1,
            epochs: 1,
            hidden_dim: 768,
            ..Default::default()
        },
        tokenizer_path: Some(tokenizer_path),
        max_pairs_per_symbol: 2,
        include_private: false,
        min_doc_length: 10,
        generate_negative_examples: false,
        base_model: "qwen2.5-7b".to_string(),
    };

    // Run ingestion on both repositories
    let ingestion = CodebaseIngestion::new(config.clone()).unwrap();

    let result1 = ingestion
        .ingest_and_train(repo1.path(), "adapter1", output1.path())
        .await
        .expect("First ingestion failed");

    let ingestion = CodebaseIngestion::new(config).unwrap();

    let result2 = ingestion
        .ingest_and_train(repo2.path(), "adapter2", output2.path())
        .await
        .expect("Second ingestion failed");

    // Verify determinism: same content hash
    assert_eq!(
        result1.content_hash, result2.content_hash,
        "Content hashes should match for identical codebases"
    );

    // Verify same adapter hash (deterministic training)
    assert_eq!(
        result1.adapter_hash, result2.adapter_hash,
        "Adapter hashes should match for identical codebases (deterministic training)"
    );

    println!("Determinism test passed");
    println!("  Content hash: {}", result1.content_hash);
    println!("  Adapter hash: {}", result1.adapter_hash);
}

/// Test that pipeline handles repositories with no documentation
/// TODO: Requires tokenizer and model files, skipped in CI
#[tokio::test]
#[ignore = "Requires tokenizer and model files not available in CI [tracking: STAB-IGN-001]"]
async fn test_no_documentation() {
    let tokenizer_path = canonical_tokenizer_path();
    if !tokenizer_path.exists() {
        eprintln!("Skipping test: tokenizer not found");
        return;
    }

    let temp_repo = new_test_tempdir();
    let src_dir = temp_repo.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    // Write code with no documentation
    let lib_rs = src_dir.join("lib.rs");
    let mut file = std::fs::File::create(&lib_rs).unwrap();
    writeln!(
        file,
        r#"
pub fn undocumented_func(x: i32) -> i32 {{
    x + 1
}}
"#
    )
    .unwrap();

    let output_dir = new_test_tempdir();

    let config = IngestionConfig {
        training_config: TrainingConfig {
            rank: 4,
            alpha: 16.0,
            learning_rate: 0.0001,
            batch_size: 1,
            epochs: 1,
            hidden_dim: 768,
            ..Default::default()
        },
        tokenizer_path: Some(tokenizer_path),
        max_pairs_per_symbol: 2,
        include_private: false,
        min_doc_length: 10,
        generate_negative_examples: true,
        base_model: "qwen2.5-7b".to_string(),
    };

    let ingestion = CodebaseIngestion::new(config).unwrap();

    let result = ingestion
        .ingest_and_train(temp_repo.path(), "test_adapter", output_dir.path())
        .await;

    // Should still succeed, but with minimal examples (just negative examples)
    match result {
        Ok(res) => {
            assert!(
                res.examples_count > 0,
                "Should have generated at least negative examples"
            );
            println!(
                "No documentation test passed with {} examples",
                res.examples_count
            );
        }
        Err(e) => {
            // It's acceptable to fail if no examples can be generated
            println!("No documentation test handled correctly: {}", e);
        }
    }
}
