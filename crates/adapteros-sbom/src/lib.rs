//! SPDX SBOM generation and validation

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxDocument {
    pub spdx_version: String,
    pub data_license: String,
    pub spdx_id: String,
    pub name: String,
    pub document_namespace: String,
    pub creation_info: CreationInfo,
    pub packages: Vec<Package>,
    pub files: Vec<File>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationInfo {
    pub created: String,
    pub creators: Vec<String>,
    pub license_list_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub spdx_id: String,
    pub name: String,
    pub version_info: String,
    pub download_location: String,
    pub files_analyzed: bool,
    pub verification_code: Option<PackageVerification>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageVerification {
    pub package_verification_code_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub spdx_id: String,
    pub file_name: String,
    pub checksums: Vec<Checksum>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checksum {
    pub algorithm: String,
    pub checksum_value: String,
}

impl SpdxDocument {
    /// Create a new SPDX document
    pub fn new(name: String, namespace: String) -> Self {
        Self {
            spdx_version: "SPDX-2.3".to_string(),
            data_license: "CC0-1.0".to_string(),
            spdx_id: "SPDXRef-DOCUMENT".to_string(),
            name,
            document_namespace: namespace,
            creation_info: CreationInfo {
                created: chrono::Utc::now().to_rfc3339(),
                creators: vec!["Tool: aos-sbom".to_string()],
                license_list_version: "3.20".to_string(),
            },
            packages: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Add a package
    pub fn add_package(&mut self, name: String, version: String) {
        let spdx_id = format!("SPDXRef-Package-{}", self.packages.len());
        self.packages.push(Package {
            spdx_id,
            name,
            version_info: version,
            download_location: "NOASSERTION".to_string(),
            files_analyzed: false,
            verification_code: None,
        });
    }

    /// Add a file with hash
    pub fn add_file(&mut self, path: String, hash: &B3Hash) {
        let spdx_id = format!("SPDXRef-File-{}", self.files.len());
        self.files.push(File {
            spdx_id,
            file_name: path,
            checksums: vec![Checksum {
                algorithm: "BLAKE3".to_string(),
                checksum_value: hash.to_hex(),
            }],
        });
    }

    /// Validate completeness
    pub fn validate(&self) -> Result<()> {
        if self.packages.is_empty() && self.files.is_empty() {
            return Err(AosError::Artifact(
                "SBOM must contain at least one package or file".to_string(),
            ));
        }

        // Check all required fields are present
        if self.spdx_version.is_empty() {
            return Err(AosError::Artifact("Missing SPDX version".to_string()));
        }

        if self.document_namespace.is_empty() {
            return Err(AosError::Artifact("Missing document namespace".to_string()));
        }

        Ok(())
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| AosError::Artifact(format!("Failed to serialize SBOM: {}", e)))
    }

    /// Parse from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| AosError::Artifact(format!("Failed to parse SBOM: {}", e)))
    }
}

// Stub chrono for timestamp
mod chrono {
    pub struct Utc;
    impl Utc {
        pub fn now() -> Self {
            Self
        }
        pub fn to_rfc3339(&self) -> String {
            // Simplified: return a fixed timestamp for now
            "2025-01-01T00:00:00Z".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_sbom() {
        let mut sbom = SpdxDocument::new(
            "test-bundle".to_string(),
            "https://example.com/test".to_string(),
        );

        sbom.add_package("test-package".to_string(), "1.0.0".to_string());
        sbom.add_file("test.bin".to_string(), &B3Hash::hash(b"test"));

        assert!(sbom.validate().is_ok());
    }

    #[test]
    fn test_sbom_serialization() {
        let mut sbom =
            SpdxDocument::new("test".to_string(), "https://example.com/test".to_string());
        sbom.add_package("pkg".to_string(), "1.0".to_string());

        let json = sbom.to_json().expect("Test SBOM should serialize to JSON");
        let sbom2 = SpdxDocument::from_json(&json).expect("Test SBOM should deserialize from JSON");

        assert_eq!(sbom.name, sbom2.name);
    }
}
