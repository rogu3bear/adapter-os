//! Input validation utilities for API handlers
//!
//! Provides comprehensive validation for user inputs following security best practices.

use crate::types::ErrorResponse;
use axum::{http::StatusCode, Json};
use regex::Regex;
use std::path::Path;

/// Result type for validation
pub type ValidationResult<T> = Result<T, (StatusCode, Json<ErrorResponse>)>;

/// Validate repository ID format (owner/repo)
pub fn validate_repo_id(repo_id: &str) -> ValidationResult<()> {
    let repo_id_regex = Regex::new(r"^[a-zA-Z0-9_-]+/[a-zA-Z0-9_-]+$").expect("Invalid regex");

    if !repo_id_regex.is_match(repo_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid repository ID format").with_code("INTERNAL_ERROR").with_string_details("Must be in format 'owner/repo' with alphanumeric characters, underscores, and hyphens only")),
        ));
    }

    if repo_id.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Repository ID too long")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("Maximum length is 100 characters"),
            ),
        ));
    }

    Ok(())
}

/// Validate file path exists and is a git repository
pub fn validate_git_repository(path: &str) -> ValidationResult<()> {
    let repo_path = Path::new(path);

    if !repo_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Repository path does not exist")
                    .with_code("BAD_REQUEST")
                    .with_string_details(path.to_string()),
            ),
        ));
    }

    if !repo_path.is_dir() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Repository path is not a directory")
                    .with_code("BAD_REQUEST")
                    .with_string_details(path.to_string()),
            ),
        ));
    }

    if !repo_path.join(".git").exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Path is not a git repository")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("Missing .git directory"),
            ),
        ));
    }

    Ok(())
}

/// Validate language support
pub fn validate_languages(languages: &[String]) -> ValidationResult<()> {
    const SUPPORTED_LANGUAGES: &[&str] = &[
        "python",
        "rust",
        "typescript",
        "javascript",
        "go",
        "java",
        "c",
        "cpp",
        "csharp",
    ];

    if languages.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("No languages specified")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("At least one language must be specified"),
            ),
        ));
    }

    for lang in languages {
        if !SUPPORTED_LANGUAGES.contains(&lang.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Unsupported language")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!(
                            "Language '{}' is not supported. Supported languages: {}",
                            lang,
                            SUPPORTED_LANGUAGES.join(", ")
                        )),
                ),
            ));
        }
    }

    Ok(())
}

/// Validate commit SHA format
pub fn validate_commit_sha(sha: &str) -> ValidationResult<()> {
    let sha_regex = Regex::new(r"^[a-f0-9]{7,40}$").expect("Invalid regex");

    if !sha_regex.is_match(sha) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid commit SHA format")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("Must be 7-40 hexadecimal characters"),
            ),
        ));
    }

    Ok(())
}

/// Validate tenant ID format
pub fn validate_tenant_id(tenant_id: &str) -> ValidationResult<()> {
    let tenant_regex = Regex::new(r"^[a-z0-9_-]+$").expect("Invalid regex");

    if !tenant_regex.is_match(tenant_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid tenant ID format")
                    .with_code("BAD_REQUEST")
                    .with_string_details(
                        "Must contain only lowercase letters, numbers, underscores, and hyphens"
                            .to_string(),
                    ),
            ),
        ));
    }

    if tenant_id.len() > 50 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Tenant ID too long")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("Maximum length is 50 characters"),
            ),
        ));
    }

    Ok(())
}

/// Validate file paths for security (prevent directory traversal)
pub fn validate_file_paths(paths: &[String]) -> ValidationResult<()> {
    for path in paths {
        // Check for directory traversal attempts
        if path.contains("..") {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Invalid file path")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details("Directory traversal not allowed"),
                ),
            ));
        }

        // Check for absolute paths
        if path.starts_with('/') || path.contains(':') {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Invalid file path")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details("Absolute paths not allowed"),
                ),
            ));
        }

        // Check path length
        if path.len() > 500 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("File path too long")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details("Maximum path length is 500 characters"),
                ),
            ));
        }
    }

    Ok(())
}

