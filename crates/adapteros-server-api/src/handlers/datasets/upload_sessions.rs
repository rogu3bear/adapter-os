use crate::error_helpers::{bad_request, db_error, internal_error};
use crate::types::ErrorResponse;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::path::PathBuf;

const KEEP_PARTIALS_ENV: &str = "AOS_KEEP_CHUNKED_UPLOAD_PARTS";

/// Current schema version for database-persisted upload sessions.
/// Increment when session structure changes to detect stale sessions.
pub const UPLOAD_SESSION_DB_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone)]
pub struct UploadSessionRecord {
    /// Schema version for detecting stale sessions after code updates
    pub schema_version: i64,
    pub session_id: String,
    pub session_key: String,
    pub tenant_id: String,
    pub workspace_id: String,
    pub dataset_id: String,
    pub file_name: String,
    pub normalized_file_name: String,
    pub total_size_bytes: u64,
    pub chunk_size_bytes: usize,
    pub content_type: String,
    pub expected_file_hash_b3: Option<String>,
    pub actual_file_hash_b3: Option<String>,
    pub received_chunks: HashMap<usize, String>,
    pub status: String,
    pub error_message: Option<String>,
    pub temp_dir: PathBuf,
    pub created_at: String,
    #[allow(dead_code)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceivedChunksPayload(HashMap<usize, String>);

pub fn keep_partial_uploads() -> bool {
    std::env::var(KEEP_PARTIALS_ENV)
        .ok()
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn build_session_key(
    idempotency_key: Option<&str>,
    expected_file_hash_b3: Option<&str>,
    tenant_id: &str,
    workspace_id: &str,
    normalized_file_name: &str,
    total_size: u64,
    chunk_size: usize,
    content_type: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"chunked-upload");
    hasher.update(tenant_id.as_bytes());
    hasher.update(b"|");
    hasher.update(workspace_id.as_bytes());
    hasher.update(b"|");

    if let Some(key) = idempotency_key.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }) {
        hasher.update(key.as_bytes());
    } else if let Some(hash) = expected_file_hash_b3 {
        hasher.update(hash.as_bytes());
    } else {
        hasher.update(normalized_file_name.as_bytes());
        hasher.update(b"|");
        hasher.update(total_size.to_string().as_bytes());
        hasher.update(b"|");
        hasher.update(chunk_size.to_string().as_bytes());
        hasher.update(b"|");
        hasher.update(content_type.as_bytes());
    }

    hasher.finalize().to_hex().to_string()
}

pub async fn fetch_session_by_key(
    db: &adapteros_db::Db,
    tenant_id: &str,
    workspace_id: &str,
    session_key: &str,
) -> Result<Option<UploadSessionRecord>, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query(
        "SELECT schema_version, session_id, session_key, tenant_id, workspace_id, dataset_id, file_name,
                normalized_file_name, total_size_bytes, chunk_size_bytes, content_type,
                expected_file_hash_b3, actual_file_hash_b3, received_chunks_json, status,
                error_message, temp_dir, created_at, updated_at
         FROM dataset_upload_sessions
         WHERE tenant_id = ? AND workspace_id = ? AND session_key = ?",
    )
    .bind(tenant_id)
    .bind(workspace_id)
    .bind(session_key)
    .fetch_optional(db.pool())
    .await
    .map_err(|e| db_error(format!("Failed to query upload session: {}", e)))?;

    match row {
        Some(row) => Ok(Some(parse_session_row(&row)?)),
        None => Ok(None),
    }
}

pub async fn fetch_session_by_id(
    db: &adapteros_db::Db,
    session_id: &str,
) -> Result<Option<UploadSessionRecord>, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query(
        "SELECT schema_version, session_id, session_key, tenant_id, workspace_id, dataset_id, file_name,
                normalized_file_name, total_size_bytes, chunk_size_bytes, content_type,
                expected_file_hash_b3, actual_file_hash_b3, received_chunks_json, status,
                error_message, temp_dir, created_at, updated_at
         FROM dataset_upload_sessions
         WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_optional(db.pool())
    .await
    .map_err(|e| db_error(format!("Failed to query upload session: {}", e)))?;

    match row {
        Some(row) => Ok(Some(parse_session_row(&row)?)),
        None => Ok(None),
    }
}

