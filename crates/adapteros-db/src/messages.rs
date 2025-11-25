use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub id: String,
    pub workspace_id: String,
    pub from_user_id: String,
    pub from_tenant_id: String,
    pub content: String,
    pub thread_id: Option<String>,
    pub created_at: String,
    pub edited_at: Option<String>,
}

impl Db {
    pub async fn create_message(
        &self,
        workspace_id: &str,
        from_user_id: &str,
        from_tenant_id: &str,
        content: &str,
        thread_id: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            r#"
            INSERT INTO messages (id, workspace_id, from_user_id, from_tenant_id, content, thread_id)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(from_user_id)
        .bind(from_tenant_id)
        .bind(content)
        .bind(thread_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create message: {}", e)))?;
        Ok(id)
    }

    pub async fn get_message(&self, id: &str) -> Result<Option<Message>> {
        let message = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, workspace_id, from_user_id, from_tenant_id, content, thread_id, created_at, edited_at
            FROM messages
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get message: {}", e)))?;
        Ok(message)
    }

    pub async fn list_workspace_messages(
        &self,
        workspace_id: &str,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Message>> {
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);
        let messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, workspace_id, from_user_id, from_tenant_id, content, thread_id, created_at, edited_at
            FROM messages
            WHERE workspace_id = ? AND thread_id IS NULL
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(workspace_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list workspace messages: {}", e)))?;
        Ok(messages)
    }

    pub async fn list_message_thread(
        &self,
        thread_id: &str,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Message>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);
        let messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, workspace_id, from_user_id, from_tenant_id, content, thread_id, created_at, edited_at
            FROM messages
            WHERE thread_id = ? OR id = ?
            ORDER BY created_at ASC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(thread_id)
        .bind(thread_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list message thread: {}", e)))?;
        Ok(messages)
    }

    pub async fn edit_message(&self, id: &str, content: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE messages
            SET content = ?, edited_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(content)
        .bind(id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to edit message: {}", e)))?;
        Ok(())
    }

    pub async fn delete_message(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM messages WHERE id = ?")
            .bind(id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete message: {}", e)))?;
        Ok(())
    }

    pub async fn get_recent_workspace_messages(
        &self,
        workspace_id: &str,
        since: Option<&str>,
    ) -> Result<Vec<Message>> {
        let messages = if let Some(since_ts) = since {
            sqlx::query_as::<_, Message>(
                r#"
                SELECT id, workspace_id, from_user_id, from_tenant_id, content, thread_id, created_at, edited_at
                FROM messages
                WHERE workspace_id = ? AND created_at > ?
                ORDER BY created_at ASC
                "#,
            )
            .bind(workspace_id)
            .bind(since_ts)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to get recent messages: {}", e)))?
        } else {
            sqlx::query_as::<_, Message>(
                r#"
                SELECT id, workspace_id, from_user_id, from_tenant_id, content, thread_id, created_at, edited_at
                FROM messages
                WHERE workspace_id = ?
                ORDER BY created_at DESC
                LIMIT 50
                "#,
            )
            .bind(workspace_id)
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to get recent messages: {}", e)))?
        };
        Ok(messages)
    }
}
