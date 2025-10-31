//! Deterministic I/O Policy Pack
//!
//! Enforces deterministic I/O operations and file system access patterns.
//! Ensures reproducible file operations and consistent I/O behavior.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Deterministic I/O policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterministicIoConfig {
    /// Enable deterministic I/O
    pub enable: bool,
    /// I/O operation validation
    pub io_validation: IoValidation,
    /// File system constraints
    pub filesystem_constraints: FilesystemConstraints,
    /// Network I/O constraints
    pub network_constraints: NetworkConstraints,
    /// Determinism requirements
    pub determinism_requirements: DeterminismRequirements,
}

/// I/O operation validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoValidation {
    /// Validate read operations
    pub validate_reads: bool,
    /// Validate write operations
    pub validate_writes: bool,
    /// Validate file operations
    pub validate_file_ops: bool,
    /// Validate directory operations
    pub validate_dir_ops: bool,
    /// Allowed I/O patterns
    pub allowed_patterns: Vec<IoPattern>,
}

/// I/O pattern
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IoPattern {
    /// Sequential read
    SequentialRead,
    /// Random read
    RandomRead,
    /// Sequential write
    SequentialWrite,
    /// Random write
    RandomWrite,
    /// Memory-mapped I/O
    MemoryMapped,
}

/// File system constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConstraints {
    /// Enable file system constraints
    pub enable: bool,
    /// Allowed file extensions
    pub allowed_extensions: Vec<String>,
    /// Blocked file extensions
    pub blocked_extensions: Vec<String>,
    /// Maximum file size
    pub max_file_size: usize,
    /// Maximum directory depth
    pub max_directory_depth: usize,
}

/// Network I/O constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConstraints {
    /// Enable network constraints
    pub enable: bool,
    /// Allowed protocols
    pub allowed_protocols: Vec<String>,
    /// Blocked protocols
    pub blocked_protocols: Vec<String>,
    /// Maximum connection count
    pub max_connections: usize,
    /// Connection timeout
    pub connection_timeout_ms: u64,
}

/// Determinism requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismRequirements {
    /// Require deterministic file ordering
    pub require_deterministic_ordering: bool,
    /// Require deterministic timestamps
    pub require_deterministic_timestamps: bool,
    /// Require deterministic file hashes
    pub require_deterministic_hashes: bool,
    /// Determinism validation rules
    pub validation_rules: Vec<DeterminismRule>,
}

/// Determinism validation rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeterminismRule {
    /// Check file ordering
    FileOrdering,
    /// Check timestamp consistency
    TimestampConsistency,
    /// Check hash consistency
    HashConsistency,
    /// Check operation ordering
    OperationOrdering,
}

impl Default for DeterministicIoConfig {
    fn default() -> Self {
        Self {
            enable: true,
            io_validation: IoValidation {
                validate_reads: true,
                validate_writes: true,
                validate_file_ops: true,
                validate_dir_ops: true,
                allowed_patterns: vec![
                    IoPattern::SequentialRead,
                    IoPattern::SequentialWrite,
                    IoPattern::MemoryMapped,
                ],
            },
            filesystem_constraints: FilesystemConstraints {
                enable: true,
                allowed_extensions: vec![
                    ".json".to_string(),
                    ".toml".to_string(),
                    ".yaml".to_string(),
                    ".txt".to_string(),
                    ".log".to_string(),
                ],
                blocked_extensions: vec![
                    ".exe".to_string(),
                    ".dll".to_string(),
                    ".so".to_string(),
                    ".dylib".to_string(),
                ],
                max_file_size: 100 * 1024 * 1024, // 100 MB
                max_directory_depth: 10,
            },
            network_constraints: NetworkConstraints {
                enable: true,
                allowed_protocols: vec!["unix".to_string()],
                blocked_protocols: vec!["tcp".to_string(), "udp".to_string()],
                max_connections: 10,
                connection_timeout_ms: 5000,
            },
            determinism_requirements: DeterminismRequirements {
                require_deterministic_ordering: true,
                require_deterministic_timestamps: true,
                require_deterministic_hashes: true,
                validation_rules: vec![
                    DeterminismRule::FileOrdering,
                    DeterminismRule::TimestampConsistency,
                    DeterminismRule::HashConsistency,
                ],
            },
        }
    }
}

/// I/O operation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoOperationContext {
    /// Operation type
    pub operation_type: IoOperationType,
    /// File path
    pub file_path: PathBuf,
    /// Operation timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Operation size
    pub size: usize,
    /// Operation pattern
    pub pattern: IoPattern,
}

/// I/O operation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IoOperationType {
    /// Read operation
    Read,
    /// Write operation
    Write,
    /// Create operation
    Create,
    /// Delete operation
    Delete,
    /// List operation
    List,
    /// Stat operation
    Stat,
}

/// Deterministic I/O policy enforcement
pub struct DeterministicIoPolicy {
    config: DeterministicIoConfig,
}

impl DeterministicIoPolicy {
    /// Create a new deterministic I/O policy
    pub fn new(config: DeterministicIoConfig) -> Self {
        Self { config }
    }

