//! Dataset validation handlers and structural validation rules.
//!
//! This module provides comprehensive validation for datasets including:
//! - Structural validation (required files, directories, format checks)
//! - Content validation (JSON/NDJSON parsing, required fields)
//! - Integrity validation (file hashes, size limits)
//! - Composable validation rules with clear error messages
//!
//! # Architecture
//!
//! The validation system is built around composable `ValidationRule` trait implementations
//! that can be combined using `CompositeValidator`. Each rule produces detailed
//! `ValidationError` instances that can be aggregated into a `DatasetValidationResult`.
//!
//! # Quick vs Deep Validation
//!
//! - **Quick validation**: Fast checks suitable for upload-time validation (format detection,
//!   size limits, basic structure)
//! - **Deep validation**: Comprehensive checks including full file parsing, hash verification,
//!   and semantic validation

use super::helpers::{
    map_validation_status, spawn_tier2_safety_validation, validate_file_hash_streaming,
    STREAM_BUFFER_SIZE,
};
use super::progress::emit_progress;
use crate::auth::Claims;
use crate::error_helpers::{db_error, forbidden, not_found};
use crate::handlers::chunked_upload::FileValidator;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{ErrorResponse, ValidateDatasetRequest, ValidateDatasetResponse};
use adapteros_core::seed::derive_seed;
use adapteros_core::B3Hash;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path as StdPath;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use utoipa::ToSchema;

// ============================================================================
// Validation Types
// ============================================================================

/// Severity level for validation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    /// Critical error that prevents dataset from being used
    Error,
    /// Warning that should be addressed but doesn't block usage
    Warning,
    /// Informational message about potential improvements
    Info,
}

impl std::fmt::Display for ValidationSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationSeverity::Error => write!(f, "error"),
            ValidationSeverity::Warning => write!(f, "warning"),
            ValidationSeverity::Info => write!(f, "info"),
        }
    }
}

/// Category of validation error for filtering and reporting
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidationCategory {
    /// File structure issues (missing files, wrong directories)
    Structure,
    /// Format issues (invalid JSON, wrong file type)
    Format,
    /// Required field issues
    Schema,
    /// Size limit violations
    Size,
    /// File type/extension issues
    FileType,
    /// Hash/integrity issues
    Integrity,
    /// Encoding issues (non-UTF8, BOM)
    Encoding,
    /// Content quality issues
    Content,
}

impl std::fmt::Display for ValidationCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationCategory::Structure => write!(f, "structure"),
            ValidationCategory::Format => write!(f, "format"),
            ValidationCategory::Schema => write!(f, "schema"),
            ValidationCategory::Size => write!(f, "size"),
            ValidationCategory::FileType => write!(f, "file_type"),
            ValidationCategory::Integrity => write!(f, "integrity"),
            ValidationCategory::Encoding => write!(f, "encoding"),
            ValidationCategory::Content => write!(f, "content"),
        }
    }
}

/// A single validation error with detailed context
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationError {
    /// Error severity
    pub severity: ValidationSeverity,
    /// Error category for filtering
    pub category: ValidationCategory,
    /// Human-readable error message
    pub message: String,
    /// File path where error occurred (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Line number where error occurred (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<usize>,
    /// Column number where error occurred (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_number: Option<usize>,
    /// Field name that caused the error (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
    /// Suggested fix for the error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    /// Error code for programmatic handling
    pub code: String,
}

impl ValidationError {
    /// Create a new validation error
    pub fn new(
        severity: ValidationSeverity,
        category: ValidationCategory,
        message: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            category,
            message: message.into(),
            file_path: None,
            line_number: None,
            column_number: None,
            field_name: None,
            suggestion: None,
            code: code.into(),
        }
    }

    /// Add file path context
    pub fn with_file(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Add line number context
    pub fn with_line(mut self, line: usize) -> Self {
        self.line_number = Some(line);
        self
    }

    /// Add column number context
    pub fn with_column(mut self, column: usize) -> Self {
        self.column_number = Some(column);
        self
    }

    /// Add field name context
    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field_name = Some(field.into());
        self
    }

    /// Add suggestion for fixing the error
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Format error as a single-line string for logging
    pub fn to_log_string(&self) -> String {
        let mut parts = vec![format!(
            "[{}] {}: {}",
            self.severity, self.category, self.message
        )];

        if let Some(ref path) = self.file_path {
            parts.push(format!("file={}", path));
        }
        if let Some(line) = self.line_number {
            parts.push(format!("line={}", line));
        }
        if let Some(col) = self.column_number {
            parts.push(format!("col={}", col));
        }

        parts.join(" ")
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;

        if let Some(ref path) = self.file_path {
            write!(f, " (in {})", path)?;
        }

        if let Some(line) = self.line_number {
            if let Some(col) = self.column_number {
                write!(f, " at line {}, column {}", line, col)?;
            } else {
                write!(f, " at line {}", line)?;
            }
        }

        Ok(())
    }
}

