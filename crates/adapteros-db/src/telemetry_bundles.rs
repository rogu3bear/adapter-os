use crate::telemetry_kv::record_telemetry_drift;
use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use tracing::warn;
use uuid::Uuid;

/// Builder for creating telemetry batch parameters
#[derive(Debug, Default)]
pub struct TelemetryBatchBuilder {
    tenant_id: Option<String>,
    event_type: Option<String>,
    event_data: Option<Value>,
    timestamp: Option<String>,
    source: Option<String>,
    user_id: Option<String>,
    session_id: Option<String>,
    metadata: Option<Value>,
    tags: Option<Value>,
    priority: Option<String>,
}

/// Parameters for telemetry batch recording
#[derive(Debug, Clone)]
pub struct TelemetryBatchParams {
    pub tenant_id: String,
    pub event_type: String,
    pub event_data: Value,
    pub timestamp: String,
    pub source: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub metadata: Option<Value>,
    pub tags: Option<Value>,
    pub priority: Option<String>,
}

/// Telemetry record from database
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TelemetryRecord {
    pub id: String,
    pub tenant_id: String,
    pub event_type: String,
    pub event_data: String,
    pub timestamp: String,
    pub source: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub metadata: Option<String>,
    pub tags: Option<String>,
    pub priority: Option<String>,
    pub created_at: String,
}

/// Telemetry bundle from database
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TelemetryBundle {
    pub id: String,
    pub tenant_id: String,
    pub cpid: String,
    pub path: String,
    pub merkle_root_b3: String,
    pub start_seq: i64,
    pub end_seq: i64,
    pub event_count: i64,
    pub created_at: String,
}

impl TelemetryBatchBuilder {
    /// Create a new telemetry batch builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tenant ID (required)
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the event type (required)
    pub fn event_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into());
        self
    }

    /// Set the event data as JSON (required)
    pub fn event_data(mut self, event_data: Value) -> Self {
        self.event_data = Some(event_data);
        self
    }

    /// Set the timestamp (required)
    pub fn timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.timestamp = Some(timestamp.into());
        self
    }

    /// Set the source (optional)
    pub fn source(mut self, source: Option<impl Into<String>>) -> Self {
        self.source = source.map(|s| s.into());
        self
    }

    /// Set the user ID (optional)
    pub fn user_id(mut self, user_id: Option<impl Into<String>>) -> Self {
        self.user_id = user_id.map(|s| s.into());
        self
    }

    /// Set the session ID (optional)
    pub fn session_id(mut self, session_id: Option<impl Into<String>>) -> Self {
        self.session_id = session_id.map(|s| s.into());
        self
    }

    /// Set the metadata as JSON (optional)
    pub fn metadata(mut self, metadata: Option<Value>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the tags as JSON array (optional)
    pub fn tags(mut self, tags: Option<Value>) -> Self {
        self.tags = tags;
        self
    }

    /// Set the priority level (optional)
    pub fn priority(mut self, priority: Option<impl Into<String>>) -> Self {
        self.priority = priority.map(|s| s.into());
        self
    }

    /// Build the telemetry batch parameters
    pub fn build(self) -> Result<TelemetryBatchParams> {
        Ok(TelemetryBatchParams {
            tenant_id: self
                .tenant_id
                .ok_or_else(|| AosError::Validation("tenant_id is required".into()))?,
            event_type: self
                .event_type
                .ok_or_else(|| AosError::Validation("event_type is required".into()))?,
            event_data: self
                .event_data
                .ok_or_else(|| AosError::Validation("event_data is required".into()))?,
            timestamp: self
                .timestamp
                .ok_or_else(|| AosError::Validation("timestamp is required".into()))?,
            source: self.source,
            user_id: self.user_id,
            session_id: self.session_id,
            metadata: self.metadata,
            tags: self.tags,
            priority: self.priority,
        })
    }
}

