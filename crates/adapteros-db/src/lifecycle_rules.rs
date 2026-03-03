//! Lifecycle rules for adapter and dataset lifecycle management.
//!
//! This module provides rule definitions and enforcement for managing
//! the lifecycle states of adapters and datasets.
//!
//! # Overview
//!
//! Lifecycle rules define conditions under which adapters or datasets should
//! transition between states or have actions taken on them. Rules are evaluated
//! in priority order, with higher priority rules taking precedence.
//!
//! # Rule Scopes
//!
//! - **System**: Applies to all resources across all tenants
//! - **Tenant**: Applies to all resources within a specific tenant
//! - **Category**: Applies to resources of a specific category
//! - **Adapter**: Applies to a specific adapter
//!
//! # State Transition Enforcement
//!
//! This module enforces that lifecycle state transitions follow the allowed paths:
//!
//! ```text
//! Draft -> Training -> Ready -> Active -> Deprecated -> Retired
//!            \          \        \ rollback /
//!             \          \---------> Failed
//!              \------------------>
//! ```
//!
//! Special rules:
//! - Ephemeral adapters: Active -> Retired (skip Deprecated)
//! - Active -> Ready: Allowed for rollback scenarios
//! - Any -> Failed: Always allowed (failure can happen at any stage)
//! - Retired/Failed are terminal states

use crate::new_id;
use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{debug, info, warn};

/// Action type for lifecycle rule actions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    /// Evict from hot storage
    Evict,
    /// Delete the resource
    Delete,
    /// Transition to a different state
    TransitionState,
    /// Archive the resource
    Archive,
    /// Notify administrators
    Notify,
}

impl ActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionType::Evict => "evict",
            ActionType::Delete => "delete",
            ActionType::TransitionState => "transition_state",
            ActionType::Archive => "archive",
            ActionType::Notify => "notify",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "evict" => Some(ActionType::Evict),
            "delete" => Some(ActionType::Delete),
            "transition_state" => Some(ActionType::TransitionState),
            "archive" => Some(ActionType::Archive),
            "notify" => Some(ActionType::Notify),
            _ => None,
        }
    }
}

/// Condition operator for rule conditions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOperator {
    /// Equals comparison
    Equals,
    /// Not equals comparison
    NotEquals,
    /// Greater than comparison
    GreaterThan,
    /// Greater than or equal comparison
    GreaterThanOrEqual,
    /// Less than comparison
    LessThan,
    /// Less than or equal comparison
    LessThanOrEqual,
    /// Value is in a list
    In,
    /// Value is not in a list
    NotIn,
    /// String contains
    Contains,
    /// String does not contain
    NotContains,
}

impl ConditionOperator {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConditionOperator::Equals => "equals",
            ConditionOperator::NotEquals => "not_equals",
            ConditionOperator::GreaterThan => "greater_than",
            ConditionOperator::GreaterThanOrEqual => "greater_than_or_equal",
            ConditionOperator::LessThan => "less_than",
            ConditionOperator::LessThanOrEqual => "less_than_or_equal",
            ConditionOperator::In => "in",
            ConditionOperator::NotIn => "not_in",
            ConditionOperator::Contains => "contains",
            ConditionOperator::NotContains => "not_contains",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "equals" => Some(ConditionOperator::Equals),
            "not_equals" => Some(ConditionOperator::NotEquals),
            "greater_than" => Some(ConditionOperator::GreaterThan),
            "greater_than_or_equal" => Some(ConditionOperator::GreaterThanOrEqual),
            "less_than" => Some(ConditionOperator::LessThan),
            "less_than_or_equal" => Some(ConditionOperator::LessThanOrEqual),
            "in" => Some(ConditionOperator::In),
            "not_in" => Some(ConditionOperator::NotIn),
            "contains" => Some(ConditionOperator::Contains),
            "not_contains" => Some(ConditionOperator::NotContains),
            _ => None,
        }
    }
}