pub async fn insert_session(
    db: &adapteros_db::Db,
    record: &UploadSessionRecord,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let received_chunks_json = serialize_received_chunks(&record.received_chunks)?;

    sqlx::query(
        "INSERT INTO dataset_upload_sessions (
            schema_version, session_id, session_key, tenant_id, workspace_id, dataset_id, file_name,
            normalized_file_name, total_size_bytes, chunk_size_bytes, content_type,
            expected_file_hash_b3, actual_file_hash_b3, received_chunks_json, received_chunks_count,
            status, error_message, temp_dir, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(record.schema_version)
    .bind(&record.session_id)
    .bind(&record.session_key)
    .bind(&record.tenant_id)
    .bind(&record.workspace_id)
    .bind(&record.dataset_id)
    .bind(&record.file_name)
    .bind(&record.normalized_file_name)
    .bind(record.total_size_bytes as i64)
    .bind(record.chunk_size_bytes as i64)
    .bind(&record.content_type)
    .bind(&record.expected_file_hash_b3)
    .bind(&record.actual_file_hash_b3)
    .bind(&received_chunks_json)
    .bind(record.received_chunks.len() as i64)
    .bind(&record.status)
    .bind(&record.error_message)
    .bind(record.temp_dir.to_string_lossy().to_string())
    .execute(db.pool())
    .await
    .map_err(|e| db_error(format!("Failed to insert upload session: {}", e)))?;

    Ok(())
}

pub async fn update_session_chunks(
    db: &adapteros_db::Db,
    session_id: &str,
    received_chunks: &HashMap<usize, String>,
) -> Result<bool, (StatusCode, Json<ErrorResponse>)> {
    let received_chunks_json = serialize_received_chunks(received_chunks)?;

    let result = sqlx::query(
        "UPDATE dataset_upload_sessions
         SET received_chunks_json = ?, received_chunks_count = ?, status = 'uploading',
             updated_at = datetime('now')
         WHERE session_id = ?
           AND status IN ('initiated','uploading')
           AND received_chunks_count <= ?",
    )
    .bind(received_chunks_json)
    .bind(received_chunks.len() as i64)
    .bind(session_id)
    .bind(received_chunks.len() as i64)
    .execute(db.pool())
    .await
    .map_err(|e| db_error(format!("Failed to update upload session chunks: {}", e)))?;

    Ok(result.rows_affected() > 0)
}

pub async fn mark_session_complete(
    db: &adapteros_db::Db,
    session_id: &str,
    dataset_id: &str,
    actual_file_hash_b3: &str,
) -> Result<bool, (StatusCode, Json<ErrorResponse>)> {
    let result = sqlx::query(
        "UPDATE dataset_upload_sessions
         SET status = 'complete', dataset_id = ?, actual_file_hash_b3 = ?,
             updated_at = datetime('now')
         WHERE session_id = ? AND status IN ('initiated','uploading')",
    )
    .bind(dataset_id)
    .bind(actual_file_hash_b3)
    .bind(session_id)
    .execute(db.pool())
    .await
    .map_err(|e| db_error(format!("Failed to mark upload session complete: {}", e)))?;

    Ok(result.rows_affected() > 0)
}

pub async fn mark_session_failed(
    db: &adapteros_db::Db,
    session_id: &str,
    error_message: &str,
) -> Result<bool, (StatusCode, Json<ErrorResponse>)> {
    let result = sqlx::query(
        "UPDATE dataset_upload_sessions
         SET status = 'failed', error_message = ?, updated_at = datetime('now')
         WHERE session_id = ? AND status IN ('initiated','uploading')",
    )
    .bind(error_message)
    .bind(session_id)
    .execute(db.pool())
    .await
    .map_err(|e| db_error(format!("Failed to mark upload session failed: {}", e)))?;

    Ok(result.rows_affected() > 0)
}

#[allow(dead_code)]
pub async fn delete_session(
    db: &adapteros_db::Db,
    session_id: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    sqlx::query("DELETE FROM dataset_upload_sessions WHERE session_id = ?")
        .bind(session_id)
        .execute(db.pool())
        .await
        .map_err(|e| db_error(format!("Failed to delete upload session: {}", e)))?;

    Ok(())
}

pub async fn fetch_expired_sessions(
    db: &adapteros_db::Db,
    cutoff_secs: u64,
) -> Result<Vec<UploadSessionRecord>, (StatusCode, Json<ErrorResponse>)> {
    let cutoff = format!("-{} seconds", cutoff_secs);
    let rows = sqlx::query(
        "SELECT schema_version, session_id, session_key, tenant_id, workspace_id, dataset_id, file_name,
                normalized_file_name, total_size_bytes, chunk_size_bytes, content_type,
                expected_file_hash_b3, actual_file_hash_b3, received_chunks_json, status,
                error_message, temp_dir, created_at, updated_at
         FROM dataset_upload_sessions
         WHERE status IN ('initiated','uploading') AND updated_at < datetime('now', ?)",
    )
    .bind(cutoff)
    .fetch_all(db.pool())
    .await
    .map_err(|e| db_error(format!("Failed to query expired upload sessions: {}", e)))?;

    rows.iter().map(parse_session_row).collect()
}

fn parse_session_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<UploadSessionRecord, (StatusCode, Json<ErrorResponse>)> {
    let received_chunks_json: String = row
        .try_get("received_chunks_json")
        .map_err(|e| internal_error(format!("Failed to read upload session chunk state: {}", e)))?;
    let received_chunks = if received_chunks_json.trim().is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str::<ReceivedChunksPayload>(&received_chunks_json)
            .map(|payload| payload.0)
            .map_err(|e| {
                internal_error(format!("Failed to parse upload session chunk state: {}", e))
            })?
    };

    let temp_dir: String = row
        .try_get("temp_dir")
        .map_err(|e| internal_error(format!("Failed to read upload session temp dir: {}", e)))?;

    Ok(UploadSessionRecord {
        schema_version: row.try_get("schema_version").map_err(map_row_error)?,
        session_id: row.try_get("session_id").map_err(map_row_error)?,
        session_key: row.try_get("session_key").map_err(map_row_error)?,
        tenant_id: row.try_get("tenant_id").map_err(map_row_error)?,
        workspace_id: row.try_get("workspace_id").map_err(map_row_error)?,
        dataset_id: row.try_get("dataset_id").map_err(map_row_error)?,
        file_name: row.try_get("file_name").map_err(map_row_error)?,
        normalized_file_name: row.try_get("normalized_file_name").map_err(map_row_error)?,
        total_size_bytes: row
            .try_get::<i64, _>("total_size_bytes")
            .map_err(map_row_error)? as u64,
        chunk_size_bytes: row
            .try_get::<i64, _>("chunk_size_bytes")
            .map_err(map_row_error)? as usize,
        content_type: row.try_get("content_type").map_err(map_row_error)?,
        expected_file_hash_b3: row
            .try_get("expected_file_hash_b3")
            .map_err(map_row_error)?,
        actual_file_hash_b3: row.try_get("actual_file_hash_b3").map_err(map_row_error)?,
        received_chunks,
        status: row.try_get("status").map_err(map_row_error)?,
        error_message: row.try_get("error_message").map_err(map_row_error)?,
        temp_dir: PathBuf::from(temp_dir),
        created_at: row.try_get("created_at").map_err(map_row_error)?,
        updated_at: row.try_get("updated_at").map_err(map_row_error)?,
    })
}

fn serialize_received_chunks(
    received_chunks: &HashMap<usize, String>,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let payload = ReceivedChunksPayload(received_chunks.clone());
    serde_json::to_string(&payload)
        .map_err(|e| internal_error(format!("Failed to serialize chunk state: {}", e)))
}

fn map_row_error(err: sqlx::Error) -> (StatusCode, Json<ErrorResponse>) {
    internal_error(format!("Failed to parse upload session row: {}", err))
}

pub fn validate_idempotency_key(value: &str) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(bad_request("Idempotency key must not be empty"));
    }
    if trimmed.len() > 256 {
        return Err(bad_request(
            "Idempotency key must be at most 256 characters",
        ));
    }
    Ok(trimmed.to_string())
}
