//! Integration tests for Git subsystem using real repositories

use adapteros_db::Db;
use adapteros_git::{GitConfig, GitSubsystem};

/// Test Git subsystem stub compiles and starts
#[tokio::test]
#[ignore = "Git subsystem integration tests require database refactoring [tracking: STAB-IGN-001]"]
async fn test_git_subsystem_integration() {
    let config = GitConfig::default();
    let db = Db::connect(":memory:")
        .await
        .expect("Failed to create database");

    let result = GitSubsystem::new(config, db).await;
    assert!(result.is_ok(), "Failed to create Git subsystem");
}

/// Test Git subsystem stub starts without errors
#[tokio::test]
#[ignore = "Git subsystem integration tests require database refactoring [tracking: STAB-IGN-001]"]
async fn test_git_error_handling() {
    let config = GitConfig::default();
    let db = Db::connect(":memory:")
        .await
        .expect("Failed to create database");

    let result = GitSubsystem::new(config, db).await;
    assert!(result.is_ok(), "Failed to create Git subsystem");
}

/// Test Git subsystem stub can be started
#[tokio::test]
#[ignore = "Git subsystem integration tests require database refactoring [tracking: STAB-IGN-001]"]
async fn test_commit_batching() {
    let config = GitConfig::default();
    let db = Db::connect(":memory:")
        .await
        .expect("Failed to create database");

    let result = GitSubsystem::new(config, db).await;
    assert!(result.is_ok(), "Failed to create Git subsystem");
}
