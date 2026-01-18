//! Pre-built validators for common use cases
//!
//! This module provides ready-to-use validators for common AdapterOS identifiers
//! and names, reducing boilerplate and ensuring consistency.
//!
//! # Example
//!
//! ```rust
//! use adapteros_core::validation::presets;
//!
//! let validator = presets::adapter_id_validator();
//! assert!(validator.validate("my-adapter-1").is_ok());
//! assert!(validator.validate("system-adapter").is_err()); // reserved prefix
//! ```

use super::builder::{Validator, ValidatorBuilder};

/// Reserved prefixes for system-managed adapters.
pub const RESERVED_ADAPTER_PREFIXES: &[&str] = &["system-", "internal-", "reserved-"];

/// Reserved prefixes for system tenants.
pub const RESERVED_TENANT_PREFIXES: &[&str] = &["system", "internal", "admin", "root"];

/// Reserved policy names.
pub const RESERVED_POLICY_NAMES: &[&str] = &["default", "system", "base"];

// =============================================================================
// Adapter Validators
// =============================================================================

/// Create a validator for adapter IDs.
///
/// # Rules
///
/// - Not empty
/// - 1-64 characters
/// - Alphanumeric with hyphens and underscores
/// - Must start and end with alphanumeric character
/// - No consecutive hyphens/underscores
/// - Cannot use reserved prefixes (system-, internal-, reserved-)
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::adapter_id_validator;
///
/// let validator = adapter_id_validator();
/// assert!(validator.validate("my-adapter").is_ok());
/// assert!(validator.validate("adapter_v2").is_ok());
/// assert!(validator.validate("").is_err());
/// assert!(validator.validate("system-foo").is_err());
/// ```
pub fn adapter_id_validator() -> Validator {
    ValidatorBuilder::new("adapter_id")
        .not_empty()
        .with_chars("-_")
        .length(1, 64)
        .starts_with_alphanumeric()
        .ends_with_alphanumeric()
        .no_consecutive_separators()
        .not_reserved_prefixes(RESERVED_ADAPTER_PREFIXES)
        .build()
}

/// Create a validator for adapter names (display names).
///
/// # Rules
///
/// - Not empty
/// - 1-128 characters
/// - Alphanumeric with spaces, hyphens, and underscores
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::adapter_name_validator;
///
/// let validator = adapter_name_validator();
/// assert!(validator.validate("My Adapter").is_ok());
/// assert!(validator.validate("Adapter-v2_beta").is_ok());
/// ```
pub fn adapter_name_validator() -> Validator {
    ValidatorBuilder::new("adapter_name")
        .not_empty()
        .with_chars(" -_")
        .length(1, 128)
        .build()
}

// =============================================================================
// Tenant Validators
// =============================================================================

/// Create a validator for tenant IDs.
///
/// # Rules
///
/// - Not empty
/// - 1-64 characters
/// - Alphanumeric with hyphens and underscores
/// - Must start and end with alphanumeric character
/// - No consecutive hyphens/underscores
/// - Cannot use reserved prefixes
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::tenant_id_validator;
///
/// let validator = tenant_id_validator();
/// assert!(validator.validate("acme-corp").is_ok());
/// assert!(validator.validate("tenant_123").is_ok());
/// assert!(validator.validate("admin").is_err()); // reserved
/// ```
pub fn tenant_id_validator() -> Validator {
    ValidatorBuilder::new("tenant_id")
        .not_empty()
        .with_chars("-_")
        .length(1, 64)
        .starts_with_alphanumeric()
        .ends_with_alphanumeric()
        .no_consecutive_separators()
        .not_reserved_words(RESERVED_TENANT_PREFIXES)
        .build()
}

// =============================================================================
// Policy Validators
// =============================================================================

