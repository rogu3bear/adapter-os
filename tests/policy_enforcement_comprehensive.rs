//! Comprehensive integration tests for policy enforcement

use adapteros_core::Result;
use adapteros_manifest::Policies;
use adapteros_policy::{
    unified_enforcement::{
        Operation as PolicyOperation, OperationType as PolicyOperationType,
        PolicyContext as UnifiedPolicyContext, PolicyEnforcer, Priority as UnifiedPriority,
    },
    EnforcementLevel, PolicyPackManager,
};
use futures_util::future;
use std::collections::HashMap;

#[tokio::test]
async fn test_enforcement_levels_info_warning() -> Result<()> {
    let mut manager = PolicyPackManager::new();

    // Set Evidence pack to Info level (should not block)
    if let Some(config) = manager.get_pack_config(&adapteros_policy::PolicyPackId::Evidence) {
        let mut new_config = config.clone();
        new_config.enforcement_level = EnforcementLevel::Info;
        manager.update_pack_config(adapteros_policy::PolicyPackId::Evidence, new_config)?;
    }

    let mut parameters = HashMap::new();
    parameters.insert(
        "evidence_spans".to_string(),
        serde_json::json!(vec![] as Vec<()>),
    );

    let context = UnifiedPolicyContext {
        component: "test".to_string(),
        operation: "inference".to_string(),
        data: Some(serde_json::json!({
            "evidence_spans": [],
        })),
        priority: UnifiedPriority::Normal,
    };

    let operation = PolicyOperation {
        operation_id: "test-info-level".to_string(),
        operation_type: PolicyOperationType::PerformInference,
        parameters,
        context,
        metadata: None,
    };

    // Should be allowed even with violations (Info level doesn't block)
    let result = manager.enforce_policy(&operation).await?;

    // Operation should be allowed (Info level violations don't block)
    assert!(
        result.allowed,
        "Info level violations should not block operations"
    );

    Ok(())
}

#[tokio::test]
async fn test_enforcement_levels_error_critical() -> Result<()> {
    let mut manager = PolicyPackManager::new();

    // Set Egress pack to Critical level
    if let Some(config) = manager.get_pack_config(&adapteros_policy::PolicyPackId::Egress) {
        let mut new_config = config.clone();
        new_config.enforcement_level = EnforcementLevel::Critical;
        manager.update_pack_config(adapteros_policy::PolicyPackId::Egress, new_config)?;
    }

    let mut parameters = HashMap::new();
    parameters.insert("protocol".to_string(), serde_json::json!("tcp"));
    parameters.insert("port".to_string(), serde_json::json!(80));

    let context = UnifiedPolicyContext {
        component: "test".to_string(),
        operation: "network_operation".to_string(),
        data: Some(serde_json::json!({
            "protocol": "tcp",
            "port": 80,
        })),
        priority: UnifiedPriority::Normal,
    };

    let operation = PolicyOperation {
        operation_id: "test-critical-level".to_string(),
        operation_type: PolicyOperationType::SystemOperation,
        parameters,
        context,
        metadata: None,
    };

    // Should be blocked (Critical level violations block)
    let result = manager.enforce_policy(&operation).await?;

    // Operation should be denied
    assert!(
        !result.allowed,
        "Critical level violations should block operations"
    );
    assert!(!result.violations.is_empty(), "Should have violations");

    Ok(())
}

#[tokio::test]
async fn test_critical_blocker_short_circuit() -> Result<()> {
    let manager = PolicyPackManager::new();

    let mut parameters = HashMap::new();
    // Create a request that would trigger multiple policy violations
    parameters.insert("protocol".to_string(), serde_json::json!("tcp"));
    parameters.insert("confidence".to_string(), serde_json::json!(0.2));

    let context = UnifiedPolicyContext {
        component: "test".to_string(),
        operation: "inference".to_string(),
        data: Some(serde_json::json!({
            "protocol": "tcp",
            "confidence": 0.2,
        })),
        priority: UnifiedPriority::Normal,
    };

    let operation = PolicyOperation {
        operation_id: "test-short-circuit".to_string(),
        operation_type: PolicyOperationType::PerformInference,
        parameters,
        context,
        metadata: None,
    };

    // Should short-circuit on critical blocker violation
    let result = manager.enforce_policy(&operation).await?;

    // Should be blocked
    assert!(
        !result.allowed,
        "Critical blocker violations should block operations"
    );

    // Should have Deny action
    let has_deny = result.actions.iter().any(|action| {
        matches!(
            action,
            adapteros_policy::unified_enforcement::EnforcementAction::Deny
        )
    });
    assert!(has_deny, "Should have Deny action for blocked operation");

    Ok(())
}

