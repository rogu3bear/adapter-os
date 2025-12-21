//! Secrets Policy Pack
//!
//! Enforces secrets management policies including Secure Enclave integration,
//! key rotation, and environment variable restrictions.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Secrets policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    /// Allowed environment variables
    pub env_allowed: Vec<String>,
    /// Keystore backend
    pub keystore: KeystoreBackend,
    /// Whether to rotate keys on promotion
    pub rotate_on_promotion: bool,
    /// Key rotation interval in seconds
    pub key_rotation_interval_secs: u64,
    /// Maximum key age in seconds
    pub max_key_age_secs: u64,
    /// Whether hardware security is required
    pub require_hardware: bool,
}

/// Keystore backend types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeystoreBackend {
    /// Secure Enclave (Apple Silicon)
    SecureEnclave,
    /// Hardware Security Module
    Hsm,
    /// Software keystore
    Software,
    /// External key management system
    External,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            env_allowed: vec![],
            keystore: KeystoreBackend::SecureEnclave,
            rotate_on_promotion: true,
            key_rotation_interval_secs: 86400 * 30, // 30 days
            max_key_age_secs: 86400 * 365,          // 1 year
            require_hardware: true,
        }
    }
}

/// Secret metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub secret_id: String,
    pub secret_type: SecretType,
    pub created_at: u64,
    pub last_rotated: u64,
    pub expires_at: Option<u64>,
    pub key_id: String,
    pub tenant_id: String,
    pub usage_count: u64,
    pub last_accessed: u64,
}

/// Types of secrets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecretType {
    /// API key
    ApiKey,
    /// Database password
    DatabasePassword,
    /// JWT signing key
    JwtSigningKey,
    /// Encryption key
    EncryptionKey,
    /// Certificate
    Certificate,
    /// SSH key
    SshKey,
}

/// Key rotation status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationStatus {
    pub key_id: String,
    pub rotation_required: bool,
    pub rotation_reason: RotationReason,
    pub days_until_rotation: u64,
    pub last_rotation: u64,
    pub next_rotation: u64,
}

/// Reasons for key rotation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationReason {
    /// Scheduled rotation
    Scheduled,
    /// Key expired
    Expired,
    /// Security incident
    SecurityIncident,
    /// Manual rotation
    Manual,
    /// Promotion event
    Promotion,
}

/// Environment variable audit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarAudit {
    pub variable_name: String,
    pub is_allowed: bool,
    pub is_present: bool,
    pub value_length: usize,
    pub risk_level: RiskLevel,
}

/// Risk levels for environment variables
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Low risk
    Low,
    /// Medium risk
    Medium,
    /// High risk
    High,
    /// Critical risk
    Critical,
}

/// Secrets policy implementation
pub struct SecretsPolicy {
    config: SecretsConfig,
}

impl SecretsPolicy {
    /// Create new secrets policy
    pub fn new(config: SecretsConfig) -> Self {
        Self { config }
    }

    /// Check if environment variable is allowed
    pub fn is_env_var_allowed(&self, var_name: &str) -> bool {
        self.config.env_allowed.contains(&var_name.to_string())
    }

    /// Audit environment variables
    pub fn audit_env_vars(&self, env_vars: &HashMap<String, String>) -> Vec<EnvVarAudit> {
        let mut audits = Vec::new();

        for (name, value) in env_vars {
            let is_allowed = self.is_env_var_allowed(name);
            let is_present = !value.is_empty();
            let value_length = value.len();

            let risk_level = if is_allowed {
                RiskLevel::Low
            } else if name.contains("PASSWORD") || name.contains("SECRET") || name.contains("KEY") {
                RiskLevel::Critical
            } else if name.contains("TOKEN") || name.contains("AUTH") {
                RiskLevel::High
            } else if name.contains("CONFIG") || name.contains("SETTING") {
                RiskLevel::Medium
            } else {
                RiskLevel::Low
            };

            audits.push(EnvVarAudit {
                variable_name: name.clone(),
                is_allowed,
                is_present,
                value_length,
                risk_level,
            });
        }

        audits
    }

