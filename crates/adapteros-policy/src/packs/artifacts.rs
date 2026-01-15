//! Artifacts Policy Pack
//!
//! Enforces artifact signing, SBOM validation, and content-addressed storage
//! requirements for adapterOS bundles and components.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Artifacts policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactsConfig {
    /// Whether signature verification is required
    pub require_signature: bool,
    /// Whether SBOM validation is required
    pub require_sbom: bool,
    /// Whether only content-addressed storage is allowed
    pub cas_only: bool,
    /// Allowed signature algorithms
    pub allowed_signature_algorithms: Vec<SignatureAlgorithm>,
    /// Required SBOM fields
    pub required_sbom_fields: Vec<String>,
    /// Maximum artifact size in bytes
    pub max_artifact_size_bytes: u64,
}

/// Supported signature algorithms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignatureAlgorithm {
    /// Ed25519 signature
    Ed25519,
    /// ECDSA P-256 signature
    EcdsaP256,
    /// RSA signature
    Rsa,
}

impl Default for ArtifactsConfig {
    fn default() -> Self {
        Self {
            require_signature: true,
            require_sbom: true,
            cas_only: true,
            allowed_signature_algorithms: vec![SignatureAlgorithm::Ed25519],
            required_sbom_fields: vec![
                "name".to_string(),
                "version".to_string(),
                "checksum".to_string(),
                "dependencies".to_string(),
            ],
            max_artifact_size_bytes: 10 * 1024 * 1024 * 1024, // 10GB
        }
    }
}

/// Artifact metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub artifact_id: String,
    pub artifact_type: ArtifactType,
    pub size_bytes: u64,
    pub checksum: String,
    pub signature: Option<ArtifactSignature>,
    pub sbom: Option<SbomData>,
    pub created_at: u64,
    pub created_by: String,
    pub cas_hash: Option<String>,
}

/// Types of artifacts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArtifactType {
    /// Model weights
    ModelWeights,
    /// Adapter bundle
    AdapterBundle,
    /// Policy pack
    PolicyPack,
    /// Configuration file
    Configuration,
    /// Documentation
    Documentation,
    /// Test data
    TestData,
}

/// Artifact signature information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactSignature {
    pub algorithm: SignatureAlgorithm,
    pub signature: String,
    pub public_key: String,
    pub key_id: String,
    pub timestamp: u64,
}

/// SBOM (Software Bill of Materials) data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomData {
    pub name: String,
    pub version: String,
    pub checksum: String,
    pub dependencies: Vec<SbomDependency>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub generated_at: u64,
    pub generated_by: String,
}

/// SBOM dependency information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomDependency {
    pub name: String,
    pub version: String,
    pub checksum: String,
    pub license: Option<String>,
    pub source: Option<String>,
}

/// Artifact validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactValidation {
    pub artifact_id: String,
    pub is_valid: bool,
    pub validation_errors: Vec<String>,
    pub validation_warnings: Vec<String>,
    pub signature_valid: bool,
    pub sbom_valid: bool,
    pub cas_valid: bool,
}

/// Artifacts policy implementation
pub struct ArtifactsPolicy {
    config: ArtifactsConfig,
}

impl ArtifactsPolicy {
    /// Create new artifacts policy
    pub fn new(config: ArtifactsConfig) -> Self {
        Self { config }
    }

    /// Validate artifact signature
    pub fn validate_signature(&self, signature: &ArtifactSignature) -> Result<bool> {
        if !self
            .config
            .allowed_signature_algorithms
            .contains(&signature.algorithm)
        {
            return Err(AosError::PolicyViolation(format!(
                "Signature algorithm {:?} not allowed",
                signature.algorithm
            )));
        }

        // In a real implementation, this would verify the signature
        // For now, we'll do basic validation
        if signature.signature.is_empty() {
            return Err(AosError::PolicyViolation("Empty signature".to_string()));
        }

        if signature.public_key.is_empty() {
            return Err(AosError::PolicyViolation("Empty public key".to_string()));
        }

        if signature.key_id.is_empty() {
            return Err(AosError::PolicyViolation("Empty key ID".to_string()));
        }

        // Check timestamp (should be recent)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if signature.timestamp > now + 3600 {
            // 1 hour in the future
            return Err(AosError::PolicyViolation(
                "Signature timestamp in the future".to_string(),
            ));
        }