/// Validate description/prompt length and content
pub fn validate_description(description: &str) -> ValidationResult<()> {
    if description.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Description cannot be empty").with_code("INTERNAL_ERROR")),
        ));
    }

    if description.len() > 5000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Description too long")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("Maximum length is 5000 characters"),
            ),
        ));
    }

    // Check for suspicious patterns
    let suspicious_patterns = [
        "DROP TABLE",
        "DELETE FROM",
        "INSERT INTO",
        "UPDATE SET",
        "<SCRIPT",
        "JAVASCRIPT:",
        "EVAL(",
        "EXEC(",
    ];

    let desc_upper = description.to_uppercase();
    for pattern in &suspicious_patterns {
        if desc_upper.contains(pattern) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Description contains suspicious content")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details("Please avoid SQL or script injection attempts"),
                ),
            ));
        }
    }

    Ok(())
}

/// Validate B3 hash format
pub fn validate_hash_b3(hash: &str) -> ValidationResult<()> {
    if !hash.starts_with("b3:") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid hash format")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("Must start with 'b3:'"),
            ),
        ));
    }

    let hash_part = &hash[3..]; // Remove "b3:" prefix
    let hash_regex = Regex::new(r"^[a-f0-9]{64}$").expect("Invalid regex");

    if !hash_regex.is_match(hash_part) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid hash format")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("Must be 64 hexadecimal characters after 'b3:'"),
            ),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_repo_id() {
        assert!(validate_repo_id("owner/repo").is_ok());
        assert!(validate_repo_id("my-org/my-project").is_ok());
        assert!(validate_repo_id("user_name/repo_name").is_ok());

        assert!(validate_repo_id("invalid").is_err());
        assert!(validate_repo_id("owner/repo/extra").is_err());
        assert!(validate_repo_id("owner repo").is_err());
        assert!(validate_repo_id("../../../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_commit_sha() {
        assert!(validate_commit_sha("abc1234").is_ok());
        assert!(validate_commit_sha("1234567890abcdef").is_ok());
        assert!(validate_commit_sha("1234567890abcdef1234567890abcdef12345678").is_ok());

        assert!(validate_commit_sha("123").is_err()); // Too short
        assert!(validate_commit_sha("ABCDEF").is_err()); // Uppercase not allowed
        assert!(validate_commit_sha("xyz123").is_err()); // Invalid characters
    }

    #[test]
    fn test_validate_file_paths() {
        assert!(validate_file_paths(&vec!["src/main.rs".to_string()]).is_ok());
        assert!(validate_file_paths(&vec![
            "lib/utils.rs".to_string(),
            "tests/test.rs".to_string()
        ])
        .is_ok());

        assert!(validate_file_paths(&vec!["../../../etc/passwd".to_string()]).is_err());
        assert!(validate_file_paths(&vec!["/etc/passwd".to_string()]).is_err());
        assert!(validate_file_paths(&vec!["C:\\Windows\\System32".to_string()]).is_err());
    }

    #[test]
    fn test_validate_description() {
        assert!(validate_description("Fix the bug in main.rs").is_ok());
        assert!(validate_description("Add new feature for user authentication").is_ok());

        assert!(validate_description("").is_err());
        assert!(validate_description("DROP TABLE users").is_err());
        assert!(validate_description("<script>alert('xss')</script>").is_err());
    }

    #[test]
    fn test_validate_languages() {
        assert!(validate_languages(&vec!["python".to_string()]).is_ok());
        assert!(validate_languages(&vec!["rust".to_string(), "typescript".to_string()]).is_ok());

        assert!(validate_languages(&vec![]).is_err());
        assert!(validate_languages(&vec!["cobol".to_string()]).is_err());
    }

    #[test]
    fn test_validate_hash_b3() {
        assert!(validate_hash_b3(
            "b3:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
        )
        .is_ok());

        assert!(validate_hash_b3("").is_err());
        assert!(validate_hash_b3("1234567890abcdef").is_err());
        assert!(validate_hash_b3("b3:invalid").is_err());
        assert!(validate_hash_b3("b3:1234567890ABCDEF").is_err()); // Uppercase not allowed
    }
}