    /// Validate I/O operation
    pub fn validate_io_operation(&self, context: &IoOperationContext) -> Result<()> {
        if !self.config.enable {
            return Ok(());
        }

        // Validate I/O pattern
        self.validate_io_pattern(&context.pattern)?;

        // Validate file system constraints
        self.validate_filesystem_constraints(&context.file_path)?;

        // Validate operation type
        self.validate_operation_type(&context.operation_type)?;

        Ok(())
    }

    /// Validate I/O pattern
    fn validate_io_pattern(&self, pattern: &IoPattern) -> Result<()> {
        if !self.config.io_validation.allowed_patterns.contains(pattern) {
            Err(AosError::PolicyViolation(format!(
                "I/O pattern {:?} is not allowed by policy",
                pattern
            )))
        } else {
            Ok(())
        }
    }

    /// Validate file system constraints
    fn validate_filesystem_constraints(&self, path: &PathBuf) -> Result<()> {
        if !self.config.filesystem_constraints.enable {
            return Ok(());
        }

        // Check file extension
        if let Some(extension) = path.extension() {
            let ext_str = extension.to_string_lossy().to_string();
            let ext_variants = [ext_str.as_str(), &format!(".{}", ext_str)];
            if self
                .config
                .filesystem_constraints
                .blocked_extensions
                .iter()
                .any(|blocked| ext_variants.contains(&blocked.as_str()))
            {
                return Err(AosError::PolicyViolation(format!(
                    "File extension {} is blocked by policy",
                    ext_str
                )));
            }
        }

        // Check directory depth
        let depth = path.components().count();
        if depth > self.config.filesystem_constraints.max_directory_depth {
            return Err(AosError::PolicyViolation(format!(
                "Directory depth {} exceeds maximum {}",
                depth, self.config.filesystem_constraints.max_directory_depth
            )));
        }

        Ok(())
    }

    /// Validate operation type
    fn validate_operation_type(&self, operation_type: &IoOperationType) -> Result<()> {
        match operation_type {
            IoOperationType::Read => {
                if !self.config.io_validation.validate_reads {
                    return Err(AosError::PolicyViolation(
                        "Read operations are not allowed by policy".to_string(),
                    ));
                }
            }
            IoOperationType::Write => {
                if !self.config.io_validation.validate_writes {
                    return Err(AosError::PolicyViolation(
                        "Write operations are not allowed by policy".to_string(),
                    ));
                }
            }
            _ => {
                // Other operation types are generally allowed
            }
        }

        Ok(())
    }

