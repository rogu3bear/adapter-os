//! Diagnostic event persistence layer.
//!
//! Provides storage operations for diagnostic runs and events.
//! Implements the `DiagPersister` trait for integration with the background writer.

use adapteros_core::{AosError, Result};
use adapteros_diagnostics::writer::{DiagPersister, PersistError, SequencedEvent};
use adapteros_diagnostics::{DiagEnvelope, DiagEvent, DiagSeverity};
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::{debug, warn};

/// Insert a new diagnostic run record.
///
/// Call this at the start of each request to create the run entry.
pub async fn insert_diag_run(
    pool: &SqlitePool,
    run_id: &str,
    tenant_id: &str,
    trace_id: &str,
    started_at_unix_ms: i64,
    request_hash: &str,
    manifest_hash: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO diag_runs (id, tenant_id, trace_id, started_at_unix_ms, request_hash, manifest_hash)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(run_id)
    .bind(tenant_id)
    .bind(trace_id)
    .bind(started_at_unix_ms)
    .bind(request_hash)
    .bind(manifest_hash)
    .execute(pool)
    .await
    .map_err(|e| AosError::Database(format!("insert_diag_run: {}", e)))?;

    Ok(())
}

/// Insert a batch of diagnostic events in a single transaction.
///
/// This is the primary write path - uses a transaction for atomicity.
pub async fn insert_diag_events_batch(
    pool: &SqlitePool,
    events: &[SequencedEvent],
) -> Result<usize> {
    if events.is_empty() {
        return Ok(0);
    }

    // Start transaction
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AosError::Database(format!("begin transaction: {}", e)))?;

    let mut inserted = 0;

    for event in events {
        let payload_json =
            serde_json::to_string(&event.envelope.payload).map_err(AosError::Serialization)?;

        let severity_str = severity_to_str(&event.envelope.severity);
        let event_type = extract_event_type(&event.envelope.payload);

        let result = sqlx::query(
            r#"
            INSERT INTO diag_events (tenant_id, run_id, seq, mono_us, event_type, severity, payload_json)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&event.envelope.tenant_id)
        .bind(event.envelope.run_id.as_str())
        .bind(event.seq as i64)
        .bind(event.envelope.emitted_at_mono_us as i64)
        .bind(event_type)
        .bind(severity_str)
        .bind(&payload_json)
        .execute(&mut *tx)
        .await;

        match result {
            Ok(_) => inserted += 1,
            Err(e) => {
                warn!(
                    run_id = event.envelope.run_id.as_str(),
                    seq = event.seq,
                    error = %e,
                    "Failed to insert diag event"
                );
                // Continue with other events
            }
        }
    }

    tx.commit()
        .await
        .map_err(|e| AosError::Database(format!("commit transaction: {}", e)))?;

    debug!(inserted, total = events.len(), "Inserted diag events batch");

    Ok(inserted)
}

/// Update run statistics after events are persisted.
pub async fn update_run_event_count(
    pool: &SqlitePool,
    run_id: &str,
    events_added: u64,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE diag_runs
        SET total_events_count = total_events_count + ?
        WHERE id = ?
        "#,
    )
    .bind(events_added as i64)
    .bind(run_id)
    .execute(pool)
    .await
    .map_err(|e| AosError::Database(format!("update_run_event_count: {}", e)))?;

    Ok(())
}

/// Complete a diagnostic run with final status and drop count.
pub async fn complete_diag_run(
    pool: &SqlitePool,
    run_id: &str,
    status: &str,
    dropped_events_count: u64,
) -> Result<()> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    sqlx::query(
        r#"
        UPDATE diag_runs
        SET status = ?,
            completed_at_unix_ms = ?,
            dropped_events_count = ?
        WHERE id = ?
        "#,
    )
    .bind(status)
    .bind(now_ms)
    .bind(dropped_events_count as i64)
    .bind(run_id)
    .execute(pool)
    .await
    .map_err(|e| AosError::Database(format!("complete_diag_run: {}", e)))?;

    Ok(())
}

/// Decision chain commit data for updating a diagnostic run.
#[derive(Debug, Clone, Default)]
pub struct DecisionChainCommit {
    /// Decision chain hash (BLAKE3 hex)
    pub decision_chain_hash: Option<String>,
    /// Backend identity hash (BLAKE3 hex)
    pub backend_identity_hash: Option<String>,
    /// Model identity hash (BLAKE3 hex)
    pub model_identity_hash: Option<String>,
    /// JSON array of adapter stack stable IDs
    pub adapter_stack_ids: Option<String>,
}

impl DecisionChainCommit {
    /// Create a new empty commit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the decision chain hash.
    pub fn with_decision_chain_hash(mut self, hash: impl Into<String>) -> Self {
        self.decision_chain_hash = Some(hash.into());
        self
    }

    /// Set the backend identity hash.
    pub fn with_backend_identity_hash(mut self, hash: impl Into<String>) -> Self {
        self.backend_identity_hash = Some(hash.into());
        self
    }

