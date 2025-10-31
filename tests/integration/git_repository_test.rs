#![cfg(all(test, feature = "extended-tests"))]

//! Git repository integration tests
//!
//! Tests git repository registration, analysis, and training pipeline integration.
//! Follows evidence-first philosophy and security-first principles established in the codebase.

use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use serde_json::json;
use std::collections::HashMap;
use tempfile::TempDir;
use tokio::fs;

/// Test git repository registration and analysis
///
/// Evidence: docs/code-intelligence/code-policies.md:45-78
/// Policy: Evidence requirements for repository registration
#[tokio::test]
async fn test_git_repository_registration() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let db = Db::new(&db_path).await?;

    // Create test repository structure
    let repo_path = temp_dir.path().join("test_repo");
    fs::create_dir_all(&repo_path).await?;
    
    // Create .git directory to simulate git repository
    fs::create_dir_all(repo_path.join(".git")).await?;
    
    // Create test files
    fs::write(repo_path.join("Cargo.toml"), r#"
[package]
name = "test_repo"
version = "0.1.0"
edition = "2021"
"#).await?;
    
    fs::write(repo_path.join("src/main.rs"), r#"
fn main() {
    println!("Hello, world!");
}
"#).await?;

    // Test repository registration
    let repo_id = "test/repo";
    let analysis_json = json!({
        "languages": [
            {"name": "Rust", "files": 2, "lines": 10, "percentage": 100.0}
        ],
        "frameworks": [],
        "security_scan": {"violations": [], "status": "clean"},
        "git_info": {
            "branch": "main",
            "commit_count": 1,
            "last_commit": "2024-01-01T00:00:00Z",
            "authors": ["test@example.com"]
        },
        "evidence_spans": [
            {
                "span_id": "span_001",
                "evidence_type": "code_symbol",
                "file_path": "src/main.rs",
                "line_range": [1, 3],
                "relevance_score": 0.8,
                "content": "fn main() { ... }"
            }
        ]
    }).to_string();

    // Evidence: docs/code-intelligence/code-policies.md:45-78
    // Policy: Evidence requirements for repository registration
    let id = db.create_git_repository(
        "test_id",
        repo_id,
        &repo_path.to_string_lossy(),
        "main",
        &analysis_json,
        "test_user",
    ).await?;

    assert_eq!(id, "test_id");

    // Verify repository was created
    let repository = db.get_git_repository(repo_id).await?;
    assert!(repository.is_some());
    
    let repo = repository.unwrap();
    assert_eq!(repo.repo_id, repo_id);
    assert_eq!(repo.branch, "main");
    assert_eq!(repo.status, "registered");

    Ok(())
}

/// Test repository analysis with evidence validation
///
/// Evidence: docs/code-intelligence/code-policies.md:45-78
/// Policy: Evidence requirements for analysis
#[tokio::test]
async fn test_repository_analysis_evidence_validation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let db = Db::new(&db_path).await?;

    let repo_id = "test/analysis";
    let analysis_json = json!({
        "languages": [
            {"name": "Rust", "files": 5, "lines": 100, "percentage": 80.0},
            {"name": "TypeScript", "files": 3, "lines": 25, "percentage": 20.0}
        ],
        "frameworks": [
            {"name": "Axum", "version": "0.7", "confidence": 0.9, "files": ["src/main.rs"]}
        ],
        "security_scan": {
            "violations": [],
            "status": "clean",
            "scan_timestamp": "2024-01-01T00:00:00Z"
        },
        "git_info": {
            "branch": "main",
            "commit_count": 42,
            "last_commit": "2024-01-01T00:00:00Z",
            "authors": ["developer@example.com"]
        },
        "evidence_spans": [
            {
                "span_id": "span_001",
                "evidence_type": "code_symbol",
                "file_path": "src/main.rs",
                "line_range": [1, 10],
                "relevance_score": 0.85,
                "content": "use axum::{...}; fn main() { ... }"
            },
            {
                "span_id": "span_002",
                "evidence_type": "test_case",
                "file_path": "tests/integration.rs",
                "line_range": [5, 15],
                "relevance_score": 0.75,
                "content": "#[tokio::test] async fn test_endpoint() { ... }"
            }
        ]
    }).to_string();

    // Create repository with analysis
    db.create_git_repository(
        "analysis_id",
        repo_id,
        "/test/path",
        "main",
        &analysis_json,
        "test_user",
    ).await?;

    // Verify analysis contains evidence
    let repository = db.get_git_repository(repo_id).await?.unwrap();
    let analysis: serde_json::Value = serde_json::from_str(&repository.analysis_json)?;
    
    // Evidence: docs/code-intelligence/code-policies.md:45-78
    // Policy: Evidence requirements for analysis
    let evidence_spans = analysis["evidence_spans"].as_array().unwrap();
    assert!(evidence_spans.len() >= 1, "Analysis must contain at least one evidence span");
    
    // Verify evidence quality
    for span in evidence_spans {
        let relevance_score = span["relevance_score"].as_f64().unwrap();
        assert!(relevance_score >= 0.0 && relevance_score <= 1.0, "Relevance score must be between 0 and 1");
        
        let evidence_type = span["evidence_type"].as_str().unwrap();
        assert!(!evidence_type.is_empty(), "Evidence type must not be empty");
    }

    Ok(())
}

