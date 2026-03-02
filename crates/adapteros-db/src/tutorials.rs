use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TutorialStatus {
    pub id: String,
    pub user_id: String,
    pub tutorial_id: String,
    pub completed_at: Option<String>,
    pub dismissed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Db {
    /// Get tutorial status for a user
    pub async fn get_tutorial_status(
        &self,
        user_id: &str,
        tutorial_id: &str,
    ) -> Result<Option<TutorialStatus>> {
        let status = sqlx::query_as::<_, TutorialStatus>(
            r#"
            SELECT id, user_id, tutorial_id, completed_at, dismissed_at, created_at, updated_at
            FROM tutorial_statuses
            WHERE user_id = ? AND tutorial_id = ?
            "#,
        )
        .bind(user_id)
        .bind(tutorial_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(status)
    }

    /// List all tutorial statuses for a user
    pub async fn list_user_tutorial_statuses(&self, user_id: &str) -> Result<Vec<TutorialStatus>> {
        let statuses = sqlx::query_as::<_, TutorialStatus>(
            r#"
            SELECT id, user_id, tutorial_id, completed_at, dismissed_at, created_at, updated_at
            FROM tutorial_statuses
            WHERE user_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(statuses)
    }

    /// Mark tutorial as completed
    pub async fn mark_tutorial_completed(&self, user_id: &str, tutorial_id: &str) -> Result<()> {
        // Try to update existing record
        let rows_affected = sqlx::query(
            r#"
            UPDATE tutorial_statuses
            SET completed_at = datetime('now'), updated_at = datetime('now')
            WHERE user_id = ? AND tutorial_id = ?
            "#,
        )
        .bind(user_id)
        .bind(tutorial_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // If no rows affected, insert new record
        if rows_affected.rows_affected() == 0 {
            sqlx::query(
                r#"
                INSERT INTO tutorial_statuses (user_id, tutorial_id, completed_at, created_at, updated_at)
                VALUES (?, ?, datetime('now'), datetime('now'), datetime('now'))
                "#,
            )
            .bind(user_id)
            .bind(tutorial_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        }

        Ok(())
    }

    /// Unmark tutorial as completed (delete completion status)
    pub async fn unmark_tutorial_completed(&self, user_id: &str, tutorial_id: &str) -> Result<()> {
        // Check if record exists
        let existing = self.get_tutorial_status(user_id, tutorial_id).await?;

        if let Some(status) = existing {
            // If dismissed_at is set, keep the record but clear completed_at
            if status.dismissed_at.is_some() {
                sqlx::query(
                    r#"
                    UPDATE tutorial_statuses
                    SET completed_at = NULL, updated_at = datetime('now')
                    WHERE user_id = ? AND tutorial_id = ?
                    "#,
                )
                .bind(user_id)
                .bind(tutorial_id)
                .execute(self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
            } else {
                // Delete the record if no dismissed_at
                sqlx::query(
                    r#"
                    DELETE FROM tutorial_statuses
                    WHERE user_id = ? AND tutorial_id = ?
                    "#,
                )
                .bind(user_id)
                .bind(tutorial_id)
                .execute(self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Mark tutorial as dismissed
    pub async fn mark_tutorial_dismissed(&self, user_id: &str, tutorial_id: &str) -> Result<()> {
        // Try to update existing record
        let rows_affected = sqlx::query(
            r#"
            UPDATE tutorial_statuses
            SET dismissed_at = datetime('now'), updated_at = datetime('now')
            WHERE user_id = ? AND tutorial_id = ?
            "#,
        )
        .bind(user_id)
        .bind(tutorial_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // If no rows affected, insert new record
        if rows_affected.rows_affected() == 0 {
            sqlx::query(
                r#"
                INSERT INTO tutorial_statuses (user_id, tutorial_id, dismissed_at, created_at, updated_at)
                VALUES (?, ?, datetime('now'), datetime('now'), datetime('now'))
                "#,
            )
            .bind(user_id)
            .bind(tutorial_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        }

        Ok(())
    }

    /// Unmark tutorial as dismissed (delete dismissal status)
    pub async fn unmark_tutorial_dismissed(&self, user_id: &str, tutorial_id: &str) -> Result<()> {
        // Check if record exists
        let existing = self.get_tutorial_status(user_id, tutorial_id).await?;

        if let Some(status) = existing {
            // If completed_at is set, keep the record but clear dismissed_at
            if status.completed_at.is_some() {
                sqlx::query(
                    r#"
                    UPDATE tutorial_statuses
                    SET dismissed_at = NULL, updated_at = datetime('now')
                    WHERE user_id = ? AND tutorial_id = ?
                    "#,
                )
                .bind(user_id)
                .bind(tutorial_id)
                .execute(self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
            } else {
                // Delete the record if no completed_at
                sqlx::query(
                    r#"
                    DELETE FROM tutorial_statuses
                    WHERE user_id = ? AND tutorial_id = ?
                    "#,
                )
                .bind(user_id)
                .bind(tutorial_id)
                .execute(self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
            }
        }

        Ok(())
    }
}
