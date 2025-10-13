//! Git integration database methods

use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Git session record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GitSession {
    pub id: String,
    pub adapter_id: String,
    pub repo_id: String,
    pub branch_name: String,
    pub base_commit_sha: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: String,
    pub merge_commit_sha: Option<String>,
}

/// File change event record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FileChangeEvent {
    pub id: String,
    pub adapter_id: Option<String>,
    pub repo_id: String,
    pub file_path: String,
    pub change_type: String,
    pub timestamp: String,
    pub broadcasted: i64,
}

/// Git commit record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GitAdapterCommit {
    pub id: String,
    pub session_id: String,
    pub commit_sha: String,
    pub message: String,
    pub files_changed: i64,
    pub created_at: String,
}

impl Db {
    /// Create a new Git session
    pub async fn create_git_session(
        &self,
        id: &str,
        adapter_id: &str,
        repo_id: &str,
        branch_name: &str,
        base_commit_sha: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO adapter_git_sessions (
                id, adapter_id, repo_id, branch_name, base_commit_sha
            ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(adapter_id)
        .bind(repo_id)
        .bind(branch_name)
        .bind(base_commit_sha)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Update Git session status
    pub async fn update_git_session_status(
        &self,
        session_id: &str,
        status: &str,
        merge_commit_sha: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE adapter_git_sessions 
             SET status = ?, ended_at = datetime('now'), merge_commit_sha = ?
             WHERE id = ?",
        )
        .bind(status)
        .bind(merge_commit_sha)
        .bind(session_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Get Git session by ID
    pub async fn get_git_session(&self, session_id: &str) -> Result<Option<GitSession>> {
        let session =
            sqlx::query_as::<_, GitSession>("SELECT * FROM adapter_git_sessions WHERE id = ?")
                .bind(session_id)
                .fetch_optional(self.pool())
                .await?;
        Ok(session)
    }

    /// List active Git sessions
    pub async fn list_active_git_sessions(&self) -> Result<Vec<GitSession>> {
        let sessions = sqlx::query_as::<_, GitSession>(
            "SELECT * FROM adapter_git_sessions WHERE status = 'active' ORDER BY started_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(sessions)
    }

    /// List Git sessions for an adapter
    pub async fn list_adapter_git_sessions(&self, adapter_id: &str) -> Result<Vec<GitSession>> {
        let sessions = sqlx::query_as::<_, GitSession>(
            "SELECT * FROM adapter_git_sessions WHERE adapter_id = ? ORDER BY started_at DESC",
        )
        .bind(adapter_id)
        .fetch_all(self.pool())
        .await?;
        Ok(sessions)
    }

    /// Create a file change event
    pub async fn create_file_change_event(
        &self,
        id: &str,
        adapter_id: Option<&str>,
        repo_id: &str,
        file_path: &str,
        change_type: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO file_change_events (
                id, adapter_id, repo_id, file_path, change_type
            ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(adapter_id)
        .bind(repo_id)
        .bind(file_path)
        .bind(change_type)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Mark file change events as broadcasted
    pub async fn mark_events_broadcasted(&self, event_ids: &[String]) -> Result<()> {
        if event_ids.is_empty() {
            return Ok(());
        }

        let placeholders = event_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "UPDATE file_change_events SET broadcasted = 1 WHERE id IN ({})",
            placeholders
        );

        let mut query_builder = sqlx::query(&query);
        for id in event_ids {
            query_builder = query_builder.bind(id);
        }

        query_builder.execute(self.pool()).await?;
        Ok(())
    }

    /// Get unbroadcasted file change events
    pub async fn get_unbroadcasted_events(&self, limit: i64) -> Result<Vec<FileChangeEvent>> {
        let events = sqlx::query_as::<_, FileChangeEvent>(
            "SELECT * FROM file_change_events 
             WHERE broadcasted = 0 
             ORDER BY timestamp ASC 
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(events)
    }

    /// Record a Git commit
    pub async fn create_git_adapter_commit(
        &self,
        id: &str,
        session_id: &str,
        commit_sha: &str,
        message: &str,
        files_changed: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO git_adapter_commits (
                id, session_id, commit_sha, message, files_changed
            ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(session_id)
        .bind(commit_sha)
        .bind(message)
        .bind(files_changed)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// List commits for a Git session
    pub async fn list_session_commits(&self, session_id: &str) -> Result<Vec<GitAdapterCommit>> {
        let commits = sqlx::query_as::<_, GitAdapterCommit>(
            "SELECT * FROM git_adapter_commits WHERE session_id = ? ORDER BY created_at DESC",
        )
        .bind(session_id)
        .fetch_all(self.pool())
        .await?;
        Ok(commits)
    }
}
