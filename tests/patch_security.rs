#![cfg(all(test, feature = "extended-tests"))]

//! Security tests for patch proposal system
//!
//! Tests security-related functionality:
//! - Secret detection and scanning
//! - Path restrictions and permissions
//! - Forbidden operations detection
//! - Dependency policy enforcement
//! - Input validation and sanitization
//!
//! Aligns with security requirements from Code Policy.

use adapteros_lora_worker::{
    patch_generator::{FilePatch, HunkType, PatchHunk},
    patch_validator::{CodePolicy, PatchValidator, ViolationSeverity, ViolationType},
};
use std::collections::HashMap;
use tokio;

/// Test secret detection with various patterns
#[tokio::test]
async fn test_secret_detection_patterns() {
    let policy = CodePolicy::default();
    let validator = PatchValidator::new(policy);

    // Test API key pattern
    let api_key_patch = FilePatch {
        file_path: "src/config.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["api_key = \"sk-1234567890abcdef\"".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[api_key_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::SecretDetected)));

    // Test password pattern
    let password_patch = FilePatch {
        file_path: "src/auth.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["password: \"super_secret_password123\"".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[password_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::SecretDetected)));

    // Test AWS credentials pattern
    let aws_patch = FilePatch {
        file_path: "src/aws.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["aws_access_key: \"AKIAIOSFODNN7EXAMPLE\"".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[aws_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::SecretDetected)));

    // Test private key pattern
    let private_key_patch = FilePatch {
        file_path: "src/crypto.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["-----BEGIN RSA PRIVATE KEY-----".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[private_key_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::SecretDetected)));
}

/// Test path restrictions and permissions
#[tokio::test]
async fn test_path_restrictions() {
    let mut policy = CodePolicy::default();
    policy.path_allowlist = vec!["src/**".to_string(), "tests/**".to_string()];
    policy.path_denylist = vec!["**/.env*".to_string(), "**/secrets/**".to_string()];

    let validator = PatchValidator::new(policy);

    // Test allowed path
    let allowed_patch = FilePatch {
        file_path: "src/main.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["fn main() {}".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[allowed_patch]).await.unwrap();
    assert!(result.is_valid);

    // Test denied path (denylist takes priority)
    let denied_patch = FilePatch {
        file_path: ".env".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["SECRET=value".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[denied_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::PathDenied)));

    // Test path not in allowlist
    let not_allowed_patch = FilePatch {
        file_path: "config/production.yml".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["database_url: postgres://...".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[not_allowed_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::PathDenied)));
}

/// Test forbidden operations detection
#[tokio::test]
async fn test_forbidden_operations() {
    let policy = CodePolicy::default();
    let validator = PatchValidator::new(policy);

    // Test eval operation
    let eval_patch = FilePatch {
        file_path: "src/script.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["eval(user_input)".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[eval_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::ForbiddenOperation)));

    // Test exec_raw operation
    let exec_patch = FilePatch {
        file_path: "src/system.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["exec_raw(\"rm -rf /\")".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[exec_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::ForbiddenOperation)));

    // Test shell_escape operation
    let shell_patch = FilePatch {
        file_path: "src/command.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["shell_escape(user_data)".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[shell_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::ForbiddenOperation)));

    // Test unsafe_deserialization operation
    let unsafe_patch = FilePatch {
        file_path: "src/serialize.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["unsafe_deserialization(data)".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[unsafe_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::ForbiddenOperation)));
}

/// Test dependency policy enforcement
#[tokio::test]
async fn test_dependency_policy() {
    let mut policy = CodePolicy::default();
    policy.allow_external_deps = false;

    let validator = PatchValidator::new(policy);

    // Test external Rust dependency
    let rust_dep_patch = FilePatch {
        file_path: "src/main.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["use external_crate::function;".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[rust_dep_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::DependencyBlocked)));

    // Test external Python dependency
    let python_dep_patch = FilePatch {
        file_path: "src/script.py".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["from external_package import function".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[python_dep_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::DependencyBlocked)));

    // Test allowed dependency (when policy allows)
    let mut allowed_policy = CodePolicy::default();
    allowed_policy.allow_external_deps = true;
    let allowed_validator = PatchValidator::new(allowed_policy);

    let result = allowed_validator.validate(&[rust_dep_patch]).await.unwrap();
    assert!(result.is_valid);
}

/// Test review requirements for sensitive changes
#[tokio::test]
async fn test_review_requirements() {
    let policy = CodePolicy::default();
    let validator = PatchValidator::new(policy);

    // Test database migration
    let migration_patch = FilePatch {
        file_path: "migrations/0001_add_users.sql".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["CREATE TABLE users (id SERIAL PRIMARY KEY);".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[migration_patch]).await.unwrap();
    assert!(result.is_valid); // Review required is a warning, not an error
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::ReviewRequired)));

    // Test security-related file
    let security_patch = FilePatch {
        file_path: "src/auth.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["fn authenticate_user() {}".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[security_patch]).await.unwrap();
    assert!(result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::ReviewRequired)));

    // Test production config
    let config_patch = FilePatch {
        file_path: "config/production.toml".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["database_url = \"postgres://...\"".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[config_patch]).await.unwrap();
    assert!(result.is_valid);
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::ReviewRequired)));
}

