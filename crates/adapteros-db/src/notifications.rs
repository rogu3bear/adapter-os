use crate::new_id;
use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationType {
    #[serde(rename = "alert")]
    Alert,
    #[serde(rename = "message")]
    Message,
    #[serde(rename = "mention")]
    Mention,
    #[serde(rename = "activity")]
    Activity,
    #[serde(rename = "system")]
    System,
}

impl std::fmt::Display for NotificationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotificationType::Alert => write!(f, "alert"),
            NotificationType::Message => write!(f, "message"),
            NotificationType::Mention => write!(f, "mention"),
            NotificationType::Activity => write!(f, "activity"),
            NotificationType::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for NotificationType {
    type Err = adapteros_core::AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "alert" => Ok(NotificationType::Alert),
            "message" => Ok(NotificationType::Message),
            "mention" => Ok(NotificationType::Mention),
            "activity" => Ok(NotificationType::Activity),
            "system" => Ok(NotificationType::System),
            _ => Err(AosError::Parse(format!("invalid notification type: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    pub workspace_id: Option<String>,
    pub type_: String, // Using type_ to avoid Rust keyword conflict
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub title: String,
    pub content: Option<String>,
    pub read_at: Option<String>,
    pub created_at: String,
}

impl Db {
    pub async fn create_notification(
        &self,
        user_id: &str,
        workspace_id: Option<&str>,
        type_: NotificationType,
        target_type: Option<&str>,
        target_id: Option<&str>,
        title: &str,
        content: Option<&str>,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Evt);
        let type_str = type_.to_string();
        sqlx::query(
            r#"
            INSERT INTO notifications (id, user_id, workspace_id, type, target_type, target_id, title, content)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(user_id)
        .bind(workspace_id)
        .bind(&type_str)
        .bind(target_type)
        .bind(target_id)
        .bind(title)
        .bind(content)
        .execute(self.pool_result()?)
        .await?;
        Ok(id)
    }

    pub async fn get_notification(&self, id: &str) -> Result<Option<Notification>> {
        let notification = sqlx::query_as::<_, Notification>(
            r#"
            SELECT id, user_id, workspace_id, type as type_, target_type, target_id, title, content, read_at, created_at
            FROM notifications
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool_result()?)
        .await?;
        Ok(notification)
    }

    pub async fn list_user_notifications(
        &self,
        user_id: &str,
        workspace_id: Option<&str>,
        type_filter: Option<NotificationType>,
        unread_only: bool,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Notification>> {
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);
        let mut query = String::from(
            r#"
            SELECT id, user_id, workspace_id, type as type_, target_type, target_id, title, content, read_at, created_at
            FROM notifications
            WHERE user_id = ?
            "#,
        );

        if workspace_id.is_some() {
            query.push_str(" AND workspace_id = ?");
        }

        if unread_only {
            query.push_str(" AND read_at IS NULL");
        }

        if type_filter.is_some() {
            query.push_str(" AND type = ?");
        }

        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

        let mut query_builder = sqlx::query_as::<_, Notification>(&query).bind(user_id);

        if let Some(ws_id) = workspace_id {
            query_builder = query_builder.bind(ws_id);
        }

        if let Some(type_filter_val) = type_filter {
            query_builder = query_builder.bind(type_filter_val.to_string());
        }

        query_builder = query_builder.bind(limit).bind(offset);

        let notifications = query_builder.fetch_all(self.pool_result()?).await?;
        Ok(notifications)
    }

    pub async fn mark_notification_read(&self, id: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE notifications
            SET read_at = datetime('now')
            WHERE id = ? AND read_at IS NULL
            "#,
        )
        .bind(id)
        .execute(self.pool_result()?)
        .await?;
        Ok(())
    }

    pub async fn mark_all_notifications_read(
        &self,
        user_id: &str,
        workspace_id: Option<&str>,
    ) -> Result<u64> {
        let result = if let Some(ws_id) = workspace_id {
            sqlx::query(
                r#"
                UPDATE notifications
                SET read_at = datetime('now')
                WHERE user_id = ? AND workspace_id = ? AND read_at IS NULL
                "#,
            )
            .bind(user_id)
            .bind(ws_id)
            .execute(self.pool_result()?)
            .await?
        } else {
            sqlx::query(
                r#"
                UPDATE notifications
                SET read_at = datetime('now')
                WHERE user_id = ? AND read_at IS NULL
                "#,
            )
            .bind(user_id)
            .execute(self.pool_result()?)
            .await?
        };
        Ok(result.rows_affected())
    }

    pub async fn get_unread_count(&self, user_id: &str, workspace_id: Option<&str>) -> Result<i64> {
        let count: (i64,) = if let Some(ws_id) = workspace_id {
            sqlx::query_as(
                r#"
                SELECT COUNT(*) as count
                FROM notifications
                WHERE user_id = ? AND workspace_id = ? AND read_at IS NULL
                "#,
            )
            .bind(user_id)
            .bind(ws_id)
            .fetch_one(self.pool_result()?)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT COUNT(*) as count
                FROM notifications
                WHERE user_id = ? AND read_at IS NULL
                "#,
            )
            .bind(user_id)
            .fetch_one(self.pool_result()?)
            .await?
        };
        Ok(count.0)
    }

    pub async fn delete_notification(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM notifications WHERE id = ?")
            .bind(id)
            .execute(self.pool_result()?)
            .await?;
        Ok(())
    }

    /// List notifications for a user created after a given timestamp (delta mode for SSE streaming).
    /// Returns notifications ordered by created_at ASC so clients process them in chronological order.
    pub async fn list_user_notifications_since(
        &self,
        user_id: &str,
        since_timestamp: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<Notification>> {
        let limit = limit.unwrap_or(50);

        let notifications = if let Some(since_ts) = since_timestamp {
            sqlx::query_as::<_, Notification>(
                r#"
                SELECT id, user_id, workspace_id, type as type_, target_type, target_id, title, content, read_at, created_at
                FROM notifications
                WHERE user_id = ? AND created_at > ?
                ORDER BY created_at ASC
                LIMIT ?
                "#,
            )
            .bind(user_id)
            .bind(since_ts)
            .bind(limit)
            .fetch_all(self.pool_result()?)
            .await?
        } else {
            // No since_timestamp: return most recent notifications
            sqlx::query_as::<_, Notification>(
                r#"
                SELECT id, user_id, workspace_id, type as type_, target_type, target_id, title, content, read_at, created_at
                FROM notifications
                WHERE user_id = ?
                ORDER BY created_at DESC
                LIMIT ?
                "#,
            )
            .bind(user_id)
            .bind(limit)
            .fetch_all(self.pool_result()?)
            .await?
        };

        Ok(notifications)
    }
}