/// Scope of a lifecycle rule
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRuleScope {
    /// System-wide rule
    System,
    /// Tenant-specific rule
    Tenant,
    /// Category-specific rule
    Category,
    /// Adapter-specific rule
    Adapter,
}

impl LifecycleRuleScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleRuleScope::System => "system",
            LifecycleRuleScope::Tenant => "tenant",
            LifecycleRuleScope::Category => "category",
            LifecycleRuleScope::Adapter => "adapter",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "system" => Some(LifecycleRuleScope::System),
            "tenant" => Some(LifecycleRuleScope::Tenant),
            "category" => Some(LifecycleRuleScope::Category),
            "adapter" => Some(LifecycleRuleScope::Adapter),
            _ => None,
        }
    }

    /// Returns true if this scope requires a target value
    pub fn requires_target(&self) -> bool {
        !matches!(self, LifecycleRuleScope::System)
    }
}

/// Type of lifecycle rule
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRuleType {
    /// Time-to-live based rule
    Ttl,
    /// Retention policy rule
    Retention,
    /// Demotion rule (e.g., hot -> warm)
    Demotion,
    /// Promotion rule (e.g., warm -> hot)
    Promotion,
    /// Archival rule
    Archival,
    /// Cleanup rule
    Cleanup,
    /// State transition enforcement rule
    StateTransition,
}

impl LifecycleRuleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleRuleType::Ttl => "ttl",
            LifecycleRuleType::Retention => "retention",
            LifecycleRuleType::Demotion => "demotion",
            LifecycleRuleType::Promotion => "promotion",
            LifecycleRuleType::Archival => "archival",
            LifecycleRuleType::Cleanup => "cleanup",
            LifecycleRuleType::StateTransition => "state_transition",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ttl" => Some(LifecycleRuleType::Ttl),
            "retention" => Some(LifecycleRuleType::Retention),
            "demotion" => Some(LifecycleRuleType::Demotion),
            "promotion" => Some(LifecycleRuleType::Promotion),
            "archival" => Some(LifecycleRuleType::Archival),
            "cleanup" => Some(LifecycleRuleType::Cleanup),
            "state_transition" => Some(LifecycleRuleType::StateTransition),
            _ => None,
        }
    }
}

/// A condition for a lifecycle rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRuleCondition {
    /// Field to evaluate
    pub field: String,
    /// Operator for comparison
    pub operator: ConditionOperator,
    /// Value to compare against
    pub value: serde_json::Value,
}

/// An action to take when a lifecycle rule matches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRuleAction {
    /// Type of action
    pub action_type: ActionType,
    /// Action parameters
    pub parameters: serde_json::Value,
}

/// A lifecycle rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRule {
    /// Unique identifier
    pub id: String,
    /// Rule name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Scope of the rule
    pub scope: LifecycleRuleScope,
    /// Target for scope (e.g., tenant_id, category name)
    pub scope_target: Option<String>,
    /// Type of rule
    pub rule_type: LifecycleRuleType,
    /// Conditions that must be met
    pub conditions: Vec<LifecycleRuleCondition>,
    /// Actions to take when conditions are met
    pub actions: Vec<LifecycleRuleAction>,
    /// Priority (higher = more important)
    pub priority: i32,
    /// Whether the rule is enabled
    pub enabled: bool,
    /// User who created the rule
    pub created_by: Option<String>,
    /// Creation timestamp
    pub created_at: String,
    /// Last update timestamp
    pub updated_at: String,
    /// Optional metadata JSON
    pub metadata_json: Option<String>,
}

