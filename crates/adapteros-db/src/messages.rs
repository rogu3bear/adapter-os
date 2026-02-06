use crate::messages_kv::{MessageKv, MessageKvRepository};
use crate::new_id;
use crate::{Db, StorageMode};
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use chrono::Utc;
use serde::{Deserialize, Serialize};

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

impl From<MessageKv> for Message {
    fn from(kv: MessageKv) -> Self {
        Self {
            id: kv.id,
            workspace_id: kv.workspace_id,
            from_user_id: kv.from_user_id,
            from_tenant_id: kv.from_tenant_id,
            content: kv.content,
            thread_id: kv.thread_id,
            created_at: kv.created_at,
            edited_at: kv.edited_at,
        }
    }
}

impl From<Message> for MessageKv {
    fn from(msg: Message) -> Self {
        Self {
            id: msg.id,
            workspace_id: msg.workspace_id,
            from_user_id: msg.from_user_id,
            from_tenant_id: msg.from_tenant_id,
            content: msg.content,
            thread_id: msg.thread_id,
            created_at: msg.created_at,
            edited_at: msg.edited_at,
        }
    }
}

impl Db {
    fn get_message_kv_repo(&self) -> Option<MessageKvRepository> {
        if (self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv())
            && self.has_kv_backend()
        {
            self.kv_backend()
                .map(|kv| MessageKvRepository::new(kv.backend().clone()))
        } else {
            None
        }
    }

    async fn sql_get_message(&self, id: &str) -> Result<Option<Message>> {
        let Some(pool) = self.pool_opt() else {
            return Ok(None);
        };

        sqlx::query_as::<_, Message>(
            r#"
            SELECT id, workspace_id, from_user_id, from_tenant_id, content, thread_id, created_at, edited_at
            FROM messages
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get message: {}", e)))
    }

    async fn sql_list_workspace_messages(
        &self,
        workspace_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Message>> {
        let Some(pool) = self.pool_opt() else {
            return Ok(Vec::new());
        };

        sqlx::query_as::<_, Message>(
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
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list workspace messages: {}", e)))
    }

    async fn sql_list_thread_messages(
        &self,
        thread_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Message>> {
        let Some(pool) = self.pool_opt() else {
            return Ok(Vec::new());
        };

        sqlx::query_as::<_, Message>(
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
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list message thread: {}", e)))
    }

    pub async fn create_message(
        &self,
        workspace_id: &str,
        from_user_id: &str,
        from_tenant_id: &str,
        content: &str,
        thread_id: Option<&str>,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Msg);
        let mut canonical: Option<Message> = None;

        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
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
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to create message: {}", e)))?;

                canonical = self.sql_get_message(&id).await?;
            } else if !self.storage_mode().write_to_kv() {
                return Err(AosError::Database(
                    "SQL backend unavailable for message creation".to_string(),
                ));
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_message_kv_repo() {
                let mut kv_record = if let Some(msg) = canonical.clone() {
                    MessageKv::from(msg)
                } else {
                    let mut rec = repo.new_message_record(
                        workspace_id,
                        from_user_id,
                        from_tenant_id,
                        content,
                        thread_id,
                    );
                    rec.id = id.clone();
                    rec
                };
                // Ensure thread_id formatting matches Option<&str>
                kv_record.thread_id = thread_id.map(|s| s.to_string()).or(kv_record.thread_id);

                if let Err(e) = repo.put(&kv_record).await {
                    self.record_kv_write_fallback("messages.create");
                    return Err(e);
                }
            } else {
                return Err(AosError::Database(
                    "KV backend unavailable for message creation".to_string(),
                ));
            }
        }

        Ok(id)
    }

    pub async fn get_message(&self, id: &str) -> Result<Option<Message>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_message_kv_repo() {
                if let Some(kv) = repo.get(id).await? {
                    return Ok(Some(kv.into()));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
            self.record_kv_read_fallback("messages.get");
        }

        self.sql_get_message(id).await
    }

    pub async fn list_workspace_messages(
        &self,
        workspace_id: &str,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Message>> {
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_message_kv_repo() {
                let messages = repo
                    .list_workspace_messages(workspace_id, limit, offset)
                    .await?
                    .into_iter()
                    .map(Message::from)
                    .collect();
                return Ok(messages);
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
            self.record_kv_read_fallback("messages.list_workspace");
        }

        self.sql_list_workspace_messages(workspace_id, limit, offset)
            .await
    }

    pub async fn list_message_thread(
        &self,
        thread_id: &str,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Message>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_message_kv_repo() {
                let messages = repo
                    .list_thread_messages(thread_id, limit, offset)
                    .await?
                    .into_iter()
                    .map(Message::from)
                    .collect();
                return Ok(messages);
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
            self.record_kv_read_fallback("messages.list_thread");
        }

        self.sql_list_thread_messages(thread_id, limit, offset)
            .await
    }

    pub async fn edit_message(&self, id: &str, content: &str) -> Result<()> {
        let mut canonical: Option<Message> = None;

        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query(
                    r#"
                    UPDATE messages
                    SET content = ?, edited_at = datetime('now')
                    WHERE id = ?
                    "#,
                )
                .bind(content)
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to edit message: {}", e)))?;

                canonical = self.sql_get_message(id).await?;
            } else if !self.storage_mode().write_to_kv() {
                return Err(AosError::Database(
                    "SQL backend unavailable for edit_message".to_string(),
                ));
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_message_kv_repo() {
                let mut record = if let Some(msg) = canonical.clone() {
                    MessageKv::from(msg)
                } else if let Some(existing) = repo.get(id).await? {
                    existing
                } else {
                    return Ok(());
                };

                record.content = content.to_string();
                record.edited_at = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());

                if let Err(e) = repo.put(&record).await {
                    self.record_kv_write_fallback("messages.edit");
                    return Err(e);
                }
            } else {
                return Err(AosError::Database(
                    "KV backend unavailable for edit_message".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub async fn delete_message(&self, id: &str) -> Result<()> {
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query("DELETE FROM messages WHERE id = ?")
                    .bind(id)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to delete message: {}", e)))?;
            } else if !self.storage_mode().write_to_kv() {
                return Err(AosError::Database(
                    "SQL backend unavailable for delete_message".to_string(),
                ));
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_message_kv_repo() {
                if let Err(e) = repo.delete(id).await {
                    self.record_kv_write_fallback("messages.delete");
                    return Err(e);
                }
            } else {
                return Err(AosError::Database(
                    "KV backend unavailable for delete_message".to_string(),
                ));
            }
        }
        Ok(())
    }

    pub async fn get_recent_workspace_messages(
        &self,
        workspace_id: &str,
        since: Option<&str>,
    ) -> Result<Vec<Message>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_message_kv_repo() {
                let mut kv_messages = repo
                    .list_recent_workspace(workspace_id, since)
                    .await?
                    .into_iter()
                    .map(Message::from)
                    .collect::<Vec<_>>();

                if since.is_none() {
                    kv_messages.sort_by(|a, b| {
                        b.created_at
                            .cmp(&a.created_at)
                            .then_with(|| b.id.cmp(&a.id))
                    });
                    if kv_messages.len() > 50 {
                        kv_messages.truncate(50);
                    }
                }

                return Ok(kv_messages);
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
            self.record_kv_read_fallback("messages.recent");
        }

        let Some(pool) = self.pool_opt() else {
            return Ok(Vec::new());
        };

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
            .fetch_all(pool)
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
            .fetch_all(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get recent messages: {}", e)))?
        };
        Ok(messages)
    }
}
