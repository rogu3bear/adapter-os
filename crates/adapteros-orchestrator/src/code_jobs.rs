///! Code intelligence job orchestration
///!
///! Handles:
///! - Repository scanning and CodeGraph construction
///! - Commit delta pack (CDP) generation
///! - Index updates
///! - Integration with CAS artifact storage
// use adapteros_codegraph::CodeGraph;  // Disabled due to tree-sitter conflict
use adapteros_core::{AosError, Result};
use adapteros_db::{repositories::ScanJob, Db};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Code job manager
#[derive(Clone)]
pub struct CodeJobManager {
    db: Db,
    artifact_store: Arc<RwLock<ArtifactStore>>,
}

/// Simple artifact store abstraction
pub struct ArtifactStore {
    base_path: PathBuf,
}

impl ArtifactStore {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Store CodeGraph artifact (stub - codegraph disabled)
    pub async fn store_codegraph(
        &self,
        _graph: &serde_json::Value,
        _repo_id: &str,
        _commit_sha: &str,
    ) -> Result<String> {
        Err(AosError::Internal("CodeGraph support is disabled".to_string()))
    }

    /// Load CodeGraph artifact (stub - codegraph disabled)
    pub async fn load_codegraph(&self, _repo_id: &str, _commit_sha: &str) -> Result<serde_json::Value> {
        Err(AosError::Internal("CodeGraph support is disabled".to_string()))
    }
}

/// Scan repository job configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRepositoryJob {
    pub repo_id: String,
    pub commit_sha: String,
    pub full_scan: bool,
}

/// Commit delta pack job configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDeltaJob {
    pub repo_id: String,
    pub base_commit: String,
    pub head_commit: String,
}

/// Update indices job configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateIndicesJob {
    pub repo_id: String,
    pub commit_sha: String,
}

impl CodeJobManager {
    /// Create new code job manager
    pub fn new(db: Db, artifact_base_path: PathBuf) -> Self {
        Self {
            db,
            artifact_store: Arc::new(RwLock::new(ArtifactStore::new(artifact_base_path))),
        }
    }

    /// Execute repository scan job (stub - codegraph disabled)
    pub async fn execute_scan_job(&self, job: ScanRepositoryJob) -> Result<()> {
        info!(
            "Starting scan job for repo={} commit={}",
            job.repo_id, job.commit_sha
        );

        // Create job record
        let job_id = self
            .db
            .create_scan_job(&job.repo_id, &job.commit_sha)
            .await?;

        // CodeGraph is disabled - fail the job
        error!("CodeGraph support is disabled due to tree-sitter conflict");
        self.db
            .update_scan_job_progress(
                &job_id,
                "failed",
                None,
                0,
                Some("CodeGraph support is disabled"),
            )
            .await?;

        Err(AosError::Internal("CodeGraph support is disabled".to_string()))
    }

    /// Execute commit delta job
    pub async fn execute_commit_delta_job(&self, job: CommitDeltaJob) -> Result<()> {
        info!(
            "Creating commit delta pack for repo={} base={} head={}",
            job.repo_id, job.base_commit, job.head_commit
        );

        // This would:
        // 1. Get both CodeGraphs
        // 2. Compute diff
        // 3. Extract changed symbols
        // 4. Run tests if configured
        // 5. Run linters if configured
        // 6. Store CDP artifact
        // 7. Create ephemeral adapter priors

        // Simplified implementation for now
        warn!("Commit delta job not fully implemented yet");
        Ok(())
    }

    /// Execute update indices job
    pub async fn execute_update_indices_job(&self, job: UpdateIndicesJob) -> Result<()> {
        info!(
            "Updating indices for repo={} commit={}",
            job.repo_id, job.commit_sha
        );

        // This would:
        // 1. Load CodeGraph
        // 2. Update FTS5 symbol index
        // 3. Update HNSW vector index
        // 4. Update test map

        // Simplified implementation for now
        warn!("Update indices job not fully implemented yet");
        Ok(())
    }

    /// Get scan job status
    pub async fn get_scan_job_status(&self, job_id: &str) -> Result<Option<ScanJob>> {
        self.db
            .get_scan_job(job_id)
            .await
            .map_err(|e| AosError::Database(e.to_string()))
    }

    /// List scan jobs for repository
    pub async fn list_scan_jobs(&self, repo_id: &str, limit: i32) -> Result<Vec<ScanJob>> {
        self.db
            .list_scan_jobs(repo_id, limit)
            .await
            .map_err(|e| AosError::Database(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_artifact_store_disabled() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(temp_dir.path().to_path_buf());

        // CodeGraph is disabled - operations should return errors
        let result = store
            .store_codegraph(&serde_json::json!({}), "test/repo", "abc123")
            .await;
        assert!(result.is_err());

        let result = store.load_codegraph("test/repo", "abc123").await;
        assert!(result.is_err());
    }
}