        if signature.timestamp < now - 86400 * 365 {
            // 1 year ago
            return Err(AosError::PolicyViolation("Signature too old".to_string()));
        }

        Ok(true)
    }

    /// Validate SBOM data
    pub fn validate_sbom(&self, sbom: &SbomData) -> Result<bool> {
        // Check required fields
        for field in &self.config.required_sbom_fields {
            match field.as_str() {
                "name" => {
                    if sbom.name.is_empty() {
                        return Err(AosError::PolicyViolation(
                            "SBOM name is required".to_string(),
                        ));
                    }
                }
                "version" => {
                    if sbom.version.is_empty() {
                        return Err(AosError::PolicyViolation(
                            "SBOM version is required".to_string(),
                        ));
                    }
                }
                "checksum" => {
                    if sbom.checksum.is_empty() {
                        return Err(AosError::PolicyViolation(
                            "SBOM checksum is required".to_string(),
                        ));
                    }
                }
                "dependencies" => {
                    if sbom.dependencies.is_empty() {
                        return Err(AosError::PolicyViolation(
                            "SBOM dependencies are required".to_string(),
                        ));
                    }
                }
                _ => {}
            }
        }

        // Validate dependencies
        for dep in &sbom.dependencies {
            if dep.name.is_empty() {
                return Err(AosError::PolicyViolation(
                    "Dependency name is required".to_string(),
                ));
            }
            if dep.version.is_empty() {
                return Err(AosError::PolicyViolation(
                    "Dependency version is required".to_string(),
                ));
            }
            if dep.checksum.is_empty() {
                return Err(AosError::PolicyViolation(
                    "Dependency checksum is required".to_string(),
                ));
            }
        }

        // Check timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if sbom.generated_at > now + 3600 {
            // 1 hour in the future
            return Err(AosError::PolicyViolation(
                "SBOM timestamp in the future".to_string(),
            ));
        }

        Ok(true)
    }

    /// Validate artifact metadata
    pub fn validate_artifact(&self, artifact: &ArtifactMetadata) -> Result<ArtifactValidation> {
        let mut validation_errors = Vec::new();
        let mut validation_warnings = Vec::new();

        // Check size
        if artifact.size_bytes > self.config.max_artifact_size_bytes {
            validation_errors.push(format!(
                "Artifact size {} bytes exceeds maximum {} bytes",
                artifact.size_bytes, self.config.max_artifact_size_bytes
            ));
        }

        // Check signature requirement
        let signature_valid = if self.config.require_signature {
            if let Some(signature) = &artifact.signature {
                match self.validate_signature(signature) {
                    Ok(valid) => valid,
                    Err(e) => {
                        validation_errors.push(format!("Signature validation failed: {}", e));
                        false
                    }
                }
            } else {
                validation_errors.push("Signature is required but not provided".to_string());
                false
            }
        } else {
            true
        };

        // Check SBOM requirement
        let sbom_valid = if self.config.require_sbom {
            if let Some(sbom) = &artifact.sbom {
                match self.validate_sbom(sbom) {
                    Ok(valid) => valid,
                    Err(e) => {
                        validation_errors.push(format!("SBOM validation failed: {}", e));
                        false
                    }
                }
            } else {
                validation_errors.push("SBOM is required but not provided".to_string());
                false
            }
        } else {
            true
        };

        // Check CAS requirement
        let cas_valid = if self.config.cas_only {
            if let Some(cas_hash) = &artifact.cas_hash {
                if cas_hash.is_empty() {
                    validation_errors.push("CAS hash is required but empty".to_string());
                    false
                } else {
                    // Basic CAS hash format validation (should be hex)
                    if cas_hash.len() < 32 || !cas_hash.chars().all(|c| c.is_ascii_hexdigit()) {
                        validation_errors.push("Invalid CAS hash format".to_string());
                        false
                    } else {
                        true
                    }
                }
            } else {
                validation_errors.push("CAS hash is required but not provided".to_string());
                false
            }
        } else {
            true
        };

        // Check checksum
        if artifact.checksum.is_empty() {
            validation_warnings.push("Artifact checksum is empty".to_string());
        }

        // Check created_by
        if artifact.created_by.is_empty() {
            validation_warnings.push("Artifact creator is empty".to_string());
        }

        let is_valid = validation_errors.is_empty() && signature_valid && sbom_valid && cas_valid;

        Ok(ArtifactValidation {
            artifact_id: artifact.artifact_id.clone(),
            is_valid,
            validation_errors,
            validation_warnings,
            signature_valid,
            sbom_valid,
            cas_valid,
        })
    }

    /// Validate artifacts configuration
    pub fn validate_config(&self) -> Result<()> {
        if self.config.allowed_signature_algorithms.is_empty() {
            return Err(AosError::PolicyViolation(
                "At least one signature algorithm must be allowed".to_string(),
            ));
        }

        if self.config.required_sbom_fields.is_empty() {
            return Err(AosError::PolicyViolation(
                "At least one SBOM field must be required".to_string(),
            ));
        }

        if self.config.max_artifact_size_bytes == 0 {
            return Err(AosError::PolicyViolation(
                "Maximum artifact size must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Context for artifacts policy enforcement
#[derive(Debug)]
pub struct ArtifactsContext {
    pub artifacts: Vec<ArtifactMetadata>,
    pub tenant_id: String,
    pub operation: ArtifactOperation,
}

/// Types of artifact operations
#[derive(Debug)]
pub enum ArtifactOperation {
    /// Import operation
    Import,
    /// Export operation
    Export,
    /// Validation operation
    Validation,
    /// Deletion operation
    Deletion,
}

impl PolicyContext for ArtifactsContext {
    fn context_type(&self) -> &str {
        "artifacts"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for ArtifactsPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Artifacts
    }

    fn name(&self) -> &'static str {
        "Artifacts"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let artifacts_ctx = ctx
            .as_any()
            .downcast_ref::<ArtifactsContext>()
            .ok_or_else(|| AosError::PolicyViolation("Invalid artifacts context".to_string()))?;

        // Validate configuration
        self.validate_config()?;

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Validate each artifact
        for artifact in &artifacts_ctx.artifacts {
            match self.validate_artifact(artifact) {
                Ok(validation) => {
                    if !validation.is_valid {
                        violations.push(Violation {
                            severity: Severity::High,
                            message: format!(
                                "Artifact {} validation failed",
                                validation.artifact_id
                            ),
                            details: Some(validation.validation_errors.join(", ")),
                        });
                    }

                    for warning in validation.validation_warnings {
                        warnings.push(format!("Artifact {}: {}", validation.artifact_id, warning));
                    }
                }
                Err(e) => {
                    violations.push(Violation {
                        severity: Severity::High,
                        message: format!("Artifact {} validation error", artifact.artifact_id),
                        details: Some(e.to_string()),
                    });
                }
            }
        }

        // Check operation-specific requirements
        match artifacts_ctx.operation {
            ArtifactOperation::Import => {
                if artifacts_ctx.artifacts.is_empty() {
                    violations.push(Violation {
                        severity: Severity::Medium,
                        message: "No artifacts provided for import".to_string(),
                        details: Some(
                            "Import operation requires at least one artifact".to_string(),
                        ),
                    });
                }
            }
            ArtifactOperation::Export => {
                // Export operations might have different requirements
                warnings.push("Export operation - verify destination security".to_string());
            }
            ArtifactOperation::Validation => {
                // Validation operations are always allowed
            }
            ArtifactOperation::Deletion => {
                // Deletion operations might require additional checks
                warnings.push("Deletion operation - verify no dependencies".to_string());
            }
        }

        Ok(Audit {
            policy_id: PolicyId::Artifacts,
            passed: violations.is_empty(),
            violations,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artifacts_config_default() {
        let config = ArtifactsConfig::default();
        assert!(config.require_signature);
        assert!(config.require_sbom);
        assert!(config.cas_only);
        assert!(!config.allowed_signature_algorithms.is_empty());
    }

    #[test]
    fn test_artifacts_policy_creation() {
        let config = ArtifactsConfig::default();
        let policy = ArtifactsPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Artifacts);
    }

    #[test]
    fn test_signature_validation() {
        let config = ArtifactsConfig::default();
        let policy = ArtifactsPolicy::new(config);

        let signature = ArtifactSignature {
            algorithm: SignatureAlgorithm::Ed25519,
            signature: "test_signature".to_string(),
            public_key: "test_public_key".to_string(),
            key_id: "test_key_id".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        assert!(policy.validate_signature(&signature).unwrap());
    }

    #[test]
    fn test_sbom_validation() {
        let config = ArtifactsConfig::default();
        let policy = ArtifactsPolicy::new(config);

        let sbom = SbomData {
            name: "test_artifact".to_string(),
            version: "1.0.0".to_string(),
            checksum: "test_checksum".to_string(),
            dependencies: vec![SbomDependency {
                name: "dep1".to_string(),
                version: "1.0.0".to_string(),
                checksum: "dep_checksum".to_string(),
                license: Some("MIT".to_string()),
                source: Some("https://example.com".to_string()),
            }],
            metadata: HashMap::new(),
            generated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            generated_by: "test_user".to_string(),
        };

        assert!(policy.validate_sbom(&sbom).unwrap());
    }

    #[test]
    fn test_artifact_validation() {
        let config = ArtifactsConfig::default();
        let policy = ArtifactsPolicy::new(config);

        let artifact = ArtifactMetadata {
            artifact_id: "test_artifact".to_string(),
            artifact_type: ArtifactType::ModelWeights,
            size_bytes: 1000,
            checksum: "test_checksum".to_string(),
            signature: Some(ArtifactSignature {
                algorithm: SignatureAlgorithm::Ed25519,
                signature: "test_signature".to_string(),
                public_key: "test_public_key".to_string(),
                key_id: "test_key_id".to_string(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            }),
            sbom: Some(SbomData {
                name: "test_artifact".to_string(),
                version: "1.0.0".to_string(),
                checksum: "test_checksum".to_string(),
                dependencies: vec![SbomDependency {
                    name: "dep1".to_string(),
                    version: "1.0.0".to_string(),
                    checksum: "dep_checksum".to_string(),
                    license: Some("MIT".to_string()),
                    source: Some("https://example.com".to_string()),
                }],
                metadata: HashMap::new(),
                generated_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                generated_by: "test_user".to_string(),
            }),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            created_by: "test_user".to_string(),
            cas_hash: Some("a1b2c3d4e5f6789012345678901234567890abcd".to_string()),
        };

        let validation = policy.validate_artifact(&artifact).unwrap();
        assert!(validation.is_valid);
        assert!(validation.signature_valid);
        assert!(validation.sbom_valid);
        assert!(validation.cas_valid);
    }

    #[test]
    fn test_artifacts_config_validation() {
        let mut config = ArtifactsConfig::default();
        config.allowed_signature_algorithms.clear(); // Invalid
        let policy = ArtifactsPolicy::new(config);

        assert!(policy.validate_config().is_err());
    }
}