/// Aggregate result of dataset validation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatasetValidationResult {
    /// Whether the dataset passed all error-level validations
    pub is_valid: bool,
    /// Total number of errors found
    pub error_count: usize,
    /// Total number of warnings found
    pub warning_count: usize,
    /// Total number of info messages
    pub info_count: usize,
    /// All validation errors/warnings/info
    pub errors: Vec<ValidationError>,
    /// Total files validated
    pub files_validated: usize,
    /// Total rows/entries validated (for JSONL/JSON)
    pub entries_validated: usize,
    /// Validation duration in milliseconds
    pub duration_ms: u64,
    /// Validation mode used (quick or deep)
    pub mode: ValidationMode,
}

impl Default for DatasetValidationResult {
    fn default() -> Self {
        Self {
            is_valid: true,
            error_count: 0,
            warning_count: 0,
            info_count: 0,
            errors: Vec::new(),
            files_validated: 0,
            entries_validated: 0,
            duration_ms: 0,
            mode: ValidationMode::Quick,
        }
    }
}

impl DatasetValidationResult {
    /// Create a new empty result
    pub fn new(mode: ValidationMode) -> Self {
        Self {
            mode,
            ..Default::default()
        }
    }

    /// Add a validation error
    pub fn add_error(&mut self, error: ValidationError) {
        match error.severity {
            ValidationSeverity::Error => {
                self.error_count += 1;
                self.is_valid = false;
            }
            ValidationSeverity::Warning => self.warning_count += 1,
            ValidationSeverity::Info => self.info_count += 1,
        }
        self.errors.push(error);
    }

    /// Merge another result into this one
    pub fn merge(&mut self, other: DatasetValidationResult) {
        self.is_valid = self.is_valid && other.is_valid;
        self.error_count += other.error_count;
        self.warning_count += other.warning_count;
        self.info_count += other.info_count;
        self.files_validated += other.files_validated;
        self.entries_validated += other.entries_validated;
        self.errors.extend(other.errors);
    }

    /// Get only errors (not warnings or info)
    pub fn errors_only(&self) -> Vec<&ValidationError> {
        self.errors
            .iter()
            .filter(|e| e.severity == ValidationSeverity::Error)
            .collect()
    }

    /// Get errors by category
    pub fn errors_by_category(&self, category: &ValidationCategory) -> Vec<&ValidationError> {
        self.errors
            .iter()
            .filter(|e| &e.category == category)
            .collect()
    }

    /// Convert to simple error strings for backward compatibility
    pub fn to_error_strings(&self) -> Vec<String> {
        self.errors.iter().map(|e| e.to_string()).collect()
    }
}

/// Validation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidationMode {
    /// Fast validation suitable for upload-time checks
    Quick,
    /// Comprehensive validation including full content parsing
    Deep,
}

impl std::fmt::Display for ValidationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationMode::Quick => write!(f, "quick"),
            ValidationMode::Deep => write!(f, "deep"),
        }
    }
}

// ============================================================================
// Validation Rules
// ============================================================================

/// Configuration for dataset validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Maximum file size in bytes (default: 100MB)
    pub max_file_size: u64,
    /// Maximum total dataset size in bytes (default: 10GB)
    pub max_total_size: u64,
    /// Maximum number of files in dataset (default: 1000)
    pub max_file_count: usize,
    /// Maximum line length in bytes (default: 1MB)
    pub max_line_length: usize,
    /// Maximum entry count for deep validation (default: 100000)
    pub max_entry_count: usize,
    /// Required fields for JSONL entries
    pub required_fields: Vec<String>,
    /// Allowed file extensions
    pub allowed_extensions: HashSet<String>,
    /// Expected dataset format
    pub expected_format: String,
    /// Whether to validate file hashes
    pub validate_hashes: bool,
    /// Buffer size for streaming operations
    pub stream_buffer_size: usize,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        let mut allowed_extensions = HashSet::new();
        allowed_extensions.insert("jsonl".to_string());
        allowed_extensions.insert("ndjson".to_string());
        allowed_extensions.insert("json".to_string());
        allowed_extensions.insert("csv".to_string());
        allowed_extensions.insert("txt".to_string());

        Self {
            max_file_size: 100 * 1024 * 1024,        // 100MB
            max_total_size: 10 * 1024 * 1024 * 1024, // 10GB
            max_file_count: 1000,
            max_line_length: 1024 * 1024, // 1MB
            max_entry_count: 100_000,
            required_fields: vec![],
            allowed_extensions,
            expected_format: "jsonl".to_string(),
            validate_hashes: true,
            stream_buffer_size: STREAM_BUFFER_SIZE,
        }
    }
}

impl ValidationConfig {
    /// Create config for JSONL training datasets
    pub fn for_training_jsonl() -> Self {
        Self {
            required_fields: vec![
                "row_id".to_string(),
                "prompt".to_string(),
                "response".to_string(),
            ],
            expected_format: "jsonl".to_string(),
            ..Default::default()
        }
    }

    /// Create config for generic JSON datasets
    pub fn for_json() -> Self {
        Self {
            expected_format: "json".to_string(),
            ..Default::default()
        }
    }
}

/// Trait for composable validation rules
#[async_trait::async_trait]
pub trait ValidationRule: Send + Sync {
    /// Name of this validation rule
    fn name(&self) -> &str;

