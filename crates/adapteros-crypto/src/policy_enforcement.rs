//! Policy-Based Cryptographic Enforcement
//!
//! Enforces cryptographic policies for all operations, integrated with
//! AdapterOS's canonical policy packs.
//!
//! ## Policy Categories
//! - **Algorithm Policies**: Approved/banned algorithms
//! - **Key Size Policies**: Minimum key sizes for each algorithm
//! - **Rotation Policies**: Maximum key age before rotation required
//! - **Usage Policies**: Permitted operations per key type
//! - **Compliance Policies**: Regulatory requirements (FIPS, etc.)
//!
//! ## Integration
//! - Validates all crypto operations against policies
//! - Rejects non-compliant operations
//! - Logs policy violations to audit trail
//! - Supports policy versioning and hot-reload

use crate::audit::{CryptoAuditLogger, CryptoOperation};
use crate::key_provider::KeyAlgorithm;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Cryptographic policy configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CryptoPolicy {
    /// Policy version
    pub version: String,
    /// Approved algorithms
    pub approved_algorithms: HashSet<String>,
    /// Banned algorithms
    pub banned_algorithms: HashSet<String>,
    /// Minimum key sizes (algorithm -> min bits)
    pub min_key_sizes: HashMap<String, u32>,
    /// Maximum key age before rotation (algorithm -> max age in seconds)
    pub max_key_ages: HashMap<String, u64>,
    /// Permitted operations per algorithm
    pub permitted_operations: HashMap<String, HashSet<String>>,
    /// FIPS 140-2 compliance mode
    pub fips_mode: bool,
    /// Require hardware-backed keys (SEP/HSM)
    pub require_hardware_backing: bool,
}

impl Default for CryptoPolicy {
    fn default() -> Self {
        let mut approved_algorithms = HashSet::new();
        approved_algorithms.insert("ed25519".to_string());
        approved_algorithms.insert("aes256gcm".to_string());
        approved_algorithms.insert("chacha20poly1305".to_string());

        let mut banned_algorithms = HashSet::new();
        banned_algorithms.insert("md5".to_string());
        banned_algorithms.insert("sha1".to_string());
        banned_algorithms.insert("des".to_string());
        banned_algorithms.insert("3des".to_string());
        banned_algorithms.insert("rc4".to_string());

        let mut min_key_sizes = HashMap::new();
        min_key_sizes.insert("rsa".to_string(), 2048);
        min_key_sizes.insert("aes".to_string(), 256);
        min_key_sizes.insert("ecdsa".to_string(), 256);

        let mut max_key_ages = HashMap::new();
        max_key_ages.insert("aes256gcm".to_string(), 90 * 24 * 3600); // 90 days
        max_key_ages.insert("chacha20poly1305".to_string(), 90 * 24 * 3600);

        let mut permitted_operations = HashMap::new();
        let mut signing_ops = HashSet::new();
        signing_ops.insert("sign".to_string());
        signing_ops.insert("verify".to_string());
        permitted_operations.insert("ed25519".to_string(), signing_ops);

        let mut encryption_ops = HashSet::new();
        encryption_ops.insert("encrypt".to_string());
        encryption_ops.insert("decrypt".to_string());
        encryption_ops.insert("seal".to_string());
        encryption_ops.insert("unseal".to_string());
        permitted_operations.insert("aes256gcm".to_string(), encryption_ops.clone());
        permitted_operations.insert("chacha20poly1305".to_string(), encryption_ops);

        Self {
            version: "1.0.0".to_string(),
            approved_algorithms,
            banned_algorithms,
            min_key_sizes,
            max_key_ages,
            permitted_operations,
            fips_mode: false,
            require_hardware_backing: false,
        }
    }
}

/// Policy violation details
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PolicyViolation {
    /// Violation type
    pub violation_type: ViolationType,
    /// Algorithm involved
    pub algorithm: String,
    /// Detailed message
    pub message: String,
    /// Policy version
    pub policy_version: String,
}