/// Parameters for creating a new lifecycle rule
#[derive(Debug, Clone)]
pub struct CreateLifecycleRuleParams {
    /// Rule name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Scope of the rule
    pub scope: LifecycleRuleScope,
    /// Target for scope
    pub scope_target: Option<String>,
    /// Type of rule
    pub rule_type: LifecycleRuleType,
    /// Conditions that must be met
    pub conditions: Vec<LifecycleRuleCondition>,
    /// Actions to take
    pub actions: Vec<LifecycleRuleAction>,
    /// Priority
    pub priority: Option<i32>,
    /// User creating the rule
    pub created_by: Option<String>,
    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Parameters for updating a lifecycle rule
#[derive(Debug, Clone, Default)]
pub struct UpdateLifecycleRuleParams {
    /// New name
    pub name: Option<String>,
    /// New description
    pub description: Option<String>,
    /// New priority
    pub priority: Option<i32>,
    /// New conditions
    pub conditions: Option<Vec<LifecycleRuleCondition>>,
    /// New actions
    pub actions: Option<Vec<LifecycleRuleAction>>,
    /// New metadata
    pub metadata: Option<serde_json::Value>,
}

/// Filter for listing lifecycle rules
#[derive(Debug, Clone, Default)]
pub struct LifecycleRuleFilter {
    /// Filter by scope
    pub scope: Option<LifecycleRuleScope>,
    /// Filter by rule type
    pub rule_type: Option<LifecycleRuleType>,
    /// Filter by enabled status
    pub enabled: Option<bool>,
    /// Filter by scope target
    pub scope_target: Option<String>,
}

/// Result of evaluating a single condition
#[derive(Debug, Clone)]
pub struct ConditionEvaluationResult {
    /// The field that was evaluated
    pub field: String,
    /// Whether the condition was met
    pub met: bool,
    /// The actual value found
    pub actual_value: Option<serde_json::Value>,
    /// The expected value
    pub expected_value: serde_json::Value,
}

/// Result of evaluating a lifecycle rule
#[derive(Debug, Clone)]
pub struct LifecycleRuleEvaluation {
    /// Rule ID
    pub rule_id: String,
    /// Rule name
    pub rule_name: String,
    /// Whether all conditions were met
    pub conditions_met: bool,
    /// Individual condition results
    pub condition_results: Vec<ConditionEvaluationResult>,
    /// Actions to execute (empty if conditions not met)
    pub actions: Vec<LifecycleRuleAction>,
}

/// Result of validating a lifecycle state transition
#[derive(Debug, Clone)]
pub struct TransitionValidationResult {
    /// Whether the transition is allowed
    pub allowed: bool,
    /// Reason for denial (if not allowed)
    pub denial_reason: Option<String>,
    /// Rules that were evaluated
    pub evaluated_rules: Vec<String>,
    /// Any warnings about the transition
    pub warnings: Vec<String>,
}

impl TransitionValidationResult {
    /// Create a successful validation result
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            denial_reason: None,
            evaluated_rules: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Create a denied validation result
    pub fn denied(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            denial_reason: Some(reason.into()),
            evaluated_rules: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Add a warning to the result
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }

    /// Add evaluated rule IDs
    pub fn with_evaluated_rules(mut self, rules: Vec<String>) -> Self {
        self.evaluated_rules = rules;
        self
    }
}

// Database operations for lifecycle rules
impl Db {
    /// Create a new lifecycle rule
    ///
    /// # Arguments
    /// * `params` - The parameters for creating the rule
    ///
    /// # Returns
    /// The ID of the newly created rule
    ///
    /// # Errors
    /// Returns an error if:
    /// - The scope requires a target but none is provided
    /// - Database operations fail
    pub async fn create_lifecycle_rule(&self, params: CreateLifecycleRuleParams) -> Result<String> {
        // Validate scope/target combination
        if params.scope.requires_target() && params.scope_target.is_none() {
            return Err(AosError::Validation(format!(
                "Scope '{}' requires a scope_target",
                params.scope.as_str()
            )));
        }

        let rule_id = new_id(IdPrefix::Pol);
        let conditions_json = serde_json::to_string(&params.conditions)
            .map_err(|e| AosError::Validation(format!("Failed to serialize conditions: {}", e)))?;
        let actions_json = serde_json::to_string(&params.actions)
            .map_err(|e| AosError::Validation(format!("Failed to serialize actions: {}", e)))?;
        let metadata_json = params
            .metadata
            .map(|m| serde_json::to_string(&m))
            .transpose()
            .map_err(|e| AosError::Validation(format!("Failed to serialize metadata: {}", e)))?;

        sqlx::query(
            "INSERT INTO lifecycle_rules
             (id, name, description, scope, scope_target, rule_type, conditions_json, actions_json, priority, enabled, created_by, metadata_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, datetime('now'), datetime('now'))"
        )
        .bind(&rule_id)
        .bind(&params.name)
        .bind(&params.description)
        .bind(params.scope.as_str())
        .bind(&params.scope_target)
        .bind(params.rule_type.as_str())
        .bind(&conditions_json)
        .bind(&actions_json)
        .bind(params.priority.unwrap_or(0))
        .bind(&params.created_by)
        .bind(&metadata_json)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create lifecycle rule: {}", e)))?;

        info!(
            rule_id = %rule_id,
            name = %params.name,
            scope = %params.scope.as_str(),
            rule_type = %params.rule_type.as_str(),
            "Created lifecycle rule"
        );

        Ok(rule_id)
    }