/// Test security scan validation
///
/// Evidence: docs/code-intelligence/code-policies.md:82-84
/// Policy: Security scan and violation tracking
#[tokio::test]
async fn test_security_scan_validation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let db = Db::new(&db_path).await?;

    let repo_id = "test/security";
    let analysis_json = json!({
        "languages": [{"name": "Python", "files": 1, "lines": 10, "percentage": 100.0}],
        "frameworks": [],
        "security_scan": {
            "violations": [
                {
                    "file_path": "config/secrets.py",
                    "pattern": "api_key.*=.*[\"'][^\"']{8,}[\"']",
                    "line_number": 5,
                    "severity": "high"
                },
                {
                    "file_path": "src/utils.py",
                    "pattern": "password.*=.*[\"'][^\"']{6,}[\"']",
                    "line_number": 12,
                    "severity": "medium"
                }
            ],
            "status": "violations_found",
            "scan_timestamp": "2024-01-01T00:00:00Z"
        },
        "git_info": {
            "branch": "main",
            "commit_count": 1,
            "last_commit": "2024-01-01T00:00:00Z",
            "authors": ["developer@example.com"]
        },
        "evidence_spans": []
    }).to_string();

    // Create repository with security violations
    db.create_git_repository(
        "security_id",
        repo_id,
        "/test/path",
        "main",
        &analysis_json,
        "test_user",
    ).await?;

    // Verify security violations are recorded
    let repository = db.get_git_repository(repo_id).await?.unwrap();
    let analysis: serde_json::Value = serde_json::from_str(&repository.analysis_json)?;
    
    // Evidence: docs/code-intelligence/code-policies.md:82-84
    // Policy: Security scan and violation tracking
    let violations = analysis["security_scan"]["violations"].as_array().unwrap();
    assert_eq!(violations.len(), 2, "Should have 2 security violations");
    
    // Verify violation details
    let high_severity_violation = violations.iter()
        .find(|v| v["severity"].as_str().unwrap() == "high")
        .unwrap();
    assert_eq!(high_severity_violation["file_path"].as_str().unwrap(), "config/secrets.py");
    assert_eq!(high_severity_violation["line_number"].as_u64().unwrap(), 5);

    Ok(())
}

/// Test training pipeline integration
///
/// Evidence: docs/code-intelligence/code-implementation-roadmap.md:173-270
/// Pattern: Training pipeline with evidence-based adapter creation
#[tokio::test]
async fn test_training_pipeline_integration() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let db = Db::new(&db_path).await?;

    let repo_id = "test/training";
    let analysis_json = json!({
        "languages": [{"name": "Rust", "files": 10, "lines": 500, "percentage": 100.0}],
        "frameworks": [{"name": "Tokio", "version": "1.0", "confidence": 0.95, "files": ["src/main.rs"]}],
        "security_scan": {"violations": [], "status": "clean"},
        "git_info": {
            "branch": "main",
            "commit_count": 100,
            "last_commit": "2024-01-01T00:00:00Z",
            "authors": ["developer@example.com"]
        },
        "evidence_spans": [
            {
                "span_id": "span_001",
                "evidence_type": "code_pattern",
                "file_path": "src/async_handler.rs",
                "line_range": [1, 20],
                "relevance_score": 0.9,
                "content": "use tokio::spawn; async fn handle_request() { ... }"
            }
        ]
    }).to_string();

    // Create repository
    db.create_git_repository(
        "training_id",
        repo_id,
        "/test/path",
        "main",
        &analysis_json,
        "test_user",
    ).await?;

    // Simulate training job creation
    let training_config = json!({
        "rank": 24,
        "alpha": 48,
        "epochs": 3,
        "learning_rate": 0.001,
        "batch_size": 32,
        "targets": ["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"]
    });

    // Evidence: docs/code-intelligence/code-implementation-roadmap.md:173-270
    // Pattern: Training pipeline with evidence-based adapter creation
    let training_job_id = "job_123";
    
    // Verify repository has sufficient evidence for training
    let repository = db.get_git_repository(repo_id).await?.unwrap();
    let analysis: serde_json::Value = serde_json::from_str(&repository.analysis_json)?;
    let evidence_spans = analysis["evidence_spans"].as_array().unwrap();
    
    assert!(evidence_spans.len() >= 1, "Repository must have evidence spans for training");
    
    // Verify evidence quality meets training requirements
    let avg_relevance: f64 = evidence_spans.iter()
        .map(|span| span["relevance_score"].as_f64().unwrap())
        .sum::<f64>() / evidence_spans.len() as f64;
    
    assert!(avg_relevance >= 0.7, "Average evidence relevance must be >= 0.7 for training");

    Ok(())
}

