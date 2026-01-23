//! Email validation utilities for authentication handlers.
//!
//! Provides RFC 5322-compliant email validation and normalization.

use once_cell::sync::Lazy;
use regex::Regex;

/// RFC 5322-compliant email regex pattern.
///
/// This pattern validates:
/// - Local part: alphanumeric, dots, hyphens, underscores, plus signs
/// - Domain: alphanumeric with hyphens, proper TLD structure
/// - Rejects malformed addresses like "a@.b" or "test@domain"
pub(super) static EMAIL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)+$"
    ).expect("email regex is valid")
});

/// Validates an email address against RFC 5322 rules.
///
/// Returns `true` if the email is valid, `false` otherwise.
///
/// # Examples
///
/// ```ignore
/// assert!(is_valid_email("user@example.com"));
/// assert!(is_valid_email("user.name+tag@sub.domain.org"));
/// assert!(!is_valid_email("a@.b"));
/// assert!(!is_valid_email("invalid"));
/// assert!(!is_valid_email("@domain.com"));
/// ```
pub(super) fn is_valid_email(email: &str) -> bool {
    // Length check: RFC 5321 limits email to 254 characters
    if email.len() > 254 || email.len() < 3 {
        return false;
    }

    EMAIL_REGEX.is_match(email)
}

/// Normalizes an email address for consistent storage and lookup.
///
/// - Trims leading/trailing whitespace
/// - Converts to lowercase
pub(super) fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_emails() {
        assert!(is_valid_email("user@example.com"));
        assert!(is_valid_email("user.name@example.com"));
        assert!(is_valid_email("user+tag@example.com"));
        assert!(is_valid_email("user@sub.domain.example.com"));
        assert!(is_valid_email("user123@example123.com"));
        assert!(is_valid_email("USER@EXAMPLE.COM")); // case insensitive
    }

    #[test]
    fn test_invalid_emails() {
        assert!(!is_valid_email("a@.b")); // dot after @
        assert!(!is_valid_email("invalid")); // no @
        assert!(!is_valid_email("@domain.com")); // no local part
        assert!(!is_valid_email("user@")); // no domain
        assert!(!is_valid_email("user@domain")); // no TLD
        assert!(!is_valid_email("user@.domain.com")); // leading dot in domain
        assert!(!is_valid_email("user@domain.")); // trailing dot
        assert!(!is_valid_email("")); // empty
        assert!(!is_valid_email("ab")); // too short
        assert!(!is_valid_email("user name@example.com")); // space in local
    }

    #[test]
    fn test_normalize_email() {
        assert_eq!(normalize_email("  User@Example.COM  "), "user@example.com");
        assert_eq!(normalize_email("TEST@test.com"), "test@test.com");
    }
}
