//! Storage policy enforcement
//!
//! Implements storage policies and constraints for tenant storage.

use crate::StorageConfig;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Storage policy for a tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePolicy {
    /// Policy name
    pub name: String,
    /// Policy description
    pub description: String,
    /// Storage configuration
    pub config: StorageConfig,
    /// Policy constraints
    pub constraints: StorageConstraints,
    /// Policy rules
    pub rules: Vec<StorageRule>,
}

/// Storage constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConstraints {
    /// Maximum file size in bytes
    pub max_file_size_bytes: u64,
    /// Maximum directory depth
    pub max_directory_depth: u32,
    /// Allowed file extensions
    pub allowed_extensions: Vec<String>,
    /// Blocked file extensions
    pub blocked_extensions: Vec<String>,
    /// Allowed file patterns
    pub allowed_patterns: Vec<String>,
    /// Blocked file patterns
    pub blocked_patterns: Vec<String>,
}

/// Storage rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageRule {
    /// Rule name
    pub name: String,
    /// Rule description
    pub description: String,
    /// Rule type
    pub rule_type: StorageRuleType,
    /// Rule conditions
    pub conditions: Vec<StorageCondition>,
    /// Rule actions
    pub actions: Vec<StorageAction>,
}

/// Storage rule type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageRuleType {
    /// Quota enforcement rule
    QuotaEnforcement,
    /// Cleanup rule
    Cleanup,
    /// Access control rule
    AccessControl,
    /// Security rule
    Security,
}

/// Storage condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageCondition {
    /// Condition type
    pub condition_type: StorageConditionType,
    /// Condition value
    pub value: String,
    /// Condition operator
    pub operator: StorageOperator,
}

/// Storage condition type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageConditionType {
    /// File size condition
    FileSize,
    /// File age condition
    FileAge,
    /// File extension condition
    FileExtension,
    /// File pattern condition
    FilePattern,
    /// Directory depth condition
    DirectoryDepth,
    /// Usage percentage condition
    UsagePercentage,
}

/// Storage operator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageOperator {
    /// Equal to
    Equal,
    /// Not equal to
    NotEqual,
    /// Greater than
    GreaterThan,
    /// Less than
    LessThan,
    /// Greater than or equal to
    GreaterThanOrEqual,
    /// Less than or equal to
    LessThanOrEqual,
    /// Contains
    Contains,
    /// Not contains
    NotContains,
    /// Matches pattern
    Matches,
    /// Not matches pattern
    NotMatches,
}

/// Storage action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageAction {
    /// Action type
    pub action_type: StorageActionType,
    /// Action parameters
    pub parameters: std::collections::HashMap<String, String>,
}

/// Storage action type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageActionType {
    /// Allow action
    Allow,
    /// Deny action
    Deny,
    /// Delete action
    Delete,
    /// Move action
    Move,
    /// Copy action
    Copy,
    /// Alert action
    Alert,
    /// Log action
    Log,
}

/// Storage policy engine
pub struct StoragePolicyEngine {
    policy: StoragePolicy,
}

impl StoragePolicyEngine {
    /// Create a new storage policy engine
    pub fn new(policy: StoragePolicy) -> Self {
        Self { policy }
    }

