use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::{ApplyEventRequest, ApplyEventResponse, ErrorResponse};
use adapteros_core::AosError;
use adapteros_db::users::Role;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{Row, Sqlite, Transaction};
use std::collections::BTreeMap;
use thiserror::Error;
use tracing::{info, instrument};

const DEFAULT_TARGETS_JSON: &str = "[]";
const DEFAULT_ADAPTER_TIER: &str = "warm";
const DEFAULT_ADAPTER_STATE: &str = "unloaded";

#[derive(Debug, Error)]
pub enum EventApplierError {
    #[error("{event_type} event validation failed: {reason}")]
    Validation { event_type: String, reason: String },
    #[error("{event_type} event serialization failed: {reason}")]
    Serialization { event_type: String, reason: String },
    #[error("{event_type} event database error: {source}")]
    Database {
        event_type: String,
        #[source]
        source: sqlx::Error,
    },
}

impl From<EventApplierError> for AosError {
    fn from(err: EventApplierError) -> Self {
        match err {
            EventApplierError::Validation { reason, .. } => AosError::Validation(reason),
            EventApplierError::Serialization { reason, .. } => AosError::Serialization(
                serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, reason)),
            ),
            EventApplierError::Database { event_type, source } => {
                AosError::Database(format!("{}: {}", event_type, source))
            }
        }
    }
}

pub type EventResult<T> = std::result::Result<T, EventApplierError>;

pub trait EventClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Default)]
pub struct SystemClock;

impl EventClock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Clone, Copy)]
pub struct FixedClock {
    now: DateTime<Utc>,
}

impl FixedClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self { now }
    }
}

impl EventClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }
}

#[derive(Debug, Deserialize)]
pub struct TenantEvent {
    #[serde(flatten)]
    kind: TenantEventKind,
    #[serde(default)]
    identity: Option<EventIdentity>,
}

