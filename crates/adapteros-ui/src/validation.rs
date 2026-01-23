//! Form validation rules and utilities
//!
//! Client-side validation for UX - server remains authoritative.
//! Provides field-level validation with composable rules.
//!
//! # PRD-UI-150: Validation Implementation
//!
//! ## Design Principles
//! - Client validation is UX optimization only
//! - Server validation is authoritative (never trust client)
//! - Errors map to specific fields (not generic "form invalid")
//! - Form state persists after validation failure
//!
//! ## Rule Categories
//! - **Presence**: Required
//! - **Length**: MinLength, MaxLength
//! - **Format**: Pattern, Email, Number
//! - **Range**: Range (f64), IntRange (i64)
//! - **Custom**: Function-based validation
//!
//! ## Predefined Rule Sets (in `rules` module)
//! - `adapter_name()`: identifier format, 3-128 chars
//! - `learning_rate()`: 1e-10 to 1.0
//! - `password()`: minimum 8 chars
//! - `email()`: RFC 5322 email pattern (matches server validation)
//!
//! ## Usage Pattern
//! ```rust
//! let errors = use_form_errors();
//! let rules = rules::adapter_name();
//! if let Err(msg) = validate_field(&name, &rules) {
//!     errors.update(|e| e.insert("name".into(), msg));
//! }
//! ```

use leptos::prelude::*;
use std::collections::HashMap;

/// A validation rule that can be applied to form fields
#[derive(Clone, Debug)]
pub enum ValidationRule {
    /// Field is required and cannot be empty
    Required,
    /// Field must have at least N characters
    MinLength(usize),
    /// Field must have at most N characters
    MaxLength(usize),
    /// Field must match a regex pattern
    Pattern {
        pattern: &'static str,
        message: &'static str,
    },
    /// Field must be a valid email
    Email,
    /// Field must be a valid number
    Number,
    /// Field must be a positive number (> 0)
    PositiveNumber,
    /// Field must be within range (inclusive)
    Range { min: f64, max: f64 },
    /// Field must be a valid integer within range
    IntRange { min: i64, max: i64 },
    /// Custom validation with a message
    Custom {
        validator: fn(&str) -> bool,
        message: &'static str,
    },
}

impl ValidationRule {
    /// Validate a value against this rule
    pub fn validate(&self, value: &str) -> Option<String> {
        match self {
            ValidationRule::Required => {
                if value.trim().is_empty() {
                    Some("This field is required".to_string())
                } else {
                    None
                }
            }
            ValidationRule::MinLength(min) => {
                if value.len() < *min {
                    Some(format!("Must be at least {} characters", min))
                } else {
                    None
                }
            }
            ValidationRule::MaxLength(max) => {
                if value.len() > *max {
                    Some(format!("Must be at most {} characters", max))
                } else {
                    None
                }
            }
            ValidationRule::Pattern { pattern, message } => {
                let re = regex_lite::Regex::new(pattern).ok()?;
                if re.is_match(value) {
                    None
                } else {
                    Some(message.to_string())
                }
            }
            ValidationRule::Email => {
                // RFC 5322-compliant email validation - matches server validation
                // Pattern validates:
                // - Local part: alphanumeric, dots, hyphens, underscores, plus signs
                // - Domain: alphanumeric with hyphens, proper TLD structure
                // - Rejects malformed addresses like "a@.b" or "test@domain"
                if value.is_empty() {
                    return None; // Let Required rule handle empty
                }
                // RFC 5321 length limits: 3-254 characters
                if value.len() < 3 || value.len() > 254 {
                    return Some("Must be a valid email address".to_string());
                }
                // Case-insensitive RFC 5322 pattern (matches server validation.rs)
                let email_pattern = r"(?i)^[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)+$";
                // Fail-closed: if regex compilation fails, validation fails
                let re = regex_lite::Regex::new(email_pattern)
                    .expect("Email regex pattern is invalid - this is a bug");
                if re.is_match(value) {
                    None
                } else {
                    Some("Must be a valid email address".to_string())
                }
            }
            ValidationRule::Number => {
                if value.is_empty() || value.parse::<f64>().is_ok() {
                    None
                } else {
                    Some("Must be a valid number".to_string())
                }
            }
            ValidationRule::PositiveNumber => {
                if value.is_empty() {
                    return None;
                }
                match value.parse::<f64>() {
                    Ok(n) if n > 0.0 => None,
                    Ok(_) => Some("Must be a positive number".to_string()),
                    Err(_) => Some("Must be a valid number".to_string()),
                }
            }
            ValidationRule::Range { min, max } => {
                if value.is_empty() {
                    return None;
                }
                match value.parse::<f64>() {
                    Ok(n) if n >= *min && n <= *max => None,
                    Ok(_) => Some(format!("Must be between {} and {}", min, max)),
                    Err(_) => Some("Must be a valid number".to_string()),
                }
            }
            ValidationRule::IntRange { min, max } => {
                if value.is_empty() {
                    return None;
                }
                match value.parse::<i64>() {
                    Ok(n) if n >= *min && n <= *max => None,
                    Ok(_) => Some(format!("Must be between {} and {}", min, max)),
                    Err(_) => Some("Must be a valid integer".to_string()),
                }
            }
            ValidationRule::Custom { validator, message } => {
                if value.is_empty() || validator(value) {
                    None
                } else {
                    Some(message.to_string())
                }
            }
        }
    }
}