/// Create a validator for policy names.
///
/// # Rules
///
/// - Not empty
/// - 1-64 characters
/// - Alphanumeric with hyphens and underscores
/// - Must start with alphanumeric character
/// - Cannot use reserved policy names
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::policy_name_validator;
///
/// let validator = policy_name_validator();
/// assert!(validator.validate("egress-policy").is_ok());
/// assert!(validator.validate("my_custom_policy").is_ok());
/// assert!(validator.validate("default").is_err()); // reserved
/// ```
pub fn policy_name_validator() -> Validator {
    ValidatorBuilder::new("policy_name")
        .not_empty()
        .with_chars("-_")
        .length(1, 64)
        .starts_with_alphanumeric()
        .not_reserved_words(RESERVED_POLICY_NAMES)
        .build()
}

// =============================================================================
// Stack Validators
// =============================================================================

/// Create a validator for stack IDs.
///
/// # Rules
///
/// - Not empty
/// - 1-64 characters
/// - Alphanumeric with hyphens, underscores, and dots
/// - Must start and end with alphanumeric character
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::stack_id_validator;
///
/// let validator = stack_id_validator();
/// assert!(validator.validate("production.main").is_ok());
/// assert!(validator.validate("dev-stack_v2").is_ok());
/// ```
pub fn stack_id_validator() -> Validator {
    ValidatorBuilder::new("stack_id")
        .not_empty()
        .with_chars("-_.")
        .length(1, 64)
        .starts_with_alphanumeric()
        .ends_with_alphanumeric()
        .build()
}

// =============================================================================
// Repository/Path Validators
// =============================================================================

/// Create a validator for repository IDs.
///
/// # Rules
///
/// - Not empty
/// - 1-256 characters
/// - Alphanumeric with hyphens, underscores, forward slashes, and dots
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::repo_id_validator;
///
/// let validator = repo_id_validator();
/// assert!(validator.validate("org/repo").is_ok());
/// assert!(validator.validate("my-org/my-repo.git").is_ok());
/// ```
pub fn repo_id_validator() -> Validator {
    ValidatorBuilder::new("repo_id")
        .not_empty()
        .with_chars("-_/.")
        .length(1, 256)
        .build()
}

/// Create a validator for descriptions.
///
/// # Rules
///
/// - Can be empty
/// - Maximum 1024 characters
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::description_validator;
///
/// let validator = description_validator();
/// assert!(validator.validate("").is_ok()); // empty allowed
/// assert!(validator.validate("A short description").is_ok());
/// ```
pub fn description_validator() -> Validator {
    ValidatorBuilder::new("description").max_length(1024).build()
}

// =============================================================================
// Hash/Crypto Validators
// =============================================================================

/// Create a validator for BLAKE3 hashes (b3:... format).
///
/// # Rules
///
/// - Not empty
/// - Must start with "b3:"
/// - Hex part must be exactly 64 characters
/// - Hex part must contain only valid hex characters
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::b3_hash_validator;
///
/// let validator = b3_hash_validator();
/// let valid_hash = "b3:a".to_string() + &"0".repeat(63);
/// assert!(validator.validate(&valid_hash).is_ok());
/// assert!(validator.validate("invalid").is_err());
/// ```
pub fn b3_hash_validator() -> Validator {
    use super::error::{ValidationError, ValidationErrorCode};

    ValidatorBuilder::new("hash")
        .not_empty()
        .custom("b3 hash format", |input| {
            if !input.starts_with("b3:") {
                return Err(ValidationError::with_code(
                    "hash",
                    "Hash must start with 'b3:' prefix",
                    ValidationErrorCode::PatternMismatch,
                ));
            }

            let hex_part = &input[3..];
            if hex_part.len() != 64 {
                return Err(ValidationError::with_code(
                    "hash",
                    format!(
                        "B3 hash hex part must be 64 characters (got {})",
                        hex_part.len()
                    ),
                    ValidationErrorCode::TooShort,
                ));
            }

            if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ValidationError::with_code(
                    "hash",
                    "B3 hash hex part must contain only hexadecimal characters",
                    ValidationErrorCode::InvalidCharacters,
                ));
            }

            Ok(())
        })
        .build()
}