    /// Set the model identity hash.
    pub fn with_model_identity_hash(mut self, hash: impl Into<String>) -> Self {
        self.model_identity_hash = Some(hash.into());
        self
    }

    /// Set the adapter stack IDs as JSON array.
    pub fn with_adapter_stack_ids(mut self, ids: &[String]) -> Self {
        self.adapter_stack_ids = Some(serde_json::to_string(ids).unwrap_or_default());
        self
    }
}

/// Update the decision chain hash and related fields for a diagnostic run.
///
/// Call this after inference completes to commit the decision chain hash
/// and environment identity to the run record.
pub async fn update_diag_run_decision_chain(
    pool: &SqlitePool,
    run_id: &str,
    commit: &DecisionChainCommit,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE diag_runs
        SET decision_chain_hash = COALESCE(?, decision_chain_hash),
            backend_identity_hash = COALESCE(?, backend_identity_hash),
            model_identity_hash = COALESCE(?, model_identity_hash),
            adapter_stack_ids = COALESCE(?, adapter_stack_ids)
        WHERE id = ?
        "#,
    )
    .bind(&commit.decision_chain_hash)
    .bind(&commit.backend_identity_hash)
    .bind(&commit.model_identity_hash)
    .bind(&commit.adapter_stack_ids)
    .bind(run_id)
    .execute(pool)
    .await
    .map_err(|e| AosError::Database(format!("update_diag_run_decision_chain: {}", e)))?;

    Ok(())
}

/// Get the event count for a specific run.
pub async fn get_run_event_count(pool: &SqlitePool, run_id: &str) -> Result<u64> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT total_events_count FROM diag_runs WHERE id = ?
        "#,
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_run_event_count: {}", e)))?;

    Ok(row.0 as u64)
}

/// List recent diagnostic runs for a tenant.
pub async fn list_diag_runs_for_tenant(
    pool: &SqlitePool,
    tenant_id: &str,
    limit: u32,
) -> Result<Vec<DiagRunSummary>> {
    let rows = sqlx::query_as::<_, DiagRunSummary>(
        r#"
        SELECT id, tenant_id, trace_id, started_at_unix_ms, completed_at_unix_ms,
               status, total_events_count, dropped_events_count, created_at
        FROM diag_runs
        WHERE tenant_id = ?
        ORDER BY created_at DESC
        LIMIT ?
        "#,
    )
    .bind(tenant_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AosError::Database(format!("list_diag_runs_for_tenant: {}", e)))?;

    Ok(rows)
}

/// Summary of a diagnostic run.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DiagRunSummary {
    pub id: String,
    pub tenant_id: String,
    pub trace_id: String,
    pub started_at_unix_ms: i64,
    pub completed_at_unix_ms: Option<i64>,
    pub status: String,
    pub total_events_count: i64,
    pub dropped_events_count: i64,
    pub created_at: String,
}

/// Full diagnostic run record (includes request_hash and manifest_hash).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DiagRunRecord {
    pub id: String,
    pub tenant_id: String,
    pub trace_id: String,
    pub started_at_unix_ms: i64,
    pub completed_at_unix_ms: Option<i64>,
    pub request_hash: String,
    pub manifest_hash: Option<String>,
    pub status: String,
    pub total_events_count: i64,
    pub dropped_events_count: i64,
    pub created_at: String,
    pub updated_at: String,
    /// Decision chain hash (BLAKE3 Merkle root of router decisions)
    pub decision_chain_hash: Option<String>,
    /// Backend/environment identity hash
    pub backend_identity_hash: Option<String>,
    /// Model identity hash (weights/manifest hash)
    pub model_identity_hash: Option<String>,
    /// JSON array of adapter stack stable IDs
    pub adapter_stack_ids: Option<String>,
}

/// Diagnostic event record.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DiagEventRecord {
    pub id: i64,
    pub tenant_id: String,
    pub run_id: String,
    pub seq: i64,
    pub mono_us: i64,
    pub event_type: String,
    pub severity: String,
    pub payload_json: String,
    pub created_at: String,
}

// ============================================================================
// Tenant-safe query functions for the diagnostics API
// ============================================================================

