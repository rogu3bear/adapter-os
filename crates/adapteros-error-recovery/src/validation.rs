//! Validation engine
//!
//! Implements validation mechanisms for files and directories.

use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;
use tracing::{debug, warn};

/// Validation engine
pub struct ValidationEngine {
    validation_cache: std::collections::HashMap<PathBuf, ValidationResult>,
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Is valid
    pub is_valid: bool,
    /// Validation timestamp
    pub timestamp: SystemTime,
    /// Validation details
    pub details: String,
    /// Validation errors
    pub errors: Vec<ValidationError>,
    /// Last modified timestamp of validated path (if known)
    pub last_modified: Option<SystemTime>,
}

/// Validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error type
    pub error_type: ValidationErrorType,
    /// Error message
    pub message: String,
    /// Error severity
    pub severity: ValidationSeverity,
}

/// Validation error type
#[derive(Debug, Clone)]
pub enum ValidationErrorType {
    /// File not found
    FileNotFound,
    /// Permission denied
    PermissionDenied,
    /// Invalid format
    InvalidFormat,
    /// Corrupted data
    CorruptedData,
    /// Size mismatch
    SizeMismatch,
    /// Checksum mismatch
    ChecksumMismatch,
    /// Metadata error
    MetadataError,
    /// Unknown error
    Unknown,
}

/// Validation severity
#[derive(Debug, Clone)]
pub enum ValidationSeverity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

impl ValidationEngine {
    /// Create a new validation engine
    pub fn new() -> Result<Self> {
        Ok(Self {
            validation_cache: std::collections::HashMap::new(),
        })
    }

    /// Validate a file
    pub async fn validate_file(&mut self, path: &Path) -> Result<bool> {
        let result = self.validate_file_detailed(path).await?;
        Ok(result.is_valid)
    }

    /// Validate a directory
    pub async fn validate_directory(&self, path: &Path) -> Result<bool> {
        let result = self.validate_directory_detailed(path).await?;
        Ok(result.is_valid)
    }

    /// Validate a file with detailed results
    pub async fn validate_file_detailed(&mut self, path: &Path) -> Result<ValidationResult> {
        // Check cache first
        if let Some(cached_result) = self.validation_cache.get(path) {
            let cache_fresh = cached_result.timestamp.elapsed().unwrap_or_default()
                < std::time::Duration::from_secs(60);

            let cache_matches_file = if let Some(last_modified) = cached_result.last_modified {
                tokio::fs::metadata(path)
                    .await
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|current| current == last_modified)
                    .unwrap_or(false)
            } else {
                false
            };

            if cache_fresh && cache_matches_file {
                return Ok(cached_result.clone());
            }
        }

        let mut errors = Vec::new();

        // Check if file exists
        if !path.exists() {
            errors.push(ValidationError {
                error_type: ValidationErrorType::FileNotFound,
                message: "File does not exist".to_string(),
                severity: ValidationSeverity::Critical,
            });
            return Ok(ValidationResult {
                is_valid: false,
                timestamp: SystemTime::now(),
                details: "File not found".to_string(),
                errors,
                last_modified: None,
            });
        }

        // Check if path is actually a file
        if !path.is_file() {
            errors.push(ValidationError {
                error_type: ValidationErrorType::Unknown,
                message: "Path is not a file".to_string(),
                severity: ValidationSeverity::High,
            });
        }

        // Check file permissions
        match self.validate_file_permissions(path).await {
            Ok(_) => {}
            Err(e) => {
                errors.push(ValidationError {
                    error_type: ValidationErrorType::PermissionDenied,
                    message: format!("Permission error: {}", e),
                    severity: ValidationSeverity::High,
                });
            }
        }

        // Check file metadata
        match self.validate_file_metadata(path).await {
            Ok(_) => {}
            Err(e) => {
                errors.push(ValidationError {
                    error_type: ValidationErrorType::MetadataError,
                    message: format!("Metadata error: {}", e),
                    severity: ValidationSeverity::Medium,
                });
            }
        }

        // Check file content
        match self.validate_file_content(path).await {
            Ok(_) => {}
            Err(e) => {
                errors.push(ValidationError {
                    error_type: ValidationErrorType::CorruptedData,
                    message: format!("Content error: {}", e),
                    severity: ValidationSeverity::High,
                });
            }
        }

        // Check file format
        match self.validate_file_format(path).await {
            Ok(_) => {}
            Err(e) => {
                errors.push(ValidationError {
                    error_type: ValidationErrorType::InvalidFormat,
                    message: format!("Format error: {}", e),
                    severity: ValidationSeverity::Medium,
                });
            }
        }

        let is_valid = errors.is_empty()
            || errors
                .iter()
                .all(|e| matches!(e.severity, ValidationSeverity::Low));

