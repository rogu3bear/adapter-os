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

/// Manages code intelligence jobs for repository scanning and indexing.
///
/// The `CodeJobManager` orchestrates various code analysis tasks:
/// - Repository scanning and CodeGraph construction
/// - Commit delta pack (CDP) generation
/// - Index updates (FTS5 and HNSW vector indices)
/// - Integration with CAS (Content-Addressable Storage) artifact storage
///
/// Jobs are persisted in the database and their outputs (CodeGraphs, CDPs)
/// are stored as artifacts for later retrieval.
///
/// # Usage
///
/// ```rust,no_run
/// use adapteros_orchestrator::CodeJobManager;
/// use adapteros_db::Db;
/// use std::path::PathBuf;
///
/// # async fn example() -> adapteros_core::Result<()> {
/// let db = Db::connect("sqlite://var/aos-cp.sqlite3").await?;
/// let manager = CodeJobManager::new(db, PathBuf::from("var/artifacts"));
///
/// let job = adapteros_orchestrator::ScanRepositoryJob {
///     repo_id: "my-repo".to_string(),
///     commit_sha: "abc123".to_string(),
///     full_scan: true,
/// };
///
/// manager.execute_scan_job(job).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct CodeJobManager {
    /// Database handle for job persistence
    _db: Db,
    /// Artifact storage for job outputs
    _artifact_store: Arc<RwLock<ArtifactStore>>,
}

/// Simple artifact store for code intelligence job outputs.
///
/// Stores and retrieves artifacts like CodeGraphs and commit delta packs.
/// Artifacts are organized by repository ID and commit SHA for efficient lookup.
///
/// # Storage Layout
///
/// Artifacts are stored under `base_path/{repo_id}/{commit_sha}.codegraph.json`
/// for CodeGraph artifacts. This allows easy retrieval by repository and commit.
pub struct ArtifactStore {
    /// Base path for artifact storage (used in store_codegraph)
    base_path: PathBuf,
}

impl ArtifactStore {
    /// Create a new artifact store with the given base path.
    ///
    /// # Arguments
    /// * `base_path` - Root directory where artifacts will be stored
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Store a CodeGraph artifact to disk.
    ///
    /// Serializes the CodeGraph to JSON and writes it to a file under the
    /// repository's artifact directory. Creates parent directories if needed.
    ///
    /// # Arguments
    /// * `graph` - The CodeGraph to store
    /// * `repo_id` - Repository identifier (used in path)
    /// * `commit_sha` - Commit SHA (used in filename)
    ///
    /// # Returns
    /// The absolute path where the artifact was stored.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Directory creation fails
    /// - JSON serialization fails
    /// - File write fails
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
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AosError::Io(format!("Failed to create artifact directory: {}", e)))?;
        }

        let json = serde_json::to_string(graph).map_err(AosError::Serialization)?;

        tokio::fs::write(&artifact_path, json)
            .await
            .map_err(|e| AosError::Io(format!("Failed to write CodeGraph artifact: {}", e)))?;

        Ok(artifact_path.to_string_lossy().to_string())
    }

    /// Load a CodeGraph artifact from disk.
    ///
    /// Reads and deserializes a CodeGraph from the artifact store.
    ///
    /// # Arguments
    /// * `repo_id` - Repository identifier
    /// * `commit_sha` - Commit SHA of the artifact to load
    ///
    /// # Returns
    /// The deserialized CodeGraph.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The artifact file doesn't exist
    /// - File read fails
    /// - JSON deserialization fails
    pub async fn load_codegraph(&self, repo_id: &str, commit_sha: &str) -> Result<CodeGraph> {
        let artifact_path = self
            .base_path
            .join(repo_id)
            .join(format!("{}.codegraph.json", commit_sha));

        let json = tokio::fs::read_to_string(&artifact_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read CodeGraph artifact: {}", e)))?;

        serde_json::from_str(&json).map_err(AosError::Serialization)
    }
}

/// Configuration for a repository scanning job.
///
/// Scans a repository at a specific commit and builds a CodeGraph representing
/// the codebase structure, symbols, and relationships.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRepositoryJob {
    /// Repository identifier (can be a path or repo ID)
    pub repo_id: String,
    /// Git commit SHA to scan
    pub commit_sha: String,
    /// Whether to perform a full scan (vs incremental)
    pub full_scan: bool,
}

/// Configuration for a commit delta pack (CDP) generation job.
///
/// Computes the difference between two commits and generates a delta pack
/// containing changed symbols, tests, and lint results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDeltaJob {
    /// Repository identifier
    pub repo_id: String,
    /// Base commit SHA (older commit)
    pub base_commit: String,
    /// Head commit SHA (newer commit)
    pub head_commit: String,
}