    /// Check if key rotation is required
    pub fn check_key_rotation(&self, secret: &SecretMetadata) -> KeyRotationStatus {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let days_since_rotation = (now - secret.last_rotated) / 86400;
        let days_until_rotation = self.config.key_rotation_interval_secs / 86400;

        let rotation_required = if let Some(expires_at) = secret.expires_at {
            now >= expires_at
        } else {
            days_since_rotation >= days_until_rotation
        };

        let rotation_reason = if let Some(expires_at) = secret.expires_at {
            if now >= expires_at {
                RotationReason::Expired
            } else {
                RotationReason::Scheduled
            }
        } else if days_since_rotation >= days_until_rotation {
            RotationReason::Scheduled
        } else {
            RotationReason::Scheduled
        };

        let next_rotation = if rotation_required {
            now
        } else {
            secret.last_rotated + self.config.key_rotation_interval_secs
        };

        KeyRotationStatus {
            key_id: secret.key_id.clone(),
            rotation_required,
            rotation_reason,
            days_until_rotation: if rotation_required {
                0
            } else {
                days_until_rotation - days_since_rotation
            },
            last_rotation: secret.last_rotated,
            next_rotation,
        }
    }

    /// Validate secrets configuration
    pub fn validate_config(&self) -> Result<()> {
        if self.config.key_rotation_interval_secs == 0 {
            return Err(AosError::PolicyViolation(
                "Key rotation interval must be greater than 0".to_string(),
            ));
        }

        if self.config.max_key_age_secs == 0 {
            return Err(AosError::PolicyViolation(
                "Maximum key age must be greater than 0".to_string(),
            ));
        }

        if self.config.key_rotation_interval_secs > self.config.max_key_age_secs {
            return Err(AosError::PolicyViolation(
                "Key rotation interval cannot exceed maximum key age".to_string(),
            ));
        }

        if self.config.require_hardware && matches!(self.config.keystore, KeystoreBackend::Software)
        {
            return Err(AosError::PolicyViolation(
                "Hardware requirement conflicts with software keystore".to_string(),
            ));
        }

        Ok(())
    }
}

/// Context for secrets policy enforcement
#[derive(Debug)]
pub struct SecretsContext {
    pub secrets: Vec<SecretMetadata>,
    pub env_vars: HashMap<String, String>,
    pub tenant_id: String,
    pub operation: SecretsOperation,
}

/// Types of secrets operations
#[derive(Debug)]
pub enum SecretsOperation {
    /// Key generation
    KeyGeneration,
    /// Key rotation
    KeyRotation,
    /// Key deletion
    KeyDeletion,
    /// Secret access
    SecretAccess,
    /// Environment audit
    EnvironmentAudit,
}

impl PolicyContext for SecretsContext {
    fn context_type(&self) -> &str {
        "secrets"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for SecretsPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Secrets
    }

