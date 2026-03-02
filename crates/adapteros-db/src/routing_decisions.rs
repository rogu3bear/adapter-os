// Routing Decisions Database Module
// Purpose: Store and query router decision events for routing inspection and analysis
// Author: JKCA
// Date: 2025-11-17

use crate::Db;
use adapteros_core::{Result, Q15_GATE_DENOMINATOR};
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
    pub source_type: Option<String>,
    pub since: Option<String>, // ISO-8601 timestamp
    pub until: Option<String>, // ISO-8601 timestamp
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
            "#,
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
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to insert routing decision: {}", e))
        })?;

        Ok(decision.id.clone())
    }

    /// Query routing decisions with filters and pagination
    ///
    /// Args: `filters` - Query filters for filtering results
    /// Errors: `AosError::Database` if query fails
    pub async fn query_routing_decisions(
        &self,
        filters: &RoutingDecisionFilters,
    ) -> Result<Vec<RoutingDecision>> {
        let mut query = String::from(
            "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
             stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
             candidate_adapters, selected_adapter_ids, router_latency_us, \
             total_inference_latency_us, overhead_pct, created_at \
             FROM routing_decisions WHERE 1=1",
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
        if let Some(ref source_type) = filters.source_type {
            query.push_str(
                " AND request_id IS NOT NULL AND EXISTS (
                    SELECT 1 FROM chat_sessions cs
                    WHERE cs.id = routing_decisions.request_id
                      AND cs.tenant_id = routing_decisions.tenant_id
                      AND cs.source_type = ?
                )",
            );
            params.push(source_type.clone());
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
        if filters.min_entropy.is_some() {
            query.push_str(" AND entropy >= ?");
        }
        if filters.max_overhead_pct.is_some() {
            query.push_str(" AND overhead_pct <= ?");
        }

        // Add ordering
        query.push_str(" ORDER BY timestamp DESC");

        // Add pagination
        query.push_str(" LIMIT ?");
        query.push_str(" OFFSET ?");

        // Execute query with parameter binding
        let mut sql_query = sqlx::query_as::<_, RoutingDecision>(&query);
        for param in &params {
            sql_query = sql_query.bind(param);
        }
        if let Some(min_entropy) = filters.min_entropy {
            sql_query = sql_query.bind(min_entropy);
        }
        if let Some(max_overhead) = filters.max_overhead_pct {
            sql_query = sql_query.bind(max_overhead);
        }
        let limit = filters.limit.unwrap_or(50) as i64;
        let offset = filters.offset.unwrap_or(0) as i64;
        sql_query = sql_query.bind(limit);
        sql_query = sql_query.bind(offset);

        let decisions = sql_query.fetch_all(self.pool()).await.map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to query routing decisions: {}", e))
        })?;

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
             FROM routing_decisions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to get routing decision: {}", e))
        })?
        .ok_or_else(|| {
            adapteros_core::AosError::NotFound(format!("Routing decision not found: {}", id))
        })?;

        Ok(decision)
    }

    /// Get recent routing decisions for a stack
    ///
    /// Args: `stack_id` - The adapter stack ID, `limit` - Maximum number of results
    /// Errors: `AosError::Database` if query fails
    pub async fn get_stack_routing_decisions(
        &self,
        stack_id: &str,
        limit: usize,
    ) -> Result<Vec<RoutingDecision>> {
        let filters = RoutingDecisionFilters {
            stack_id: Some(stack_id.to_string()),
            limit: Some(limit),
            ..Default::default()
        };
        self.query_routing_decisions(&filters).await
    }

    /// Get routing decisions for a chat session (request_id)
    ///
    /// Args: `request_id` - The request ID to query, `limit` - Maximum number of results
    /// Errors: `AosError::Database` if query fails
    pub async fn get_session_routing_decisions(
        &self,
        request_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<RoutingDecision>> {
        let filters = RoutingDecisionFilters {
            request_id: Some(request_id.to_string()),
            limit,
            ..Default::default()
        };
        self.query_routing_decisions(&filters).await
    }

    /// Get routing decisions with high overhead (>8% budget)
    ///
    /// Args: `tenant_id` - Optional tenant filter, `limit` - Maximum number of results
    /// Errors: `AosError::Database` if query fails
    pub async fn get_high_overhead_decisions(
        &self,
        tenant_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<RoutingDecision>> {
        // Use view instead for performance
        // Using parameterized limit to prevent SQL injection
        let limit = limit as i64;

        let decisions = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, RoutingDecision>(
                "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
                 stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
                 candidate_adapters, selected_adapter_ids, router_latency_us, \
                 total_inference_latency_us, overhead_pct, created_at \
                 FROM routing_decisions_high_overhead WHERE tenant_id = ? LIMIT ?",
            )
            .bind(tid)
            .bind(limit)
            .fetch_all(self.pool())
            .await
        } else {
            sqlx::query_as::<_, RoutingDecision>(
                "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
                 stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
                 candidate_adapters, selected_adapter_ids, router_latency_us, \
                 total_inference_latency_us, overhead_pct, created_at \
                 FROM routing_decisions_high_overhead LIMIT ?",
            )
            .bind(limit)
            .fetch_all(self.pool())
            .await
        };

        decisions.map_err(|e| {
            adapteros_core::AosError::Database(format!(
                "Failed to query high overhead decisions: {}",
                e
            ))
        })
    }

    /// Get routing decisions with low entropy (<0.5)
    ///
    /// Args: `tenant_id` - Optional tenant filter, `limit` - Maximum number of results
    /// Errors: `AosError::Database` if query fails
    pub async fn get_low_entropy_decisions(
        &self,
        tenant_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<RoutingDecision>> {
        // Using parameterized limit to prevent SQL injection
        let limit = limit as i64;

        let decisions = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, RoutingDecision>(
                "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
                 stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
                 candidate_adapters, selected_adapter_ids, router_latency_us, \
                 total_inference_latency_us, overhead_pct, created_at \
                 FROM routing_decisions_low_entropy WHERE tenant_id = ? LIMIT ?",
            )
            .bind(tid)
            .bind(limit)
            .fetch_all(self.pool())
            .await
        } else {
            sqlx::query_as::<_, RoutingDecision>(
                "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
                 stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
                 candidate_adapters, selected_adapter_ids, router_latency_us, \
                 total_inference_latency_us, overhead_pct, created_at \
                 FROM routing_decisions_low_entropy LIMIT ?",
            )
            .bind(limit)
            .fetch_all(self.pool())
            .await
        };

        decisions.map_err(|e| {
            adapteros_core::AosError::Database(format!(
                "Failed to query low entropy decisions: {}",
                e
            ))
        })
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
            .map_err(|e| {
                adapteros_core::AosError::Database(format!(
                    "Failed to delete old routing decisions: {}",
                    e
                ))
            })?;

        Ok(result.rows_affected())
    }

    /// Get adapter usage statistics from routing decisions
    ///
    /// Aggregates adapter usage from routing_decisions table:
    /// - Count activations (where adapter appears in selected_adapter_ids)
    /// - Average gate value (from candidate_adapters JSON where adapter was selected)
    /// - Last used timestamp
    ///
    /// Args: `adapter_id` - The adapter ID to query
    /// Errors: `AosError::Database` if query fails
    pub async fn get_adapter_usage_stats(
        &self,
        adapter_id: &str,
    ) -> Result<(i64, f64, Option<String>)> {
        // Query routing decisions where adapter appears in selected_adapter_ids
        // Use exact matching with comma-separated list handling to avoid partial ID matches
        // Pattern: match adapter_id as whole word (preceded by comma or start, followed by comma or end)
        let decisions = sqlx::query_as::<_, RoutingDecision>(
            "SELECT id, tenant_id, timestamp, request_id, step, input_token_id, \
             stack_id, stack_hash, entropy, tau, entropy_floor, k_value, \
             candidate_adapters, selected_adapter_ids, router_latency_us, \
             total_inference_latency_us, overhead_pct, created_at \
             FROM routing_decisions \
             WHERE selected_adapter_ids = ? \
                OR selected_adapter_ids LIKE ? \
                OR selected_adapter_ids LIKE ? \
                OR selected_adapter_ids LIKE ? \
             ORDER BY timestamp DESC",
        )
        .bind(adapter_id) // Exact match
        .bind(format!("{},{}", adapter_id, "%")) // Start of list
        .bind(format!("%,{},%", adapter_id)) // Middle of list
        .bind(format!("%,{}", adapter_id)) // End of list
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!(
                "Failed to query adapter usage stats: {}",
                e
            ))
        })?;

        let call_count = decisions.len() as i64;
        let last_used = decisions.first().map(|d| d.timestamp.clone());

        // Calculate average gate value from candidate_adapters JSON
        // For each decision where adapter was selected, extract gate values
        // Note: selected_adapter_ids contains adapter_id strings, but candidate_adapters
        // contains adapter_idx (numeric). We average gate values from all selected
        // candidates in decisions where this adapter was selected.
        let mut gate_values = Vec::new();
        for decision in &decisions {
            if let Some(selected_ids) = &decision.selected_adapter_ids {
                // Check if this adapter was selected in this decision
                let adapter_selected = selected_ids.split(',').any(|id| id.trim() == adapter_id);

                if adapter_selected {
                    // Parse candidates and collect gate values from selected candidates
                    if let Ok(candidates) =
                        serde_json::from_str::<Vec<RouterCandidate>>(&decision.candidate_adapters)
                    {
                        // Get top-K candidates (selected ones) and average their gates
                        let mut sorted_candidates = candidates.clone();
                        sorted_candidates.sort_by(|a, b| b.gate_q15.cmp(&a.gate_q15));
                        let k = decision.k_value.unwrap_or(0) as usize;
                        for candidate in sorted_candidates.iter().take(k) {
                            if candidate.gate_q15 > 0 {
                                let gate_float = (candidate.gate_q15 as f32) / Q15_GATE_DENOMINATOR;
                                gate_values.push(gate_float as f64);
                            }
                        }
                    }
                }
            }
        }

        let avg_gate = if !gate_values.is_empty() {
            gate_values.iter().sum::<f64>() / gate_values.len() as f64
        } else {
            0.0
        };

        Ok((call_count, avg_gate, last_used))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_routing_decision_crud() {
        let db = Db::new_in_memory()
            .await
            .expect("Failed to create in-memory database");

        // Create required tenant for FK constraint
        let tenant_id = db
            .create_tenant("Default Tenant", false)
            .await
            .expect("Failed to create tenant");

        let decision = RoutingDecision {
            id: "test-decision-1".to_string(),
            tenant_id: tenant_id.clone(),
            timestamp: "2025-11-17T23:00:00Z".to_string(),
            request_id: Some("req-123".to_string()),
            step: 5,
            input_token_id: Some(42),
            stack_id: None,
            stack_hash: None,
            entropy: 0.75,
            tau: 0.1,
            entropy_floor: 0.01,
            k_value: Some(3),
            candidate_adapters: r#"[{"adapter_idx":0,"raw_score":0.5,"gate_q15":16384}]"#
                .to_string(),
            selected_adapter_ids: Some("adapter-1,adapter-2".to_string()),
            router_latency_us: Some(1500),
            total_inference_latency_us: Some(50000),
            overhead_pct: Some(3.0),
            created_at: "2025-11-17T23:00:00Z".to_string(),
        };

        // Insert
        let id = db
            .insert_routing_decision(&decision)
            .await
            .expect("Failed to insert decision");
        assert_eq!(id, "test-decision-1");

        // Get by ID
        let retrieved = db
            .get_routing_decision(&id)
            .await
            .expect("Failed to get decision");
        assert_eq!(retrieved.id, decision.id);
        assert_eq!(retrieved.entropy, decision.entropy);

        // Query with filters
        let filters = RoutingDecisionFilters {
            tenant_id: Some(tenant_id.clone()),
            limit: Some(10),
            ..Default::default()
        };
        let results = db
            .query_routing_decisions(&filters)
            .await
            .expect("Failed to query decisions");
        assert_eq!(results.len(), 1);
    }
}
