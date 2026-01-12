//! Dataset format and status validation.
//!
//! These functions validate dataset attributes against allowed values.

use adapteros_core::{AosError, Result};

// ============================================================================
// Dataset Format and Status Validation
// ============================================================================

/// Valid dataset format types
pub const VALID_FORMATS: &[&str] = &["patches", "jsonl", "txt", "custom", "parquet", "csv"];

/// Valid dataset status values
pub const VALID_STATUSES: &[&str] = &["uploaded", "processing", "ready", "failed"];

/// Valid dataset categories (for row classification and source type)
pub const VALID_CATEGORIES: &[&str] = &[
    "positive",
    "negative",
    "neutral",
    "synthetic",
    "manual",
    "augmented",
    "upload",
    "codebase",
];

/// Validate dataset format
pub fn validate_format(format: &str) -> Result<()> {
    if VALID_FORMATS.contains(&format) {
        Ok(())
    } else {
        Err(AosError::Validation(format!(
            "Invalid dataset format '{}'. Must be one of: {}",
            format,
            VALID_FORMATS.join(", ")
        )))
    }
}

/// Validate dataset status
pub fn validate_status(status: &str) -> Result<()> {
    if VALID_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(AosError::Validation(format!(
            "Invalid dataset status '{}'. Must be one of: {}",
            status,
            VALID_STATUSES.join(", ")
        )))
    }
}

/// Validate BLAKE3 hash format (64 hex characters)
pub fn validate_hash_b3(hash: &str) -> Result<()> {
    if hash.len() != 64 {
        return Err(AosError::Validation(format!(
            "Invalid hash_b3 length: expected 64 hex characters, got {}",
            hash.len()
        )));
    }
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AosError::Validation(
            "Invalid hash_b3: must contain only hexadecimal characters".to_string(),
        ));
    }
    Ok(())
}

/// Validate dataset category
pub fn validate_category(category: &str) -> Result<()> {
    if VALID_CATEGORIES.contains(&category) {
        Ok(())
    } else {
        Err(AosError::Validation(format!(
            "Invalid dataset category '{}'. Must be one of: {}",
            category,
            VALID_CATEGORIES.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_formats_accepted() {
        for format in VALID_FORMATS {
            assert!(validate_format(format).is_ok());
        }
    }

    #[test]
    fn invalid_format_rejected() {
        assert!(validate_format("invalid_format").is_err());
    }

    #[test]
    fn valid_statuses_accepted() {
        for status in VALID_STATUSES {
            assert!(validate_status(status).is_ok());
        }
    }

    #[test]
    fn invalid_status_rejected() {
        assert!(validate_status("invalid_status").is_err());
    }

    #[test]
    fn valid_hash_accepted() {
        let valid_hash = "a".repeat(64);
        assert!(validate_hash_b3(&valid_hash).is_ok());
    }

    #[test]
    fn invalid_hash_length_rejected() {
        assert!(validate_hash_b3("abc").is_err());
        assert!(validate_hash_b3(&"a".repeat(63)).is_err());
        assert!(validate_hash_b3(&"a".repeat(65)).is_err());
    }

    #[test]
    fn invalid_hash_chars_rejected() {
        let invalid_hash = "g".repeat(64);
        assert!(validate_hash_b3(&invalid_hash).is_err());
    }

    #[test]
    fn valid_categories_accepted() {
        for category in VALID_CATEGORIES {
            assert!(validate_category(category).is_ok());
        }
    }

    #[test]
    fn invalid_category_rejected() {
        assert!(validate_category("unknown_category").is_err());
    }
}
