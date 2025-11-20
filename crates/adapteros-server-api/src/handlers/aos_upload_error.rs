//! Specific error types for .aos adapter upload operations
//!
//! Provides detailed, actionable error messages for file upload, storage, and
//! database registration operations. Each error variant maps to appropriate HTTP
//! status codes and user-friendly messages.
//!
//! Error Message Strategy:
//! - Disk errors → specific I/O context (disk full, permission denied, etc.)
//! - Database errors → operation-specific details (constraint violation, connection)
//! - Format errors → field-level validation feedback with recommendations
//! - Security errors → sanitized messages (no path leaks, no sensitive context)

use axum::http::StatusCode;
use std::io;
use thiserror::Error;

/// Specific errors that can occur during .aos adapter upload operations
///
/// Each variant includes context-specific information to help users understand
/// what went wrong and how to fix it.
#[derive(Error, Debug)]
pub enum AosUploadError {
    /// File size exceeds maximum allowed size
    ///
    /// # Example
    /// `File exceeds maximum size of 1024MB (received: 1500MB)`
    #[error("File exceeds maximum size of {max_mb}MB (received: {actual_mb}MB)")]
    FileTooLarge { max_mb: u64, actual_mb: u64 },

    /// Disk storage is full, cannot write file
    ///
    /// # Context
    /// Typically indicates /adapters partition has insufficient space.
    /// Includes remaining space estimate.
    ///
    /// # Example
    /// `Insufficient disk space: need 250MB but only 50MB available`
    #[error("Insufficient disk space: need {required_mb}MB but only {available_mb}MB available")]
    DiskFull { required_mb: u64, available_mb: u64 },

    /// Permission denied when writing file or creating directory
    ///
    /// # Context
    /// Process lacks write permissions on /adapters directory or file.
    /// Common causes: incorrect file ownership, umask restrictions.
    ///
    /// # Example
    /// `Permission denied: cannot write to ./adapters directory (check directory ownership and permissions)`
    #[error("Permission denied: cannot write to ./adapters directory (check directory ownership and permissions)")]
    PermissionDenied,

    /// Invalid .aos file format or content
    ///
    /// # Context
    /// File doesn't match expected .aos archive structure.
    /// The file might be corrupted, truncated, or wrong format entirely.
    ///
    /// # Example
    /// `Invalid .aos file format: expected manifest at offset {offset}, got {bytes_available} bytes`
    #[error("Invalid .aos file format: {reason}")]
    InvalidFormat { reason: String },

    /// File extension is not .aos
    ///
    /// # Example
    /// `Invalid file extension: expected .aos, got .txt`
    #[error("Invalid file extension: expected .aos, got .{extension}")]
    InvalidExtension { extension: String },

    /// File hash verification failed (corruption detected)
    ///
    /// # Context
    /// File was written successfully but content doesn't match calculated hash.
    /// File is deleted automatically after detection.
    ///
    /// # Example
    /// `File integrity check failed: hash mismatch (expected: abc123..., got: def456...)`
    #[error("File integrity check failed: hash mismatch (expected: {expected}, got: {actual})")]
    HashMismatch { expected: String, actual: String },

    /// Adapter name validation failed
    ///
    /// # Variants
    /// - TooLong: name exceeds 256 characters
    /// - Empty: name is empty string
    /// - InvalidChars: name contains disallowed characters
    ///
    /// # Example
    /// `Invalid adapter name: exceeds maximum length (256 chars max, got 512)`
    #[error("Invalid adapter name: {reason}")]
    InvalidAdapterName { reason: String },

    /// Multipart form parsing failed
    ///
    /// # Context
    /// Request body is malformed or missing required fields
    ///
    /// # Example
    /// `Invalid request format: missing file field`
    #[error("Invalid request format: {reason}")]
    InvalidRequest { reason: String },

    /// Database constraint violation during registration
    ///
    /// # Variants
    /// - AdapterIdExists: adapter_id already registered
    /// - UniqueHashConflict: file hash already exists
    /// - ForeignKeyViolation: referenced tenant doesn't exist
    ///
    /// # Example
    /// `Adapter ID already exists: 'adapter_xyz123' is already registered in this tenant`
    #[error("Database constraint violation: {reason}")]
    DatabaseConstraintViolation { reason: String },

    /// Database connection error
    ///
    /// # Context
    /// Unable to connect to database or query timed out
    ///
    /// # Example
    /// `Database connection failed: timeout after 30s waiting for connection pool`
    #[error("Database connection failed: {reason}")]
    DatabaseConnection { reason: String },

    /// Generic database operation error
    ///
    /// # Context
    /// Other database-related failures not covered by specific variants
    ///
    /// # Example
    /// `Database error: failed to insert adapter metadata: {details}`
    #[error("Database error: {reason}")]
    DatabaseOperation { reason: String },