    /// Get a lifecycle rule by ID
    ///
    /// # Arguments
    /// * `rule_id` - The rule ID to look up
    ///
    /// # Returns
    /// The rule if found, None otherwise
    pub async fn get_lifecycle_rule(&self, rule_id: &str) -> Result<Option<LifecycleRule>> {
        let row = sqlx::query(
            "SELECT id, name, description, scope, scope_target, rule_type, conditions_json, actions_json, priority, enabled, created_by, metadata_json, created_at, updated_at
             FROM lifecycle_rules
             WHERE id = ?"
        )
        .bind(rule_id)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get lifecycle rule: {}", e)))?;

        match row {
            Some(row) => Ok(Some(Self::row_to_lifecycle_rule(&row)?)),
            None => Ok(None),
        }
    }

    /// List lifecycle rules with optional filtering
    ///
    /// # Arguments
    /// * `filter` - Filter criteria for the query
    ///
    /// # Returns
    /// A vector of matching rules, sorted by priority (descending)
    pub async fn list_lifecycle_rules(
        &self,
        filter: LifecycleRuleFilter,
    ) -> Result<Vec<LifecycleRule>> {
        let mut query = String::from(
            "SELECT id, name, description, scope, scope_target, rule_type, conditions_json, actions_json, priority, enabled, created_by, metadata_json, created_at, updated_at
             FROM lifecycle_rules WHERE 1=1"
        );
        let mut binds: Vec<String> = Vec::new();

        if let Some(scope) = &filter.scope {
            query.push_str(" AND scope = ?");
            binds.push(scope.as_str().to_string());
        }
        if let Some(rule_type) = &filter.rule_type {
            query.push_str(" AND rule_type = ?");
            binds.push(rule_type.as_str().to_string());
        }
        if let Some(enabled) = filter.enabled {
            query.push_str(" AND enabled = ?");
            binds.push(if enabled {
                "1".to_string()
            } else {
                "0".to_string()
            });
        }
        if let Some(scope_target) = &filter.scope_target {
            query.push_str(" AND scope_target = ?");
            binds.push(scope_target.clone());
        }

        query.push_str(" ORDER BY priority DESC, created_at ASC");

        let mut q = sqlx::query(&query);
        for bind in &binds {
            q = q.bind(bind);
        }

        let rows = q
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list lifecycle rules: {}", e)))?;

        let mut rules = Vec::with_capacity(rows.len());
        for row in rows {
            rules.push(Self::row_to_lifecycle_rule(&row)?);
        }

        Ok(rules)
    }

