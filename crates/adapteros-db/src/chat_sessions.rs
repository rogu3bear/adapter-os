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
        .map_err(|e| AosError::Database(format!("Failed to create chat session: {}", e)))?;

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
        .map_err(|e| AosError::Database(format!("Failed to list chat sessions: {}", e)))?;

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
        .map_err(|e| AosError::Database(format!("Failed to get chat session: {}", e)))?;

        Ok(session)
    }

    /// Update session activity timestamp
    ///
    /// Should be called whenever a message is added or other activity occurs
    ///
    /// # Arguments
    /// * `session_id` - The session ID
    pub async fn update_session_activity(&self, session_id: &str) -> Result<()> {
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
        .map_err(|e| AosError::Database(format!("Failed to update session activity: {}", e)))?;

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
        .map_err(|e| AosError::Database(format!("Failed to update session collection: {}", e)))?;

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
        .map_err(|e| AosError::Database(format!("Failed to add chat message: {}", e)))?;

        // Update session activity
        self.update_session_activity(&params.session_id).await?;

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
        .map_err(|e| AosError::Database(format!("Failed to get chat messages: {}", e)))?;

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
        .map_err(|e| AosError::Database(format!("Failed to add session trace: {}", e)))?;

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
        .map_err(|e| AosError::Database(format!("Failed to get session traces: {}", e)))?;

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
                .map_err(|e| AosError::Database(format!("Failed to count messages: {}", e)))?;

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
        .map_err(|e| AosError::Database(format!("Failed to count traces: {}", e)))?;

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
            .map_err(|e| AosError::Database(format!("Failed to delete chat session: {}", e)))?;

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