    /// Validate a file and return any errors
    async fn validate_file(
        &self,
        path: &StdPath,
        config: &ValidationConfig,
    ) -> Vec<ValidationError>;
}

/// Validates that files exist and are accessible
pub struct FileExistsRule;

#[async_trait::async_trait]
impl ValidationRule for FileExistsRule {
    fn name(&self) -> &str {
        "file_exists"
    }

    async fn validate_file(
        &self,
        path: &StdPath,
        _config: &ValidationConfig,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let path_str = path.display().to_string();

        match fs::try_exists(path).await {
            Ok(true) => {}
            Ok(false) => {
                errors.push(
                    ValidationError::new(
                        ValidationSeverity::Error,
                        ValidationCategory::Structure,
                        format!("File does not exist: {}", path_str),
                        "FILE_NOT_FOUND",
                    )
                    .with_file(&path_str)
                    .with_suggestion("Ensure the file was uploaded correctly"),
                );
            }
            Err(e) => {
                errors.push(
                    ValidationError::new(
                        ValidationSeverity::Error,
                        ValidationCategory::Structure,
                        format!("Cannot access file: {} ({})", path_str, e),
                        "FILE_ACCESS_ERROR",
                    )
                    .with_file(&path_str),
                );
            }
        }

        errors
    }
}

/// Validates file size limits
pub struct FileSizeRule;

#[async_trait::async_trait]
impl ValidationRule for FileSizeRule {
    fn name(&self) -> &str {
        "file_size"
    }

    async fn validate_file(
        &self,
        path: &StdPath,
        config: &ValidationConfig,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let path_str = path.display().to_string();

        match fs::metadata(path).await {
            Ok(metadata) => {
                let size = metadata.len();

                if size == 0 {
                    errors.push(
                        ValidationError::new(
                            ValidationSeverity::Error,
                            ValidationCategory::Size,
                            format!("File is empty: {}", path_str),
                            "FILE_EMPTY",
                        )
                        .with_file(&path_str)
                        .with_suggestion("Upload a non-empty file"),
                    );
                } else if size > config.max_file_size {
                    errors.push(
                        ValidationError::new(
                            ValidationSeverity::Error,
                            ValidationCategory::Size,
                            format!(
                                "File exceeds maximum size: {} bytes (max: {} bytes)",
                                size, config.max_file_size
                            ),
                            "FILE_TOO_LARGE",
                        )
                        .with_file(&path_str)
                        .with_suggestion("Split the file into smaller chunks"),
                    );
                }
            }
            Err(e) => {
                errors.push(
                    ValidationError::new(
                        ValidationSeverity::Error,
                        ValidationCategory::Structure,
                        format!("Cannot read file metadata: {}", e),
                        "METADATA_ERROR",
                    )
                    .with_file(&path_str),
                );
            }
        }

        errors
    }
}

/// Validates file extensions
pub struct FileExtensionRule;

#[async_trait::async_trait]
impl ValidationRule for FileExtensionRule {
    fn name(&self) -> &str {
        "file_extension"
    }

    async fn validate_file(
        &self,
        path: &StdPath,
        config: &ValidationConfig,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let path_str = path.display().to_string();

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if extension.is_empty() {
            errors.push(
                ValidationError::new(
                    ValidationSeverity::Warning,
                    ValidationCategory::FileType,
                    format!("File has no extension: {}", path_str),
                    "NO_EXTENSION",
                )
                .with_file(&path_str)
                .with_suggestion("Add a file extension like .jsonl, .json, or .csv"),
            );
        } else if !config.allowed_extensions.contains(&extension) {
            errors.push(
                ValidationError::new(
                    ValidationSeverity::Error,
                    ValidationCategory::FileType,
                    format!(
                        "Unsupported file extension: .{} (allowed: {})",
                        extension,
                        config
                            .allowed_extensions
                            .iter()
                            .map(|e| format!(".{}", e))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    "INVALID_EXTENSION",
                )
                .with_file(&path_str),
            );
        }

        errors
    }
}

/// Validates UTF-8 encoding
pub struct EncodingRule;

#[async_trait::async_trait]
impl ValidationRule for EncodingRule {
    fn name(&self) -> &str {
        "encoding"
    }

    async fn validate_file(
        &self,
        path: &StdPath,
        config: &ValidationConfig,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let path_str = path.display().to_string();

        // Read first chunk to check encoding
        let mut file = match fs::File::open(path).await {
            Ok(f) => f,
            Err(_) => return errors, // Skip if can't open (other rules catch this)
        };

        let mut buffer = vec![0u8; config.stream_buffer_size.min(64 * 1024)];
        let n = match file.read(&mut buffer).await {
            Ok(n) => n,
            Err(_) => return errors,
        };

        buffer.truncate(n);

        // Check for BOM
        if buffer.starts_with(&[0xEF, 0xBB, 0xBF]) {
            errors.push(
                ValidationError::new(
                    ValidationSeverity::Warning,
                    ValidationCategory::Encoding,
                    "File contains UTF-8 BOM (byte order mark)",
                    "UTF8_BOM",
                )
                .with_file(&path_str)
                .with_suggestion("Remove the BOM for better compatibility"),
            );
        }

        // Check for invalid UTF-8
        let content = String::from_utf8_lossy(&buffer);
        if content.contains('\u{FFFD}') {
            errors.push(
                ValidationError::new(
                    ValidationSeverity::Error,
                    ValidationCategory::Encoding,
                    "File contains invalid UTF-8 sequences",
                    "INVALID_UTF8",
                )
                .with_file(&path_str)
                .with_suggestion("Ensure the file is saved with UTF-8 encoding"),
            );
        }

        errors
    }
}

/// Validates JSONL format and structure
pub struct JsonlFormatRule;

#[async_trait::async_trait]
impl ValidationRule for JsonlFormatRule {
    fn name(&self) -> &str {
        "jsonl_format"
    }

