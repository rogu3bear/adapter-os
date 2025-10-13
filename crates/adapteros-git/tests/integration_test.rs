//! Integration tests for Git subsystem using real repositories

use adapteros_db::Db;
use adapteros_git::{GitConfig, GitSubsystem};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;
use tokio::time::{sleep, Duration};

/// Test Git subsystem with real repository operations
#[tokio::test]
async fn test_git_subsystem_integration() {
    // Create temporary directory for test repository
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path().join("test_repo");

    // Initialize Git repository
    let repo = git2::Repository::init(&repo_path).expect("Failed to initialize Git repository");

    // Create initial commit
    let mut index = repo.index().expect("Failed to get index");
    let file_path = repo_path.join("README.md");
    fs::write(&file_path, "# Test Repository\n")
        .await
        .expect("Failed to write README");

    index
        .add_path(&PathBuf::from("README.md"))
        .expect("Failed to add README to index");
    index.write().expect("Failed to write index");

    let tree_id = index.write_tree().expect("Failed to write tree");
    let tree = repo.find_tree(tree_id).expect("Failed to find tree");

    let signature =
        git2::Signature::now("Test User", "test@example.com").expect("Failed to create signature");

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Initial commit",
        &tree,
        &[],
    )
    .expect("Failed to create initial commit");

    // Create Git subsystem
    let config = GitConfig::default();
    let db = Db::connect(":memory:")
        .await
        .expect("Failed to create database");

    let mut git_subsystem = GitSubsystem::new(config, db)
        .await
        .expect("Failed to create Git subsystem");

    // Register repository
    git_subsystem
        .branch_manager()
        .register_repository("test_repo".to_string(), repo_path.clone())
        .await
        .expect("Failed to register repository");

    // Start Git subsystem
    git_subsystem
        .start()
        .await
        .expect("Failed to start Git subsystem");

    // Test branch creation
    let session = git_subsystem
        .branch_manager()
        .start_session("test_adapter".to_string(), "test_repo".to_string(), None)
        .await
        .expect("Failed to start Git session");

    assert_eq!(session.adapter_id, "test_adapter");
    assert_eq!(session.repo_id, "test_repo");
    assert!(session.branch_name.contains("adapter/v1/test_adapter"));

    // Test file change detection
    let test_file = repo_path.join("src").join("main.rs");
    fs::create_dir_all(test_file.parent().unwrap())
        .await
        .expect("Failed to create directory");
    fs::write(
        &test_file,
        "fn main() {\n    println!(\"Hello, world!\");\n}\n",
    )
    .await
    .expect("Failed to write test file");

    // Wait for file change to be processed
    sleep(Duration::from_millis(1000)).await;

    // Test session end (merge)
    let merge_commit = git_subsystem
        .branch_manager()
        .end_session(&session.id, true)
        .await
        .expect("Failed to end Git session");

    assert!(merge_commit.is_some());

    // Stop Git subsystem
    git_subsystem
        .stop()
        .await
        .expect("Failed to stop Git subsystem");

    // Verify merge was successful
    let head = repo.head().expect("Failed to get HEAD");
    let commit = head.peel_to_commit().expect("Failed to get commit");
    let message = commit.message().expect("Failed to get commit message");

    assert!(message.contains("Merge branch"));
    assert!(message.contains("adapter session"));
}

/// Test error handling and recovery
#[tokio::test]
async fn test_git_error_handling() {
    let config = GitConfig::default();
    let db = Db::connect(":memory:")
        .await
        .expect("Failed to create database");

    let mut git_subsystem = GitSubsystem::new(config, db)
        .await
        .expect("Failed to create Git subsystem");

    // Test starting session with non-existent repository
    let result = git_subsystem
        .branch_manager()
        .start_session(
            "test_adapter".to_string(),
            "nonexistent_repo".to_string(),
            None,
        )
        .await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Repository nonexistent_repo not found"));

    // Test graceful shutdown
    git_subsystem
        .stop()
        .await
        .expect("Failed to stop Git subsystem");
}

/// Test commit batching and auto-commit
#[tokio::test]
async fn test_commit_batching() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path().join("test_repo");

    // Initialize Git repository
    let repo = git2::Repository::init(&repo_path).expect("Failed to initialize Git repository");

    // Create initial commit
    let mut index = repo.index().expect("Failed to get index");
    let file_path = repo_path.join("README.md");
    fs::write(&file_path, "# Test Repository\n")
        .await
        .expect("Failed to write README");

    index
        .add_path(&PathBuf::from("README.md"))
        .expect("Failed to add README to index");
    index.write().expect("Failed to write index");

    let tree_id = index.write_tree().expect("Failed to write tree");
    let tree = repo.find_tree(tree_id).expect("Failed to find tree");

    let signature =
        git2::Signature::now("Test User", "test@example.com").expect("Failed to create signature");

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Initial commit",
        &tree,
        &[],
    )
    .expect("Failed to create initial commit");

    // Create Git subsystem with short commit interval for testing
    let mut config = GitConfig::default();
    config.commit_daemon.auto_commit_interval_secs = 1; // 1 second for testing

    let db = Db::connect(":memory:")
        .await
        .expect("Failed to create database");

    let mut git_subsystem = GitSubsystem::new(config, db)
        .await
        .expect("Failed to create Git subsystem");

    // Register repository
    git_subsystem
        .branch_manager()
        .register_repository("test_repo".to_string(), repo_path.clone())
        .await
        .expect("Failed to register repository");

    // Start Git subsystem
    git_subsystem
        .start()
        .await
        .expect("Failed to start Git subsystem");

    // Start session
    let session = git_subsystem
        .branch_manager()
        .start_session("test_adapter".to_string(), "test_repo".to_string(), None)
        .await
        .expect("Failed to start Git session");

    // Create multiple files to test batching
    for i in 0..5 {
        let file_path = repo_path.join(format!("file_{}.rs", i));
        fs::write(&file_path, format!("// File {}\nfn test_{}() {{}}\n", i, i))
            .await
            .expect("Failed to write test file");
    }

    // Wait for auto-commit
    sleep(Duration::from_millis(2000)).await;

    // Verify commits were created
    let head = repo.head().expect("Failed to get HEAD");
    let commit = head.peel_to_commit().expect("Failed to get commit");
    let message = commit.message().expect("Failed to get commit message");

    assert!(message.contains("chore(adapter:test_adapter)"));
    assert!(message.contains("add 5 file(s)"));

    // End session
    git_subsystem
        .branch_manager()
        .end_session(&session.id, true)
        .await
        .expect("Failed to end Git session");

    // Stop Git subsystem
    git_subsystem
        .stop()
        .await
        .expect("Failed to stop Git subsystem");
}
