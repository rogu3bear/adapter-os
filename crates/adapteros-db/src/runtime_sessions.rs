//! Runtime sessions database operations
//!
//! Provides methods for managing runtime sessions, configuration drift tracking,
//! and session lifecycle management. Runtime sessions track each server instance's
//! lifecycle from startup to shutdown, enabling configuration drift detection and
//! runtime behavior analysis.

use crate::Db;
use adapteros_core::error_helpers::DbErrorExt;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// Runtime session record
///
/// Tracks a single server runtime session from startup to shutdown.
/// Used for configuration drift detection and runtime behavior analysis.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RuntimeSession {
    /// Unique ID for this runtime session record
    pub id: String,

    /// Session ID (generated at startup, used for correlation)
    pub session_id: String,

    /// Hash of the configuration (for drift detection)
    pub config_hash: String,

    /// Binary version (e.g., "0.3.0-alpha")
    pub binary_version: String,

    /// Git commit hash of the binary
    pub binary_commit: Option<String>,

    /// When this session started
    pub started_at: String,

    /// When this session ended (NULL if still running)
    pub ended_at: Option<String>,

    /// Reason for session ending ('graceful', 'crash', 'terminated', or NULL)
    pub end_reason: Option<String>,

    /// Hostname where this session ran
    pub hostname: String,

    /// Runtime mode ('development' or 'production')
    pub runtime_mode: String,

    /// Full configuration snapshot (JSON)
    pub config_snapshot: String,

    /// Whether configuration drift was detected (boolean as integer: 0 or 1)
    pub drift_detected: bool,

    /// Summary of detected drift (JSON, NULL if no drift)
    pub drift_summary: Option<String>,

    /// Reference to previous session ID on this host (for continuity tracking)
    pub previous_session_id: Option<String>,

    /// Model path used in this session
    pub model_path: Option<String>,

    /// Adapters root directory
    pub adapters_root: Option<String>,

    /// Database path
    pub database_path: Option<String>,

    /// Var directory path
    pub var_dir: Option<String>,
}

