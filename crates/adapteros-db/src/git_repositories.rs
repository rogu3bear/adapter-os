use crate::{new_id, Db};
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Git repository record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GitRepository {
    pub id: String,
    pub repo_id: String,
    pub path: String,
    pub branch: String,
    pub analysis_json: String,
    pub evidence_json: String,
    pub security_scan_json: String,
    pub status: String,
    pub created_at: String,
    pub created_by: String,
    pub last_scan: Option<String>,
}

impl Db {
    /// Ensure a synthetic git_repositories parent row exists for the given repo_id.
    ///
    /// Training jobs are persisted in `repository_training_jobs`, which keeps a
    /// foreign key to `git_repositories(repo_id)`. Some control-plane flows use
    /// adapter repository IDs, so we create a minimal parent row when missing to
    /// keep FK constraints satisfied without changing external API contracts.
    pub async fn ensure_training_repo_parent_exists(
        &self,
        repo_id: &str,
        created_by: &str,
    ) -> Result<()> {
        let repo_id = repo_id.trim();
        if repo_id.is_empty() {
            return Err(AosError::Validation(
                "repo_id cannot be empty when ensuring git repository parent row".to_string(),
            ));
        }

        let id = new_id(IdPrefix::Rep);
        sqlx::query(
            "INSERT OR IGNORE INTO git_repositories \
             (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(repo_id)
        .bind(format!("training://{repo_id}"))
        .bind("main")
        .bind("{}")
        .bind("[]")
        .bind("{}")
        .bind("active")
        .bind(created_by)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Ensure the synthetic direct-training git repository exists.
    ///
    /// The training pipeline persists `repository_training_jobs` with a non-null
    /// `repo_id`. When callers provide `repo_id = None`, training uses this
    /// synthetic parent row so FK constraints remain valid without changing
    /// API behavior.
    pub async fn ensure_direct_training_repo_exists(&self, created_by: &str) -> Result<()> {
        const DIRECT_TRAINING_REPO_ID: &str = "direct-training";
        const DIRECT_TRAINING_REPO_PK: &str = "rep-direct-training";

        sqlx::query(
            "INSERT OR IGNORE INTO git_repositories \
             (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(DIRECT_TRAINING_REPO_PK)
        .bind(DIRECT_TRAINING_REPO_ID)
        .bind(format!("direct-training://{DIRECT_TRAINING_REPO_ID}"))
        .bind("main")
        .bind("{}")
        .bind("[]")
        .bind("{}")
        .bind("active")
        .bind(created_by)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Create a new git repository record
    ///
    /// Evidence: migrations/0002_patch_proposals.sql:1-18
    /// Pattern: Database schema for patch proposals
    pub async fn create_git_repository(
        &self,
        _id: &str,
        repo_id: &str,
        path: &str,
        branch: &str,
        analysis_json: &str,
        created_by: &str,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Rep);
        sqlx::query(
            "INSERT INTO git_repositories 
             (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(repo_id)
        .bind(path)
        .bind(branch)
        .bind(analysis_json)
        .bind("[]") // Empty evidence JSON for now
        .bind("{}") // Empty security scan JSON for now
        .bind("registered")
        .bind(created_by)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(id)
    }

    /// Get a git repository by ID
    ///
    /// Evidence: migrations/0002_patch_proposals.sql:1-18
    /// Pattern: Database schema for patch proposals
    pub async fn get_git_repository(&self, repo_id: &str) -> Result<Option<GitRepository>> {
        let repository = sqlx::query_as::<_, GitRepository>(
            "SELECT id, repo_id, path, branch, analysis_json, evidence_json,
                    security_scan_json, status, created_at, created_by, last_scan
             FROM git_repositories WHERE repo_id = ?",
        )
        .bind(repo_id)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(repository)
    }

    /// List all git repositories
    ///
    /// Evidence: migrations/0002_patch_proposals.sql:1-18
    /// Pattern: Database schema for patch proposals
    pub async fn list_git_repositories(&self) -> Result<Vec<GitRepository>> {
        let repositories = sqlx::query_as::<_, GitRepository>(
            "SELECT id, repo_id, path, branch, analysis_json, evidence_json,
                    security_scan_json, status, created_at, created_by, last_scan
             FROM git_repositories ORDER BY created_at DESC",
        )
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(repositories)
    }

    /// Update git repository status
    ///
    /// Evidence: migrations/0002_patch_proposals.sql:1-18
    /// Pattern: Database schema for patch proposals
    pub async fn update_git_repository_status(&self, repo_id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE git_repositories SET status = ? WHERE repo_id = ?")
            .bind(status)
            .bind(repo_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Update repository analysis
    ///
    /// Evidence: migrations/0002_patch_proposals.sql:1-18
    /// Pattern: Database schema for patch proposals
    pub async fn update_repository_analysis(
        &self,
        repo_id: &str,
        analysis_json: &str,
        evidence_json: &str,
        security_scan_json: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE git_repositories
             SET analysis_json = ?, evidence_json = ?, security_scan_json = ?, last_scan = datetime('now')
             WHERE repo_id = ?",
        )
        .bind(analysis_json)
        .bind(evidence_json)
        .bind(security_scan_json)
        .bind(repo_id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Delete a git repository
    ///
    /// Evidence: migrations/0002_patch_proposals.sql:1-18
    /// Pattern: Database schema for patch proposals
    pub async fn delete_git_repository(&self, repo_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM git_repositories WHERE repo_id = ?")
            .bind(repo_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Get repositories by status
    ///
    /// Evidence: migrations/0002_patch_proposals.sql:1-18
    /// Pattern: Database schema for patch proposals
    pub async fn get_repositories_by_status(&self, status: &str) -> Result<Vec<GitRepository>> {
        let repositories = sqlx::query_as::<_, GitRepository>(
            "SELECT id, repo_id, path, branch, analysis_json, evidence_json,
                    security_scan_json, status, created_at, created_by, last_scan
             FROM git_repositories WHERE status = ? ORDER BY created_at DESC",
        )
        .bind(status)
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(repositories)
    }

    /// Get repositories by creator
    ///
    /// Evidence: migrations/0002_patch_proposals.sql:1-18
    /// Pattern: Database schema for patch proposals
    pub async fn get_repositories_by_creator(
        &self,
        created_by: &str,
    ) -> Result<Vec<GitRepository>> {
        let repositories = sqlx::query_as::<_, GitRepository>(
            "SELECT id, repo_id, path, branch, analysis_json, evidence_json,
                    security_scan_json, status, created_at, created_by, last_scan
             FROM git_repositories WHERE created_by = ? ORDER BY created_at DESC",
        )
        .bind(created_by)
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(repositories)
    }

    /// Update last_scan timestamp for a repository
    ///
    /// Evidence: migrations/0054_add_git_repository_last_scan.sql:1-5
    /// Pattern: Track when repository was last scanned
    pub async fn update_git_repository_last_scan(&self, repo_id: &str) -> Result<()> {
        sqlx::query("UPDATE git_repositories SET last_scan = datetime('now') WHERE repo_id = ?")
            .bind(repo_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }
}