/// Types of policy violations
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ViolationType {
    /// Banned algorithm used
    BannedAlgorithm,
    /// Algorithm not in approved list
    UnapprovedAlgorithm,
    /// Key size below minimum
    InsufficientKeySize,
    /// Key age exceeds maximum
    KeyAgeExceeded,
    /// Operation not permitted for algorithm
    UnpermittedOperation,
    /// FIPS compliance violation
    FipsViolation,
    /// Hardware backing required but not available
    HardwareBackingRequired,
}

impl std::fmt::Display for ViolationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViolationType::BannedAlgorithm => write!(f, "banned_algorithm"),
            ViolationType::UnapprovedAlgorithm => write!(f, "unapproved_algorithm"),
            ViolationType::InsufficientKeySize => write!(f, "insufficient_key_size"),
            ViolationType::KeyAgeExceeded => write!(f, "key_age_exceeded"),
            ViolationType::UnpermittedOperation => write!(f, "unpermitted_operation"),
            ViolationType::FipsViolation => write!(f, "fips_violation"),
            ViolationType::HardwareBackingRequired => write!(f, "hardware_backing_required"),
        }
    }
}

/// Policy enforcement engine
pub struct CryptoPolicyEnforcer {
    /// Current policy
    policy: Arc<RwLock<CryptoPolicy>>,
    /// Audit logger for policy violations
    audit_logger: Arc<CryptoAuditLogger>,
}

impl CryptoPolicyEnforcer {
    /// Create a new policy enforcer
    pub fn new(policy: CryptoPolicy, audit_logger: Arc<CryptoAuditLogger>) -> Self {
        Self {
            policy: Arc::new(RwLock::new(policy)),
            audit_logger,
        }
    }

    /// Create with default policy
    pub fn with_default_policy(audit_logger: Arc<CryptoAuditLogger>) -> Self {
        Self::new(CryptoPolicy::default(), audit_logger)
    }

    /// Validate an algorithm against policy
    pub async fn validate_algorithm(&self, algorithm: &KeyAlgorithm) -> Result<()> {
        let policy = self.policy.read().await;
        let alg_str = algorithm.to_string();

        // Check if banned
        if policy.banned_algorithms.contains(&alg_str) {
            let violation = PolicyViolation {
                violation_type: ViolationType::BannedAlgorithm,
                algorithm: alg_str.clone(),
                message: format!("Algorithm '{}' is banned by policy", alg_str),
                policy_version: policy.version.clone(),
            };

            self.log_violation(&violation).await;

            return Err(AosError::PolicyViolation(format!(
                "Banned algorithm: {}",
                alg_str
            )));
        }

        // Check if approved (if not in FIPS mode where all must be explicitly approved)
        if policy.fips_mode && !policy.approved_algorithms.contains(&alg_str) {
            let violation = PolicyViolation {
                violation_type: ViolationType::UnapprovedAlgorithm,
                algorithm: alg_str.clone(),
                message: format!(
                    "Algorithm '{}' not in approved list (FIPS mode enabled)",
                    alg_str
                ),
                policy_version: policy.version.clone(),
            };

            self.log_violation(&violation).await;

            return Err(AosError::PolicyViolation(format!(
                "Unapproved algorithm (FIPS mode): {}",
                alg_str
            )));
        }

        debug!(algorithm = %alg_str, "Algorithm validated against policy");
        Ok(())
    }

    /// Validate key size for an algorithm
    pub async fn validate_key_size(&self, algorithm: &KeyAlgorithm, size_bits: u32) -> Result<()> {
        let policy = self.policy.read().await;
        let alg_str = algorithm.to_string();

        if let Some(&min_size) = policy.min_key_sizes.get(&alg_str) {
            if size_bits < min_size {
                let violation = PolicyViolation {
                    violation_type: ViolationType::InsufficientKeySize,
                    algorithm: alg_str.clone(),
                    message: format!(
                        "Key size {} bits below minimum {} bits for algorithm '{}'",
                        size_bits, min_size, alg_str
                    ),
                    policy_version: policy.version.clone(),
                };

                self.log_violation(&violation).await;

                return Err(AosError::PolicyViolation(format!(
                    "Key size {} below minimum {} for {}",
                    size_bits, min_size, alg_str
                )));
            }
        }

        debug!(
            algorithm = %alg_str,
            size_bits = size_bits,
            "Key size validated against policy"
        );
        Ok(())
    }

