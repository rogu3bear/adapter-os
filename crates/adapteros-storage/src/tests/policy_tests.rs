//! Storage policy tests
//!
//! Tests for policy enforcement, constraints validation, and rule evaluation.

use super::new_test_tempdir;
use crate::policy::{
    FileOperation, StorageAction, StorageActionType, StorageCondition, StorageConditionType,
    StorageConstraints, StorageOperator, StoragePolicy, StoragePolicyEngine, StorageRule,
    StorageRuleType,
};
use crate::StorageConfig;
use adapteros_core::Result;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn create_default_policy() -> StoragePolicy {
    StoragePolicy {
        name: "test_policy".to_string(),
        description: "Test storage policy".to_string(),
        config: StorageConfig::default(),
        constraints: StorageConstraints::default(),
        rules: vec![],
    }
}

#[test]
fn test_policy_engine_creation() -> Result<()> {
    let policy = create_default_policy();
    let engine = StoragePolicyEngine::new(policy);

    let temp_dir = new_test_tempdir()?;
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "hello")?;

    engine.validate_file_operation(&test_file, &FileOperation::Read)?;

    Ok(())
}

#[test]
fn test_allowed_file_extensions() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let policy = create_default_policy();
    let engine = StoragePolicyEngine::new(policy);

    // Test allowed extensions
    let txt_file = temp_dir.path().join("test.txt");
    fs::write(&txt_file, "text content")?;
    assert!(engine
        .validate_file_operation(&txt_file, &FileOperation::Read)
        .is_ok());

    let json_file = temp_dir.path().join("test.json");
    fs::write(&json_file, "{}")?;
    assert!(engine
        .validate_file_operation(&json_file, &FileOperation::Read)
        .is_ok());

    Ok(())
}

#[test]
fn test_blocked_file_extensions() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let policy = create_default_policy();
    let engine = StoragePolicyEngine::new(policy);

    // Test blocked extensions
    let exe_file = temp_dir.path().join("test.exe");
    let result = engine.validate_file_operation(&exe_file, &FileOperation::Write);
    assert!(result.is_err(), "Should block .exe files");

    let bat_file = temp_dir.path().join("test.bat");
    let result = engine.validate_file_operation(&bat_file, &FileOperation::Write);
    assert!(result.is_err(), "Should block .bat files");

    Ok(())
}

#[test]
fn test_file_size_constraint() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let mut constraints = StorageConstraints::default();
    constraints.max_file_size_bytes = 100;

    let policy = StoragePolicy {
        name: "size_policy".to_string(),
        description: "File size policy".to_string(),
        config: StorageConfig::default(),
        constraints,
        rules: vec![],
    };

    let engine = StoragePolicyEngine::new(policy);

    // Create a file larger than limit
    let large_file = temp_dir.path().join("large.txt");
    fs::write(&large_file, "a".repeat(200))?;

    let result = engine.validate_file_operation(&large_file, &FileOperation::Write);
    assert!(result.is_err(), "Should reject files exceeding size limit");

    Ok(())
}

#[test]
fn test_directory_depth_constraint() -> Result<()> {
    let _temp_dir = new_test_tempdir()?;
    let mut constraints = StorageConstraints::default();
    constraints.max_directory_depth = 3;

    let policy = StoragePolicy {
        name: "depth_policy".to_string(),
        description: "Directory depth policy".to_string(),
        config: StorageConfig::default(),
        constraints,
        rules: vec![],
    };

    let engine = StoragePolicyEngine::new(policy);

    // Create a deeply nested path
    let deep_path = PathBuf::from("/a/b/c/d/e/file.txt");
    let result = engine.validate_file_operation(&deep_path, &FileOperation::Write);
    assert!(result.is_err(), "Should reject deeply nested paths");

    Ok(())
}

#[test]
fn test_blocked_patterns() -> Result<()> {
    let _temp_dir = new_test_tempdir()?;
    let mut constraints = StorageConstraints::default();
    constraints.blocked_patterns = vec!["*/secret/*".to_string()];

    let policy = StoragePolicy {
        name: "pattern_policy".to_string(),
        description: "Pattern blocking policy".to_string(),
        config: StorageConfig::default(),
        constraints,
        rules: vec![],
    };

    let engine = StoragePolicyEngine::new(policy);

    let blocked_path = PathBuf::from("/data/secret/file.txt");
    let result = engine.validate_file_operation(&blocked_path, &FileOperation::Read);
    assert!(
        result.is_err(),
        "Should block paths matching blocked patterns"
    );

    Ok(())
}

