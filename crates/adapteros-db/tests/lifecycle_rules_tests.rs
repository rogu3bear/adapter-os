//! Integration tests for lifecycle rules database operations
//!
//! Tests the CRUD operations and rule evaluation for lifecycle rules
//! that govern adapter and dataset lifecycle management.

use adapteros_db::lifecycle_rules::{
    ActionType, ConditionOperator, CreateLifecycleRuleParams, LifecycleRule, LifecycleRuleAction,
    LifecycleRuleCondition, LifecycleRuleFilter, LifecycleRuleScope, LifecycleRuleType,
    UpdateLifecycleRuleParams,
};
use adapteros_db::Db;

#[tokio::test]
async fn test_create_lifecycle_rule() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let params = CreateLifecycleRuleParams {
        name: "Test TTL Rule".to_string(),
        description: Some("Evict ephemeral adapters after 24 hours".to_string()),
        scope: LifecycleRuleScope::Category,
        scope_target: Some("ephemeral".to_string()),
        rule_type: LifecycleRuleType::Ttl,
        conditions: vec![LifecycleRuleCondition {
            field: "hours_since_last_use".to_string(),
            operator: ConditionOperator::GreaterThanOrEqual,
            value: serde_json::json!(24),
        }],
        actions: vec![LifecycleRuleAction {
            action_type: ActionType::Evict,
            parameters: serde_json::json!({"reason": "ttl_expired"}),
        }],
        priority: Some(50),
        created_by: Some("test-user".to_string()),
        metadata: Some(serde_json::json!({"source": "test"})),
    };

    let rule_id = db
        .create_lifecycle_rule(params)
        .await
        .expect("Failed to create lifecycle rule");

    assert!(!rule_id.is_empty());

    // Retrieve and verify
    let rule = db
        .get_lifecycle_rule(&rule_id)
        .await
        .expect("Failed to get lifecycle rule")
        .expect("Rule should exist");

    assert_eq!(rule.name, "Test TTL Rule");
    assert_eq!(rule.scope, LifecycleRuleScope::Category);
    assert_eq!(rule.scope_target.as_deref(), Some("ephemeral"));
    assert_eq!(rule.rule_type, LifecycleRuleType::Ttl);
    assert_eq!(rule.priority, 50);
    assert!(rule.enabled);
    assert_eq!(rule.created_by.as_deref(), Some("test-user"));
    assert_eq!(rule.conditions.len(), 1);
    assert_eq!(rule.actions.len(), 1);
}

#[tokio::test]
async fn test_create_system_rule_no_target_required() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let params = CreateLifecycleRuleParams {
        name: "System Retention Rule".to_string(),
        description: None,
        scope: LifecycleRuleScope::System,
        scope_target: None, // Not required for system scope
        rule_type: LifecycleRuleType::Retention,
        conditions: vec![],
        actions: vec![],
        priority: None,
        created_by: None,
        metadata: None,
    };

    let result = db.create_lifecycle_rule(params).await;
    assert!(result.is_ok(), "System scope should not require target");
}

#[tokio::test]
async fn test_create_tenant_rule_requires_target() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let params = CreateLifecycleRuleParams {
        name: "Tenant Rule Without Target".to_string(),
        description: None,
        scope: LifecycleRuleScope::Tenant,
        scope_target: None, // Should fail - tenant scope requires target
        rule_type: LifecycleRuleType::Retention,
        conditions: vec![],
        actions: vec![],
        priority: None,
        created_by: None,
        metadata: None,
    };

    let result = db.create_lifecycle_rule(params).await;
    assert!(result.is_err(), "Tenant scope should require target");
}

#[tokio::test]
async fn test_list_lifecycle_rules_with_filter() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    // Create multiple rules
    let ttl_rule = CreateLifecycleRuleParams {
        name: "TTL Rule 1".to_string(),
        description: None,
        scope: LifecycleRuleScope::Category,
        scope_target: Some("ephemeral".to_string()),
        rule_type: LifecycleRuleType::Ttl,
        conditions: vec![],
        actions: vec![],
        priority: Some(10),
        created_by: None,
        metadata: None,
    };
    db.create_lifecycle_rule(ttl_rule).await.unwrap();

    let retention_rule = CreateLifecycleRuleParams {
        name: "Retention Rule".to_string(),
        description: None,
        scope: LifecycleRuleScope::System,
        scope_target: None,
        rule_type: LifecycleRuleType::Retention,
        conditions: vec![],
        actions: vec![],
        priority: Some(5),
        created_by: None,
        metadata: None,
    };
    db.create_lifecycle_rule(retention_rule).await.unwrap();

    // Filter by type
    let filter = LifecycleRuleFilter {
        rule_type: Some(LifecycleRuleType::Ttl),
        ..Default::default()
    };
    let rules = db.list_lifecycle_rules(filter).await.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "TTL Rule 1");

    // Filter by scope
    let filter = LifecycleRuleFilter {
        scope: Some(LifecycleRuleScope::System),
        ..Default::default()
    };
    let rules = db.list_lifecycle_rules(filter).await.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "Retention Rule");

    // Get all rules (sorted by priority desc)
    let all_rules = db
        .list_lifecycle_rules(LifecycleRuleFilter::default())
        .await
        .unwrap();
    assert_eq!(all_rules.len(), 2);
    assert_eq!(all_rules[0].priority, 10); // Higher priority first
    assert_eq!(all_rules[1].priority, 5);
}