impl Db {
    /// Insert a new runtime session
    ///
    /// # Arguments
    /// * `session` - The runtime session to insert
    ///
    /// # Example
    /// ```rust,no_run
    /// # use adapteros_db::{Db, RuntimeSession};
    /// # async fn example(db: &Db) -> Result<(), Box<dyn std::error::Error>> {
    /// let session = RuntimeSession {
    ///     id: uuid::Uuid::new_v4().to_string(),
    ///     session_id: "session-123".to_string(),
    ///     config_hash: "abc123".to_string(),
    ///     binary_version: "0.3.0-alpha".to_string(),
    ///     binary_commit: Some("def456".to_string()),
    ///     started_at: chrono::Utc::now().to_rfc3339(),
    ///     ended_at: None,
    ///     end_reason: None,
    ///     hostname: "server-01".to_string(),
    ///     runtime_mode: "production".to_string(),
    ///     config_snapshot: "{}".to_string(),
    ///     drift_detected: false,
    ///     drift_summary: None,
    ///     previous_session_id: None,
    ///     model_path: Some("/models/qwen".to_string()),
    ///     adapters_root: Some("/var/adapters".to_string()),
    ///     database_path: Some("/var/db.sqlite".to_string()),
    ///     var_dir: Some("/var".to_string()),
    /// };
    /// db.insert_runtime_session(&session).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn insert_runtime_session(&self, session: &RuntimeSession) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO runtime_sessions (
                id, session_id, config_hash, binary_version, binary_commit,
                started_at, ended_at, end_reason, hostname, runtime_mode,
                config_snapshot, drift_detected, drift_summary, previous_session_id,
                model_path, adapters_root, database_path, var_dir
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&session.id)
        .bind(&session.session_id)
        .bind(&session.config_hash)
        .bind(&session.binary_version)
        .bind(&session.binary_commit)
        .bind(&session.started_at)
        .bind(&session.ended_at)
        .bind(&session.end_reason)
        .bind(&session.hostname)
        .bind(&session.runtime_mode)
        .bind(&session.config_snapshot)
        .bind(session.drift_detected as i64)
        .bind(&session.drift_summary)
        .bind(&session.previous_session_id)
        .bind(&session.model_path)
        .bind(&session.adapters_root)
        .bind(&session.database_path)
        .bind(&session.var_dir)
        .execute(self.pool())
        .await
        .db_err("insert runtime session")?;

        Ok(())
    }

    /// Get the most recent ended session for a hostname
    ///
    /// This is used to detect configuration drift by comparing the current
    /// configuration with the previous session's configuration.
    ///
    /// # Arguments
    /// * `hostname` - The hostname to query for
    ///
    /// # Returns
    /// The most recent ended session for this hostname, or None if no previous session exists
    pub async fn get_most_recent_session(&self, hostname: &str) -> Result<Option<RuntimeSession>> {
        let session = sqlx::query_as::<_, RuntimeSession>(
            r#"
            SELECT id, session_id, config_hash, binary_version, binary_commit,
                   started_at, ended_at, end_reason, hostname, runtime_mode,
                   config_snapshot, drift_detected, drift_summary, previous_session_id,
                   model_path, adapters_root, database_path, var_dir
            FROM runtime_sessions
            WHERE hostname = ? AND ended_at IS NOT NULL
            ORDER BY started_at DESC
            LIMIT 1
            "#,
        )
        .bind(hostname)
        .fetch_optional(self.pool())
        .await
        .db_err("get most recent session")?;

        Ok(session)
    }

    /// Get a runtime session by ID
    ///
    /// # Arguments
    /// * `id` - The session ID to retrieve
    ///
    /// # Returns
    /// The runtime session if found, or None
    pub async fn get_runtime_session(&self, id: &str) -> Result<Option<RuntimeSession>> {
        let session = sqlx::query_as::<_, RuntimeSession>(
            r#"
            SELECT id, session_id, config_hash, binary_version, binary_commit,
                   started_at, ended_at, end_reason, hostname, runtime_mode,
                   config_snapshot, drift_detected, drift_summary, previous_session_id,
                   model_path, adapters_root, database_path, var_dir
            FROM runtime_sessions
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .db_err("get runtime session")?;

        Ok(session)
    }

    /// Mark a session as ended
    ///
    /// Updates the session's ended_at timestamp and records the reason for ending.
    ///
    /// # Arguments
    /// * `id` - The session ID to mark as ended
    /// * `reason` - The reason for ending ('graceful', 'crash', or 'terminated')
    ///
    /// # Example
    /// ```rust,no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db, session_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    /// db.end_runtime_session(session_id, "graceful").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn end_runtime_session(&self, id: &str, reason: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE runtime_sessions
            SET ended_at = datetime('now'),
                end_reason = ?
            WHERE id = ?
            "#,
        )
        .bind(reason)
        .bind(id)
        .execute(self.pool())
        .await
        .db_err("end runtime session")?;

        Ok(())
    }

    /// Clean up old sessions based on retention policy
    ///
    /// Removes old runtime sessions to prevent unbounded growth of the table.
    /// The retention policy keeps:
    /// 1. Sessions within the retention period (retention_days)
    /// 2. The N most recent sessions per host (max_per_host)
    ///
    /// # Arguments
    /// * `retention_days` - Number of days to retain sessions (e.g., 90)
    /// * `max_per_host` - Maximum sessions to keep per hostname (e.g., 100)
    ///
    /// # Returns
    /// Number of sessions deleted
    ///
    /// # Example
    /// ```rust,no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> Result<(), Box<dyn std::error::Error>> {
    /// // Keep last 90 days, max 100 sessions per host
    /// let deleted = db.cleanup_old_sessions(90, 100).await?;
    /// println!("Cleaned up {} old sessions", deleted);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn cleanup_old_sessions(
        &self,
        retention_days: i64,
        max_per_host: i64,
    ) -> Result<i64> {
        // Delete sessions older than retention period, excluding the N most recent per host
        let result = sqlx::query(
            r#"
            DELETE FROM runtime_sessions
            WHERE id IN (
                SELECT id FROM runtime_sessions rs
                WHERE
                    -- Older than retention period
                    julianday('now') - julianday(started_at) > ?
                    -- Not in the N most recent for this host
                    AND id NOT IN (
                        SELECT id FROM runtime_sessions
                        WHERE hostname = rs.hostname
                        ORDER BY started_at DESC
                        LIMIT ?
                    )
            )
            "#,
        )
        .bind(retention_days)
        .bind(max_per_host)
        .execute(self.pool())
        .await
        .db_err("cleanup old sessions")?;

        Ok(result.rows_affected() as i64)
    }
}
