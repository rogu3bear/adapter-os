use crate::Db;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

/// Bundle metadata from database
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BundleMetadata {
    /// Bundle hash (BLAKE3 Merkle root)
    #[sqlx(rename = "merkle_root_b3")]
    pub bundle_hash: String,
    pub tenant_id: String,
    /// Sequence number (using end_seq from database)
    #[sqlx(rename = "end_seq")]
    pub sequence_no: i64,
    pub event_count: i32,
    pub prev_bundle_hash: Option<String>,
    pub schema_version: i32,
    pub created_at: String,
}

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
#[derive(Debug)]
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
                .ok_or_else(|| anyhow!("tenant_id is required"))?,
            event_type: self
                .event_type
                .ok_or_else(|| anyhow!("event_type is required"))?,
            event_data: self
                .event_data
                .ok_or_else(|| anyhow!("event_data is required"))?,
            timestamp: self
                .timestamp
                .ok_or_else(|| anyhow!("timestamp is required"))?,
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
        let event_data_json = serde_json::to_string(&params.event_data)?;
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
        .execute(self.pool())
        .await?;

        Ok(id)
    }

    /// Get telemetry records by tenant
    pub async fn get_telemetry_by_tenant(
        &self,
        tenant_id: &str,
        limit: i32,
    ) -> Result<Vec<TelemetryRecord>> {
        let records = sqlx::query_as::<_, TelemetryRecord>(
            r#"
            SELECT id, tenant_id, event_type, event_data, timestamp, source,
                   user_id, session_id, metadata, tags, priority, created_at
            FROM telemetry_events
            WHERE tenant_id = ?
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        Ok(records)
    }

    /// Get telemetry records by event type
    pub async fn get_telemetry_by_event_type(
        &self,
        event_type: &str,
        limit: i32,
    ) -> Result<Vec<TelemetryRecord>> {
        let records = sqlx::query_as::<_, TelemetryRecord>(
            r#"
            SELECT id, tenant_id, event_type, event_data, timestamp, source,
                   user_id, session_id, metadata, tags, priority, created_at
            FROM telemetry_events
            WHERE event_type = ?
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(event_type)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        Ok(records)
    }

    /// List all bundles for a tenant (ordered by sequence_no)
    ///
    /// Returns bundle metadata from the telemetry_bundles table.
    /// Used for hydration to find available bundles.
    ///
    /// # Arguments
    ///
    /// * `tenant_id` - Tenant identifier
    /// * `limit` - Maximum number of bundles to return
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> Result<(), Box<dyn std::error::Error>> {
    /// let bundles = db.list_bundles_for_tenant("tenant-a", 50).await?;
    /// for bundle in bundles {
    ///     println!("Bundle: {} (seq {})", bundle.bundle_hash, bundle.sequence_no);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_bundles_for_tenant(
        &self,
        tenant_id: &str,
        limit: i32,
    ) -> Result<Vec<BundleMetadata>> {
        let bundles = sqlx::query_as::<_, BundleMetadata>(
            r#"
            SELECT merkle_root_b3, tenant_id, end_seq, event_count,
                   prev_bundle_hash, COALESCE(schema_version, 1) as schema_version, created_at
            FROM telemetry_bundles
            WHERE tenant_id = ?
            ORDER BY end_seq DESC
            LIMIT ?
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        Ok(bundles)
    }

    /// Get bundle metadata by hash
    ///
    /// Returns bundle information for a specific content-addressed hash.
    ///
    /// # Arguments
    ///
    /// * `bundle_hash` - BLAKE3 hash of bundle (hex string)
    ///
    /// # Errors
    ///
    /// Returns error if bundle not found in database.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> Result<(), Box<dyn std::error::Error>> {
    /// let hash = "a3f2b1..."; // BLAKE3 hash
    /// let metadata = db.get_bundle_by_hash(hash).await?;
    /// println!("Bundle for tenant: {}", metadata.tenant_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_bundle_by_hash(&self, bundle_hash: &str) -> Result<BundleMetadata> {
        let bundle = sqlx::query_as::<_, BundleMetadata>(
            r#"
            SELECT merkle_root_b3, tenant_id, end_seq, event_count,
                   prev_bundle_hash, COALESCE(schema_version, 1) as schema_version, created_at
            FROM telemetry_bundles
            WHERE merkle_root_b3 = ?
            "#,
        )
        .bind(bundle_hash)
        .fetch_one(self.pool())
        .await?;

        Ok(bundle)
    }

    /// Get latest bundle for a tenant
    ///
    /// Returns the most recent bundle (highest sequence_no).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> Result<(), Box<dyn std::error::Error>> {
    /// let latest = db.get_latest_bundle("tenant-a").await?;
    /// println!("Latest bundle: {} (seq {})", latest.bundle_hash, latest.sequence_no);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_latest_bundle(&self, tenant_id: &str) -> Result<BundleMetadata> {
        let bundle = sqlx::query_as::<_, BundleMetadata>(
            r#"
            SELECT merkle_root_b3, tenant_id, end_seq, event_count,
                   prev_bundle_hash, COALESCE(schema_version, 1) as schema_version, created_at
            FROM telemetry_bundles
            WHERE tenant_id = ?
            ORDER BY end_seq DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await?;

        Ok(bundle)
    }

    /// Verify bundle chain integrity
    ///
    /// Checks that prev_bundle_hash links are valid for a tenant.
    ///
    /// # Returns
    ///
    /// `Ok(true)` if chain is valid, `Ok(false)` if broken.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> Result<(), Box<dyn std::error::Error>> {
    /// let is_valid = db.verify_bundle_chain("tenant-a").await?;
    /// if !is_valid {
    ///     eprintln!("Bundle chain integrity violated!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn verify_bundle_chain(&self, tenant_id: &str) -> Result<bool> {
        let bundles = self.list_bundles_for_tenant(tenant_id, 1000).await?;

        // Reverse to check from oldest to newest
        let mut bundles_asc = bundles;
        bundles_asc.reverse();

        for i in 1..bundles_asc.len() {
            let current = &bundles_asc[i];
            let prev = &bundles_asc[i - 1];

            // Check if prev_bundle_hash matches previous bundle
            if let Some(ref prev_hash) = current.prev_bundle_hash {
                if prev_hash != &prev.bundle_hash {
                    return Ok(false);
                }
            } else if i > 0 {
                // First bundle (seq 0) can have None, others must have prev_hash
                return Ok(false);
            }
        }

        Ok(true)
    }
}