impl Db {
    /// Record a batch of telemetry events
    ///
    /// Use [`TelemetryBatchBuilder`] to construct complex parameter sets:
    /// ```no_run
    /// use adapteros_db::telemetry_bundles::TelemetryBatchBuilder;
    /// use serde_json::json;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let params = TelemetryBatchBuilder::new()
    ///     .tenant_id("tenant-123")
    ///     .event_type("user_action")
    ///     .event_data(json!({"action": "click", "element": "button"}))
    ///     .timestamp("2025-10-31T12:00:00Z")
    ///     .source(Some("web_app"))
    ///     .user_id(Some("user-456"))
    ///     .session_id(Some("session-789"))
    ///     .metadata(Some(json!({"user_agent": "Chrome", "ip": "192.168.1.1"})))
    ///     .tags(Some(json!(["tag1", "tag2"])))
    ///     .priority(Some("normal"))
    ///     .build()
    ///     .expect("required fields");
    ///
    /// db.record_telemetry_batch(params).await.expect("telemetry recorded");
    /// # }
    /// ```
    pub async fn record_telemetry_batch(&self, params: TelemetryBatchParams) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let event_data_json = serde_json::to_string(&params.event_data)
            .map_err(|e| AosError::Validation(format!("Failed to serialize event_data: {}", e)))?;
        let metadata_json = params.metadata.as_ref().map(|m| m.to_string());
        let tags_json = params.tags.as_ref().map(|t| t.to_string());

        sqlx::query(
            r#"
            INSERT INTO telemetry_events (
                id, tenant_id, event_type, event_data, timestamp, source,
                user_id, session_id, metadata, tags, priority
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&params.tenant_id)
        .bind(&params.event_type)
        .bind(&event_data_json)
        .bind(&params.timestamp)
        .bind(&params.source)
        .bind(&params.user_id)
        .bind(&params.session_id)
        .bind(&metadata_json)
        .bind(&tags_json)
        .bind(&params.priority)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to record telemetry batch: {}", e)))?;

        // Dual-write to KV when enabled
        if let Some(repo) = self.telemetry_repo_if_write() {
            let kv_event = self.kv_event_from_params(&id, &params);
            match repo.store_event(kv_event.clone()).await {
                Ok(_) => {
                    // Diff check for drift detection
                    if let Ok(Some(fetched)) =
                        repo.get_event(&kv_event.tenant_id, &kv_event.seq).await
                    {
                        let kv_event_data_json =
                            serde_json::to_string(&fetched.event_data).unwrap_or_default();
                        if kv_event_data_json != event_data_json {
                            record_telemetry_drift("telemetry_event_payload_mismatch");
                        }
                    }
                }
                Err(e) => {
                    self.record_kv_write_fallback("telemetry.events.dual_write");
                    warn!(
                        error = %e,
                        tenant_id = %params.tenant_id,
                        "Failed to dual-write telemetry event to KV"
                    );
                }
            }
        }

        Ok(id)
    }