impl TenantEvent {
    pub fn event_type(&self) -> &'static str {
        self.kind.event_type()
    }

    pub fn identity_label(&self) -> Option<String> {
        self.identity.as_ref().map(EventIdentity::label)
    }

    pub fn kind(&self) -> &TenantEventKind {
        &self.kind
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EventIdentity {
    Str(String),
    Object(BTreeMap<String, Value>),
}

impl EventIdentity {
    fn label(&self) -> String {
        match self {
            EventIdentity::Str(id) => id.clone(),
            EventIdentity::Object(map) => map
                .get("event_id")
                .and_then(|v| v.as_str())
                .or_else(|| map.get("id").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| serde_json::to_string(map).unwrap_or_default()),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event_type", content = "metadata")]
pub enum TenantEventKind {
    #[serde(rename = "adapter.registered")]
    AdapterRegistered(AdapterRegistered),
    #[serde(rename = "stack.created")]
    StackCreated(StackCreated),
    #[serde(rename = "policy.updated")]
    PolicyUpdated(PolicyUpdated),
    #[serde(rename = "config.updated")]
    ConfigUpdated(ConfigUpdated),
    #[serde(rename = "plugin.config.updated")]
    PluginConfigUpdated(PluginConfigUpdated),
    #[serde(rename = "feature.flag.toggled")]
    FeatureFlagToggled(FeatureFlagToggled),
}

impl TenantEventKind {
    fn event_type(&self) -> &'static str {
        match self {
            TenantEventKind::AdapterRegistered(_) => "adapter.registered",
            TenantEventKind::StackCreated(_) => "stack.created",
            TenantEventKind::PolicyUpdated(_) => "policy.updated",
            TenantEventKind::ConfigUpdated(_) => "config.updated",
            TenantEventKind::PluginConfigUpdated(_) => "plugin.config.updated",
            TenantEventKind::FeatureFlagToggled(_) => "feature.flag.toggled",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AdapterRegistered {
    pub id: String,
    pub name: Option<String>,
    pub rank: i32,
    pub version: Option<String>,
    pub hash_b3: String,
}

#[derive(Debug, Deserialize)]
pub struct StackCreated {
    pub name: String,
    pub adapter_ids: Vec<String>,
    pub workflow_type: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyUpdated {
    pub name: String,
    #[serde(default)]
    pub rules: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ConfigUpdated {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Deserialize)]
pub struct PluginConfigUpdated {
    pub plugin: String,
    pub config_key: String,
    pub value: Value,
}

#[derive(Debug, Deserialize)]
pub struct FeatureFlagToggled {
    pub flag: String,
    pub enabled: bool,
}

pub fn parse_event(event: &Value) -> EventResult<TenantEvent> {
    let event_type = event
        .get("event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("<missing>")
        .to_string();

    serde_json::from_value(event.clone()).map_err(|err| EventApplierError::Validation {
        event_type,
        reason: err.to_string(),
    })
}

#[instrument(
    skip_all,
    fields(
        tenant_id = tenant_id,
        event_type = event.event_type(),
        event_identity = event
            .identity_label()
            .as_deref()
            .unwrap_or(""),
    )
)]
pub async fn apply_event_with_clock<C: EventClock>(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    event: &TenantEvent,
    clock: &C,
) -> EventResult<()> {
    let timestamp = format_timestamp(clock.now());

    match event.kind() {
        TenantEventKind::AdapterRegistered(payload) => {
            apply_adapter_registered(tx, tenant_id, payload, &timestamp).await?
        }
        TenantEventKind::StackCreated(payload) => {
            apply_stack_created(tx, tenant_id, payload, &timestamp).await?
        }
        TenantEventKind::PolicyUpdated(payload) => {
            apply_policy_updated(tx, tenant_id, payload, &timestamp).await?
        }
        TenantEventKind::ConfigUpdated(payload) => {
            upsert_tenant_config(tx, tenant_id, &payload.key, &payload.value, &timestamp).await?
        }
        TenantEventKind::PluginConfigUpdated(payload) => {
            let key = format!("plugin.{}.{}", payload.plugin, payload.config_key);
            upsert_tenant_config(tx, tenant_id, &key, &payload.value, &timestamp).await?
        }
        TenantEventKind::FeatureFlagToggled(payload) => {
            let key = format!("flag.{}", payload.flag);
            let value = Value::Bool(payload.enabled);
            upsert_tenant_config(tx, tenant_id, &key, &value, &timestamp).await?
        }
    }

    Ok(())
}

pub async fn apply_event(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    event: &TenantEvent,
) -> EventResult<()> {
    apply_event_with_clock(tx, tenant_id, event, &SystemClock).await
}

fn validation_error(event_type: &str, reason: impl Into<String>) -> EventApplierError {
    EventApplierError::Validation {
        event_type: event_type.to_string(),
        reason: reason.into(),
    }
}

fn format_timestamp(now: DateTime<Utc>) -> String {
    now.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string()
}

async fn apply_adapter_registered(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    payload: &AdapterRegistered,
    timestamp: &str,
) -> EventResult<()> {
    let event_type = "adapter.registered";
    let adapter_id = payload.id.trim();
    if adapter_id.is_empty() {
        return Err(validation_error(event_type, "adapter id cannot be empty"));
    }

    let name = payload
        .name
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .unwrap_or(adapter_id)
        .to_string();
    let version = payload
        .version
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .unwrap_or("0.0")
        .to_string();

    let alpha = (payload.rank as f64) * 2.0;

    sqlx::query(
        r#"
        INSERT INTO adapters (
            id, tenant_id, adapter_id, name, rank, version, hash_b3, current_state, tier,
            created_at, updated_at, alpha, targets_json, active
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name=excluded.name,
            rank=excluded.rank,
            version=excluded.version,
            hash_b3=excluded.hash_b3,
            current_state=excluded.current_state,
            tier=excluded.tier,
            updated_at=excluded.updated_at
        "#,
    )
    .bind(adapter_id)
    .bind(tenant_id)
    .bind(adapter_id)
    .bind(name)
    .bind(payload.rank)
    .bind(version)
    .bind(payload.hash_b3.trim())
    .bind(DEFAULT_ADAPTER_STATE)
    .bind(DEFAULT_ADAPTER_TIER)
    .bind(timestamp)
    .bind(timestamp)
    .bind(alpha)
    .bind(DEFAULT_TARGETS_JSON)
    .bind(1)
    .execute(tx.as_mut())
    .await
    .map_err(|source| EventApplierError::Database {
        event_type: event_type.to_string(),
        source,
    })?;

    Ok(())
}

async fn apply_stack_created(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    payload: &StackCreated,
    timestamp: &str,
) -> EventResult<()> {
    let event_type = "stack.created";
    let stack_id = payload
        .id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .map(|id| id.to_string())
        .unwrap_or_else(|| {
            crate::id_generator::readable_id(adapteros_id::IdPrefix::Stk, &payload.name)
        });

    let mut adapter_ids = payload.adapter_ids.clone();
    adapter_ids.sort();
    adapter_ids.dedup();

    let adapter_ids_json =
        serde_json::to_string(&adapter_ids).map_err(|err| EventApplierError::Serialization {
            event_type: event_type.to_string(),
            reason: err.to_string(),
        })?;

    sqlx::query(
        r#"
        INSERT INTO adapter_stacks (
            id, tenant_id, name, adapter_ids_json, workflow_type, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name=excluded.name,
            adapter_ids_json=excluded.adapter_ids_json,
            workflow_type=excluded.workflow_type,
            updated_at=excluded.updated_at
        "#,
    )
    .bind(stack_id)
    .bind(tenant_id)
    .bind(&payload.name)
    .bind(adapter_ids_json)
    .bind(&payload.workflow_type)
    .bind(timestamp)
    .bind(timestamp)
    .execute(tx.as_mut())
    .await
    .map_err(|source| EventApplierError::Database {
        event_type: event_type.to_string(),
        source,
    })?;

    Ok(())
}

async fn apply_policy_updated(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    payload: &PolicyUpdated,
    timestamp: &str,
) -> EventResult<()> {
    let event_type = "policy.updated";
    let rules_json =
        serde_json::to_string(&payload.rules).map_err(|err| EventApplierError::Serialization {
            event_type: event_type.to_string(),
            reason: err.to_string(),
        })?;

    sqlx::query(
        r#"
        INSERT INTO router_policies (tenant_id, name, rules_json, updated_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(tenant_id, name) DO UPDATE SET
            rules_json=excluded.rules_json,
            updated_at=excluded.updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(&payload.name)
    .bind(rules_json)
    .bind(timestamp)
    .execute(tx.as_mut())
    .await
    .map_err(|source| EventApplierError::Database {
        event_type: event_type.to_string(),
        source,
    })?;

    Ok(())
}

async fn upsert_tenant_config(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    key: &str,
    value: &Value,
    timestamp: &str,
) -> EventResult<()> {
    let event_type = "tenant.config";
    let value_json =
        serde_json::to_string(value).map_err(|err| EventApplierError::Serialization {
            event_type: event_type.to_string(),
            reason: err.to_string(),
        })?;

    sqlx::query(
        r#"
        INSERT INTO tenant_configs (tenant_id, key, value_json, updated_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(tenant_id, key) DO UPDATE SET
            value_json=excluded.value_json,
            updated_at=excluded.updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(key)
    .bind(value_json)
    .bind(timestamp)
    .execute(tx.as_mut())
    .await
    .map_err(|source| EventApplierError::Database {
        event_type: event_type.to_string(),
        source,
    })?;

    Ok(())
}

// =============================================================================
// HTTP Handler
// =============================================================================

/// Apply a single event to a tenant's state.
///
/// This endpoint allows applying configuration events to modify tenant state
/// deterministically. Events are recorded in the audit log for compliance.
///
/// Supported event types:
/// - `adapter.registered` - Register a new adapter
/// - `stack.created` - Create an adapter stack
/// - `policy.updated` - Update a policy
/// - `config.updated` - Update tenant configuration
/// - `plugin.config.updated` - Update plugin configuration
/// - `feature.flag.toggled` - Toggle a feature flag
#[utoipa::path(
    tag = "tenants",
    post,
    path = "/v1/tenants/{tenant_id}/events",
    request_body = ApplyEventRequest,
    responses(
        (status = 200, description = "Event applied successfully", body = ApplyEventResponse),
        (status = 400, description = "Invalid event format", body = ErrorResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - insufficient permissions", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    params(
        ("tenant_id" = String, Path, description = "Tenant ID to apply event to")
    ),
    security(("bearer_token" = []))
)]
pub async fn apply_tenant_event(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<ApplyEventRequest>,
) -> Result<Json<ApplyEventResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require Admin or Operator role for event application
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;
    let tenant_id = crate::id_resolver::resolve_any_id(&state.db, &tenant_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Validate tenant isolation - users can only apply events to their own tenant
    if claims.tenant_id != tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("cannot apply events to a different tenant")
                    .with_code("TENANT_MISMATCH"),
            ),
        ));
    }

    // Build the event value for parsing
    let event_value = serde_json::json!({
        "event_type": req.event_type,
        "metadata": req.metadata,
        "identity": req.identity,
    });

    // Parse the event
    let event = parse_event(&event_value).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid event format")
                    .with_code("INVALID_EVENT")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Apply the event within a transaction
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to start transaction")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let clock = SystemClock;
    let applied_at = clock.now();

    apply_event_with_clock(&mut tx, &tenant_id, &event, &clock)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("failed to apply event")
                        .with_code("EVENT_APPLICATION_FAILED")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to commit transaction")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    info!(
        tenant_id = %tenant_id,
        event_type = %req.event_type,
        user = %claims.sub,
        "Applied tenant event"
    );

    Ok(Json(ApplyEventResponse {
        success: true,
        event_type: event.event_type().to_string(),
        applied_at: applied_at.to_rfc3339(),
        identity_label: event.identity_label(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn fixed_clock() -> FixedClock {
        let now = DateTime::parse_from_rfc3339("2025-01-02T03:04:05Z")
            .unwrap()
            .with_timezone(&Utc);
        FixedClock::new(now)
    }

    async fn setup_db() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE adapters (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                adapter_id TEXT NOT NULL,
                name TEXT NOT NULL,
                rank INTEGER NOT NULL,
                version TEXT,
                hash_b3 TEXT NOT NULL,
                current_state TEXT,
                tier TEXT,
                created_at TEXT,
                updated_at TEXT,
                alpha REAL NOT NULL,
                targets_json TEXT NOT NULL,
                active INTEGER NOT NULL,
                UNIQUE(tenant_id, adapter_id)
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE adapter_stacks (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                name TEXT NOT NULL,
                adapter_ids_json TEXT NOT NULL,
                workflow_type TEXT,
                created_at TEXT,
                updated_at TEXT
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE router_policies (
                tenant_id TEXT NOT NULL,
                name TEXT NOT NULL,
                rules_json TEXT NOT NULL,
                updated_at TEXT,
                PRIMARY KEY (tenant_id, name)
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE tenant_configs (
                tenant_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value_json TEXT NOT NULL,
                updated_at TEXT,
                PRIMARY KEY (tenant_id, key)
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn applies_adapter_registration_with_fixed_clock() {
        let pool = setup_db().await;
        let mut tx = pool.begin().await.unwrap();
        let event = TenantEvent {
            kind: TenantEventKind::AdapterRegistered(AdapterRegistered {
                id: "adapter-1".into(),
                name: Some("Adapter One".into()),
                rank: 3,
                version: None,
                hash_b3: "b3hash".into(),
            }),
            identity: None,
        };

        apply_event_with_clock(&mut tx, "tenant-a", &event, &fixed_clock())
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let row = sqlx::query(
            r#"SELECT name, version, current_state, tier, alpha, targets_json, created_at, updated_at
               FROM adapters WHERE tenant_id = ? AND adapter_id = ?"#,
        )
        .bind("tenant-a")
        .bind("adapter-1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(row.get::<String, _>("name"), "Adapter One");
        assert_eq!(
            row.get::<Option<String>, _>("version"),
            Some("0.0".to_string())
        );
        assert_eq!(
            row.get::<Option<String>, _>("current_state"),
            Some(DEFAULT_ADAPTER_STATE.to_string())
        );
        assert_eq!(
            row.get::<Option<String>, _>("tier"),
            Some(DEFAULT_ADAPTER_TIER.to_string())
        );
        assert!((row.get::<f64, _>("alpha") - 6.0).abs() < f64::EPSILON);
        assert_eq!(
            row.get::<Option<String>, _>("targets_json"),
            Some(DEFAULT_TARGETS_JSON.to_string())
        );
        assert_eq!(row.get::<String, _>("created_at"), "2025-01-02 03:04:05");
        assert_eq!(row.get::<String, _>("updated_at"), "2025-01-02 03:04:05");
    }

    #[tokio::test]
    async fn applies_stack_creation_with_sorted_ids() {
        let pool = setup_db().await;
        let mut tx = pool.begin().await.unwrap();

        let event = TenantEvent {
            kind: TenantEventKind::StackCreated(StackCreated {
                name: "stack-a".into(),
                adapter_ids: vec!["b".into(), "a".into(), "a".into()],
                workflow_type: Some("Parallel".into()),
                id: Some("stack-1".into()),
            }),
            identity: None,
        };

        apply_event_with_clock(&mut tx, "tenant-a", &event, &fixed_clock())
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let row = sqlx::query(
            r#"SELECT adapter_ids_json, workflow_type, created_at, updated_at
               FROM adapter_stacks WHERE id = ?"#,
        )
        .bind("stack-1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(row.get::<String, _>("adapter_ids_json"), "[\"a\",\"b\"]");
        assert_eq!(
            row.get::<Option<String>, _>("workflow_type"),
            Some("Parallel".to_string())
        );
        assert_eq!(row.get::<String, _>("created_at"), "2025-01-02 03:04:05");
        assert_eq!(row.get::<String, _>("updated_at"), "2025-01-02 03:04:05");
    }

    #[tokio::test]
    async fn upserts_configs_and_flags() {
        let pool = setup_db().await;
        let mut tx = pool.begin().await.unwrap();

        let config_event = TenantEvent {
            kind: TenantEventKind::ConfigUpdated(ConfigUpdated {
                key: "max_sessions".into(),
                value: Value::from(10),
            }),
            identity: None,
        };

        let flag_event = TenantEvent {
            kind: TenantEventKind::FeatureFlagToggled(FeatureFlagToggled {
                flag: "beta".into(),
                enabled: true,
            }),
            identity: None,
        };

        apply_event_with_clock(&mut tx, "tenant-a", &config_event, &fixed_clock())
            .await
            .unwrap();
        apply_event_with_clock(&mut tx, "tenant-a", &flag_event, &fixed_clock())
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let rows = sqlx::query(
            r#"SELECT key, value_json, updated_at FROM tenant_configs WHERE tenant_id = ? ORDER BY key"#,
        )
        .bind("tenant-a")
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get::<String, _>("key"), "flag.beta");
        assert_eq!(rows[0].get::<String, _>("value_json"), "true");
        assert_eq!(
            rows[0].get::<Option<String>, _>("updated_at"),
            Some("2025-01-02 03:04:05".to_string())
        );
        assert_eq!(rows[1].get::<String, _>("key"), "max_sessions");
        assert_eq!(rows[1].get::<String, _>("value_json"), "10");
    }

    #[tokio::test]
    async fn rejects_missing_adapter_id() {
        let pool = setup_db().await;
        let mut tx = pool.begin().await.unwrap();
        let event = TenantEvent {
            kind: TenantEventKind::AdapterRegistered(AdapterRegistered {
                id: " ".into(),
                name: None,
                rank: 1,
                version: None,
                hash_b3: "hash".into(),
            }),
            identity: None,
        };

        let result = apply_event_with_clock(&mut tx, "tenant-a", &event, &fixed_clock()).await;
        assert!(matches!(result, Err(EventApplierError::Validation { .. })));

        // Rollback transaction to release the connection before querying the pool directly
        tx.rollback().await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM adapters")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