#[tokio::test]
async fn test_update_lifecycle_rule() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    // Create a rule
    let params = CreateLifecycleRuleParams {
        name: "Original Name".to_string(),
        description: Some("Original description".to_string()),
        scope: LifecycleRuleScope::System,
        scope_target: None,
        rule_type: LifecycleRuleType::Retention,
        conditions: vec![],
        actions: vec![],
        priority: Some(10),
        created_by: None,
        metadata: None,
    };
    let rule_id = db.create_lifecycle_rule(params).await.unwrap();

    // Update the rule
    let update_params = UpdateLifecycleRuleParams {
        name: Some("Updated Name".to_string()),
        description: Some("Updated description".to_string()),
        priority: Some(20),
        ..Default::default()
    };
    db.update_lifecycle_rule(&rule_id, update_params)
        .await
        .unwrap();

    // Verify update
    let rule = db.get_lifecycle_rule(&rule_id).await.unwrap().unwrap();
    assert_eq!(rule.name, "Updated Name");
    assert_eq!(rule.description.as_deref(), Some("Updated description"));
    assert_eq!(rule.priority, 20);
}

#[tokio::test]
async fn test_delete_lifecycle_rule() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let params = CreateLifecycleRuleParams {
        name: "To Be Deleted".to_string(),
        description: None,
        scope: LifecycleRuleScope::System,
        scope_target: None,
        rule_type: LifecycleRuleType::Retention,
        conditions: vec![],
        actions: vec![],
        priority: None,
        created_by: None,
        metadata: None,
    };
    let rule_id = db.create_lifecycle_rule(params).await.unwrap();

    // Verify exists
    let rule = db.get_lifecycle_rule(&rule_id).await.unwrap();
    assert!(rule.is_some());

    // Delete
    db.delete_lifecycle_rule(&rule_id).await.unwrap();

    // Verify gone
    let rule = db.get_lifecycle_rule(&rule_id).await.unwrap();
    assert!(rule.is_none());
}

#[tokio::test]
async fn test_set_lifecycle_rule_enabled() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let params = CreateLifecycleRuleParams {
        name: "Toggle Test".to_string(),
        description: None,
        scope: LifecycleRuleScope::System,
        scope_target: None,
        rule_type: LifecycleRuleType::Retention,
        conditions: vec![],
        actions: vec![],
        priority: None,
        created_by: None,
        metadata: None,
    };
    let rule_id = db.create_lifecycle_rule(params).await.unwrap();

    // Initially enabled
    let rule = db.get_lifecycle_rule(&rule_id).await.unwrap().unwrap();
    assert!(rule.enabled);

    // Disable
    db.set_lifecycle_rule_enabled(&rule_id, false)
        .await
        .unwrap();
    let rule = db.get_lifecycle_rule(&rule_id).await.unwrap().unwrap();
    assert!(!rule.enabled);

    // Re-enable
    db.set_lifecycle_rule_enabled(&rule_id, true).await.unwrap();
    let rule = db.get_lifecycle_rule(&rule_id).await.unwrap().unwrap();
    assert!(rule.enabled);
}

#[tokio::test]
async fn test_evaluate_condition_equals() {
    let condition = LifecycleRuleCondition {
        field: "lifecycle_state".to_string(),
        operator: ConditionOperator::Equals,
        value: serde_json::json!("retired"),
    };

    let field_values = serde_json::json!({
        "lifecycle_state": "retired"
    });

    let result = Db::evaluate_condition(&condition, &field_values);
    assert!(result.met);
    assert_eq!(result.field, "lifecycle_state");

    // Non-matching case
    let field_values = serde_json::json!({
        "lifecycle_state": "active"
    });
    let result = Db::evaluate_condition(&condition, &field_values);
    assert!(!result.met);
}

#[tokio::test]
async fn test_evaluate_condition_numeric_comparison() {
    let condition = LifecycleRuleCondition {
        field: "hours_since_last_use".to_string(),
        operator: ConditionOperator::GreaterThanOrEqual,
        value: serde_json::json!(24),
    };

    // Meets threshold
    let field_values = serde_json::json!({
        "hours_since_last_use": 48
    });
    let result = Db::evaluate_condition(&condition, &field_values);
    assert!(result.met);

    // Exactly at threshold
    let field_values = serde_json::json!({
        "hours_since_last_use": 24
    });
    let result = Db::evaluate_condition(&condition, &field_values);
    assert!(result.met);

    // Below threshold
    let field_values = serde_json::json!({
        "hours_since_last_use": 12
    });
    let result = Db::evaluate_condition(&condition, &field_values);
    assert!(!result.met);
}

