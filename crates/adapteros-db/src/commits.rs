use crate::new_id;
use crate::Db;
use adapteros_id::IdPrefix;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// Builder for creating commit metadata parameters
#[derive(Debug, Default)]
pub struct CommitBuilder {
    repo_id: Option<String>,
    sha: Option<String>,
    author: Option<String>,
    date: Option<String>,
    message: Option<String>,
    branch: Option<String>,
    changed_files_json: Option<String>,
    impacted_symbols_json: Option<String>,
    test_results_json: Option<String>,
    ephemeral_adapter_id: Option<String>,
}

/// Parameters for commit metadata saving
#[derive(Debug)]
pub struct CommitParams {
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
}

impl CommitBuilder {
    /// Create a new commit builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the repository ID (required)
    pub fn repo_id(mut self, repo_id: impl Into<String>) -> Self {
        self.repo_id = Some(repo_id.into());
        self
    }

    /// Set the commit SHA (required)
    pub fn sha(mut self, sha: impl Into<String>) -> Self {
        self.sha = Some(sha.into());
        self
    }

    /// Set the commit author (required)
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Set the commit date (required)
    pub fn date(mut self, date: impl Into<String>) -> Self {
        self.date = Some(date.into());
        self
    }

    /// Set the commit message (required)
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Set the branch name (optional)
    pub fn branch(mut self, branch: Option<impl Into<String>>) -> Self {
        self.branch = branch.map(|s| s.into());
        self
    }

    /// Set the changed files JSON (required)
    pub fn changed_files_json(mut self, changed_files_json: impl Into<String>) -> Self {
        self.changed_files_json = Some(changed_files_json.into());
        self
    }

    /// Set the impacted symbols JSON (optional)
    pub fn impacted_symbols_json(
        mut self,
        impacted_symbols_json: Option<impl Into<String>>,
    ) -> Self {
        self.impacted_symbols_json = impacted_symbols_json.map(|s| s.into());
        self
    }

    /// Set the test results JSON (optional)
    pub fn test_results_json(mut self, test_results_json: Option<impl Into<String>>) -> Self {
        self.test_results_json = test_results_json.map(|s| s.into());
        self
    }

    /// Set the ephemeral adapter ID (optional)
    pub fn ephemeral_adapter_id(mut self, ephemeral_adapter_id: Option<impl Into<String>>) -> Self {
        self.ephemeral_adapter_id = ephemeral_adapter_id.map(|s| s.into());
        self
    }

    /// Build the commit parameters
    pub fn build(self) -> Result<CommitParams> {
        Ok(CommitParams {
            repo_id: self.repo_id.ok_or_else(|| anyhow!("repo_id is required"))?,
            sha: self.sha.ok_or_else(|| anyhow!("sha is required"))?,
            author: self.author.ok_or_else(|| anyhow!("author is required"))?,
            date: self.date.ok_or_else(|| anyhow!("date is required"))?,
            message: self.message.ok_or_else(|| anyhow!("message is required"))?,
            branch: self.branch,
            changed_files_json: self
                .changed_files_json
                .ok_or_else(|| anyhow!("changed_files_json is required"))?,
            impacted_symbols_json: self.impacted_symbols_json,
            test_results_json: self.test_results_json,
            ephemeral_adapter_id: self.ephemeral_adapter_id,
        })
    }
}

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
    ///
    /// Use [`CommitBuilder`] to construct complex parameter sets:
    /// ```no_run
    /// use adapteros_db::commits::CommitBuilder;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let params = CommitBuilder::new()
    ///     .repo_id("github.com/org/repo")
    ///     .sha("a1b2c3d4...")
    ///     .author("John Doe")
    ///     .date("2025-10-31T12:00:00Z")
    ///     .message("Add new feature")
    ///     .branch(Some("main"))
    ///     .changed_files_json(r#"["src/main.rs", "src/lib.rs"]"#)
    ///     .impacted_symbols_json(Some(r#"{"functions": ["main"]}"#))
    ///     .build()
    ///     .expect("required fields");
    /// db.save_commit(params).await.expect("commit saved");
    /// # }
    /// ```
    pub async fn save_commit(&self, params: CommitParams) -> Result<String> {
        let id = new_id(IdPrefix::Ver);
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
        .bind(&params.repo_id)
        .bind(&params.sha)
        .bind(&params.author)
        .bind(&params.date)
        .bind(&params.message)
        .bind(&params.branch)
        .bind(&params.changed_files_json)
        .bind(&params.impacted_symbols_json)
        .bind(&params.test_results_json)
        .bind(&params.ephemeral_adapter_id)
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

    /// List commits by repository with optional branch filtering and limit
    pub async fn list_commits(
        &self,
        repo_id: Option<&str>,
        branch: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Commit>> {
        let commits = match (repo_id, branch) {
            (Some(repo_id), Some(branch)) => {
                sqlx::query_as::<_, Commit>(
                    "SELECT id, repo_id, sha, author, date, message, branch, changed_files_json,
                            impacted_symbols_json, test_results_json, ephemeral_adapter_id, created_at
                     FROM commits WHERE repo_id = ? AND branch = ?
                     ORDER BY date DESC LIMIT ?",
                )
                .bind(repo_id)
                .bind(branch)
                .bind(limit as i64)
                .fetch_all(self.pool())
                .await?
            }
            (Some(repo_id), None) => {
                sqlx::query_as::<_, Commit>(
                    "SELECT id, repo_id, sha, author, date, message, branch, changed_files_json,
                            impacted_symbols_json, test_results_json, ephemeral_adapter_id, created_at
                     FROM commits WHERE repo_id = ?
                     ORDER BY date DESC LIMIT ?",
                )
                .bind(repo_id)
                .bind(limit as i64)
                .fetch_all(self.pool())
                .await?
            }
            (None, Some(branch)) => {
                sqlx::query_as::<_, Commit>(
                    "SELECT id, repo_id, sha, author, date, message, branch, changed_files_json,
                            impacted_symbols_json, test_results_json, ephemeral_adapter_id, created_at
                     FROM commits WHERE branch = ?
                     ORDER BY date DESC LIMIT ?",
                )
                .bind(branch)
                .bind(limit as i64)
                .fetch_all(self.pool())
                .await?
            }
            (None, None) => {
                sqlx::query_as::<_, Commit>(
                    "SELECT id, repo_id, sha, author, date, message, branch, changed_files_json,
                            impacted_symbols_json, test_results_json, ephemeral_adapter_id, created_at
                     FROM commits
                     ORDER BY date DESC LIMIT ?",
                )
                .bind(limit as i64)
                .fetch_all(self.pool())
                .await?
            }
        };

        Ok(commits)
    }
}
