//! Chat session database operations
//!
//! Provides methods for managing persistent chat sessions with stack context,
//! trace linkage to router decisions, adapters, and training jobs.
//!
//! # Overview
//! Chat sessions are the primary way users interact with AdapterOS in the
//! workspace experience. Each session:
//! - Is scoped to a tenant
//! - Can be associated with a specific adapter stack
//! - Maintains a history of messages
//! - Links to router decisions, adapters, and training jobs for traceability
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_db】

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::{debug, info};

use crate::query_helpers::db_err;
use crate::Db;

/// Chat session record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ChatSession {
    pub id: String,
    pub tenant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    pub name: String,
    pub created_at: String,
    pub last_activity_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

/// Chat message record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String, // 'user', 'assistant', 'system'
    pub content: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

/// Chat session trace record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ChatSessionTrace {
    pub id: i64,
    pub session_id: String,
    pub trace_type: String, // 'router_decision', 'adapter', 'training_job', 'audit_event'
    pub trace_id: String,
    pub created_at: String,
}

/// Parameters for creating a new chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CreateChatSessionParams {
    pub id: String,
    pub tenant_id: String,
    pub user_id: Option<String>,
    pub stack_id: Option<String>,
    pub collection_id: Option<String>,
    pub name: String,
    pub metadata_json: Option<String>,
}

/// Parameters for adding a message to a session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct AddMessageParams {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub metadata_json: Option<String>,
}

// =============================================================================
// New Types for Tags, Categories, Soft Delete, Search, and Sharing
// =============================================================================

/// Chat session tag (tenant-scoped)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ChatTag {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Chat session category (hierarchical)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ChatCategory {
    pub id: String,
    pub tenant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub name: String,
    pub path: String,
    pub depth: i32,
    pub sort_order: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub created_at: String,
}

/// Extended chat session with status fields
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ChatSessionWithStatus {
    pub id: String,
    pub tenant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    pub name: String,
    pub created_at: String,
    pub last_activity_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub is_shared: bool,
}

/// Search result for chat sessions/messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ChatSearchResult {
    pub session_id: String,
    pub session_name: String,
    pub match_type: String, // "session" or "message"
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_role: Option<String>,
    pub relevance_score: f64,
    pub last_activity_at: String,
}

/// Internal row type for session FTS search
#[derive(Debug, FromRow)]
struct SessionSearchRow {
    pub id: String,
    pub name: String,
    pub last_activity_at: String,
    pub name_snippet: Option<String>,
    pub description_snippet: Option<String>,
    pub rank: f64,
}

/// Internal row type for message FTS search
#[derive(Debug, FromRow)]
struct MessageSearchRow {
    pub message_id: String,
    pub session_id: String,
    pub role: String,
    pub session_name: String,
    pub last_activity_at: String,
    pub content_snippet: Option<String>,
    pub rank: f64,
}

/// Session share record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionShare {
    pub id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_with_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_with_tenant_id: Option<String>,
    pub permission: String,
    pub shared_by: String,
    pub shared_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

impl Db {
    /// Create a new chat session
    ///
    /// # Arguments
    /// * `params` - Session creation parameters
    ///
    /// # Returns
    /// The created session ID
    ///
    /// # Errors
    /// Returns `AosError::Database` if the session creation fails
    pub async fn create_chat_session(&self, params: CreateChatSessionParams) -> Result<String> {
        debug!(
            session_id = %params.id,
            tenant_id = %params.tenant_id,
            stack_id = ?params.stack_id,
            collection_id = ?params.collection_id,
            "Creating chat session"
        );

        sqlx::query(
            r#"
            INSERT INTO chat_sessions (id, tenant_id, user_id, stack_id, collection_id, name, metadata_json)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.user_id)
        .bind(&params.stack_id)
        .bind(&params.collection_id)
        .bind(&params.name)
        .bind(&params.metadata_json)
        .execute(&*self.pool())
        .await
        .map_err(db_err("create chat session"))?;

        info!(session_id = %params.id, "Chat session created");
        Ok(params.id)
    }

