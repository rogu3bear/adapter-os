//! Storage mode and statistics API handlers
//!
//! Provides read-only visibility into the database storage backend configuration
//! and statistics comparing SQL vs KV record counts.
//!
//! Endpoints:
//! - GET /v1/storage/mode - Returns current storage mode
//! - GET /v1/storage/stats - Returns KV vs SQL record counts

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::users::Role;
use adapteros_db::KvBackend;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use tracing::{debug, warn};
use utoipa::ToSchema;

/// Storage mode response
#[derive(Debug, Serialize, ToSchema)]
pub struct StorageModeResponse {
    /// Current storage mode
    pub mode: String,
    /// Human-readable description of the mode
    pub description: String,
    /// Whether KV backend is available
    pub kv_available: bool,
    /// Whether dual-write is active
    pub dual_write_active: bool,
}

/// Storage statistics response
#[derive(Debug, Serialize, ToSchema)]
pub struct StorageStatsResponse {
    /// Current storage mode
    pub mode: String,
    /// SQL record counts by table
    pub sql_counts: TableCounts,
    /// KV record counts by namespace
    pub kv_counts: KvCounts,
    /// Timestamp when stats were collected
    pub collected_at: String,
}

/// Record counts for SQL tables
#[derive(Debug, Serialize, ToSchema)]
pub struct TableCounts {
    /// Number of adapters in SQL
    pub adapters: i64,
    /// Number of tenants in SQL
    pub tenants: i64,
    /// Number of users in SQL
    pub users: i64,
    /// Number of adapter stacks in SQL
    pub stacks: i64,
    /// Number of training datasets in SQL
    pub datasets: i64,
    /// Number of documents in SQL
    pub documents: i64,
}

/// Record counts for KV namespaces
#[derive(Debug, Serialize, ToSchema)]
pub struct KvCounts {
    /// Number of adapters in KV (if available)
    pub adapters: Option<i64>,
    /// Number of tenants in KV (if available)
    pub tenants: Option<i64>,
    /// Number of users in KV (if available)
    pub users: Option<i64>,
    /// Number of stacks in KV (if available)
    pub stacks: Option<i64>,
}

/// GET /v1/storage/mode - Get current storage mode
///
/// Returns the current database storage mode configuration, including
/// whether KV backend is available and whether dual-write is active.
///
/// This is a read-only administrative visibility endpoint.
#[utoipa::path(
    get,
    path = "/v1/storage/mode",
    responses(
        (status = 200, description = "Current storage mode", body = StorageModeResponse)
    ),
    tag = "storage",
    security(("bearer_token" = []))
)]
pub async fn get_storage_mode(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<StorageModeResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require admin role for storage visibility
    require_any_role(&claims, &[Role::Admin])?;

    debug!("Querying storage mode");

    let storage_mode = state.db.storage_mode();
    let kv_available = state.db.has_kv_backend();
    let dual_write_active = storage_mode.is_dual_write();

    let description = match storage_mode {
        adapteros_db::StorageMode::SqlOnly => {
            "SQL backend only (default production mode)".to_string()
        }
        adapteros_db::StorageMode::DualWrite => {
            "Write to both SQL and KV backends, read from SQL (migration validation)".to_string()
        }
        adapteros_db::StorageMode::KvPrimary => {
            "Write to both SQL and KV backends, read from KV (migration cutover)".to_string()
        }
        adapteros_db::StorageMode::KvOnly => "KV backend only (full migration)".to_string(),
    };

    Ok(Json(StorageModeResponse {
        mode: storage_mode.to_string(),
        description,
        kv_available,
        dual_write_active,
    }))
}

