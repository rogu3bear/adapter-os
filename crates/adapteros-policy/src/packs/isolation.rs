//! Isolation Policy Pack
//!
//! Process per tenant (UID/GID separation). Enforces multi-tenant isolation
//! with process, file, and key isolation.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Isolation policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolationConfig {
    /// Process model configuration
    pub process_model: ProcessModel,
    /// Unix domain socket root path
    pub uds_root: PathBuf,
    /// Forbid shared memory
    pub forbid_shm: bool,
    /// Key management configuration
    pub keys: KeyConfig,
    /// File system isolation
    pub filesystem: FilesystemIsolation,
    /// Network isolation
    pub network: NetworkIsolation,
}

/// Process model configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProcessModel {
    /// One process per tenant
    PerTenant,
    /// Shared process with isolation
    Shared,
    /// Hybrid approach
    Hybrid,
}

/// Key management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyConfig {
    /// Key backend
    pub backend: KeyBackend,
    /// Require hardware support
    pub require_hardware: bool,
    /// Key rotation policy
    pub rotation_policy: RotationPolicy,
}

/// Key backend options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KeyBackend {
    /// Secure Enclave (macOS)
    SecureEnclave,
    /// TPM (Trusted Platform Module)
    Tpm,
    /// Software-based
    Software,
}

/// Key rotation policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationPolicy {
    /// Rotation interval (days)
    pub interval_days: u32,
    /// Automatic rotation
    pub automatic: bool,
    /// Rotation triggers
    pub triggers: Vec<RotationTrigger>,
}

/// Rotation trigger
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RotationTrigger {
    /// Time-based rotation
    TimeBased,
    /// Event-based rotation
    EventBased,
    /// Manual rotation
    Manual,
}

/// File system isolation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemIsolation {
    /// Enable file system isolation
    pub enable: bool,
    /// Root directory for tenant data
    pub root_dir: PathBuf,
    /// Allowed file operations
    pub allowed_operations: Vec<FileOperation>,
    /// Blocked file operations
    pub blocked_operations: Vec<FileOperation>,
}

/// File operation types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileOperation {
    /// Read operation
    Read,
    /// Write operation
    Write,
    /// Execute operation
    Execute,
    /// Delete operation
    Delete,
    /// Create operation
    Create,
}

/// Network isolation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkIsolation {
    /// Enable network isolation
    pub enable: bool,
    /// Allowed network interfaces
    pub allowed_interfaces: Vec<String>,
    /// Blocked network interfaces
    pub blocked_interfaces: Vec<String>,
    /// Network namespace isolation
    pub namespace_isolation: bool,
}

impl Default for IsolationConfig {
    fn default() -> Self {
        Self {
            process_model: ProcessModel::PerTenant,
            uds_root: adapteros_core::rebase_var_path("var/run/aos"),
            forbid_shm: true,
            keys: KeyConfig {
                backend: KeyBackend::SecureEnclave,
                require_hardware: true,
                rotation_policy: RotationPolicy {
                    interval_days: 30,
                    automatic: true,
                    triggers: vec![RotationTrigger::TimeBased],
                },
            },
            filesystem: FilesystemIsolation {
                enable: true,
                root_dir: PathBuf::from("var"),
                allowed_operations: vec![
                    FileOperation::Read,
                    FileOperation::Write,
                    FileOperation::Create,
                ],
                blocked_operations: vec![FileOperation::Execute, FileOperation::Delete],
            },
            network: NetworkIsolation {
                enable: true,
                allowed_interfaces: vec!["lo".to_string()],
                blocked_interfaces: vec!["eth0".to_string(), "wlan0".to_string()],
                namespace_isolation: true,
            },
        }
    }
}

/// Tenant isolation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantContext {
    /// Tenant ID
    pub tenant_id: String,
    /// Process ID
    pub process_id: u32,
    /// User ID
    pub user_id: u32,
    /// Group ID
    pub group_id: u32,
    /// Working directory
    pub working_directory: PathBuf,
    /// Environment variables
    pub environment: std::collections::HashMap<String, String>,
}

/// Isolation policy enforcement
pub struct IsolationPolicy {
    config: IsolationConfig,
}

impl IsolationPolicy {
    /// Create a new isolation policy
    pub fn new(config: IsolationConfig) -> Self {
        Self { config }
    }