#[test]
fn test_allowed_patterns() -> Result<()> {
    let _temp_dir = new_test_tempdir()?;
    let mut constraints = StorageConstraints::default();
    constraints.allowed_patterns = vec!["*/public/*".to_string()];
    constraints.allowed_extensions = vec![]; // Clear default allowed extensions

    let policy = StoragePolicy {
        name: "pattern_policy".to_string(),
        description: "Pattern allowing policy".to_string(),
        config: StorageConfig::default(),
        constraints,
        rules: vec![],
    };

    let engine = StoragePolicyEngine::new(policy);

    let allowed_path = PathBuf::from("/data/public/file.txt");
    let result = engine.validate_file_operation(&allowed_path, &FileOperation::Read);
    assert!(
        result.is_ok(),
        "Should allow paths matching allowed patterns"
    );

    let disallowed_path = PathBuf::from("/data/private/file.txt");
    let result = engine.validate_file_operation(&disallowed_path, &FileOperation::Read);
    assert!(
        result.is_err(),
        "Should reject paths not matching allowed patterns"
    );

    Ok(())
}

#[test]
fn test_rule_with_deny_action() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let deny_action = StorageAction {
        action_type: StorageActionType::Deny,
        parameters: HashMap::new(),
    };

    let rule = StorageRule {
        name: "deny_rule".to_string(),
        description: "Always deny".to_string(),
        rule_type: StorageRuleType::Security,
        conditions: vec![],
        actions: vec![deny_action],
    };

    let mut policy = create_default_policy();
    policy.rules = vec![rule];

    let engine = StoragePolicyEngine::new(policy);

    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "content")?;

    let result = engine.validate_file_operation(&test_file, &FileOperation::Read);
    assert!(
        result.is_err(),
        "Rule with deny action should reject operation"
    );

    Ok(())
}

#[test]
fn test_rule_with_file_size_condition() -> Result<()> {
    let temp_dir = new_test_tempdir()?;

    let condition = StorageCondition {
        condition_type: StorageConditionType::FileSize,
        value: "100".to_string(),
        operator: StorageOperator::GreaterThan,
    };

    let deny_action = StorageAction {
        action_type: StorageActionType::Deny,
        parameters: HashMap::new(),
    };

    let rule = StorageRule {
        name: "size_rule".to_string(),
        description: "Deny large files".to_string(),
        rule_type: StorageRuleType::Security,
        conditions: vec![condition],
        actions: vec![deny_action],
    };

    let mut policy = create_default_policy();
    policy.constraints.max_file_size_bytes = 1000000; // Set high to not trigger constraint check
    policy.rules = vec![rule];

    let engine = StoragePolicyEngine::new(policy);

    // Create a large file (200 bytes > 100)
    let large_file = temp_dir.path().join("large.txt");
    fs::write(&large_file, "a".repeat(200))?;

    let result = engine.validate_file_operation(&large_file, &FileOperation::Write);
    assert!(result.is_err(), "Should deny files larger than condition");

    // Create a small file (50 bytes < 100)
    let small_file = temp_dir.path().join("small.txt");
    fs::write(&small_file, "a".repeat(50))?;

    let result = engine.validate_file_operation(&small_file, &FileOperation::Write);
    assert!(result.is_ok(), "Should allow files smaller than condition");

    Ok(())
}

#[test]
fn test_rule_with_extension_condition() -> Result<()> {
    let temp_dir = new_test_tempdir()?;

    let condition = StorageCondition {
        condition_type: StorageConditionType::FileExtension,
        value: "tmp".to_string(),
        operator: StorageOperator::Equal,
    };

    let deny_action = StorageAction {
        action_type: StorageActionType::Deny,
        parameters: HashMap::new(),
    };

    let rule = StorageRule {
        name: "extension_rule".to_string(),
        description: "Deny tmp files".to_string(),
        rule_type: StorageRuleType::Security,
        conditions: vec![condition],
        actions: vec![deny_action],
    };

    let mut policy = create_default_policy();
    policy.constraints.blocked_extensions = vec![]; // Clear default blocked extensions
    policy.rules = vec![rule];

    let engine = StoragePolicyEngine::new(policy);

    let tmp_file = temp_dir.path().join("test.tmp");
    let result = engine.validate_file_operation(&tmp_file, &FileOperation::Write);
    assert!(result.is_err(), "Should deny .tmp files");

    let txt_file = temp_dir.path().join("test.txt");
    let result = engine.validate_file_operation(&txt_file, &FileOperation::Write);
    assert!(result.is_ok(), "Should allow .txt files");

    Ok(())
}

#[test]
fn test_storage_operator_comparisons() {
    assert_eq!(StorageOperator::Equal, StorageOperator::Equal);
    assert_ne!(StorageOperator::Equal, StorageOperator::NotEqual);
}

