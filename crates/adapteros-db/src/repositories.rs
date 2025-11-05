use crate::Db;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite};
use std::collections::HashMap;
// UUID generation simplified - using timestamp-based IDs
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros();
    let random_suffix: u32 = rand::random();
    format!("{:016x}{:08x}", timestamp, random_suffix)
}

/// Builder for extended repository registration with metadata
#[derive(Debug, Default)]
pub struct RepositoryExtendedBuilder {
    tenant_id: Option<String>,
    repo_id: Option<String>,
    path: Option<String>,
    languages: Option<Vec<String>>,
    default_branch: Option<String>,
    latest_scan_commit: Option<String>,
    latest_scan_at: Option<String>,
    latest_graph_hash: Option<String>,
    status: Option<String>,
    description: Option<String>,
    url: Option<String>,
    license: Option<String>,
    visibility: Option<String>,
}

/// Parameters for extended repository registration
#[derive(Debug)]
pub struct RepositoryExtendedParams {
    pub tenant_id: String,
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
    pub latest_scan_commit: Option<String>,
    pub latest_scan_at: Option<String>,
    pub latest_graph_hash: Option<String>,
    pub status: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub license: Option<String>,
    pub visibility: Option<String>,
}

/// Builder for creating code graph metadata parameters
#[derive(Debug, Default)]
pub struct CodeGraphMetadataBuilder {
    repo_id: Option<String>,
    commit_sha: Option<String>,
    hash_b3: Option<String>,
    file_count: Option<i32>,
    symbol_count: Option<i32>,
    test_count: Option<i32>,
    languages: Option<Vec<String>>,
    frameworks: Option<serde_json::Value>,
    size_bytes: Option<i64>,
    symbol_index_hash: Option<String>,
    vector_index_hash: Option<String>,
    test_map_hash: Option<String>,
}

/// Parameters for code graph metadata storage
#[derive(Debug)]
pub struct CodeGraphMetadataParams {
    pub repo_id: String,
    pub commit_sha: String,
    pub hash_b3: String,
    pub file_count: i32,
    pub symbol_count: i32,
    pub test_count: i32,
    pub languages: Vec<String>,
    pub frameworks: Option<serde_json::Value>,
    pub size_bytes: i64,
    pub symbol_index_hash: Option<String>,
    pub vector_index_hash: Option<String>,
    pub test_map_hash: Option<String>,
}

impl RepositoryExtendedBuilder {
    /// Create a new extended repository registration builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tenant ID (required)
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the repository ID (required)
    pub fn repo_id(mut self, repo_id: impl Into<String>) -> Self {
        self.repo_id = Some(repo_id.into());
        self
    }

    /// Set the repository path (required)
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Set the languages list (required)
    pub fn languages(mut self, languages: Vec<String>) -> Self {
        self.languages = Some(languages);
        self
    }

    /// Set the default branch (required)
    pub fn default_branch(mut self, default_branch: impl Into<String>) -> Self {
        self.default_branch = Some(default_branch.into());
        self
    }

    /// Set the latest scan commit (optional)
    pub fn latest_scan_commit(mut self, latest_scan_commit: Option<impl Into<String>>) -> Self {
        self.latest_scan_commit = latest_scan_commit.map(|s| s.into());
        self
    }

    /// Set the latest scan timestamp (optional)
    pub fn latest_scan_at(mut self, latest_scan_at: Option<impl Into<String>>) -> Self {
        self.latest_scan_at = latest_scan_at.map(|s| s.into());
        self
    }

    /// Set the latest graph hash (optional)
    pub fn latest_graph_hash(mut self, latest_graph_hash: Option<impl Into<String>>) -> Self {
        self.latest_graph_hash = latest_graph_hash.map(|s| s.into());
        self
    }

    /// Set the repository status (optional)
    pub fn status(mut self, status: Option<impl Into<String>>) -> Self {
        self.status = status.map(|s| s.into());
        self
    }

    /// Set the repository description (optional)
    pub fn description(mut self, description: Option<impl Into<String>>) -> Self {
        self.description = description.map(|s| s.into());
        self
    }

    /// Set the repository URL (optional)
    pub fn url(mut self, url: Option<impl Into<String>>) -> Self {
        self.url = url.map(|s| s.into());
        self
    }

    /// Set the repository license (optional)
    pub fn license(mut self, license: Option<impl Into<String>>) -> Self {
        self.license = license.map(|s| s.into());
        self
    }

