// Routing Decisions Database Module
// Purpose: Store and query router decision events for PRD-04 Routing Inspector
// Author: JKCA
// Date: 2025-11-17

use crate::Db;
use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Router candidate structure matching telemetry RouterCandidate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterCandidate {
    pub adapter_idx: u16,
    pub raw_score: f32,
    pub gate_q15: i16,
}

/// Routing decision database record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoutingDecision {
    pub id: String,
    pub tenant_id: String,
    pub timestamp: String,
    pub request_id: Option<String>,

    // Router Decision Context
    pub step: i64,
    pub input_token_id: Option<i64>,
    pub stack_id: Option<String>,
    pub stack_hash: Option<String>,

    // Routing Parameters
    pub entropy: f64,
    pub tau: f64,
    pub entropy_floor: f64,
    pub k_value: Option<i64>,

    // Candidate Adapters (JSON)
    pub candidate_adapters: String,
    pub selected_adapter_ids: Option<String>,

    // Timing Metrics
    pub router_latency_us: Option<i64>,
    pub total_inference_latency_us: Option<i64>,
    pub overhead_pct: Option<f64>,

    pub created_at: String,
}

/// Query filters for routing decisions
#[derive(Debug, Clone, Default)]
pub struct RoutingDecisionFilters {
    pub tenant_id: Option<String>,
    pub stack_id: Option<String>,
    pub adapter_id: Option<String>,
    pub request_id: Option<String>,
    pub since: Option<String>,  // ISO-8601 timestamp
    pub until: Option<String>,  // ISO-8601 timestamp
    pub min_entropy: Option<f64>,
    pub max_overhead_pct: Option<f64>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl Db {
    /// Insert a new routing decision record
    ///
    /// Args: `decision` - The routing decision to insert
    /// Errors: `AosError::Database` if insertion fails
    pub async fn insert_routing_decision(&self, decision: &RoutingDecision) -> Result<String> {
        sqlx::query(
            r#"
            INSERT INTO routing_decisions (
                id, tenant_id, timestamp, request_id, step, input_token_id,
                stack_id, stack_hash, entropy, tau, entropy_floor, k_value,
                candidate_adapters, selected_adapter_ids, router_latency_us,
                total_inference_latency_us, overhead_pct, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#
        )
        .bind(&decision.id)
        .bind(&decision.tenant_id)
        .bind(&decision.timestamp)
        .bind(&decision.request_id)
        .bind(decision.step)
        .bind(decision.input_token_id)
        .bind(&decision.stack_id)
        .bind(&decision.stack_hash)
        .bind(decision.entropy)
        .bind(decision.tau)
        .bind(decision.entropy_floor)
        .bind(decision.k_value)
        .bind(&decision.candidate_adapters)
        .bind(&decision.selected_adapter_ids)
        .bind(decision.router_latency_us)
        .bind(decision.total_inference_latency_us)
        .bind(decision.overhead_pct)
        .execute(self.pool())
        .await
        .map_err(|e| adapteros_core::AosError::Database(format!("Failed to insert routing decision: {}", e)))?;

        Ok(decision.id.clone())
    }

    /// Query routing decisions with filters and pagination
    ///
    /// Args: `filters` - Query filters for filtering results
    /// Errors: `AosError::Database` if query fails
    pub async fn query_routing_decisions(&self, filters: &RoutingDecisionFilters) -> Result<Vec<RoutingDecision>> {
        let mut query = String::from(
            "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
             stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
             candidate_adapters, selected_adapter_ids, router_latency_us, \
             total_inference_latency_us, overhead_pct, created_at \
             FROM routing_decisions WHERE 1=1"
        );
        let mut params: Vec<String> = Vec::new();

        // Build dynamic WHERE clause
        if let Some(ref tenant_id) = filters.tenant_id {
            query.push_str(" AND tenant_id = ?");
            params.push(tenant_id.clone());
        }
        if let Some(ref stack_id) = filters.stack_id {
            query.push_str(" AND stack_id = ?");
            params.push(stack_id.clone());
        }
        if let Some(ref adapter_id) = filters.adapter_id {
            query.push_str(" AND selected_adapter_ids LIKE ?");
            params.push(format!("%{}%", adapter_id));
        }
        if let Some(ref request_id) = filters.request_id {
            query.push_str(" AND request_id = ?");
            params.push(request_id.clone());
        }
        if let Some(ref since) = filters.since {
            query.push_str(" AND timestamp >= ?");
            params.push(since.clone());
        }
        if let Some(ref until) = filters.until {
            query.push_str(" AND timestamp <= ?");
            params.push(until.clone());
        }
        if let Some(min_entropy) = filters.min_entropy {
            query.push_str(&format!(" AND entropy >= {}", min_entropy));
        }
        if let Some(max_overhead) = filters.max_overhead_pct {
            query.push_str(&format!(" AND overhead_pct <= {}", max_overhead));
        }

        // Add ordering
        query.push_str(" ORDER BY timestamp DESC");

        // Add pagination
        if let Some(limit) = filters.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        } else {
            query.push_str(" LIMIT 50");  // Default limit
        }
        if let Some(offset) = filters.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        // Execute query with parameter binding
        let mut sql_query = sqlx::query_as::<_, RoutingDecision>(&query);
        for param in &params {
            sql_query = sql_query.bind(param);
        }

        let decisions = sql_query
            .fetch_all(self.pool())
            .await
            .map_err(|e| adapteros_core::AosError::Database(format!("Failed to query routing decisions: {}", e)))?;

        Ok(decisions)
    }

    /// Get a single routing decision by ID
    ///
    /// Args: `id` - The routing decision ID
    /// Errors: `AosError::Database` if query fails, `AosError::NotFound` if not found
    pub async fn get_routing_decision(&self, id: &str) -> Result<RoutingDecision> {
        let decision = sqlx::query_as::<_, RoutingDecision>(
            "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
             stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
             candidate_adapters, selected_adapter_ids, router_latency_us, \
             total_inference_latency_us, overhead_pct, created_at \
             FROM routing_decisions WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| adapteros_core::AosError::Database(format!("Failed to get routing decision: {}", e)))?
        .ok_or_else(|| adapteros_core::AosError::NotFound(format!("Routing decision not found: {}", id)))?;

        Ok(decision)
    }

    /// Get recent routing decisions for a stack
    ///
    /// Args: `stack_id` - The adapter stack ID, `limit` - Maximum number of results
    /// Errors: `AosError::Database` if query fails
    pub async fn get_stack_routing_decisions(&self, stack_id: &str, limit: usize) -> Result<Vec<RoutingDecision>> {
        let filters = RoutingDecisionFilters {
            stack_id: Some(stack_id.to_string()),
            limit: Some(limit),
            ..Default::default()
        };
        self.query_routing_decisions(&filters).await
    }

    /// Get routing decisions with high overhead (>8% budget)
    ///
    /// Args: `tenant_id` - Optional tenant filter, `limit` - Maximum number of results
    /// Errors: `AosError::Database` if query fails
    pub async fn get_high_overhead_decisions(&self, tenant_id: Option<String>, limit: usize) -> Result<Vec<RoutingDecision>> {
        // Use view instead for performance
        let query = if tenant_id.is_some() {
            format!(
                "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
                 stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
                 candidate_adapters, selected_adapter_ids, router_latency_us, \
                 total_inference_latency_us, overhead_pct, created_at \
                 FROM routing_decisions_high_overhead WHERE tenant_id = ? LIMIT {}",
                limit
            )
        } else {
            format!(
                "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
                 stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
                 candidate_adapters, selected_adapter_ids, router_latency_us, \
                 total_inference_latency_us, overhead_pct, created_at \
                 FROM routing_decisions_high_overhead LIMIT {}",
                limit
            )
        };

        let mut sql_query = sqlx::query_as::<_, RoutingDecision>(&query);
        if let Some(tid) = tenant_id {
            sql_query = sql_query.bind(tid);
        }

        let decisions = sql_query
            .fetch_all(self.pool())
            .await
            .map_err(|e| adapteros_core::AosError::Database(format!("Failed to query high overhead decisions: {}", e)))?;

        Ok(decisions)
    }

    /// Get routing decisions with low entropy (<0.5)
    ///
    /// Args: `tenant_id` - Optional tenant filter, `limit` - Maximum number of results
    /// Errors: `AosError::Database` if query fails
    pub async fn get_low_entropy_decisions(&self, tenant_id: Option<String>, limit: usize) -> Result<Vec<RoutingDecision>> {
        let query = if tenant_id.is_some() {
            format!(
                "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
                 stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
                 candidate_adapters, selected_adapter_ids, router_latency_us, \
                 total_inference_latency_us, overhead_pct, created_at \
                 FROM routing_decisions_low_entropy WHERE tenant_id = ? LIMIT {}",
                limit
            )
        } else {
            format!(
                "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
                 stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
                 candidate_adapters, selected_adapter_ids, router_latency_us, \
                 total_inference_latency_us, overhead_pct, created_at \
                 FROM routing_decisions_low_entropy LIMIT {}",
                limit
            )
        };

        let mut sql_query = sqlx::query_as::<_, RoutingDecision>(&query);
        if let Some(tid) = tenant_id {
            sql_query = sql_query.bind(tid);
        }

        let decisions = sql_query
            .fetch_all(self.pool())
            .await
            .map_err(|e| adapteros_core::AosError::Database(format!("Failed to query low entropy decisions: {}", e)))?;

        Ok(decisions)
    }

    /// Delete old routing decisions (cleanup)
    ///
    /// Args: `older_than` - ISO-8601 timestamp, delete decisions older than this
    /// Errors: `AosError::Database` if deletion fails
    pub async fn delete_old_routing_decisions(&self, older_than: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM routing_decisions WHERE timestamp < ?")
            .bind(older_than)
            .execute(self.pool())
            .await
            .map_err(|e| adapteros_core::AosError::Database(format!("Failed to delete old routing decisions: {}", e)))?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_routing_decision_crud() {
        let db = Db::new_in_memory().await.expect("Failed to create in-memory database");

        let decision = RoutingDecision {
            id: "test-decision-1".to_string(),
            tenant_id: "default".to_string(),
            timestamp: "2025-11-17T23:00:00Z".to_string(),
            request_id: Some("req-123".to_string()),
            step: 5,
            input_token_id: Some(42),
            stack_id: Some("stack-1".to_string()),
            stack_hash: Some("abc123".to_string()),
            entropy: 0.75,
            tau: 0.1,
            entropy_floor: 0.01,
            k_value: Some(3),
            candidate_adapters: r#"[{"adapter_idx":0,"raw_score":0.5,"gate_q15":16384}]"#.to_string(),
            selected_adapter_ids: Some("adapter-1,adapter-2".to_string()),
            router_latency_us: Some(1500),
            total_inference_latency_us: Some(50000),
            overhead_pct: Some(3.0),
            created_at: "2025-11-17T23:00:00Z".to_string(),
        };

        // Insert
        let id = db.insert_routing_decision(&decision).await.expect("Failed to insert decision");
        assert_eq!(id, "test-decision-1");

        // Get by ID
        let retrieved = db.get_routing_decision(&id).await.expect("Failed to get decision");
        assert_eq!(retrieved.id, decision.id);
        assert_eq!(retrieved.entropy, decision.entropy);

        // Query with filters
        let filters = RoutingDecisionFilters {
            tenant_id: Some("default".to_string()),
            limit: Some(10),
            ..Default::default()
        };
        let results = db.query_routing_decisions(&filters).await.expect("Failed to query decisions");
        assert_eq!(results.len(), 1);
    }
}
