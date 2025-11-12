use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "alert" => Ok(NotificationType::Alert),
            "message" => Ok(NotificationType::Message),
            "mention" => Ok(NotificationType::Mention),
            "activity" => Ok(NotificationType::Activity),
            "system" => Ok(NotificationType::System),
            _ => Err(anyhow::anyhow!("invalid notification type: {}", s)),
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

#[derive(Debug)]
pub struct NotificationParams {
    pub user_id: String,
    pub workspace_id: Option<String>,
    pub type_: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub title: String,
    pub content: Option<String>,
}

impl Db {
    pub async fn create_notification(&self, params: NotificationParams) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            r#"
            INSERT INTO notifications (id, user_id, workspace_id, type, target_type, target_id, title, content, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&params.user_id)
        .bind(&params.workspace_id)
        .bind(&params.type_)
        .bind(&params.target_type)
        .bind(&params.target_id)
        .bind(&params.title)
        .bind(&params.content)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create notification: {}", e)))?;

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
        .fetch_optional(self.pool())
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

        let notifications = query_builder.fetch_all(self.pool()).await?;
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
        .execute(self.pool())
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
            .execute(self.pool())
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
            .execute(self.pool())
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
            .fetch_one(self.pool())
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
            .fetch_one(self.pool())
            .await?
        };
        Ok(count.0)
    }

    pub async fn delete_notification(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM notifications WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