    /// Set the repository visibility (optional)
    pub fn visibility(mut self, visibility: Option<impl Into<String>>) -> Self {
        self.visibility = visibility.map(|s| s.into());
        self
    }

    /// Build the extended repository registration parameters
    pub fn build(self) -> Result<RepositoryExtendedParams> {
        Ok(RepositoryExtendedParams {
            tenant_id: self
                .tenant_id
                .ok_or_else(|| anyhow!("tenant_id is required"))?,
            repo_id: self.repo_id.ok_or_else(|| anyhow!("repo_id is required"))?,
            path: self.path.ok_or_else(|| anyhow!("path is required"))?,
            languages: self
                .languages
                .ok_or_else(|| anyhow!("languages is required"))?,
            default_branch: self
                .default_branch
                .ok_or_else(|| anyhow!("default_branch is required"))?,
            latest_scan_commit: self.latest_scan_commit,
            latest_scan_at: self.latest_scan_at,
            latest_graph_hash: self.latest_graph_hash,
            status: self.status,
            description: self.description,
            url: self.url,
            license: self.license,
            visibility: self.visibility,
        })
    }
}

impl CodeGraphMetadataBuilder {
    /// Create a new code graph metadata builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the repository ID (required)
    pub fn repo_id(mut self, repo_id: impl Into<String>) -> Self {
        self.repo_id = Some(repo_id.into());
        self
    }

    /// Set the commit SHA (required)
    pub fn commit_sha(mut self, commit_sha: impl Into<String>) -> Self {
        self.commit_sha = Some(commit_sha.into());
        self
    }

    /// Set the B3 hash (required)
    pub fn hash_b3(mut self, hash_b3: impl Into<String>) -> Self {
        self.hash_b3 = Some(hash_b3.into());
        self
    }

    /// Set the file count (required)
    pub fn file_count(mut self, file_count: i32) -> Self {
        self.file_count = Some(file_count);
        self
    }

    /// Set the symbol count (required)
    pub fn symbol_count(mut self, symbol_count: i32) -> Self {
        self.symbol_count = Some(symbol_count);
        self
    }

    /// Set the test count (required)
    pub fn test_count(mut self, test_count: i32) -> Self {
        self.test_count = Some(test_count);
        self
    }

    /// Set the languages list (required)
    pub fn languages(mut self, languages: Vec<String>) -> Self {
        self.languages = Some(languages);
        self
    }

    /// Set the frameworks JSON (optional)
    pub fn frameworks(mut self, frameworks: Option<serde_json::Value>) -> Self {
        self.frameworks = frameworks;
        self
    }

    /// Set the size in bytes (required)
    pub fn size_bytes(mut self, size_bytes: i64) -> Self {
        self.size_bytes = Some(size_bytes);
        self
    }

    /// Set the symbol index hash (optional)
    pub fn symbol_index_hash(mut self, symbol_index_hash: Option<impl Into<String>>) -> Self {
        self.symbol_index_hash = symbol_index_hash.map(|s| s.into());
        self
    }

    /// Set the vector index hash (optional)
    pub fn vector_index_hash(mut self, vector_index_hash: Option<impl Into<String>>) -> Self {
        self.vector_index_hash = vector_index_hash.map(|s| s.into());
        self
    }

    /// Set the test map hash (optional)
    pub fn test_map_hash(mut self, test_map_hash: Option<impl Into<String>>) -> Self {
        self.test_map_hash = test_map_hash.map(|s| s.into());
        self
    }

    /// Build the code graph metadata parameters
    pub fn build(self) -> Result<CodeGraphMetadataParams> {
        Ok(CodeGraphMetadataParams {
            repo_id: self.repo_id.ok_or_else(|| anyhow!("repo_id is required"))?,
            commit_sha: self
                .commit_sha
                .ok_or_else(|| anyhow!("commit_sha is required"))?,
            hash_b3: self.hash_b3.ok_or_else(|| anyhow!("hash_b3 is required"))?,
            file_count: self
                .file_count
                .ok_or_else(|| anyhow!("file_count is required"))?,
            symbol_count: self
                .symbol_count
                .ok_or_else(|| anyhow!("symbol_count is required"))?,
            test_count: self
                .test_count
                .ok_or_else(|| anyhow!("test_count is required"))?,
            languages: self
                .languages
                .ok_or_else(|| anyhow!("languages is required"))?,
            frameworks: self.frameworks,
            size_bytes: self
                .size_bytes
                .ok_or_else(|| anyhow!("size_bytes is required"))?,
            symbol_index_hash: self.symbol_index_hash,
            vector_index_hash: self.vector_index_hash,
            test_map_hash: self.test_map_hash,
        })
    }
}

