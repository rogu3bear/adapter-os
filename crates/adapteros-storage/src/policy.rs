//! Storage policy enforcement
//!
//! Implements storage policies and constraints for tenant storage.

use crate::secure_fs::traversal::check_path_traversal;
use crate::StorageConfig;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    root_path: Option<PathBuf>,
}

impl StoragePolicyEngine {
    /// Create a new storage policy engine
    pub fn new(policy: StoragePolicy) -> Self {
        Self {
            policy,
            root_path: None,
        }
    }

    /// Provide a root path for usage-based policy evaluation
    pub fn with_root_path(mut self, root_path: PathBuf) -> Self {
        self.root_path = Some(root_path);
        self
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
        match file_path.extension() {
            Some(extension) => {
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
            None => {
                if !self.policy.constraints.allowed_extensions.is_empty() {
                    return Err(AosError::PolicyViolation(
                        "File extension is required by policy".to_string(),
                    ));
                }
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
                return Err(AosError::PolicyViolation(
                    "File path does not match any allowed pattern".to_string(),
                ));
            }
        }

        // Evaluate rules
        for rule in &self.policy.rules {
            self.evaluate_rule(rule, file_path, operation)?
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
            StorageConditionType::FileAge => {
                let metadata = match fs::metadata(file_path) {
                    Ok(meta) => meta,
                    Err(_) => return Ok(false),
                };
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let age = SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or(Duration::from_secs(0))
                    .as_secs();
                let condition_value = parse_duration_seconds(&condition.value)?;
                Ok(self.compare_values(age, condition_value, &condition.operator))
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
            StorageConditionType::UsagePercentage => {
                let usage_pct = self.compute_usage_percentage(file_path)?;
                let condition_value: f32 = condition.value.parse().map_err(|e| {
                    AosError::PolicyViolation(format!(
                        "Invalid usage percentage condition value: {}",
                        e
                    ))
                })?;
                Ok(self.compare_values(usage_pct, condition_value, &condition.operator))
            }
        }
    }

    /// Execute a storage action
    fn execute_action(
        &self,
        action: &StorageAction,
        file_path: &PathBuf,
        _operation: &FileOperation,
    ) -> Result<()> {
        match action.action_type {
            StorageActionType::Allow => Ok(()),
            StorageActionType::Deny => Err(AosError::PolicyViolation(
                "File operation denied by policy rule".to_string(),
            )),
            StorageActionType::Delete => {
                self.enforce_path_safety(file_path)?;
                if file_path.exists() {
                    tracing::info!(
                        action = "delete",
                        path = %file_path.display(),
                        "Storage policy deleting file"
                    );
                    fs::remove_file(file_path).map_err(|e| {
                        AosError::Io(format!(
                            "Failed to delete file {}: {}",
                            file_path.display(),
                            e
                        ))
                    })?;
                } else {
                    tracing::warn!(
                        action = "delete",
                        path = %file_path.display(),
                        "Storage policy delete skipped (file missing)"
                    );
                }
                Ok(())
            }
            StorageActionType::Move | StorageActionType::Copy => {
                self.enforce_path_safety(file_path)?;
                let dest_key = if action.parameters.contains_key("destination") {
                    "destination"
                } else {
                    "dest"
                };
                let dest = action
                    .parameters
                    .get(dest_key)
                    .ok_or_else(|| {
                        AosError::PolicyViolation(
                            "Missing destination for move/copy action".to_string(),
                        )
                    })?
                    .to_string();
                let dest_path = PathBuf::from(dest);
                self.enforce_path_safety(&dest_path)?;

                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        AosError::Io(format!(
                            "Failed to create destination directory {}: {}",
                            parent.display(),
                            e
                        ))
                    })?;
                }

                match action.action_type {
                    StorageActionType::Move => {
                        tracing::info!(
                            action = "move",
                            source = %file_path.display(),
                            destination = %dest_path.display(),
                            "Storage policy moving file"
                        );
                        fs::rename(file_path, &dest_path).map_err(|e| {
                            AosError::Io(format!(
                                "Failed to move file {} -> {}: {}",
                                file_path.display(),
                                dest_path.display(),
                                e
                            ))
                        })?;
                    }
                    StorageActionType::Copy => {
                        tracing::info!(
                            action = "copy",
                            source = %file_path.display(),
                            destination = %dest_path.display(),
                            "Storage policy copying file"
                        );
                        if file_path.is_dir() {
                            return Err(AosError::PolicyViolation(
                                "Copy action does not support directories".to_string(),
                            ));
                        }
                        fs::copy(file_path, &dest_path).map_err(|e| {
                            AosError::Io(format!(
                                "Failed to copy file {} -> {}: {}",
                                file_path.display(),
                                dest_path.display(),
                                e
                            ))
                        })?;
                    }
                    _ => {}
                }

                Ok(())
            }
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

    fn compute_usage_percentage(&self, file_path: &Path) -> Result<f32> {
        let root = self
            .root_path
            .clone()
            .or_else(|| file_path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| file_path.to_path_buf());

        let mut used_bytes = 0u64;

        if root.exists() {
            if root.is_dir() {
                self.walk_usage(&root, &mut used_bytes)?;
            } else if root.is_file() {
                let metadata = fs::metadata(&root).map_err(|e| {
                    AosError::Io(format!("Failed to read metadata {}: {}", root.display(), e))
                })?;
                used_bytes = metadata.len();
            }
        }

        if self.policy.config.max_disk_space_bytes == 0 {
            return Err(AosError::PolicyViolation(
                "max_disk_space_bytes is zero".to_string(),
            ));
        }

        Ok((used_bytes as f32 / self.policy.config.max_disk_space_bytes as f32) * 100.0)
    }

    fn walk_usage(&self, path: &PathBuf, used_bytes: &mut u64) -> Result<()> {
        let entries = fs::read_dir(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read directory {}: {}",
                path.display(),
                e
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| AosError::Io(format!("Failed to read entry: {}", e)))?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                self.walk_usage(&entry_path, used_bytes)?;
            } else if entry_path.is_file() {
                let metadata = entry.metadata().map_err(|e| {
                    AosError::Io(format!(
                        "Failed to read metadata {}: {}",
                        entry_path.display(),
                        e
                    ))
                })?;
                *used_bytes += metadata.len();
            }
        }

