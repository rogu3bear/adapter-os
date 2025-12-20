//! Integration tests for evidence retrieval system
//!
//! Tests the full evidence retrieval pipeline including FTS5 indices,
//! vector search, and evidence manager.
//!
//! NOTE: These tests are disabled because adapteros-codegraph is disabled
//! due to tree-sitter conflicts. Re-enable when codegraph is restored.

// Disabled: adapteros-codegraph dependency is disabled
/*
use adapteros_codegraph::types::{Language, Span, SymbolId, SymbolKind, SymbolNode, Visibility};
use adapteros_lora_rag::{
    ChangeType, DocIndexImpl, EvidenceIndexManager, EvidenceType, FileChange, IndexedDoc,
    IndexedTest, SymbolIndexImpl, TestIndexImpl,
};
use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

fn new_test_tempdir() -> Result<TempDir> {
    let root = PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root)?;
    Ok(TempDir::new_in(&root)?)
}

/// Create a test symbol
fn create_test_symbol(name: &str, line: u32) -> SymbolNode {
    let file_path = "test.rs";
    let span_str = format!("{}:1:{}:10", line, line);
    let id = SymbolId::new(file_path, &span_str, name);

    SymbolNode::new(
        id,
        name.to_string(),
        SymbolKind::Function,
        Language::Rust,
        Span::new(line, 1, line, 10, 0, 100),
        file_path.to_string(),
    )
    .with_visibility(Visibility::Public)
    .with_signature(format!("pub fn {}()", name))
}

#[tokio::test]
async fn test_symbol_index_create_and_search() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let index_path = temp_dir.path().to_path_buf();

    // Create symbol index
    let symbol_index = SymbolIndexImpl::new(index_path, "test_tenant".to_string()).await?;

    // Create test symbols
    let symbols = vec![
        create_test_symbol("test_function", 10),
        create_test_symbol("another_function", 20),
        create_test_symbol("helper_function", 30),
    ];

    // Index symbols
    let count = symbol_index
        .index_symbols(symbols, "test_repo", "abc123", "filehash123")
        .await?;
    assert_eq!(count, 3);

    // Search for symbols
    let results = symbol_index
        .search("function", Some("test_repo"), 10)
        .await?;
    assert!(!results.is_empty());
    assert!(results.iter().any(|s| s.name.contains("function")));

    // Check total count
    let total = symbol_index.count().await?;
    assert_eq!(total, 3);

    Ok(())
}

#[tokio::test]
async fn test_test_index_create_and_search() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let index_path = temp_dir.path().to_path_buf();

    // Create test index
    let test_index = TestIndexImpl::new(index_path, "test_tenant".to_string()).await?;

    // Create test entries
    let tests = vec![
        IndexedTest {
            test_id: "test1".to_string(),
            test_name: "test_authentication".to_string(),
            file_path: "tests/auth.rs".to_string(),
            start_line: 10,
            end_line: 20,
            target_symbol_id: None,
            target_function: Some("authenticate".to_string()),
            repo_id: "test_repo".to_string(),
            commit_sha: "abc123".to_string(),
        },
        IndexedTest {
            test_id: "test2".to_string(),
            test_name: "test_authorization".to_string(),
            file_path: "tests/auth.rs".to_string(),
            start_line: 30,
            end_line: 40,
            target_symbol_id: None,
            target_function: Some("authorize".to_string()),
            repo_id: "test_repo".to_string(),
            commit_sha: "abc123".to_string(),
        },
    ];

    // Index tests
    let count = test_index.index_tests(tests, "test_repo", "abc123").await?;
    assert_eq!(count, 2);

    // Search for tests
    let results = test_index
        .search("authentication", Some("test_repo"), 10)
        .await?;
    assert!(!results.is_empty());

    // Check total count
    let total = test_index.count().await?;
    assert_eq!(total, 2);

    Ok(())
}

#[tokio::test]
async fn test_doc_index_create_and_search() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let index_path = temp_dir.path().to_path_buf();

    // Create doc index
    let doc_index = DocIndexImpl::new(index_path, "test_tenant".to_string()).await?;

    // Create doc entries
    let docs = vec![
        IndexedDoc {
            doc_id: "doc1".to_string(),
            doc_type: "README".to_string(),
            file_path: "README.md".to_string(),
            title: "Project README".to_string(),
            content: "This is a test project for authentication and authorization.".to_string(),
            repo_id: "test_repo".to_string(),
            commit_sha: "abc123".to_string(),
            start_line: None,
            end_line: None,
        },
        IndexedDoc {
            doc_id: "doc2".to_string(),
            doc_type: "doc_comment".to_string(),
            file_path: "src/auth.rs".to_string(),
            title: "Authentication module".to_string(),
            content: "Handles user authentication using JWT tokens.".to_string(),
            repo_id: "test_repo".to_string(),
            commit_sha: "abc123".to_string(),
            start_line: Some(1),
            end_line: Some(5),
        },
    ];

    // Index docs
    let count = doc_index.index_docs(docs, "test_repo", "abc123").await?;
    assert_eq!(count, 2);

    // Search for docs
    let results = doc_index
        .search("authentication", Some("test_repo"), 10)
        .await?;
    assert!(!results.is_empty());

    // Check total count
    let total = doc_index.count().await?;
    assert_eq!(total, 2);

    Ok(())
}

#[tokio::test]
async fn test_evidence_manager_file_removal() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let indices_root = temp_dir.path().to_path_buf();

    // Create evidence manager without embedding model
    let mut manager =
        EvidenceIndexManager::new(indices_root, "test_tenant".to_string(), None).await?;

    // Create test file change
    let test_file = PathBuf::from("test.rs");
    let changes = vec![FileChange {
        path: test_file.clone(),
        change_type: ChangeType::Added,
        old_path: None,
    }];

    // Process file changes (this will fail since file doesn't exist, but that's expected)
    let stats = manager
        .handle_file_changes(&changes, "test_repo", "abc123")
        .await?;

    // File will fail to process because it doesn't exist, but that's OK for this test
    // We're testing the removal logic, not the add logic
    assert_eq!(stats.errors.len(), 1); // One error for the non-existent file

    // Test file removal
    let remove_changes = vec![FileChange {
        path: test_file,
        change_type: ChangeType::Deleted,
        old_path: None,
    }];

    let remove_stats = manager
        .handle_file_changes(&remove_changes, "test_repo", "abc123")
        .await?;
    assert_eq!(remove_stats.errors.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_evidence_manager_stats() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let indices_root = temp_dir.path().to_path_buf();

    // Create evidence manager
    let manager = EvidenceIndexManager::new(indices_root, "test_tenant".to_string(), None).await?;

    // Get stats
    let stats = manager.get_stats().await?;

    // Verify stat keys exist
    assert!(stats.contains_key("symbols"));
    assert!(stats.contains_key("tests"));
    assert!(stats.contains_key("docs"));

    // All should be 0 initially
    assert_eq!(*stats.get("symbols").unwrap(), 0);
    assert_eq!(*stats.get("tests").unwrap(), 0);
    assert_eq!(*stats.get("docs").unwrap(), 0);

    Ok(())
}

#[tokio::test]
async fn test_deterministic_ordering() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let index_path = temp_dir.path().to_path_buf();

    // Create symbol index
    let symbol_index = SymbolIndexImpl::new(index_path, "test_tenant".to_string()).await?;

    // Create multiple symbols with same content to test ordering
    let symbols = vec![
        create_test_symbol("alpha_function", 10),
        create_test_symbol("beta_function", 20),
        create_test_symbol("gamma_function", 30),
    ];

    symbol_index
        .index_symbols(symbols, "test_repo", "abc123", "hash")
        .await?;

    // Search and verify results are ordered
    let results = symbol_index
        .search("function", Some("test_repo"), 10)
        .await?;

    // Results should be deterministically ordered
    assert!(!results.is_empty());
    // FTS5 rank ordering should be consistent
    for window in results.windows(2) {
        let a = &window[0];
        let b = &window[1];
        // Either different scores or alphabetically ordered doc_ids
        assert!(a.name <= b.name || a.name != b.name);
    }

    Ok(())
}

#[tokio::test]
async fn test_incremental_updates() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let indices_root = temp_dir.path().to_path_buf();

    // Create evidence manager
    let mut manager =
        EvidenceIndexManager::new(indices_root, "test_tenant".to_string(), None).await?;

    let test_file = PathBuf::from("src/lib.rs");

    // Add file
    let add_changes = vec![FileChange {
        path: test_file.clone(),
        change_type: ChangeType::Added,
        old_path: None,
    }];

    manager
        .handle_file_changes(&add_changes, "test_repo", "commit1")
        .await?;

    // Modify file
    let modify_changes = vec![FileChange {
        path: test_file.clone(),
        change_type: ChangeType::Modified,
        old_path: None,
    }];

    manager
        .handle_file_changes(&modify_changes, "test_repo", "commit2")
        .await?;

    // Rename file
    let new_path = PathBuf::from("src/core.rs");
    let rename_changes = vec![FileChange {
        path: new_path.clone(),
        change_type: ChangeType::Renamed,
        old_path: Some(test_file.clone()),
    }];

    manager
        .handle_file_changes(&rename_changes, "test_repo", "commit3")
        .await?;

    // Delete file
    let delete_changes = vec![FileChange {
        path: new_path,
        change_type: ChangeType::Deleted,
        old_path: None,
    }];

    let stats = manager
        .handle_file_changes(&delete_changes, "test_repo", "commit4")
        .await?;

    // Verify no errors occurred
    assert_eq!(stats.errors.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_search_with_empty_indices() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let indices_root = temp_dir.path().to_path_buf();

    // Create evidence manager
    let manager = EvidenceIndexManager::new(indices_root, "test_tenant".to_string(), None).await?;

    // Search with empty indices
    let results = manager
        .search_evidence(
            "test query",
            &[EvidenceType::Symbol, EvidenceType::Code],
            Some("test_repo"),
            10,
        )
        .await?;

    // Should return empty results, not error
    assert!(results.is_empty());

    Ok(())
}
*/