    /// Validate a file operation
    pub fn validate_file_operation(
        &self,
        file_path: &PathBuf,
        operation: &FileOperation,
    ) -> Result<()> {
        // Check file size constraints
        if let Ok(metadata) = std::fs::metadata(file_path) {
            if metadata.len() > self.policy.constraints.max_file_size_bytes {
                return Err(AosError::PolicyViolation(format!(
                    "File size {} exceeds maximum {} bytes",
                    metadata.len(),
                    self.policy.constraints.max_file_size_bytes
                )));
            }
        }

        // Check directory depth constraints
        let depth = file_path.components().count();
        if depth > self.policy.constraints.max_directory_depth as usize {
            return Err(AosError::PolicyViolation(format!(
                "Directory depth {} exceeds maximum {}",
                depth, self.policy.constraints.max_directory_depth
            )));
        }

        // Check file extension constraints
        if let Some(extension) = file_path.extension() {
            let ext_str = extension.to_string_lossy().to_string();

            // Check blocked extensions
            if self
                .policy
                .constraints
                .blocked_extensions
                .contains(&ext_str)
            {
                return Err(AosError::PolicyViolation(format!(
                    "File extension {} is blocked by policy",
                    ext_str
                )));
            }

            // Check allowed extensions (if specified)
            if !self.policy.constraints.allowed_extensions.is_empty()
                && !self
                    .policy
                    .constraints
                    .allowed_extensions
                    .contains(&ext_str)
            {
                return Err(AosError::PolicyViolation(format!(
                    "File extension {} is not allowed by policy",
                    ext_str
                )));
            }
        }

        // Check file pattern constraints
        let file_path_str = file_path.to_string_lossy().to_string();

        // Check blocked patterns
        for pattern in &self.policy.constraints.blocked_patterns {
            if Self::matches_pattern(&file_path_str, pattern) {
                return Err(AosError::PolicyViolation(format!(
                    "File path matches blocked pattern: {}",
                    pattern
                )));
            }
        }

        // Check allowed patterns (if specified)
        if !self.policy.constraints.allowed_patterns.is_empty() {
            let mut matches_allowed = false;
            for pattern in &self.policy.constraints.allowed_patterns {
                if Self::matches_pattern(&file_path_str, pattern) {
                    matches_allowed = true;
                    break;
                }
            }

            if !matches_allowed {
                return Err(AosError::PolicyViolation(format!(
                    "File path does not match any allowed pattern"
                )));
            }
        }

        // Evaluate rules
        for rule in &self.policy.rules {
            if let Err(e) = self.evaluate_rule(rule, file_path, operation) {
                return Err(e);
            }
        }

        Ok(())
    }

    /// Evaluate a storage rule
    fn evaluate_rule(
        &self,
        rule: &StorageRule,
        file_path: &PathBuf,
        operation: &FileOperation,
    ) -> Result<()> {
        // Check if all conditions are met
        let mut conditions_met = true;

        for condition in &rule.conditions {
            if !self.evaluate_condition(condition, file_path, operation)? {
                conditions_met = false;
                break;
            }
        }

        if conditions_met {
            // Execute actions
            for action in &rule.actions {
                self.execute_action(action, file_path, operation)?;
            }
        }

        Ok(())
    }

    /// Evaluate a storage condition
    fn evaluate_condition(
        &self,
        condition: &StorageCondition,
        file_path: &PathBuf,
        _operation: &FileOperation,
    ) -> Result<bool> {
        match condition.condition_type {
            StorageConditionType::FileSize => {
                if let Ok(metadata) = std::fs::metadata(file_path) {
                    let file_size = metadata.len();
                    let condition_value: u64 = condition.value.parse().map_err(|e| {
                        AosError::PolicyViolation(format!(
                            "Invalid file size condition value: {}",
                            e
                        ))
                    })?;

                    Ok(self.compare_values(file_size, condition_value, &condition.operator))
                } else {
                    Ok(false)
                }
            }
            StorageConditionType::FileExtension => {
                if let Some(extension) = file_path.extension() {
                    let ext_str = extension.to_string_lossy().to_string();
                    Ok(self.compare_strings(&ext_str, &condition.value, &condition.operator))
                } else {
                    Ok(false)
                }
            }
            StorageConditionType::FilePattern => {
                let file_path_str = file_path.to_string_lossy().to_string();
                Ok(self.compare_strings(&file_path_str, &condition.value, &condition.operator))
            }
            StorageConditionType::DirectoryDepth => {
                let depth = file_path.components().count();
                let condition_value: usize = condition.value.parse().map_err(|e| {
                    AosError::PolicyViolation(format!(
                        "Invalid directory depth condition value: {}",
                        e
                    ))
                })?;

                Ok(self.compare_values(depth, condition_value, &condition.operator))
            }
            _ => {
                // Other condition types not implemented yet
                Ok(true)
            }
        }
    }