/// Configuration for an index update job.
///
/// Updates search indices (FTS5 full-text and HNSW vector) for a repository
/// at a specific commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateIndicesJob {
    /// Repository identifier
    pub repo_id: String,
    /// Commit SHA to index
    pub commit_sha: String,
}

impl CodeJobManager {
    /// Create a new code job manager.
    ///
    /// # Arguments
    /// * `db` - Database handle for job persistence
    /// * `artifact_base_path` - Base directory for storing job artifacts
    ///
    /// # Returns
    /// A new `CodeJobManager` instance.
    pub fn new(db: Db, artifact_base_path: PathBuf) -> Self {
        Self {
            _db: db,
            _artifact_store: Arc::new(RwLock::new(ArtifactStore::new(artifact_base_path))),
        }
    }

    /// Execute a repository scan job.
    ///
    /// Scans the repository at the specified commit, builds a CodeGraph,
    /// stores it as an artifact, and updates the job status in the database.
    ///
    /// # Arguments
    /// * `job` - Scan job configuration
    ///
    /// # Returns
    /// `Ok(())` if the job completes successfully.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Job creation in database fails
    /// - Repository path is invalid or inaccessible
    /// - CodeGraph construction fails
    /// - Artifact storage fails
    /// - Database updates fail
    ///
    /// # Process
    /// 1. Creates a job record in the database
    /// 2. Updates status to "running"
    /// 3. Builds CodeGraph from repository directory
    /// 4. Stores CodeGraph as artifact
    /// 5. Updates job status to "completed" with artifact path
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
                    .update_scan_job_progress(&job_id, "failed", None, 0, Some(&e.to_string()))
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
            .update_scan_job_progress(&job_id, "completed", Some(&artifact_path), 100, None)
            .await?;

        Ok(())
    }

    /// Execute a commit delta pack (CDP) generation job.
    ///
    /// Computes the difference between two commits and generates a delta pack.
    /// This is used to identify changed symbols, run tests, and create
    /// ephemeral adapter priors for incremental updates.
    ///
    /// # Arguments
    /// * `job` - Commit delta job configuration
    ///
    /// # Returns
    /// `Ok(())` if the job completes successfully.
    ///
    /// # Errors
    /// Currently returns `Ok(())` as this feature is not fully implemented.
    ///
    /// # Note
    /// This method is a placeholder. Full implementation would:
    /// 1. Load CodeGraphs for both commits
    /// 2. Compute diff between graphs
    /// 3. Extract changed symbols
    /// 4. Run tests if configured
    /// 5. Run linters if configured
    /// 6. Store CDP artifact
    /// 7. Create ephemeral adapter priors
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

    /// Execute an index update job.
    ///
    /// Updates search indices (FTS5 full-text and HNSW vector) for the repository
    /// at the specified commit. This enables fast symbol lookup and semantic search.
    ///
    /// # Arguments
    /// * `job` - Index update job configuration
    ///
    /// # Returns
    /// `Ok(())` if the job completes successfully.
    ///
    /// # Errors
    /// Currently returns `Ok(())` as this feature is not fully implemented.
    ///
    /// # Note
    /// This method is a placeholder. Full implementation would:
    /// 1. Load CodeGraph for the commit
    /// 2. Update FTS5 symbol index
    /// 3. Update HNSW vector index
    /// 4. Update test map
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

    /// Get the status of a scan job by ID.
    ///
    /// # Arguments
    /// * `job_id` - The job identifier
    ///
    /// # Returns
    /// `Some(ScanJob)` if the job exists, `None` if not found.
    ///
    /// # Errors
    /// Returns an error if database query fails.
    pub async fn get_scan_job_status(&self, job_id: &str) -> Result<Option<ScanJob>> {
        self._db
            .get_scan_job(job_id)
            .await
            .map_err(|e| AosError::Database(e.to_string()))
    }

    /// List scan jobs for a repository.
    ///
    /// Returns the most recent scan jobs for the specified repository,
    /// ordered by creation time (newest first).
    ///
    /// # Arguments
    /// * `repo_id` - Repository identifier
    /// * `limit` - Maximum number of jobs to return
    ///
    /// # Returns
    /// A vector of scan jobs, ordered by creation time descending.
    ///
    /// # Errors
    /// Returns an error if database query fails.
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
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let temp_dir = tempfile::tempdir_in(&tmp_root).expect("tempdir");
        let store = ArtifactStore::new(temp_dir.path().to_path_buf());

        // Create a new CodeGraph
        let graph = CodeGraph::new();

        // Store it
        let result = store.store_codegraph(&graph, "test/repo", "abc123").await;
        assert!(result.is_ok());

        // Load it back
        let loaded = store.load_codegraph("test/repo", "abc123").await;
        assert!(loaded.is_ok());
        let loaded_graph = loaded.unwrap();
        assert_eq!(loaded_graph.symbols.len(), graph.symbols.len());
    }
}