/// Create a validator for git commit hashes (short or full).
///
/// # Rules
///
/// - Not empty
/// - 7-40 characters (short hash to full SHA)
/// - Only hexadecimal characters
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::commit_hash_validator;
///
/// let validator = commit_hash_validator();
/// assert!(validator.validate("abc123f").is_ok()); // short hash
/// assert!(validator.validate("abcdef1234567890abcdef1234567890abcdef12").is_ok()); // full SHA
/// assert!(validator.validate("abc").is_err()); // too short
/// ```
pub fn commit_hash_validator() -> Validator {
    ValidatorBuilder::new("commit_hash")
        .not_empty()
        .length(7, 40)
        .hexadecimal()
        .build()
}

// =============================================================================
// Codebase Validators
// =============================================================================

/// Create a validator for codebase repository slugs.
///
/// # Rules
///
/// - Not empty
/// - 1-64 characters
/// - Lowercase letters, numbers, and underscores only
/// - Cannot start or end with underscore
/// - No consecutive underscores
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::codebase_slug_validator;
///
/// let validator = codebase_slug_validator();
/// assert!(validator.validate("my_repo").is_ok());
/// assert!(validator.validate("repo123").is_ok());
/// assert!(validator.validate("_repo").is_err()); // starts with underscore
/// ```
pub fn codebase_slug_validator() -> Validator {
    use super::error::{ValidationError, ValidationErrorCode};

    ValidatorBuilder::new("repo_slug")
        .not_empty()
        .length(1, 64)
        .lowercase_slug()
        .custom("slug boundaries", |input| {
            if input.starts_with('_') || input.ends_with('_') {
                return Err(ValidationError::with_code(
                    "repo_slug",
                    "Repository slug cannot start or end with underscore",
                    ValidationErrorCode::InvalidStart,
                ));
            }
            if input.contains("__") {
                return Err(ValidationError::with_code(
                    "repo_slug",
                    "Repository slug cannot contain consecutive underscores",
                    ValidationErrorCode::ConsecutiveSpecialChars,
                ));
            }
            Ok(())
        })
        .build()
}

// =============================================================================
// Version Validators
// =============================================================================