    /// Temporary file operations failed
    ///
    /// # Context
    /// Cannot create, write, or rename temporary files during atomic upload
    ///
    /// # Example
    /// `Temporary file error: cannot create temp file in ./adapters directory`
    #[error("Temporary file error: {reason}")]
    TemporaryFileFailed { reason: String },

    /// Path normalization or validation failed
    ///
    /// # Context
    /// Path contains traversal attempts (../, etc.) or invalid characters
    ///
    /// # Example
    /// `Invalid file path: path traversal detected`
    #[error("Invalid file path: {reason}")]
    InvalidPath { reason: String },

    /// UUID collision on adapter ID (exhausted retries)
    ///
    /// # Context
    /// Generated UUID already exists in database after multiple retry attempts
    /// This is extremely rare (< 1 in 10 billion), indicates UUID RNG issue
    ///
    /// # Example
    /// `Failed to generate unique adapter ID after 3 attempts: check UUID generation`
    #[error(
        "Failed to generate unique adapter ID after {attempts} attempts: check UUID generation"
    )]
    UniqueIdGenerationFailed { attempts: usize },

    /// Rank parameter validation failed
    ///
    /// # Example
    /// `Invalid rank value: must be between 1 and 512, got 1024`
    #[error("Invalid rank value: must be between {min} and {max}, got {actual}")]
    InvalidRank { min: i32, max: i32, actual: i32 },

    /// Alpha parameter validation failed
    ///
    /// # Example
    /// `Invalid alpha value: must be between 0.0 and 100.0, got 150.5`
    #[error("Invalid alpha value: must be between {min} and {max}, got {actual}")]
    InvalidAlpha { min: f64, max: f64, actual: f64 },

    /// Enumeration value not recognized (tier, category, scope)
    ///
    /// # Example
    /// `Invalid tier value 'super_fast': must be one of: ephemeral, warm, persistent`
    #[error("Invalid {field} value '{value}': must be one of: {valid_values}")]
    InvalidEnumValue {
        field: String,
        value: String,
        valid_values: String,
    },

    /// Generic/unknown error with fallback message
    ///
    /// Used when error type doesn't match specific variants.
    /// Includes original error for logging.
    #[error("Upload failed: {message}")]
    Other { message: String },
}

impl AosUploadError {
    /// Map this upload error to an appropriate HTTP status code
    ///
    /// Status codes:
    /// - 400 Bad Request: invalid input (format, validation, etc.)
    /// - 409 Conflict: database constraint violation
    /// - 413 Payload Too Large: file exceeds size limit
    /// - 422 Unprocessable Entity: semantic validation errors (after parsing)
    /// - 507 Insufficient Storage: disk full
    /// - 500 Internal Server Error: server-side failures
    pub fn status_code(&self) -> StatusCode {
        match self {
            // 400 Bad Request: input validation issues
            AosUploadError::InvalidExtension { .. }
            | AosUploadError::InvalidRequest { .. }
            | AosUploadError::InvalidAdapterName { .. }
            | AosUploadError::InvalidPath { .. }
            | AosUploadError::InvalidFormat { .. }
            | AosUploadError::InvalidRank { .. }
            | AosUploadError::InvalidAlpha { .. }
            | AosUploadError::InvalidEnumValue { .. } => StatusCode::BAD_REQUEST,

            // 409 Conflict: database constraint violations
            AosUploadError::DatabaseConstraintViolation { .. } => StatusCode::CONFLICT,

            // 413 Payload Too Large: file size violation
            AosUploadError::FileTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,

            // 507 Insufficient Storage: disk space issues
            AosUploadError::DiskFull { .. } => StatusCode::INSUFFICIENT_STORAGE,

            // 500 Internal Server Error: server-side failures
            AosUploadError::PermissionDenied
            | AosUploadError::HashMismatch { .. }
            | AosUploadError::DatabaseConnection { .. }
            | AosUploadError::DatabaseOperation { .. }
            | AosUploadError::TemporaryFileFailed { .. }
            | AosUploadError::UniqueIdGenerationFailed { .. }
            | AosUploadError::Other { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Get an error code for the response (for API clients)
    ///
    /// Used in structured error responses for programmatic handling
    pub fn error_code(&self) -> &'static str {
        match self {
            AosUploadError::FileTooLarge { .. } => "AOS_FILE_TOO_LARGE",
            AosUploadError::DiskFull { .. } => "AOS_DISK_FULL",
            AosUploadError::PermissionDenied => "AOS_PERMISSION_DENIED",
            AosUploadError::InvalidFormat { .. } => "AOS_INVALID_FORMAT",
            AosUploadError::InvalidExtension { .. } => "AOS_INVALID_EXTENSION",
            AosUploadError::HashMismatch { .. } => "AOS_HASH_MISMATCH",
            AosUploadError::InvalidAdapterName { .. } => "AOS_INVALID_NAME",
            AosUploadError::InvalidRequest { .. } => "AOS_INVALID_REQUEST",
            AosUploadError::DatabaseConstraintViolation { .. } => "AOS_DB_CONSTRAINT",
            AosUploadError::DatabaseConnection { .. } => "AOS_DB_CONNECTION",
            AosUploadError::DatabaseOperation { .. } => "AOS_DB_OPERATION",
            AosUploadError::TemporaryFileFailed { .. } => "AOS_TEMP_FILE_FAILED",
            AosUploadError::InvalidPath { .. } => "AOS_INVALID_PATH",
            AosUploadError::UniqueIdGenerationFailed { .. } => "AOS_ID_GEN_FAILED",
            AosUploadError::InvalidRank { .. } => "AOS_INVALID_RANK",
            AosUploadError::InvalidAlpha { .. } => "AOS_INVALID_ALPHA",
            AosUploadError::InvalidEnumValue { .. } => "AOS_INVALID_ENUM",
            AosUploadError::Other { .. } => "AOS_UNKNOWN_ERROR",
        }
    }

    /// Check if this is a retryable error
    ///
    /// Some errors (transient network/disk issues) may succeed on retry.
    /// Others (validation errors) will never succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            AosUploadError::DiskFull { .. }
                | AosUploadError::DatabaseConnection { .. }
                | AosUploadError::TemporaryFileFailed { .. }
        )
    }