    async fn validate_file(
        &self,
        path: &StdPath,
        config: &ValidationConfig,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let path_str = path.display().to_string();

        // Only validate JSONL files
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if !matches!(extension.as_str(), "jsonl" | "ndjson") {
            return errors;
        }

        let file = match fs::File::open(path).await {
            Ok(f) => f,
            Err(_) => return errors,
        };

        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut line_number = 0;
        let mut entry_count = 0;
        let mut seen_row_ids: HashSet<String> = HashSet::new();

        while let Ok(Some(line)) = lines.next_line().await {
            line_number += 1;

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Check line length
            if line.len() > config.max_line_length {
                errors.push(
                    ValidationError::new(
                        ValidationSeverity::Error,
                        ValidationCategory::Size,
                        format!(
                            "Line {} exceeds maximum length: {} bytes (max: {} bytes)",
                            line_number,
                            line.len(),
                            config.max_line_length
                        ),
                        "LINE_TOO_LONG",
                    )
                    .with_file(&path_str)
                    .with_line(line_number),
                );
                continue;
            }

            // Parse JSON
            match serde_json::from_str::<serde_json::Value>(&line) {
                Ok(value) => {
                    entry_count += 1;

                    // Check for required fields
                    if let Some(obj) = value.as_object() {
                        for field in &config.required_fields {
                            if !obj.contains_key(field) {
                                errors.push(
                                    ValidationError::new(
                                        ValidationSeverity::Error,
                                        ValidationCategory::Schema,
                                        format!("Missing required field: {}", field),
                                        "MISSING_FIELD",
                                    )
                                    .with_file(&path_str)
                                    .with_line(line_number)
                                    .with_field(field),
                                );
                            }
                        }

                        // Check for duplicate row_ids
                        if let Some(row_id) = obj.get("row_id").and_then(|v| v.as_str()) {
                            if !seen_row_ids.insert(row_id.to_string()) {
                                errors.push(
                                    ValidationError::new(
                                        ValidationSeverity::Error,
                                        ValidationCategory::Content,
                                        format!("Duplicate row_id: {}", row_id),
                                        "DUPLICATE_ROW_ID",
                                    )
                                    .with_file(&path_str)
                                    .with_line(line_number)
                                    .with_field("row_id"),
                                );
                            }
                        }
                    } else {
                        errors.push(
                            ValidationError::new(
                                ValidationSeverity::Error,
                                ValidationCategory::Format,
                                "JSONL entry is not an object",
                                "NOT_OBJECT",
                            )
                            .with_file(&path_str)
                            .with_line(line_number)
                            .with_suggestion("Each line should be a JSON object"),
                        );
                    }

                    // Limit entries validated in quick mode
                    if entry_count >= config.max_entry_count {
                        errors.push(
                            ValidationError::new(
                                ValidationSeverity::Info,
                                ValidationCategory::Size,
                                format!(
                                    "Validation stopped after {} entries (limit reached)",
                                    config.max_entry_count
                                ),
                                "ENTRY_LIMIT_REACHED",
                            )
                            .with_file(&path_str),
                        );
                        break;
                    }
                }
                Err(e) => {
                    errors.push(
                        ValidationError::new(
                            ValidationSeverity::Error,
                            ValidationCategory::Format,
                            format!("Invalid JSON: {}", e),
                            "INVALID_JSON",
                        )
                        .with_file(&path_str)
                        .with_line(line_number)
                        .with_column(e.column()),
                    );

                    // Limit number of parse errors to avoid overwhelming output
                    if errors.len() > 100 {
                        errors.push(
                            ValidationError::new(
                                ValidationSeverity::Info,
                                ValidationCategory::Format,
                                "Too many JSON parse errors, stopping validation",
                                "TOO_MANY_ERRORS",
                            )
                            .with_file(&path_str),
                        );
                        break;
                    }
                }
            }
        }

        if entry_count == 0 {
            errors.push(
                ValidationError::new(
                    ValidationSeverity::Error,
                    ValidationCategory::Content,
                    "JSONL file contains no valid entries",
                    "NO_ENTRIES",
                )
                .with_file(&path_str)
                .with_suggestion("Add at least one valid JSON object per line"),
            );
        }

        errors
    }
}

/// Validates JSON format
pub struct JsonFormatRule;

#[async_trait::async_trait]
impl ValidationRule for JsonFormatRule {
    fn name(&self) -> &str {
        "json_format"
    }

