//! Single-file adapter validator

use super::loader::SingleFileAdapterLoader;
use adapteros_core::Result;
use std::path::Path;

/// Single-file adapter validator
pub struct SingleFileAdapterValidator;

impl SingleFileAdapterValidator {
    /// Validate .aos file integrity
    pub async fn validate<P: AsRef<Path>>(path: P) -> Result<ValidationResult> {
        let path = path.as_ref();

        // Check file exists
        if !path.exists() {
            return Ok(ValidationResult {
                is_valid: false,
                errors: vec!["File does not exist".to_string()],
                warnings: vec![],
            });
        }

        // Check file extension
        if path.extension().and_then(|s| s.to_str()) != Some("aos") {
            return Ok(ValidationResult {
                is_valid: false,
                errors: vec!["File must have .aos extension".to_string()],
                warnings: vec![],
            });
        }

        // Try to load the adapter
        match SingleFileAdapterLoader::load(path).await {
            Ok(adapter) => {
                // Additional validation checks
                let mut errors = Vec::new();
                let mut warnings = Vec::new();

                // Check manifest fields
                if adapter.manifest.adapter_id.is_empty() {
                    errors.push("Adapter ID is empty".to_string());
                }

                if adapter.manifest.version.is_empty() {
                    errors.push("Version is empty".to_string());
                }

                if adapter.weights.positive.lora_a.is_empty()
                    && adapter
                        .weights
                        .combined
                        .as_ref()
                        .map(|g| g.lora_a.is_empty())
                        .unwrap_or(true)
                {
                    errors.push("Weights are empty".to_string());
                }

                if adapter.training_data.is_empty() {
                    warnings.push("No training data provided".to_string());
                }

                // Check signature if present
                if let Some(sig) = &adapter.signature {
                    // Signature is already verified in adapter.verify()
                    // Just note that it's present and valid
                    tracing::debug!("Signature present and verified: key_id={}", sig.key_id);
                }

                Ok(ValidationResult {
                    is_valid: errors.is_empty(),
                    errors,
                    warnings,
                })
            }
            Err(e) => Ok(ValidationResult {
                is_valid: false,
                errors: vec![format!("Failed to load adapter: {}", e)],
                warnings: vec![],
            }),
        }
    }
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Create a successful validation result
    pub fn success() -> Self {
        Self {
            is_valid: true,
            errors: vec![],
            warnings: vec![],
        }
    }

    /// Create a failed validation result with error
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            is_valid: false,
            errors: vec![msg.into()],
            warnings: vec![],
        }
    }

    /// Add a warning to the result
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}