    /// Update a lifecycle rule
    ///
    /// # Arguments
    /// * `rule_id` - The rule ID to update
    /// * `params` - The update parameters (only non-None fields are updated)
    ///
    /// # Errors
    /// Returns an error if the rule doesn't exist or database operations fail
    pub async fn update_lifecycle_rule(
        &self,
        rule_id: &str,
        params: UpdateLifecycleRuleParams,
    ) -> Result<()> {
        // Build dynamic UPDATE query
        let mut updates = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(name) = params.name {
            updates.push("name = ?");
            binds.push(name);
        }
        if let Some(description) = params.description {
            updates.push("description = ?");
            binds.push(description);
        }
        if let Some(priority) = params.priority {
            updates.push("priority = ?");
            binds.push(priority.to_string());
        }
        if let Some(conditions) = params.conditions {
            updates.push("conditions_json = ?");
            binds.push(serde_json::to_string(&conditions).map_err(|e| {
                AosError::Validation(format!("Failed to serialize conditions: {}", e))
            })?);
        }
        if let Some(actions) = params.actions {
            updates.push("actions_json = ?");
            binds.push(serde_json::to_string(&actions).map_err(|e| {
                AosError::Validation(format!("Failed to serialize actions: {}", e))
            })?);
        }
        if let Some(metadata) = params.metadata {
            updates.push("metadata_json = ?");
            binds.push(serde_json::to_string(&metadata).map_err(|e| {
                AosError::Validation(format!("Failed to serialize metadata: {}", e))
            })?);
        }

        if updates.is_empty() {
            return Ok(());
        }

        updates.push("updated_at = datetime('now')");
        let query = format!(
            "UPDATE lifecycle_rules SET {} WHERE id = ?",
            updates.join(", ")
        );

        let mut q = sqlx::query(&query);
        for bind in &binds {
            q = q.bind(bind);
        }
        q = q.bind(rule_id);

        let result = q
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update lifecycle rule: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AosError::NotFound(format!(
                "Lifecycle rule not found: {}",
                rule_id
            )));
        }

