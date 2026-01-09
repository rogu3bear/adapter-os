//! Structured preflight error codes for programmatic handling
//!
//! Provides machine-readable error codes that can be used by both CLI and API
//! to report preflight failures consistently.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Structured preflight error codes for programmatic handling
///
/// These codes are designed to be:
/// - Machine-readable for API clients
/// - Consistent across CLI and Server API
/// - Informative for operators debugging issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreflightErrorCode {
    // ========================================================================
    // Existence checks
    // ========================================================================
    /// Target adapter not found in registry
    AdapterNotFound,

    /// Target adapter .aos file not found on disk
    AdapterFileNotFound,

    // ========================================================================
    // Hash validation (required for deterministic operations)
    // ========================================================================
    /// Adapter missing content_hash_b3 (BLAKE3 of manifest + payload)
    MissingContentHash,

    /// Adapter missing manifest_hash (BLAKE3 of manifest bytes)
    MissingManifestHash,

    /// Adapter missing aos_file_hash
    MissingAosFileHash,

    /// Hash verification failed (computed hash doesn't match stored)
    HashIntegrityFailure,

    // ========================================================================
    // File integrity
    // ========================================================================
    /// .aos file is corrupted or cannot be read
    AosFileCorrupted,

    /// .aos file exists but is not readable (permissions, locked, etc.)
    AosFileUnreadable,

    // ========================================================================
    // Lifecycle state
    // ========================================================================
    /// Lifecycle state string is not recognized
    InvalidLifecycleState,

    /// Adapter is in a terminal state (Retired or Failed)
    TerminalLifecycleState,

    /// Lifecycle state does not allow the requested operation
    LifecycleStateNotAllowed,

    // ========================================================================
    // Evidence requirements
    // ========================================================================
    /// Training snapshot evidence is missing
    MissingTrainingEvidence,

    // ========================================================================
    // Conflict detection
    // ========================================================================
    /// Another adapter is already active for the same repo/branch
    ConflictingActiveAdapters,

    // ========================================================================
    // System state
    // ========================================================================
    /// System is in maintenance mode
    MaintenanceModeActive,

    /// Tenant isolation boundaries would be violated
    TenantIsolationViolation,

    // ========================================================================
    // Database errors
    // ========================================================================
    /// Database operation failed
    DatabaseError,

    // ========================================================================
    // Model checks
    // ========================================================================
    /// Model directory not found
    ModelNotFound,

    /// Required model files missing (config.json, tokenizer.json)
    ModelFileMissing,

    /// No model weights found (.safetensors or .bin files)
    ModelWeightsMissing,

    /// Model path resolution failed
    ModelPathResolutionFailed,
}

