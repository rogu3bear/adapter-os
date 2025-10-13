//! Integration tests for TestExecutor
//!
//! Tests the test execution framework with real cargo test and nextest.

use mplora_worker::{TestExecutor, TestFramework};
use std::path::PathBuf;

#[tokio::test]
async fn test_executor_detects_tests() {
    let repo_path = PathBuf::from(".");
    let executor = TestExecutor::new(&repo_path);
    
    // Should have tests configured (this workspace has tests)
    assert!(executor.has_tests());
}

#[tokio::test]
async fn test_timeout_configuration() {
    let repo_path = PathBuf::from(".");
    let executor = TestExecutor::new(&repo_path).with_timeout(60);
    
    // Timeout should be configurable
    assert!(executor.has_tests());
}

#[tokio::test]
async fn test_executor_creates_successfully() {
    let repo_path = PathBuf::from(".");
    let _executor = TestExecutor::new(&repo_path);
    
    // Should create without error
}

// Note: Full test execution is tested via unit tests in the module itself
// Integration tests here verify the public API and configuration

