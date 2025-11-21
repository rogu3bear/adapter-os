//! Code intelligence job orchestration
//!
//! Handles:
//! - Repository scanning and CodeGraph construction
//! - Commit delta pack (CDP) generation
//! - Index updates
//! - Integration with CAS artifact storage

use adapteros_codegraph::CodeGraph;
use adapteros_core::{AosError, Result};
use adapteros_db::{repositories::ScanJob, Db};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Code job manager
#[derive(Clone)]
pub struct CodeJobManager {
    /// Database handle for job persistence
    _db: Db,
    /// Artifact storage for job outputs
    _artifact_store: Arc<RwLock<ArtifactStore>>,
}

/// Simple artifact store abstraction
pub struct ArtifactStore {
    /// Base path for artifact storage (used in store_codegraph)
    base_path: PathBuf,
}

impl ArtifactStore {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Store CodeGraph artifact
    pub async fn store_codegraph(
        &self,
        graph: &CodeGraph,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<String> {
        let artifact_path = self
            .base_path
            .join(repo_id)
            .join(format!("{}.codegraph.json", commit_sha));

        if let Some(parent) = artifact_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AosError::Io(format!("Failed to create artifact directory: {}", e))
            })?;
        }

        let json = serde_json::to_string(graph).map_err(AosError::Serialization)?;

        tokio::fs::write(&artifact_path, json).await.map_err(|e| {
            AosError::Io(format!("Failed to write CodeGraph artifact: {}", e))
        })?;

        Ok(artifact_path.to_string_lossy().to_string())
    }

    /// Load CodeGraph artifact
    pub async fn load_codegraph(&self, repo_id: &str, commit_sha: &str) -> Result<CodeGraph> {
        let artifact_path = self
            .base_path
            .join(repo_id)
            .join(format!("{}.codegraph.json", commit_sha));

        let json = tokio::fs::read_to_string(&artifact_path).await.map_err(|e| {
            AosError::Io(format!("Failed to read CodeGraph artifact: {}", e))
        })?;

        serde_json::from_str(&json).map_err(AosError::Serialization)
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
            _db: db,
            _artifact_store: Arc::new(RwLock::new(ArtifactStore::new(artifact_base_path))),
        }
    }

    /// Execute repository scan job
    pub async fn execute_scan_job(&self, job: ScanRepositoryJob) -> Result<()> {
        info!(
            "Starting scan job for repo={} commit={}",
            job.repo_id, job.commit_sha
        );

        // Create job record
        let job_id = self
            ._db
            .create_scan_job(&job.repo_id, &job.commit_sha)
            .await?;

        // Update status to running
        self._db
            .update_scan_job_progress(&job_id, "running", None, 0, None)
            .await?;

        // Build CodeGraph from repository
        // For now, we assume the repo_id is a path. In production, this would resolve to actual repo location.
        let repo_path = std::path::PathBuf::from(&job.repo_id);

        let graph = match CodeGraph::from_directory(&repo_path, None).await {
            Ok(g) => g,
            Err(e) => {
                error!(error = %e, "Failed to build CodeGraph");
                self._db
                    .update_scan_job_progress(
                        &job_id,
                        "failed",
                        None,
                        0,
                        Some(&e.to_string()),
                    )
                    .await?;
                return Err(e);
            }
        };

        // Store the artifact
        let store = self._artifact_store.read().await;
        let artifact_path = store
            .store_codegraph(&graph, &job.repo_id, &job.commit_sha)
            .await?;

        info!(
            symbols = graph.symbols.len(),
            artifact = %artifact_path,
            "CodeGraph scan completed"
        );

        // Update job as completed
        self._db
            .update_scan_job_progress(
                &job_id,
                "completed",
                Some(&artifact_path),
                100,
                None,
            )
            .await?;

        Ok(())
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
        self._db
            .get_scan_job(job_id)
            .await
            .map_err(|e| AosError::Database(e.to_string()))
    }

    /// List scan jobs for repository
    pub async fn list_scan_jobs(&self, repo_id: &str, limit: i32) -> Result<Vec<ScanJob>> {
        self._db
            .list_scan_jobs(repo_id, limit)
            .await
            .map_err(|e| AosError::Database(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_artifact_store_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(temp_dir.path().to_path_buf());

        // Create a new CodeGraph
        let graph = CodeGraph::new();

        // Store it
        let result = store
            .store_codegraph(&graph, "test/repo", "abc123")
            .await;
        assert!(result.is_ok());

        // Load it back
        let loaded = store.load_codegraph("test/repo", "abc123").await;
        assert!(loaded.is_ok());
        let loaded_graph = loaded.unwrap();
        assert_eq!(loaded_graph.symbols.len(), graph.symbols.len());
    }
}