        Ok(())
    }

    fn enforce_path_safety(&self, path: &PathBuf) -> Result<()> {
        check_path_traversal(path)?;

        if let Some(base) = &self.root_path {
            let canonical_base = base.canonicalize().map_err(|e| {
                AosError::Io(format!(
                    "Failed to canonicalize base path for validation: {}",
                    e
                ))
            })?;

            let canonical_path = if path.exists() {
                path.canonicalize().map_err(|e| {
                    AosError::Io(format!("Failed to canonicalize path for validation: {}", e))
                })?
            } else if path.is_absolute() {
                if let Some(parent) = path.parent() {
                    if parent.exists() {
                        let canonical_parent = parent.canonicalize().map_err(|e| {
                            AosError::Io(format!(
                                "Failed to canonicalize parent for validation: {}",
                                e
                            ))
                        })?;
                        if let Some(name) = path.file_name() {
                            canonical_parent.join(name)
                        } else {
                            canonical_parent
                        }
                    } else {
                        path.to_path_buf()
                    }
                } else {
                    path.to_path_buf()
                }
            } else {
                canonical_base.join(path)
            };

            if !canonical_path.starts_with(&canonical_base) {
                return Err(AosError::PolicyViolation(format!(
                    "Path {} is outside configured storage root {}",
                    path.display(),
                    base.display()
                )));
            }
        }

        Ok(())
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

fn parse_duration_seconds(value: &str) -> Result<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AosError::PolicyViolation(
            "File age condition value is empty".to_string(),
        ));
    }

    let (number_part, multiplier) = match trimmed.chars().last() {
        Some('s') | Some('S') => (&trimmed[..trimmed.len() - 1], 1),
        Some('m') | Some('M') => (&trimmed[..trimmed.len() - 1], 60),
        Some('h') | Some('H') => (&trimmed[..trimmed.len() - 1], 60 * 60),
        Some('d') | Some('D') => (&trimmed[..trimmed.len() - 1], 60 * 60 * 24),
        Some(_) => (trimmed, 1),
        None => (trimmed, 1),
    };

    let value: u64 = number_part.parse().map_err(|e| {
        AosError::PolicyViolation(format!("Invalid duration value '{}': {}", number_part, e))
    })?;

    Ok(value.saturating_mul(multiplier))
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

    #[test]
    fn test_storage_policy_engine() -> Result<()> {
        let temp_dir = crate::tests::new_test_tempdir()?;
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
        let temp_dir = crate::tests::new_test_tempdir()?;
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