/// Test patch size limits
#[tokio::test]
async fn test_patch_size_limits() {
    let mut policy = CodePolicy::default();
    policy.max_patch_size_lines = 10;

    let validator = PatchValidator::new(policy);

    // Test patch within size limit
    let small_patch = FilePatch {
        file_path: "src/small.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["fn small() {}".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[small_patch]).await.unwrap();
    assert!(result.is_valid);
    assert!(result.warnings.is_empty());

    // Test patch exceeding size limit
    let large_patch = FilePatch {
        file_path: "src/large.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 20,
            context_lines: vec![],
            modified_lines: vec!["fn large() {}".to_string(); 15],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 15,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[large_patch]).await.unwrap();
    assert!(result.is_valid); // Size limit is a warning, not an error
    assert!(!result.warnings.is_empty());
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::SizeExceeded)));
}

/// Test multiple security violations in single patch
#[tokio::test]
async fn test_multiple_violations() {
    let policy = CodePolicy::default();
    let validator = PatchValidator::new(policy);

    let violation_patch = FilePatch {
        file_path: ".env".to_string(), // Path denied
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec![
                "api_key = \"sk-1234567890abcdef\"".to_string(), // Secret detected
                "eval(user_input)".to_string(),                  // Forbidden operation
                "use external_crate::function;".to_string(),     // Dependency blocked
            ],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 3,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[violation_patch]).await.unwrap();
    assert!(!result.is_valid);
    assert!(result.errors.len() >= 3); // Multiple errors
    assert!(result.violations.len() >= 3); // Multiple violations

    // Check for specific violation types
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::PathDenied)));
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::SecretDetected)));
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::ForbiddenOperation)));
    assert!(result
        .violations
        .iter()
        .any(|v| matches!(v.violation_type, ViolationType::DependencyBlocked)));
}

/// Test violation severity levels
#[tokio::test]
async fn test_violation_severity() {
    let policy = CodePolicy::default();
    let validator = PatchValidator::new(policy);

    // Test critical severity (secrets)
    let secret_patch = FilePatch {
        file_path: "src/config.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["api_key = \"sk-1234567890abcdef\"".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[secret_patch]).await.unwrap();
    let secret_violation = result
        .violations
        .iter()
        .find(|v| matches!(v.violation_type, ViolationType::SecretDetected))
        .unwrap();
    assert!(matches!(
        secret_violation.severity,
        ViolationSeverity::Critical
    ));

    // Test high severity (forbidden operations)
    let forbidden_patch = FilePatch {
        file_path: "src/script.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 5,
            context_lines: vec![],
            modified_lines: vec!["eval(user_input)".to_string()],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 1,
        metadata: HashMap::new(),
    };

    let result = validator.validate(&[forbidden_patch]).await.unwrap();
    let forbidden_violation = result
        .violations
        .iter()
        .find(|v| matches!(v.violation_type, ViolationType::ForbiddenOperation))
        .unwrap();
    assert!(matches!(
        forbidden_violation.severity,
        ViolationSeverity::Critical
    ));

    // Test medium severity (size limits)
    let mut size_policy = CodePolicy::default();
    size_policy.max_patch_size_lines = 1;
    let size_validator = PatchValidator::new(size_policy);

    let large_patch = FilePatch {
        file_path: "src/large.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 10,
            context_lines: vec![],
            modified_lines: vec!["fn large() {}".to_string(); 5],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 5,
        metadata: HashMap::new(),
    };

    let result = size_validator.validate(&[large_patch]).await.unwrap();
    let size_violation = result
        .violations
        .iter()
        .find(|v| matches!(v.violation_type, ViolationType::SizeExceeded))
        .unwrap();
    assert!(matches!(size_violation.severity, ViolationSeverity::Medium));
}
