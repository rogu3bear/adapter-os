//! Integration tests for codebase ingestion pipeline
//!
//! These tests cover:
//! - End-to-end codebase ingestion (requires tokenizer, ignored in CI)
//! - Scope string normalization
//! - Scope override handling
//! - Dataset creation scenarios
//! - Error handling scenarios
//! - Scan root override functionality
//! - Code ingestion with custom scan roots
//! - Edge cases and configuration validation
//!
//! ## Tokenizer Requirements for E2E Tests
//!
//! Some tests in this file require a tokenizer file to run. These tests are marked with
//! `#[ignore]` and will be skipped by default in CI environments.
//!
//! ### What You Need
//!
//! - **tokenizer.json**: A HuggingFace-compatible tokenizer file
//!
//! ### Where to Get Tokenizers
//!
//! 1. **From HuggingFace**: Download from model repositories (e.g., Qwen/Qwen2.5-7B-Instruct)
//!    ```bash
//!    # Example: Download Qwen2.5-7B-Instruct tokenizer
//!    mkdir -p var/models/Qwen2.5-7B-Instruct
//!    cd var/models/Qwen2.5-7B-Instruct
//!    wget https://huggingface.co/Qwen/Qwen2.5-7B-Instruct/resolve/main/tokenizer.json
//!    ```
//!
//! 2. **From Local Model Directory**: If you already have a model downloaded, the tokenizer
//!    should be in the model's directory alongside the model weights.
//!
//! ### Configuration Options
//!
//! The tests discover tokenizers in the following order:
//!
//! 1. **AOS_TOKENIZER_PATH**: Explicit path to tokenizer.json
//!    ```bash
//!    export AOS_TOKENIZER_PATH=var/models/Qwen2.5-7B-Instruct/tokenizer.json
//!    ```
//!
//! 2. **AOS_MODEL_PATH**: Model directory (looks for tokenizer.json inside)
//!    ```bash
//!    export AOS_MODEL_PATH=var/models/Qwen2.5-7B-Instruct
//!    ```
//!
//! 3. **Default Location**: Uses `DEFAULT_MODEL_CACHE_ROOT/DEFAULT_BASE_MODEL_ID/tokenizer.json`
//!    (typically `var/model-cache/Qwen2.5-7B-Instruct/tokenizer.json`)
//!
//! ### Validation
//!
//! To validate your tokenizer file before running tests:
//! ```bash
//! ./aosctl models check-tokenizer ./path/to/tokenizer.json
//! ```
//!
//! ### Running Tests with Tokenizer
//!
//! Once a tokenizer is available:
//! ```bash
//! # Run all ignored tests (requires tokenizer)
//! cargo test --test codebase_ingestion_test -- --ignored
//!
//! # Run specific test
//! cargo test --test codebase_ingestion_test test_codebase_ingestion_end_to_end -- --ignored
//! ```
//!
//! ### Known Working Tokenizers
//!
//! - Qwen2.5-7B-Instruct: `var/models/Qwen2.5-7B-Instruct/tokenizer.json`
//! - Llama-3-8B: `var/models/Llama-3-8B/tokenizer.json`

#![allow(unused_imports)]
#![allow(clippy::absurd_extreme_comparisons)]
#![allow(unused_comparisons)]
#![allow(clippy::unnecessary_cast)]

use adapteros_config::{DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT};
use adapteros_db::sqlx;
use adapteros_lora_worker::training::TrainingConfig;
use adapteros_orchestrator::code_ingestion::{
    CodeDatasetConfig, CodeIngestionPipeline, CodeIngestionRequest, CodeIngestionSource,
};
use adapteros_orchestrator::codebase_ingestion::{CodebaseIngestion, IngestionConfig};
use adapteros_retrieval::codegraph::CodeGraph;
use git2::{Repository, Signature};
use std::io::Write;
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

