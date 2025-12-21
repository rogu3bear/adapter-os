//! KV storage for repositories and code graph metadata.
//!
//! Mirrors the SQL `repositories`, `code_graph_metadata`, and `scan_jobs` tables
//! sufficiently to support dual-write and KV-primary modes for repository CRUD
//! and scans.

use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepositoryKv {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeGraphMetadataKv {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanJobKv {
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

pub struct RepositoryKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl RepositoryKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn repo_key(id: &str) -> String {
        format!("repo:{id}")
    }

    fn repo_tenant_index(tenant_id: &str, id: &str) -> String {
        format!("repo-tenant:{tenant_id}:{id}")
    }

    fn repo_by_repo_id_index(tenant_id: &str, repo_id: &str) -> String {
        format!("repo-tenant-repoid:{tenant_id}:{repo_id}")
    }

    fn metadata_key(id: &str) -> String {
        format!("repo-metadata:{id}")
    }

    fn metadata_latest_index(repo_id: &str, created_at: &str, id: &str) -> String {
        format!("repo-metadata-latest:{repo_id}:{created_at}:{id}")
    }

    fn scan_job_key(id: &str) -> String {
        format!("scan-job:{id}")
    }

    fn scan_job_repo_index(repo_id: &str, started_at: &str, id: &str) -> String {
        format!("scan-job-repo:{repo_id}:{started_at}:{id}")
    }

    pub async fn put_repository(&self, repo: &RepositoryKv) -> Result<()> {
        let bytes = serde_json::to_vec(repo).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::repo_key(&repo.id), bytes)
            .await
            .map_err(|e| AosError::Database(format!("KV store repository failed: {e}")))?;

        // tenant index
        self.backend
            .set(
                &Self::repo_tenant_index(&repo.tenant_id, &repo.id),
                repo.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("KV set repo tenant index failed: {e}")))?;

        // repo_id index
        self.backend
            .set(
                &Self::repo_by_repo_id_index(&repo.tenant_id, &repo.repo_id),
                repo.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("KV set repo repo_id index failed: {e}")))?;

        Ok(())
    }

    pub async fn get_repository(&self, id: &str) -> Result<Option<RepositoryKv>> {
        let Some(bytes) = self
            .backend
            .get(&Self::repo_key(id))
            .await
            .map_err(|e| AosError::Database(format!("KV get repository failed: {e}")))?
        else {
            return Ok(None);
        };

        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    pub async fn get_repository_by_repo_id(
        &self,
        tenant_id: &str,
        repo_id: &str,
    ) -> Result<Option<RepositoryKv>> {
        let Some(id_bytes) = self
            .backend
            .get(&Self::repo_by_repo_id_index(tenant_id, repo_id))
            .await
            .map_err(|e| AosError::Database(format!("KV get repo index failed: {e}")))?
        else {
            return Ok(None);
        };
        let id = String::from_utf8(id_bytes)
            .map_err(|e| AosError::Database(format!("Invalid UTF-8 in repo index: {e}")))?;
        self.get_repository(&id).await
    }

    pub async fn list_repositories(
        &self,
        tenant_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<RepositoryKv>> {
        let prefix = format!("repo-tenant:{tenant_id}:");
        let mut repos = Vec::new();
        let keys = self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("KV scan repositories failed: {e}")))?;

        for key in keys {
            if let Some(id) = key.strip_prefix(&prefix) {
                if let Some(repo) = self.get_repository(id).await? {
                    repos.push(repo);
                }
            }
        }

        // Sort by created_at DESC then id DESC to mimic SQL ordering
        repos.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });

        let start = offset.max(0) as usize;
        let end = (start + limit.max(0) as usize).min(repos.len());
        Ok(repos.get(start..end).unwrap_or_default().to_vec())
    }

    pub async fn count_repositories(&self, tenant_id: &str) -> Result<i64> {
        let prefix = format!("repo-tenant:{tenant_id}:");
        let keys = self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("KV scan repo count failed: {e}")))?;
        Ok(keys.len() as i64)
    }

    pub async fn delete_repository(&self, repo: &RepositoryKv) -> Result<()> {
        self.backend
            .delete(&Self::repo_key(&repo.id))
            .await
            .map_err(|e| AosError::Database(format!("KV delete repository failed: {e}")))?;

        let _ = self
            .backend
            .delete(&Self::repo_tenant_index(&repo.tenant_id, &repo.id))
            .await;
        let _ = self
            .backend
            .delete(&Self::repo_by_repo_id_index(&repo.tenant_id, &repo.repo_id))
            .await;
        Ok(())
    }

    pub async fn update_status(&self, id: &str, status: &str) -> Result<()> {
        if let Some(mut repo) = self.get_repository(id).await? {
            repo.status = status.to_string();
            repo.updated_at = Self::now_ts();
            self.put_repository(&repo).await?;
        }
        Ok(())
    }

    pub async fn update_scan(&self, id: &str, commit_sha: &str, graph_hash: &str) -> Result<()> {
        if let Some(mut repo) = self.get_repository(id).await? {
            repo.latest_scan_commit = Some(commit_sha.to_string());
            repo.latest_scan_at = Some(Self::now_ts());
            repo.latest_graph_hash = Some(graph_hash.to_string());
            repo.status = "scanned".to_string();
            repo.updated_at = Self::now_ts();
            self.put_repository(&repo).await?;
        }
        Ok(())
    }

    pub async fn update_frameworks(&self, id: &str, frameworks_json: Option<String>) -> Result<()> {
        if let Some(mut repo) = self.get_repository(id).await? {
            repo.frameworks_json = frameworks_json;
            repo.updated_at = Self::now_ts();
            self.put_repository(&repo).await?;
        }
        Ok(())
    }

    pub async fn put_metadata(&self, row: &CodeGraphMetadataKv) -> Result<()> {
        let bytes = serde_json::to_vec(row).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::metadata_key(&row.id), bytes)
            .await
            .map_err(|e| AosError::Database(format!("KV store metadata failed: {e}")))?;

        self.backend
            .set(
                &Self::metadata_latest_index(&row.repo_id, &row.created_at, &row.id),
                row.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("KV set metadata index failed: {e}")))?;
        Ok(())
    }

    pub async fn latest_metadata(&self, repo_id: &str) -> Result<Option<CodeGraphMetadataKv>> {
        let prefix = format!("repo-metadata-latest:{repo_id}:");
        let mut entries = Vec::new();
        let keys = self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("KV scan metadata failed: {e}")))?;
        for key in keys {
            if let Some(id) = key.split(':').next_back() {
                if let Some(meta) = self.get_metadata(id).await? {
                    entries.push(meta);
                }
            }
        }

        entries.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(entries.into_iter().next())
    }

    pub async fn get_metadata(&self, id: &str) -> Result<Option<CodeGraphMetadataKv>> {
        let Some(bytes) = self
            .backend
            .get(&Self::metadata_key(id))
            .await
            .map_err(|e| AosError::Database(format!("KV get metadata failed: {e}")))?
        else {
            return Ok(None);
        };

        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    pub async fn create_scan_job(&self, job: &ScanJobKv) -> Result<()> {
        let bytes = serde_json::to_vec(job).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::scan_job_key(&job.id), bytes)
            .await
            .map_err(|e| AosError::Database(format!("KV store scan job failed: {e}")))?;

        self.backend
            .set(
                &Self::scan_job_repo_index(&job.repo_id, &job.started_at, &job.id),
                job.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("KV set scan job index failed: {e}")))?;
        Ok(())
    }

    pub async fn update_scan_job(
        &self,
        job_id: &str,
        status: &str,
        stage: Option<&str>,
        progress_pct: i32,
        error_message: Option<&str>,
    ) -> Result<()> {
        if let Some(mut job) = self.get_scan_job(job_id).await? {
            job.status = status.to_string();
            job.current_stage = stage.map(|s| s.to_string());
            job.progress_pct = progress_pct;
            job.error_message = error_message.map(|s| s.to_string());
            if status == "completed" || status == "failed" {
                job.completed_at = Some(Self::now_ts());
            }
            self.create_scan_job(&job).await?;
        }
        Ok(())
    }

    pub async fn get_scan_job(&self, job_id: &str) -> Result<Option<ScanJobKv>> {
        let Some(bytes) = self
            .backend
            .get(&Self::scan_job_key(job_id))
            .await
            .map_err(|e| AosError::Database(format!("KV get scan job failed: {e}")))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    pub async fn list_scan_jobs(&self, repo_id: &str, limit: i32) -> Result<Vec<ScanJobKv>> {
        let prefix = format!("scan-job-repo:{repo_id}:");
        let mut jobs = Vec::new();
        let keys = self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("KV scan scan_jobs failed: {e}")))?;
        for key in keys {
            if let Some(id) = key.split(':').next_back() {
                if let Some(job) = self.get_scan_job(id).await? {
                    jobs.push(job);
                }
            }
        }

        jobs.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        jobs.truncate(limit.max(0) as usize);
        Ok(jobs)
    }

    fn now_ts() -> String {
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }
}