/// Repository information
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Repository {
    pub id: String,
    pub tenant_id: String,
    pub repo_id: String,
    pub path: String,
    pub languages_json: Option<String>,
    pub default_branch: String,
    pub latest_scan_commit: Option<String>,
    pub latest_scan_at: Option<String>,
    pub latest_graph_hash: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// CodeGraph metadata
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CodeGraphMetadata {
    pub id: String,
    pub repo_id: String,
    pub commit_sha: String,
    pub hash_b3: String,
    pub file_count: i32,
    pub symbol_count: i32,
    pub test_count: i32,
    pub languages_json: String,
    pub frameworks_json: Option<String>,
    pub size_bytes: i64,
    pub symbol_index_hash: Option<String>,
    pub vector_index_hash: Option<String>,
    pub test_map_hash: Option<String>,
    pub created_at: String,
}

/// Scan job status
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ScanJob {
    pub id: String,
    pub repo_id: String,
    pub commit_sha: String,
    pub status: String,
    pub current_stage: Option<String>,
    pub progress_pct: i32,
    pub error_message: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

impl Db {
    /// Register a new repository (basic)
    ///
    /// This method provides a simplified interface that internally uses the
    /// extended repository builder for consistency.
    pub async fn register_repository(
        &self,
        tenant_id: &str,
        repo_id: &str,
        path: &str,
        languages: &[String],
        default_branch: &str,
    ) -> Result<Repository> {
        let params = RepositoryExtendedBuilder::new()
            .tenant_id(tenant_id)
            .repo_id(repo_id)
            .path(path)
            .languages(languages.to_vec())
            .default_branch(default_branch)
            .build()?;

        let id = self.register_repository_extended(params).await?;
        self.get_repository(&id).await
    }

    /// Register a new repository with extended metadata
    ///
    /// Use [`RepositoryExtendedBuilder`] to construct complex parameter sets:
    /// ```no_run
    /// use adapteros_db::repositories::RepositoryExtendedBuilder;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let params = RepositoryExtendedBuilder::new()
    ///     .tenant_id("tenant-123")
    ///     .repo_id("github.com/org/repo")
    ///     .path("/path/to/repo")
    ///     .languages(vec!["rust".to_string(), "python".to_string()])
    ///     .default_branch("main")
    ///     .latest_scan_commit(Some("a1b2c3d4..."))
    ///     .latest_scan_at(Some("2025-10-31T12:00:00Z"))
    ///     .latest_graph_hash(Some("graph-hash-123"))
    ///     .status(Some("scanned"))
    ///     .description(Some("My awesome repository"))
    ///     .url(Some("https://github.com/org/repo"))
    ///     .license(Some("MIT"))
    ///     .visibility(Some("public"))
    ///     .build()
    ///     .expect("required fields");
    /// db.register_repository_extended(params).await.expect("repository registered");
    /// # }
    /// ```
    pub async fn register_repository_extended(
        &self,
        params: RepositoryExtendedParams,
    ) -> Result<String> {
        let id = generate_id();
        let languages_json = serde_json::to_string(&params.languages)?;
        let status = params.status.unwrap_or_else(|| "registered".to_string());

        sqlx::query(
            r#"
            INSERT INTO repositories (
                id, tenant_id, repo_id, path, languages_json, default_branch,
                latest_scan_commit, latest_scan_at, latest_graph_hash, status,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
            "#,
        )
        .bind(&id)
        .bind(&params.tenant_id)
        .bind(&params.repo_id)
        .bind(&params.path)
        .bind(&languages_json)
        .bind(&params.default_branch)
        .bind(&params.latest_scan_commit)
        .bind(&params.latest_scan_at)
        .bind(&params.latest_graph_hash)
        .bind(&status)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Get repository by ID
    pub async fn get_repository(&self, id: &str) -> Result<Repository> {
        let repo = sqlx::query_as::<_, Repository>(
            r#"
            SELECT id, tenant_id, repo_id, path, languages_json, default_branch,
                   latest_scan_commit, latest_scan_at, latest_graph_hash, status,
                   created_at, updated_at
            FROM repositories
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(repo)
    }

    /// Get repository by repo_id and tenant
    pub async fn get_repository_by_repo_id(
        &self,
        tenant_id: &str,
        repo_id: &str,
    ) -> Result<Option<Repository>> {
        let repo = sqlx::query_as::<_, Repository>(
            r#"
            SELECT id, tenant_id, repo_id, path, languages_json, default_branch,
                   latest_scan_commit, latest_scan_at, latest_graph_hash, status,
                   created_at, updated_at
            FROM repositories
            WHERE tenant_id = ? AND repo_id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(repo_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(repo)
    }

    /// List repositories for a tenant
    pub async fn list_repositories(
        &self,
        tenant_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Repository>> {
        let repos = sqlx::query_as::<_, Repository>(
            r#"
            SELECT id, tenant_id, repo_id, path, languages_json, default_branch,
                   latest_scan_commit, latest_scan_at, latest_graph_hash, status,
                   created_at, updated_at
            FROM repositories
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(repos)
    }

    /// Count commits for a set of repositories
    pub async fn get_commit_counts_for_repositories(
        &self,
        repo_ids: &[String],
    ) -> Result<HashMap<String, i64>> {
        if repo_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut builder = QueryBuilder::<Sqlite>::new(
            "SELECT repo_id, COUNT(*) as commit_count FROM commits WHERE repo_id IN (",
        );

        {
            let mut separated = builder.separated(", ");
            for repo_id in repo_ids {
                separated.push_bind(repo_id);
            }
        }

        builder.push(") GROUP BY repo_id");

        let rows: Vec<(String, i64)> = builder.build_query_as().fetch_all(&self.pool).await?;

        let mut counts = HashMap::with_capacity(rows.len());
        for (repo_id, count) in rows {
            counts.insert(repo_id, count);
        }

        Ok(counts)
    }

    /// Count repositories for a tenant
    pub async fn count_repositories(&self, tenant_id: &str) -> Result<i64> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM repositories WHERE tenant_id = ?")
                .bind(tenant_id)
                .fetch_one(&self.pool)
                .await?;

        Ok(count)
    }

    /// Update repository status
    pub async fn update_repository_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE repositories
            SET status = ?, updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(status)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update repository scan info
    pub async fn update_repository_scan(
        &self,
        id: &str,
        commit_sha: &str,
        graph_hash: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE repositories
            SET latest_scan_commit = ?,
                latest_scan_at = datetime('now'),
                latest_graph_hash = ?,
                status = 'scanned',
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(commit_sha)
        .bind(graph_hash)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete repository and associated data
    pub async fn delete_repository(&self, id: &str) -> Result<()> {
        // Delete related CodeGraph metadata first (if exists)
        sqlx::query("DELETE FROM code_graph_metadata WHERE repo_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        // Delete scan jobs
        sqlx::query("DELETE FROM scan_jobs WHERE repo_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        // Delete repository
        sqlx::query("DELETE FROM repositories WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Store CodeGraph metadata
    ///
    /// Use [`CodeGraphMetadataBuilder`] to construct complex parameter sets:
    /// ```no_run
    /// use adapteros_db::repositories::CodeGraphMetadataBuilder;
    /// use adapteros_db::Db;
    /// use serde_json::json;
    ///
    /// # async fn example(db: &Db) {
    /// let params = CodeGraphMetadataBuilder::new()
    ///     .repo_id("github.com/org/repo")
    ///     .commit_sha("a1b2c3d4...")
    ///     .hash_b3("meta-hash-123")
    ///     .file_count(42)
    ///     .symbol_count(156)
    ///     .test_count(23)
    ///     .languages(vec!["rust".to_string(), "python".to_string()])
    ///     .frameworks(Some(json!({"rust": "cargo", "python": "poetry"})))
    ///     .size_bytes(2048000)
    ///     .symbol_index_hash(Some("sym-hash-456"))
    ///     .vector_index_hash(Some("vec-hash-789"))
    ///     .test_map_hash(Some("test-hash-101"))
    ///     .build()
    ///     .expect("required fields");
    /// db.store_code_graph_metadata(params)
    ///     .await
    ///     .expect("metadata stored");
    /// # }
    /// ```
    pub async fn store_code_graph_metadata(
        &self,
        params: CodeGraphMetadataParams,
    ) -> Result<String> {
        let id = generate_id();
        let languages_json = serde_json::to_string(&params.languages)?;
        let frameworks_json = params.frameworks.as_ref().map(|f| f.to_string());

        sqlx::query(
            r#"
            INSERT INTO code_graph_metadata (
                id, repo_id, commit_sha, hash_b3, file_count, symbol_count, test_count,
                languages_json, frameworks_json, size_bytes, symbol_index_hash,
                vector_index_hash, test_map_hash, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&id)
        .bind(&params.repo_id)
        .bind(&params.commit_sha)
        .bind(&params.hash_b3)
        .bind(params.file_count)
        .bind(params.symbol_count)
        .bind(params.test_count)
        .bind(&languages_json)
        .bind(&frameworks_json)
        .bind(params.size_bytes)
        .bind(&params.symbol_index_hash)
        .bind(&params.vector_index_hash)
        .bind(&params.test_map_hash)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Get CodeGraph metadata by commit
    pub async fn get_code_graph_metadata(
        &self,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<Option<CodeGraphMetadata>> {
        let metadata = sqlx::query_as::<_, CodeGraphMetadata>(
            r#"
            SELECT id, repo_id, commit_sha, hash_b3, file_count, symbol_count, test_count,
                   languages_json, frameworks_json, size_bytes, symbol_index_hash,
                   vector_index_hash, test_map_hash, created_at
            FROM code_graph_metadata
            WHERE repo_id = ? AND commit_sha = ?
            "#,
        )
        .bind(repo_id)
        .bind(commit_sha)
        .fetch_optional(&self.pool)
        .await?;

        Ok(metadata)
    }

    /// Create scan job
    pub async fn create_scan_job(&self, repo_id: &str, commit_sha: &str) -> Result<String> {
        let id = generate_id();

        sqlx::query(
            r#"
            INSERT INTO scan_jobs (id, repo_id, commit_sha, status, progress_pct, started_at)
            VALUES (?, ?, ?, 'pending', 0, datetime('now'))
            "#,
        )
        .bind(&id)
        .bind(repo_id)
        .bind(commit_sha)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Update scan job progress
    pub async fn update_scan_job_progress(
        &self,
        job_id: &str,
        status: &str,
        stage: Option<&str>,
        progress_pct: i32,
        error_message: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE scan_jobs
            SET status = ?,
                current_stage = ?,
                progress_pct = ?,
                error_message = ?,
                completed_at = CASE WHEN ? IN ('completed', 'failed') THEN datetime('now') ELSE completed_at END
            WHERE id = ?
            "#,
        )
        .bind(status)
        .bind(stage)
        .bind(progress_pct)
        .bind(error_message)
        .bind(status)
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get scan job by ID
    pub async fn get_scan_job(&self, job_id: &str) -> Result<Option<ScanJob>> {
        let job = sqlx::query_as::<_, ScanJob>(
            r#"
            SELECT id, repo_id, commit_sha, status, current_stage, progress_pct,
                   error_message, started_at, completed_at
            FROM scan_jobs
            WHERE id = ?
            "#,
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(job)
    }

    /// List scan jobs for a repository
    pub async fn list_scan_jobs(&self, repo_id: &str, limit: i32) -> Result<Vec<ScanJob>> {
        let jobs = sqlx::query_as::<_, ScanJob>(
            r#"
            SELECT id, repo_id, commit_sha, status, current_stage, progress_pct,
                   error_message, started_at, completed_at
            FROM scan_jobs
            WHERE repo_id = ?
            ORDER BY started_at DESC
            LIMIT ?
            "#,
        )
        .bind(repo_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(jobs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_get_commit_counts_for_repositories() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("repo_counts.db");
        let db = Db::connect(db_path.to_str().unwrap()).await?;
        sqlx::query("CREATE TABLE commits (repo_id TEXT NOT NULL);")
            .execute(db.pool())
            .await?;

        let repo_id = "test/repo".to_string();

        let counts = db
            .get_commit_counts_for_repositories(&[repo_id.clone()])
            .await?;
        assert_eq!(counts.get(&repo_id).copied().unwrap_or(0), 0);

        sqlx::query("INSERT INTO commits (repo_id) VALUES (?);")
            .bind(&repo_id)
            .execute(db.pool())
            .await?;

        let counts = db
            .get_commit_counts_for_repositories(&[repo_id.clone()])
            .await?;
        assert_eq!(counts.get(&repo_id).copied().unwrap_or(0), 1);

        Ok(())
    }
}