/// GET /v1/storage/stats - Get storage statistics
///
/// Returns record counts for both SQL and KV backends (if available),
/// allowing operators to verify data consistency during migration.
///
/// This is a read-only administrative visibility endpoint.
#[utoipa::path(
    get,
    path = "/v1/storage/stats",
    responses(
        (status = 200, description = "Storage statistics", body = StorageStatsResponse)
    ),
    tag = "storage",
    security(("bearer_token" = []))
)]
pub async fn get_storage_stats(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<StorageStatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require admin role for storage visibility
    require_any_role(&claims, &[Role::Admin])?;

    debug!("Querying storage statistics");

    let storage_mode = state.db.storage_mode();

    // Collect SQL counts
    let sql_counts = collect_sql_counts(&state).await?;

    // Collect KV counts if KV backend is available
    let kv_counts = if state.db.has_kv_backend() {
        collect_kv_counts(&state).await?
    } else {
        KvCounts {
            adapters: None,
            tenants: None,
            users: None,
            stacks: None,
        }
    };

    Ok(Json(StorageStatsResponse {
        mode: storage_mode.to_string(),
        sql_counts,
        kv_counts,
        collected_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Collect record counts from SQL backend
async fn collect_sql_counts(
    state: &AppState,
) -> Result<TableCounts, (StatusCode, Json<ErrorResponse>)> {
    use sqlx::Row;

    // Count adapters
    let adapters = sqlx::query("SELECT COUNT(*) as count FROM adapters")
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to count adapters");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to count adapters")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .try_get::<i64, _>("count")
        .unwrap_or(0);

    // Count tenants
    let tenants = sqlx::query("SELECT COUNT(*) as count FROM tenants")
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to count tenants");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to count tenants")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .try_get::<i64, _>("count")
        .unwrap_or(0);

    // Count users
    let users = sqlx::query("SELECT COUNT(*) as count FROM users")
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to count users");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to count users")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .try_get::<i64, _>("count")
        .unwrap_or(0);

    // Count stacks
    let stacks = sqlx::query("SELECT COUNT(*) as count FROM adapter_stacks")
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to count stacks");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to count stacks")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .try_get::<i64, _>("count")
        .unwrap_or(0);

    // Count datasets
    let datasets = sqlx::query("SELECT COUNT(*) as count FROM training_datasets")
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to count datasets");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to count datasets")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .try_get::<i64, _>("count")
        .unwrap_or(0);

    // Count documents
    let documents = sqlx::query("SELECT COUNT(*) as count FROM documents")
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to count documents");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to count documents")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .try_get::<i64, _>("count")
        .unwrap_or(0);

    Ok(TableCounts {
        adapters,
        tenants,
        users,
        stacks,
        datasets,
        documents,
    })
}

/// Collect record counts from KV backend
async fn collect_kv_counts(
    state: &AppState,
) -> Result<KvCounts, (StatusCode, Json<ErrorResponse>)> {
    // Get KV backend
    let kv_backend = match state.db.kv_backend() {
        Some(kv) => kv,
        None => {
            return Ok(KvCounts {
                adapters: None,
                tenants: None,
                users: None,
                stacks: None,
            })
        }
    };

    // Count adapters by scanning prefix "adapter:"
    let adapters = count_kv_prefix(kv_backend.backend().as_ref(), "adapter:").await;

    // Count tenants by scanning prefix "tenant:"
    let tenants = count_kv_prefix(kv_backend.backend().as_ref(), "tenant:").await;

    // Count users by scanning prefix "user:"
    let users = count_kv_prefix(kv_backend.backend().as_ref(), "user:").await;

    // Count stacks by scanning prefix "stack:"
    let stacks = count_kv_prefix(kv_backend.backend().as_ref(), "stack:").await;

    Ok(KvCounts {
        adapters,
        tenants,
        users,
        stacks,
    })
}

/// Count records in KV by prefix scan
async fn count_kv_prefix(backend: &dyn KvBackend, prefix: &str) -> Option<i64> {
    match backend.scan_prefix(prefix).await {
        Ok(keys) => Some(keys.len() as i64),
        Err(e) => {
            warn!(error = %e, prefix = %prefix, "Failed to scan KV prefix");
            None
        }
    }
}