/// Validate a value against multiple rules, returning the first error
pub fn validate_field(value: &str, rules: &[ValidationRule]) -> Option<String> {
    for rule in rules {
        if let Some(error) = rule.validate(value) {
            return Some(error);
        }
    }
    None
}

/// Validate a value against multiple rules, returning all errors
pub fn validate_field_all(value: &str, rules: &[ValidationRule]) -> Vec<String> {
    rules
        .iter()
        .filter_map(|rule| rule.validate(value))
        .collect()
}

/// Form-level validation state
/// Tracks errors for multiple fields and overall form validity
#[derive(Clone, Debug, Default)]
pub struct FormErrors {
    errors: HashMap<String, String>,
}

impl FormErrors {
    /// Create a new empty FormErrors
    pub fn new() -> Self {
        Self {
            errors: HashMap::new(),
        }
    }

    /// Set an error for a field
    pub fn set(&mut self, field: &str, error: String) {
        self.errors.insert(field.to_string(), error);
    }

    /// Clear the error for a field
    pub fn clear(&mut self, field: &str) {
        self.errors.remove(field);
    }

    /// Clear all errors
    pub fn clear_all(&mut self) {
        self.errors.clear();
    }

    /// Get the error for a field
    pub fn get(&self, field: &str) -> Option<&String> {
        self.errors.get(field)
    }

    /// Check if a field has an error
    pub fn has_error(&self, field: &str) -> bool {
        self.errors.contains_key(field)
    }

    /// Check if the form has any errors
    pub fn has_any(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check if the form is valid (no errors)
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get all errors
    pub fn all(&self) -> &HashMap<String, String> {
        &self.errors
    }

    /// Get the count of errors
    pub fn count(&self) -> usize {
        self.errors.len()
    }
}

/// Hook to create a reactive form error state
pub fn use_form_errors() -> RwSignal<FormErrors> {
    RwSignal::new(FormErrors::new())
}

/// Validate a field and update the form errors signal
pub fn validate_and_update(
    field: &str,
    value: &str,
    rules: &[ValidationRule],
    errors: RwSignal<FormErrors>,
) -> bool {
    if let Some(error) = validate_field(value, rules) {
        errors.update(|e| e.set(field, error));
        false
    } else {
        errors.update(|e| e.clear(field));
        true
    }
}

/// Common validation rule sets
pub mod rules {
    use super::ValidationRule;

    /// Rules for adapter names
    pub fn adapter_name() -> Vec<ValidationRule> {
        vec![
            ValidationRule::Required,
            ValidationRule::MinLength(3),
            ValidationRule::MaxLength(128),
            ValidationRule::Pattern {
                pattern: r"^[a-zA-Z][a-zA-Z0-9_-]*$",
                message: "Must start with a letter and contain only letters, numbers, underscores, and hyphens",
            },
        ]
    }

