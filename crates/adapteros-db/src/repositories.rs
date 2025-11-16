use crate::Db;
use adapteros_core::{derive_seed, B3Hash};
use anyhow::Result;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

// Global counter for deterministic ID generation
static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate deterministic ID using HKDF-derived seed
///
/// Per Determinism Policy: All randomness must be seeded via HKDF.
/// This replaces the previous rand::random() implementation which violated the policy.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros();

    // Derive deterministic seed for ID generation
    let global_seed = B3Hash::hash(b"adapteros-db-id-generation");
    let counter = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let seed = derive_seed(&global_seed, &format!("repo_id:{}", counter));

    // Use seeded RNG instead of rand::random()
    let mut rng = ChaCha20Rng::from_seed(seed);
    let random_suffix: u32 = rand::Rng::gen(&mut rng);

    format!("{:016x}{:08x}", timestamp, random_suffix)
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
    /// Register a new repository
    pub async fn register_repository(
        &self,
        tenant_id: &str,
        repo_id: &str,
        path: &str,
        languages: &[String],
        default_branch: &str,
    ) -> Result<Repository> {
        let id = generate_id();
        let languages_json = serde_json::to_string(&languages)?;

        sqlx::query(
            r#"
            INSERT INTO repositories (id, tenant_id, repo_id, path, languages_json, default_branch, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, 'registered', datetime('now'), datetime('now'))
            "#,
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(repo_id)
        .bind(path)
        .bind(&languages_json)
        .bind(default_branch)
        .execute(&self.pool)
        .await?;

        self.get_repository(&id).await
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
    ///
    /// Uses a transaction to ensure atomicity - either all deletions succeed or none do.
    /// This prevents partial deletion if any step fails.
    pub async fn delete_repository(&self, id: &str) -> Result<()> {
        // Begin transaction for atomic multi-step deletion
        let mut tx = self.pool.begin().await?;

        // Delete related CodeGraph metadata first (if exists)
        sqlx::query("DELETE FROM code_graph_metadata WHERE repo_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        // Delete scan jobs
        sqlx::query("DELETE FROM scan_jobs WHERE repo_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        // Delete repository
        sqlx::query("DELETE FROM repositories WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        // Commit transaction - all deletions succeed together
        tx.commit().await?;

        Ok(())
    }

    /// Store CodeGraph metadata
    pub async fn store_code_graph_metadata(
        &self,
        repo_id: &str,
        commit_sha: &str,
        hash_b3: &str,
        file_count: i32,
        symbol_count: i32,
        test_count: i32,
        languages: &[String],
        frameworks: Option<&serde_json::Value>,
        size_bytes: i64,
        symbol_index_hash: Option<&str>,
        vector_index_hash: Option<&str>,
        test_map_hash: Option<&str>,
    ) -> Result<String> {
        let id = generate_id();
        let languages_json = serde_json::to_string(&languages)?;
        let frameworks_json = frameworks.map(|f| f.to_string());

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
        .bind(repo_id)
        .bind(commit_sha)
        .bind(hash_b3)
        .bind(file_count)
        .bind(symbol_count)
        .bind(test_count)
        .bind(&languages_json)
        .bind(&frameworks_json)
        .bind(size_bytes)
        .bind(symbol_index_hash)
        .bind(vector_index_hash)
        .bind(test_map_hash)
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