/// List diagnostic runs for a tenant with pagination and filtering.
///
/// Uses cursor-based pagination with the run ID as cursor.
/// Always filters by tenant_id for multi-tenant safety.
pub async fn list_diag_runs_paginated(
    pool: &SqlitePool,
    tenant_id: &str,
    since: Option<i64>,
    limit: u32,
    after_cursor: Option<&str>,
    status_filter: Option<&str>,
) -> Result<(Vec<DiagRunRecord>, i64)> {
    let limit = limit.min(200) as i64;

    // Build the query dynamically based on filters
    let mut conditions = vec!["tenant_id = ?1"];
    let mut bind_idx = 2;

    if since.is_some() {
        conditions.push("started_at_unix_ms >= ?2");
        bind_idx = 3;
    }

    if after_cursor.is_some() {
        conditions.push(if since.is_some() {
            "id < ?3"
        } else {
            "id < ?2"
        });
        bind_idx = if since.is_some() { 4 } else { 3 };
    }

    if status_filter.is_some() {
        let status_cond = format!("status = ?{}", bind_idx);
        conditions.push(Box::leak(status_cond.into_boxed_str()));
    }

    let where_clause = conditions.join(" AND ");
    let query = format!(
        r#"
        SELECT id, tenant_id, trace_id, started_at_unix_ms, completed_at_unix_ms,
               request_hash, manifest_hash, status, total_events_count,
               dropped_events_count, created_at, updated_at,
               decision_chain_hash, backend_identity_hash, model_identity_hash, adapter_stack_ids
        FROM diag_runs
        WHERE {}
        ORDER BY id DESC
        LIMIT ?
        "#,
        where_clause
    );

    // Execute with dynamic bindings
    let mut query_builder = sqlx::query_as::<_, DiagRunRecord>(&query);
    query_builder = query_builder.bind(tenant_id);

    if let Some(since_val) = since {
        query_builder = query_builder.bind(since_val);
    }

    if let Some(cursor) = after_cursor {
        query_builder = query_builder.bind(cursor);
    }

    if let Some(status) = status_filter {
        query_builder = query_builder.bind(status);
    }

    query_builder = query_builder.bind(limit + 1); // Fetch one extra to check for more

    let mut rows = query_builder
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("list_diag_runs_paginated: {}", e)))?;

    // Check if there are more results
    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.pop();
    }

    // Get total count for pagination UI
    let total_count = count_diag_runs_for_tenant(pool, tenant_id, since, status_filter).await?;

    Ok((rows, total_count))
}

/// Count diagnostic runs for a tenant with optional filters.
pub async fn count_diag_runs_for_tenant(
    pool: &SqlitePool,
    tenant_id: &str,
    since: Option<i64>,
    status_filter: Option<&str>,
) -> Result<i64> {
    let (count,): (i64,) = match (since, status_filter) {
        (Some(since_val), Some(status)) => {
            sqlx::query_as(
                r#"
                SELECT COUNT(*) FROM diag_runs
                WHERE tenant_id = ? AND started_at_unix_ms >= ? AND status = ?
                "#,
            )
            .bind(tenant_id)
            .bind(since_val)
            .bind(status)
            .fetch_one(pool)
            .await
        }
        (Some(since_val), None) => {
            sqlx::query_as(
                r#"
                SELECT COUNT(*) FROM diag_runs
                WHERE tenant_id = ? AND started_at_unix_ms >= ?
                "#,
            )
            .bind(tenant_id)
            .bind(since_val)
            .fetch_one(pool)
            .await
        }
        (None, Some(status)) => {
            sqlx::query_as(
                r#"
                SELECT COUNT(*) FROM diag_runs
                WHERE tenant_id = ? AND status = ?
                "#,
            )
            .bind(tenant_id)
            .bind(status)
            .fetch_one(pool)
            .await
        }
        (None, None) => {
            sqlx::query_as(
                r#"
                SELECT COUNT(*) FROM diag_runs
                WHERE tenant_id = ?
                "#,
            )
            .bind(tenant_id)
            .fetch_one(pool)
            .await
        }
    }
    .map_err(|e| AosError::Database(format!("count_diag_runs_for_tenant: {}", e)))?;

    Ok(count)
}