        let details = if !is_valid {
            format!("Validation failed with {} errors", errors.len())
        } else {
            "Validation passed".to_string()
        };

        let last_modified = tokio::fs::metadata(path)
            .await
            .ok()
            .and_then(|m| m.modified().ok());

        let result = ValidationResult {
            is_valid,
            timestamp: SystemTime::now(),
            details,
            errors,
            last_modified,
        };

        // Cache the result
        self.validation_cache
            .insert(path.to_path_buf(), result.clone());

        if is_valid {
            debug!("File validation passed: {}", path.display());
        } else {
            warn!(
                "File validation failed: {} - {}",
                path.display(),
                result.details
            );
        }

        Ok(result)
    }

    /// Validate a directory with detailed results
    pub async fn validate_directory_detailed(&self, path: &Path) -> Result<ValidationResult> {
        let mut errors = Vec::new();

        // Check if directory exists
        if !path.exists() {
            errors.push(ValidationError {
                error_type: ValidationErrorType::FileNotFound,
                message: "Directory does not exist".to_string(),
                severity: ValidationSeverity::Critical,
            });
            return Ok(ValidationResult {
                is_valid: false,
                timestamp: SystemTime::now(),
                details: "Directory not found".to_string(),
                errors,
                last_modified: None,
            });
        }

        // Check if path is actually a directory
        if !path.is_dir() {
            errors.push(ValidationError {
                error_type: ValidationErrorType::Unknown,
                message: "Path is not a directory".to_string(),
                severity: ValidationSeverity::High,
            });
        }

        // Check directory permissions
        match self.validate_directory_permissions(path).await {
            Ok(_) => {}
            Err(e) => {
                errors.push(ValidationError {
                    error_type: ValidationErrorType::PermissionDenied,
                    message: format!("Permission error: {}", e),
                    severity: ValidationSeverity::High,
                });
            }
        }

        // Check directory structure
        match self.validate_directory_structure(path).await {
            Ok(_) => {}
            Err(e) => {
                errors.push(ValidationError {
                    error_type: ValidationErrorType::CorruptedData,
                    message: format!("Structure error: {}", e),
                    severity: ValidationSeverity::Medium,
                });
            }
        }

        let is_valid = errors.is_empty()
            || errors
                .iter()
                .all(|e| matches!(e.severity, ValidationSeverity::Low));

        let details = if !is_valid {
            format!("Validation failed with {} errors", errors.len())
        } else {
            "Validation passed".to_string()
        };

        let last_modified = tokio::fs::metadata(path)
            .await
            .ok()
            .and_then(|m| m.modified().ok());

        let result = ValidationResult {
            is_valid,
            timestamp: SystemTime::now(),
            details,
            errors,
            last_modified,
        };

        if is_valid {
            debug!("Directory validation passed: {}", path.display());
        } else {
            warn!(
                "Directory validation failed: {} - {}",
                path.display(),
                result.details
            );
        }

        Ok(result)
    }

    /// Validate file permissions
    async fn validate_file_permissions(&self, path: &Path) -> Result<()> {
        let _metadata = fs::metadata(path)
            .await
            .map_err(|e| AosError::Validation(format!("Failed to get file metadata: {}", e)))?;

        // Check if file is readable
        if fs::read(path).await.is_err() {
            return Err(AosError::Validation("File is not readable".to_string()));
        }

        Ok(())
    }

    /// Validate file metadata
    async fn validate_file_metadata(&self, path: &Path) -> Result<()> {
        let metadata = fs::metadata(path)
            .await
            .map_err(|e| AosError::Validation(format!("Failed to get file metadata: {}", e)))?;

        // Check if file size is reasonable
        if metadata.len() > 100 * 1024 * 1024 * 1024 {
            // 100GB
            return Err(AosError::Validation(
                "File size unreasonably large".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate file content
    async fn validate_file_content(&self, path: &Path) -> Result<()> {
        let content = fs::read(path)
            .await
            .map_err(|e| AosError::Validation(format!("Failed to read file content: {}", e)))?;

        // Check for common corruption patterns
        if content.is_empty() {
            // Empty files are not necessarily corrupted
            return Ok(());
        }

        // Check for null bytes in text files
        if let Some(ext_str) = path.extension().and_then(|ext| ext.to_str()) {
            if matches!(ext_str, "txt" | "json" | "toml" | "yaml" | "md") && content.contains(&0) {
                return Err(AosError::Validation("File contains null bytes".to_string()));
            }
        }

        Ok(())
    }

    /// Validate file format
    async fn validate_file_format(&self, path: &Path) -> Result<()> {
        if let Some(ext_str) = path.extension().and_then(|ext| ext.to_str()) {
            match ext_str {
                "json" => {
                    let content = fs::read_to_string(path).await.map_err(|e| {
                        AosError::Validation(format!("Failed to read JSON file: {}", e))
                    })?;

                    // Validate JSON format
                    serde_json::from_str::<serde_json::Value>(&content)
                        .map_err(|_| AosError::Validation("Invalid JSON format".to_string()))?;
                }
                "toml" => {
                    let content = fs::read_to_string(path).await.map_err(|e| {
                        AosError::Validation(format!("Failed to read TOML file: {}", e))
                    })?;

                    // Validate TOML format
                    toml::from_str::<toml::Value>(&content)
                        .map_err(|_| AosError::Validation("Invalid TOML format".to_string()))?;
                }
                _ => {
                    // Other formats not validated
                }
            }
        }

        Ok(())
    }

    /// Validate directory permissions
    async fn validate_directory_permissions(&self, path: &Path) -> Result<()> {
        let _metadata = fs::metadata(path).await.map_err(|e| {
            AosError::Validation(format!("Failed to get directory metadata: {}", e))
        })?;

        // Check if directory is readable
        if fs::read_dir(path).await.is_err() {
            return Err(AosError::Validation(
                "Directory is not readable".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate directory structure
    async fn validate_directory_structure(&self, path: &Path) -> Result<()> {
        let mut entries = fs::read_dir(path)
            .await
            .map_err(|e| AosError::Validation(format!("Failed to read directory: {}", e)))?;

        let mut entry_count = 0;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AosError::Validation(format!("Failed to read directory entry: {}", e)))?
        {
            entry_count += 1;

            // Check if entry is accessible
            let entry_path = entry.path();
            if let Err(e) = fs::metadata(&entry_path).await {
                return Err(AosError::Validation(format!(
                    "Cannot access directory entry {}: {}",
                    entry_path.display(),
                    e
                )));
            }
        }

        // Check for reasonable entry count
        if entry_count > 10000 {
            return Err(AosError::Validation(
                "Directory has too many entries".to_string(),
            ));
        }

        Ok(())
    }

    /// Clear validation cache
    pub fn clear_cache(&mut self) {
        self.validation_cache.clear();
    }

    /// Get validation statistics
    pub fn get_validation_statistics(&self) -> ValidationStatistics {
        let total_validations = self.validation_cache.len();
        let valid_files = self
            .validation_cache
            .values()
            .filter(|result| result.is_valid)
            .count();
        let invalid_files = self
            .validation_cache
            .values()
            .filter(|result| !result.is_valid)
            .count();

        let success_rate = if total_validations > 0 {
            valid_files as f32 / total_validations as f32
        } else {
            0.0
        };

        ValidationStatistics {
            total_validations,
            valid_files,
            invalid_files,
            success_rate,
        }
    }
}

/// Validation statistics
#[derive(Debug, Clone)]
pub struct ValidationStatistics {
    /// Total number of validations
    pub total_validations: usize,
    /// Number of valid files
    pub valid_files: usize,
    /// Number of invalid files
    pub invalid_files: usize,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_storage::platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> Result<TempDir> {
        Ok(TempDir::with_prefix("aos-test-")?)
    }

    #[tokio::test]
    async fn test_validation_engine() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test validation of non-existent file
        let is_valid = engine.validate_file(&test_file).await?;
        assert!(!is_valid);

        // Test validation of existing file
        fs::write(&test_file, "hello world").await?;
        let is_valid = engine.validate_file(&test_file).await?;
        assert!(is_valid);

        Ok(())
    }

    #[tokio::test]
    async fn test_file_validation() -> Result<()> {
        let mut engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.json");

        // Test JSON validation
        fs::write(&test_file, r#"{"key": "value"}"#).await?;
        let result = engine.validate_file_detailed(&test_file).await?;
        assert!(result.is_valid);

        // Test invalid JSON
        fs::write(&test_file, r#"{"key": "value""#).await?;
        let result = engine.validate_file_detailed(&test_file).await?;
        assert!(!result.is_valid);

        Ok(())
    }

    #[tokio::test]
    async fn test_directory_validation() -> Result<()> {
        let engine = ValidationEngine::new()?;

        let temp_dir = new_test_tempdir()?;
        let test_dir = temp_dir.path().join("test_dir");

        // Test validation of non-existent directory
        let is_valid = engine.validate_directory(&test_dir).await?;
        assert!(!is_valid);

        // Test validation of existing directory
        fs::create_dir(&test_dir).await?;
        let is_valid = engine.validate_directory(&test_dir).await?;
        assert!(is_valid);

        Ok(())
    }

    #[test]
    fn test_validation_statistics() {
        let engine = ValidationEngine::new().unwrap();

        let stats = engine.get_validation_statistics();
        assert_eq!(stats.total_validations, 0);
        assert_eq!(stats.valid_files, 0);
        assert_eq!(stats.invalid_files, 0);
        assert_eq!(stats.success_rate, 0.0);
    }
}
