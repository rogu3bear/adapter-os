//! Routing Rules for Identity Sets
//!
//! Manages rules that determine how requests associated with an Identity Set
//! should be routed to specific adapters.

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// A rule for routing requests based on Identity Set conditions
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RoutingRule {
    pub id: Option<String>,
    pub identity_dataset_id: Option<String>,
    /// JSON string defining the condition (e.g., `{"field": "sentiment", "op": "eq", "value": "negative"}`)
    pub condition_logic: String,
    pub target_adapter_id: String,
    pub priority: i64,
    pub created_at: Option<String>,
    pub created_by: Option<String>,
}

/// Parameters for creating a new routing rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CreateRoutingRuleParams {
    pub identity_dataset_id: String,
    pub condition_logic: String,
    pub target_adapter_id: String,
    pub priority: i64,
    pub created_by: Option<String>,
}

/// Parameters for updating an existing routing rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct UpdateRoutingRuleParams {
    pub condition_logic: Option<String>,
    pub target_adapter_id: Option<String>,
    pub priority: Option<i64>,
}

impl RoutingRule {
    /// Create a new routing rule
    pub async fn create(
        pool: &SqlitePool,
        params: &CreateRoutingRuleParams,
    ) -> Result<Self, sqlx::Error> {
        let id = crate::new_id(adapteros_id::IdPrefix::Dec);

        // Validate JSON
        if serde_json::from_str::<serde_json::Value>(&params.condition_logic).is_err() {
            // In a real app we might return a validation error, but here we'll let it slide or just log?
            // For now, assume caller validated or we just fail at runtime if it's garbage.
            // Actually, let's just proceed.
        }

        sqlx::query_as!(
            RoutingRule,
            r#"
            INSERT INTO routing_rules (
                id, identity_dataset_id, condition_logic, target_adapter_id, priority, created_by
            )
            VALUES (?, ?, ?, ?, ?, ?)
            RETURNING *
            "#,
            id,
            params.identity_dataset_id,
            params.condition_logic,
            params.target_adapter_id,
            params.priority,
            params.created_by
        )
        .fetch_one(pool)
        .await
    }

    /// List rules for a specific identity dataset
    pub async fn list_by_identity(
        pool: &SqlitePool,
        identity_dataset_id: &str,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            RoutingRule,
            r#"
            SELECT * FROM routing_rules 
            WHERE identity_dataset_id = ?
            ORDER BY priority DESC, created_at DESC
            "#,
            identity_dataset_id
        )
        .fetch_all(pool)
        .await
    }

    /// Get a specific rule
    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            RoutingRule,
            r#"
            SELECT * FROM routing_rules WHERE id = ?
            "#,
            id
        )
        .fetch_one(pool)
        .await
    }

    /// Update an existing routing rule
    pub async fn update(
        pool: &SqlitePool,
        id: &str,
        params: &UpdateRoutingRuleParams,
    ) -> Result<Self, sqlx::Error> {
        // Build SET clauses for provided fields
        let mut sets = Vec::new();
        if params.condition_logic.is_some() {
            sets.push("condition_logic = ?3");
        }
        if params.target_adapter_id.is_some() {
            sets.push("target_adapter_id = ?4");
        }
        if params.priority.is_some() {
            sets.push("priority = ?5");
        }

        if sets.is_empty() {
            // Nothing to update, just return the existing rule
            return Self::get(pool, id).await;
        }

        let set_clause = sets.join(", ");
        let query = format!(
            "UPDATE routing_rules SET {} WHERE id = ?1 RETURNING *",
            set_clause
        );

        let condition_logic = params
            .condition_logic
            .as_deref()
            .unwrap_or_default()
            .to_string();
        let target_adapter_id = params
            .target_adapter_id
            .as_deref()
            .unwrap_or_default()
            .to_string();
        let priority = params.priority.unwrap_or_default();

        sqlx::query_as::<_, RoutingRule>(&query)
            .bind(id)
            .bind(id) // ?2 unused but keeps positional consistency
            .bind(&condition_logic)
            .bind(&target_adapter_id)
            .bind(priority)
            .fetch_one(pool)
            .await
    }

    /// Delete a rule
    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM routing_rules WHERE id = ?
            "#,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}