    /// Validate deterministic ordering
    pub fn validate_deterministic_ordering(&self, operations: &[IoOperationContext]) -> Result<()> {
        if !self
            .config
            .determinism_requirements
            .require_deterministic_ordering
        {
            return Ok(());
        }

        // Check that operations are ordered by timestamp
        for i in 1..operations.len() {
            if operations[i].timestamp < operations[i - 1].timestamp {
                return Err(AosError::PolicyViolation(
                    "I/O operations are not in deterministic order".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validate deterministic timestamps
    pub fn validate_deterministic_timestamps(
        &self,
        operations: &[IoOperationContext],
    ) -> Result<()> {
        if !self
            .config
            .determinism_requirements
            .require_deterministic_timestamps
        {
            return Ok(());
        }

        // Check that timestamps are consistent
        let mut last_timestamp = None;
        for operation in operations {
            if let Some(last_ts) = last_timestamp {
                if operation.timestamp < last_ts {
                    return Err(AosError::PolicyViolation(
                        "Timestamps are not deterministic".to_string(),
                    ));
                }
            }
            last_timestamp = Some(operation.timestamp);
        }

        Ok(())
    }

    /// Validate file size constraints
    pub fn validate_file_size(&self, size: usize) -> Result<()> {
        if size > self.config.filesystem_constraints.max_file_size {
            Err(AosError::PolicyViolation(format!(
                "File size {} exceeds maximum {}",
                size, self.config.filesystem_constraints.max_file_size
            )))
        } else {
            Ok(())
        }
    }

    /// Validate network constraints
    pub fn validate_network_constraints(&self, protocol: &str) -> Result<()> {
        if !self.config.network_constraints.enable {
            return Ok(());
        }

        if self
            .config
            .network_constraints
            .blocked_protocols
            .contains(&protocol.to_string())
        {
            return Err(AosError::PolicyViolation(format!(
                "Network protocol {} is blocked by policy",
                protocol
            )));
        }

        if !self
            .config
            .network_constraints
            .allowed_protocols
            .contains(&protocol.to_string())
        {
            return Err(AosError::PolicyViolation(format!(
                "Network protocol {} is not allowed by policy",
                protocol
            )));
        }

        Ok(())
    }

    /// Generate deterministic file path
    pub fn generate_deterministic_path(&self, base_path: &PathBuf, filename: &str) -> PathBuf {
        base_path.join(filename)
    }

    /// Check if I/O operation is deterministic
    pub fn is_deterministic_operation(&self, context: &IoOperationContext) -> bool {
        // Check if operation follows deterministic patterns
        matches!(
            context.pattern,
            IoPattern::SequentialRead | IoPattern::SequentialWrite
        )
    }
}

impl Policy for DeterministicIoPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::DeterministicIo
    }

    fn name(&self) -> &'static str {
        "Deterministic I/O"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
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
    use chrono::Utc;

    #[test]
    fn test_deterministic_io_policy_creation() {
        let config = DeterministicIoConfig::default();
        let policy = DeterministicIoPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::DeterministicIo);
        assert_eq!(policy.name(), "Deterministic I/O");
        assert_eq!(policy.severity(), Severity::Medium);
    }

    #[test]
    fn test_deterministic_io_config_default() {
        let config = DeterministicIoConfig::default();
        assert!(config.enable);
        assert!(config.io_validation.validate_reads);
        assert!(config.io_validation.validate_writes);
        assert!(config.filesystem_constraints.enable);
        assert!(config.network_constraints.enable);
    }

    #[test]
    fn test_validate_io_operation() {
        let config = DeterministicIoConfig::default();
        let policy = DeterministicIoPolicy::new(config);

        let valid_operation = IoOperationContext {
            operation_type: IoOperationType::Read,
            file_path: PathBuf::from("test.json"),
            timestamp: Utc::now(),
            size: 1024,
            pattern: IoPattern::SequentialRead,
        };

        assert!(policy.validate_io_operation(&valid_operation).is_ok());
    }

    #[test]
    fn test_validate_io_pattern() {
        let config = DeterministicIoConfig::default();
        let policy = DeterministicIoPolicy::new(config);

        // Valid pattern
        assert!(policy
            .validate_io_pattern(&IoPattern::SequentialRead)
            .is_ok());

        // Invalid pattern
        assert!(policy.validate_io_pattern(&IoPattern::RandomRead).is_err());
    }

    #[test]
    fn test_validate_filesystem_constraints() {
        let config = DeterministicIoConfig::default();
        let policy = DeterministicIoPolicy::new(config);

        // Valid file
        assert!(policy
            .validate_filesystem_constraints(&PathBuf::from("test.json"))
            .is_ok());

        // Blocked extension
        assert!(policy
            .validate_filesystem_constraints(&PathBuf::from("test.exe"))
            .is_err());
    }

    #[test]
    fn test_validate_deterministic_ordering() {
        let config = DeterministicIoConfig::default();
        let policy = DeterministicIoPolicy::new(config);

        let operations = vec![
            IoOperationContext {
                operation_type: IoOperationType::Read,
                file_path: PathBuf::from("test1.json"),
                timestamp: Utc::now(),
                size: 1024,
                pattern: IoPattern::SequentialRead,
            },
            IoOperationContext {
                operation_type: IoOperationType::Read,
                file_path: PathBuf::from("test2.json"),
                timestamp: Utc::now() + chrono::Duration::seconds(1),
                size: 1024,
                pattern: IoPattern::SequentialRead,
            },
        ];

        assert!(policy.validate_deterministic_ordering(&operations).is_ok());

        // Test out-of-order operations
        let mut out_of_order = operations.clone();
        out_of_order.reverse();
        assert!(policy
            .validate_deterministic_ordering(&out_of_order)
            .is_err());
    }

    #[test]
    fn test_validate_file_size() {
        let config = DeterministicIoConfig::default();
        let policy = DeterministicIoPolicy::new(config);

        // Valid size
        assert!(policy.validate_file_size(1024).is_ok());

        // Invalid size
        assert!(policy.validate_file_size(200 * 1024 * 1024).is_err());
    }

    #[test]
    fn test_validate_network_constraints() {
        let config = DeterministicIoConfig::default();
        let policy = DeterministicIoPolicy::new(config);

        // Valid protocol
        assert!(policy.validate_network_constraints("unix").is_ok());

        // Blocked protocol
        assert!(policy.validate_network_constraints("tcp").is_err());

        // Not allowed protocol
        assert!(policy.validate_network_constraints("http").is_err());
    }

    #[test]
    fn test_is_deterministic_operation() {
        let config = DeterministicIoConfig::default();
        let policy = DeterministicIoPolicy::new(config);

        let deterministic_op = IoOperationContext {
            operation_type: IoOperationType::Read,
            file_path: PathBuf::from("test.json"),
            timestamp: Utc::now(),
            size: 1024,
            pattern: IoPattern::SequentialRead,
        };

        assert!(policy.is_deterministic_operation(&deterministic_op));

        let non_deterministic_op = IoOperationContext {
            operation_type: IoOperationType::Read,
            file_path: PathBuf::from("test.json"),
            timestamp: Utc::now(),
            size: 1024,
            pattern: IoPattern::RandomRead,
        };

        assert!(!policy.is_deterministic_operation(&non_deterministic_op));
    }

    #[test]
    fn test_generate_deterministic_path() {
        let config = DeterministicIoConfig::default();
        let policy = DeterministicIoPolicy::new(config);

        let base_path = PathBuf::from("/tmp");
        let filename = "test.json";
        let result = policy.generate_deterministic_path(&base_path, filename);

        assert_eq!(result, PathBuf::from("/tmp/test.json"));
    }
}
