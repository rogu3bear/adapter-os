//! .aos Format Validation
//!
//! Validates .aos archive format structure before disk write.
//! Checks:
//! - Minimum file size (8-byte header)
//! - Valid manifest offset and length
//! - Valid JSON manifest structure
//! - Valid safetensors format in weights section
//!
//! ## Format Specification
//!
//! ```text
//! [0-3]    manifest_offset (u32, little-endian)
//! [4-7]    manifest_len (u32, little-endian)
//! [8...]   weights (safetensors format)
//! [offset] manifest (JSON metadata)
//! ```

use adapteros_core::{AosError, Result};
use serde_json::Value;
use tracing::{debug, warn};

/// Validates .aos format structure from in-memory bytes
///
/// This validator checks the format WITHOUT writing to disk, ensuring
/// that only valid archives are persisted.
#[derive(Debug)]
pub struct AosFormatValidator;

impl AosFormatValidator {
    /// Validate .aos file format from bytes
    ///
    /// Args:
    /// - `data`: Raw file bytes
    ///
    /// Errors:
    /// - `AosError::Validation` if format is invalid
    ///
    /// Returns:
    /// - `ValidationResult` with detailed information about the archive
    pub fn validate(data: &[u8]) -> Result<ValidationResult> {
        // Step 1: Check minimum size
        if data.len() < 8 {
            return Err(AosError::Validation(format!(
                ".aos file too small: {} bytes (minimum 8 bytes required for header)",
                data.len()
            )));
        }

        // Step 2: Parse header
        let manifest_offset = Self::parse_u32_le(&data[0..4]) as usize;
        let manifest_len = Self::parse_u32_le(&data[4..8]) as usize;

        debug!(
            manifest_offset = manifest_offset,
            manifest_len = manifest_len,
            file_size = data.len(),
            "Validating .aos header"
        );

        // Step 3: Validate header bounds
        if manifest_offset < 8 {
            return Err(AosError::Validation(format!(
                "Invalid manifest offset: {} (must be >= 8 to account for header)",
                manifest_offset
            )));
        }

        if manifest_offset + manifest_len > data.len() {
            return Err(AosError::Validation(format!(
                "Invalid manifest bounds: offset={}, len={}, file_size={} (offset + len must be <= file_size)",
                manifest_offset, manifest_len, data.len()
            )));
        }

        if manifest_len == 0 {
            return Err(AosError::Validation(
                "Manifest length is zero: manifest data is required".to_string(),
            ));
        }

        // Step 4: Extract and validate weights section
        let weights_start = 8;
        let weights_end = manifest_offset;

        if weights_start > weights_end {
            return Err(AosError::Validation(format!(
                "Invalid weights section: start={} > end={}",
                weights_start, weights_end
            )));
        }

        let weights_len = weights_end - weights_start;

        // Step 5: Validate safetensors format
        if weights_len > 0 {
            let weights_bytes = &data[weights_start..weights_end];
            Self::validate_safetensors(weights_bytes)?;
        }

        // Step 6: Extract and validate manifest JSON
        let manifest_bytes = &data[manifest_offset..manifest_offset + manifest_len];
        let manifest_json = Self::validate_manifest_json(manifest_bytes)?;

        debug!(
            weights_len = weights_len,
            manifest_len = manifest_len,
            "Successfully validated .aos format"
        );

        Ok(ValidationResult {
            manifest_offset,
            manifest_len,
            weights_len,
            manifest: manifest_json,
        })
    }

    /// Parse little-endian u32 from 4-byte slice
    #[inline]
    fn parse_u32_le(bytes: &[u8]) -> u32 {
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    /// Validate safetensors format header
    ///
    /// Safetensors format starts with header_len (u64 LE), then JSON header
    fn validate_safetensors(data: &[u8]) -> Result<()> {
        if data.len() < 8 {
            return Err(AosError::Validation(
                "Weights section too small: cannot contain valid safetensors format".to_string(),
            ));
        }

        // Read header length (first 8 bytes)
        let header_len_bytes: [u8; 8] = [
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ];
        let header_len = u64::from_le_bytes(header_len_bytes) as usize;

        debug!(header_len = header_len, "Safetensors header length");

        // Validate header length
        if header_len == 0 {
            return Err(AosError::Validation(
                "Invalid safetensors: header length is zero".to_string(),
            ));
        }

        if 8 + header_len > data.len() {
            return Err(AosError::Validation(format!(
                "Invalid safetensors: header declares {} bytes but file only has {} bytes after length field",
                header_len,
                data.len().saturating_sub(8)
            )));
        }

        // Try to parse header as JSON
        let header_json_bytes = &data[8..8 + header_len];
        match std::str::from_utf8(header_json_bytes) {
            Ok(header_str) => {
                if let Err(e) = serde_json::from_str::<serde_json::Value>(header_str) {
                    return Err(AosError::Validation(format!(
                        "Invalid safetensors header JSON: {}",
                        e
                    )));
                }
            }
            Err(e) => {
                return Err(AosError::Validation(format!(
                    "Safetensors header is not valid UTF-8: {}",
                    e
                )));
            }
        }

        debug!("Safetensors format validated successfully");
        Ok(())
    }

    /// Validate manifest JSON structure
    fn validate_manifest_json(bytes: &[u8]) -> Result<serde_json::Value> {
        // Ensure it's valid UTF-8
        let manifest_str = std::str::from_utf8(bytes)
            .map_err(|e| AosError::Validation(format!("Manifest is not valid UTF-8: {}", e)))?;

        // Parse as JSON
        let manifest = serde_json::from_str::<Value>(manifest_str)
            .map_err(|e| AosError::Validation(format!("Manifest is not valid JSON: {}", e)))?;

        // Ensure it's an object (dict)
        if !manifest.is_object() {
            return Err(AosError::Validation(
                "Manifest must be a JSON object, not array or primitive".to_string(),
            ));
        }

        // Validate required fields (examples)
        // Note: Exact requirements depend on your manifest schema
        if let Some(obj) = manifest.as_object() {
            if obj.is_empty() {
                warn!("Manifest is an empty JSON object");
            }
        }

        debug!("Manifest JSON structure validated");
        Ok(manifest)
    }
}

/// Result of successful .aos format validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Byte offset where manifest JSON starts
    pub manifest_offset: usize,
    /// Size of manifest JSON in bytes
    pub manifest_len: usize,
    /// Size of weights (safetensors) section in bytes
    pub weights_len: usize,
    /// Parsed manifest JSON value
    pub manifest: serde_json::Value,
}