    /// Execute a storage action
    fn execute_action(
        &self,
        action: &StorageAction,
        _file_path: &PathBuf,
        _operation: &FileOperation,
    ) -> Result<()> {
        match action.action_type {
            StorageActionType::Deny => Err(AosError::PolicyViolation(format!(
                "File operation denied by policy rule"
            ))),
            StorageActionType::Alert => {
                // Log alert (in real implementation, this would send an alert)
                tracing::warn!(
                    "Storage policy alert: {}",
                    action
                        .parameters
                        .get("message")
                        .unwrap_or(&"Unknown alert".to_string())
                );
                Ok(())
            }
            StorageActionType::Log => {
                // Log action (in real implementation, this would log to telemetry)
                tracing::info!(
                    "Storage policy log: {}",
                    action
                        .parameters
                        .get("message")
                        .unwrap_or(&"Unknown log".to_string())
                );
                Ok(())
            }
            _ => {
                // Other action types not implemented yet
                Ok(())
            }
        }
    }

    /// Compare two values using an operator
    fn compare_values<T: PartialOrd>(&self, left: T, right: T, operator: &StorageOperator) -> bool {
        match operator {
            StorageOperator::Equal => left == right,
            StorageOperator::NotEqual => left != right,
            StorageOperator::GreaterThan => left > right,
            StorageOperator::LessThan => left < right,
            StorageOperator::GreaterThanOrEqual => left >= right,
            StorageOperator::LessThanOrEqual => left <= right,
            _ => false, // String operators not applicable to values
        }
    }

    /// Compare two strings using an operator
    fn compare_strings(&self, left: &str, right: &str, operator: &StorageOperator) -> bool {
        match operator {
            StorageOperator::Equal => left == right,
            StorageOperator::NotEqual => left != right,
            StorageOperator::Contains => left.contains(right),
            StorageOperator::NotContains => !left.contains(right),
            StorageOperator::Matches => Self::matches_pattern(left, right),
            StorageOperator::NotMatches => !Self::matches_pattern(left, right),
            _ => false, // Numeric operators not applicable to strings
        }
    }

    /// Check if a string matches a pattern
    fn matches_pattern(text: &str, pattern: &str) -> bool {
        // Simple glob pattern matching
        // In production, use a proper glob library
        if pattern.contains('*') {
            let regex_pattern = pattern.replace('*', ".*");
            if let Ok(regex) = regex::Regex::new(&format!("^{}$", regex_pattern)) {
                regex.is_match(text)
            } else {
                false
            }
        } else {
            text == pattern
        }
    }
}

/// File operation type
#[derive(Debug, Clone)]
pub enum FileOperation {
    /// Read operation
    Read,
    /// Write operation
    Write,
    /// Delete operation
    Delete,
    /// Create operation
    Create,
    /// Move operation
    Move,
    /// Copy operation
    Copy,
}

impl Default for StorageConstraints {
    fn default() -> Self {
        Self {
            max_file_size_bytes: 100 * 1024 * 1024, // 100MB
            max_directory_depth: 10,
            allowed_extensions: vec![
                "txt".to_string(),
                "json".to_string(),
                "jsonl".to_string(),
                "toml".to_string(),
                "yaml".to_string(),
                "aos".to_string(),
                "safetensors".to_string(),
            ],
            blocked_extensions: vec![
                "exe".to_string(),
                "bat".to_string(),
                "sh".to_string(),
                "ps1".to_string(),
            ],
            allowed_patterns: vec![],
            blocked_patterns: vec![
                "*.tmp".to_string(),
                "*.log".to_string(),
                "*.cache".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_storage_policy_engine() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let policy = StoragePolicy {
            name: "test_policy".to_string(),
            description: "Test storage policy".to_string(),
            config: StorageConfig::default(),
            constraints: StorageConstraints::default(),
            rules: vec![],
        };

        let engine = StoragePolicyEngine::new(policy);
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "hello")?;

        // Test file operation validation
        engine.validate_file_operation(&test_file, &FileOperation::Read)?;

        Ok(())
    }

    #[test]
    fn test_file_extension_constraints() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut constraints = StorageConstraints::default();
        constraints.blocked_extensions.push("exe".to_string());

        let policy = StoragePolicy {
            name: "test_policy".to_string(),
            description: "Test storage policy".to_string(),
            config: StorageConfig::default(),
            constraints,
            rules: vec![],
        };

        let engine = StoragePolicyEngine::new(policy);
        let test_file = temp_dir.path().join("test.exe");

        // Test blocked extension
        let result = engine.validate_file_operation(&test_file, &FileOperation::Read);
        assert!(result.is_err());

        Ok(())
    }
}
