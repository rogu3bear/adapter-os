///! Code intelligence job orchestration
///!
///! Handles:
///! - Repository scanning and CodeGraph construction
///! - Commit delta pack (CDP) generation
///! - Index updates
///! - Integration with CAS artifact storage
use adapteros_codegraph::CodeGraph;
use adapteros_cdp::{CommitDeltaPack, CdpMetadata, MetadataExtractor};
use adapteros_core::{AosError, Result};
use adapteros_db::{repositories::ScanJob, Db};
use adapteros_git::DiffAnalyzer;
use adapteros_lora_worker::training::{AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample};
use adapteros_lora_worker::{TestExecutor, LinterRunner};
use adapteros_single_file_adapter::format::WeightGroupConfig;
use chrono::{Utc, Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use adapteros_server::config::{OrchestratorConfig, PathsConfig};

/// Code job manager
#[derive(Clone)]
pub struct CodeJobManager {
    db: Db,
    artifact_store: Arc<RwLock<ArtifactStore>>,
    paths_config: PathsConfig,
    orchestrator_config: OrchestratorConfig,
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
        let serialized = serde_json::to_vec(&graph).map_err(AosError::Serialization)?;

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

        let graph: CodeGraph =
            serde_json::from_slice(&serialized).map_err(AosError::Serialization)?;

        Ok(graph)
    }

    /// Store CommitDeltaPack artifact
    pub async fn store_cdp(
        &self,
        cdp: &CommitDeltaPack,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<String> {
        let artifact_id = format!("{}_{}", repo_id.replace("/", "_"), commit_sha);
        let artifact_path = self.base_path.join(format!("{}.cdp", artifact_id));

        let serialized = serde_json::to_vec(cdp).map_err(AosError::Serialization)?;

        tokio::fs::write(&artifact_path, serialized)
            .await
            .map_err(|e| AosError::Io(e.to_string()))?;

        Ok(cdp.content_hash.to_string())
    }

    /// Load CommitDeltaPack artifact
    pub async fn load_cdp(&self, repo_id: &str, commit_sha: &str) -> Result<CommitDeltaPack> {
        let artifact_id = format!("{}_{}", repo_id.replace("/", "_"), commit_sha);
        let artifact_path = self.base_path.join(format!("{}.cdp", artifact_id));

        let serialized = tokio::fs::read(&artifact_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to load CDP: {}", e)))?;

        let cdp: CommitDeltaPack =
            serde_json::from_slice(&serialized).map_err(AosError::Serialization)?;

        Ok(cdp)
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
    pub tenant_id: String,
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
    pub fn new(db: Db, paths_config: PathsConfig, orchestrator_config: OrchestratorConfig) -> Self {
        Self {
            db,
            artifact_store: Arc::new(RwLock::new(ArtifactStore::new(PathBuf::from(&paths_config.artifacts_root)))),
            paths_config,
            orchestrator_config,
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
            .db
            .create_scan_job(&job.repo_id, &job.commit_sha)
            .await?;

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
                return Err(AosError::NotFound(format!("Repository: {}", job.repo_id)));
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
        info!("Starting commit delta job for repo={}", job.repo_id);

        let repo = self.db.get_repository_by_repo_id(&job.tenant_id, &job.repo_id).await?
            .ok_or_else(|| AosError::NotFound(format!("Repository not found: {}", job.repo_id)))?;

        let analysis = DiffAnalyzer::new(&repo.path).analyze_commits(&job.base_commit, &job.head_commit)?;
        info!("Diff analysis complete: {} files changed.", analysis.summary.total_files());

        let (test_results, linter_results) = self.run_validation_steps(&repo.path).await?;
        
        let cdp = self.assemble_and_store_cdp(&job, analysis, test_results, linter_results, &repo.path).await?;
        
        let training_examples = self.generate_training_data_from_cdp(&cdp, &repo.path).await?;

        if !training_examples.is_empty() {
            self.train_and_register_ephemeral_adapter(&cdp, training_examples).await?;
        } else {
            info!("No training examples generated. Skipping ephemeral adapter training.");
        }

        info!("Commit delta job completed successfully for repo={}", job.repo_id);
        Ok(())
    }

    async fn run_validation_steps(&self, repo_path: &Path) -> Result<(Vec<TestResult>, Vec<LinterResult>)> {
        let test_executor = TestExecutor::new(&repo_path);
        let test_results = if test_executor.has_tests() {
            match test_executor.run_tests().await {
                Ok(results) => {
                    info!("Test run completed: {} passed, {} failed.", results.passed, results.failed);
                    vec![results]
                },
                Err(e) => {
                    warn!("Test execution failed: {}", e);
                    vec![]
                }
            }
        } else {
            info!("No tests found, skipping test execution.");
            vec![]
        };

        let linter_runner = LinterRunner::new(&repo_path);
        let linter_results = match linter_runner.run_linters().await {
            Ok(results) => {
                let total_errors = results.iter().map(|r| r.errors.len()).sum::<usize>();
                let total_warnings = results.iter().map(|r| r.warnings.len()).sum::<usize>();
                info!("Linter run completed: {} errors, {} warnings.", total_errors, total_warnings);
                results
            },
            Err(e) => {
                warn!("Linter execution failed: {}", e);
                vec![]
            }
        };
        Ok((test_results, linter_results))
    }

    async fn assemble_and_store_cdp(&self, job: &CommitDeltaJob, analysis: DiffAnalysis, test_results: Vec<TestResult>, linter_results: Vec<LinterResult>, repo_path: &Path) -> Result<CommitDeltaPack> {
        let metadata_extractor = MetadataExtractor::new(&repo_path);
        let metadata = metadata_extractor.extract_for_commit(&job.head_commit)?;

        let cdp = CommitDeltaPack::new(
            job.repo_id.clone(),
            job.head_commit.clone(),
            job.base_commit.clone(),
            analysis.summary,
            analysis.changed_symbols,
            test_results,
            linter_results,
            metadata,
        );

        let cdp_hash = self
            .artifact_store
            .write()
            .await
            .store_cdp(&cdp, &job.repo_id, &job.head_commit)
            .await?;

        info!("Stored Commit Delta Pack with hash: {}", cdp_hash);
        Ok(cdp)
    }

    async fn generate_training_data_from_cdp(&self, cdp: &CommitDeltaPack, repo_path: &Path) -> Result<Vec<TrainingExample>> {
        let head_codegraph = self
            .artifact_store
            .read()
            .await
            .load_codegraph(&cdp.repo_id, &cdp.head_commit)
            .await?;
        
        let analyzer = DiffAnalyzer::new(repo_path);
        let mut training_examples = Vec::new();
        for file_path in &cdp.summary.modified_files {
            let before_content =
                analyzer.get_file_content_at_commit(file_path, &cdp.base_commit)?;
            let after_content =
                analyzer.get_file_content_at_commit(file_path, &cdp.head_commit)?;

            // Find symbols in this file from the codegraph
            for symbol in head_codegraph.symbols.values().filter(|s| PathBuf::from(&s.file_path) == *file_path) {
                // This is a simplification. We'd need a more robust way to find the
                // corresponding symbol in the 'before' content, as line numbers can change.
                // For now, we'll extract based on the 'after' content's line numbers and
                // assume we can find it in the 'before' content.
                let after_symbol_text = after_content
                    .lines()
                    .skip(symbol.span.start_line as usize - 1)
                    .take((symbol.span.end_line - symbol.span.start_line) as usize + 1)
                    .collect::<Vec<_>>()
                    .join("\n");

                // Heuristic: find the same function definition in the before_content.
                // This could be improved with more advanced parsing.
                let symbol_def = format!("fn {}", symbol.name); // Simplified for now
                if let Some(pos) = before_content.find(&symbol_def) {
                    // This is a very rough way to get the 'before' symbol text.
                    // We'll just take a similar number of lines for now.
                    let before_symbol_text = before_content[pos..]
                        .lines()
                        .take((symbol.span.end_line - symbol.span.start_line) as usize + 1)
                        .collect::<Vec<_>>()
                        .join("\n");

                    if before_symbol_text != after_symbol_text {
                        training_examples.push(TrainingExample {
                            input: before_symbol_text.chars().map(|c| c as u32).collect(),
                            target: after_symbol_text.chars().map(|c| c as u32).collect(),
                            metadata: HashMap::new(),
                            weight: 1.0,
                        });
                    }
                }
            }
        }
        info!("Generated {} training examples.", training_examples.len());
        Ok(training_examples)
    }

    async fn train_and_register_ephemeral_adapter(&self, cdp: &CommitDeltaPack, training_examples: Vec<TrainingExample>) -> Result<()> {
        let config = TrainingConfig {
            rank: 4, // Low rank for ephemeral adapters
            alpha: 16.0,
            learning_rate: 0.0001,
            epochs: 1, // Few epochs for speed
            batch_size: 1,
            hidden_dim: 768, // This should match the base model
            weight_group_config: WeightGroupConfig::default(),
        };

        let mut trainer = MicroLoRATrainer::new(config.clone())?;
        let training_result = trainer.train(&training_examples).await?;

        info!(
            "Ephemeral adapter training complete. Final loss: {:.4}",
            training_result.final_loss
        );

        // 6. Package and register the ephemeral adapter
        let adapters_root = PathBuf::from(&self.paths_config.adapters_root);
        std::fs::create_dir_all(&adapters_root)?;

        let packager = AdapterPackager::new(&adapters_root);

        // Quantize weights
        let quantized_weights = LoRAQuantizer::quantize_to_q15(&training_result.weights);

        let packaged_adapter = packager
            .package(
                &training_result.adapter_id,
                &quantized_weights,
                &config,
                &self.orchestrator_config.base_model,
            )
            .await?;

        // Register in DB with a TTL
        let expires_at = Utc::now() + Duration::hours(self.orchestrator_config.ephemeral_adapter_ttl_hours as i64);
        let expires_at_str = expires_at.to_rfc3339();

        self.db
            .register_adapter_extended(
                &training_result.adapter_id,
                &format!("ephemeral_{}", cdp.head_commit),
                &packaged_adapter.hash_b3,
                config.rank as i32,
                4, // Tier 4 for ephemeral
                None,
                None,
                "code",
                "ephemeral",
                None,
                None,
                Some(&cdp.repo_id),
                Some(&cdp.head_commit),
                Some("auto-generated from commit"),
                Some(&expires_at_str),
            )
            .await?;

        info!(
            "Successfully packaged and registered ephemeral adapter {} with TTL.",
            training_result.adapter_id
        );
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

    /// Run garbage collection for ephemeral adapters
    pub async fn run_ephemeral_adapter_gc(&self) -> Result<()> {
        info!("Running ephemeral adapter garbage collection...");

        let expired_adapters = self.db.find_expired_adapters().await?;

        if expired_adapters.is_empty() {
            info!("No expired ephemeral adapters to clean up.");
            return Ok(());
        }

        info!("Found {} expired adapters to clean up.", expired_adapters.len());

        let adapters_root = PathBuf::from(&self.paths_config.adapters_root);

        for adapter in expired_adapters {
            // 1. Delete adapter package file
            let adapter_dir = adapters_root.join(&adapter.adapter_id);
            if adapter_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&adapter_dir) {
                    error!(
                        "Failed to delete adapter directory {}: {}. Skipping database deletion.",
                        adapter_dir.display(),
                        e
                    );
                    continue; // Skip DB deletion if file deletion fails
                }
                info!("Deleted adapter package: {}", adapter_dir.display());
            } else {
                warn!(
                    "Adapter package not found for expired adapter {}, but proceeding with DB deletion.",
                    adapter.adapter_id
                );
            }

            // 2. Delete from database
            if let Err(e) = self.db.delete_adapter(&adapter.id).await {
                error!(
                    "Failed to delete adapter {} from database: {}",
                    adapter.adapter_id, e
                );
            } else {
                info!("Deleted adapter {} from database.", adapter.adapter_id);
            }
        }

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