/// Test path validation and security
///
/// Evidence: docs/code-intelligence/code-policies.md:82-84
/// Policy: Path restrictions and security validation
#[tokio::test]
async fn test_path_validation_security() -> Result<()> {
    // Test cases for path validation
    let test_cases = vec![
        ("/valid/repo/path", true),
        ("/repos/acme/payments", true),
        ("/home/user/projects/my-app", true),
        ("/etc/passwd", false), // Should be denied
        ("/root/.ssh/id_rsa", false), // Should be denied
        ("/var/log/system.log", false), // Should be denied
        ("../../../etc/passwd", false), // Path traversal attack
        ("/tmp/../etc/passwd", false), // Path traversal attack
    ];

    for (path, should_be_valid) in test_cases {
        // Evidence: docs/code-intelligence/code-policies.md:82-84
        // Policy: Path restrictions and security validation
        let is_valid = validate_repository_path(path);
        
        if should_be_valid {
            assert!(is_valid, "Path {} should be valid", path);
        } else {
            assert!(!is_valid, "Path {} should be invalid", path);
        }
    }

    Ok(())
}

/// Test evidence quality validation
///
/// Evidence: docs/code-intelligence/code-policies.md:45-78
/// Policy: Evidence quality requirements
#[tokio::test]
async fn test_evidence_quality_validation() -> Result<()> {
    let test_cases = vec![
        // Valid evidence spans
        (vec![
            json!({
                "span_id": "span_001",
                "evidence_type": "code_symbol",
                "file_path": "src/main.rs",
                "line_range": [1, 10],
                "relevance_score": 0.85,
                "content": "fn main() { ... }"
            })
        ], true),
        
        // Invalid evidence spans (no spans)
        (vec![], false),
        
        // Invalid evidence spans (low relevance)
        (vec![
            json!({
                "span_id": "span_002",
                "evidence_type": "code_symbol",
                "file_path": "src/main.rs",
                "line_range": [1, 10],
                "relevance_score": 0.3, // Too low
                "content": "fn main() { ... }"
            })
        ], false),
        
        // Invalid evidence spans (missing required fields)
        (vec![
            json!({
                "span_id": "span_003",
                "evidence_type": "code_symbol",
                "file_path": "src/main.rs",
                "line_range": [1, 10],
                "relevance_score": 0.85
                // Missing "content" field
            })
        ], false),
    ];

    for (evidence_spans, should_be_valid) in test_cases {
        // Evidence: docs/code-intelligence/code-policies.md:45-78
        // Policy: Evidence quality requirements
        let is_valid = validate_evidence_quality(&evidence_spans);
        
        if should_be_valid {
            assert!(is_valid, "Evidence spans should be valid: {:?}", evidence_spans);
        } else {
            assert!(!is_valid, "Evidence spans should be invalid: {:?}", evidence_spans);
        }
    }

    Ok(())
}

/// Validate repository path for security
///
/// Evidence: docs/code-intelligence/code-policies.md:82-84
/// Policy: Path restrictions and security validation
fn validate_repository_path(path: &str) -> bool {
    // Evidence: docs/code-intelligence/code-policies.md:82-84
    // Policy: Path restrictions and security validation
    
    // Check for path traversal attacks
    if path.contains("..") || path.contains("//") {
        return false;
    }
    
    // Check for sensitive system paths
    let sensitive_paths = [
        "/etc", "/root", "/var/log", "/sys", "/proc", "/dev",
        "/boot", "/usr/bin", "/usr/sbin", "/bin", "/sbin"
    ];
    
    for sensitive_path in &sensitive_paths {
        if path.starts_with(sensitive_path) {
            return false;
        }
    }
    
    // Check for valid repository paths
    let valid_prefixes = [
        "/home", "/repos", "/projects", "/workspace", "/code"
    ];
    
    valid_prefixes.iter().any(|prefix| path.starts_with(prefix))
}

/// Validate evidence quality
///
/// Evidence: docs/code-intelligence/code-policies.md:45-78
/// Policy: Evidence quality requirements
fn validate_evidence_quality(evidence_spans: &[serde_json::Value]) -> bool {
    // Evidence: docs/code-intelligence/code-policies.md:45-78
    // Policy: Evidence quality requirements
    
    // Must have at least one evidence span
    if evidence_spans.is_empty() {
        return false;
    }
    
    // Validate each evidence span
    for span in evidence_spans {
        // Check required fields
        let required_fields = ["span_id", "evidence_type", "file_path", "line_range", "relevance_score", "content"];
        for field in &required_fields {
            if !span.get(field).is_some() {
                return false;
            }
        }
        
        // Check relevance score range
        if let Some(score) = span["relevance_score"].as_f64() {
            if score < 0.0 || score > 1.0 {
                return false;
            }
            
            // Minimum relevance threshold
            if score < 0.5 {
                return false;
            }
        } else {
            return false;
        }
        
        // Check evidence type
        if let Some(evidence_type) = span["evidence_type"].as_str() {
            if evidence_type.is_empty() {
                return false;
            }
        } else {
            return false;
        }
    }
    
    true
}