    /// Validate key age
    pub async fn validate_key_age(
        &self,
        algorithm: &KeyAlgorithm,
        key_age_secs: u64,
    ) -> Result<()> {
        let policy = self.policy.read().await;
        let alg_str = algorithm.to_string();

        if let Some(&max_age) = policy.max_key_ages.get(&alg_str) {
            if key_age_secs > max_age {
                let violation = PolicyViolation {
                    violation_type: ViolationType::KeyAgeExceeded,
                    algorithm: alg_str.clone(),
                    message: format!(
                        "Key age {} seconds exceeds maximum {} seconds for algorithm '{}'",
                        key_age_secs, max_age, alg_str
                    ),
                    policy_version: policy.version.clone(),
                };

                self.log_violation(&violation).await;

                return Err(AosError::PolicyViolation(format!(
                    "Key age {} exceeds maximum {} for {}",
                    key_age_secs, max_age, alg_str
                )));
            }
        }

        debug!(
            algorithm = %alg_str,
            key_age_secs = key_age_secs,
            "Key age validated against policy"
        );
        Ok(())
    }

    /// Validate operation is permitted for algorithm
    pub async fn validate_operation(
        &self,
        algorithm: &KeyAlgorithm,
        operation: &CryptoOperation,
    ) -> Result<()> {
        let policy = self.policy.read().await;
        let alg_str = algorithm.to_string();
        let op_str = match operation {
            CryptoOperation::Encrypt => "encrypt",
            CryptoOperation::Decrypt => "decrypt",
            CryptoOperation::Sign => "sign",
            CryptoOperation::Verify => "verify",
            CryptoOperation::Seal => "seal",
            CryptoOperation::Unseal => "unseal",
            _ => "other",
        };

        if let Some(permitted_ops) = policy.permitted_operations.get(&alg_str) {
            if !permitted_ops.contains(op_str) {
                let violation = PolicyViolation {
                    violation_type: ViolationType::UnpermittedOperation,
                    algorithm: alg_str.clone(),
                    message: format!(
                        "Operation '{}' not permitted for algorithm '{}'",
                        op_str, alg_str
                    ),
                    policy_version: policy.version.clone(),
                };

                self.log_violation(&violation).await;

                return Err(AosError::PolicyViolation(format!(
                    "Operation {} not permitted for {}",
                    op_str, alg_str
                )));
            }
        }

        debug!(
            algorithm = %alg_str,
            operation = %op_str,
            "Operation validated against policy"
        );
        Ok(())
    }

    /// Validate hardware backing if required
    pub async fn validate_hardware_backing(&self, hardware_backed: bool) -> Result<()> {
        let policy = self.policy.read().await;

        if policy.require_hardware_backing && !hardware_backed {
            let violation = PolicyViolation {
                violation_type: ViolationType::HardwareBackingRequired,
                algorithm: "N/A".to_string(),
                message: "Hardware-backed keys required by policy".to_string(),
                policy_version: policy.version.clone(),
            };

            self.log_violation(&violation).await;

            return Err(AosError::PolicyViolation(
                "Hardware-backed keys required".to_string(),
            ));
        }

        Ok(())
    }