impl ValidationResult {
    /// Get total file size this validation expects
    pub fn expected_file_size(&self) -> usize {
        self.manifest_offset + self.manifest_len
    }

    /// Get manifest as strongly-typed struct
    pub fn manifest_as<T: for<'de> serde::Deserialize<'de>>(&self) -> Result<T> {
        serde_json::from_value(self.manifest.clone()).map_err(|e| AosError::Serialization(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct TestManifest {
        version: String,
        adapter_id: String,
        rank: u32,
    }

    /// Helper to create valid .aos bytes
    fn create_valid_aos_bytes(manifest: &TestManifest, weights: &[u8]) -> Vec<u8> {
        let manifest_json = serde_json::to_vec(manifest).unwrap();

        // Build safetensors header (minimal valid version)
        // For testing, we'll use a simple empty tensor map
        let safetensors_header = r#"{}"#;
        let mut safetensors_data = Vec::new();
        safetensors_data.extend_from_slice(&(safetensors_header.len() as u64).to_le_bytes());
        safetensors_data.extend_from_slice(safetensors_header.as_bytes());
        safetensors_data.extend_from_slice(weights);

        // Build .aos file
        let manifest_offset = 8 + safetensors_data.len();
        let manifest_len = manifest_json.len();

        let mut aos_data = Vec::new();
        aos_data.extend_from_slice(&(manifest_offset as u32).to_le_bytes());
        aos_data.extend_from_slice(&(manifest_len as u32).to_le_bytes());
        aos_data.extend_from_slice(&safetensors_data);
        aos_data.extend_from_slice(&manifest_json);

        aos_data
    }

    #[test]
    fn test_validate_valid_aos_file() {
        let manifest = TestManifest {
            version: "2.0".to_string(),
            adapter_id: "test-001".to_string(),
            rank: 8,
        };
        let weights = b"test_weights_data";

        let data = create_valid_aos_bytes(&manifest, weights);
        let result = AosFormatValidator::validate(&data);

        assert!(result.is_ok(), "Valid .aos should validate");
        let validation = result.unwrap();
        // weights_len includes the 8-byte safetensors header length field + the header JSON string
        let safetensors_header = r#"{}"#;
        let expected_weights_len = 8 + safetensors_header.len() + weights.len();
        assert_eq!(validation.weights_len, expected_weights_len,
            "weights_len should include safetensors header length field (8 bytes) + header JSON ({} bytes) + weights data ({} bytes)",
            safetensors_header.len(), weights.len());
    }

    #[test]
    fn test_validate_file_too_small() {
        let data = vec![1, 2, 3, 4]; // Only 4 bytes (less than 8-byte header)

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "File < 8 bytes should fail");
        if let Err(e) = result {
            assert!(e.to_string().contains("too small"));
        }
    }

    #[test]
    fn test_validate_empty_file() {
        let data = vec![];

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "Empty file should fail");
    }

    #[test]
    fn test_validate_invalid_manifest_offset_too_small() {
        let mut data = vec![0u8; 16];

        // Set offset to 4 (less than minimum 8)
        data[0..4].copy_from_slice(&(4u32).to_le_bytes());
        data[4..8].copy_from_slice(&(8u32).to_le_bytes());

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "Offset < 8 should fail");
        if let Err(e) = result {
            assert!(e.to_string().contains("manifest offset"));
        }
    }

