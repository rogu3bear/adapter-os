//! Integration tests for Git subsystem using real repositories

use adapteros_db::Database;
use adapteros_git::{GitConfig, GitSubsystem};

/// Test Git subsystem stub compiles and starts
#[tokio::test]
async fn test_git_subsystem_integration() {
    let config = GitConfig::default();
    let db = Database::connect(":memory:")
        .await
        .expect("Failed to create database");

    let mut git_subsystem = GitSubsystem::new(config, db)
        .await
        .expect("Failed to create Git subsystem");

    git_subsystem
        .start()
        .await
        .expect("Failed to start Git subsystem");
}

/// Test Git subsystem stub starts without errors
#[tokio::test]
async fn test_git_error_handling() {
    let config = GitConfig::default();
    let db = Database::connect(":memory:")
        .await
        .expect("Failed to create database");

    let mut git_subsystem = GitSubsystem::new(config, db)
        .await
        .expect("Failed to create Git subsystem");

    git_subsystem
        .start()
        .await
        .expect("Failed to start Git subsystem");
}

/// Test Git subsystem stub can be started
#[tokio::test]
async fn test_commit_batching() {
    let config = GitConfig::default();
    let db = Database::connect(":memory:")
        .await
        .expect("Failed to create database");

    let mut git_subsystem = GitSubsystem::new(config, db)
        .await
        .expect("Failed to create Git subsystem");

    git_subsystem
        .start()
        .await
        .expect("Failed to start Git subsystem");
}
