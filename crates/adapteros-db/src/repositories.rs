use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Repository {
    pub id: String,
    pub repo_id: String,
    pub path: String,
    pub languages: String, // JSON array
    pub default_branch: String,
    pub status: String,
    pub frameworks_json: Option<String>,
    pub file_count: Option<i64>,
    pub symbol_count: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

impl Db {
    /// Register a new repository
    pub async fn register_repository(
        &self,
        repo_id: &str,
        path: &str,
        languages: &str, // JSON string
        default_branch: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO repositories (id, repo_id, path, languages, default_branch, status) 
             VALUES (?, ?, ?, ?, ?, 'registered')",
        )
        .bind(&id)
        .bind(repo_id)
        .bind(path)
        .bind(languages)
        .bind(default_branch)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    /// Get a repository by repo_id
    pub async fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>> {
        let repo = sqlx::query_as::<_, Repository>(
            "SELECT id, repo_id, path, languages, default_branch, status, frameworks_json, 
                    file_count, symbol_count, created_at, updated_at 
             FROM repositories WHERE repo_id = ?",
        )
        .bind(repo_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(repo)
    }

    /// List all repositories
    pub async fn list_repositories(&self) -> Result<Vec<Repository>> {
        let repos = sqlx::query_as::<_, Repository>(
            "SELECT id, repo_id, path, languages, default_branch, status, frameworks_json, 
                    file_count, symbol_count, created_at, updated_at 
             FROM repositories ORDER BY created_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(repos)
    }

    /// Update repository statistics
    pub async fn update_repository_stats(
        &self,
        repo_id: &str,
        file_count: i64,
        symbol_count: i64,
        frameworks_json: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE repositories 
             SET file_count = ?, symbol_count = ?, frameworks_json = ?, 
                 updated_at = datetime('now') 
             WHERE repo_id = ?",
        )
        .bind(file_count)
        .bind(symbol_count)
        .bind(frameworks_json)
        .bind(repo_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Update repository status
    pub async fn update_repository_status(&self, repo_id: &str, status: &str) -> Result<()> {
        sqlx::query(
            "UPDATE repositories SET status = ?, updated_at = datetime('now') WHERE repo_id = ?",
        )
        .bind(status)
        .bind(repo_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