    #[test]
    fn test_validate_manifest_offset_out_of_bounds() {
        let mut data = vec![0u8; 32];

        // Set offset beyond file size
        data[0..4].copy_from_slice(&(1000u32).to_le_bytes());
        data[4..8].copy_from_slice(&(100u32).to_le_bytes());

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "Out-of-bounds offset should fail");
        if let Err(e) = result {
            assert!(e.to_string().contains("manifest bounds"));
        }
    }

    #[test]
    fn test_validate_zero_manifest_length() {
        let mut data = vec![0u8; 16];

        // Valid offset (8) but zero length
        data[0..4].copy_from_slice(&(8u32).to_le_bytes());
        data[4..8].copy_from_slice(&(0u32).to_le_bytes());

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "Zero manifest length should fail");
        if let Err(e) = result {
            assert!(e.to_string().contains("Manifest length is zero"));
        }
    }

    #[test]
    fn test_validate_invalid_manifest_json() {
        let mut data = vec![0u8; 32];

        // Valid header with offset=8, length=16
        data[0..4].copy_from_slice(&(8u32).to_le_bytes());
        data[4..8].copy_from_slice(&(16u32).to_le_bytes());

        // Manifest section: invalid JSON
        data[8..24].copy_from_slice(b"not valid json!!");

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "Invalid JSON should fail");
        if let Err(e) = result {
            assert!(e.to_string().contains("JSON"));
        }
    }

    #[test]
    fn test_validate_manifest_not_object() {
        let manifest_json = b"[1, 2, 3]"; // Array, not object
        let mut data = Vec::new();

        let manifest_offset = 8;
        let manifest_len = manifest_json.len();

        data.extend_from_slice(&(manifest_offset as u32).to_le_bytes());
        data.extend_from_slice(&(manifest_len as u32).to_le_bytes());
        data.extend_from_slice(&vec![0u8; 0]); // No weights
        data.extend_from_slice(manifest_json);

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "Array manifest should fail");
        if let Err(e) = result {
            assert!(e.to_string().contains("JSON object"));
        }
    }

    #[test]
    fn test_validate_invalid_safetensors_too_small() {
        // Create a file with weights section that's too small for safetensors header
        let manifest_json = b"{}";
        let mut data = vec![0u8; 8 + 4 + manifest_json.len()];

        // Offset=12, length=2 (safetensors section will be bytes 8-12, only 4 bytes)
        data[0..4].copy_from_slice(&(12u32).to_le_bytes());
        data[4..8].copy_from_slice(&(manifest_json.len() as u32).to_le_bytes());
        // bytes 8-12 are safetensors section (only 4 bytes - too small for 8-byte header)
        data[12..12 + manifest_json.len()].copy_from_slice(manifest_json);

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "Safetensors < 8 bytes should fail");
    }

    #[test]
    fn test_validate_corrupted_safetensors_header() {
        // Create safetensors with invalid header length
        let mut safetensors = Vec::new();
        safetensors.extend_from_slice(&(1000u64).to_le_bytes()); // Claims 1000 bytes header but file is smaller
        safetensors.extend_from_slice(b"short");

        let manifest_json = b"{}";
        let mut data = Vec::new();

        let manifest_offset = 8 + safetensors.len();
        let manifest_len = manifest_json.len();

        data.extend_from_slice(&(manifest_offset as u32).to_le_bytes());
        data.extend_from_slice(&(manifest_len as u32).to_le_bytes());
        data.extend_from_slice(&safetensors);
        data.extend_from_slice(manifest_json);

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "Invalid safetensors header should fail");
    }

    #[test]
    fn test_validate_safetensors_invalid_json_header() {
        // Create safetensors with non-JSON header
        let mut safetensors = Vec::new();
        let header_data = b"not json at all!!!";
        safetensors.extend_from_slice(&(header_data.len() as u64).to_le_bytes());
        safetensors.extend_from_slice(header_data);

        let manifest_json = b"{}";
        let mut data = Vec::new();

        let manifest_offset = 8 + safetensors.len();
        let manifest_len = manifest_json.len();

        data.extend_from_slice(&(manifest_offset as u32).to_le_bytes());
        data.extend_from_slice(&(manifest_len as u32).to_le_bytes());
        data.extend_from_slice(&safetensors);
        data.extend_from_slice(manifest_json);

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "Invalid safetensors JSON should fail");
    }

    #[test]
    fn test_validate_manifest_not_utf8() {
        // Create manifest with invalid UTF-8
        let mut data = vec![0u8; 32];

        data[0..4].copy_from_slice(&(8u32).to_le_bytes());
        data[4..8].copy_from_slice(&(16u32).to_le_bytes());

        // Invalid UTF-8 in manifest section
        data[8..24].copy_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

        let result = AosFormatValidator::validate(&data);

        assert!(result.is_err(), "Non-UTF8 manifest should fail");
    }

    #[test]
    fn test_validation_result_expected_file_size() {
        let manifest = TestManifest {
            version: "2.0".to_string(),
            adapter_id: "test-001".to_string(),
            rank: 8,
        };
        let weights = b"weights";

        let data = create_valid_aos_bytes(&manifest, weights);
        let result = AosFormatValidator::validate(&data).unwrap();

        assert_eq!(result.expected_file_size(), data.len());
    }
}