/// Get a diagnostic run by trace_id with tenant isolation.
///
/// Returns None if the run doesn't exist or belongs to a different tenant.
pub async fn get_diag_run_by_trace_id(
    pool: &SqlitePool,
    tenant_id: &str,
    trace_id: &str,
) -> Result<Option<DiagRunRecord>> {
    let row = sqlx::query_as::<_, DiagRunRecord>(
        r#"
        SELECT id, tenant_id, trace_id, started_at_unix_ms, completed_at_unix_ms,
               request_hash, manifest_hash, status, total_events_count,
               dropped_events_count, created_at, updated_at,
               decision_chain_hash, backend_identity_hash, model_identity_hash, adapter_stack_ids
        FROM diag_runs
        WHERE tenant_id = ? AND trace_id = ?
        "#,
    )
    .bind(tenant_id)
    .bind(trace_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_diag_run_by_trace_id: {}", e)))?;

    Ok(row)
}

/// Get a diagnostic run by run_id with tenant isolation.
///
/// Returns None if the run doesn't exist or belongs to a different tenant.
pub async fn get_diag_run_by_id(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
) -> Result<Option<DiagRunRecord>> {
    let row = sqlx::query_as::<_, DiagRunRecord>(
        r#"
        SELECT id, tenant_id, trace_id, started_at_unix_ms, completed_at_unix_ms,
               request_hash, manifest_hash, status, total_events_count,
               dropped_events_count, created_at, updated_at,
               decision_chain_hash, backend_identity_hash, model_identity_hash, adapter_stack_ids
        FROM diag_runs
        WHERE tenant_id = ? AND id = ?
        "#,
    )
    .bind(tenant_id)
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_diag_run_by_id: {}", e)))?;

    Ok(row)
}

/// List diagnostic events for a run with pagination and filtering.
///
/// Uses sequence-based cursor pagination.
/// Always verifies the run belongs to the tenant for multi-tenant safety.
pub async fn list_diag_events_paginated(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    after_seq: Option<i64>,
    limit: u32,
    event_type_filter: Option<&str>,
    severity_filter: Option<&str>,
) -> Result<Vec<DiagEventRecord>> {
    let limit = limit.min(1000) as i64;

    // First verify the run belongs to the tenant
    let run_exists: Option<(i64,)> =
        sqlx::query_as(r#"SELECT 1 FROM diag_runs WHERE id = ? AND tenant_id = ?"#)
            .bind(run_id)
            .bind(tenant_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AosError::Database(format!("verify run ownership: {}", e)))?;

    if run_exists.is_none() {
        return Ok(vec![]); // Run doesn't exist or belongs to different tenant
    }

    // Build query based on filters
    let mut conditions = vec!["run_id = ?", "tenant_id = ?"];

    if after_seq.is_some() {
        conditions.push("seq > ?");
    }

    if event_type_filter.is_some() {
        conditions.push("event_type = ?");
    }

    if severity_filter.is_some() {
        conditions.push("severity = ?");
    }

    let where_clause = conditions.join(" AND ");
    let query = format!(
        r#"
        SELECT id, tenant_id, run_id, seq, mono_us, event_type, severity, payload_json, created_at
        FROM diag_events
        WHERE {}
        ORDER BY seq ASC
        LIMIT ?
        "#,
        where_clause
    );

    let mut query_builder = sqlx::query_as::<_, DiagEventRecord>(&query);
    query_builder = query_builder.bind(run_id).bind(tenant_id);

    if let Some(seq) = after_seq {
        query_builder = query_builder.bind(seq);
    }

    if let Some(event_type) = event_type_filter {
        query_builder = query_builder.bind(event_type);
    }

    if let Some(severity) = severity_filter {
        query_builder = query_builder.bind(severity);
    }

    query_builder = query_builder.bind(limit + 1); // Fetch one extra to check for more

    let rows = query_builder
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("list_diag_events_paginated: {}", e)))?;

    Ok(rows)
}

/// Get all events for a run (for export), with tenant isolation.
///
/// Used for the export endpoint. Has a hard limit to prevent memory issues.
pub async fn get_all_diag_events_for_run(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    max_events: u32,
) -> Result<Vec<DiagEventRecord>> {
    let max_events = max_events.min(50000) as i64;

    // Verify run belongs to tenant
    let run_exists: Option<(i64,)> =
        sqlx::query_as(r#"SELECT 1 FROM diag_runs WHERE id = ? AND tenant_id = ?"#)
            .bind(run_id)
            .bind(tenant_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AosError::Database(format!("verify run ownership: {}", e)))?;

    if run_exists.is_none() {
        return Ok(vec![]);
    }

    let rows = sqlx::query_as::<_, DiagEventRecord>(
        r#"
        SELECT id, tenant_id, run_id, seq, mono_us, event_type, severity, payload_json, created_at
        FROM diag_events
        WHERE run_id = ? AND tenant_id = ?
        ORDER BY seq ASC
        LIMIT ?
        "#,
    )
    .bind(run_id)
    .bind(tenant_id)
    .bind(max_events)
    .fetch_all(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_all_diag_events_for_run: {}", e)))?;

    Ok(rows)
}

/// Get stage timing summary for a run.
///
/// Extracts timing information from stage_enter and stage_complete events.
pub async fn get_stage_timing_summary(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
) -> Result<Vec<(String, i64, Option<i64>, bool)>> {
    // Verify run belongs to tenant
    let run_exists: Option<(i64,)> =
        sqlx::query_as(r#"SELECT 1 FROM diag_runs WHERE id = ? AND tenant_id = ?"#)
            .bind(run_id)
            .bind(tenant_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AosError::Database(format!("verify run ownership: {}", e)))?;

    if run_exists.is_none() {
        return Ok(vec![]);
    }

    // Get stage events
    let events = sqlx::query_as::<_, (String, i64, String)>(
        r#"
        SELECT event_type, mono_us, payload_json
        FROM diag_events
        WHERE run_id = ? AND tenant_id = ?
          AND event_type IN ('stage_enter', 'stage_complete', 'stage_failed')
        ORDER BY mono_us ASC
        "#,
    )
    .bind(run_id)
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_stage_timing_summary: {}", e)))?;

    // Parse and aggregate timing info
    let mut stage_starts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut results: Vec<(String, i64, Option<i64>, bool)> = Vec::new();

    for (event_type, mono_us, payload_json) in events {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&payload_json) {
            let stage_name = payload
                .get("stage")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            match event_type.as_str() {
                "stage_enter" => {
                    stage_starts.insert(stage_name, mono_us);
                }
                "stage_complete" => {
                    if let Some(start) = stage_starts.remove(&stage_name) {
                        let duration = mono_us - start;
                        results.push((stage_name, start, Some(duration), true));
                    }
                }
                "stage_failed" => {
                    if let Some(start) = stage_starts.remove(&stage_name) {
                        let duration = mono_us - start;
                        results.push((stage_name, start, Some(duration), false));
                    }
                }
                _ => {}
            }
        }
    }

    // Add any stages that started but never completed
    for (stage_name, start) in stage_starts {
        results.push((stage_name, start, None, false));
    }

    Ok(results)
}

/// Parsed router step event for comparison.
#[derive(Debug, Clone)]
pub struct RouterStepEvent {
    /// Step index
    pub step_idx: u32,
    /// Selected adapter stable IDs
    pub selected_stable_ids: Vec<u64>,
    /// Q15 gate scores for selected adapters
    pub gates_q15: Vec<i16>,
    /// Decision hash (BLAKE3)
    pub decision_hash: Option<String>,
}

/// Get router step events (ksparse_selected events) for a run.
///
/// These events contain the deterministic routing decisions at each step.
pub async fn get_router_step_events(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
) -> Result<Vec<RouterStepEvent>> {
    // Verify run belongs to tenant
    let run_exists: Option<(i64,)> =
        sqlx::query_as(r#"SELECT 1 FROM diag_runs WHERE id = ? AND tenant_id = ?"#)
            .bind(run_id)
            .bind(tenant_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AosError::Database(format!("verify run ownership: {}", e)))?;

    if run_exists.is_none() {
        return Ok(vec![]);
    }

    // Fetch ksparse_selected events (contain router decisions)
    let events = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT payload_json
        FROM diag_events
        WHERE run_id = ? AND tenant_id = ? AND event_type = 'ksparse_selected'
        ORDER BY seq ASC
        "#,
    )
    .bind(run_id)
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_router_step_events: {}", e)))?;

    let mut steps = Vec::with_capacity(events.len());
    for (payload_json,) in events {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&payload_json) {
            let step_idx = payload
                .get("step_idx")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            let selected_stable_ids: Vec<u64> = payload
                .get("selected_stable_ids")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
                .unwrap_or_default();

            let gates_q15: Vec<i16> = payload
                .get("gates_q15")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_i64().map(|n| n as i16))
                        .collect()
                })
                .unwrap_or_default();

            let decision_hash = payload
                .get("decision_hash")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            steps.push(RouterStepEvent {
                step_idx,
                selected_stable_ids,
                gates_q15,
                decision_hash,
            });
        }
    }

    Ok(steps)
}

