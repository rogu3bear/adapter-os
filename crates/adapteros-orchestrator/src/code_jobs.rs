///! Code intelligence job orchestration
///!
///! Handles:
///! - Repository scanning and CodeGraph construction
///! - Commit delta pack (CDP) generation
///! - Index updates
///! - Integration with CAS artifact storage

use adapteros_codegraph::CodeGraph;
use adapteros_core::{AosError, Result};
use adapteros_db::{CodeGraphMetadata, Db, Repository, ScanJob};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

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

    /// Store CodeGraph artifact
    pub async fn store_codegraph(
        &self,
        graph: &CodeGraph,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<String> {
        let artifact_id = format!("{}_{}", repo_id.replace("/", "_"), commit_sha);
        let artifact_path = self.base_path.join(format!("{}.codegraph", artifact_id));

        // Serialize and store (simplified - in production would use CAS)
        let serialized = bincode::serialize(&graph)
            .map_err(|e| AosError::Serialization(e.to_string()))?;
        
        tokio::fs::write(&artifact_path, serialized)
            .await
            .map_err(|e| AosError::Io(e.to_string()))?;

        Ok(graph.content_hash.to_string())
    }

    /// Load CodeGraph artifact
    pub async fn load_codegraph(&self, repo_id: &str, commit_sha: &str) -> Result<CodeGraph> {
        let artifact_id = format!("{}_{}", repo_id.replace("/", "_"), commit_sha);
        let artifact_path = self.base_path.join(format!("{}.codegraph", artifact_id));

        let serialized = tokio::fs::read(&artifact_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to load CodeGraph: {}", e)))?;

        let graph: CodeGraph = bincode::deserialize(&serialized)
            .map_err(|e| AosError::Serialization(e.to_string()))?;

        Ok(graph)
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

    /// Execute repository scan job
    pub async fn execute_scan_job(&self, job: ScanRepositoryJob) -> Result<()> {
        info!(
            "Starting scan job for repo={} commit={}",
            job.repo_id, job.commit_sha
        );

        // Create job record
        let job_id = self.db.create_scan_job(&job.repo_id, &job.commit_sha).await?;

        // Update status to running
        self.db
            .update_scan_job_progress(&job_id, "running", Some("parse_and_build_graph"), 10, None)
            .await?;

        // Get repository
        let repo = match self.db.get_repository(&job.repo_id).await {
            Ok(r) => r,
            Err(e) => {
                self.db
                    .update_scan_job_progress(
                        &job_id,
                        "failed",
                        None,
                        0,
                        Some(&format!("Repository not found: {}", e)),
                    )
                    .await?;
                return Err(AosError::NotFound(format!("Repository: {}", job.repo_id)).into());
            }
        };

        // Parse and build CodeGraph
        let path = PathBuf::from(&repo.path);
        let graph = match self.build_codegraph(&path).await {
            Ok(g) => g,
            Err(e) => {
                error!("Failed to build CodeGraph: {}", e);
                self.db
                    .update_scan_job_progress(
                        &job_id,
                        "failed",
                        None,
                        0,
                        Some(&format!("CodeGraph build failed: {}", e)),
                    )
                    .await?;
                return Err(e);
            }
        };

        debug!("Built CodeGraph with {} symbols", graph.symbols.len());

        // Update progress
        self.db
            .update_scan_job_progress(&job_id, "running", Some("store_artifacts"), 50, None)
            .await?;

        // Store CodeGraph artifact
        let graph_hash = self
            .artifact_store
            .write()
            .await
            .store_codegraph(&graph, &job.repo_id, &job.commit_sha)
            .await?;

        debug!("Stored CodeGraph with hash: {}", graph_hash);

        // Update progress
        self.db
            .update_scan_job_progress(&job_id, "running", Some("index_symbols"), 70, None)
            .await?;

        // Store metadata
        let languages: Vec<String> = graph
            .symbols
            .values()
            .map(|s| format!("{:?}", s.language))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let metadata_id = self
            .db
            .store_code_graph_metadata(
                &job.repo_id,
                &job.commit_sha,
                &graph_hash,
                graph.symbols.len() as i32,
                graph.symbols.len() as i32,
                0, // test_count - would be computed from graph
                &languages,
                None, // frameworks - would be detected
                1024, // size_bytes - would be computed
                None, // symbol_index_hash
                None, // vector_index_hash
                None, // test_map_hash
            )
            .await?;

        debug!("Stored CodeGraph metadata: {}", metadata_id);

        // Update repository scan info
        self.db
            .update_repository_scan(&job.repo_id, &job.commit_sha, &graph_hash)
            .await?;

        // Mark job complete
        self.db
            .update_scan_job_progress(&job_id, "completed", Some("complete"), 100, None)
            .await?;

        info!("Scan job completed: {}", job_id);
        Ok(())
    }

    /// Build CodeGraph from directory
    async fn build_codegraph(&self, path: &Path) -> Result<CodeGraph> {
        // Use CodeGraph to parse and build graph
        let graph = CodeGraph::from_directory(path, None).await?;
        Ok(graph)
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
        self.db.get_scan_job(job_id).await
    }

    /// List scan jobs for repository
    pub async fn list_scan_jobs(&self, repo_id: &str, limit: i32) -> Result<Vec<ScanJob>> {
        self.db.list_scan_jobs(repo_id, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_artifact_store() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(temp_dir.path().to_path_buf());

        // Create minimal graph for testing
        let graph = CodeGraph::new();

        // Store and verify
        let hash = store
            .store_codegraph(&graph, "test/repo", "abc123")
            .await
            .unwrap();

        assert!(!hash.is_empty());

        // Load and verify
        let loaded = store.load_codegraph("test/repo", "abc123").await.unwrap();
        assert_eq!(loaded.content_hash, graph.content_hash);
    }
}