    /// Check if sensitive information might leak in error message
    ///
    /// Some errors contain file paths or system details that shouldn't
    /// be exposed to untrusted clients.
    pub fn may_leak_sensitive_info(&self) -> bool {
        matches!(
            self,
            AosUploadError::PermissionDenied
                | AosUploadError::InvalidPath { .. }
                | AosUploadError::TemporaryFileFailed { .. }
        )
    }
}

/// Convert IO errors to specific AosUploadError variants
///
/// Maps OS-level IO error kinds to user-friendly variants with context
pub fn io_error_to_upload_error(error: &io::Error, context: &str) -> AosUploadError {
    match error.kind() {
        io::ErrorKind::NotFound => AosUploadError::TemporaryFileFailed {
            reason: format!("File not found: {}", context),
        },
        io::ErrorKind::PermissionDenied => AosUploadError::PermissionDenied,
        io::ErrorKind::Interrupted => AosUploadError::TemporaryFileFailed {
            reason: format!("Operation interrupted: {}", context),
        },
        io::ErrorKind::Other if error.os_error() == Some(28) => {
            // ENOSPC (No space left on device)
            AosUploadError::DiskFull {
                required_mb: 0, // Not known at this point
                available_mb: 0,
            }
        }
        io::ErrorKind::InvalidInput => AosUploadError::InvalidPath {
            reason: "Invalid path or filename".to_string(),
        },
        io::ErrorKind::InvalidData => AosUploadError::InvalidFormat {
            reason: format!("Invalid file data: {}", context),
        },
        io::ErrorKind::TimedOut => AosUploadError::TemporaryFileFailed {
            reason: format!("File operation timed out: {}", context),
        },
        _ => AosUploadError::Other {
            message: format!("File operation failed ({}): {}", context, error),
        },
    }
}

/// Result type for upload operations
pub type UploadResult<T> = std::result::Result<T, AosUploadError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_codes() {
        let file_too_large = AosUploadError::FileTooLarge {
            max_mb: 1024,
            actual_mb: 2048,
        };
        assert_eq!(file_too_large.status_code(), StatusCode::PAYLOAD_TOO_LARGE);

        let disk_full = AosUploadError::DiskFull {
            required_mb: 250,
            available_mb: 50,
        };
        assert_eq!(disk_full.status_code(), StatusCode::INSUFFICIENT_STORAGE);

        let invalid_ext = AosUploadError::InvalidExtension {
            extension: "txt".to_string(),
        };
        assert_eq!(invalid_ext.status_code(), StatusCode::BAD_REQUEST);

        let constraint = AosUploadError::DatabaseConstraintViolation {
            reason: "test".to_string(),
        };
        assert_eq!(constraint.status_code(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_retryable_errors() {
        assert!(AosUploadError::DiskFull {
            required_mb: 100,
            available_mb: 10
        }
        .is_retryable());
        assert!(AosUploadError::DatabaseConnection {
            reason: "timeout".to_string()
        }
        .is_retryable());

        assert!(!AosUploadError::InvalidExtension {
            extension: "txt".to_string()
        }
        .is_retryable());
    }
}