    /// Comprehensive validation of a crypto operation
    pub async fn validate_crypto_operation(
        &self,
        algorithm: &KeyAlgorithm,
        operation: &CryptoOperation,
        key_size_bits: Option<u32>,
        key_age_secs: Option<u64>,
        hardware_backed: bool,
    ) -> Result<()> {
        // Validate algorithm
        self.validate_algorithm(algorithm).await?;

        // Validate key size if provided
        if let Some(size) = key_size_bits {
            self.validate_key_size(algorithm, size).await?;
        }

        // Validate key age if provided
        if let Some(age) = key_age_secs {
            self.validate_key_age(algorithm, age).await?;
        }

        // Validate operation
        self.validate_operation(algorithm, operation).await?;

        // Validate hardware backing
        self.validate_hardware_backing(hardware_backed).await?;

        info!(
            algorithm = %algorithm,
            operation = ?operation,
            "Crypto operation validated against all policies"
        );

        Ok(())
    }

    /// Log a policy violation to audit trail
    async fn log_violation(&self, violation: &PolicyViolation) {
        warn!(
            violation_type = %violation.violation_type,
            algorithm = %violation.algorithm,
            message = %violation.message,
            "Policy violation detected"
        );

        let _ = self
            .audit_logger
            .log_failure(
                CryptoOperation::Verify, // Generic operation for policy checks
                None,
                None,
                &violation.message,
                serde_json::json!({
                    "violation_type": violation.violation_type.to_string(),
                    "algorithm": violation.algorithm,
                    "policy_version": violation.policy_version,
                }),
            )
            .await;
    }

    /// Update the policy (hot-reload)
    pub async fn update_policy(&self, new_policy: CryptoPolicy) {
        let mut policy = self.policy.write().await;
        let old_version = policy.version.clone();
        let new_version = new_policy.version.clone();
        *policy = new_policy;

        info!(
            old_version = %old_version,
            new_version = %new_version,
            "Crypto policy updated"
        );
    }

    /// Get current policy
    pub async fn get_policy(&self) -> CryptoPolicy {
        self.policy.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::OperationResult;

    #[tokio::test]
    async fn test_default_policy() {
        let policy = CryptoPolicy::default();
        assert!(policy.approved_algorithms.contains("ed25519"));
        assert!(policy.approved_algorithms.contains("aes256gcm"));
        assert!(policy.banned_algorithms.contains("md5"));
        assert!(policy.banned_algorithms.contains("sha1"));
        assert!(!policy.fips_mode);
    }

    #[tokio::test]
    async fn test_validate_approved_algorithm() {
        let audit_logger = Arc::new(CryptoAuditLogger::new());
        let enforcer = CryptoPolicyEnforcer::with_default_policy(audit_logger);

        let result = enforcer.validate_algorithm(&KeyAlgorithm::Ed25519).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_banned_algorithm() {
        let audit_logger = Arc::new(CryptoAuditLogger::new());
        let mut policy = CryptoPolicy::default();
        policy.banned_algorithms.insert("aes256gcm".to_string());

        let enforcer = CryptoPolicyEnforcer::new(policy, audit_logger.clone());

        let result = enforcer.validate_algorithm(&KeyAlgorithm::Aes256Gcm).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(AosError::PolicyViolation(_))));

        // Verify audit log entry was created
        assert_eq!(
            audit_logger.count_by_result(OperationResult::Failure).await,
            1
        );
    }