    async fn validate_file(
        &self,
        path: &StdPath,
        config: &ValidationConfig,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let path_str = path.display().to_string();

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if extension != "json" {
            return errors;
        }

        // Read file content
        let content = match fs::read_to_string(path).await {
            Ok(c) => c,
            Err(e) => {
                errors.push(
                    ValidationError::new(
                        ValidationSeverity::Error,
                        ValidationCategory::Structure,
                        format!("Cannot read file: {}", e),
                        "READ_ERROR",
                    )
                    .with_file(&path_str),
                );
                return errors;
            }
        };

        // Limit content size for validation
        if content.len() > config.max_file_size as usize {
            errors.push(
                ValidationError::new(
                    ValidationSeverity::Error,
                    ValidationCategory::Size,
                    format!(
                        "File content exceeds maximum size for validation: {} bytes",
                        content.len()
                    ),
                    "CONTENT_TOO_LARGE",
                )
                .with_file(&path_str),
            );
            return errors;
        }

        match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(value) => {
                // Check if it's an array of objects (common dataset format)
                if let Some(arr) = value.as_array() {
                    if arr.is_empty() {
                        errors.push(
                            ValidationError::new(
                                ValidationSeverity::Warning,
                                ValidationCategory::Content,
                                "JSON array is empty",
                                "EMPTY_ARRAY",
                            )
                            .with_file(&path_str),
                        );
                    } else {
                        // Validate required fields in array entries
                        for (idx, entry) in arr.iter().enumerate() {
                            if let Some(obj) = entry.as_object() {
                                for field in &config.required_fields {
                                    if !obj.contains_key(field) {
                                        errors.push(
                                            ValidationError::new(
                                                ValidationSeverity::Error,
                                                ValidationCategory::Schema,
                                                format!(
                                                    "Entry {} missing required field: {}",
                                                    idx, field
                                                ),
                                                "MISSING_FIELD",
                                            )
                                            .with_file(&path_str)
                                            .with_field(field),
                                        );
                                    }
                                }
                            }

                            // Limit validation
                            if idx >= config.max_entry_count {
                                break;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                errors.push(
                    ValidationError::new(
                        ValidationSeverity::Error,
                        ValidationCategory::Format,
                        format!("Invalid JSON: {}", e),
                        "INVALID_JSON",
                    )
                    .with_file(&path_str)
                    .with_line(e.line())
                    .with_column(e.column()),
                );
            }
        }

        errors
    }
}

// ============================================================================
// Composite Validator
// ============================================================================

/// Composite validator that runs multiple validation rules
pub struct CompositeValidator {
    rules: Vec<Box<dyn ValidationRule>>,
    config: ValidationConfig,
}

impl CompositeValidator {
    /// Create a new composite validator with default rules
    pub fn new(config: ValidationConfig) -> Self {
        Self {
            rules: Vec::new(),
            config,
        }
    }

    /// Create validator with standard rules for quick validation
    pub fn quick_validator(config: ValidationConfig) -> Self {
        let mut validator = Self::new(config);
        validator.add_rule(Box::new(FileExistsRule));
        validator.add_rule(Box::new(FileSizeRule));
        validator.add_rule(Box::new(FileExtensionRule));
        validator.add_rule(Box::new(EncodingRule));
        validator
    }

    /// Create validator with all rules for deep validation
    pub fn deep_validator(config: ValidationConfig) -> Self {
        let mut validator = Self::quick_validator(config);
        validator.add_rule(Box::new(JsonlFormatRule));
        validator.add_rule(Box::new(JsonFormatRule));
        validator
    }

    /// Add a validation rule
    pub fn add_rule(&mut self, rule: Box<dyn ValidationRule>) {
        self.rules.push(rule);
    }

    /// Validate a single file
    pub async fn validate_file(&self, path: &StdPath) -> DatasetValidationResult {
        let start = std::time::Instant::now();
        let mut result = DatasetValidationResult::new(ValidationMode::Deep);

        for rule in &self.rules {
            let errors = rule.validate_file(path, &self.config).await;
            for error in errors {
                result.add_error(error);
            }
        }

        result.files_validated = 1;
        result.duration_ms = start.elapsed().as_millis() as u64;
        result
    }

    /// Validate multiple files
    pub async fn validate_files(&self, paths: &[&StdPath]) -> DatasetValidationResult {
        let start = std::time::Instant::now();
        let mut result = DatasetValidationResult::new(ValidationMode::Deep);

        // Check total file count
        if paths.len() > self.config.max_file_count {
            result.add_error(
                ValidationError::new(
                    ValidationSeverity::Error,
                    ValidationCategory::Structure,
                    format!(
                        "Too many files: {} (max: {})",
                        paths.len(),
                        self.config.max_file_count
                    ),
                    "TOO_MANY_FILES",
                )
                .with_suggestion("Reduce the number of files or combine them"),
            );
        }

        for path in paths {
            let file_result = self.validate_file(path).await;
            result.merge(file_result);
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        result
    }
}

// ============================================================================
// Public Validation Functions
// ============================================================================

/// Perform quick validation on a dataset file
///
/// Quick validation checks:
/// - File exists and is accessible
/// - File is not empty
/// - File size within limits
/// - File extension is valid
/// - UTF-8 encoding
pub async fn quick_validate_file(
    path: &StdPath,
    config: Option<ValidationConfig>,
) -> DatasetValidationResult {
    let config = config.unwrap_or_default();
    let mut result = DatasetValidationResult::new(ValidationMode::Quick);
    let start = std::time::Instant::now();

    let validator = CompositeValidator::quick_validator(config);
    let file_result = validator.validate_file(path).await;
    result.merge(file_result);
    result.mode = ValidationMode::Quick;
    result.duration_ms = start.elapsed().as_millis() as u64;

    result
}

/// Perform deep validation on a dataset file
///
/// Deep validation includes all quick validation checks plus:
/// - JSON/JSONL parsing and format validation
/// - Required field checks
/// - Duplicate detection (row_id)
/// - Content quality checks
pub async fn deep_validate_file(
    path: &StdPath,
    config: Option<ValidationConfig>,
) -> DatasetValidationResult {
    let config = config.unwrap_or_default();
    let start = std::time::Instant::now();

    let validator = CompositeValidator::deep_validator(config);
    let mut result = validator.validate_file(path).await;
    result.mode = ValidationMode::Deep;
    result.duration_ms = start.elapsed().as_millis() as u64;

    result
}

/// Validate a dataset directory structure
///
/// Checks that expected files and directories are present
pub async fn validate_dataset_structure(
    root_path: &StdPath,
    expected_files: &[&str],
) -> DatasetValidationResult {
    let start = std::time::Instant::now();
    let mut result = DatasetValidationResult::new(ValidationMode::Quick);

    // Check root exists
    if !root_path.exists() {
        result.add_error(
            ValidationError::new(
                ValidationSeverity::Error,
                ValidationCategory::Structure,
                format!(
                    "Dataset root directory does not exist: {}",
                    root_path.display()
                ),
                "ROOT_NOT_FOUND",
            )
            .with_suggestion("Ensure the dataset was uploaded correctly"),
        );
        result.duration_ms = start.elapsed().as_millis() as u64;
        return result;
    }

    // Check expected files
    for file_name in expected_files {
        let file_path = root_path.join(file_name);
        if !file_path.exists() {
            result.add_error(
                ValidationError::new(
                    ValidationSeverity::Error,
                    ValidationCategory::Structure,
                    format!("Required file missing: {}", file_name),
                    "REQUIRED_FILE_MISSING",
                )
                .with_file(file_path.display().to_string()),
            );
        }
    }

    result.duration_ms = start.elapsed().as_millis() as u64;
    result
}

/// Validate file hash integrity
pub async fn validate_file_integrity(
    path: &StdPath,
    expected_hash: &str,
) -> DatasetValidationResult {
    let start = std::time::Instant::now();
    let mut result = DatasetValidationResult::new(ValidationMode::Quick);
    let path_str = path.display().to_string();

    match validate_file_hash_streaming(path, expected_hash).await {
        Ok(true) => {
            // Hash matches
        }
        Ok(false) => {
            result.add_error(
                ValidationError::new(
                    ValidationSeverity::Error,
                    ValidationCategory::Integrity,
                    "File hash does not match expected value",
                    "HASH_MISMATCH",
                )
                .with_file(&path_str)
                .with_suggestion("The file may have been corrupted during transfer"),
            );
        }
        Err(e) => {
            result.add_error(
                ValidationError::new(
                    ValidationSeverity::Error,
                    ValidationCategory::Integrity,
                    format!("Failed to compute file hash: {}", e),
                    "HASH_ERROR",
                )
                .with_file(&path_str),
            );
        }
    }

    result.files_validated = 1;
    result.duration_ms = start.elapsed().as_millis() as u64;
    result
}

// ============================================================================
// HTTP Handler
// ============================================================================

/// Validate a dataset
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/validate",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    request_body = ValidateDatasetRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateDatasetResponse),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn validate_dataset(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(request): Json<ValidateDatasetRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only validate their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id can only be validated by admins
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Set status to 'validating' at start
    state
        .db
        .update_dataset_validation(&dataset_id, "validating", None)
        .await
        .map_err(|e| db_error(format!("Failed to update validation status: {}", e)))?;

    // Send initial validation event
    emit_progress(
        state.dataset_progress_tx.as_ref(),
        &dataset_id,
        "validation",
        None,
        0.0,
        "Starting dataset validation...".to_string(),
        Some(dataset.file_count),
        Some(0),
    );

    // Get dataset files
    let files = state
        .db
        .get_dataset_files(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset files: {}", e)))?;

    let mut validation_errors = Vec::new();
    let mut is_valid = true;
    let total_files = files.len() as f32;
    let mut processed_files = 0;

    // Validate each file
    for file in &files {
        // Check file exists
        if !tokio::fs::try_exists(&file.file_path)
            .await
            .unwrap_or(false)
        {
            validation_errors.push(format!(
                "File {} does not exist at path {}",
                file.file_name, file.file_path
            ));
            is_valid = false;
            processed_files += 1;
            emit_progress(
                state.dataset_progress_tx.as_ref(),
                &dataset_id,
                "validation",
                Some(file.file_name.clone()),
                if total_files > 0.0 {
                    (processed_files as f32 / total_files) * 100.0
                } else {
                    0.0
                },
                format!("Validating {}", file.file_name),
                Some(files.len() as i32),
                Some(processed_files),
            );
            continue;
        }

        // Verify file hash with streaming to avoid loading entire file
        match validate_file_hash_streaming(std::path::Path::new(&file.file_path), &file.hash_b3)
            .await
        {
            Ok(matches) => {
                if !matches {
                    validation_errors.push(format!("File {} hash mismatch", file.file_name));
                    is_valid = false;
                }
            }
            Err(e) => {
                validation_errors
                    .push(format!("Failed to validate file {}: {}", file.file_name, e));
                is_valid = false;
                continue;
            }
        }

        // Format-specific validation with quick checks
        if request.check_format.unwrap_or(true) {
            if let Err(e) = FileValidator::quick_validate(
                std::path::Path::new(&file.file_path),
                &dataset.format,
                STREAM_BUFFER_SIZE,
            )
            .await
            {
                validation_errors.push(format!(
                    "File {} format validation failed: {}",
                    file.file_name, e
                ));
                is_valid = false;
            }
        }

        processed_files += 1;

        // Send progress event for this file
        emit_progress(
            state.dataset_progress_tx.as_ref(),
            &dataset_id,
            "validation",
            Some(file.file_name.clone()),
            if total_files > 0.0 {
                (processed_files as f32 / total_files) * 100.0
            } else {
                0.0
            },
            format!("Validated {}", file.file_name),
            Some(files.len() as i32),
            Some(processed_files),
        );
    }

    // Update validation status in database - set to "invalid" if validation failed
    let validation_status = if is_valid { "valid" } else { "invalid" };
    let validation_errors_str = if validation_errors.is_empty() {
        None
    } else {
        Some(validation_errors.join("; "))
    };

    state
        .db
        .update_dataset_validation(
            &dataset_id,
            validation_status,
            validation_errors_str.as_deref(),
        )
        .await
        .map_err(|e| {
            // On database error, try to reset status to 'invalid' to prevent stuck 'validating' state
            let db_clone = state.db.clone();
            let dataset_id_clone = dataset_id.clone();
            tokio::spawn(async move {
                let _ = db_clone
                    .update_dataset_validation(
                        &dataset_id_clone,
                        "invalid",
                        Some("Validation failed due to internal error"),
                    )
                    .await;
            });
            crate::error_helpers::internal_error(format!(
                "Failed to update validation status: {}",
                e
            ))
        })?;

    // Mirror structural validation into dataset version trust pipeline
    if let Ok(version_id) = state.db.ensure_dataset_version_exists(&dataset_id).await {
        let _ = state
            .db
            .update_dataset_version_structural_validation(
                &version_id,
                validation_status,
                validation_errors_str.as_deref(),
            )
            .await;
        // Kick off tier2 safety validation asynchronously (stub pipeline)
        spawn_tier2_safety_validation(state.clone(), version_id.clone(), claims.sub.clone());

        // Derive validation seed from determinism context if available
        let (validation_seed_hex, determinism_mode_str) = {
            let config = state.config.read().unwrap_or_else(|e| {
                tracing::warn!("Config lock poisoned in validation handler, recovering");
                e.into_inner()
            });
            let determinism_mode = config.general.as_ref().and_then(|g| g.determinism_mode);

            if let Some(ref mode) = determinism_mode {
                // Derive validation seed using HKDF from dataset_id + version_id
                let global_seed = B3Hash::hash(format!("{}:{}", dataset_id, version_id).as_bytes());
                let validation_seed = derive_seed(&global_seed, "dataset_validation");
                (Some(hex::encode(validation_seed)), Some(mode.as_str()))
            } else {
                (None, None)
            }
        };

        let _ = state
            .db
            .record_dataset_version_validation_run(
                &version_id,
                "tier1_structural",
                if is_valid { "valid" } else { "invalid" },
                Some("structural"),
                validation_errors_str.as_deref(),
                None,
                Some(claims.sub.as_str()),
                validation_seed_hex.as_deref(),
                determinism_mode_str,
                None, // validation_hash_b3 - can be computed later if needed
            )
            .await;
    }

    Ok(Json(ValidateDatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id,
        is_valid,
        validation_status: map_validation_status(validation_status),
        errors: if validation_errors.is_empty() {
            None
        } else {
            Some(validation_errors)
        },
        validated_at: chrono::Utc::now().to_rfc3339(),
    }))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod validation_tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    async fn create_test_file(dir: &StdPath, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut file = File::create(&path).await.unwrap();
        file.write_all(content.as_bytes()).await.unwrap();
        path
    }

    #[tokio::test]
    async fn test_validation_error_display() {
        let error = ValidationError::new(
            ValidationSeverity::Error,
            ValidationCategory::Format,
            "Invalid JSON",
            "INVALID_JSON",
        )
        .with_file("test.jsonl")
        .with_line(10)
        .with_column(5);

        let display = error.to_string();
        assert!(display.contains("Invalid JSON"));
        assert!(display.contains("test.jsonl"));
        assert!(display.contains("line 10"));
        assert!(display.contains("column 5"));
    }

    #[tokio::test]
    async fn test_validation_result_merge() {
        let mut result1 = DatasetValidationResult::new(ValidationMode::Quick);
        result1.add_error(ValidationError::new(
            ValidationSeverity::Error,
            ValidationCategory::Format,
            "Error 1",
            "ERR1",
        ));

        let mut result2 = DatasetValidationResult::new(ValidationMode::Quick);
        result2.add_error(ValidationError::new(
            ValidationSeverity::Warning,
            ValidationCategory::Size,
            "Warning 1",
            "WARN1",
        ));

        result1.merge(result2);

        assert!(!result1.is_valid);
        assert_eq!(result1.error_count, 1);
        assert_eq!(result1.warning_count, 1);
        assert_eq!(result1.errors.len(), 2);
    }

    #[tokio::test]
    async fn test_file_exists_rule() {
        let rule = FileExistsRule;
        let config = ValidationConfig::default();

        // Test non-existent file
        let errors = rule
            .validate_file(StdPath::new("/nonexistent/file.jsonl"), &config)
            .await;
        assert!(!errors.is_empty());
        assert_eq!(errors[0].code, "FILE_NOT_FOUND");
    }

    #[tokio::test]
    async fn test_file_size_rule() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let path = create_test_file(dir.path(), "test.jsonl", "").await;

        let rule = FileSizeRule;
        let config = ValidationConfig::default();

        let errors = rule.validate_file(&path, &config).await;
        assert!(!errors.is_empty());
        assert_eq!(errors[0].code, "FILE_EMPTY");
    }

    #[tokio::test]
    async fn test_file_extension_rule() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let path = create_test_file(dir.path(), "test.xyz", "content").await;

        let rule = FileExtensionRule;
        let config = ValidationConfig::default();

        let errors = rule.validate_file(&path, &config).await;
        assert!(!errors.is_empty());
        assert_eq!(errors[0].code, "INVALID_EXTENSION");
    }

    #[tokio::test]
    async fn test_jsonl_format_rule_valid() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let content = r#"{"row_id": "1", "prompt": "Hello", "response": "World"}
{"row_id": "2", "prompt": "Foo", "response": "Bar"}"#;
        let path = create_test_file(dir.path(), "test.jsonl", content).await;

        let rule = JsonlFormatRule;
        let config = ValidationConfig::for_training_jsonl();

        let errors = rule.validate_file(&path, &config).await;
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[tokio::test]
    async fn test_jsonl_format_rule_invalid_json() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let content = r#"{"valid": true}
{invalid json}
{"also_valid": true}"#;
        let path = create_test_file(dir.path(), "test.jsonl", content).await;

        let rule = JsonlFormatRule;
        let config = ValidationConfig::default();

        let errors = rule.validate_file(&path, &config).await;
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.code == "INVALID_JSON"));
    }

    #[tokio::test]
    async fn test_jsonl_format_rule_missing_field() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let content = r#"{"row_id": "1", "prompt": "Hello"}"#; // missing response
        let path = create_test_file(dir.path(), "test.jsonl", content).await;

        let rule = JsonlFormatRule;
        let config = ValidationConfig::for_training_jsonl();

        let errors = rule.validate_file(&path, &config).await;
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.code == "MISSING_FIELD"));
    }

    #[tokio::test]
    async fn test_jsonl_format_rule_duplicate_row_id() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let content = r#"{"row_id": "1", "prompt": "A", "response": "B"}
{"row_id": "1", "prompt": "C", "response": "D"}"#;
        let path = create_test_file(dir.path(), "test.jsonl", content).await;

        let rule = JsonlFormatRule;
        let config = ValidationConfig::for_training_jsonl();

        let errors = rule.validate_file(&path, &config).await;
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.code == "DUPLICATE_ROW_ID"));
    }

    #[tokio::test]
    async fn test_quick_validate_file() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let content = r#"{"data": "test"}"#;
        let path = create_test_file(dir.path(), "test.jsonl", content).await;

        let result = quick_validate_file(&path, None).await;
        assert!(result.is_valid);
        assert_eq!(result.mode, ValidationMode::Quick);
    }

    #[tokio::test]
    async fn test_deep_validate_file() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let content = r#"{"row_id": "1", "prompt": "Hello", "response": "World"}"#;
        let path = create_test_file(dir.path(), "test.jsonl", content).await;

        let config = ValidationConfig::for_training_jsonl();
        let result = deep_validate_file(&path, Some(config)).await;

        assert!(result.is_valid);
        assert_eq!(result.mode, ValidationMode::Deep);
    }

    #[tokio::test]
    async fn test_composite_validator() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let path = create_test_file(dir.path(), "test.jsonl", "{}").await;

        let config = ValidationConfig::default();
        let validator = CompositeValidator::quick_validator(config);

        let result = validator.validate_file(&path).await;
        assert!(result.is_valid);
        assert_eq!(result.files_validated, 1);
    }
}
