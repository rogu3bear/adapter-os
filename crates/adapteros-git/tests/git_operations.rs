//! Tests for Git subsystem operations
//!
//! These tests verify the core Git operations: list_commits, get_commit,
//! get_commit_diff, and get_status.

use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_git::{GitConfig, GitSubsystem};
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Helper to create a test Git repository with commits
async fn create_test_repo() -> Result<(TempDir, String)> {
    let root = PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root)
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to create temp root: {}", e)))?;
    let temp_dir = TempDir::new_in(&root)
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let repo_path = temp_dir.path();

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| adapteros_core::AosError::Git(format!("Failed to init repo: {}", e)))?;

    // Configure git user
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| adapteros_core::AosError::Git(format!("Failed to config user: {}", e)))?;

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| adapteros_core::AosError::Git(format!("Failed to config email: {}", e)))?;

    // Create first commit
    std::fs::write(repo_path.join("README.md"), "# Test Repository\n")
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to write README: {}", e)))?;

    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| adapteros_core::AosError::Git(format!("Failed to add file: {}", e)))?;

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| adapteros_core::AosError::Git(format!("Failed to commit: {}", e)))?;

    // Create second commit
    std::fs::write(repo_path.join("src.rs"), "fn main() {}\n")
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to write src: {}", e)))?;

    Command::new("git")
        .args(["add", "src.rs"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| adapteros_core::AosError::Git(format!("Failed to add src: {}", e)))?;

    Command::new("git")
        .args(["commit", "-m", "Add source file"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| adapteros_core::AosError::Git(format!("Failed to commit src: {}", e)))?;

    // Get the latest commit SHA
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| adapteros_core::AosError::Git(format!("Failed to get HEAD: {}", e)))?;

    let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok((temp_dir, commit_sha))
}

/// Helper to create a database with a registered repository
async fn setup_db_with_repo(repo_path: &str) -> Result<Db> {
    let db = Db::new_in_memory().await?;

    // Run migrations
    db.migrate().await?;

    // Register the repository
    db.create_git_repository(
        "test-repo-id",
        "test-repo",
        repo_path,
        "main",
        "{}",
        "test-user",
    )
    .await?;

    Ok(db)
}

#[tokio::test]
async fn test_list_commits() -> Result<()> {
    let (temp_dir, _commit_sha) = create_test_repo().await?;
    let repo_path = temp_dir.path().to_str().unwrap();
    let db = setup_db_with_repo(repo_path).await?;

    let config = GitConfig { enabled: true };
    let subsystem = GitSubsystem::new(config, db).await?;

    // List commits with default limit
    let commits = subsystem.list_commits(Some("test-repo"), None, 10).await?;

    // Should have 2 commits
    assert_eq!(commits.len(), 2, "Should have 2 commits");

    // Check first commit (most recent)
    assert_eq!(commits[0].message, "Add source file");
    assert_eq!(commits[0].author, "Test User");
    assert_eq!(commits[0].repo_id, "test-repo");
    assert!(commits[0].changed_files.contains(&"src.rs".to_string()));

    // Check second commit
    assert_eq!(commits[1].message, "Initial commit");
    assert!(commits[1].changed_files.contains(&"README.md".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_get_commit() -> Result<()> {
    let (temp_dir, commit_sha) = create_test_repo().await?;
    let repo_path = temp_dir.path().to_str().unwrap();
    let db = setup_db_with_repo(repo_path).await?;

    let config = GitConfig { enabled: true };
    let subsystem = GitSubsystem::new(config, db).await?;

    // Get specific commit
    let commit = subsystem.get_commit(Some("test-repo"), &commit_sha).await?;

    // Verify commit details
    assert_eq!(commit.sha, commit_sha);
    assert_eq!(commit.message, "Add source file");
    assert_eq!(commit.author, "Test User");
    assert_eq!(commit.repo_id, "test-repo");
    assert_eq!(commit.changed_files.len(), 1);
    assert!(commit.changed_files.contains(&"src.rs".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_get_commit_not_found() -> Result<()> {
    let (temp_dir, _commit_sha) = create_test_repo().await?;
    let repo_path = temp_dir.path().to_str().unwrap();
    let db = setup_db_with_repo(repo_path).await?;

    let config = GitConfig { enabled: true };
    let subsystem = GitSubsystem::new(config, db).await?;

    // Try to get non-existent commit
    let invalid_sha = "0000000000000000000000000000000000000000";
    let result = subsystem.get_commit(Some("test-repo"), invalid_sha).await;

    assert!(result.is_err(), "Should fail for invalid commit");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found") || err_msg.contains("invalid"),
        "Error should mention not found or invalid: {}",
        err_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_get_commit_diff() -> Result<()> {
    let (temp_dir, commit_sha) = create_test_repo().await?;
    let repo_path = temp_dir.path().to_str().unwrap();
    let db = setup_db_with_repo(repo_path).await?;

    let config = GitConfig { enabled: true };
    let subsystem = GitSubsystem::new(config, db).await?;

    // Get commit diff
    let diff = subsystem
        .get_commit_diff(Some("test-repo"), &commit_sha)
        .await?;

    // Verify diff details
    assert_eq!(diff.sha, commit_sha);
    assert_eq!(diff.files_changed, 1, "Should have 1 file changed");
    assert!(diff.insertions > 0, "Should have insertions");
    assert_eq!(diff.deletions, 0, "Should have no deletions");

    // Check diff content
    assert!(diff.diff.contains("src.rs"), "Diff should mention src.rs");
    assert!(diff.diff.contains("+"), "Diff should contain additions");

    Ok(())
}

#[tokio::test]
async fn test_get_status() -> Result<()> {
    let (temp_dir, _commit_sha) = create_test_repo().await?;
    let repo_path = temp_dir.path().to_str().unwrap();
    let db = setup_db_with_repo(repo_path).await?;

    let config = GitConfig { enabled: true };
    let subsystem = GitSubsystem::new(config, db).await?;

    // Get status
    let status = subsystem.get_status().await?;

    // Verify status
    assert!(status.enabled, "Git subsystem should be enabled");
    assert_eq!(status.repositories_tracked, 1, "Should track 1 repository");
    assert_eq!(status.active_sessions, 0, "Should have no active sessions");

    Ok(())
}

#[tokio::test]
async fn test_list_commits_with_limit() -> Result<()> {
    let (temp_dir, _commit_sha) = create_test_repo().await?;
    let repo_path = temp_dir.path().to_str().unwrap();
    let db = setup_db_with_repo(repo_path).await?;

    let config = GitConfig { enabled: true };
    let subsystem = GitSubsystem::new(config, db).await?;

    // List commits with limit of 1
    let commits = subsystem.list_commits(Some("test-repo"), None, 1).await?;

    // Should only have 1 commit
    assert_eq!(commits.len(), 1, "Should have only 1 commit");
    assert_eq!(commits[0].message, "Add source file");

    Ok(())
}

#[tokio::test]
async fn test_list_commits_default_repo() -> Result<()> {
    let (temp_dir, _commit_sha) = create_test_repo().await?;
    let repo_path = temp_dir.path().to_str().unwrap();
    let db = setup_db_with_repo(repo_path).await?;

    let config = GitConfig { enabled: true };
    let subsystem = GitSubsystem::new(config, db).await?;

    // List commits without specifying repo_id (should use first registered repo)
    let commits = subsystem.list_commits(None, None, 10).await?;

    assert_eq!(commits.len(), 2, "Should have 2 commits");

    Ok(())
}