/// Create a validator for semantic versions.
///
/// # Rules
///
/// - Not empty
/// - Format: MAJOR.MINOR.PATCH (optional pre-release and build metadata)
///
/// # Example
///
/// ```rust
/// use adapteros_core::validation::presets::semver_validator;
///
/// let validator = semver_validator();
/// assert!(validator.validate("1.0.0").is_ok());
/// assert!(validator.validate("2.1.3-beta.1").is_ok());
/// assert!(validator.validate("invalid").is_err());
/// ```
pub fn semver_validator() -> Validator {
    use super::error::{ValidationError, ValidationErrorCode};

    ValidatorBuilder::new("version")
        .not_empty()
        .custom("semantic version", |input| {
            // Simple semver validation: MAJOR.MINOR.PATCH
            // Optionally with -prerelease and +buildmetadata
            let base = input.split(&['-', '+'][..]).next().unwrap_or(input);
            let parts: Vec<&str> = base.split('.').collect();

            if parts.len() != 3 {
                return Err(ValidationError::with_code(
                    "version",
                    "Version must be in MAJOR.MINOR.PATCH format",
                    ValidationErrorCode::PatternMismatch,
                ));
            }

            for (i, part) in parts.iter().enumerate() {
                if part.parse::<u64>().is_err() {
                    let name = match i {
                        0 => "MAJOR",
                        1 => "MINOR",
                        _ => "PATCH",
                    };
                    return Err(ValidationError::with_code(
                        "version",
                        format!("{} version component must be a number", name),
                        ValidationErrorCode::PatternMismatch,
                    ));
                }
            }

            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_id_validator() {
        let v = adapter_id_validator();
        assert!(v.validate("my-adapter").is_ok());
        assert!(v.validate("adapter_v2").is_ok());
        assert!(v.validate("a1").is_ok());
        assert!(v.validate("").is_err());
        assert!(v.validate("-start").is_err());
        assert!(v.validate("end-").is_err());
        assert!(v.validate("double--hyphen").is_err());
        assert!(v.validate("system-foo").is_err());
        assert!(v.validate("internal-bar").is_err());
    }

    #[test]
    fn test_adapter_name_validator() {
        let v = adapter_name_validator();
        assert!(v.validate("My Adapter").is_ok());
        assert!(v.validate("Adapter-v2_beta").is_ok());
        assert!(v.validate("").is_err());
    }

    #[test]
    fn test_tenant_id_validator() {
        let v = tenant_id_validator();
        assert!(v.validate("acme-corp").is_ok());
        assert!(v.validate("tenant_123").is_ok());
        assert!(v.validate("").is_err());
        assert!(v.validate("admin").is_err());
        assert!(v.validate("system").is_err());
    }

    #[test]
    fn test_policy_name_validator() {
        let v = policy_name_validator();
        assert!(v.validate("egress-policy").is_ok());
        assert!(v.validate("my_custom_policy").is_ok());
        assert!(v.validate("").is_err());
        assert!(v.validate("default").is_err());
        assert!(v.validate("system").is_err());
    }

    #[test]
    fn test_stack_id_validator() {
        let v = stack_id_validator();
        assert!(v.validate("production.main").is_ok());
        assert!(v.validate("dev-stack_v2").is_ok());
        assert!(v.validate("").is_err());
    }

    #[test]
    fn test_repo_id_validator() {
        let v = repo_id_validator();
        assert!(v.validate("org/repo").is_ok());
        assert!(v.validate("my-org/my-repo.git").is_ok());
        assert!(v.validate("").is_err());
    }

    #[test]
    fn test_description_validator() {
        let v = description_validator();
        assert!(v.validate("").is_ok());
        assert!(v.validate("A short description").is_ok());
        assert!(v.validate(&"x".repeat(1024)).is_ok());
        assert!(v.validate(&"x".repeat(1025)).is_err());
    }

    #[test]
    fn test_b3_hash_validator() {
        let v = b3_hash_validator();
        let valid_hash = format!("b3:{}", "a".repeat(64));
        assert!(v.validate(&valid_hash).is_ok());
        assert!(v.validate("invalid").is_err());
        assert!(v.validate("b3:short").is_err());
    }

    #[test]
    fn test_commit_hash_validator() {
        let v = commit_hash_validator();
        assert!(v.validate("abc123f").is_ok());
        assert!(v.validate("abcdef1234567890abcdef1234567890abcdef12").is_ok());
        assert!(v.validate("abc").is_err());
        assert!(v.validate("not-hex!").is_err());
    }

    #[test]
    fn test_codebase_slug_validator() {
        let v = codebase_slug_validator();
        assert!(v.validate("my_repo").is_ok());
        assert!(v.validate("repo123").is_ok());
        assert!(v.validate("_repo").is_err());
        assert!(v.validate("repo_").is_err());
        assert!(v.validate("repo__name").is_err());
        assert!(v.validate("UPPERCASE").is_err());
    }

    #[test]
    fn test_semver_validator() {
        let v = semver_validator();
        assert!(v.validate("1.0.0").is_ok());
        assert!(v.validate("2.1.3").is_ok());
        assert!(v.validate("0.0.1").is_ok());
        assert!(v.validate("10.20.30").is_ok());
        assert!(v.validate("1.0.0-beta.1").is_ok());
        assert!(v.validate("1.0.0+build.123").is_ok());
        assert!(v.validate("invalid").is_err());
        assert!(v.validate("1.0").is_err());
        assert!(v.validate("1.0.0.0").is_err());
    }
}