/// SQLite implementation of DiagPersister.
///
/// Wraps a connection pool and implements the persistence trait.
pub struct SqliteDiagPersister {
    pool: SqlitePool,
}

impl SqliteDiagPersister {
    /// Create a new persister with the given pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new persister wrapped in Arc.
    pub fn new_arc(pool: SqlitePool) -> Arc<Self> {
        Arc::new(Self::new(pool))
    }
}

#[async_trait::async_trait]
impl DiagPersister for SqliteDiagPersister {
    async fn persist_batch(
        &self,
        events: &[SequencedEvent],
    ) -> std::result::Result<usize, PersistError> {
        insert_diag_events_batch(&self.pool, events)
            .await
            .map_err(|e| PersistError::Database(e.to_string()))
    }

    async fn update_run_stats(
        &self,
        run_id: &str,
        events_added: u64,
    ) -> std::result::Result<(), PersistError> {
        update_run_event_count(&self.pool, run_id, events_added)
            .await
            .map_err(|e| PersistError::Database(e.to_string()))
    }
}

// Helper functions

fn severity_to_str(severity: &DiagSeverity) -> &'static str {
    match severity {
        DiagSeverity::Trace => "trace",
        DiagSeverity::Debug => "debug",
        DiagSeverity::Info => "info",
        DiagSeverity::Warn => "warn",
        DiagSeverity::Error => "error",
    }
}

fn extract_event_type(event: &DiagEvent) -> &'static str {
    match event {
        DiagEvent::RunStarted { .. } => "run_started",
        DiagEvent::RunFinished { .. } => "run_finished",
        DiagEvent::RunFailed { .. } => "run_failed",
        DiagEvent::StreamClosed { .. } => "stream_closed",
        DiagEvent::StageEnter { .. } => "stage_enter",
        DiagEvent::StageComplete { .. } => "stage_complete",
        DiagEvent::StageFailed { .. } => "stage_failed",
        DiagEvent::StageExit { .. } => "stage_exit",
        DiagEvent::AdapterResolved { .. } => "adapter_resolved",
        DiagEvent::RouterDecisionMade { .. } => "router_decision",
        DiagEvent::PolicyCheckResult { .. } => "policy_check",
        DiagEvent::WorkerSelected { .. } => "worker_selected",
        DiagEvent::InferenceTiming { .. } => "inference_timing",
        DiagEvent::RagContextRetrieved { .. } => "rag_context",
        DiagEvent::RoutingStart { .. } => "routing_start",
        DiagEvent::GateComputed { .. } => "gate_computed",
        DiagEvent::KsparseSelected { .. } => "ksparse_selected",
        DiagEvent::TieBreakApplied { .. } => "tie_break_applied",
        DiagEvent::RoutingEnd { .. } => "routing_end",
        DiagEvent::Custom { .. } => "custom",
    }
}

