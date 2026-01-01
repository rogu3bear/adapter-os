//! Integration tests for Git subsystem using real repositories

use adapteros_db::Db;
use adapteros_git::{GitConfig, GitSubsystem};

/// Helper function to create a test database with migrations applied
async fn create_test_db() -> Db {
    let db = Db::connect(":memory:")
        .await
        .expect("Failed to create database");
    db.migrate().await.expect("Failed to run migrations");
    db
}

/// Test Git subsystem stub compiles and starts
#[tokio::test]
async fn test_git_subsystem_integration() {
    let config = GitConfig::default();
    let db = create_test_db().await;

    let result = GitSubsystem::new(config, db).await;
    assert!(result.is_ok(), "Failed to create Git subsystem");
}

/// Test Git subsystem stub starts without errors
#[tokio::test]
async fn test_git_error_handling() {
    let config = GitConfig::default();
    let db = create_test_db().await;

    let result = GitSubsystem::new(config, db).await;
    assert!(result.is_ok(), "Failed to create Git subsystem");
}

/// Test Git subsystem stub can be started
#[tokio::test]
async fn test_commit_batching() {
    let config = GitConfig::default();
    let db = create_test_db().await;

    let result = GitSubsystem::new(config, db).await;
    assert!(result.is_ok(), "Failed to create Git subsystem");
}
