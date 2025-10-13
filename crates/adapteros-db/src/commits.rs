use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Commit {
    pub id: String,
    pub repo_id: String,
    pub sha: String,
    pub author: String,
    pub date: String,
    pub message: String,
    pub branch: Option<String>,
    pub changed_files_json: String,
    pub impacted_symbols_json: Option<String>,
    pub test_results_json: Option<String>,
    pub ephemeral_adapter_id: Option<String>,
    pub created_at: String,
}

impl Db {
    /// Save commit metadata
    pub async fn save_commit(
        &self,
        repo_id: &str,
        sha: &str,
        author: &str,
        date: &str,
        message: &str,
        branch: Option<&str>,
        changed_files_json: &str,
        impacted_symbols_json: Option<&str>,
        test_results_json: Option<&str>,
        ephemeral_adapter_id: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO commits 
             (id, repo_id, sha, author, date, message, branch, changed_files_json, 
              impacted_symbols_json, test_results_json, ephemeral_adapter_id) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(repo_id, sha) DO UPDATE SET
                author = excluded.author,
                date = excluded.date,
                message = excluded.message,
                branch = excluded.branch,
                changed_files_json = excluded.changed_files_json,
                impacted_symbols_json = excluded.impacted_symbols_json,
                test_results_json = excluded.test_results_json,
                ephemeral_adapter_id = excluded.ephemeral_adapter_id",
        )
        .bind(&id)
        .bind(repo_id)
        .bind(sha)
        .bind(author)
        .bind(date)
        .bind(message)
        .bind(branch)
        .bind(changed_files_json)
        .bind(impacted_symbols_json)
        .bind(test_results_json)
        .bind(ephemeral_adapter_id)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    /// Get commit by repo_id and sha
    pub async fn get_commit(&self, repo_id: &str, sha: &str) -> Result<Option<Commit>> {
        let commit = sqlx::query_as::<_, Commit>(
            "SELECT id, repo_id, sha, author, date, message, branch, changed_files_json, 
                    impacted_symbols_json, test_results_json, ephemeral_adapter_id, created_at 
             FROM commits WHERE repo_id = ? AND sha = ?",
        )
        .bind(repo_id)
        .bind(sha)
        .fetch_optional(self.pool())
        .await?;
        Ok(commit)
    }

    /// List commits by repository
    pub async fn list_commits_by_repo(&self, repo_id: &str) -> Result<Vec<Commit>> {
        let commits = sqlx::query_as::<_, Commit>(
            "SELECT id, repo_id, sha, author, date, message, branch, changed_files_json, 
                    impacted_symbols_json, test_results_json, ephemeral_adapter_id, created_at 
             FROM commits WHERE repo_id = ? ORDER BY date DESC",
        )
        .bind(repo_id)
        .fetch_all(self.pool())
        .await?;
        Ok(commits)
    }
}