#[tokio::test]
async fn test_evaluate_condition_in_operator() {
    let condition = LifecycleRuleCondition {
        field: "category".to_string(),
        operator: ConditionOperator::In,
        value: serde_json::json!(["code", "framework", "codebase"]),
    };

    let field_values = serde_json::json!({
        "category": "code"
    });
    let result = Db::evaluate_condition(&condition, &field_values);
    assert!(result.met);

    let field_values = serde_json::json!({
        "category": "ephemeral"
    });
    let result = Db::evaluate_condition(&condition, &field_values);
    assert!(!result.met);
}

#[tokio::test]
async fn test_evaluate_rule_all_conditions() {
    let rule = LifecycleRule {
        id: "test-rule".to_string(),
        name: "Test Rule".to_string(),
        description: None,
        scope: LifecycleRuleScope::System,
        scope_target: None,
        rule_type: LifecycleRuleType::Demotion,
        conditions: vec![
            LifecycleRuleCondition {
                field: "allocation_tier".to_string(),
                operator: ConditionOperator::Equals,
                value: serde_json::json!("hot"),
            },
            LifecycleRuleCondition {
                field: "minutes_since_last_use".to_string(),
                operator: ConditionOperator::GreaterThanOrEqual,
                value: serde_json::json!(60),
            },
        ],
        actions: vec![LifecycleRuleAction {
            action_type: ActionType::TransitionState,
            parameters: serde_json::json!({"target_tier": "warm"}),
        }],
        priority: 0,
        enabled: true,
        created_by: None,
        created_at: "2024-01-01".to_string(),
        updated_at: "2024-01-01".to_string(),
        metadata_json: None,
    };

    // Both conditions met
    let field_values = serde_json::json!({
        "allocation_tier": "hot",
        "minutes_since_last_use": 120
    });
    let eval = Db::evaluate_rule(&rule, &field_values);
    assert!(eval.conditions_met);
    assert_eq!(eval.actions.len(), 1);

    // Only one condition met
    let field_values = serde_json::json!({
        "allocation_tier": "hot",
        "minutes_since_last_use": 30
    });
    let eval = Db::evaluate_rule(&rule, &field_values);
    assert!(!eval.conditions_met);
    assert_eq!(eval.actions.len(), 0);
}

#[tokio::test]
async fn test_find_matching_rule() {
    let rules = vec![
        LifecycleRule {
            id: "rule-1".to_string(),
            name: "High Priority Rule".to_string(),
            description: None,
            scope: LifecycleRuleScope::Category,
            scope_target: Some("ephemeral".to_string()),
            rule_type: LifecycleRuleType::Ttl,
            conditions: vec![LifecycleRuleCondition {
                field: "hours_since_last_use".to_string(),
                operator: ConditionOperator::GreaterThanOrEqual,
                value: serde_json::json!(48),
            }],
            actions: vec![LifecycleRuleAction {
                action_type: ActionType::Delete,
                parameters: serde_json::json!({}),
            }],
            priority: 30,
            enabled: true,
            created_by: None,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
            metadata_json: None,
        },
        LifecycleRule {
            id: "rule-2".to_string(),
            name: "Lower Priority Rule".to_string(),
            description: None,
            scope: LifecycleRuleScope::System,
            scope_target: None,
            rule_type: LifecycleRuleType::Ttl,
            conditions: vec![LifecycleRuleCondition {
                field: "hours_since_last_use".to_string(),
                operator: ConditionOperator::GreaterThanOrEqual,
                value: serde_json::json!(24),
            }],
            actions: vec![LifecycleRuleAction {
                action_type: ActionType::Evict,
                parameters: serde_json::json!({}),
            }],
            priority: 0,
            enabled: true,
            created_by: None,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
            metadata_json: None,
        },
    ];

    // 30 hours - only matches second rule
    let field_values = serde_json::json!({
        "hours_since_last_use": 30
    });
    let matching = Db::find_matching_rule(&rules, &field_values);
    assert!(matching.is_some());
    assert_eq!(matching.unwrap().rule_name, "Lower Priority Rule");

    // 50 hours - matches first rule (higher priority, evaluated first)
    let field_values = serde_json::json!({
        "hours_since_last_use": 50
    });
    let matching = Db::find_matching_rule(&rules, &field_values);
    assert!(matching.is_some());
    assert_eq!(matching.unwrap().rule_name, "High Priority Rule");

    // 12 hours - no match
    let field_values = serde_json::json!({
        "hours_since_last_use": 12
    });
    let matching = Db::find_matching_rule(&rules, &field_values);
    assert!(matching.is_none());
}