// ============================================================================
// Bundle Export Database Functions
// ============================================================================

/// Bundle export record.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DiagBundleExportRecord {
    pub id: String,
    pub tenant_id: String,
    pub run_id: String,
    pub trace_id: String,
    pub format: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub bundle_hash: String,
    pub merkle_root: String,
    pub signature: String,
    pub public_key: String,
    pub key_id: String,
    pub manifest_json: String,
    pub evidence_included: i64,
    pub request_hash: Option<String>,
    pub decision_chain_hash: Option<String>,
    pub backend_identity_hash: Option<String>,
    pub model_identity_hash: Option<String>,
    pub adapter_stack_ids: Option<String>,
    pub code_identity: Option<String>,
    pub status: String,
    pub expires_at: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Parameters for creating a bundle export record.
#[derive(Debug, Clone)]
pub struct CreateBundleExportParams {
    pub id: String,
    pub tenant_id: String,
    pub run_id: String,
    pub trace_id: String,
    pub format: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub bundle_hash: String,
    pub merkle_root: String,
    pub signature: String,
    pub public_key: String,
    pub key_id: String,
    pub manifest_json: String,
    pub evidence_included: bool,
    pub request_hash: Option<String>,
    pub decision_chain_hash: Option<String>,
    pub backend_identity_hash: Option<String>,
    pub model_identity_hash: Option<String>,
    pub adapter_stack_ids: Option<String>,
    pub code_identity: Option<String>,
    pub created_by: Option<String>,
    pub expires_at: Option<String>,
}

/// Insert a new bundle export record.
pub async fn insert_bundle_export(
    pool: &SqlitePool,
    params: &CreateBundleExportParams,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO diag_bundle_exports (
            id, tenant_id, run_id, trace_id, format, file_path, size_bytes,
            bundle_hash, merkle_root, signature, public_key, key_id, manifest_json,
            evidence_included, request_hash, decision_chain_hash, backend_identity_hash,
            model_identity_hash, adapter_stack_ids, code_identity, created_by, expires_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&params.id)
    .bind(&params.tenant_id)
    .bind(&params.run_id)
    .bind(&params.trace_id)
    .bind(&params.format)
    .bind(&params.file_path)
    .bind(params.size_bytes)
    .bind(&params.bundle_hash)
    .bind(&params.merkle_root)
    .bind(&params.signature)
    .bind(&params.public_key)
    .bind(&params.key_id)
    .bind(&params.manifest_json)
    .bind(if params.evidence_included { 1 } else { 0 })
    .bind(&params.request_hash)
    .bind(&params.decision_chain_hash)
    .bind(&params.backend_identity_hash)
    .bind(&params.model_identity_hash)
    .bind(&params.adapter_stack_ids)
    .bind(&params.code_identity)
    .bind(&params.created_by)
    .bind(&params.expires_at)
    .execute(pool)
    .await
    .map_err(|e| AosError::Database(format!("insert_bundle_export: {}", e)))?;

    Ok(())
}