    #[tokio::test]
    async fn test_validate_key_size() {
        let audit_logger = Arc::new(CryptoAuditLogger::new());
        let enforcer = CryptoPolicyEnforcer::with_default_policy(audit_logger);

        // Valid key size
        let result = enforcer
            .validate_key_size(&KeyAlgorithm::Aes256Gcm, 256)
            .await;
        assert!(result.is_ok());

        // Invalid key size
        let result = enforcer
            .validate_key_size(&KeyAlgorithm::Aes256Gcm, 128)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_key_age() {
        let audit_logger = Arc::new(CryptoAuditLogger::new());
        let enforcer = CryptoPolicyEnforcer::with_default_policy(audit_logger);

        // Valid key age (30 days)
        let result = enforcer
            .validate_key_age(&KeyAlgorithm::Aes256Gcm, 30 * 24 * 3600)
            .await;
        assert!(result.is_ok());

        // Invalid key age (180 days > 90 day limit)
        let result = enforcer
            .validate_key_age(&KeyAlgorithm::Aes256Gcm, 180 * 24 * 3600)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_operation() {
        let audit_logger = Arc::new(CryptoAuditLogger::new());
        let enforcer = CryptoPolicyEnforcer::with_default_policy(audit_logger);

        // Valid operation for Ed25519 (signing)
        let result = enforcer
            .validate_operation(&KeyAlgorithm::Ed25519, &CryptoOperation::Sign)
            .await;
        assert!(result.is_ok());

        // Invalid operation for Ed25519 (encryption)
        let result = enforcer
            .validate_operation(&KeyAlgorithm::Ed25519, &CryptoOperation::Encrypt)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_hardware_backing() {
        let audit_logger = Arc::new(CryptoAuditLogger::new());
        let policy = CryptoPolicy {
            require_hardware_backing: true,
            ..Default::default()
        };

        let enforcer = CryptoPolicyEnforcer::new(policy, audit_logger);

        // Hardware-backed key
        let result = enforcer.validate_hardware_backing(true).await;
        assert!(result.is_ok());

        // Software key when hardware required
        let result = enforcer.validate_hardware_backing(false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_comprehensive_validation() {
        let audit_logger = Arc::new(CryptoAuditLogger::new());
        let enforcer = CryptoPolicyEnforcer::with_default_policy(audit_logger);

        // Valid operation
        let result = enforcer
            .validate_crypto_operation(
                &KeyAlgorithm::Aes256Gcm,
                &CryptoOperation::Encrypt,
                Some(256),
                Some(30 * 24 * 3600),
                false,
            )
            .await;
        assert!(result.is_ok());

        // Invalid key size
        let result = enforcer
            .validate_crypto_operation(
                &KeyAlgorithm::Aes256Gcm,
                &CryptoOperation::Encrypt,
                Some(128),
                Some(30 * 24 * 3600),
                false,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_policy_update() {
        let audit_logger = Arc::new(CryptoAuditLogger::new());
        let enforcer = CryptoPolicyEnforcer::with_default_policy(audit_logger);

        let new_policy = CryptoPolicy {
            version: "2.0.0".to_string(),
            fips_mode: true,
            ..Default::default()
        };

        enforcer.update_policy(new_policy).await;

        let current_policy = enforcer.get_policy().await;
        assert_eq!(current_policy.version, "2.0.0");
        assert!(current_policy.fips_mode);
    }

    #[tokio::test]
    async fn test_fips_mode_enforcement() {
        let audit_logger = Arc::new(CryptoAuditLogger::new());
        let policy = CryptoPolicy {
            fips_mode: true,
            approved_algorithms: HashSet::from(["aes256gcm".to_string()]),
            ..Default::default()
        };

        let enforcer = CryptoPolicyEnforcer::new(policy, audit_logger);

        // Approved algorithm
        let result = enforcer.validate_algorithm(&KeyAlgorithm::Aes256Gcm).await;
        assert!(result.is_ok());

        // Unapproved algorithm in FIPS mode
        let result = enforcer
            .validate_algorithm(&KeyAlgorithm::ChaCha20Poly1305)
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_violation_type_display() {
        assert_eq!(
            ViolationType::BannedAlgorithm.to_string(),
            "banned_algorithm"
        );
        assert_eq!(
            ViolationType::UnapprovedAlgorithm.to_string(),
            "unapproved_algorithm"
        );
        assert_eq!(
            ViolationType::InsufficientKeySize.to_string(),
            "insufficient_key_size"
        );
        assert_eq!(
            ViolationType::KeyAgeExceeded.to_string(),
            "key_age_exceeded"
        );
        assert_eq!(
            ViolationType::UnpermittedOperation.to_string(),
            "unpermitted_operation"
        );
        assert_eq!(ViolationType::FipsViolation.to_string(), "fips_violation");
        assert_eq!(
            ViolationType::HardwareBackingRequired.to_string(),
            "hardware_backing_required"
        );
    }
}