impl PreflightErrorCode {
    /// Returns the string code for API responses
    ///
    /// All codes follow the pattern: `PREFLIGHT_{CHECK}_{FAILURE_TYPE}`
    /// This ensures backward compatibility with existing error parsers.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AdapterNotFound => "PREFLIGHT_ADAPTER_NOT_FOUND",
            Self::AdapterFileNotFound => "PREFLIGHT_FILE_NOT_FOUND",
            Self::MissingContentHash => "PREFLIGHT_MISSING_CONTENT_HASH",
            Self::MissingManifestHash => "PREFLIGHT_MISSING_MANIFEST_HASH",
            Self::MissingAosFileHash => "PREFLIGHT_MISSING_AOS_HASH",
            Self::HashIntegrityFailure => "PREFLIGHT_HASH_INTEGRITY_FAILURE",
            Self::AosFileCorrupted => "PREFLIGHT_FILE_CORRUPTED",
            Self::AosFileUnreadable => "PREFLIGHT_FILE_UNREADABLE",
            Self::InvalidLifecycleState => "PREFLIGHT_INVALID_LIFECYCLE_STATE",
            Self::TerminalLifecycleState => "PREFLIGHT_TERMINAL_STATE",
            Self::LifecycleStateNotAllowed => "PREFLIGHT_LIFECYCLE_NOT_ALLOWED",
            Self::MissingTrainingEvidence => "PREFLIGHT_MISSING_TRAINING_EVIDENCE",
            Self::ConflictingActiveAdapters => "PREFLIGHT_CONFLICTING_ADAPTERS",
            Self::MaintenanceModeActive => "PREFLIGHT_MAINTENANCE_MODE",
            Self::TenantIsolationViolation => "PREFLIGHT_TENANT_ISOLATION",
            Self::DatabaseError => "PREFLIGHT_DATABASE_ERROR",
            Self::ModelNotFound => "PREFLIGHT_MODEL_NOT_FOUND",
            Self::ModelFileMissing => "PREFLIGHT_MODEL_FILE_MISSING",
            Self::ModelWeightsMissing => "PREFLIGHT_MODEL_WEIGHTS_MISSING",
            Self::ModelPathResolutionFailed => "PREFLIGHT_MODEL_PATH_RESOLUTION_FAILED",
        }
    }

    /// Returns a short human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::AdapterNotFound => "Target adapter not found in registry",
            Self::AdapterFileNotFound => "Target adapter .aos file not found on disk",
            Self::MissingContentHash => "Adapter missing content_hash_b3 (required for integrity)",
            Self::MissingManifestHash => "Adapter missing manifest_hash (required for routing)",
            Self::MissingAosFileHash => "Adapter missing .aos file hash",
            Self::HashIntegrityFailure => "Hash verification failed",
            Self::AosFileCorrupted => "Adapter file is corrupted or unreadable",
            Self::AosFileUnreadable => "Adapter file exists but cannot be read",
            Self::InvalidLifecycleState => "Lifecycle state string is not recognized",
            Self::TerminalLifecycleState => "Adapter is in a terminal state",
            Self::LifecycleStateNotAllowed => "Lifecycle state does not allow this operation",
            Self::MissingTrainingEvidence => "Training snapshot evidence is missing",
            Self::ConflictingActiveAdapters => "Conflicting active adapter exists",
            Self::MaintenanceModeActive => "System is in maintenance mode",
            Self::TenantIsolationViolation => "Tenant isolation violation",
            Self::DatabaseError => "Database operation failed",
            Self::ModelNotFound => "Model directory not found",
            Self::ModelFileMissing => "Required model files missing",
            Self::ModelWeightsMissing => "No model weights found",
            Self::ModelPathResolutionFailed => "Model path resolution failed",
        }
    }

    /// Returns suggested remediation for the error
    pub fn remediation(&self, adapter_id: &str) -> Option<String> {
        match self {
            Self::MissingContentHash | Self::MissingManifestHash | Self::MissingAosFileHash => {
                Some(format!(
                    "Run: aosctl adapter repair-hashes --adapter-id {}",
                    adapter_id
                ))
            }
            Self::MaintenanceModeActive => {
                Some("rm var/.maintenance  # or unset AOS_MAINTENANCE_MODE".to_string())
            }
            Self::TerminalLifecycleState => {
                Some("Create a new adapter version - terminal states cannot be reactivated".to_string())
            }
            Self::ConflictingActiveAdapters => Some(
                "Deactivate conflicting adapter first: aosctl adapter deactivate <conflicting-id>".to_string()
            ),
            Self::MissingTrainingEvidence => Some(format!(
                "Retrain the adapter or restore training evidence: aosctl adapter train --adapter-id {}",
                adapter_id
            )),
            Self::ModelNotFound | Self::ModelFileMissing | Self::ModelWeightsMissing => {
                Some("Run: ./scripts/download-model.sh  # or: aosctl models seed".to_string())
            }
            Self::ModelPathResolutionFailed => Some(
                "Set AOS_MODEL_CACHE_DIR and AOS_BASE_MODEL_ID environment variables, or use --model-path".to_string()
            ),
            _ => None,
        }
    }
}

impl fmt::Display for PreflightErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Structured preflight check failure with full context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightCheckFailure {
    /// The error code for programmatic handling
    pub code: PreflightErrorCode,

    /// Name of the check that failed
    pub check_name: String,

    /// Human-readable error message
    pub message: String,

    /// Suggested remediation command or action
    pub remediation: Option<String>,

    /// Additional context (adapter_id, file paths, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

impl PreflightCheckFailure {
    /// Create a new check failure
    pub fn new(
        code: PreflightErrorCode,
        check_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            check_name: check_name.into(),
            message: message.into(),
            remediation: None,
            context: None,
        }
    }

    /// Add remediation suggestion
    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = Some(remediation.into());
        self
    }

    /// Add context
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }
}

impl fmt::Display for PreflightCheckFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}: {}",
            self.code.as_str(),
            self.check_name,
            self.message
        )?;
        if let Some(ref rem) = self.remediation {
            write!(f, " (fix: {})", rem)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_strings() {
        assert_eq!(
            PreflightErrorCode::MissingContentHash.as_str(),
            "PREFLIGHT_MISSING_CONTENT_HASH"
        );
        assert_eq!(
            PreflightErrorCode::MissingManifestHash.as_str(),
            "PREFLIGHT_MISSING_MANIFEST_HASH"
        );
    }

    #[test]
    fn test_remediation() {
        let remediation = PreflightErrorCode::MissingContentHash.remediation("test-adapter");
        assert!(remediation.is_some());
        assert!(remediation.unwrap().contains("repair-hashes"));
    }

    #[test]
    fn test_check_failure_display() {
        let failure = PreflightCheckFailure::new(
            PreflightErrorCode::MissingContentHash,
            "Content Hash",
            "Adapter missing content_hash_b3",
        )
        .with_remediation("aosctl adapter repair-hashes --adapter-id test");

        let display = format!("{}", failure);
        assert!(display.contains("PREFLIGHT_MISSING_CONTENT_HASH"));
        assert!(display.contains("Content Hash"));
    }
}