#[test]
fn test_storage_constraints_default() {
    let constraints = StorageConstraints::default();

    assert_eq!(constraints.max_file_size_bytes, 100 * 1024 * 1024);
    assert_eq!(constraints.max_directory_depth, 10);
    assert!(constraints.allowed_extensions.contains(&"txt".to_string()));
    assert!(constraints.blocked_extensions.contains(&"exe".to_string()));
}

#[test]
fn test_file_operation_types() {
    let _read = FileOperation::Read;
    let _write = FileOperation::Write;
    let _delete = FileOperation::Delete;
    let _create = FileOperation::Create;
    let _move = FileOperation::Move;
    let _copy = FileOperation::Copy;
}

#[test]
fn test_multiple_conditions_all_must_match() -> Result<()> {
    let temp_dir = new_test_tempdir()?;

    let condition1 = StorageCondition {
        condition_type: StorageConditionType::FileExtension,
        value: "txt".to_string(),
        operator: StorageOperator::Equal,
    };

    let condition2 = StorageCondition {
        condition_type: StorageConditionType::FileSize,
        value: "100".to_string(),
        operator: StorageOperator::GreaterThan,
    };

    let deny_action = StorageAction {
        action_type: StorageActionType::Deny,
        parameters: HashMap::new(),
    };

    let rule = StorageRule {
        name: "multi_condition_rule".to_string(),
        description: "Deny large txt files".to_string(),
        rule_type: StorageRuleType::Security,
        conditions: vec![condition1, condition2],
        actions: vec![deny_action],
    };

    let mut policy = create_default_policy();
    policy.constraints.max_file_size_bytes = 1000000;
    policy.rules = vec![rule];

    let engine = StoragePolicyEngine::new(policy);

    // Create large txt file - both conditions match
    let large_txt = temp_dir.path().join("large.txt");
    fs::write(&large_txt, "a".repeat(200))?;
    let result = engine.validate_file_operation(&large_txt, &FileOperation::Write);
    assert!(result.is_err(), "Should deny when all conditions match");

    // Create small txt file - only first condition matches
    let small_txt = temp_dir.path().join("small.txt");
    fs::write(&small_txt, "small")?;
    let result = engine.validate_file_operation(&small_txt, &FileOperation::Write);
    assert!(result.is_ok(), "Should allow when not all conditions match");

    Ok(())
}

#[test]
fn test_alert_action() -> Result<()> {
    let temp_dir = new_test_tempdir()?;

    let mut alert_params = HashMap::new();
    alert_params.insert("message".to_string(), "Test alert".to_string());

    let alert_action = StorageAction {
        action_type: StorageActionType::Alert,
        parameters: alert_params,
    };

    let rule = StorageRule {
        name: "alert_rule".to_string(),
        description: "Send alert".to_string(),
        rule_type: StorageRuleType::Security,
        conditions: vec![],
        actions: vec![alert_action],
    };

    let mut policy = create_default_policy();
    policy.rules = vec![rule];

    let engine = StoragePolicyEngine::new(policy);

    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "content")?;

    // Alert action should not prevent operation
    let result = engine.validate_file_operation(&test_file, &FileOperation::Read);
    assert!(result.is_ok(), "Alert action should not block operation");

    Ok(())
}

#[test]
fn test_log_action() -> Result<()> {
    let temp_dir = new_test_tempdir()?;

    let mut log_params = HashMap::new();
    log_params.insert("message".to_string(), "Test log".to_string());

    let log_action = StorageAction {
        action_type: StorageActionType::Log,
        parameters: log_params,
    };

    let rule = StorageRule {
        name: "log_rule".to_string(),
        description: "Log access".to_string(),
        rule_type: StorageRuleType::Security,
        conditions: vec![],
        actions: vec![log_action],
    };

    let mut policy = create_default_policy();
    policy.rules = vec![rule];

    let engine = StoragePolicyEngine::new(policy);

    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "content")?;

    // Log action should not prevent operation
    let result = engine.validate_file_operation(&test_file, &FileOperation::Read);
    assert!(result.is_ok(), "Log action should not block operation");

    Ok(())
}

#[test]
fn test_pattern_matching_wildcard() -> Result<()> {
    let _temp_dir = new_test_tempdir()?;
    let mut constraints = StorageConstraints::default();
    constraints.blocked_patterns = vec!["*.tmp".to_string()];

    let policy = StoragePolicy {
        name: "wildcard_policy".to_string(),
        description: "Wildcard pattern policy".to_string(),
        config: StorageConfig::default(),
        constraints,
        rules: vec![],
    };

    let engine = StoragePolicyEngine::new(policy);

    let tmp_file = PathBuf::from("test.tmp");
    let result = engine.validate_file_operation(&tmp_file, &FileOperation::Write);
    assert!(result.is_err(), "Should block *.tmp pattern");

    let txt_file = PathBuf::from("test.txt");
    let result = engine.validate_file_operation(&txt_file, &FileOperation::Write);
    assert!(result.is_ok(), "Should allow non-matching pattern");

    Ok(())
}