        debug!(rule_id = %rule_id, "Updated lifecycle rule");
        Ok(())
    }

    /// Delete a lifecycle rule
    ///
    /// # Arguments
    /// * `rule_id` - The rule ID to delete
    ///
    /// # Errors
    /// Returns an error if database operations fail
    pub async fn delete_lifecycle_rule(&self, rule_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM lifecycle_rules WHERE id = ?")
            .bind(rule_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete lifecycle rule: {}", e)))?;

        info!(rule_id = %rule_id, "Deleted lifecycle rule");
        Ok(())
    }

    /// Enable or disable a lifecycle rule
    ///
    /// # Arguments
    /// * `rule_id` - The rule ID to update
    /// * `enabled` - Whether the rule should be enabled
    pub async fn set_lifecycle_rule_enabled(&self, rule_id: &str, enabled: bool) -> Result<()> {
        let result = sqlx::query(
            "UPDATE lifecycle_rules SET enabled = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(if enabled { 1 } else { 0 })
        .bind(rule_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update lifecycle rule: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AosError::NotFound(format!(
                "Lifecycle rule not found: {}",
                rule_id
            )));
        }

        debug!(rule_id = %rule_id, enabled = %enabled, "Updated lifecycle rule enabled status");
        Ok(())
    }

    /// Evaluate a single condition against field values
    ///
    /// # Arguments
    /// * `condition` - The condition to evaluate
    /// * `field_values` - JSON object containing field values to check
    ///
    /// # Returns
    /// The evaluation result indicating if the condition was met
    pub fn evaluate_condition(
        condition: &LifecycleRuleCondition,
        field_values: &serde_json::Value,
    ) -> ConditionEvaluationResult {
        let actual_value = field_values.get(&condition.field).cloned();

        let met = match &actual_value {
            None => false,
            Some(actual) => Self::compare_values(actual, &condition.operator, &condition.value),
        };

        ConditionEvaluationResult {
            field: condition.field.clone(),
            met,
            actual_value,
            expected_value: condition.value.clone(),
        }
    }

    /// Evaluate a lifecycle rule against field values
    ///
    /// # Arguments
    /// * `rule` - The rule to evaluate
    /// * `field_values` - JSON object containing field values to check
    ///
    /// # Returns
    /// The evaluation result with condition details and applicable actions
    pub fn evaluate_rule(
        rule: &LifecycleRule,
        field_values: &serde_json::Value,
    ) -> LifecycleRuleEvaluation {
        let condition_results: Vec<ConditionEvaluationResult> = rule
            .conditions
            .iter()
            .map(|c| Self::evaluate_condition(c, field_values))
            .collect();

        let conditions_met = condition_results.iter().all(|r| r.met);

        let actions = if conditions_met {
            rule.actions.clone()
        } else {
            Vec::new()
        };

        LifecycleRuleEvaluation {
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            conditions_met,
            condition_results,
            actions,
        }
    }

    /// Find the first matching rule from a list of rules
    ///
    /// Rules are assumed to be sorted by priority (descending).
    /// Returns the first rule where all conditions are met.
    ///
    /// # Arguments
    /// * `rules` - List of rules to evaluate (should be sorted by priority)
    /// * `field_values` - JSON object containing field values to check
    ///
    /// # Returns
    /// The evaluation of the first matching rule, or None if no rules match
    pub fn find_matching_rule(
        rules: &[LifecycleRule],
        field_values: &serde_json::Value,
    ) -> Option<LifecycleRuleEvaluation> {
        for rule in rules {
            if !rule.enabled {
                continue;
            }
            let evaluation = Self::evaluate_rule(rule, field_values);
            if evaluation.conditions_met {
                return Some(evaluation);
            }
        }
        None
    }

    /// Validate a lifecycle state transition against configured rules
    ///
    /// This method checks if a transition from one state to another is allowed
    /// based on:
    /// 1. Core transition graph rules (built-in)
    /// 2. Tier-specific rules (e.g., ephemeral cannot be deprecated)
    /// 3. Custom lifecycle rules configured in the database
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter undergoing the transition
    /// * `from_state` - The current lifecycle state
    /// * `to_state` - The target lifecycle state
    /// * `tier` - The adapter tier (ephemeral, warm, persistent)
    ///
    /// # Returns
    /// A validation result indicating if the transition is allowed
    pub async fn validate_lifecycle_transition(
        &self,
        adapter_id: &str,
        from_state: &str,
        to_state: &str,
        tier: &str,
    ) -> Result<TransitionValidationResult> {
        use adapteros_core::lifecycle::LifecycleState;
        use std::str::FromStr;

        // Parse states
        let from = LifecycleState::from_str(from_state).map_err(|e| {
            AosError::Validation(format!("Invalid from_state '{}': {}", from_state, e))
        })?;

        let to = LifecycleState::from_str(to_state)
            .map_err(|e| AosError::Validation(format!("Invalid to_state '{}': {}", to_state, e)))?;

        // Check basic transition validity using tier-specific rules
        if !from.can_transition_to_for_tier(to, tier) {
            let reason = format!(
                "Transition from '{}' to '{}' is not allowed for tier '{}'",
                from_state, to_state, tier
            );
            warn!(
                adapter_id = %adapter_id,
                from_state = %from_state,
                to_state = %to_state,
                tier = %tier,
                "Lifecycle transition denied by core rules"
            );
            return Ok(TransitionValidationResult::denied(reason));
        }

        // Fetch applicable lifecycle rules
        let rules = self
            .list_lifecycle_rules(LifecycleRuleFilter {
                rule_type: Some(LifecycleRuleType::StateTransition),
                enabled: Some(true),
                ..Default::default()
            })
            .await?;

        // Build field values for rule evaluation
        let field_values = serde_json::json!({
            "adapter_id": adapter_id,
            "from_state": from_state,
            "to_state": to_state,
            "tier": tier,
        });

        // Evaluate rules
        let mut evaluated_rule_ids = Vec::new();
        let mut warnings = Vec::new();

        for rule in &rules {
            evaluated_rule_ids.push(rule.id.clone());
            let evaluation = Self::evaluate_rule(rule, &field_values);

            if evaluation.conditions_met {
                // Check if any action blocks the transition
                for action in &evaluation.actions {
                    if action.action_type == ActionType::Notify {
                        // Notify actions generate warnings but don't block
                        if let Some(msg) = action.parameters.get("message").and_then(|v| v.as_str())
                        {
                            warnings.push(msg.to_string());
                        }
                    }
                    // Other actions could potentially block here if needed
                }
            }
        }

        debug!(
            adapter_id = %adapter_id,
            from_state = %from_state,
            to_state = %to_state,
            evaluated_rules = ?evaluated_rule_ids,
            "Lifecycle transition validated"
        );

        let mut result = TransitionValidationResult::allowed();
        result.evaluated_rules = evaluated_rule_ids;
        result.warnings = warnings;

        Ok(result)
    }

    /// Get applicable lifecycle rules for an adapter
    ///
    /// Returns rules in priority order that apply to the given adapter
    /// based on its tenant, category, and adapter ID.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID
    /// * `tenant_id` - The tenant ID
    /// * `category` - The adapter category
    ///
    /// # Returns
    /// A vector of applicable rules sorted by priority (highest first)
    pub async fn get_applicable_lifecycle_rules(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        category: &str,
    ) -> Result<Vec<LifecycleRule>> {
        // Get all enabled rules
        let all_rules = self
            .list_lifecycle_rules(LifecycleRuleFilter {
                enabled: Some(true),
                ..Default::default()
            })
            .await?;

        // Filter to applicable rules
        let applicable: Vec<LifecycleRule> = all_rules
            .into_iter()
            .filter(|rule| match &rule.scope {
                LifecycleRuleScope::System => true,
                LifecycleRuleScope::Tenant => rule.scope_target.as_deref() == Some(tenant_id),
                LifecycleRuleScope::Category => rule.scope_target.as_deref() == Some(category),
                LifecycleRuleScope::Adapter => rule.scope_target.as_deref() == Some(adapter_id),
            })
            .collect();

        Ok(applicable)
    }

    // Helper: Convert a database row to LifecycleRule
    fn row_to_lifecycle_rule(row: &sqlx::sqlite::SqliteRow) -> Result<LifecycleRule> {
        let id: String = row.get(0);
        let name: String = row.get(1);
        let description: Option<String> = row.get(2);
        let scope_str: String = row.get(3);
        let scope_target: Option<String> = row.get(4);
        let rule_type_str: String = row.get(5);
        let conditions_json: String = row.get(6);
        let actions_json: String = row.get(7);
        let priority: i32 = row.get(8);
        let enabled: i32 = row.get(9);
        let created_by: Option<String> = row.get(10);
        let metadata_json: Option<String> = row.get(11);
        let created_at: String = row.get(12);
        let updated_at: String = row.get(13);

        let scope = LifecycleRuleScope::from_str(&scope_str).ok_or_else(|| {
            AosError::Database(format!("Invalid scope in database: {}", scope_str))
        })?;

        let rule_type = LifecycleRuleType::from_str(&rule_type_str).ok_or_else(|| {
            AosError::Database(format!("Invalid rule_type in database: {}", rule_type_str))
        })?;

        let conditions: Vec<LifecycleRuleCondition> = serde_json::from_str(&conditions_json)
            .map_err(|e| AosError::Database(format!("Failed to parse conditions_json: {}", e)))?;

        let actions: Vec<LifecycleRuleAction> = serde_json::from_str(&actions_json)
            .map_err(|e| AosError::Database(format!("Failed to parse actions_json: {}", e)))?;

        Ok(LifecycleRule {
            id,
            name,
            description,
            scope,
            scope_target,
            rule_type,
            conditions,
            actions,
            priority,
            enabled: enabled != 0,
            created_by,
            created_at,
            updated_at,
            metadata_json,
        })
    }

    // Helper: Compare two JSON values using an operator
    fn compare_values(
        actual: &serde_json::Value,
        operator: &ConditionOperator,
        expected: &serde_json::Value,
    ) -> bool {
        use serde_json::Value;

        match operator {
            ConditionOperator::Equals => actual == expected,
            ConditionOperator::NotEquals => actual != expected,
            ConditionOperator::GreaterThan => Self::compare_numeric(actual, expected, |a, b| a > b),
            ConditionOperator::GreaterThanOrEqual => {
                Self::compare_numeric(actual, expected, |a, b| a >= b)
            }
            ConditionOperator::LessThan => Self::compare_numeric(actual, expected, |a, b| a < b),
            ConditionOperator::LessThanOrEqual => {
                Self::compare_numeric(actual, expected, |a, b| a <= b)
            }
            ConditionOperator::In => {
                if let Value::Array(arr) = expected {
                    arr.contains(actual)
                } else {
                    false
                }
            }
            ConditionOperator::NotIn => {
                if let Value::Array(arr) = expected {
                    !arr.contains(actual)
                } else {
                    true
                }
            }
            ConditionOperator::Contains => {
                if let (Value::String(a), Value::String(e)) = (actual, expected) {
                    a.contains(e)
                } else {
                    false
                }
            }
            ConditionOperator::NotContains => {
                if let (Value::String(a), Value::String(e)) = (actual, expected) {
                    !a.contains(e)
                } else {
                    true
                }
            }
        }
    }

    // Helper: Compare two values numerically
    fn compare_numeric<F>(actual: &serde_json::Value, expected: &serde_json::Value, f: F) -> bool
    where
        F: Fn(f64, f64) -> bool,
    {
        let a = Self::to_f64(actual);
        let e = Self::to_f64(expected);
        match (a, e) {
            (Some(a_val), Some(e_val)) => f(a_val, e_val),
            _ => false,
        }
    }

    // Helper: Convert a JSON value to f64
    fn to_f64(value: &serde_json::Value) -> Option<f64> {
        match value {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_requires_target() {
        assert!(!LifecycleRuleScope::System.requires_target());
        assert!(LifecycleRuleScope::Tenant.requires_target());
        assert!(LifecycleRuleScope::Category.requires_target());
        assert!(LifecycleRuleScope::Adapter.requires_target());
    }

    #[test]
    fn test_condition_operators() {
        let field_values = serde_json::json!({
            "state": "active",
            "count": 42,
            "name": "test-adapter"
        });

        // Equals
        let condition = LifecycleRuleCondition {
            field: "state".to_string(),
            operator: ConditionOperator::Equals,
            value: serde_json::json!("active"),
        };
        assert!(Db::evaluate_condition(&condition, &field_values).met);

        // GreaterThan
        let condition = LifecycleRuleCondition {
            field: "count".to_string(),
            operator: ConditionOperator::GreaterThan,
            value: serde_json::json!(40),
        };
        assert!(Db::evaluate_condition(&condition, &field_values).met);

        // Contains
        let condition = LifecycleRuleCondition {
            field: "name".to_string(),
            operator: ConditionOperator::Contains,
            value: serde_json::json!("adapter"),
        };
        assert!(Db::evaluate_condition(&condition, &field_values).met);

        // In
        let condition = LifecycleRuleCondition {
            field: "state".to_string(),
            operator: ConditionOperator::In,
            value: serde_json::json!(["active", "ready", "deprecated"]),
        };
        assert!(Db::evaluate_condition(&condition, &field_values).met);
    }

    #[test]
    fn test_transition_validation_result() {
        let allowed = TransitionValidationResult::allowed();
        assert!(allowed.allowed);
        assert!(allowed.denial_reason.is_none());

        let denied = TransitionValidationResult::denied("Not allowed");
        assert!(!denied.allowed);
        assert_eq!(denied.denial_reason, Some("Not allowed".to_string()));

        let with_warning = TransitionValidationResult::allowed().with_warning("Caution");
        assert!(with_warning.allowed);
        assert_eq!(with_warning.warnings, vec!["Caution".to_string()]);
    }
}