    /// Rules for email fields
    pub fn email() -> Vec<ValidationRule> {
        vec![ValidationRule::Required, ValidationRule::Email]
    }

    /// Rules for password fields
    pub fn password() -> Vec<ValidationRule> {
        vec![ValidationRule::Required, ValidationRule::MinLength(8)]
    }

    /// Rules for positive integer fields (e.g., epochs, batch size)
    pub fn positive_int() -> Vec<ValidationRule> {
        vec![
            ValidationRule::Required,
            ValidationRule::IntRange {
                min: 1,
                max: i64::MAX,
            },
        ]
    }

    /// Rules for learning rate
    /// Note: Learning rate must be > 0 (PositiveNumber) and <= 1.0
    pub fn learning_rate() -> Vec<ValidationRule> {
        vec![
            ValidationRule::Required,
            ValidationRule::PositiveNumber,
            // Range max only - PositiveNumber handles min > 0
            ValidationRule::Range {
                min: 1e-10,
                max: 1.0,
            },
        ]
    }

    /// Rules for optional descriptions
    pub fn description() -> Vec<ValidationRule> {
        vec![ValidationRule::MaxLength(10000)]
    }

    /// Rules for short descriptions (e.g., publishing)
    pub fn short_description() -> Vec<ValidationRule> {
        vec![ValidationRule::MaxLength(280)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_rule() {
        let rule = ValidationRule::Required;
        assert!(rule.validate("").is_some());
        assert!(rule.validate("   ").is_some());
        assert!(rule.validate("hello").is_none());
    }

    #[test]
    fn test_min_length_rule() {
        let rule = ValidationRule::MinLength(3);
        assert!(rule.validate("ab").is_some());
        assert!(rule.validate("abc").is_none());
        assert!(rule.validate("abcd").is_none());
    }

    #[test]
    fn test_email_rule() {
        let rule = ValidationRule::Email;
        // Invalid emails - RFC 5322 strict validation
        assert!(rule.validate("invalid").is_some());
        assert!(rule.validate("test@").is_some());
        assert!(rule.validate("@example.com").is_some());
        assert!(rule.validate("test@example").is_some()); // No TLD
        assert!(rule.validate("test @example.com").is_some()); // Whitespace
        assert!(rule.validate("a@.b").is_some()); // Dot after @
        assert!(rule.validate("user@.domain.com").is_some()); // Leading dot in domain
        assert!(rule.validate("user@domain.").is_some()); // Trailing dot
        assert!(rule.validate("ab").is_some()); // Too short
        assert!(rule.validate("user name@example.com").is_some()); // Space in local

        // Valid emails - RFC 5322 compliant
        assert!(rule.validate("test@example.com").is_none());
        assert!(rule.validate("user.name@example.co.uk").is_none());
        assert!(rule.validate("user+tag@example.com").is_none());
        assert!(rule.validate("user@sub.domain.example.com").is_none());
        assert!(rule.validate("user123@example123.com").is_none());
        assert!(rule.validate("USER@EXAMPLE.COM").is_none()); // Case insensitive

        // Empty should pass (Required rule handles that)
        assert!(rule.validate("").is_none());
    }

    #[test]
    fn test_positive_number_rule() {
        let rule = ValidationRule::PositiveNumber;
        assert!(rule.validate("0").is_some());
        assert!(rule.validate("-1").is_some());
        assert!(rule.validate("abc").is_some());
        assert!(rule.validate("1").is_none());
        assert!(rule.validate("0.5").is_none());
    }

    #[test]
    fn test_form_errors() {
        let mut errors = FormErrors::new();
        assert!(errors.is_valid());

        errors.set("email", "Invalid email".to_string());
        assert!(!errors.is_valid());
        assert!(errors.has_error("email"));
        assert_eq!(errors.get("email"), Some(&"Invalid email".to_string()));

        errors.clear("email");
        assert!(errors.is_valid());
    }
}