#[test]
fn test_storage_rule_types() {
    let _quota = StorageRuleType::QuotaEnforcement;
    let _cleanup = StorageRuleType::Cleanup;
    let _access = StorageRuleType::AccessControl;
    let _security = StorageRuleType::Security;
}

#[test]
fn test_condition_operators() {
    // Numeric operators
    let _eq = StorageOperator::Equal;
    let _ne = StorageOperator::NotEqual;
    let _gt = StorageOperator::GreaterThan;
    let _lt = StorageOperator::LessThan;
    let _gte = StorageOperator::GreaterThanOrEqual;
    let _lte = StorageOperator::LessThanOrEqual;

    // String operators
    let _contains = StorageOperator::Contains;
    let _not_contains = StorageOperator::NotContains;
    let _matches = StorageOperator::Matches;
    let _not_matches = StorageOperator::NotMatches;
}

#[test]
fn test_directory_depth_condition() -> Result<()> {
    let _temp_dir = new_test_tempdir()?;

    let condition = StorageCondition {
        condition_type: StorageConditionType::DirectoryDepth,
        value: "5".to_string(),
        operator: StorageOperator::GreaterThan,
    };

    let deny_action = StorageAction {
        action_type: StorageActionType::Deny,
        parameters: HashMap::new(),
    };

    let rule = StorageRule {
        name: "depth_rule".to_string(),
        description: "Deny deeply nested files".to_string(),
        rule_type: StorageRuleType::Security,
        conditions: vec![condition],
        actions: vec![deny_action],
    };

    let mut policy = create_default_policy();
    policy.constraints.max_directory_depth = 100; // High limit to not trigger constraint
    policy.rules = vec![rule];

    let engine = StoragePolicyEngine::new(policy);

    // Deep path (> 5 components)
    let deep_path = PathBuf::from("/a/b/c/d/e/f/file.txt");
    let result = engine.validate_file_operation(&deep_path, &FileOperation::Write);
    assert!(result.is_err(), "Should deny deeply nested paths");

    // Shallow path (<= 5 components)
    let shallow_path = PathBuf::from("/a/b/file.txt");
    let result = engine.validate_file_operation(&shallow_path, &FileOperation::Write);
    assert!(result.is_ok(), "Should allow shallow paths");

    Ok(())
}

#[test]
fn test_file_without_extension() -> Result<()> {
    let _temp_dir = new_test_tempdir()?;
    let mut constraints = StorageConstraints::default();
    constraints.allowed_extensions = vec!["txt".to_string()];

    let policy = StoragePolicy {
        name: "extension_policy".to_string(),
        description: "Extension policy".to_string(),
        config: StorageConfig::default(),
        constraints,
        rules: vec![],
    };

    let engine = StoragePolicyEngine::new(policy);

    let no_ext_file = PathBuf::from("README");
    let result = engine.validate_file_operation(&no_ext_file, &FileOperation::Read);
    assert!(
        result.is_err(),
        "Should reject files without allowed extension"
    );

    Ok(())
}

#[test]
fn test_empty_allowed_extensions() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let mut constraints = StorageConstraints::default();
    constraints.allowed_extensions = vec![]; // Empty means all allowed
    constraints.blocked_extensions = vec![];

    let policy = StoragePolicy {
        name: "permissive_policy".to_string(),
        description: "Permissive extension policy".to_string(),
        config: StorageConfig::default(),
        constraints,
        rules: vec![],
    };

    let engine = StoragePolicyEngine::new(policy);

    let test_file = temp_dir.path().join("test.xyz");
    fs::write(&test_file, "content")?;

    let result = engine.validate_file_operation(&test_file, &FileOperation::Read);
    assert!(result.is_ok(), "Empty allowed_extensions should allow all");

    Ok(())
}

#[test]
fn test_action_types() {
    let _allow = StorageActionType::Allow;
    let _deny = StorageActionType::Deny;
    let _delete = StorageActionType::Delete;
    let _move = StorageActionType::Move;
    let _copy = StorageActionType::Copy;
    let _alert = StorageActionType::Alert;
    let _log = StorageActionType::Log;
}

#[test]
fn test_nonexistent_file_validation() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let policy = create_default_policy();
    let engine = StoragePolicyEngine::new(policy);

    let nonexistent = temp_dir.path().join("nonexistent.txt");
    // Should still validate based on path/extension rules
    let result = engine.validate_file_operation(&nonexistent, &FileOperation::Create);
    assert!(
        result.is_ok(),
        "Should validate based on path for nonexistent files"
    );

    Ok(())
}