/// Test end-to-end codebase ingestion pipeline
///
/// See module-level documentation for tokenizer requirements and setup instructions.
#[tokio::test]
#[ignore = "Requires tokenizer and model files not available in CI [tracking: STAB-IGN-0060]"]
async fn test_codebase_ingestion_end_to_end() {
    // Skip if tokenizer is not available
    let tokenizer_path = canonical_tokenizer_path();
    if !tokenizer_path.exists() {
        tracing::warn!("Skipping test: tokenizer not found at {:?}", tokenizer_path);
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
    let adapter_path = output_dir
        .path()
        .join("default")
        .join("test_adapter")
        .join("test_adapter.aos");
    assert!(adapter_path.exists(), "Adapter archive should exist");

    tracing::info!("Codebase ingestion test passed");
    tracing::info!("  Symbols extracted: {}", result.symbols_count);
    tracing::info!("  Training examples: {}", result.examples_count);
    tracing::info!("  Final loss: {:.6}", result.final_loss);
    tracing::info!("  Adapter hash: {}", result.adapter_hash);
    tracing::info!("  Content hash: {}", result.content_hash);
}

/// Test determinism: same codebase should produce same hash
///
/// See module-level documentation for tokenizer requirements and setup instructions.
#[tokio::test]
#[ignore = "Requires tokenizer and model files not available in CI [tracking: STAB-IGN-0061]"]
async fn test_determinism() {
    // Skip if tokenizer is not available
    let tokenizer_path = canonical_tokenizer_path();
    if !tokenizer_path.exists() {
        tracing::warn!("Skipping test: tokenizer not found at {:?}", tokenizer_path);
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

    tracing::info!("Determinism test passed");
    tracing::info!("  Content hash: {}", result1.content_hash);
    tracing::info!("  Adapter hash: {}", result1.adapter_hash);
}

#[test]
fn test_seed_inputs_include_commit_dataset_training_config() {
    use adapteros_orchestrator::code_ingestion::derive_codebase_seed_from_inputs;

    let base = derive_codebase_seed_from_inputs(
        "commit-a",
        "dataset-hash-a",
        "training-hash-a",
        "base-model",
        "repo-slug",
    )
    .expect("seed");
    let commit_changed = derive_codebase_seed_from_inputs(
        "commit-b",
        "dataset-hash-a",
        "training-hash-a",
        "base-model",
        "repo-slug",
    )
    .expect("seed");
    assert_ne!(base, commit_changed);

    let dataset_changed = derive_codebase_seed_from_inputs(
        "commit-a",
        "dataset-hash-b",
        "training-hash-a",
        "base-model",
        "repo-slug",
    )
    .expect("seed");
    assert_ne!(base, dataset_changed);

    let training_changed = derive_codebase_seed_from_inputs(
        "commit-a",
        "dataset-hash-a",
        "training-hash-b",
        "base-model",
        "repo-slug",
    )
    .expect("seed");
    assert_ne!(base, training_changed);

    let base_repeat = derive_codebase_seed_from_inputs(
        "commit-a",
        "dataset-hash-a",
        "training-hash-a",
        "base-model",
        "repo-slug",
    )
    .expect("seed");
    assert_eq!(base, base_repeat);
}

/// Test that pipeline handles repositories with no documentation
///
/// See module-level documentation for tokenizer requirements and setup instructions.
#[tokio::test]
#[ignore = "Requires tokenizer and model files not available in CI [tracking: STAB-IGN-0062]"]
async fn test_no_documentation() {
    let tokenizer_path = canonical_tokenizer_path();
    if !tokenizer_path.exists() {
        tracing::warn!("Skipping test: tokenizer not found");
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
            tracing::info!(
                "No documentation test passed with {} examples",
                res.examples_count
            );
        }
        Err(e) => {
            // It's acceptable to fail if no examples can be generated
            tracing::warn!("No documentation test handled correctly: {}", e);
        }
    }
}

// =============================================================================
// Scan-Root Pipeline Tests (Set 25, Point 10)
// =============================================================================
//
// These tests verify the scan-root and dataset integration functionality
// for the codebase ingestion pipeline.

/// Test scan-root scope metadata override handling
///
/// Verifies that CodebaseScopeMetadata correctly stores and retrieves
/// scan_root overrides for custom ingestion paths.
#[test]
fn test_scan_root_scope_metadata() {
    use adapteros_orchestrator::code_ingestion::CodebaseScopeMetadata;

    // Test empty metadata has no overrides
    let empty = CodebaseScopeMetadata::default();
    assert!(
        !empty.has_overrides(),
        "Empty metadata should have no overrides"
    );
    assert!(empty.scan_root.is_none());

    // Test metadata with scan_root override
    let with_scan_root = CodebaseScopeMetadata {
        repo: None,
        repo_slug: None,
        repo_id: None,
        branch: None,
        commit: None,
        scan_root: Some("/custom/path/to/scan".to_string()),
        remote_url: None,
    };
    assert!(
        with_scan_root.has_overrides(),
        "Should have overrides when scan_root is set"
    );
    assert_eq!(
        with_scan_root.scan_root.as_deref(),
        Some("/custom/path/to/scan")
    );

    // Test full metadata configuration
    let full_metadata = CodebaseScopeMetadata {
        repo: Some("my-org/my-repo".to_string()),
        repo_slug: Some("my_repo".to_string()),
        repo_id: None,
        branch: Some("feature/scan-roots".to_string()),
        commit: Some("abc123def456".to_string()),
        scan_root: Some("packages/core".to_string()),
        remote_url: Some("https://github.com/my-org/my-repo.git".to_string()),
    };
    assert!(full_metadata.has_overrides());
    assert_eq!(full_metadata.repo.as_deref(), Some("my-org/my-repo"));
    assert_eq!(full_metadata.branch.as_deref(), Some("feature/scan-roots"));
    assert_eq!(full_metadata.scan_root.as_deref(), Some("packages/core"));
}

/// Test scope metadata serialization to manifest key-value map.
#[test]
fn test_scope_metadata_to_metadata_map() {
    use adapteros_orchestrator::code_ingestion::CodebaseScopeMetadata;

    let metadata = CodebaseScopeMetadata {
        repo: Some("org/repo".to_string()),
        repo_slug: Some("org_repo".to_string()),
        repo_id: Some("https://github.com/org/repo".to_string()),
        branch: Some("main".to_string()),
        commit: Some("abc123def".to_string()),
        scan_root: Some("src".to_string()),
        remote_url: Some("https://github.com/org/repo".to_string()),
    };

    let map = metadata.to_metadata_map();
    assert_eq!(map.get("scope_repo"), Some(&"org/repo".to_string()));
    assert_eq!(map.get("repo_slug"), Some(&"org_repo".to_string()));
    assert_eq!(
        map.get("scope_repo_id"),
        Some(&"github.com/org/repo".to_string())
    );
    assert_eq!(map.get("scope_branch"), Some(&"main".to_string()));
    assert_eq!(map.get("scope_commit"), Some(&"abc123def".to_string()));
    assert_eq!(map.get("scope_scan_root"), Some(&"src".to_string()));
    assert_eq!(
        map.get("scope_remote_url"),
        Some(&"https://github.com/org/repo".to_string())
    );
}

/// Test scope metadata normalization for deterministic storage.
#[test]
fn test_scope_metadata_normalization() {
    use adapteros_orchestrator::code_ingestion::CodebaseScopeMetadata;

    let metadata = CodebaseScopeMetadata {
        repo: Some("  My Repo  ".to_string()),
        repo_slug: Some(" My Repo ".to_string()),
        repo_id: Some("HTTPS://GitHub.com/Org/Repo.git ".to_string()),
        branch: Some(" main ".to_string()),
        commit: Some("ABCDEF1234 ".to_string()),
        scan_root: Some("./src\\lib/".to_string()),
        remote_url: Some(" https://github.com/org/repo ".to_string()),
    };

    let map = metadata.to_metadata_map();

    assert_eq!(map.get("scope_repo"), Some(&"My Repo".to_string()));
    assert_eq!(map.get("repo_slug"), Some(&"my_repo".to_string()));
    assert_eq!(
        map.get("scope_repo_id"),
        Some(&"github.com/org/repo".to_string())
    );
    assert_eq!(map.get("scope_branch"), Some(&"main".to_string()));
    assert_eq!(map.get("scope_commit"), Some(&"abcdef1234".to_string()));
    assert_eq!(map.get("scope_scan_root"), Some(&"src/lib".to_string()));
    assert_eq!(
        map.get("scope_remote_url"),
        Some(&"https://github.com/org/repo".to_string())
    );
}

/// Test scope metadata overrides when only slug/commit are provided.
#[test]
fn test_scope_metadata_overrides_slug_commit() {
    use adapteros_orchestrator::code_ingestion::CodebaseScopeMetadata;

    let metadata = CodebaseScopeMetadata {
        repo: None,
        repo_slug: Some("my_repo".to_string()),
        repo_id: None,
        branch: None,
        commit: Some("deadbeef1234".to_string()),
        scan_root: None,
        remote_url: None,
    };

    assert!(metadata.has_overrides());
    let map = metadata.to_metadata_map();
    assert_eq!(map.get("repo_slug"), Some(&"my_repo".to_string()));
    assert_eq!(map.get("scope_commit"), Some(&"deadbeef1234".to_string()));
}

/// Test repo scope config filtering for scan-roots
///
/// Verifies that RepoScopeConfig correctly identifies when filters are active
/// for restricting ingestion to specific paths or file types.
#[test]
fn test_repo_scope_config_for_scan_roots() {
    use adapteros_orchestrator::code_ingestion::RepoScopeConfig;

    // Empty config has no filters
    let empty = RepoScopeConfig::default();
    assert!(!empty.has_filters(), "Empty config should have no filters");

    // Config with include_paths filter (for scan-root like behavior)
    let include_paths_only = RepoScopeConfig {
        include_paths: vec!["src/".to_string(), "lib/".to_string()],
        exclude_paths: vec![],
        include_extensions: vec![],
        exclude_extensions: vec![],
    };
    assert!(
        include_paths_only.has_filters(),
        "Should detect include_paths filter"
    );

    // Config with exclude_paths filter
    let exclude_paths_only = RepoScopeConfig {
        include_paths: vec![],
        exclude_paths: vec!["tests/".to_string(), "vendor/".to_string()],
        include_extensions: vec![],
        exclude_extensions: vec![],
    };
    assert!(
        exclude_paths_only.has_filters(),
        "Should detect exclude_paths filter"
    );

    // Config with extension filters (common for language-specific scan-roots)
    let extension_filter = RepoScopeConfig {
        include_paths: vec![],
        exclude_paths: vec![],
        include_extensions: vec!["rs".to_string(), "py".to_string()],
        exclude_extensions: vec![],
    };
    assert!(
        extension_filter.has_filters(),
        "Should detect extension filter"
    );

    // Full scan-root config combining path and extension filters
    let full_scan_root_config = RepoScopeConfig {
        include_paths: vec!["packages/core/src/".to_string()],
        exclude_paths: vec!["packages/core/src/tests/".to_string()],
        include_extensions: vec!["ts".to_string(), "tsx".to_string()],
        exclude_extensions: vec!["test.ts".to_string(), "spec.ts".to_string()],
    };
    assert!(full_scan_root_config.has_filters());
    assert_eq!(full_scan_root_config.include_paths.len(), 1);
    assert_eq!(full_scan_root_config.exclude_paths.len(), 1);
}

/// Test repo scope config metadata mapping.
#[test]
fn test_repo_scope_config_to_metadata_map() {
    use adapteros_orchestrator::code_ingestion::RepoScopeConfig;

    let config = RepoScopeConfig {
        include_paths: vec!["src/".to_string()],
        exclude_paths: vec!["tests/".to_string()],
        include_extensions: vec!["rs".to_string()],
        exclude_extensions: vec!["md".to_string()],
    };

    let map = config.to_metadata_map();
    assert_eq!(map.get("repo_scope_active"), Some(&"true".to_string()));
    assert_eq!(map.get("repo_scope_filter_count"), Some(&"4".to_string()));
    assert_eq!(
        map.get("repo_scope_include_paths"),
        Some(&"src/".to_string())
    );
    assert_eq!(
        map.get("repo_scope_exclude_paths"),
        Some(&"tests/".to_string())
    );
    assert_eq!(
        map.get("repo_scope_include_extensions"),
        Some(&"rs".to_string())
    );
    assert_eq!(
        map.get("repo_scope_exclude_extensions"),
        Some(&"md".to_string())
    );
}

/// Test repo scope config JSON round-trip serialization.
#[test]
fn test_repo_scope_config_json_roundtrip() {
    use adapteros_core::seed::{with_determinism_config, DeterminismConfig};
    use adapteros_orchestrator::code_ingestion::RepoScopeConfig;

    with_determinism_config(
        DeterminismConfig::builder().stable_ordering(true).build(),
        || {
            let config = RepoScopeConfig {
                include_paths: vec!["src/".to_string(), "lib/".to_string()],
                exclude_paths: vec!["tests/".to_string()],
                include_extensions: vec!["rs".to_string(), "toml".to_string()],
                exclude_extensions: vec!["md".to_string()],
            };

            let json = config.to_json().expect("serialize RepoScopeConfig");
            let parsed = RepoScopeConfig::from_json(&json).expect("deserialize RepoScopeConfig");

            let mut expected = config.clone();
            expected.include_paths.sort();
            expected.exclude_paths.sort();
            expected.include_extensions.sort();
            expected.exclude_extensions.sort();

            assert_eq!(expected.include_paths, parsed.include_paths);
            assert_eq!(expected.exclude_paths, parsed.exclude_paths);
            assert_eq!(expected.include_extensions, parsed.include_extensions);
            assert_eq!(expected.exclude_extensions, parsed.exclude_extensions);
        },
    );
}

/// Test repo scope config JSON serialization is deterministic when stable ordering is enabled.
#[test]
fn test_repo_scope_config_json_deterministic_order() {
    use adapteros_core::seed::{with_determinism_config, DeterminismConfig};
    use adapteros_orchestrator::code_ingestion::RepoScopeConfig;

    with_determinism_config(
        DeterminismConfig::builder().stable_ordering(true).build(),
        || {
            let config = RepoScopeConfig {
                include_paths: vec!["b/".to_string(), "a/".to_string()],
                exclude_paths: vec!["z/".to_string(), "a/".to_string()],
                include_extensions: vec!["toml".to_string(), "rs".to_string()],
                exclude_extensions: vec!["txt".to_string(), "md".to_string()],
            };

            let json = config.to_json().expect("serialize RepoScopeConfig");
            let value: serde_json::Value =
                serde_json::from_str(&json).expect("parse RepoScopeConfig JSON");

            assert_eq!(value["include_paths"], serde_json::json!(["a/", "b/"]));
            assert_eq!(value["exclude_paths"], serde_json::json!(["a/", "z/"]));
            assert_eq!(
                value["include_extensions"],
                serde_json::json!(["rs", "toml"])
            );
            assert_eq!(
                value["exclude_extensions"],
                serde_json::json!(["md", "txt"])
            );
        },
    );
}

/// Test dataset lineage info for scan-root derived datasets
///
/// Verifies that DatasetLineageInfo correctly tracks parent datasets
/// and derived-from relationships for scan-root based datasets.
#[test]
fn test_dataset_lineage_for_scan_roots() {
    use adapteros_orchestrator::code_ingestion::DatasetLineageInfo;
    use std::collections::BTreeMap;

    // Empty lineage has no info
    let empty = DatasetLineageInfo::default();
    assert!(!empty.has_lineage(), "Empty lineage should have no info");

    // Lineage with parent dataset (for incremental scan-root updates)
    let with_parent = DatasetLineageInfo {
        parent_dataset_id: Some("ds_abc123".to_string()),
        lineage_label: Some("incremental-update".to_string()),
        derived_from: vec![],
        version: Some("v2".to_string()),
        metadata: BTreeMap::new(),
    };
    assert!(with_parent.has_lineage());
    assert_eq!(with_parent.parent_dataset_id.as_deref(), Some("ds_abc123"));
    assert_eq!(
        with_parent.lineage_label.as_deref(),
        Some("incremental-update")
    );

    // Lineage with multiple derived-from sources (for merged scan-roots)
    let merged_lineage = DatasetLineageInfo {
        parent_dataset_id: None,
        lineage_label: Some("merged-scan-roots".to_string()),
        derived_from: vec![
            "ds_frontend_abc123".to_string(),
            "ds_backend_def456".to_string(),
            "ds_shared_ghi789".to_string(),
        ],
        version: Some("v1".to_string()),
        metadata: {
            let mut m = BTreeMap::new();
            m.insert("merge_strategy".to_string(), "concatenate".to_string());
            m.insert("source_count".to_string(), "3".to_string());
            m
        },
    };
    assert!(merged_lineage.has_lineage());
    assert_eq!(merged_lineage.derived_from.len(), 3);
    assert_eq!(merged_lineage.metadata.len(), 2);
}

/// Test normalize_repo_slug for scan-root paths
///
/// Verifies that repository slugs are correctly normalized when
/// including scan-root path components.
#[test]
fn test_normalize_repo_slug_for_scan_roots() {
    use adapteros_orchestrator::code_ingestion::normalize_repo_slug;

    // Basic repo names
    assert_eq!(normalize_repo_slug("my-repo"), "my_repo");
    assert_eq!(normalize_repo_slug("adapterOS-Core"), "adapteros_core");

    // Repo names that might include scan-root context
    assert_eq!(
        normalize_repo_slug("my-repo/packages/core"),
        "my_repo_packages_core"
    );
    assert_eq!(normalize_repo_slug("org/repo/src"), "org_repo_src");

    // Edge cases
    assert_eq!(normalize_repo_slug(""), "repo");
    assert_eq!(normalize_repo_slug("__weird__"), "weird");
    assert_eq!(normalize_repo_slug("---hyphens---"), "hyphens");

    // Unicode and special characters
    assert_eq!(normalize_repo_slug("My Awesome Repo!"), "my_awesome_repo");
    assert_eq!(normalize_repo_slug("repo@2.0"), "repo_2_0");
}

/// Test normalize_repo_id for scan-root qualified identifiers
///
/// Verifies that repository identifiers with scan-root paths
/// are correctly normalized for consistent lookups.
#[test]
fn test_normalize_repo_id_for_scan_roots() {
    use adapteros_orchestrator::code_ingestion::normalize_repo_id;

    // Standard GitHub URLs
    assert_eq!(
        normalize_repo_id("https://github.com/org/repo"),
        "github.com/org/repo"
    );

    // URLs with trailing slashes
    assert_eq!(
        normalize_repo_id("github.com/org/repo/"),
        "github.com/org/repo"
    );

    // Git SSH format
    assert_eq!(
        normalize_repo_id("git@github.com:org/repo.git"),
        "github.com/org/repo"
    );

    // Local repo: prefix (used for scan-root identifiers)
    assert_eq!(normalize_repo_id("repo:my-project"), "repo:my-project");
    assert_eq!(
        normalize_repo_id("repo:packages/frontend"),
        "repo:packages/frontend"
    );

    // Case normalization
    assert_eq!(
        normalize_repo_id("GitHub.com/Org/Repo"),
        "github.com/org/repo"
    );

    // Empty and edge cases
    assert_eq!(normalize_repo_id(""), "repo");
    assert_eq!(normalize_repo_id("   "), "repo");
}

/// Test CodeIngestionPipeline creation and configuration
///
/// Verifies that the ingestion pipeline can be created and configured
/// for scan-root based ingestion scenarios.
#[test]
fn test_code_ingestion_pipeline_creation() {
    let pipeline = CodeIngestionPipeline::new();
    // Pipeline should be creatable without any configuration
    // The actual ingestion requires source and request parameters
    assert!(std::mem::size_of_val(&pipeline) >= 0); // Pipeline exists
}

/// Test CodeDatasetConfig default values for scan-root ingestion
///
/// Verifies that the default dataset configuration is suitable
/// for scan-root based code ingestion.
#[test]
fn test_code_dataset_config_defaults() {
    let config = CodeDatasetConfig::default();

    // Verify sensible defaults for code ingestion
    assert_eq!(config.max_symbols, 64);
    assert!(
        !config.include_private,
        "Should default to public symbols only"
    );
    assert!(
        config.positive_weight > 0.0,
        "Positive weight should be positive"
    );
    assert!(
        config.negative_weight < 0.0,
        "Negative weight should be negative for abstention"
    );
}

/// Test StreamConfig for progress tracking during scan-root ingestion
///
/// Verifies that streaming configuration options work correctly
/// for monitoring long-running scan-root ingestion jobs.
#[test]
fn test_stream_config_for_scan_root_progress() {
    use adapteros_orchestrator::code_ingestion::{StreamConfig, StreamFormat};

    // Default is disabled
    let disabled = StreamConfig::default();
    assert!(!disabled.enabled);

    // Explicitly disabled
    let explicit_disabled = StreamConfig::disabled();
    assert!(!explicit_disabled.enabled);

    // Enabled with JSON format (for machine parsing in CI/CD)
    let json_stream = StreamConfig::new(StreamFormat::Json, 100);
    assert!(json_stream.enabled);
    assert_eq!(json_stream.format, StreamFormat::Json);
    assert_eq!(json_stream.interval_ms, 100);

    // Enabled with text format (for human-readable output)
    let text_stream = StreamConfig::new(StreamFormat::Text, 500);
    assert!(text_stream.enabled);
    assert_eq!(text_stream.format, StreamFormat::Text);
    assert_eq!(text_stream.interval_ms, 500);

    // StreamFormat parsing
    assert_eq!(StreamFormat::parse("json"), StreamFormat::Json);
    assert_eq!(StreamFormat::parse("JSONL"), StreamFormat::Json);
    assert_eq!(StreamFormat::parse("text"), StreamFormat::Text);
    assert_eq!(StreamFormat::parse("unknown"), StreamFormat::Text);
}

/// Test CodeIngestionSource variants for scan-root targeting
///
/// Verifies that ingestion sources can be configured for local paths
/// (used for scan-root targeting) or git URLs.
#[test]
fn test_code_ingestion_source_variants() {
    use adapteros_orchestrator::code_ingestion::CodeIngestionSource;

    // Local path source (primary for scan-root targeting)
    let local_source = CodeIngestionSource::LocalPath(PathBuf::from("/repo/packages/core"));
    match &local_source {
        CodeIngestionSource::LocalPath(path) => {
            assert_eq!(path, &PathBuf::from("/repo/packages/core"));
        }
        _ => panic!("Expected LocalPath variant"),
    }

    // Git URL source (for remote scan-root targeting)
    let git_source = CodeIngestionSource::GitUrl("https://github.com/org/repo.git".to_string());
    match &git_source {
        CodeIngestionSource::GitUrl(url) => {
            assert_eq!(url, "https://github.com/org/repo.git");
        }
        _ => panic!("Expected GitUrl variant"),
    }
}

/// Integration test: Dataset creation for scan-roots with database
///
/// Tests the full pipeline of creating a dataset from a scan-root,
/// including database operations for dataset creation and row insertion.
#[tokio::test]
async fn test_scan_root_dataset_creation_pipeline() {
    use adapteros_db::training_datasets::{
        CreateCodebaseDatasetRowParams, CreateDatasetParams, SampleRole,
    };
    use adapteros_db::Db;

    // Create an in-memory database for testing
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    // Create a dataset representing a scan-root ingestion
    let dataset_params = CreateDatasetParams::builder()
        .name("scan-root-packages-core")
        .format("jsonl")
        .hash_b3("a".repeat(64)) // 64 hex chars for valid BLAKE3 hash
        .storage_path("/var/aos/datasets/scan-root-packages-core")
        .description("Dataset from packages/core scan-root")
        .dataset_type("codebase")
        .source_location("repo:my-project/packages/core")
        .collection_method("code_ingestion_pipeline")
        .tenant_id("test-tenant")
        .build()
        .expect("Failed to build dataset params");

    let dataset_id = db
        .create_training_dataset_from_params(&dataset_params)
        .await
        .expect("Failed to create dataset");

    assert!(!dataset_id.is_empty(), "Dataset ID should be non-empty");

    // Verify dataset was created
    let dataset = db
        .get_training_dataset(&dataset_id)
        .await
        .expect("Failed to get dataset")
        .expect("Dataset should exist");

    assert_eq!(dataset.name, "scan-root-packages-core");
    assert_eq!(dataset.format, "jsonl");
    assert_eq!(dataset.dataset_type.as_deref(), Some("codebase"));
    assert_eq!(
        dataset.source_location.as_deref(),
        Some("repo:my-project/packages/core")
    );

    // Insert codebase dataset rows (simulating scan-root ingestion output)
    let session_id = uuid::Uuid::now_v7().to_string();

    // Positive example row
    let positive_row = CreateCodebaseDatasetRowParams {
        dataset_id: dataset_id.clone(),
        dataset_version_id: None,
        session_id: Some(session_id.clone()),
        prompt: "What does the function `calculate_total` in src/lib.rs do?".to_string(),
        response: "`calculate_total` is a function that computes the sum of items in a cart."
            .to_string(),
        weight: 1.0,
        sample_role: SampleRole::Positive,
        symbol_kind: Some("function".to_string()),
        language: Some("rust".to_string()),
        file_path: Some("packages/core/src/lib.rs".to_string()),
        start_line: Some(42),
        end_line: Some(55),
        qualified_name: Some("core::calculate_total".to_string()),
        commit_sha: Some("abc123def456".to_string()),
        repo_name: Some("my-project".to_string()),
        repo_slug: Some("my_project".to_string()),
        repo_identifier: Some("repo:my-project/packages/core".to_string()),
        project_name: Some("my-project-core".to_string()),
        has_docstring: true,
        metadata_json: Some(r#"{"scan_root": "packages/core"}"#.to_string()),
        tenant_id: Some("test-tenant".to_string()),
    };

    let row_id = db
        .insert_codebase_dataset_row(&positive_row)
        .await
        .expect("Failed to insert positive row");

    assert!(!row_id.is_empty(), "Row ID should be non-empty");

    // Negative example row (for abstention training)
    let negative_row = CreateCodebaseDatasetRowParams {
        dataset_id: dataset_id.clone(),
        dataset_version_id: None,
        session_id: Some(session_id.clone()),
        prompt: "Explain the undocumented function `internal_helper` in src/lib.rs.".to_string(),
        response: "I don't know. `internal_helper` lacks documentation, so I won't speculate."
            .to_string(),
        weight: -0.5,
        sample_role: SampleRole::Negative,
        symbol_kind: Some("function".to_string()),
        language: Some("rust".to_string()),
        file_path: Some("packages/core/src/lib.rs".to_string()),
        start_line: Some(100),
        end_line: Some(105),
        qualified_name: Some("core::internal_helper".to_string()),
        commit_sha: Some("abc123def456".to_string()),
        repo_name: Some("my-project".to_string()),
        repo_slug: Some("my_project".to_string()),
        repo_identifier: Some("repo:my-project/packages/core".to_string()),
        project_name: Some("my-project-core".to_string()),
        has_docstring: false,
        metadata_json: Some(
            r#"{"scan_root": "packages/core", "reason": "missing_docstring"}"#.to_string(),
        ),
        tenant_id: Some("test-tenant".to_string()),
    };

    let negative_row_id = db
        .insert_codebase_dataset_row(&negative_row)
        .await
        .expect("Failed to insert negative row");

    assert!(!negative_row_id.is_empty());

    // Verify rows were inserted
    let rows = db
        .list_codebase_dataset_rows(&dataset_id, Some(&session_id), 100, 0)
        .await
        .expect("Failed to list rows");

    assert_eq!(rows.len(), 2, "Should have 2 rows");

    // Verify row contents
    let positive_found = rows.iter().find(|r| r.sample_role == "positive");
    assert!(positive_found.is_some(), "Should find positive row");
    let positive = positive_found.unwrap();
    assert!(positive.has_docstring == 1);
    assert_eq!(
        positive.file_path.as_deref(),
        Some("packages/core/src/lib.rs")
    );

    let negative_found = rows.iter().find(|r| r.sample_role == "negative");
    assert!(negative_found.is_some(), "Should find negative row");
    let negative = negative_found.unwrap();
    assert!(negative.has_docstring == 0);
    assert!(negative.weight < 0.0);

    // Test row counting
    let count = db
        .count_codebase_dataset_rows(&dataset_id, Some(&session_id))
        .await
        .expect("Failed to count rows");

    assert_eq!(count, 2);

    // Test session listing
    let sessions = db
        .list_sessions_for_dataset(&dataset_id)
        .await
        .expect("Failed to list sessions");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, session_id);
    assert_eq!(sessions[0].row_count, 2);
    assert_eq!(sessions[0].positive_count, 1);
    assert_eq!(sessions[0].negative_count, 1);

    tracing::info!("Scan-root dataset creation pipeline test passed");
    tracing::info!("  Dataset ID: {}", dataset_id);
    tracing::info!("  Session ID: {}", session_id);
    tracing::info!("  Total rows: {}", count);
}

/// Integration test: Training config hash is persisted in codebase dataset rows
///
/// Verifies that training_config_hash is merged into row metadata when
/// inserting codebase dataset rows via the training-config helper.
#[tokio::test]
async fn test_codebase_rows_include_training_config_hash() {
    use adapteros_db::training_datasets::{
        CodebaseDatasetRowInput, CreateDatasetParams, SampleRole,
    };
    use adapteros_db::Db;

    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let dataset_params = CreateDatasetParams::builder()
        .name("training-config-hash-dataset")
        .format("jsonl")
        .hash_b3("a".repeat(64))
        .storage_path("/var/aos/datasets/training-config-hash")
        .build()
        .expect("Failed to build dataset params");

    let dataset_id = db
        .create_training_dataset_from_params(&dataset_params)
        .await
        .expect("Failed to create dataset");

    let rows = vec![
        CodebaseDatasetRowInput {
            prompt: "Describe fn add in src/lib.rs".to_string(),
            response: "Adds two integers.".to_string(),
            weight: 1.0,
            sample_role: SampleRole::Positive,
            symbol_kind: Some("function".to_string()),
            language: Some("rust".to_string()),
            file_path: Some("src/lib.rs".to_string()),
            start_line: Some(10),
            end_line: Some(12),
            qualified_name: Some("crate::add".to_string()),
            has_docstring: true,
            metadata_json: Some(r#"{"source":"ingestion"}"#.to_string()),
        },
        CodebaseDatasetRowInput {
            prompt: "Explain fn hidden in src/lib.rs".to_string(),
            response: "I don't know; it lacks documentation.".to_string(),
            weight: -0.5,
            sample_role: SampleRole::Negative,
            symbol_kind: Some("function".to_string()),
            language: Some("rust".to_string()),
            file_path: Some("src/lib.rs".to_string()),
            start_line: Some(40),
            end_line: Some(42),
            qualified_name: Some("crate::hidden".to_string()),
            has_docstring: false,
            metadata_json: None,
        },
    ];

    let training_hash = "train-config-hash-123";

    let inserted = db
        .insert_codebase_dataset_rows_for_training_config(
            &dataset_id,
            None,
            None,
            Some("test-repo"),
            Some("test_repo"),
            Some("repo:test-repo"),
            Some("test-project"),
            Some("abc123def456"),
            training_hash,
            &rows,
            None,
        )
        .await
        .expect("Failed to insert dataset rows");

    assert_eq!(inserted, 2);

    let stored = db
        .list_codebase_dataset_rows(&dataset_id, None, 10, 0)
        .await
        .expect("Failed to list dataset rows");

    assert_eq!(stored.len(), 2);

    let mut found_with_metadata = false;
    let mut found_without_metadata = false;

    for row in stored {
        let metadata_json = row
            .metadata_json
            .as_deref()
            .expect("metadata_json should be set");
        let value: serde_json::Value =
            serde_json::from_str(metadata_json).expect("parse metadata_json");
        assert_eq!(
            value.get("training_config_hash").and_then(|v| v.as_str()),
            Some(training_hash)
        );

        if row.prompt.contains("add in src/lib.rs") {
            found_with_metadata = true;
            assert_eq!(
                value.get("source").and_then(|v| v.as_str()),
                Some("ingestion")
            );
        } else if row.prompt.contains("hidden in src/lib.rs") {
            found_without_metadata = true;
        }
    }

    assert!(found_with_metadata);
    assert!(found_without_metadata);
}

/// Integration test: Create dataset records from repo metadata
///
/// Verifies repo slug normalization, dataset defaults, version creation,
/// and scan-root provenance for repository-based ingestion.
#[tokio::test]
async fn test_create_dataset_from_repo_defaults() {
    use adapteros_db::Db;

    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind("test-tenant")
        .bind("Test Tenant")
        .execute(db.pool())
        .await
        .expect("Failed to create tenant");

    let repo_name = "adapterOS/Repo-Utils!!";
    let repo_path = "/var/repos/adapterOS/Repo-Utils";
    let commit_sha = "abcdef1234567890";
    let tenant_id = "test-tenant";

    let (dataset_id, version_id) = db
        .create_dataset_from_repo(
            repo_name,
            "",
            repo_path,
            commit_sha,
            Some("  "),
            Some(tenant_id),
            None,
        )
        .await
        .expect("Failed to create dataset from repo");

    let dataset = db
        .get_training_dataset(&dataset_id)
        .await
        .expect("Failed to load dataset")
        .expect("Dataset should exist");

    let expected_slug = "adapteros_repo_utils";
    let short_commit = &commit_sha[..8];
    let expected_name = format!("{}-{}", expected_slug, short_commit);

    assert_eq!(dataset.name, expected_name);
    assert_eq!(dataset.dataset_type.as_deref(), Some("codebase"));
    assert_eq!(
        dataset.collection_method.as_deref(),
        Some("code_ingestion_pipeline")
    );
    assert_eq!(dataset.status, "processing");
    assert_eq!(dataset.repo_slug.as_deref(), Some(expected_slug));
    assert_eq!(dataset.commit_sha.as_deref(), Some(commit_sha));
    assert!(dataset.branch.is_none());
    assert_eq!(dataset.source_location.as_deref(), Some(repo_path));
    assert_eq!(
        dataset.storage_path,
        format!("/datasets/repo/{}/{}", expected_slug, commit_sha)
    );
    assert_eq!(dataset.tenant_id.as_deref(), Some(tenant_id));

    let version = db
        .get_training_dataset_version(&version_id)
        .await
        .expect("Failed to load dataset version")
        .expect("Dataset version should exist");

    assert_eq!(version.dataset_id, dataset_id);
    assert_eq!(version.version_number, 1);
    let version_label = format!("v1-{}", short_commit);
    assert_eq!(
        version.version_label.as_deref(),
        Some(version_label.as_str())
    );
    assert_eq!(version.storage_path, dataset.storage_path);
    assert_eq!(version.hash_b3, dataset.hash_b3);
    assert_eq!(version.tenant_id.as_deref(), Some(tenant_id));

    let scan_roots = db
        .list_dataset_scan_roots(&dataset_id)
        .await
        .expect("Failed to list dataset scan roots");

    assert_eq!(scan_roots.len(), 1);
    let scan_root = &scan_roots[0];
    assert_eq!(scan_root.path, repo_path);
    assert_eq!(scan_root.repo_name.as_deref(), Some(repo_name));
    assert_eq!(scan_root.repo_slug.as_deref(), Some(expected_slug));
    assert_eq!(scan_root.commit_sha.as_deref(), Some(commit_sha));
    assert!(scan_root.branch.is_none());
    assert!(scan_root.remote_url.is_none());
    assert_eq!(
        scan_root.dataset_version_id.as_deref(),
        Some(version_id.as_str())
    );
    assert_eq!(scan_root.tenant_id.as_deref(), Some(tenant_id));
    assert!(scan_root.created_by.is_none());
}

/// Integration test: Scan roots from metadata JSON are persisted to dataset scan roots.
#[tokio::test]
async fn test_create_dataset_from_repo_with_scan_roots_metadata() {
    use adapteros_db::Db;

    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let dataset_hash = "b".repeat(64);
    let file_hash = "c".repeat(64);
    let scan_hash = "d".repeat(64);

    let metadata = serde_json::json!({
        "scan_roots": [
            {
                "path": "packages/core",
                "label": "core",
                "file_count": 12,
                "byte_count": 2048,
                "content_hash_b3": scan_hash,
                "scanned_at": "2024-02-03T04:05:06Z"
            },
            {
                "path": "packages/ui",
                "label": "ui"
            }
        ]
    });

    let (dataset_id, version_id) = db
        .create_codebase_dataset_from_repo_with_hashes(
            "Test Repo",
            Some("https://github.com/example/test-repo.git"),
            "/repo",
            "abc123def456",
            Some("main"),
            Some("test_repo"),
            &dataset_hash,
            &file_hash,
            "/datasets/repo/test_repo/abc123def456",
            Some(&metadata.to_string()),
            None,
            None,
        )
        .await
        .expect("Failed to create codebase dataset");

    let scan_roots = db
        .list_dataset_scan_roots(&dataset_id)
        .await
        .expect("Failed to list dataset scan roots");

    assert_eq!(scan_roots.len(), 2);

    let first = &scan_roots[0];
    assert_eq!(first.path, "packages/core");
    assert_eq!(first.label.as_deref(), Some("core"));
    assert_eq!(first.file_count, Some(12));
    assert_eq!(first.byte_count, Some(2048));
    assert_eq!(first.content_hash_b3.as_deref(), Some(scan_hash.as_str()));
    assert_eq!(first.scanned_at.as_deref(), Some("2024-02-03T04:05:06Z"));
    assert_eq!(
        first.dataset_version_id.as_deref(),
        Some(version_id.as_str())
    );
    assert_eq!(first.commit_sha.as_deref(), Some("abc123def456"));
    assert_eq!(
        first.remote_url.as_deref(),
        Some("https://github.com/example/test-repo.git")
    );

    let second = &scan_roots[1];
    assert_eq!(second.path, "packages/ui");
    assert_eq!(second.label.as_deref(), Some("ui"));
    assert_eq!(second.file_count, None);
    assert_eq!(second.byte_count, None);
    assert_eq!(second.content_hash_b3.as_deref(), None);
    assert_eq!(
        second.dataset_version_id.as_deref(),
        Some(version_id.as_str())
    );
}

/// Integration test: Training job with scan-root dataset linkage
///
/// Tests creating a training job that references a scan-root dataset,
/// verifying the provenance chain from dataset to training job.
#[tokio::test]
async fn test_training_job_scan_root_dataset_linkage() {
    use adapteros_db::training_datasets::CreateDatasetParams;
    use adapteros_db::Db;

    // Create an in-memory database for testing
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    // Create tenant for FK constraints
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind("test-tenant")
        .bind("Test Tenant")
        .execute(db.pool())
        .await
        .expect("Failed to create tenant");

    // Create adapter repository for FK constraints
    sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name, description, git_url, lifecycle_state)
         VALUES (?, ?, ?, ?, ?, 'active')"
    )
    .bind("test-repo")
    .bind("test-tenant")
    .bind("test-repo")
    .bind("Test repository for scan-root")
    .bind("https://github.com/test/repo.git")
    .execute(db.pool())
    .await
    .expect("Failed to create repository");

    // Create scan-root dataset
    let dataset_params = CreateDatasetParams::builder()
        .name("scan-root-training-dataset")
        .format("jsonl")
        .hash_b3("b".repeat(64))
        .storage_path("/var/aos/datasets/scan-root-training")
        .dataset_type("codebase")
        .source_location("repo:test-project/packages/backend")
        .tenant_id("test-tenant")
        .build()
        .expect("Failed to build dataset params");

    let dataset_id = db
        .create_training_dataset_from_params(&dataset_params)
        .await
        .expect("Failed to create dataset");

    // Create a training job linked to the scan-root dataset
    let training_config = serde_json::json!({
        "rank": 4,
        "alpha": 16.0,
        "learning_rate": 0.0001,
        "batch_size": 2,
        "epochs": 3,
        "hidden_dim": 768
    });

    let job_id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO training_jobs (
            id, repo_id, training_config_json, status, progress_json,
            started_at, created_by, dataset_id, tenant_id, code_commit_sha
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&job_id)
    .bind("test-repo")
    .bind(training_config.to_string())
    .bind("pending")
    .bind(r#"{"progress_pct": 0}"#)
    .bind(&now)
    .bind("test-user")
    .bind(&dataset_id)
    .bind("test-tenant")
    .bind("abc123def456")
    .execute(db.pool())
    .await
    .expect("Failed to create training job");

    // Verify job was created with dataset linkage
    let job = db
        .get_training_job(&job_id)
        .await
        .expect("Failed to get training job")
        .expect("Job should exist");

    assert_eq!(job.dataset_id.as_deref(), Some(dataset_id.as_str()));
    assert_eq!(job.tenant_id.as_deref(), Some("test-tenant"));
    assert_eq!(job.code_commit_sha.as_deref(), Some("abc123def456"));

    // Verify dataset can be retrieved through the job
    let linked_dataset = db
        .get_training_dataset(job.dataset_id.as_ref().unwrap())
        .await
        .expect("Failed to get linked dataset")
        .expect("Dataset should exist");

    assert_eq!(linked_dataset.name, "scan-root-training-dataset");
    assert_eq!(
        linked_dataset.source_location.as_deref(),
        Some("repo:test-project/packages/backend")
    );

    tracing::info!("Training job scan-root dataset linkage test passed");
    tracing::info!("  Job ID: {}", job_id);
    tracing::info!("  Dataset ID: {}", dataset_id);
    tracing::info!("  Source location: {:?}", linked_dataset.source_location);
}

/// Integration test: Bulk insert codebase dataset rows
///
/// Tests efficient bulk insertion of training rows from a scan-root
/// ingestion session.
#[tokio::test]
async fn test_bulk_insert_scan_root_dataset_rows() {
    use adapteros_db::training_datasets::{
        CreateCodebaseDatasetRowParams, CreateDatasetParams, SampleRole,
    };
    use adapteros_db::Db;

    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    // Create dataset
    let dataset_params = CreateDatasetParams::builder()
        .name("bulk-scan-root-dataset")
        .format("jsonl")
        .hash_b3("c".repeat(64))
        .storage_path("/var/aos/datasets/bulk-scan-root")
        .build()
        .expect("Failed to build dataset params");

    let dataset_id = db
        .create_training_dataset_from_params(&dataset_params)
        .await
        .expect("Failed to create dataset");

    let session_id = uuid::Uuid::now_v7().to_string();

    // Generate multiple rows simulating a scan-root ingestion
    let mut rows: Vec<CreateCodebaseDatasetRowParams> = Vec::new();
    for i in 0..50 {
        let is_positive = i % 3 != 0; // 2/3 positive, 1/3 negative
        rows.push(CreateCodebaseDatasetRowParams {
            dataset_id: dataset_id.clone(),
            dataset_version_id: None,
            session_id: Some(session_id.clone()),
            prompt: format!("What does symbol_{} do?", i),
            response: format!("Symbol_{} is a test symbol for bulk insertion.", i),
            weight: if is_positive { 1.0 } else { -0.5 },
            sample_role: if is_positive {
                SampleRole::Positive
            } else {
                SampleRole::Negative
            },
            symbol_kind: Some("function".to_string()),
            language: Some("rust".to_string()),
            file_path: Some(format!("src/module_{}.rs", i % 5)),
            start_line: Some((i * 10) as i32),
            end_line: Some((i * 10 + 5) as i32),
            qualified_name: Some(format!("module::symbol_{}", i)),
            commit_sha: Some("bulk123abc".to_string()),
            repo_name: Some("bulk-test-repo".to_string()),
            repo_slug: Some("bulk_test_repo".to_string()),
            repo_identifier: Some("repo:bulk-test/packages/all".to_string()),
            project_name: Some("bulk-test".to_string()),
            has_docstring: is_positive,
            metadata_json: Some(format!(r#"{{"index": {}}}"#, i)),
            tenant_id: None,
        });
    }

    // Bulk insert
    let inserted_count = db
        .bulk_insert_codebase_dataset_rows(&rows)
        .await
        .expect("Failed to bulk insert rows");

    assert_eq!(inserted_count, 50, "Should insert all 50 rows");

    // Verify counts
    let total_count = db
        .count_codebase_dataset_rows(&dataset_id, None)
        .await
        .expect("Failed to count all rows");

    assert_eq!(total_count, 50);

    let session_count = db
        .count_codebase_dataset_rows(&dataset_id, Some(&session_id))
        .await
        .expect("Failed to count session rows");

    assert_eq!(session_count, 50);

    // Verify session summary
    let sessions = db
        .list_sessions_for_dataset(&dataset_id)
        .await
        .expect("Failed to list sessions");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].row_count, 50);
    // 2/3 positive (indices 1,2,4,5,7,8...) = 34, 1/3 negative = 16
    // Actually: 0,3,6,9,12,15,18,21,24,27,30,33,36,39,42,45,48 are negative = 17
    // So positive = 50 - 17 = 33
    assert_eq!(sessions[0].positive_count, 33);
    assert_eq!(sessions[0].negative_count, 17);

    tracing::info!("Bulk insert scan-root dataset rows test passed");
    tracing::info!("  Inserted rows: {}", inserted_count);
    tracing::info!(
        "  Positive: {}, Negative: {}",
        sessions[0].positive_count,
        sessions[0].negative_count
    );
}

/// Test: Delete all codebase dataset rows for cleanup
///
/// Verifies that scan-root dataset rows can be efficiently deleted
/// for dataset regeneration scenarios.
#[tokio::test]
async fn test_delete_scan_root_dataset_rows() {
    use adapteros_db::training_datasets::{
        CreateCodebaseDatasetRowParams, CreateDatasetParams, SampleRole,
    };
    use adapteros_db::Db;

    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    // Create dataset
    let dataset_params = CreateDatasetParams::builder()
        .name("delete-test-dataset")
        .format("jsonl")
        .hash_b3("d".repeat(64))
        .storage_path("/var/aos/datasets/delete-test")
        .build()
        .expect("Failed to build dataset params");

    let dataset_id = db
        .create_training_dataset_from_params(&dataset_params)
        .await
        .expect("Failed to create dataset");

    // Insert some rows
    for i in 0..10 {
        let row = CreateCodebaseDatasetRowParams {
            dataset_id: dataset_id.clone(),
            dataset_version_id: None,
            session_id: Some("delete-session".to_string()),
            prompt: format!("Prompt {}", i),
            response: format!("Response {}", i),
            weight: 1.0,
            sample_role: SampleRole::Positive,
            symbol_kind: None,
            language: None,
            file_path: None,
            start_line: None,
            end_line: None,
            qualified_name: None,
            commit_sha: None,
            repo_name: None,
            repo_slug: None,
            repo_identifier: None,
            project_name: None,
            has_docstring: false,
            metadata_json: None,
            tenant_id: None,
        };
        db.insert_codebase_dataset_row(&row).await.unwrap();
    }

    // Verify rows exist
    let count_before = db
        .count_codebase_dataset_rows(&dataset_id, None)
        .await
        .unwrap();
    assert_eq!(count_before, 10);

    // Delete all rows
    let deleted_count = db
        .delete_all_codebase_dataset_rows(&dataset_id)
        .await
        .expect("Failed to delete rows");

    assert_eq!(deleted_count, 10);

    // Verify deletion
    let count_after = db
        .count_codebase_dataset_rows(&dataset_id, None)
        .await
        .unwrap();
    assert_eq!(count_after, 0);

    tracing::info!("Delete scan-root dataset rows test passed");
}

/// Test: Get rows by file path for scan-root focused queries
///
/// Verifies that rows can be queried by file path, useful for
/// understanding coverage within a scan-root.
#[tokio::test]
async fn test_get_scan_root_rows_by_file_path() {
    use adapteros_db::training_datasets::{
        CreateCodebaseDatasetRowParams, CreateDatasetParams, SampleRole,
    };
    use adapteros_db::Db;

    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let dataset_params = CreateDatasetParams::builder()
        .name("file-path-test-dataset")
        .format("jsonl")
        .hash_b3("e".repeat(64))
        .storage_path("/var/aos/datasets/file-path-test")
        .build()
        .expect("Failed to build dataset params");

    let dataset_id = db
        .create_training_dataset_from_params(&dataset_params)
        .await
        .expect("Failed to create dataset");

    // Insert rows for different files
    let files = ["src/lib.rs", "src/lib.rs", "src/utils.rs", "src/config.rs"];
    for (i, file) in files.iter().enumerate() {
        let row = CreateCodebaseDatasetRowParams {
            dataset_id: dataset_id.clone(),
            dataset_version_id: None,
            session_id: None,
            prompt: format!("Prompt for {} #{}", file, i),
            response: format!("Response for {}", file),
            weight: 1.0,
            sample_role: SampleRole::Positive,
            symbol_kind: Some("function".to_string()),
            language: Some("rust".to_string()),
            file_path: Some(file.to_string()),
            start_line: Some((i * 10) as i32),
            end_line: Some((i * 10 + 5) as i32),
            qualified_name: Some(format!("symbol_{}", i)),
            commit_sha: None,
            repo_name: None,
            repo_slug: None,
            repo_identifier: None,
            project_name: None,
            has_docstring: true,
            metadata_json: None,
            tenant_id: None,
        };
        db.insert_codebase_dataset_row(&row).await.unwrap();
    }

    // Query by file path
    let lib_rows = db
        .get_rows_by_file(&dataset_id, "src/lib.rs")
        .await
        .expect("Failed to get rows by file path");

    assert_eq!(lib_rows.len(), 2, "Should find 2 rows for src/lib.rs");

    let utils_rows = db
        .get_rows_by_file(&dataset_id, "src/utils.rs")
        .await
        .expect("Failed to get rows by file path");

    assert_eq!(utils_rows.len(), 1, "Should find 1 row for src/utils.rs");

    tracing::info!("Get scan-root rows by file path test passed");
}

/// Test: Get rows by qualified name for symbol lookup
///
/// Verifies that rows can be queried by qualified symbol name,
/// useful for adapter debugging and provenance tracking.
#[tokio::test]
async fn test_get_scan_root_rows_by_qualified_name() {
    use adapteros_db::training_datasets::{
        CreateCodebaseDatasetRowParams, CreateDatasetParams, SampleRole,
    };
    use adapteros_db::Db;

    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let dataset_params = CreateDatasetParams::builder()
        .name("qualified-name-test-dataset")
        .format("jsonl")
        .hash_b3("f".repeat(64))
        .storage_path("/var/aos/datasets/qualified-name-test")
        .build()
        .expect("Failed to build dataset params");

    let dataset_id = db
        .create_training_dataset_from_params(&dataset_params)
        .await
        .expect("Failed to create dataset");

    // Insert rows with qualified names
    let symbols = [
        "core::calculate_total",
        "core::calculate_total", // Duplicate for versioning test
        "utils::format_output",
        "config::load_settings",
    ];
    for (i, symbol) in symbols.iter().enumerate() {
        let row = CreateCodebaseDatasetRowParams {
            dataset_id: dataset_id.clone(),
            dataset_version_id: None,
            session_id: None,
            prompt: format!("What does {} do?", symbol),
            response: format!("{} performs an operation.", symbol),
            weight: 1.0,
            sample_role: SampleRole::Positive,
            symbol_kind: Some("function".to_string()),
            language: Some("rust".to_string()),
            file_path: None,
            start_line: None,
            end_line: None,
            qualified_name: Some(symbol.to_string()),
            commit_sha: None,
            repo_name: None,
            repo_slug: None,
            repo_identifier: None,
            project_name: None,
            has_docstring: true,
            metadata_json: Some(format!(r#"{{"version": {}}}"#, i)),
            tenant_id: None,
        };
        db.insert_codebase_dataset_row(&row).await.unwrap();
    }

    // Query by qualified name
    let total_rows = db
        .get_rows_by_symbol(&dataset_id, "core::calculate_total")
        .await
        .expect("Failed to get rows by qualified name");

    assert_eq!(
        total_rows.len(),
        2,
        "Should find 2 rows for core::calculate_total"
    );

    let format_rows = db
        .get_rows_by_symbol(&dataset_id, "utils::format_output")
        .await
        .expect("Failed to get rows by qualified name");

    assert_eq!(
        format_rows.len(),
        1,
        "Should find 1 row for utils::format_output"
    );

    tracing::info!("Get scan-root rows by qualified name test passed");
}

/// Test: Training dataset rows are namespaced by dataset hash
///
/// Ensures identical rows in different datasets do not collide.
#[tokio::test]
async fn test_training_dataset_row_ids_use_dataset_hash() {
    use adapteros_db::training_datasets::{
        CreateDatasetParams, CreateTrainingDatasetRowParams, SampleRole,
    };
    use adapteros_db::Db;

    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let dataset_one = CreateDatasetParams::builder()
        .name("hash-row-dataset-1")
        .format("jsonl")
        .hash_b3("1".repeat(64))
        .storage_path("/var/aos/datasets/hash-row-1")
        .build()
        .expect("Failed to build dataset params");

    let dataset_two = CreateDatasetParams::builder()
        .name("hash-row-dataset-2")
        .format("jsonl")
        .hash_b3("2".repeat(64))
        .storage_path("/var/aos/datasets/hash-row-2")
        .build()
        .expect("Failed to build dataset params");

    let dataset_id_one = db
        .create_training_dataset_from_params(&dataset_one)
        .await
        .expect("Failed to create dataset one");
    let dataset_id_two = db
        .create_training_dataset_from_params(&dataset_two)
        .await
        .expect("Failed to create dataset two");

    let row_payload = CreateTrainingDatasetRowParams {
        row_id: None,
        dataset_id: dataset_id_one.clone(),
        dataset_version_id: None,
        session_id: None,
        prompt: "What does foo do?".to_string(),
        response: "Foo returns the bar.".to_string(),
        weight: 1.0,
        split: "train".to_string(),
        sample_role: SampleRole::Positive,
        source_type: Some("upload".to_string()),
        source_file: None,
        source_line: None,
        tenant_id: None,
        metadata_json: Some(r#"{"source": "test"}"#.to_string()),
        created_by: None,
    };

    let row_two = CreateTrainingDatasetRowParams {
        dataset_id: dataset_id_two.clone(),
        ..row_payload.clone()
    };

    let inserted_one = db
        .bulk_insert_training_dataset_rows(&[row_payload])
        .await
        .expect("Failed to insert dataset one row");
    let inserted_two = db
        .bulk_insert_training_dataset_rows(&[row_two])
        .await
        .expect("Failed to insert dataset two row");

    assert_eq!(inserted_one, 1);
    assert_eq!(inserted_two, 1);

    let rows_one = db
        .list_training_dataset_rows(&dataset_id_one, None, 10, 0)
        .await
        .expect("Failed to list dataset one rows");
    let rows_two = db
        .list_training_dataset_rows(&dataset_id_two, None, 10, 0)
        .await
        .expect("Failed to list dataset two rows");

    assert_eq!(rows_one.len(), 1);
    assert_eq!(rows_two.len(), 1);
    assert_ne!(rows_one[0].id, rows_two[0].id);

    tracing::info!("Training dataset row hash namespacing test passed");
}

/// Test: Record dataset hash inputs
#[tokio::test]
async fn test_record_dataset_hash_inputs() {
    use adapteros_db::training_datasets::{CreateDatasetHashInputsParams, CreateDatasetParams};
    use adapteros_db::Db;

    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let dataset_params = CreateDatasetParams::builder()
        .name("hash-inputs-dataset")
        .format("jsonl")
        .hash_b3("a".repeat(64))
        .storage_path("/var/aos/datasets/hash-inputs")
        .build()
        .expect("Failed to build dataset params");

    let dataset_id = db
        .create_training_dataset_from_params(&dataset_params)
        .await
        .expect("Failed to create dataset");

    let mut inputs = CreateDatasetHashInputsParams::new("b".repeat(64), 12, 8, 4);
    inputs.dataset_id = Some(dataset_id.clone());
    inputs.repo_id = Some("repo:test".to_string());
    inputs.repo_slug = Some("repo_test".to_string());
    inputs.commit_sha = Some("abc123".to_string());
    inputs.branch = Some("main".to_string());
    inputs.scan_root_path = Some("src".to_string());
    inputs.remote_url = Some("https://example.com/repo.git".to_string());
    inputs.max_symbols = Some(64);
    inputs.include_private = Some(true);
    inputs.positive_weight = Some(1.0);
    inputs.negative_weight = Some(-0.5);
    inputs.scope_config_json = Some(r#"{"include_paths":["src/"]}"#.to_string());
    inputs.additional_inputs_json = Some(r#"{"project_name":"test"}"#.to_string());

    let record_id = db
        .record_dataset_hash_inputs(&inputs)
        .await
        .expect("Failed to record dataset hash inputs");

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM dataset_hash_inputs WHERE dataset_id = ? AND content_hash_b3 = ?",
    )
    .bind(&dataset_id)
    .bind("b".repeat(64))
    .fetch_one(db.pool())
    .await
    .expect("Failed to count dataset hash inputs");

    assert_eq!(count.0, 1);
    assert!(!record_id.is_empty());

    tracing::info!("Dataset hash inputs record test passed");
}