    fn name(&self) -> &'static str {
        "Secrets"
    }

    fn severity(&self) -> Severity {
        Severity::Critical
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let secrets_ctx = ctx
            .as_any()
            .downcast_ref::<SecretsContext>()
            .ok_or_else(|| AosError::PolicyViolation("Invalid secrets context".to_string()))?;

        // Validate configuration
        self.validate_config()?;

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Audit environment variables
        let env_audits = self.audit_env_vars(&secrets_ctx.env_vars);
        for audit in env_audits {
            if !audit.is_allowed {
                let severity = match audit.risk_level {
                    RiskLevel::Low => Severity::Low,
                    RiskLevel::Medium => Severity::Medium,
                    RiskLevel::High => Severity::High,
                    RiskLevel::Critical => Severity::Critical,
                };

                violations.push(Violation {
                    severity,
                    message: format!("Unauthorized environment variable: {}", audit.variable_name),
                    details: Some(format!(
                        "Risk level: {:?}, Value length: {}",
                        audit.risk_level, audit.value_length
                    )),
                });
            }
        }

        // Check key rotation requirements
        for secret in &secrets_ctx.secrets {
            let rotation_status = self.check_key_rotation(secret);

            if rotation_status.rotation_required {
                warnings.push(format!(
                    "Key rotation required for {}: {:?}",
                    rotation_status.key_id, rotation_status.rotation_reason
                ));
            }

            // Check for expired keys
            if let Some(expires_at) = secret.expires_at {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                if now >= expires_at {
                    violations.push(Violation {
                        severity: Severity::Critical,
                        message: format!("Expired key: {}", secret.key_id),
                        details: Some(format!("Expired at: {}, Current time: {}", expires_at, now)),
                    });
                }
            }

            // Check key age
            let key_age = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - secret.created_at;

            if key_age > self.config.max_key_age_secs {
                violations.push(Violation {
                    severity: Severity::High,
                    message: format!("Key too old: {}", secret.key_id),
                    details: Some(format!(
                        "Age: {} seconds, Maximum: {} seconds",
                        key_age, self.config.max_key_age_secs
                    )),
                });
            }
        }

        // Check keystore requirements
        match self.config.keystore {
            KeystoreBackend::SecureEnclave => {
                if self.config.require_hardware {
                    // In a real implementation, this would check if Secure Enclave is available
                    warnings.push("Secure Enclave keystore in use".to_string());
                }
            }
            KeystoreBackend::Hsm => {
                warnings.push("HSM keystore in use".to_string());
            }
            KeystoreBackend::Software => {
                if self.config.require_hardware {
                    violations.push(Violation {
                        severity: Severity::High,
                        message: "Hardware keystore required but software keystore in use"
                            .to_string(),
                        details: Some("Policy requires hardware security".to_string()),
                    });
                }
            }
            KeystoreBackend::External => {
                warnings.push("External keystore in use".to_string());
            }
        }

        Ok(Audit {
            policy_id: PolicyId::Secrets,
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
    fn test_secrets_config_default() {
        let config = SecretsConfig::default();
        assert!(config.env_allowed.is_empty());
        assert!(matches!(config.keystore, KeystoreBackend::SecureEnclave));
        assert!(config.rotate_on_promotion);
        assert!(config.require_hardware);
    }

    #[test]
    fn test_secrets_policy_creation() {
        let config = SecretsConfig::default();
        let policy = SecretsPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Secrets);
    }

    #[test]
    fn test_env_var_audit() {
        // Create config with PATH explicitly allowed
        let mut config = SecretsConfig::default();
        config.env_allowed = vec!["PATH".to_string()];
        let policy = SecretsPolicy::new(config);

        let mut env_vars = HashMap::new();
        env_vars.insert("PATH".to_string(), "/usr/bin".to_string());
        env_vars.insert("SECRET_KEY".to_string(), "secret_value".to_string());

        let audits = policy.audit_env_vars(&env_vars);
        assert_eq!(audits.len(), 2);

        let path_audit = audits.iter().find(|a| a.variable_name == "PATH").unwrap();
        assert!(path_audit.is_allowed); // PATH is explicitly allowed in config

        let secret_audit = audits
            .iter()
            .find(|a| a.variable_name == "SECRET_KEY")
            .unwrap();
        assert!(!secret_audit.is_allowed); // SECRET_KEY is not in allowed list
        assert!(matches!(secret_audit.risk_level, RiskLevel::Critical));
    }

    #[test]
    fn test_key_rotation_check() {
        let config = SecretsConfig::default();
        let policy = SecretsPolicy::new(config);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let secret = SecretMetadata {
            secret_id: "test_secret".to_string(),
            secret_type: SecretType::ApiKey,
            created_at: now - 86400 * 31,   // 31 days ago
            last_rotated: now - 86400 * 31, // 31 days ago
            expires_at: None,
            key_id: "test_key".to_string(),
            tenant_id: "test_tenant".to_string(),
            usage_count: 100,
            last_accessed: now - 3600, // 1 hour ago
        };

        let rotation_status = policy.check_key_rotation(&secret);
        assert!(rotation_status.rotation_required);
        assert!(matches!(
            rotation_status.rotation_reason,
            RotationReason::Scheduled
        ));
    }

    #[test]
    fn test_secrets_config_validation() {
        let mut config = SecretsConfig::default();
        config.key_rotation_interval_secs = 0; // Invalid
        let policy = SecretsPolicy::new(config);

        assert!(policy.validate_config().is_err());
    }

    #[test]
    fn test_hardware_requirement_conflict() {
        let mut config = SecretsConfig::default();
        config.keystore = KeystoreBackend::Software;
        config.require_hardware = true; // Conflict
        let policy = SecretsPolicy::new(config);

        assert!(policy.validate_config().is_err());
    }
}