#[tokio::test]
async fn test_violation_logging_actions() -> Result<()> {
    let mut manager = PolicyPackManager::new();

    // Set a pack to Critical level to trigger alerts
    if let Some(config) = manager.get_pack_config(&adapteros_policy::PolicyPackId::Egress) {
        let mut new_config = config.clone();
        new_config.enforcement_level = EnforcementLevel::Critical;
        manager.update_pack_config(adapteros_policy::PolicyPackId::Egress, new_config)?;
    }

    let mut parameters = HashMap::new();
    parameters.insert("protocol".to_string(), serde_json::json!("tcp"));

    let context = UnifiedPolicyContext {
        component: "test".to_string(),
        operation: "network_operation".to_string(),
        data: Some(serde_json::json!({
            "protocol": "tcp",
        })),
        priority: UnifiedPriority::Normal,
    };

    let operation = PolicyOperation {
        operation_id: "test-alerts".to_string(),
        operation_type: PolicyOperationType::SystemOperation,
        parameters,
        context,
        metadata: None,
    };

    let result = manager.enforce_policy(&operation).await?;

    // Should have LogViolation actions
    let has_log_violation = result.actions.iter().any(|action| {
        matches!(
            action,
            adapteros_policy::unified_enforcement::EnforcementAction::LogViolation { .. }
        )
    });
    assert!(has_log_violation, "Should have LogViolation actions");

    // Should have SendAlert for Critical level violations
    let has_alert = result.actions.iter().any(|action| {
        matches!(
            action,
            adapteros_policy::unified_enforcement::EnforcementAction::SendAlert { .. }
        )
    });
    assert!(has_alert, "Critical level violations should trigger alerts");

    Ok(())
}

#[tokio::test]
async fn test_manifest_configuration_integration() -> Result<()> {
    use adapteros_policy::PolicyEngine;

    // Create policies with custom evidence requirements
    let mut policies = Policies::default();
    policies.evidence.require_open_book = true;
    policies.evidence.min_spans = 3; // Require 3 spans instead of default 1

    // Create PolicyEngine from manifest
    let engine = PolicyEngine::new(policies.clone());

    // Verify pack manager was configured
    let pack_config = engine
        .pack_manager()
        .get_config(&adapteros_policy::PolicyPackId::Evidence);

    assert!(pack_config.is_some(), "Evidence pack should be configured");

    if let Some(config) = pack_config {
        let min_spans = config.config.get("min_spans").and_then(|v| v.as_u64());
        assert_eq!(
            min_spans,
            Some(3),
            "Evidence pack should have min_spans=3 from manifest"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_concurrent_policy_validation() -> Result<()> {
    use std::sync::Arc;
    use tokio::task;

    let manager = Arc::new(PolicyPackManager::new());

    // Create multiple concurrent validation requests
    let mut handles = Vec::new();

    for i in 0..10 {
        let manager = manager.clone();
        let handle = task::spawn(async move {
            let mut parameters = HashMap::new();
            parameters.insert("prompt_length".to_string(), serde_json::json!(i * 10));

            let context = UnifiedPolicyContext {
                component: "test".to_string(),
                operation: "inference".to_string(),
                data: Some(serde_json::json!({
                    "prompt_length": i * 10,
                })),
                priority: UnifiedPriority::Normal,
            };

            let operation = PolicyOperation {
                operation_id: format!("test-concurrent-{}", i),
                operation_type: PolicyOperationType::PerformInference,
                parameters,
                context,
                metadata: None,
            };

            manager.enforce_policy(&operation).await
        });

        handles.push(handle);
    }

    // Wait for all validations to complete
    let results: Vec<_> = future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("Task should not panic"))
        .collect();

    // All should succeed (validation should be thread-safe)
    for result in results {
        assert!(result.is_ok(), "Concurrent validation should not fail");
    }

    Ok(())
}

#[tokio::test]
async fn test_error_message_quality() -> Result<()> {
    let manager = PolicyPackManager::new();

    let mut parameters = HashMap::new();
    parameters.insert("protocol".to_string(), serde_json::json!("tcp"));

    let context = UnifiedPolicyContext {
        component: "test".to_string(),
        operation: "network_operation".to_string(),
        data: Some(serde_json::json!({
            "protocol": "tcp",
        })),
        priority: UnifiedPriority::Normal,
    };

    let operation = PolicyOperation {
        operation_id: "test-error-messages".to_string(),
        operation_type: PolicyOperationType::SystemOperation,
        parameters,
        context,
        metadata: None,
    };

    let result = manager.enforce_policy(&operation).await?;

    // Check that violations have actionable remediation steps
    for violation in &result.violations {
        assert!(
            !violation.message.is_empty(),
            "Violation message should not be empty"
        );

        // Check for remediation steps
        if let Some(remediation) = &violation.remediation {
            assert!(
                !remediation.is_empty(),
                "Violations should have remediation steps"
            );
        }

        // Check for details
        if let Some(details) = &violation.details {
            assert!(
                details.is_object() || details.is_string(),
                "Violation details should be structured"
            );
        }
    }

    Ok(())
}
