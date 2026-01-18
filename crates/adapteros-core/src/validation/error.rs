//! Validation error types
//!
//! Provides structured error information for validation failures.

use std::fmt;

/// A validation error with detailed context.
///
/// This error type provides:
/// - A field name indicating what was being validated
/// - A human-readable message describing the failure
/// - An optional violation code for programmatic handling
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// The field or context being validated
    pub field: String,
    /// Human-readable error message
    pub message: String,
    /// Optional error code for programmatic handling
    pub code: Option<ValidationErrorCode>,
}

impl ValidationError {
    /// Create a new validation error with a message.
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            code: None,
        }
    }

    /// Create a validation error with an error code.
    pub fn with_code(
        field: impl Into<String>,
        message: impl Into<String>,
        code: ValidationErrorCode,
    ) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            code: Some(code),
        }
    }

    /// Create an "empty" validation error.
    pub fn empty(field: impl Into<String>) -> Self {
        let field = field.into();
        Self {
            message: format!("{} cannot be empty", field),
            field,
            code: Some(ValidationErrorCode::Empty),
        }
    }

    /// Create a "too short" validation error.
    pub fn too_short(field: impl Into<String>, min: usize, actual: usize) -> Self {
        let field = field.into();
        Self {
            message: format!(
                "{} must be at least {} characters (got {})",
                field, min, actual
            ),
            field,
            code: Some(ValidationErrorCode::TooShort),
        }
    }

    /// Create a "too long" validation error.
    pub fn too_long(field: impl Into<String>, max: usize, actual: usize) -> Self {
        let field = field.into();
        Self {
            message: format!(
                "{} must be at most {} characters (got {})",
                field, max, actual
            ),
            field,
            code: Some(ValidationErrorCode::TooLong),
        }
    }

    /// Create an "invalid characters" validation error.
    pub fn invalid_chars(field: impl Into<String>, invalid: &[char]) -> Self {
        let field = field.into();
        Self {
            message: format!("{} contains invalid characters: {:?}", field, invalid),
            field,
            code: Some(ValidationErrorCode::InvalidCharacters),
        }
    }

    /// Create a "reserved word" validation error.
    pub fn reserved_word(field: impl Into<String>, word: &str) -> Self {
        let field = field.into();
        Self {
            message: format!("{} uses reserved word: '{}'", field, word),
            field,
            code: Some(ValidationErrorCode::ReservedWord),
        }
    }

    /// Create a "pattern mismatch" validation error.
    pub fn pattern_mismatch(field: impl Into<String>, expected: &str) -> Self {
        let field = field.into();
        Self {
            message: format!("{} does not match expected pattern: {}", field, expected),
            field,
            code: Some(ValidationErrorCode::PatternMismatch),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Categorized validation error codes.
///
/// These codes allow programmatic handling of specific validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidationErrorCode {
    /// Value is empty when it shouldn't be
    Empty,
    /// Value is shorter than minimum length
    TooShort,
    /// Value is longer than maximum length
    TooLong,
    /// Value contains disallowed characters
    InvalidCharacters,
    /// Value uses a reserved/forbidden word
    ReservedWord,
    /// Value doesn't match required pattern
    PatternMismatch,
    /// Value doesn't start with required prefix/character
    InvalidStart,
    /// Value doesn't end with required suffix/character
    InvalidEnd,
    /// Value contains consecutive special characters
    ConsecutiveSpecialChars,
    /// Custom validation rule failed
    Custom,
}

impl fmt::Display for ValidationErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "EMPTY"),
            Self::TooShort => write!(f, "TOO_SHORT"),
            Self::TooLong => write!(f, "TOO_LONG"),
            Self::InvalidCharacters => write!(f, "INVALID_CHARS"),
            Self::ReservedWord => write!(f, "RESERVED_WORD"),
            Self::PatternMismatch => write!(f, "PATTERN_MISMATCH"),
            Self::InvalidStart => write!(f, "INVALID_START"),
            Self::InvalidEnd => write!(f, "INVALID_END"),
            Self::ConsecutiveSpecialChars => write!(f, "CONSECUTIVE_SPECIAL"),
            Self::Custom => write!(f, "CUSTOM"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_new() {
        let err = ValidationError::new("adapter_id", "Invalid adapter ID");
        assert_eq!(err.field, "adapter_id");
        assert_eq!(err.message, "Invalid adapter ID");
        assert!(err.code.is_none());
    }

    #[test]
    fn test_validation_error_with_code() {
        let err = ValidationError::with_code("name", "Name is empty", ValidationErrorCode::Empty);
        assert_eq!(err.field, "name");
        assert_eq!(err.code, Some(ValidationErrorCode::Empty));
    }

    #[test]
    fn test_validation_error_constructors() {
        let empty = ValidationError::empty("field");
        assert!(empty.message.contains("cannot be empty"));

        let short = ValidationError::too_short("field", 5, 3);
        assert!(short.message.contains("at least 5"));

        let long = ValidationError::too_long("field", 10, 15);
        assert!(long.message.contains("at most 10"));
    }

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError::new("test", "Test message");
        assert_eq!(format!("{}", err), "Test message");
    }
}
