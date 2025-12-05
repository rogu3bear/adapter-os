use crate::query_helpers::db_err;
use crate::repositories_kv::{
    CodeGraphMetadataKv, RepositoryKv, RepositoryKvRepository, ScanJobKv,
};
use crate::{Db, StorageMode};
use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use chrono::Utc;
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
    pub frameworks_json: Option<String>,
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
    fn get_repo_kv_repo(&self) -> Option<RepositoryKvRepository> {
        if (self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv())
            && self.has_kv_backend()
        {
            self.kv_backend()
                .map(|kv| RepositoryKvRepository::new(kv.backend().clone()))
        } else {
            None
        }
    }

    fn repo_to_kv(repo: &Repository) -> RepositoryKv {
        RepositoryKv {
            id: repo.id.clone(),
            tenant_id: repo.tenant_id.clone(),
            repo_id: repo.repo_id.clone(),
            path: repo.path.clone(),
            languages_json: repo.languages_json.clone(),
            frameworks_json: repo.frameworks_json.clone(),
            default_branch: repo.default_branch.clone(),
            latest_scan_commit: repo.latest_scan_commit.clone(),
            latest_scan_at: repo.latest_scan_at.clone(),
            latest_graph_hash: repo.latest_graph_hash.clone(),
            status: repo.status.clone(),
            created_at: repo.created_at.clone(),
            updated_at: repo.updated_at.clone(),
        }
    }

    fn repo_from_kv(repo: RepositoryKv) -> Repository {
        Repository {
            id: repo.id,
            tenant_id: repo.tenant_id,
            repo_id: repo.repo_id,
            path: repo.path,
            languages_json: repo.languages_json,
            frameworks_json: repo.frameworks_json,
            default_branch: repo.default_branch,
            latest_scan_commit: repo.latest_scan_commit,
            latest_scan_at: repo.latest_scan_at,
            latest_graph_hash: repo.latest_graph_hash,
            status: repo.status,
            created_at: repo.created_at,
            updated_at: repo.updated_at,
        }
    }

    fn meta_to_kv(meta: &CodeGraphMetadata) -> CodeGraphMetadataKv {
        CodeGraphMetadataKv {
            id: meta.id.clone(),
            repo_id: meta.repo_id.clone(),
            commit_sha: meta.commit_sha.clone(),
            hash_b3: meta.hash_b3.clone(),
            file_count: meta.file_count,
            symbol_count: meta.symbol_count,
            test_count: meta.test_count,
            languages_json: meta.languages_json.clone(),
            frameworks_json: meta.frameworks_json.clone(),
            size_bytes: meta.size_bytes,
            symbol_index_hash: meta.symbol_index_hash.clone(),
            vector_index_hash: meta.vector_index_hash.clone(),
            test_map_hash: meta.test_map_hash.clone(),
            created_at: meta.created_at.clone(),
        }
    }

    fn meta_from_kv(meta: CodeGraphMetadataKv) -> CodeGraphMetadata {
        CodeGraphMetadata {
            id: meta.id,
            repo_id: meta.repo_id,
            commit_sha: meta.commit_sha,
            hash_b3: meta.hash_b3,
            file_count: meta.file_count,
            symbol_count: meta.symbol_count,
            test_count: meta.test_count,
            languages_json: meta.languages_json,
            frameworks_json: meta.frameworks_json,
            size_bytes: meta.size_bytes,
            symbol_index_hash: meta.symbol_index_hash,
            vector_index_hash: meta.vector_index_hash,
            test_map_hash: meta.test_map_hash,
            created_at: meta.created_at,
        }
    }

    fn scan_to_kv(job: &ScanJob) -> ScanJobKv {
        ScanJobKv {
            id: job.id.clone(),
            repo_id: job.repo_id.clone(),
            commit_sha: job.commit_sha.clone(),
            status: job.status.clone(),
            current_stage: job.current_stage.clone(),
            progress_pct: job.progress_pct,
            error_message: job.error_message.clone(),
            started_at: job.started_at.clone(),
            completed_at: job.completed_at.clone(),
        }
    }

    fn scan_from_kv(job: ScanJobKv) -> ScanJob {
        ScanJob {
            id: job.id,
            repo_id: job.repo_id,
            commit_sha: job.commit_sha,
            status: job.status,
            current_stage: job.current_stage,
            progress_pct: job.progress_pct,
            error_message: job.error_message,
            started_at: job.started_at,
            completed_at: job.completed_at,
        }
    }

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
        let languages_json = serde_json::to_string(&languages)
            .map_err(|e| AosError::Validation(format!("Failed to serialize languages: {}", e)))?;

        let mut created: Option<Repository> = None;

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                r#"
                INSERT INTO repositories (id, tenant_id, repo_id, path, languages_json, frameworks_json, default_branch, status, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, NULL, ?, 'registered', datetime('now'), datetime('now'))
                "#,
            )
            .bind(&id)
            .bind(tenant_id)
            .bind(repo_id)
            .bind(path)
            .bind(&languages_json)
            .bind(default_branch)
            .execute(self.pool())
            .await
            .map_err(db_err("register repository"))?;

            created = Some(self.get_repository(&id).await?);
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                let record = if let Some(sql_repo) = created.clone() {
                    Self::repo_to_kv(&sql_repo)
                } else {
                    RepositoryKv {
                        id: id.clone(),
                        tenant_id: tenant_id.to_string(),
                        repo_id: repo_id.to_string(),
                        path: path.to_string(),
                        languages_json: Some(languages_json),
                        frameworks_json: None,
                        default_branch: default_branch.to_string(),
                        latest_scan_commit: None,
                        latest_scan_at: None,
                        latest_graph_hash: None,
                        status: "registered".to_string(),
                        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    }
                };

                if let Err(e) = repo.put_repository(&record).await {
                    self.record_kv_write_fallback("repositories.create");
                    return Err(e);
                }
            } else if !self.storage_mode().write_to_sql() {
                return Err(AosError::Database(
                    "KV backend unavailable for repository creation".to_string(),
                ));
            }
        }

        self.get_repository(&id).await
    }

    /// Get repository by ID
    pub async fn get_repository(&self, id: &str) -> Result<Repository> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                if let Some(kv) = repo.get_repository(id).await? {
                    return Ok(Self::repo_from_kv(kv));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Err(AosError::NotFound(format!("repository {}", id)));
            }
            self.record_kv_read_fallback("repositories.get");
        }

        let repo = sqlx::query_as::<_, Repository>(
            r#"
            SELECT id, tenant_id, repo_id, path, languages_json, frameworks_json, default_branch,
                   latest_scan_commit, latest_scan_at, latest_graph_hash, status,
                   created_at, updated_at
            FROM repositories
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("get repository"))?;

        Ok(repo)
    }

    /// Get repository by repo_id and tenant
    pub async fn get_repository_by_repo_id(
        &self,
        tenant_id: &str,
        repo_id: &str,
    ) -> Result<Option<Repository>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                if let Some(kv) = repo.get_repository_by_repo_id(tenant_id, repo_id).await? {
                    return Ok(Some(Self::repo_from_kv(kv)));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
            self.record_kv_read_fallback("repositories.get_by_repo_id");
        }

        let repo = sqlx::query_as::<_, Repository>(
            r#"
            SELECT id, tenant_id, repo_id, path, languages_json, frameworks_json, default_branch,
                   latest_scan_commit, latest_scan_at, latest_graph_hash, status,
                   created_at, updated_at
            FROM repositories
            WHERE tenant_id = ? AND repo_id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(repo_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get repository by repo_id"))?;

        Ok(repo)
    }

    /// List repositories for a tenant
    pub async fn list_repositories(
        &self,
        tenant_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Repository>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                let repos = repo
                    .list_repositories(tenant_id, limit, offset)
                    .await?
                    .into_iter()
                    .map(Self::repo_from_kv)
                    .collect();
                return Ok(repos);
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
            self.record_kv_read_fallback("repositories.list");
        }

        let repos = sqlx::query_as::<_, Repository>(
            r#"
            SELECT id, tenant_id, repo_id, path, languages_json, frameworks_json, default_branch,
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
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list repositories"))?;

        Ok(repos)
    }

    /// Count repositories for a tenant
    pub async fn count_repositories(&self, tenant_id: &str) -> Result<i64> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                let count = repo.count_repositories(tenant_id).await?;
                return Ok(count);
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(0);
            }
            self.record_kv_read_fallback("repositories.count");
        }

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM repositories WHERE tenant_id = ?")
                .bind(tenant_id)
                .fetch_one(self.pool())
                .await
                .map_err(db_err("count repositories"))?;

        Ok(count)
    }

    /// Update repository status
    pub async fn update_repository_status(&self, id: &str, status: &str) -> Result<()> {
        if self.storage_mode().write_to_sql() {
            sqlx::query(
                r#"
                UPDATE repositories
                SET status = ?, updated_at = datetime('now')
                WHERE id = ?
                "#,
            )
            .bind(status)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db_err("update repository status"))?;
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                if let Err(e) = repo.update_status(id, status).await {
                    self.record_kv_write_fallback("repositories.update_status");
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Update repository scan info
    pub async fn update_repository_scan(
        &self,
        id: &str,
        commit_sha: &str,
        graph_hash: &str,
    ) -> Result<()> {
        if self.storage_mode().write_to_sql() {
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
            .execute(self.pool())
            .await
            .map_err(db_err("update repository scan"))?;
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                if let Err(e) = repo.update_scan(id, commit_sha, graph_hash).await {
                    self.record_kv_write_fallback("repositories.update_scan");
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Update repository frameworks
    pub async fn update_repository_frameworks(
        &self,
        id: &str,
        frameworks: &[String],
    ) -> Result<()> {
        let frameworks_json = serde_json::to_string(frameworks)
            .map_err(|e| AosError::Validation(format!("Failed to serialize frameworks: {}", e)))?;

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                r#"
                UPDATE repositories
                SET frameworks_json = ?, updated_at = datetime('now')
                WHERE id = ?
                "#,
            )
            .bind(&frameworks_json)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update repository frameworks: {}", e))
            })?;
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                if let Err(e) = repo
                    .update_frameworks(id, Some(frameworks_json.clone()))
                    .await
                {
                    self.record_kv_write_fallback("repositories.update_frameworks");
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Delete repository and associated data
    ///
    /// Uses a transaction to ensure atomicity - either all deletions succeed or none do.
    /// This prevents partial deletion if any step fails.
    pub async fn delete_repository(&self, id: &str) -> Result<()> {
        if self.storage_mode().write_to_sql() {
            // Begin transaction for atomic multi-step deletion
            let mut tx = self
                .pool()
                .begin()
                .await
                .map_err(db_err("begin transaction"))?;

            // Delete related CodeGraph metadata first (if exists)
            sqlx::query("DELETE FROM code_graph_metadata WHERE repo_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to delete code graph metadata: {}", e))
                })?;

            // Delete scan jobs
            sqlx::query("DELETE FROM scan_jobs WHERE repo_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(db_err("delete scan jobs"))?;

            // Delete repository
            sqlx::query("DELETE FROM repositories WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(db_err("delete repository"))?;

            // Commit transaction - all deletions succeed together
            tx.commit().await.map_err(db_err("commit transaction"))?;
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo_store) = self.get_repo_kv_repo() {
                if let Some(repo) = repo_store.get_repository(id).await? {
                    let _ = repo_store.delete_repository(&repo).await;
                }
            }
        }

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
        let languages_json = serde_json::to_string(&languages)
            .map_err(|e| AosError::Validation(format!("Failed to serialize languages: {}", e)))?;
        let frameworks_json = frameworks.map(|f| f.to_string());

        let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        if self.storage_mode().write_to_sql() {
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
            .execute(self.pool())
            .await
            .map_err(db_err("store code graph metadata"))?;
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                let record = CodeGraphMetadataKv {
                    id: id.clone(),
                    repo_id: repo_id.to_string(),
                    commit_sha: commit_sha.to_string(),
                    hash_b3: hash_b3.to_string(),
                    file_count,
                    symbol_count,
                    test_count,
                    languages_json: languages_json.clone(),
                    frameworks_json: frameworks_json.clone(),
                    size_bytes,
                    symbol_index_hash: symbol_index_hash.map(|s| s.to_string()),
                    vector_index_hash: vector_index_hash.map(|s| s.to_string()),
                    test_map_hash: test_map_hash.map(|s| s.to_string()),
                    created_at: created_at.clone(),
                };

                if let Err(e) = repo.put_metadata(&record).await {
                    self.record_kv_write_fallback("repositories.store_metadata");
                    return Err(e);
                }
            }
        }

        Ok(id)
    }

    /// Get CodeGraph metadata by commit
    pub async fn get_code_graph_metadata(
        &self,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<Option<CodeGraphMetadata>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                // KV metadata is stored keyed by repo; scan latest and match commit
                if let Some(latest) = repo.latest_metadata(repo_id).await? {
                    if latest.commit_sha == commit_sha {
                        return Ok(Some(Self::meta_from_kv(latest)));
                    }
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
            self.record_kv_read_fallback("repositories.get_metadata");
        }

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
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get code graph metadata"))?;

        Ok(metadata)
    }

    /// Get latest CodeGraph metadata for a repository
    pub async fn get_latest_code_graph_metadata(
        &self,
        repo_id: &str,
    ) -> Result<Option<CodeGraphMetadata>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                if let Some(kv) = repo.latest_metadata(repo_id).await? {
                    return Ok(Some(Self::meta_from_kv(kv)));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
            self.record_kv_read_fallback("repositories.latest_metadata");
        }

        let metadata = sqlx::query_as::<_, CodeGraphMetadata>(
            r#"
            SELECT id, repo_id, commit_sha, hash_b3, file_count, symbol_count, test_count,
                   languages_json, frameworks_json, size_bytes, symbol_index_hash,
                   vector_index_hash, test_map_hash, created_at
            FROM code_graph_metadata
            WHERE repo_id = ?
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(repo_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get latest code graph metadata: {}", e))
        })?;

        Ok(metadata)
    }

    /// Create scan job
    pub async fn create_scan_job(&self, repo_id: &str, commit_sha: &str) -> Result<String> {
        let id = generate_id();

        let started_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                r#"
                INSERT INTO scan_jobs (id, repo_id, commit_sha, status, progress_pct, started_at)
                VALUES (?, ?, ?, 'pending', 0, datetime('now'))
                "#,
            )
            .bind(&id)
            .bind(repo_id)
            .bind(commit_sha)
            .execute(self.pool())
            .await
            .map_err(db_err("create scan job"))?;
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                let job = ScanJobKv {
                    id: id.clone(),
                    repo_id: repo_id.to_string(),
                    commit_sha: commit_sha.to_string(),
                    status: "pending".to_string(),
                    current_stage: None,
                    progress_pct: 0,
                    error_message: None,
                    started_at: started_at.clone(),
                    completed_at: None,
                };

                if let Err(e) = repo.create_scan_job(&job).await {
                    self.record_kv_write_fallback("repositories.create_scan_job");
                    return Err(e);
                }
            }
        }

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
        if self.storage_mode().write_to_sql() {
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
            .execute(self.pool())
            .await
            .map_err(db_err("update scan job progress"))?;
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                if let Err(e) = repo
                    .update_scan_job(job_id, status, stage, progress_pct, error_message)
                    .await
                {
                    self.record_kv_write_fallback("repositories.update_scan_job");
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Get scan job by ID
    pub async fn get_scan_job(&self, job_id: &str) -> Result<Option<ScanJob>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                if let Some(kv) = repo.get_scan_job(job_id).await? {
                    return Ok(Some(Self::scan_from_kv(kv)));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
            self.record_kv_read_fallback("repositories.get_scan_job");
        }

        let job = sqlx::query_as::<_, ScanJob>(
            r#"
            SELECT id, repo_id, commit_sha, status, current_stage, progress_pct,
                   error_message, started_at, completed_at
            FROM scan_jobs
            WHERE id = ?
            "#,
        )
        .bind(job_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get scan job"))?;

        Ok(job)
    }

    /// List scan jobs for a repository
    pub async fn list_scan_jobs(&self, repo_id: &str, limit: i32) -> Result<Vec<ScanJob>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_repo_kv_repo() {
                let jobs = repo
                    .list_scan_jobs(repo_id, limit)
                    .await?
                    .into_iter()
                    .map(Self::scan_from_kv)
                    .collect();
                return Ok(jobs);
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
            self.record_kv_read_fallback("repositories.list_scan_jobs");
        }

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
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list scan jobs"))?;

        Ok(jobs)
    }
}