    /// List chat sessions for a user/tenant
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    /// * `user_id` - Optional user ID filter
    /// * `limit` - Maximum number of sessions to return (default: 50)
    ///
    /// # Returns
    /// List of chat sessions ordered by last activity (most recent first)
    pub async fn list_chat_sessions(
        &self,
        tenant_id: &str,
        user_id: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ChatSession>> {
        let limit = limit.unwrap_or(50);

        let sessions = if let Some(user_id) = user_id {
            sqlx::query_as::<_, ChatSession>(
                r#"
                SELECT id, tenant_id, user_id, stack_id, collection_id, name, created_at, last_activity_at, metadata_json
                FROM chat_sessions
                WHERE tenant_id = ? AND user_id = ?
                ORDER BY last_activity_at DESC
                LIMIT ?
                "#,
            )
            .bind(tenant_id)
            .bind(user_id)
            .bind(limit)
            .fetch_all(&*self.pool())
            .await
        } else {
            sqlx::query_as::<_, ChatSession>(
                r#"
                SELECT id, tenant_id, user_id, stack_id, collection_id, name, created_at, last_activity_at, metadata_json
                FROM chat_sessions
                WHERE tenant_id = ?
                ORDER BY last_activity_at DESC
                LIMIT ?
                "#,
            )
            .bind(tenant_id)
            .bind(limit)
            .fetch_all(&*self.pool())
            .await
        }
        .map_err(db_err("list chat sessions"))?;

        debug!(
            tenant_id = %tenant_id,
            count = sessions.len(),
            "Listed chat sessions"
        );

        Ok(sessions)
    }

    /// Get a specific chat session
    ///
    /// # Arguments
    /// * `session_id` - The session ID
    ///
    /// # Returns
    /// The session if found, None otherwise
    pub async fn get_chat_session(&self, session_id: &str) -> Result<Option<ChatSession>> {
        let session = sqlx::query_as::<_, ChatSession>(
            r#"
            SELECT id, tenant_id, user_id, stack_id, collection_id, name, created_at, last_activity_at, metadata_json
            FROM chat_sessions
            WHERE id = ?
            "#,
        )
        .bind(session_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(db_err("get chat session"))?;

        Ok(session)
    }

    /// Update chat session activity timestamp
    ///
    /// Should be called whenever a message is added or other activity occurs
    ///
    /// # Arguments
    /// * `session_id` - The session ID
    pub async fn update_chat_session_activity(&self, session_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE chat_sessions
            SET last_activity_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(session_id)
        .execute(&*self.pool())
        .await
        .map_err(db_err("update session activity"))?;

        Ok(())
    }

    /// Update the collection ID for a chat session
    ///
    /// # Arguments
    /// * `session_id` - The session ID
    /// * `collection_id` - The new collection ID (or None to clear)
    ///
    /// # Returns
    /// Ok if the update succeeded
    ///
    /// # Errors
    /// Returns `AosError::Database` if the update fails
    pub async fn update_session_collection(
        &self,
        session_id: &str,
        collection_id: Option<String>,
    ) -> Result<()> {
        debug!(
            session_id = %session_id,
            collection_id = ?collection_id,
            "Updating session collection"
        );

        sqlx::query(
            r#"
            UPDATE chat_sessions
            SET collection_id = ?
            WHERE id = ?
            "#,
        )
        .bind(&collection_id)
        .bind(session_id)
        .execute(&*self.pool())
        .await
        .map_err(db_err("update session collection"))?;

        Ok(())
    }

    /// Add a message to a chat session
    ///
    /// Automatically updates the session's last_activity_at timestamp
    ///
    /// # Arguments
    /// * `params` - Message parameters
    pub async fn add_chat_message(&self, params: AddMessageParams) -> Result<String> {
        debug!(
            message_id = %params.id,
            session_id = %params.session_id,
            role = %params.role,
            "Adding chat message"
        );

        // Insert message
        sqlx::query(
            r#"
            INSERT INTO chat_messages (id, session_id, role, content, metadata_json)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&params.id)
        .bind(&params.session_id)
        .bind(&params.role)
        .bind(&params.content)
        .bind(&params.metadata_json)
        .execute(&*self.pool())
        .await
        .map_err(db_err("add chat message"))?;

        // Update session activity
        self.update_chat_session_activity(&params.session_id).await?;

        Ok(params.id)
    }

    /// Get messages for a chat session
    ///
    /// # Arguments
    /// * `session_id` - The session ID
    /// * `limit` - Maximum number of messages to return (default: 100)
    ///
    /// # Returns
    /// List of messages ordered by timestamp (oldest first)
    pub async fn get_chat_messages(
        &self,
        session_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<ChatMessage>> {
        let limit = limit.unwrap_or(100);

        let messages = sqlx::query_as::<_, ChatMessage>(
            r#"
            SELECT id, session_id, role, content, timestamp, metadata_json
            FROM chat_messages
            WHERE session_id = ?
            ORDER BY timestamp ASC
            LIMIT ?
            "#,
        )
        .bind(session_id)
        .bind(limit)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("get chat messages"))?;

        debug!(
            session_id = %session_id,
            count = messages.len(),
            "Retrieved chat messages"
        );

        Ok(messages)
    }

    /// Link a trace (router decision, adapter, job) to a session
    ///
    /// # Arguments
    /// * `session_id` - The session ID
    /// * `trace_type` - Type of trace ('router_decision', 'adapter', 'training_job', 'audit_event')
    /// * `trace_id` - ID of the traced entity
    pub async fn add_session_trace(
        &self,
        session_id: &str,
        trace_type: &str,
        trace_id: &str,
    ) -> Result<i64> {
        debug!(
            session_id = %session_id,
            trace_type = %trace_type,
            trace_id = %trace_id,
            "Adding session trace"
        );

        let result = sqlx::query(
            r#"
            INSERT INTO chat_session_traces (session_id, trace_type, trace_id)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(session_id)
        .bind(trace_type)
        .bind(trace_id)
        .execute(&*self.pool())
        .await
        .map_err(db_err("add session trace"))?;

        Ok(result.last_insert_rowid())
    }

    /// Get all traces for a session
    ///
    /// # Arguments
    /// * `session_id` - The session ID
    ///
    /// # Returns
    /// List of traces ordered by creation time (most recent first)
    pub async fn get_session_traces(&self, session_id: &str) -> Result<Vec<ChatSessionTrace>> {
        let traces = sqlx::query_as::<_, ChatSessionTrace>(
            r#"
            SELECT id, session_id, trace_type, trace_id, created_at
            FROM chat_session_traces
            WHERE session_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(session_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("get session traces"))?;

        debug!(
            session_id = %session_id,
            count = traces.len(),
            "Retrieved session traces"
        );

        Ok(traces)
    }

    /// Get session summary with trace counts
    ///
    /// # Arguments
    /// * `session_id` - The session ID
    ///
    /// # Returns
    /// JSON summary with message count, trace counts by type, active adapters
    pub async fn get_session_summary(&self, session_id: &str) -> Result<serde_json::Value> {
        // Get message count
        let message_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE session_id = ?")
                .bind(session_id)
                .fetch_one(&*self.pool())
                .await
                .map_err(db_err("count messages"))?;

        // Get trace counts by type
        let trace_counts: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT trace_type, COUNT(*) as count
            FROM chat_session_traces
            WHERE session_id = ?
            GROUP BY trace_type
            "#,
        )
        .bind(session_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("count traces"))?;

        // Get session details
        let session = self
            .get_chat_session(session_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Session not found: {}", session_id)))?;

        // Build summary
        let mut trace_counts_map = serde_json::Map::new();
        for (trace_type, count) in trace_counts {
            trace_counts_map.insert(trace_type, serde_json::json!(count));
        }

        let summary = serde_json::json!({
            "session_id": session.id,
            "tenant_id": session.tenant_id,
            "stack_id": session.stack_id,
            "collection_id": session.collection_id,
            "name": session.name,
            "created_at": session.created_at,
            "last_activity_at": session.last_activity_at,
            "message_count": message_count,
            "trace_counts": trace_counts_map,
        });

        Ok(summary)
    }

    /// Delete a chat session and all associated data
    ///
    /// Cascading deletes will remove messages and traces automatically
    ///
    /// # Arguments
    /// * `session_id` - The session ID
    pub async fn delete_chat_session(&self, session_id: &str) -> Result<()> {
        info!(session_id = %session_id, "Deleting chat session");

        sqlx::query("DELETE FROM chat_sessions WHERE id = ?")
            .bind(session_id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("delete chat session"))?;

        Ok(())
    }

    // =========================================================================
    // Tags Management
    // =========================================================================

    /// Create a new chat session tag (tenant-scoped)
    pub async fn create_chat_tag(
        &self,
        tenant_id: &str,
        name: &str,
        color: Option<&str>,
        description: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<ChatTag> {
        let id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO chat_session_tags (id, tenant_id, name, color, description, created_by)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(name)
        .bind(color)
        .bind(description)
        .bind(created_by)
        .execute(&*self.pool())
        .await
        .map_err(db_err("create chat tag"))?;

        self.get_chat_tag(&id)
            .await?
            .ok_or_else(|| AosError::Database("Failed to retrieve created tag".to_string()))
    }

    /// Get a chat tag by ID
    pub async fn get_chat_tag(&self, tag_id: &str) -> Result<Option<ChatTag>> {
        sqlx::query_as::<_, ChatTag>(
            "SELECT id, tenant_id, name, color, description, created_at, created_by FROM chat_session_tags WHERE id = ?"
        )
        .bind(tag_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get chat tag: {}", e)))
    }

    /// List all tags for a tenant
    pub async fn list_chat_tags(&self, tenant_id: &str) -> Result<Vec<ChatTag>> {
        sqlx::query_as::<_, ChatTag>(
            r#"
            SELECT id, tenant_id, name, color, description, created_at, created_by
            FROM chat_session_tags
            WHERE tenant_id = ?
            ORDER BY name ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list chat tags: {}", e)))
    }

    /// Update a chat tag
    pub async fn update_chat_tag(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
        description: Option<&str>,
    ) -> Result<()> {
        let mut updates = Vec::new();
        let mut bindings: Vec<String> = Vec::new();

        if let Some(n) = name {
            updates.push("name = ?");
            bindings.push(n.to_string());
        }
        if let Some(c) = color {
            updates.push("color = ?");
            bindings.push(c.to_string());
        }
        if let Some(d) = description {
            updates.push("description = ?");
            bindings.push(d.to_string());
        }

        if updates.is_empty() {
            return Ok(());
        }

        let query = format!(
            "UPDATE chat_session_tags SET {} WHERE id = ?",
            updates.join(", ")
        );

        let mut q = sqlx::query(&query);
        for b in &bindings {
            q = q.bind(b);
        }
        q = q.bind(tag_id);

        q.execute(&*self.pool())
            .await
            .map_err(db_err("update chat tag"))?;

        Ok(())
    }

    /// Delete a chat tag
    pub async fn delete_chat_tag(&self, tag_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM chat_session_tags WHERE id = ?")
            .bind(tag_id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("delete chat tag"))?;
        Ok(())
    }

    /// Assign tags to a session
    pub async fn assign_tags_to_session(
        &self,
        session_id: &str,
        tag_ids: &[String],
        assigned_by: Option<&str>,
    ) -> Result<()> {
        for tag_id in tag_ids {
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO chat_session_tag_assignments (session_id, tag_id, assigned_by)
                VALUES (?, ?, ?)
                "#,
            )
            .bind(session_id)
            .bind(tag_id)
            .bind(assigned_by)
            .execute(&*self.pool())
            .await
            .map_err(db_err("assign tag"))?;
        }
        Ok(())
    }

    /// Remove a tag from a session
    pub async fn remove_tag_from_session(&self, session_id: &str, tag_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM chat_session_tag_assignments WHERE session_id = ? AND tag_id = ?")
            .bind(session_id)
            .bind(tag_id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("remove tag"))?;
        Ok(())
    }

    /// Get tags for a session
    pub async fn get_session_tags(&self, session_id: &str) -> Result<Vec<ChatTag>> {
        sqlx::query_as::<_, ChatTag>(
            r#"
            SELECT t.id, t.tenant_id, t.name, t.color, t.description, t.created_at, t.created_by
            FROM chat_session_tags t
            JOIN chat_session_tag_assignments a ON a.tag_id = t.id
            WHERE a.session_id = ?
            ORDER BY t.name ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get session tags: {}", e)))
    }

    // =========================================================================
    // Categories Management
    // =========================================================================

    /// Create a new category (hierarchical)
    pub async fn create_chat_category(
        &self,
        tenant_id: &str,
        name: &str,
        parent_id: Option<&str>,
        icon: Option<&str>,
        color: Option<&str>,
    ) -> Result<ChatCategory> {
        let id = uuid::Uuid::new_v4().to_string();

        // Calculate path and depth
        let (path, depth) = if let Some(pid) = parent_id {
            let parent = self
                .get_chat_category(pid)
                .await?
                .ok_or_else(|| AosError::NotFound(format!("Parent category not found: {}", pid)))?;
            (format!("{}/{}", parent.path, &id), parent.depth + 1)
        } else {
            (id.clone(), 0)
        };

        // Get max sort_order for siblings
        let sort_order: i32 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(sort_order), -1) + 1
            FROM chat_session_categories
            WHERE tenant_id = ? AND (parent_id = ? OR (? IS NULL AND parent_id IS NULL))
            "#,
        )
        .bind(tenant_id)
        .bind(parent_id)
        .bind(parent_id)
        .fetch_one(&*self.pool())
        .await
        .map_err(db_err("get sort order"))?;

        sqlx::query(
            r#"
            INSERT INTO chat_session_categories (id, tenant_id, parent_id, name, path, depth, sort_order, icon, color)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(parent_id)
        .bind(name)
        .bind(&path)
        .bind(depth)
        .bind(sort_order)
        .bind(icon)
        .bind(color)
        .execute(&*self.pool())
        .await
        .map_err(db_err("create category"))?;

        self.get_chat_category(&id)
            .await?
            .ok_or_else(|| AosError::Database("Failed to retrieve created category".to_string()))
    }

    /// Get a category by ID
    pub async fn get_chat_category(&self, category_id: &str) -> Result<Option<ChatCategory>> {
        sqlx::query_as::<_, ChatCategory>(
            r#"
            SELECT id, tenant_id, parent_id, name, path, depth, sort_order, icon, color, created_at
            FROM chat_session_categories WHERE id = ?
            "#,
        )
        .bind(category_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get category: {}", e)))
    }

    /// List all categories for a tenant (tree-sorted by path)
    pub async fn list_chat_categories(&self, tenant_id: &str) -> Result<Vec<ChatCategory>> {
        sqlx::query_as::<_, ChatCategory>(
            r#"
            SELECT id, tenant_id, parent_id, name, path, depth, sort_order, icon, color, created_at
            FROM chat_session_categories
            WHERE tenant_id = ?
            ORDER BY path ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list categories: {}", e)))
    }

    /// Update a category
    pub async fn update_chat_category(
        &self,
        category_id: &str,
        name: Option<&str>,
        icon: Option<&str>,
        color: Option<&str>,
    ) -> Result<()> {
        let mut updates = Vec::new();
        let mut bindings: Vec<String> = Vec::new();

        if let Some(n) = name {
            updates.push("name = ?");
            bindings.push(n.to_string());
        }
        if let Some(i) = icon {
            updates.push("icon = ?");
            bindings.push(i.to_string());
        }
        if let Some(c) = color {
            updates.push("color = ?");
            bindings.push(c.to_string());
        }

        if updates.is_empty() {
            return Ok(());
        }

        let query = format!(
            "UPDATE chat_session_categories SET {} WHERE id = ?",
            updates.join(", ")
        );

        let mut q = sqlx::query(&query);
        for b in &bindings {
            q = q.bind(b);
        }
        q = q.bind(category_id);

        q.execute(&*self.pool())
            .await
            .map_err(db_err("update category"))?;

        Ok(())
    }

    /// Delete a category (fails if has children or sessions)
    pub async fn delete_chat_category(&self, category_id: &str) -> Result<()> {
        // Check for children
        let child_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chat_session_categories WHERE parent_id = ?")
                .bind(category_id)
                .fetch_one(&*self.pool())
                .await
                .map_err(db_err("check children"))?;

        if child_count > 0 {
            return Err(AosError::Validation(
                "Cannot delete category with children".to_string(),
            ));
        }

        // Check for sessions
        let session_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chat_sessions WHERE category_id = ?")
                .bind(category_id)
                .fetch_one(&*self.pool())
                .await
                .map_err(db_err("check sessions"))?;

        if session_count > 0 {
            return Err(AosError::Validation(
                "Cannot delete category with assigned sessions".to_string(),
            ));
        }

        sqlx::query("DELETE FROM chat_session_categories WHERE id = ?")
            .bind(category_id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("delete category"))?;

        Ok(())
    }

    /// Set the category for a session
    pub async fn set_session_category(
        &self,
        session_id: &str,
        category_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE chat_sessions SET category_id = ? WHERE id = ?")
            .bind(category_id)
            .bind(session_id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("set session category"))?;
        Ok(())
    }

    // =========================================================================
    // Soft Delete / Archive
    // =========================================================================

    /// Soft delete a session (moves to trash)
    pub async fn soft_delete_session(&self, session_id: &str, deleted_by: &str) -> Result<()> {
        // Calculate retention_until (30 days default)
        sqlx::query(
            r#"
            UPDATE chat_sessions
            SET status = 'deleted',
                deleted_at = datetime('now'),
                deleted_by = ?,
                retention_until = datetime('now', '+30 days')
            WHERE id = ? AND status != 'deleted'
            "#,
        )
        .bind(deleted_by)
        .bind(session_id)
        .execute(&*self.pool())
        .await
        .map_err(db_err("soft delete session"))?;

        info!(session_id = %session_id, "Session soft deleted");
        Ok(())
    }

    /// Archive a session
    pub async fn archive_session(
        &self,
        session_id: &str,
        archived_by: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE chat_sessions
            SET status = 'archived',
                archived_at = datetime('now'),
                archived_by = ?,
                archive_reason = ?
            WHERE id = ? AND status = 'active'
            "#,
        )
        .bind(archived_by)
        .bind(reason)
        .bind(session_id)
        .execute(&*self.pool())
        .await
        .map_err(db_err("archive session"))?;

        info!(session_id = %session_id, "Session archived");
        Ok(())
    }

    /// Restore a deleted or archived session (admin-only)
    pub async fn restore_session(&self, session_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE chat_sessions
            SET status = 'active',
                deleted_at = NULL,
                deleted_by = NULL,
                archived_at = NULL,
                archived_by = NULL,
                archive_reason = NULL,
                retention_until = NULL
            WHERE id = ? AND status IN ('deleted', 'archived')
            "#,
        )
        .bind(session_id)
        .execute(&*self.pool())
        .await
        .map_err(db_err("restore session"))?;

        info!(session_id = %session_id, "Session restored");
        Ok(())
    }

    /// Hard delete a session permanently
    pub async fn hard_delete_session(&self, session_id: &str) -> Result<()> {
        info!(session_id = %session_id, "Permanently deleting session");

        sqlx::query("DELETE FROM chat_sessions WHERE id = ?")
            .bind(session_id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("hard delete session"))?;

        Ok(())
    }

    /// List archived sessions
    pub async fn list_archived_sessions(
        &self,
        tenant_id: &str,
        user_id: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ChatSessionWithStatus>> {
        let limit = limit.unwrap_or(50);

        let sessions = if let Some(uid) = user_id {
            sqlx::query_as::<_, ChatSessionWithStatus>(
                r#"
                SELECT id, tenant_id, user_id, stack_id, collection_id, name, created_at,
                       last_activity_at, metadata_json, category_id, status, deleted_at,
                       deleted_by, archived_at, archived_by, archive_reason, description, is_shared
                FROM chat_sessions
                WHERE tenant_id = ? AND user_id = ? AND status = 'archived'
                ORDER BY archived_at DESC
                LIMIT ?
                "#,
            )
            .bind(tenant_id)
            .bind(uid)
            .bind(limit)
            .fetch_all(&*self.pool())
            .await
        } else {
            sqlx::query_as::<_, ChatSessionWithStatus>(
                r#"
                SELECT id, tenant_id, user_id, stack_id, collection_id, name, created_at,
                       last_activity_at, metadata_json, category_id, status, deleted_at,
                       deleted_by, archived_at, archived_by, archive_reason, description, is_shared
                FROM chat_sessions
                WHERE tenant_id = ? AND status = 'archived'
                ORDER BY archived_at DESC
                LIMIT ?
                "#,
            )
            .bind(tenant_id)
            .bind(limit)
            .fetch_all(&*self.pool())
            .await
        }
        .map_err(db_err("list archived sessions"))?;

        Ok(sessions)
    }

    /// List deleted sessions (trash)
    pub async fn list_deleted_sessions(
        &self,
        tenant_id: &str,
        user_id: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ChatSessionWithStatus>> {
        let limit = limit.unwrap_or(50);

        let sessions = if let Some(uid) = user_id {
            sqlx::query_as::<_, ChatSessionWithStatus>(
                r#"
                SELECT id, tenant_id, user_id, stack_id, collection_id, name, created_at,
                       last_activity_at, metadata_json, category_id, status, deleted_at,
                       deleted_by, archived_at, archived_by, archive_reason, description, is_shared
                FROM chat_sessions
                WHERE tenant_id = ? AND user_id = ? AND status = 'deleted'
                ORDER BY deleted_at DESC
                LIMIT ?
                "#,
            )
            .bind(tenant_id)
            .bind(uid)
            .bind(limit)
            .fetch_all(&*self.pool())
            .await
        } else {
            sqlx::query_as::<_, ChatSessionWithStatus>(
                r#"
                SELECT id, tenant_id, user_id, stack_id, collection_id, name, created_at,
                       last_activity_at, metadata_json, category_id, status, deleted_at,
                       deleted_by, archived_at, archived_by, archive_reason, description, is_shared
                FROM chat_sessions
                WHERE tenant_id = ? AND status = 'deleted'
                ORDER BY deleted_at DESC
                LIMIT ?
                "#,
            )
            .bind(tenant_id)
            .bind(limit)
            .fetch_all(&*self.pool())
            .await
        }
        .map_err(db_err("list deleted sessions"))?;

        Ok(sessions)
    }

    // =========================================================================
    // Full-Text Search
    // =========================================================================

    /// Search sessions and messages using FTS5
    pub async fn search_chat_sessions(
        &self,
        tenant_id: &str,
        query: &str,
        scope: &str, // "sessions", "messages", "all"
        category_id: Option<&str>,
        tag_ids: Option<&[String]>,
        include_archived: bool,
        limit: i64,
    ) -> Result<Vec<ChatSearchResult>> {
        let mut results = Vec::new();

        // Search sessions
        if scope == "sessions" || scope == "all" {
            let status_filter = if include_archived {
                "AND cs.status IN ('active', 'archived')"
            } else {
                "AND cs.status = 'active'"
            };

            let category_filter = if category_id.is_some() {
                "AND cs.category_id = ?"
            } else {
                ""
            };

            let base_query = format!(
                r#"
                SELECT cs.id, cs.name, cs.last_activity_at,
                       snippet(chat_sessions_fts, 2, '<mark>', '</mark>', '...', 32) as name_snippet,
                       snippet(chat_sessions_fts, 3, '<mark>', '</mark>', '...', 64) as description_snippet,
                       rank
                FROM chat_sessions_fts
                JOIN chat_sessions cs ON cs.id = chat_sessions_fts.session_id
                WHERE chat_sessions_fts MATCH ?
                  AND chat_sessions_fts.tenant_id = ?
                  {} {}
                ORDER BY rank
                LIMIT ?
                "#,
                status_filter, category_filter
            );

            let mut q = sqlx::query_as::<_, SessionSearchRow>(&base_query)
                .bind(query)
                .bind(tenant_id);

            if let Some(cat_id) = category_id {
                q = q.bind(cat_id);
            }

            let rows: Vec<SessionSearchRow> = q
                .bind(limit)
                .fetch_all(&*self.pool())
                .await
                .map_err(db_err("search sessions"))?;

            for row in rows {
                // Filter by tags if specified
                if let Some(tags) = tag_ids {
                    let session_tags = self.get_session_tags(&row.id).await?;
                    let session_tag_ids: Vec<&str> =
                        session_tags.iter().map(|t| t.id.as_str()).collect();
                    if !tags.iter().any(|t| session_tag_ids.contains(&t.as_str())) {
                        continue;
                    }
                }

                results.push(ChatSearchResult {
                    session_id: row.id,
                    session_name: row.name,
                    match_type: "session".to_string(),
                    snippet: row
                        .name_snippet
                        .or(row.description_snippet)
                        .unwrap_or_default(),
                    message_id: None,
                    message_role: None,
                    relevance_score: -row.rank, // FTS5 rank is negative, lower is better
                    last_activity_at: row.last_activity_at,
                });
            }
        }

        // Search messages
        if scope == "messages" || scope == "all" {
            let status_filter = if include_archived {
                "AND cs.status IN ('active', 'archived')"
            } else {
                "AND cs.status = 'active'"
            };

            let msg_query = format!(
                r#"
                SELECT cm.id as message_id, cm.session_id, cm.role, cs.name as session_name,
                       cs.last_activity_at,
                       snippet(chat_messages_fts, 3, '<mark>', '</mark>', '...', 64) as content_snippet,
                       rank
                FROM chat_messages_fts
                JOIN chat_messages cm ON cm.id = chat_messages_fts.message_id
                JOIN chat_sessions cs ON cs.id = cm.session_id
                WHERE chat_messages_fts MATCH ?
                  AND chat_messages_fts.tenant_id = ?
                  AND cm.deleted_at IS NULL
                  {}
                ORDER BY rank
                LIMIT ?
                "#,
                status_filter
            );

            let rows: Vec<MessageSearchRow> = sqlx::query_as(&msg_query)
                .bind(query)
                .bind(tenant_id)
                .bind(limit)
                .fetch_all(&*self.pool())
                .await
                .map_err(db_err("search messages"))?;

            for row in rows {
                results.push(ChatSearchResult {
                    session_id: row.session_id,
                    session_name: row.session_name,
                    match_type: "message".to_string(),
                    snippet: row.content_snippet.unwrap_or_default(),
                    message_id: Some(row.message_id),
                    message_role: Some(row.role),
                    relevance_score: -row.rank,
                    last_activity_at: row.last_activity_at,
                });
            }
        }

        // Sort combined results by relevance
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit as usize);

        Ok(results)
    }

    // =========================================================================
    // Session Sharing
    // =========================================================================

    /// Share a session with a workspace
    pub async fn share_session_with_workspace(
        &self,
        session_id: &str,
        workspace_id: &str,
        permission: &str,
        shared_by: &str,
        expires_at: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO chat_session_shares (id, session_id, workspace_id, permission, shared_by, expires_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(session_id, workspace_id) DO UPDATE SET
                permission = excluded.permission,
                expires_at = excluded.expires_at,
                revoked_at = NULL
            "#,
        )
        .bind(&id)
        .bind(session_id)
        .bind(workspace_id)
        .bind(permission)
        .bind(shared_by)
        .bind(expires_at)
        .execute(&*self.pool())
        .await
        .map_err(db_err("share session with workspace"))?;

        // Update is_shared flag
        sqlx::query("UPDATE chat_sessions SET is_shared = 1 WHERE id = ?")
            .bind(session_id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("update is_shared"))?;

        Ok(id)
    }

    /// Share a session with a user directly
    pub async fn share_session_with_user(
        &self,
        session_id: &str,
        user_id: &str,
        tenant_id: &str,
        permission: &str,
        shared_by: &str,
        expires_at: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO chat_session_user_shares (id, session_id, shared_with_user_id, shared_with_tenant_id, permission, shared_by, expires_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(session_id, shared_with_user_id) DO UPDATE SET
                permission = excluded.permission,
                expires_at = excluded.expires_at,
                revoked_at = NULL
            "#,
        )
        .bind(&id)
        .bind(session_id)
        .bind(user_id)
        .bind(tenant_id)
        .bind(permission)
        .bind(shared_by)
        .bind(expires_at)
        .execute(&*self.pool())
        .await
        .map_err(db_err("share session with user"))?;

        // Update is_shared flag
        sqlx::query("UPDATE chat_sessions SET is_shared = 1 WHERE id = ?")
            .bind(session_id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("update is_shared"))?;

        Ok(id)
    }

    /// Revoke a session share
    pub async fn revoke_session_share(&self, share_id: &str, share_type: &str) -> Result<()> {
        let table = match share_type {
            "workspace" => "chat_session_shares",
            "user" => "chat_session_user_shares",
            _ => return Err(AosError::Validation("Invalid share type".to_string())),
        };

        let query = format!(
            "UPDATE {} SET revoked_at = datetime('now') WHERE id = ?",
            table
        );
        sqlx::query(&query)
            .bind(share_id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("revoke share"))?;

        Ok(())
    }

    /// Get all shares for a session
    pub async fn get_session_shares(&self, session_id: &str) -> Result<Vec<SessionShare>> {
        // Get workspace shares
        let workspace_shares: Vec<SessionShare> = sqlx::query_as(
            r#"
            SELECT id, session_id, workspace_id, NULL as shared_with_user_id, NULL as shared_with_tenant_id,
                   permission, shared_by, shared_at, expires_at, revoked_at
            FROM chat_session_shares
            WHERE session_id = ? AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > datetime('now'))
            "#,
        )
        .bind(session_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("get workspace shares"))?;

        // Get user shares
        let user_shares: Vec<SessionShare> = sqlx::query_as(
            r#"
            SELECT id, session_id, NULL as workspace_id, shared_with_user_id, shared_with_tenant_id,
                   permission, shared_by, shared_at, expires_at, revoked_at
            FROM chat_session_user_shares
            WHERE session_id = ? AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > datetime('now'))
            "#,
        )
        .bind(session_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("get user shares"))?;

        let mut all_shares = workspace_shares;
        all_shares.extend(user_shares);
        Ok(all_shares)
    }

    /// Get sessions shared with a user
    pub async fn get_sessions_shared_with_user(
        &self,
        user_id: &str,
        tenant_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<ChatSessionWithStatus>> {
        let limit = limit.unwrap_or(50);

        // Get directly shared sessions
        let direct: Vec<ChatSessionWithStatus> = sqlx::query_as(
            r#"
            SELECT cs.id, cs.tenant_id, cs.user_id, cs.stack_id, cs.collection_id, cs.name,
                   cs.created_at, cs.last_activity_at, cs.metadata_json, cs.category_id,
                   cs.status, cs.deleted_at, cs.deleted_by, cs.archived_at, cs.archived_by,
                   cs.archive_reason, cs.description, cs.is_shared
            FROM chat_sessions cs
            JOIN chat_session_user_shares sus ON sus.session_id = cs.id
            WHERE sus.shared_with_user_id = ?
              AND sus.revoked_at IS NULL
              AND (sus.expires_at IS NULL OR sus.expires_at > datetime('now'))
              AND cs.status = 'active'
            ORDER BY cs.last_activity_at DESC
            LIMIT ?
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get directly shared sessions: {}", e))
        })?;

        // Get workspace-shared sessions (via workspace membership)
        let via_workspace: Vec<ChatSessionWithStatus> = sqlx::query_as(
            r#"
            SELECT DISTINCT cs.id, cs.tenant_id, cs.user_id, cs.stack_id, cs.collection_id, cs.name,
                   cs.created_at, cs.last_activity_at, cs.metadata_json, cs.category_id,
                   cs.status, cs.deleted_at, cs.deleted_by, cs.archived_at, cs.archived_by,
                   cs.archive_reason, cs.description, cs.is_shared
            FROM chat_sessions cs
            JOIN chat_session_shares ss ON ss.session_id = cs.id
            JOIN workspace_members wm ON wm.workspace_id = ss.workspace_id
            WHERE wm.user_id = ? AND wm.tenant_id = ?
              AND ss.revoked_at IS NULL
              AND (ss.expires_at IS NULL OR ss.expires_at > datetime('now'))
              AND cs.status = 'active'
            ORDER BY cs.last_activity_at DESC
            LIMIT ?
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&*self.pool())
        .await
        .unwrap_or_default(); // Workspace tables may not exist

        // Combine and deduplicate
        let mut all_sessions = direct;
        let existing_ids: std::collections::HashSet<String> =
            all_sessions.iter().map(|s| s.id.clone()).collect();
        for session in via_workspace {
            if !existing_ids.contains(&session.id) {
                all_sessions.push(session);
            }
        }

        // Sort by last activity and limit
        all_sessions.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));
        all_sessions.truncate(limit as usize);

        Ok(all_sessions)
    }

    /// Check if user has access to a shared session
    pub async fn check_session_share_access(
        &self,
        session_id: &str,
        user_id: &str,
        tenant_id: &str,
    ) -> Result<Option<String>> {
        // Check direct share
        let direct: Option<String> = sqlx::query_scalar(
            r#"
            SELECT permission FROM chat_session_user_shares
            WHERE session_id = ? AND shared_with_user_id = ?
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > datetime('now'))
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(db_err("check direct share"))?;

        if direct.is_some() {
            return Ok(direct);
        }

        // Check workspace share
        let via_workspace: Option<String> = sqlx::query_scalar(
            r#"
            SELECT ss.permission FROM chat_session_shares ss
            JOIN workspace_members wm ON wm.workspace_id = ss.workspace_id
            WHERE ss.session_id = ? AND wm.user_id = ? AND wm.tenant_id = ?
              AND ss.revoked_at IS NULL
              AND (ss.expires_at IS NULL OR ss.expires_at > datetime('now'))
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(tenant_id)
        .fetch_optional(&*self.pool())
        .await
        .unwrap_or(None);

        Ok(via_workspace)
    }

    /// Update session description
    pub async fn update_session_description(
        &self,
        session_id: &str,
        description: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE chat_sessions SET description = ? WHERE id = ?")
            .bind(description)
            .bind(session_id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("update description"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_retrieve_session() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Create tenant
        sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant')")
            .execute(db.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Create session
        let params = CreateChatSessionParams {
            id: "session-1".to_string(),
            tenant_id: "test-tenant".to_string(),
            user_id: Some("user-1".to_string()),
            stack_id: None,
            collection_id: None,
            name: "Test Session".to_string(),
            metadata_json: None,
        };

        db.create_chat_session(params).await?;

        // Retrieve session
        let session = db.get_chat_session("session-1").await?;
        assert!(session.is_some());
        let session = session.unwrap();
        assert_eq!(session.id, "session-1");
        assert_eq!(session.tenant_id, "test-tenant");
        assert_eq!(session.name, "Test Session");

        Ok(())
    }

    #[tokio::test]
    async fn test_add_and_retrieve_messages() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Create tenant and session
        sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant')")
            .execute(db.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        let session_params = CreateChatSessionParams {
            id: "session-1".to_string(),
            tenant_id: "test-tenant".to_string(),
            user_id: None,
            stack_id: None,
            collection_id: None,
            name: "Test Session".to_string(),
            metadata_json: None,
        };
        db.create_chat_session(session_params).await?;

        // Add messages
        let msg1_params = AddMessageParams {
            id: "msg-1".to_string(),
            session_id: "session-1".to_string(),
            role: "user".to_string(),
            content: "Hello".to_string(),
            metadata_json: None,
        };
        db.add_chat_message(msg1_params).await?;

        let msg2_params = AddMessageParams {
            id: "msg-2".to_string(),
            session_id: "session-1".to_string(),
            role: "assistant".to_string(),
            content: "Hi there!".to_string(),
            metadata_json: None,
        };
        db.add_chat_message(msg2_params).await?;

        // Retrieve messages
        let messages = db.get_chat_messages("session-1", None).await?;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].content, "Hi there!");

        Ok(())
    }

    #[tokio::test]
    async fn test_session_traces() -> Result<()> {
        let db = Db::new_in_memory().await?;

        // Create tenant and session
        sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant')")
            .execute(db.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        let session_params = CreateChatSessionParams {
            id: "session-1".to_string(),
            tenant_id: "test-tenant".to_string(),
            user_id: None,
            stack_id: None,
            collection_id: None,
            name: "Test Session".to_string(),
            metadata_json: None,
        };
        db.create_chat_session(session_params).await?;

        // Add traces
        db.add_session_trace("session-1", "router_decision", "decision-1")
            .await?;
        db.add_session_trace("session-1", "adapter", "adapter-1")
            .await?;

        // Retrieve traces
        let traces = db.get_session_traces("session-1").await?;
        assert_eq!(traces.len(), 2);

        // Get summary
        let summary = db.get_session_summary("session-1").await?;
        assert_eq!(summary["session_id"], "session-1");
        assert_eq!(summary["message_count"], 0);

        Ok(())
    }
}