    /// Get telemetry records by tenant
    pub async fn get_telemetry_by_tenant(
        &self,
        tenant_id: &str,
        limit: i32,
    ) -> Result<Vec<TelemetryRecord>> {
        let limit = limit.max(0);
        // KV-primary read path
        if let Some(repo) = self.telemetry_repo_if_read() {
            match repo.list_events_by_tenant(tenant_id, limit as usize).await {
                Ok(events) => {
                    let mut converted = Vec::new();
                    for ev in events {
                        converted.push(Db::kv_event_to_record(ev)?);
                    }
                    if self.storage_mode().is_dual_write() && self.storage_mode().read_from_sql() {
                        if let Some(pool) = self.pool_opt() {
                            let sql_records = sqlx::query_as::<_, TelemetryRecord>(
                                r#"
                                SELECT id, tenant_id, event_type, event_data, timestamp, source,
                                       user_id, session_id, metadata, tags, priority, created_at
                                FROM telemetry_events
                                WHERE tenant_id = ?
                                ORDER BY timestamp DESC, id DESC
                                LIMIT ?
                                "#,
                            )
                            .bind(tenant_id)
                            .bind(limit)
                            .fetch_all(pool)
                            .await
                            .map_err(|e| {
                                AosError::Database(format!(
                                    "Failed to get telemetry by tenant: {}",
                                    e
                                ))
                            })?;

                            if sql_records.len() != converted.len()
                                || sql_records.iter().zip(converted.iter()).any(|(sql, kv)| {
                                    sql.id != kv.id || sql.event_data != kv.event_data
                                })
                            {
                                record_telemetry_drift("telemetry_events_drift_dual_write");
                            }
                        }
                    }
                    return Ok(converted);
                }
                Err(e) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Err(AosError::Database(format!(
                            "KV read failed for telemetry: {}",
                            e
                        )));
                    }
                    self.record_kv_read_fallback("telemetry.events.list.fallback");
                    warn!(
                        tenant_id = %tenant_id,
                        error = %e,
                        "KV telemetry read failed, falling back to SQL"
                    );
                }
            }
        }

        let records = sqlx::query_as::<_, TelemetryRecord>(
            r#"
            SELECT id, tenant_id, event_type, event_data, timestamp, source,
                   user_id, session_id, metadata, tags, priority, created_at
            FROM telemetry_events
            WHERE tenant_id = ?
            ORDER BY timestamp DESC, id DESC
            LIMIT ?
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get telemetry by tenant: {}", e)))?;

        Ok(records)
    }

    /// Get telemetry records by event type
    pub async fn get_telemetry_by_event_type(
        &self,
        tenant_id: &str,
        event_type: &str,
        limit: i32,
    ) -> Result<Vec<TelemetryRecord>> {
        let limit = limit.max(0);
        if let Some(repo) = self.telemetry_repo_if_read() {
            match repo
                .list_events_by_type(tenant_id, event_type, limit as usize)
                .await
            {
                Ok(events) => {
                    let mut converted = Vec::new();
                    for ev in events {
                        converted.push(Db::kv_event_to_record(ev)?);
                    }
                    if self.storage_mode().is_dual_write() && self.storage_mode().read_from_sql() {
                        if let Some(pool) = self.pool_opt() {
                            let sql_records = sqlx::query_as::<_, TelemetryRecord>(
                                r#"
                                SELECT id, tenant_id, event_type, event_data, timestamp, source,
                                       user_id, session_id, metadata, tags, priority, created_at
                                FROM telemetry_events
                                WHERE tenant_id = ? AND event_type = ?
                                ORDER BY timestamp DESC, id DESC
                                LIMIT ?
                                "#,
                            )
                            .bind(tenant_id)
                            .bind(event_type)
                            .bind(limit)
                            .fetch_all(pool)
                            .await
                            .map_err(|e| {
                                AosError::Database(format!(
                                    "Failed to get telemetry by event type: {}",
                                    e
                                ))
                            })?;

                            if sql_records.len() != converted.len()
                                || sql_records.iter().zip(converted.iter()).any(|(sql, kv)| {
                                    sql.id != kv.id || sql.event_data != kv.event_data
                                })
                            {
                                record_telemetry_drift("telemetry_events_drift_dual_write");
                            }
                        }
                    }
                    return Ok(converted);
                }
                Err(e) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Err(AosError::Database(format!(
                            "KV read failed for telemetry by type: {}",
                            e
                        )));
                    }
                    self.record_kv_read_fallback("telemetry.events.list_by_type.fallback");
                    warn!(
                        tenant_id = %tenant_id,
                        event_type = %event_type,
                        error = %e,
                        "KV telemetry read failed, falling back to SQL"
                    );
                }
            }
        }

        let records = sqlx::query_as::<_, TelemetryRecord>(
            r#"
            SELECT id, tenant_id, event_type, event_data, timestamp, source,
                   user_id, session_id, metadata, tags, priority, created_at
            FROM telemetry_events
            WHERE tenant_id = ? AND event_type = ?
            ORDER BY timestamp DESC, id DESC
            LIMIT ?
            "#,
        )
        .bind(tenant_id)
        .bind(event_type)
        .bind(limit)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get telemetry by event type: {}", e)))?;

        Ok(records)
    }

    /// Get telemetry bundles by tenant
    pub async fn get_telemetry_bundles_by_tenant(
        &self,
        tenant_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<TelemetryBundle>> {
        let limit = limit.max(0);
        let offset = offset.max(0);
        if let Some(repo) = self.telemetry_repo_if_read() {
            match repo
                .list_bundles_by_tenant(tenant_id, limit as usize, offset as usize)
                .await
            {
                Ok(bundles) => {
                    let converted: Vec<TelemetryBundle> =
                        bundles.into_iter().map(Db::kv_bundle_to_record).collect();

                    if self.storage_mode().is_dual_write() && self.storage_mode().read_from_sql() {
                        if let Some(pool) = self.pool_opt() {
                            let sql_bundles = sqlx::query_as::<_, TelemetryBundle>(
                                r#"
                                SELECT id, tenant_id, cpid, path, merkle_root_b3, start_seq, end_seq, event_count, created_at
                                FROM telemetry_bundles
                                WHERE tenant_id = ?
                                ORDER BY created_at DESC, id DESC
                                LIMIT ? OFFSET ?
                                "#,
                            )
                            .bind(tenant_id)
                            .bind(limit)
                            .bind(offset)
                            .fetch_all(pool)
                            .await
                            .map_err(|e| AosError::Database(format!("Failed to get telemetry bundles: {}", e)))?;

                            if sql_bundles.len() != converted.len()
                                || sql_bundles.iter().zip(converted.iter()).any(|(sql, kv)| {
                                    sql.id != kv.id || sql.merkle_root_b3 != kv.merkle_root_b3
                                })
                            {
                                record_telemetry_drift("telemetry_bundles_drift_dual_write");
                            }
                        }
                    }

                    return Ok(converted);
                }
                Err(e) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Err(AosError::Database(format!(
                            "KV read failed for telemetry bundles: {}",
                            e
                        )));
                    }
                    self.record_kv_read_fallback("telemetry.bundles.list.fallback");
                    warn!(
                        tenant_id = %tenant_id,
                        error = %e,
                        "KV telemetry bundles read failed, falling back to SQL"
                    );
                }
            }
        }

        let bundles = sqlx::query_as::<_, TelemetryBundle>(
            r#"
            SELECT id, tenant_id, cpid, path, merkle_root_b3, start_seq, end_seq, event_count, created_at
            FROM telemetry_bundles
            WHERE tenant_id = ?
            ORDER BY created_at DESC, id DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get telemetry bundles: {}", e)))?;

        Ok(bundles)
    }

    /// Get a single telemetry bundle by ID
    pub async fn get_telemetry_bundle(
        &self,
        tenant_id: &str,
        bundle_id: &str,
    ) -> Result<Option<TelemetryBundle>> {
        if let Some(repo) = self.telemetry_repo_if_read() {
            match repo.get_bundle_metadata(tenant_id, bundle_id).await {
                Ok(Some(bundle)) => {
                    let record = Db::kv_bundle_to_record(bundle);

                    if self.storage_mode().is_dual_write() && self.storage_mode().read_from_sql() {
                        if let Some(pool) = self.pool_opt() {
                            if let Ok(Some(sql_bundle)) = sqlx::query_as::<_, TelemetryBundle>(
                                r#"
                                SELECT id, tenant_id, cpid, path, merkle_root_b3, start_seq, end_seq, event_count, created_at
                                FROM telemetry_bundles
                                WHERE tenant_id = ? AND id = ?
                                "#,
                            )
                            .bind(tenant_id)
                            .bind(bundle_id)
                            .fetch_optional(pool)
                            .await
                            {
                                if sql_bundle.id != record.id
                                    || sql_bundle.merkle_root_b3 != record.merkle_root_b3
                                {
                                    record_telemetry_drift("telemetry_bundle_drift_dual_write");
                                }
                            }
                        }
                    }

                    return Ok(Some(record));
                }
                Ok(None) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Ok(None);
                    }
                }
                Err(e) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Err(AosError::Database(format!(
                            "KV read failed for telemetry bundle: {}",
                            e
                        )));
                    }
                    self.record_kv_read_fallback("telemetry.bundles.get.fallback");
                    warn!(
                        tenant_id = %tenant_id,
                        bundle_id = %bundle_id,
                        error = %e,
                        "KV telemetry bundle read failed, falling back to SQL"
                    );
                }
            }
        }

        let bundle = sqlx::query_as::<_, TelemetryBundle>(
            r#"
            SELECT id, tenant_id, cpid, path, merkle_root_b3, start_seq, end_seq, event_count, created_at
            FROM telemetry_bundles
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(bundle_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get telemetry bundle: {}", e)))?;

        Ok(bundle)
    }
}
