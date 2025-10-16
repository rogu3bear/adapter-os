//! Integration tests for LinterRunner
//!
//! Tests the linter integration framework with clippy and rustfmt.

use adapteros_lora_worker::{LinterConfig, LinterRunner, LinterType};
use std::path::PathBuf;

#[tokio::test]
async fn test_linter_runner_creates_successfully() {
    let repo_path = PathBuf::from(".");
    let _runner = LinterRunner::new(&repo_path);

    // Should create without error
}

#[tokio::test]
async fn test_linter_with_custom_config() {
    let repo_path = PathBuf::from(".");
    let config = LinterConfig {
        run_clippy: true,
        run_rustfmt: false,
        clippy_args: vec!["-W".to_string(), "clippy::all".to_string()],
        fail_on_warnings: true,
    };

    let _runner = LinterRunner::new(&repo_path).with_config(config);

    // Should create with custom config
}

#[tokio::test]
async fn test_linter_result_methods() {
    use adapteros_lora_worker::{LintIssue, LintSeverity, LinterResult};

    let result = LinterResult {
        linter: LinterType::Clippy,
        errors: vec![],
        warnings: vec![],
        duration_ms: 1000,
    };

    assert!(result.passed());
    assert_eq!(result.total_issues(), 0);
}

#[tokio::test]
async fn test_linter_result_with_issues() {
    use adapteros_lora_worker::{LintIssue, LintSeverity, LinterResult};

    let result = LinterResult {
        linter: LinterType::Clippy,
        errors: vec![LintIssue {
            file_path: "test.rs".to_string(),
            line: 1,
            column: Some(1),
            severity: LintSeverity::Error,
            message: "test error".to_string(),
            code: Some("E0001".to_string()),
        }],
        warnings: vec![],
        duration_ms: 1000,
    };

    assert!(!result.passed());
    assert_eq!(result.total_issues(), 1);
}

#[tokio::test]
async fn test_static_helper_methods() {
    use adapteros_lora_worker::{LintIssue, LintSeverity, LinterResult, LinterRunner};

    let results = vec![
        LinterResult {
            linter: LinterType::Clippy,
            errors: vec![LintIssue {
                file_path: "test.rs".to_string(),
                line: 1,
                column: Some(1),
                severity: LintSeverity::Error,
                message: "error".to_string(),
                code: None,
            }],
            warnings: vec![],
            duration_ms: 1000,
        },
        LinterResult {
            linter: LinterType::Rustfmt,
            errors: vec![],
            warnings: vec![],
            duration_ms: 500,
        },
    ];

    assert_eq!(LinterRunner::total_errors(&results), 1);
    assert_eq!(LinterRunner::total_warnings(&results), 0);
    assert!(!LinterRunner::all_passed(&results));
}

// Note: Full linter execution is tested via unit tests in the module itself
// Integration tests here verify the public API and configuration