/// Get a bundle export by ID with tenant isolation.
pub async fn get_bundle_export_by_id(
    pool: &SqlitePool,
    tenant_id: &str,
    export_id: &str,
) -> Result<Option<DiagBundleExportRecord>> {
    let row = sqlx::query_as::<_, DiagBundleExportRecord>(
        r#"
        SELECT id, tenant_id, run_id, trace_id, format, file_path, size_bytes,
               bundle_hash, merkle_root, signature, public_key, key_id, manifest_json,
               evidence_included, request_hash, decision_chain_hash, backend_identity_hash,
               model_identity_hash, adapter_stack_ids, code_identity, status, expires_at,
               created_by, created_at, updated_at
        FROM diag_bundle_exports
        WHERE tenant_id = ? AND id = ?
        "#,
    )
    .bind(tenant_id)
    .bind(export_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_bundle_export_by_id: {}", e)))?;

    Ok(row)
}

/// Get a bundle export by bundle hash (for verification).
pub async fn get_bundle_export_by_hash(
    pool: &SqlitePool,
    bundle_hash: &str,
) -> Result<Option<DiagBundleExportRecord>> {
    let row = sqlx::query_as::<_, DiagBundleExportRecord>(
        r#"
        SELECT id, tenant_id, run_id, trace_id, format, file_path, size_bytes,
               bundle_hash, merkle_root, signature, public_key, key_id, manifest_json,
               evidence_included, request_hash, decision_chain_hash, backend_identity_hash,
               model_identity_hash, adapter_stack_ids, code_identity, status, expires_at,
               created_by, created_at, updated_at
        FROM diag_bundle_exports
        WHERE bundle_hash = ?
        "#,
    )
    .bind(bundle_hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_bundle_export_by_hash: {}", e)))?;

    Ok(row)
}

/// List bundle exports for a tenant.
pub async fn list_bundle_exports_for_tenant(
    pool: &SqlitePool,
    tenant_id: &str,
    limit: u32,
) -> Result<Vec<DiagBundleExportRecord>> {
    let rows = sqlx::query_as::<_, DiagBundleExportRecord>(
        r#"
        SELECT id, tenant_id, run_id, trace_id, format, file_path, size_bytes,
               bundle_hash, merkle_root, signature, public_key, key_id, manifest_json,
               evidence_included, request_hash, decision_chain_hash, backend_identity_hash,
               model_identity_hash, adapter_stack_ids, code_identity, status, expires_at,
               created_by, created_at, updated_at
        FROM diag_bundle_exports
        WHERE tenant_id = ? AND status = 'completed'
        ORDER BY created_at DESC
        LIMIT ?
        "#,
    )
    .bind(tenant_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AosError::Database(format!("list_bundle_exports_for_tenant: {}", e)))?;

    Ok(rows)
}

/// List bundle exports for a specific run.
pub async fn list_bundle_exports_for_run(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
) -> Result<Vec<DiagBundleExportRecord>> {
    let rows = sqlx::query_as::<_, DiagBundleExportRecord>(
        r#"
        SELECT id, tenant_id, run_id, trace_id, format, file_path, size_bytes,
               bundle_hash, merkle_root, signature, public_key, key_id, manifest_json,
               evidence_included, request_hash, decision_chain_hash, backend_identity_hash,
               model_identity_hash, adapter_stack_ids, code_identity, status, expires_at,
               created_by, created_at, updated_at
        FROM diag_bundle_exports
        WHERE tenant_id = ? AND run_id = ? AND status = 'completed'
        ORDER BY created_at DESC
        "#,
    )
    .bind(tenant_id)
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AosError::Database(format!("list_bundle_exports_for_run: {}", e)))?;

    Ok(rows)
}

/// Update bundle export status.
pub async fn update_bundle_export_status(
    pool: &SqlitePool,
    export_id: &str,
    status: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE diag_bundle_exports
        SET status = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(status)
    .bind(export_id)
    .execute(pool)
    .await
    .map_err(|e| AosError::Database(format!("update_bundle_export_status: {}", e)))?;

    Ok(())
}

/// Delete expired bundle exports and return their file paths for cleanup.
pub async fn delete_expired_bundle_exports(pool: &SqlitePool) -> Result<Vec<String>> {
    // First get the file paths
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT file_path FROM diag_bundle_exports
        WHERE expires_at IS NOT NULL AND expires_at < datetime('now')
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AosError::Database(format!("query expired exports: {}", e)))?;

    let file_paths: Vec<String> = rows.into_iter().map(|(p,)| p).collect();

    // Then delete the records
    sqlx::query(
        r#"
        DELETE FROM diag_bundle_exports
        WHERE expires_at IS NOT NULL AND expires_at < datetime('now')
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| AosError::Database(format!("delete_expired_bundle_exports: {}", e)))?;

    Ok(file_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_to_str() {
        assert_eq!(severity_to_str(&DiagSeverity::Trace), "trace");
        assert_eq!(severity_to_str(&DiagSeverity::Debug), "debug");
        assert_eq!(severity_to_str(&DiagSeverity::Info), "info");
        assert_eq!(severity_to_str(&DiagSeverity::Warn), "warn");
        assert_eq!(severity_to_str(&DiagSeverity::Error), "error");
    }

    #[test]
    fn test_extract_event_type() {
        use adapteros_core::B3Hash;
        use adapteros_diagnostics::DiagStage;

        assert_eq!(
            extract_event_type(&DiagEvent::StageEnter {
                stage: DiagStage::RequestValidation
            }),
            "stage_enter"
        );

        assert_eq!(
            extract_event_type(&DiagEvent::StageComplete {
                stage: DiagStage::RequestValidation,
                duration_us: 1000
            }),
            "stage_complete"
        );

        assert_eq!(
            extract_event_type(&DiagEvent::RouterDecisionMade {
                candidate_count: 5,
                selected_count: 2,
                decision_chain_hash: B3Hash::hash(b"test")
            }),
            "router_decision"
        );
    }

    /// Test tenant isolation - tenant A cannot see tenant B's runs
    #[tokio::test]
    async fn test_tenant_isolation_runs() {
        // Create in-memory test database
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create test pool");

        // Run the diagnostics table migration
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS diag_runs (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                trace_id TEXT NOT NULL,
                started_at_unix_ms INTEGER NOT NULL,
                completed_at_unix_ms INTEGER,
                request_hash TEXT NOT NULL DEFAULT '',
                manifest_hash TEXT,
                decision_chain_hash TEXT,
                backend_identity_hash TEXT,
                model_identity_hash TEXT,
                status TEXT NOT NULL DEFAULT 'running',
                total_events_count INTEGER NOT NULL DEFAULT 0,
                dropped_events_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create table");

        // Insert runs for tenant_a and tenant_b
        insert_diag_run(
            &pool, "run_a1", "tenant_a", "trace_a1", 1000, "hash_a1", None,
        )
        .await
        .expect("insert run_a1");
        insert_diag_run(
            &pool, "run_a2", "tenant_a", "trace_a2", 2000, "hash_a2", None,
        )
        .await
        .expect("insert run_a2");
        insert_diag_run(
            &pool, "run_b1", "tenant_b", "trace_b1", 1500, "hash_b1", None,
        )
        .await
        .expect("insert run_b1");

        // Tenant A should only see their own runs
        let (runs_a, count_a) = list_diag_runs_paginated(&pool, "tenant_a", None, 100, None, None)
            .await
            .expect("list_diag_runs_paginated");
        assert_eq!(runs_a.len(), 2);
        assert_eq!(count_a, 2);

        // Tenant B should only see their own runs
        let (runs_b, count_b) = list_diag_runs_paginated(&pool, "tenant_b", None, 100, None, None)
            .await
            .expect("list_diag_runs_paginated");
        assert_eq!(runs_b.len(), 1);
        assert_eq!(count_b, 1);
        assert_eq!(runs_b[0].trace_id, "trace_b1");

        // Tenant A cannot access tenant B's run by trace_id
        let run_b_via_a = get_diag_run_by_trace_id(&pool, "tenant_a", "trace_b1")
            .await
            .expect("get_diag_run_by_trace_id");
        assert!(
            run_b_via_a.is_none(),
            "tenant_a should not see tenant_b's run"
        );

        // Tenant B can access their own run
        let run_b = get_diag_run_by_trace_id(&pool, "tenant_b", "trace_b1")
            .await
            .expect("get_diag_run_by_trace_id");
        assert!(run_b.is_some(), "tenant_b should see their own run");
    }

    /// Test pagination with since and limit parameters
    #[tokio::test]
    async fn test_pagination_limit_and_since() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create test pool");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS diag_runs (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                trace_id TEXT NOT NULL,
                started_at_unix_ms INTEGER NOT NULL,
                completed_at_unix_ms INTEGER,
                request_hash TEXT NOT NULL DEFAULT '',
                manifest_hash TEXT,
                decision_chain_hash TEXT,
                backend_identity_hash TEXT,
                model_identity_hash TEXT,
                status TEXT NOT NULL DEFAULT 'running',
                total_events_count INTEGER NOT NULL DEFAULT 0,
                dropped_events_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create table");

        // Insert 5 runs with different timestamps
        for i in 1..=5 {
            insert_diag_run(
                &pool,
                &format!("run_{}", i),
                "tenant_x",
                &format!("trace_{}", i),
                i * 1000,
                &format!("hash_{}", i),
                None,
            )
            .await
            .expect("insert run");
        }

        // Test limit
        let (runs, count) = list_diag_runs_paginated(&pool, "tenant_x", None, 2, None, None)
            .await
            .expect("list_diag_runs_paginated");
        assert_eq!(runs.len(), 2);
        assert_eq!(count, 5); // Total count should be 5

        // Test since filter (runs started after 2500ms)
        let (runs, count) =
            list_diag_runs_paginated(&pool, "tenant_x", Some(2500), 100, None, None)
                .await
                .expect("list_diag_runs_paginated");
        assert_eq!(runs.len(), 3); // runs 3, 4, 5 (3000, 4000, 5000)
        assert_eq!(count, 3);
    }

    /// Test status filter
    #[tokio::test]
    async fn test_status_filter() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create test pool");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS diag_runs (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                trace_id TEXT NOT NULL,
                started_at_unix_ms INTEGER NOT NULL,
                completed_at_unix_ms INTEGER,
                request_hash TEXT NOT NULL DEFAULT '',
                manifest_hash TEXT,
                decision_chain_hash TEXT,
                backend_identity_hash TEXT,
                model_identity_hash TEXT,
                status TEXT NOT NULL DEFAULT 'running',
                total_events_count INTEGER NOT NULL DEFAULT 0,
                dropped_events_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create table");

        // Insert runs with different statuses
        insert_diag_run(&pool, "run_1", "tenant_x", "trace_1", 1000, "hash_1", None)
            .await
            .expect("insert run");
        complete_diag_run(&pool, "run_1", "completed", 0)
            .await
            .expect("complete run");

        insert_diag_run(&pool, "run_2", "tenant_x", "trace_2", 2000, "hash_2", None)
            .await
            .expect("insert run");
        complete_diag_run(&pool, "run_2", "failed", 5)
            .await
            .expect("complete run");

        insert_diag_run(&pool, "run_3", "tenant_x", "trace_3", 3000, "hash_3", None)
            .await
            .expect("insert run");
        // run_3 stays in "running" status

        // Filter by completed status
        let (completed_runs, count) =
            list_diag_runs_paginated(&pool, "tenant_x", None, 100, None, Some("completed"))
                .await
                .expect("list_diag_runs_paginated");
        assert_eq!(completed_runs.len(), 1);
        assert_eq!(count, 1);
        assert_eq!(completed_runs[0].trace_id, "trace_1");

        // Filter by running status
        let (running_runs, _) =
            list_diag_runs_paginated(&pool, "tenant_x", None, 100, None, Some("running"))
                .await
                .expect("list_diag_runs_paginated");
        assert_eq!(running_runs.len(), 1);
        assert_eq!(running_runs[0].trace_id, "trace_3");
    }
}