    /// Validate tenant context
    pub fn validate_tenant_context(&self, context: &TenantContext) -> Result<()> {
        // Validate tenant ID
        if context.tenant_id.is_empty() {
            return Err(AosError::PolicyViolation(
                "Tenant ID cannot be empty".to_string(),
            ));
        }

        // Validate process ID
        if context.process_id == 0 {
            return Err(AosError::PolicyViolation(
                "Process ID cannot be zero".to_string(),
            ));
        }

        // Validate user ID
        if context.user_id == 0 {
            return Err(AosError::PolicyViolation(
                "User ID cannot be zero (root)".to_string(),
            ));
        }

        // Validate group ID
        if context.group_id == 0 {
            return Err(AosError::PolicyViolation(
                "Group ID cannot be zero (root)".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate process isolation
    pub fn validate_process_isolation(&self, tenant_contexts: &[TenantContext]) -> Result<()> {
        match self.config.process_model {
            ProcessModel::PerTenant => {
                // Check that each tenant has a unique process
                let mut process_ids = std::collections::HashSet::new();
                for context in tenant_contexts {
                    if !process_ids.insert(context.process_id) {
                        return Err(AosError::PolicyViolation(format!(
                            "Duplicate process ID {} for tenant {}",
                            context.process_id, context.tenant_id
                        )));
                    }
                }
            }
            ProcessModel::Shared => {
                // Shared process model - validate isolation mechanisms
                self.validate_shared_process_isolation(tenant_contexts)?;
            }
            ProcessModel::Hybrid => {
                // Hybrid model - validate both approaches
                self.validate_hybrid_process_isolation(tenant_contexts)?;
            }
        }

        Ok(())
    }

    /// Validate shared process isolation
    fn validate_shared_process_isolation(&self, contexts: &[TenantContext]) -> Result<()> {
        // Check that each tenant has unique UID/GID
        let mut uid_gid_pairs = std::collections::HashSet::new();
        for context in contexts {
            let pair = (context.user_id, context.group_id);
            if !uid_gid_pairs.insert(pair) {
                return Err(AosError::PolicyViolation(format!(
                    "Duplicate UID/GID pair ({}, {}) for tenant {}",
                    context.user_id, context.group_id, context.tenant_id
                )));
            }
        }

        Ok(())
    }

    /// Validate hybrid process isolation
    fn validate_hybrid_process_isolation(&self, contexts: &[TenantContext]) -> Result<()> {
        // For hybrid model, we need both process and UID/GID isolation
        self.validate_process_isolation(contexts)?;
        self.validate_shared_process_isolation(contexts)?;
        Ok(())
    }

    /// Validate Unix domain socket path
    pub fn validate_uds_path(&self, path: &PathBuf, tenant_id: &str) -> Result<()> {
        let expected_path = self.config.uds_root.join(tenant_id);
        if !path.starts_with(&expected_path) {
            return Err(AosError::PolicyViolation(format!(
                "UDS path {:?} not in tenant directory {:?}",
                path, expected_path
            )));
        }

        Ok(())
    }

    /// Validate shared memory access
    pub fn validate_shared_memory_access(&self, has_shm_access: bool) -> Result<()> {
        if self.config.forbid_shm && has_shm_access {
            Err(AosError::PolicyViolation(
                "Shared memory access is forbidden by policy".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Validate key backend
    pub fn validate_key_backend(&self, backend: &KeyBackend) -> Result<()> {
        if backend != &self.config.keys.backend {
            Err(AosError::PolicyViolation(format!(
                "Key backend {:?} does not match policy requirement {:?}",
                backend, self.config.keys.backend
            )))
        } else {
            Ok(())
        }
    }

    /// Validate file system operation
    pub fn validate_file_operation(&self, operation: &FileOperation) -> Result<()> {
        if self
            .config
            .filesystem
            .blocked_operations
            .contains(operation)
        {
            Err(AosError::PolicyViolation(format!(
                "File operation {:?} is blocked by policy",
                operation
            )))
        } else if !self
            .config
            .filesystem
            .allowed_operations
            .contains(operation)
        {
            Err(AosError::PolicyViolation(format!(
                "File operation {:?} is not allowed by policy",
                operation
            )))
        } else {
            Ok(())
        }
    }

    /// Validate network interface access
    pub fn validate_network_interface(&self, interface: &str) -> Result<()> {
        if self
            .config
            .network
            .blocked_interfaces
            .contains(&interface.to_string())
        {
            Err(AosError::PolicyViolation(format!(
                "Network interface {} is blocked by policy",
                interface
            )))
        } else if !self
            .config
            .network
            .allowed_interfaces
            .contains(&interface.to_string())
        {
            Err(AosError::PolicyViolation(format!(
                "Network interface {} is not allowed by policy",
                interface
            )))
        } else {
            Ok(())
        }
    }

    /// Check key rotation requirements
    pub fn check_key_rotation(&self, last_rotation: chrono::DateTime<chrono::Utc>) -> Result<()> {
        let rotation_interval =
            chrono::Duration::days(self.config.keys.rotation_policy.interval_days as i64);
        let next_rotation = last_rotation + rotation_interval;
        let now = chrono::Utc::now();

        if now > next_rotation {
            Err(AosError::PolicyViolation(
                "Key rotation is overdue".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Generate tenant-specific UDS path
    pub fn generate_tenant_uds_path(&self, tenant_id: &str, socket_name: &str) -> PathBuf {
        self.config.uds_root.join(tenant_id).join(socket_name)
    }

    /// Generate tenant-specific file system path
    pub fn generate_tenant_fs_path(&self, tenant_id: &str, relative_path: &str) -> PathBuf {
        self.config
            .filesystem
            .root_dir
            .join(tenant_id)
            .join(relative_path)
    }
}

impl Policy for IsolationPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Isolation
    }

    fn name(&self) -> &'static str {
        "Isolation"
    }

    fn severity(&self) -> Severity {
        Severity::Critical
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let violations = Vec::new();

        // Basic validation - in a real implementation, this would check
        // specific policy requirements

        if violations.is_empty() {
            Ok(Audit::passed(self.id()))
        } else {
            Ok(Audit::failed(self.id(), violations))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_isolation_policy_creation() {
        let config = IsolationConfig::default();
        let policy = IsolationPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Isolation);
        assert_eq!(policy.name(), "Isolation");
        assert_eq!(policy.severity(), Severity::Critical);
    }

    #[test]
    fn test_isolation_config_default() {
        let config = IsolationConfig::default();
        assert!(config.forbid_shm);
        assert!(config.filesystem.enable);
        assert!(config.network.enable);
        assert_eq!(config.keys.backend, KeyBackend::SecureEnclave);
    }

    #[test]
    fn test_validate_tenant_context() {
        let config = IsolationConfig::default();
        let policy = IsolationPolicy::new(config);

        let valid_context = TenantContext {
            tenant_id: "tenant1".to_string(),
            process_id: 1234,
            user_id: 1000,
            group_id: 1000,
            working_directory: PathBuf::from("var/run/aos/tenant1"),
            environment: HashMap::new(),
        };

        assert!(policy.validate_tenant_context(&valid_context).is_ok());

        let invalid_context = TenantContext {
            tenant_id: "".to_string(), // Empty tenant ID
            process_id: 1234,
            user_id: 1000,
            group_id: 1000,
            working_directory: PathBuf::from("var/run/aos/tenant1"),
            environment: HashMap::new(),
        };

        assert!(policy.validate_tenant_context(&invalid_context).is_err());
    }

    #[test]
    fn test_validate_process_isolation() {
        let config = IsolationConfig::default();
        let policy = IsolationPolicy::new(config);

        let contexts = vec![
            TenantContext {
                tenant_id: "tenant1".to_string(),
                process_id: 1234,
                user_id: 1000,
                group_id: 1000,
                working_directory: PathBuf::from("var/run/aos/tenant1"),
                environment: HashMap::new(),
            },
            TenantContext {
                tenant_id: "tenant2".to_string(),
                process_id: 5678,
                user_id: 1001,
                group_id: 1001,
                working_directory: PathBuf::from("var/run/aos/tenant2"),
                environment: HashMap::new(),
            },
        ];

        assert!(policy.validate_process_isolation(&contexts).is_ok());

        let duplicate_contexts = vec![
            TenantContext {
                tenant_id: "tenant1".to_string(),
                process_id: 1234,
                user_id: 1000,
                group_id: 1000,
                working_directory: PathBuf::from("var/run/aos/tenant1"),
                environment: HashMap::new(),
            },
            TenantContext {
                tenant_id: "tenant2".to_string(),
                process_id: 1234, // Duplicate process ID
                user_id: 1001,
                group_id: 1001,
                working_directory: PathBuf::from("var/run/aos/tenant2"),
                environment: HashMap::new(),
            },
        ];

        assert!(policy
            .validate_process_isolation(&duplicate_contexts)
            .is_err());
    }

    #[test]
    fn test_validate_uds_path() {
        let config = IsolationConfig::default();
        let policy = IsolationPolicy::new(config);

        let valid_path = policy.config.uds_root.join("tenant1/socket.sock");
        assert!(policy.validate_uds_path(&valid_path, "tenant1").is_ok());

        let invalid_path = PathBuf::from("/tmp/socket.sock");
        assert!(policy.validate_uds_path(&invalid_path, "tenant1").is_err());
    }

    #[test]
    fn test_validate_shared_memory_access() {
        let config = IsolationConfig::default();
        let policy = IsolationPolicy::new(config);

        // Should fail when shared memory access is detected
        assert!(policy.validate_shared_memory_access(true).is_err());

        // Should pass when no shared memory access
        assert!(policy.validate_shared_memory_access(false).is_ok());
    }

    #[test]
    fn test_validate_file_operation() {
        let config = IsolationConfig::default();
        let policy = IsolationPolicy::new(config);

        // Allowed operation
        assert!(policy.validate_file_operation(&FileOperation::Read).is_ok());

        // Blocked operation
        assert!(policy
            .validate_file_operation(&FileOperation::Execute)
            .is_err());
    }

    #[test]
    fn test_validate_network_interface() {
        let config = IsolationConfig::default();
        let policy = IsolationPolicy::new(config);

        // Allowed interface
        assert!(policy.validate_network_interface("lo").is_ok());

        // Blocked interface
        assert!(policy.validate_network_interface("eth0").is_err());
    }

    #[test]
    fn test_generate_tenant_paths() {
        let config = IsolationConfig::default();
        let policy = IsolationPolicy::new(config);

        let uds_path = policy.generate_tenant_uds_path("tenant1", "socket.sock");
        assert_eq!(
            uds_path,
            policy.config.uds_root.join("tenant1/socket.sock")
        );

        let fs_path = policy.generate_tenant_fs_path("tenant1", "data/file.txt");
        assert_eq!(fs_path, policy.config.filesystem.root_dir.join("tenant1/data/file.txt"));
    }
}
