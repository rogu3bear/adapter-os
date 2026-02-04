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
//! - **Validate on blur or submit, not on every keystroke**
//!
//! ## Rule Categories
//! - **Presence**: Required
//! - **Length**: MinLength, MaxLength
//! - **Format**: Pattern, Email, Number
//! - **Range**: Range (f64), IntRange (i64)
//! - **Custom**: Function-based validation
//!
//! ## Predefined Rule Sets (in `rules` module)
//! - `adapter_name()`: identifier format, 1-64 chars, alphanumeric with hyphens/underscores,
//!   must start/end with alphanumeric, no consecutive hyphens/underscores, no reserved prefixes
//! - `learning_rate()`: 1e-10 to 1.0
//! - `password()`: minimum 8 chars
//! - `email()`: RFC 5322 email pattern (matches server validation)
//! - `description()`: maximum 1024 chars (matches server validation)
//!
//! ## Usage Pattern
//! ```rust
//! let form = use_form_state();
//! let rules = rules::adapter_name();
//!
//! // Validate on blur (after first interaction)
//! form.validate_on_blur("name", &name_value, &rules);
//!
//! // Validate all on submit
//! if form.validate_on_submit() {
//!     // proceed with submission
//! }
//! ```

use leptos::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Pre-compiled email regex for efficient validation.
/// Compiled once on first use via OnceLock to avoid panics and improve performance.
/// Returns None if the regex pattern is somehow invalid (defensive - should never happen).
fn get_email_regex() -> Option<&'static regex_lite::Regex> {
    static EMAIL_REGEX: OnceLock<Option<regex_lite::Regex>> = OnceLock::new();
    EMAIL_REGEX
        .get_or_init(|| {
            // RFC 5322 compliant email pattern (case-insensitive)
            let email_pattern = r"(?i)^[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)+$";
            regex_lite::Regex::new(email_pattern).ok()
        })
        .as_ref()
}

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
                let char_count = value.chars().count();
                if char_count < *min {
                    Some(format!("Must be at least {} characters", min))
                } else {
                    None
                }
            }
            ValidationRule::MaxLength(max) => {
                let char_count = value.chars().count();
                if char_count > *max {
                    Some(format!("Must be at most {} characters", max))
                } else {
                    None
                }
            }
            ValidationRule::Pattern { pattern, message } => {
                // Fail-closed: if regex compilation fails, validation fails
                let Ok(re) = regex_lite::Regex::new(pattern) else {
                    // Invalid pattern - fail validation (fail-closed)
                    return Some(message.to_string());
                };
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
                // Use pre-compiled regex via OnceLock (panic-free, compiled once)
                // Fail-closed: if regex is unavailable, validation fails
                let Some(re) = get_email_regex() else {
                    return Some("Must be a valid email address".to_string());
                };
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

/// Form state with touched field tracking for blur-based validation.
///
/// Fields are only validated after they've been "touched" (blurred at least once)
/// or after form submit. This prevents showing errors on every keystroke.
#[derive(Clone, Debug, Default)]
pub struct FormState {
    /// Validation errors per field
    errors: HashMap<String, String>,
    /// Fields that have been touched (blurred at least once)
    touched: HashSet<String>,
    /// Whether form has been submitted (validates all fields)
    submitted: bool,
}

impl FormState {
    /// Create a new empty FormState
    pub fn new() -> Self {
        Self {
            errors: HashMap::new(),
            touched: HashSet::new(),
            submitted: false,
        }
    }

    /// Mark a field as touched (after blur event)
    pub fn touch(&mut self, field: &str) {
        self.touched.insert(field.to_string());
    }

    /// Check if a field has been touched
    pub fn is_touched(&self, field: &str) -> bool {
        self.touched.contains(field)
    }

    /// Check if a field should show its error.
    /// Only shows error if field was touched or form was submitted.
    pub fn should_show_error(&self, field: &str) -> bool {
        self.submitted || self.touched.contains(field)
    }

    /// Set an error for a field (internal)
    fn set_error(&mut self, field: &str, error: String) {
        self.errors.insert(field.to_string(), error);
    }

    /// Clear the error for a field
    pub fn clear_error(&mut self, field: &str) {
        self.errors.remove(field);
    }

    /// Get the error for a field, but only if it should be shown
    pub fn get_visible_error(&self, field: &str) -> Option<&String> {
        if self.should_show_error(field) {
            self.errors.get(field)
        } else {
            None
        }
    }

    /// Get the raw error for a field (regardless of touched state)
    pub fn get_error(&self, field: &str) -> Option<&String> {
        self.errors.get(field)
    }

    /// Check if a field has a visible error
    pub fn has_visible_error(&self, field: &str) -> bool {
        self.should_show_error(field) && self.errors.contains_key(field)
    }

    /// Check if any field has a visible error
    pub fn has_any_visible_error(&self) -> bool {
        if self.submitted {
            !self.errors.is_empty()
        } else {
            self.touched.iter().any(|f| self.errors.contains_key(f))
        }
    }

    /// Check if the form is valid (no errors at all)
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Mark form as submitted (shows all errors)
    pub fn mark_submitted(&mut self) {
        self.submitted = true;
    }

    /// Check if form has been submitted
    pub fn is_submitted(&self) -> bool {
        self.submitted
    }

    /// Clear all state (errors, touched, submitted)
    pub fn clear_all(&mut self) {
        self.errors.clear();
        self.touched.clear();
        self.submitted = false;
    }

    /// Reset for a new submission attempt (clears errors and submitted flag, keeps touched)
    pub fn reset_for_retry(&mut self) {
        self.errors.clear();
        self.submitted = false;
    }
}

/// Hook to create a reactive form state with touched field tracking
pub fn use_form_state() -> RwSignal<FormState> {
    RwSignal::new(FormState::new())
}

/// Validate a field on blur (after first interaction).
///
/// Call this from an `on:blur` handler. The field will be marked as touched
/// and validated. Errors will be shown for this field going forward.
pub fn validate_on_blur(
    field: &str,
    value: &str,
    rules: &[ValidationRule],
    state: RwSignal<FormState>,
) -> bool {
    state.update(|s| {
        s.touch(field);
        if let Some(error) = validate_field(value, rules) {
            s.set_error(field, error);
        } else {
            s.clear_error(field);
        }
    });
    state.get_untracked().get_error(field).is_none()
}

/// Validate a field silently (update error state but don't mark as touched).
///
/// Use this for re-validation on subsequent input changes after initial blur.
/// Only updates the error state if the field was already touched.
pub fn validate_silently(
    field: &str,
    value: &str,
    rules: &[ValidationRule],
    state: RwSignal<FormState>,
) -> bool {
    let is_touched = state.get_untracked().is_touched(field);
    if is_touched {
        state.update(|s| {
            if let Some(error) = validate_field(value, rules) {
                s.set_error(field, error);
            } else {
                s.clear_error(field);
            }
        });
    }
    state.get_untracked().get_error(field).is_none()
}

/// Validate a single field for form submission.
///
/// Returns true if valid. The field is NOT marked as touched (use for submit validation).
pub fn validate_for_submit(
    field: &str,
    value: &str,
    rules: &[ValidationRule],
    state: RwSignal<FormState>,
) -> bool {
    if let Some(error) = validate_field(value, rules) {
        state.update(|s| s.set_error(field, error));
        false
    } else {
        state.update(|s| s.clear_error(field));
        true
    }
}

/// Mark form as submitted and return whether it's valid.
///
/// After calling this, all field errors will be visible regardless of touched state.
pub fn mark_submitted(state: RwSignal<FormState>) -> bool {
    state.update(|s| s.mark_submitted());
    state.get_untracked().is_valid()
}

/// Get a derived signal for a field's visible error.
///
/// Returns `None` if the field hasn't been touched and form hasn't been submitted.
pub fn use_field_error(state: RwSignal<FormState>, field: &'static str) -> Signal<Option<String>> {
    Signal::derive(move || state.get().get_visible_error(field).cloned())
}

/// Common validation rule sets
pub mod rules {
    use super::ValidationRule;

    /// Reserved prefixes for adapter names (matches server validation)
    const RESERVED_ADAPTER_PREFIXES: &[&str] = &["system-", "internal-", "reserved-"];

    /// Rules for adapter names
    ///
    /// Matches server validation in adapteros-core/src/validation/mod.rs:
    /// - Not empty
    /// - Maximum 64 characters
    /// - Alphanumeric with hyphens and underscores
    /// - Must start and end with alphanumeric character
    /// - No consecutive hyphens/underscores
    /// - Cannot use reserved prefixes (system-, internal-, reserved-)
    pub fn adapter_name() -> Vec<ValidationRule> {
        vec![
            ValidationRule::Required,
            ValidationRule::MinLength(1),
            ValidationRule::MaxLength(64),
            ValidationRule::Pattern {
                pattern: r"^[a-zA-Z0-9][a-zA-Z0-9_-]*[a-zA-Z0-9]$|^[a-zA-Z0-9]$",
                message: "Must start and end with alphanumeric character, contain only letters, numbers, underscores, and hyphens",
            },
            // Custom rule for no consecutive hyphens/underscores
            ValidationRule::Custom {
                validator: |s| {
                    !s.contains("--") && !s.contains("__") && !s.contains("-_") && !s.contains("_-")
                },
                message: "Cannot contain consecutive hyphens or underscores",
            },
            // Custom rule for reserved prefixes
            ValidationRule::Custom {
                validator: |s| {
                    let lower = s.to_lowercase();
                    !RESERVED_ADAPTER_PREFIXES.iter().any(|prefix| lower.starts_with(prefix))
                },
                message: "Cannot start with reserved prefix (system-, internal-, reserved-)",
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
    ///
    /// Matches server validation in adapteros-core/src/validation/mod.rs:
    /// - Maximum 1024 characters
    pub fn description() -> Vec<ValidationRule> {
        vec![ValidationRule::MaxLength(1024)]
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

    #[test]
    fn test_form_state_touched() {
        let mut state = FormState::new();
        assert!(!state.is_touched("email"));
        assert!(!state.should_show_error("email"));

        state.touch("email");
        assert!(state.is_touched("email"));
        assert!(state.should_show_error("email"));
        assert!(!state.is_touched("name")); // Other fields unaffected
    }

    #[test]
    fn test_form_state_visible_errors() {
        let mut state = FormState::new();
        state.set_error("email", "Invalid email".to_string());

        // Error exists but shouldn't be visible until touched
        assert!(state.get_error("email").is_some());
        assert!(state.get_visible_error("email").is_none());
        assert!(!state.has_visible_error("email"));

        // After touching, error becomes visible
        state.touch("email");
        assert!(state.get_visible_error("email").is_some());
        assert!(state.has_visible_error("email"));
    }

    #[test]
    fn test_form_state_submitted() {
        let mut state = FormState::new();
        state.set_error("email", "Invalid email".to_string());
        state.set_error("name", "Name required".to_string());

        // Before submit, errors are hidden (not touched)
        assert!(state.get_visible_error("email").is_none());
        assert!(state.get_visible_error("name").is_none());
        assert!(!state.has_any_visible_error());

        // After submit, all errors become visible
        state.mark_submitted();
        assert!(state.is_submitted());
        assert!(state.get_visible_error("email").is_some());
        assert!(state.get_visible_error("name").is_some());
        assert!(state.has_any_visible_error());
    }

    #[test]
    fn test_form_state_clear_all() {
        let mut state = FormState::new();
        state.touch("email");
        state.set_error("email", "Invalid".to_string());
        state.mark_submitted();

        assert!(state.is_touched("email"));
        assert!(state.is_submitted());
        assert!(state.get_error("email").is_some());

        state.clear_all();
        assert!(!state.is_touched("email"));
        assert!(!state.is_submitted());
        assert!(state.get_error("email").is_none());
    }

    #[test]
    fn test_form_state_reset_for_retry() {
        let mut state = FormState::new();
        state.touch("email");
        state.set_error("email", "Invalid".to_string());
        state.mark_submitted();

        state.reset_for_retry();

        // Touched state preserved, errors and submitted cleared
        assert!(state.is_touched("email"));
        assert!(!state.is_submitted());
        assert!(state.get_error("email").is_none());
    }

    #[test]
    fn test_min_length_counts_characters_not_bytes() {
        // Test with multi-byte UTF-8 characters
        let rule = ValidationRule::MinLength(3);
        // 3 emoji = 3 characters but 12 bytes
        assert!(rule.validate("\u{1F600}\u{1F600}\u{1F600}").is_none()); // 3 chars: valid
        assert!(rule.validate("\u{1F600}\u{1F600}").is_some()); // 2 chars: invalid
                                                                // Chinese characters: 3 chars but 9 bytes
        assert!(rule.validate("\u{4E2D}\u{6587}\u{5B57}").is_none()); // 3 chars: valid
        assert!(rule.validate("\u{4E2D}\u{6587}").is_some()); // 2 chars: invalid
    }

    #[test]
    fn test_max_length_counts_characters_not_bytes() {
        // Test with multi-byte UTF-8 characters
        let rule = ValidationRule::MaxLength(3);
        // 3 emoji = 3 characters but 12 bytes
        assert!(rule.validate("\u{1F600}\u{1F600}\u{1F600}").is_none()); // 3 chars: valid
        assert!(rule
            .validate("\u{1F600}\u{1F600}\u{1F600}\u{1F600}")
            .is_some()); // 4 chars: invalid
                         // Chinese characters
        assert!(rule.validate("\u{4E2D}\u{6587}\u{5B57}").is_none()); // 3 chars: valid
        assert!(rule.validate("\u{4E2D}\u{6587}\u{5B57}\u{7B26}").is_some()); // 4 chars: invalid
    }

    #[test]
    fn test_adapter_name_rules_basic() {
        let adapter_rules = rules::adapter_name();

        // Valid adapter names
        assert!(validate_field("a", &adapter_rules).is_none()); // Single char
        assert!(validate_field("my-adapter", &adapter_rules).is_none());
        assert!(validate_field("my_adapter", &adapter_rules).is_none());
        assert!(validate_field("adapter123", &adapter_rules).is_none());
        assert!(validate_field("a1", &adapter_rules).is_none());
        assert!(validate_field("MyAdapter", &adapter_rules).is_none());
    }

    #[test]
    fn test_adapter_name_rules_max_length() {
        let adapter_rules = rules::adapter_name();

        // 64 chars should pass
        let valid_64 = "a".repeat(64);
        assert!(validate_field(&valid_64, &adapter_rules).is_none());

        // 65 chars should fail
        let invalid_65 = "a".repeat(65);
        assert!(validate_field(&invalid_65, &adapter_rules).is_some());
    }

    #[test]
    fn test_adapter_name_rules_must_start_end_alphanumeric() {
        let adapter_rules = rules::adapter_name();

        // Cannot start with hyphen/underscore
        assert!(validate_field("-adapter", &adapter_rules).is_some());
        assert!(validate_field("_adapter", &adapter_rules).is_some());

        // Cannot end with hyphen/underscore
        assert!(validate_field("adapter-", &adapter_rules).is_some());
        assert!(validate_field("adapter_", &adapter_rules).is_some());
    }

    #[test]
    fn test_adapter_name_rules_no_consecutive_separators() {
        let adapter_rules = rules::adapter_name();

        // No consecutive hyphens
        assert!(validate_field("my--adapter", &adapter_rules).is_some());

        // No consecutive underscores
        assert!(validate_field("my__adapter", &adapter_rules).is_some());

        // No mixed consecutive separators
        assert!(validate_field("my-_adapter", &adapter_rules).is_some());
        assert!(validate_field("my_-adapter", &adapter_rules).is_some());
    }

    #[test]
    fn test_adapter_name_rules_reserved_prefixes() {
        let adapter_rules = rules::adapter_name();

        // Reserved prefixes should fail (case-insensitive)
        assert!(validate_field("system-foo", &adapter_rules).is_some());
        assert!(validate_field("System-Foo", &adapter_rules).is_some());
        assert!(validate_field("SYSTEM-FOO", &adapter_rules).is_some());
        assert!(validate_field("internal-bar", &adapter_rules).is_some());
        assert!(validate_field("Internal-Bar", &adapter_rules).is_some());
        assert!(validate_field("reserved-baz", &adapter_rules).is_some());
        assert!(validate_field("Reserved-Baz", &adapter_rules).is_some());

        // Similar but not reserved prefixes should pass
        assert!(validate_field("systems", &adapter_rules).is_none());
        assert!(validate_field("internals", &adapter_rules).is_none());
        assert!(validate_field("reservations", &adapter_rules).is_none());
    }

    #[test]
    fn test_description_rules_max_length() {
        let desc_rules = rules::description();

        // 1024 chars should pass
        let valid_1024 = "a".repeat(1024);
        assert!(validate_field(&valid_1024, &desc_rules).is_none());

        // 1025 chars should fail
        let invalid_1025 = "a".repeat(1025);
        assert!(validate_field(&invalid_1025, &desc_rules).is_some());

        // Empty should pass (not required)
        assert!(validate_field("", &desc_rules).is_none());
    }

    #[test]
    fn test_pattern_rule_fail_closed() {
        // This test documents that Pattern rule returns error on invalid regex
        // rather than silently passing (fail-closed behavior)
        let rule = ValidationRule::Pattern {
            pattern: r"^valid$",
            message: "Must be valid",
        };
        // Valid regex should work
        assert!(rule.validate("valid").is_none());
        assert!(rule.validate("invalid").is_some());
    }

    #[test]
    fn test_pattern_rule_invalid_regex_fails_closed() {
        // Invalid regex should fail validation (not pass silently)
        let rule = ValidationRule::Pattern {
            pattern: r"[invalid(regex", // Unclosed bracket - invalid regex
            message: "Pattern error",
        };
        // Should return error message, not pass
        assert!(rule.validate("anything").is_some());
    }
}
