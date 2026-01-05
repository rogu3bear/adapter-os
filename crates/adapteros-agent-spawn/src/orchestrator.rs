//! Agent orchestrator for coordinating multi-agent planning
//!
//! The orchestrator is the main entry point for executing planning tasks
//! with multiple agents.

use crate::config::AgentSpawnConfig;
use crate::error::{AgentSpawnError, Result};
use crate::protocol::{AgentRequest, AgentResponse, TaskAssignment, TaskProposal};
use crate::result_merger::{ConflictResolution, ResultMerger, UnifiedPlan};
use crate::supervisor::AgentSupervisor;
use crate::task_router::{CodebaseContext, PlanningTask, TaskRouter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Progress update from orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorProgress {
    /// Current phase
    pub phase: OrchestratorPhase,

    /// Overall progress (0-100)
    pub percent: u8,

    /// Message describing current activity
    pub message: String,

    /// Active agent count
    pub active_agents: usize,

    /// Completed agent count
    pub completed_agents: usize,
}

/// Phases of orchestration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorPhase {
    /// Initializing orchestrator
    Initializing,
    /// Spawning agents
    SpawningAgents,
    /// Analyzing codebase
    AnalyzingCodebase,
    /// Distributing tasks
    DistributingTasks,
    /// Agents working
    AgentsWorking,
    /// Collecting proposals
    CollectingProposals,
    /// Merging results
    MergingResults,
    /// Complete
    Complete,
    /// Failed
    Failed,
}

/// Main orchestrator that coordinates multi-agent planning
pub struct AgentOrchestrator {
    /// Configuration
    config: AgentSpawnConfig,

    /// Agent supervisor
    supervisor: AgentSupervisor,

    /// Task router
    router: TaskRouter,

    /// Result merger
    merger: ResultMerger,

    /// Global sequence counter
    sequence_counter: Arc<AtomicU64>,

    /// Current phase
    phase: OrchestratorPhase,
}

impl AgentOrchestrator {
    /// Create a new orchestrator
    pub fn new(config: AgentSpawnConfig) -> Result<Self> {
        config.validate().map_err(AgentSpawnError::config_error)?;

        let supervisor = AgentSupervisor::new(config.clone());
        let router = TaskRouter::new(config.distribution_strategy);
        let merger = ResultMerger::new(ConflictResolution::HighestConfidence);

        Ok(Self {
            config,
            supervisor,
            router,
            merger,
            sequence_counter: Arc::new(AtomicU64::new(0)),
            phase: OrchestratorPhase::Initializing,
        })
    }

    /// Create an orchestrator with custom merger
    pub fn with_merger(config: AgentSpawnConfig, merger: ResultMerger) -> Result<Self> {
        config.validate().map_err(AgentSpawnError::config_error)?;

        let supervisor = AgentSupervisor::new(config.clone());
        let router = TaskRouter::new(config.distribution_strategy);

        Ok(Self {
            config,
            supervisor,
            router,
            merger,
            sequence_counter: Arc::new(AtomicU64::new(0)),
            phase: OrchestratorPhase::Initializing,
        })
    }

    /// Execute a planning task with multiple agents
    pub async fn execute(&mut self, task: PlanningTask) -> Result<UnifiedPlan> {
        info!(
            objective = %task.objective,
            agent_count = self.config.agent_count,
            "Starting multi-agent planning"
        );

        let start_time = std::time::Instant::now();

        // Phase 1: Spawn agents
        self.phase = OrchestratorPhase::SpawningAgents;
        self.supervisor.spawn_all().await?;

        // Start health monitor
        let _health_handle = self.supervisor.start_health_monitor();

        // Phase 2: Wait for all agents to be ready (barrier at tick 0)
        info!("Waiting for agents to synchronize at tick 0");
        self.supervisor.sync_barrier(0, "init").await?;

        // Phase 3: Analyze codebase and create context
        self.phase = OrchestratorPhase::AnalyzingCodebase;
        let context = self.analyze_codebase(&task).await?;

        // Phase 4: Create and distribute task assignments
        self.phase = OrchestratorPhase::DistributingTasks;
        let assignments =
            self.router
                .create_assignments(&task, &context, self.supervisor.agent_ids())?;

        self.distribute_tasks(&assignments).await?;

        // Phase 5: Wait for agents to complete work
        self.phase = OrchestratorPhase::AgentsWorking;
        let task_timeout = Duration::from_secs(self.config.task_timeout_secs);

        // Wait for completion barrier
        info!("Waiting for agents to complete work (tick 1)");
        if let Err(e) = self.supervisor.sync_barrier(1, "complete").await {
            warn!(error = %e, "Some agents did not complete in time");
        }

        // Phase 6: Collect proposals
        self.phase = OrchestratorPhase::CollectingProposals;
        let proposals = self.collect_proposals(task_timeout).await?;

        // Phase 7: Merge proposals into unified plan
        self.phase = OrchestratorPhase::MergingResults;
        let plan = self.merger.merge(proposals)?;

        // Phase 8: Shutdown agents
        self.supervisor
            .shutdown_all(Duration::from_secs(30))
            .await?;

        self.phase = OrchestratorPhase::Complete;

        let elapsed = start_time.elapsed();
        info!(
            elapsed_secs = elapsed.as_secs(),
            modifications = plan.modifications.len(),
            contributors = plan.contributors.len(),
            confidence = %plan.confidence,
            "Multi-agent planning complete"
        );

        Ok(plan)
    }

    /// Analyze the codebase to create distribution context
    async fn analyze_codebase(&self, task: &PlanningTask) -> Result<CodebaseContext> {
        info!(root = %task.root_dir.display(), "Analyzing codebase");

        // If target files specified, use those; otherwise walk directory
        let files = if !task.target_files.is_empty() {
            task.target_files.clone()
        } else {
            // Walk the directory tree
            self.walk_directory(&task.root_dir, &task.exclude_patterns)
                .await?
        };

        debug!(file_count = files.len(), "Found files for analysis");

        Ok(CodebaseContext::from_files(files))
    }

    /// Walk directory to find files
    async fn walk_directory(
        &self,
        root: &PathBuf,
        exclude_patterns: &[String],
    ) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        // Use tokio's async filesystem
        let mut entries = tokio::fs::read_dir(root).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip hidden files and excluded patterns
            if file_name.starts_with('.') {
                continue;
            }

            let should_exclude = exclude_patterns.iter().any(|pattern| {
                path.to_string_lossy().contains(pattern) || file_name.contains(pattern)
            });

            if should_exclude {
                continue;
            }

            let metadata = entry.metadata().await?;

            if metadata.is_dir() {
                // Recurse into subdirectory
                let sub_files = Box::pin(self.walk_directory(&path, exclude_patterns)).await?;
                files.extend(sub_files);
            } else if metadata.is_file() {
                // Check if it's a code file
                if Self::is_code_file(&path) {
                    files.push(path);
                }
            }
        }

        Ok(files)
    }

    /// Check if a file is a code file
    fn is_code_file(path: &Path) -> bool {
        let extensions = [
            "rs", "py", "js", "ts", "jsx", "tsx", "go", "java", "c", "cpp", "h", "hpp", "cs", "rb",
            "swift", "kt", "scala", "clj", "ex", "exs", "hs", "ml", "fs", "sh", "bash", "zsh",
            "yaml", "yml", "json", "toml", "xml", "html", "css", "scss", "less", "sql", "md",
            "txt",
        ];

        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| extensions.contains(&ext))
            .unwrap_or(false)
    }

    /// Distribute tasks to agents
    async fn distribute_tasks(&self, assignments: &[TaskAssignment]) -> Result<()> {
        info!(
            assignment_count = assignments.len(),
            "Distributing tasks to agents"
        );

        for assignment in assignments {
            let agent_id = format!("agent-{:02}", assignment.sequence);

            if let Some(handle) = self.supervisor.get_agent(&agent_id) {
                let request = AgentRequest::AssignTask(Box::new(assignment.clone()));

                if let Err(e) = handle.send(request).await {
                    warn!(agent_id = %agent_id, error = %e, "Failed to send task to agent");
                }

                // Wait for acknowledgment
                match handle.recv(Duration::from_secs(10)).await {
                    Ok(AgentResponse::TaskAccepted { task_id: _ }) => {
                        debug!(agent_id = %agent_id, "Agent accepted task");
                    }
                    Ok(other) => {
                        warn!(agent_id = %agent_id, response = ?other, "Unexpected response to task assignment");
                    }
                    Err(e) => {
                        warn!(agent_id = %agent_id, error = %e, "Failed to receive task acknowledgment");
                    }
                }
            } else {
                warn!(agent_id = %agent_id, "Agent not found for task assignment");
            }
        }

        Ok(())
    }

    /// Collect proposals from all agents
    async fn collect_proposals(&self, timeout: Duration) -> Result<Vec<TaskProposal>> {
        info!("Collecting proposals from agents");

        let mut proposals = Vec::new();

        for handle in self.supervisor.get_all_agents() {
            match handle.recv(timeout).await {
                Ok(AgentResponse::TaskComplete(proposal)) => {
                    debug!(
                        agent_id = %handle.id,
                        modifications = proposal.modifications.len(),
                        confidence = %proposal.confidence,
                        "Received proposal from agent"
                    );
                    proposals.push(proposal);
                }
                Ok(AgentResponse::TaskFailed { task_id: _, error }) => {
                    warn!(
                        agent_id = %handle.id,
                        error = %error,
                        "Agent failed to complete task"
                    );
                }
                Ok(other) => {
                    warn!(
                        agent_id = %handle.id,
                        response = ?other,
                        "Unexpected response when collecting proposals"
                    );
                }
                Err(e) => {
                    warn!(
                        agent_id = %handle.id,
                        error = %e,
                        "Failed to receive proposal from agent"
                    );
                }
            }
        }

        if proposals.is_empty() {
            return Err(AgentSpawnError::AllAgentsFailed);
        }

        info!(
            proposal_count = proposals.len(),
            "Collected proposals from agents"
        );

        Ok(proposals)
    }

    /// Get the current orchestrator phase
    pub fn phase(&self) -> OrchestratorPhase {
        self.phase
    }

    /// Get the number of active agents
    pub fn active_agent_count(&self) -> usize {
        self.supervisor.active_count()
    }

    /// Get next sequence number
    pub fn next_sequence(&self) -> u64 {
        self.sequence_counter.fetch_add(1, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_creation() {
        let config = AgentSpawnConfig::builder().agent_count(5).build();

        let orchestrator = AgentOrchestrator::new(config).unwrap();
        assert_eq!(orchestrator.phase(), OrchestratorPhase::Initializing);
    }

    #[test]
    fn test_is_code_file() {
        assert!(AgentOrchestrator::is_code_file(&PathBuf::from("test.rs")));
        assert!(AgentOrchestrator::is_code_file(&PathBuf::from("test.py")));
        assert!(AgentOrchestrator::is_code_file(&PathBuf::from("test.toml")));
        assert!(!AgentOrchestrator::is_code_file(&PathBuf::from("test.exe")));
        assert!(!AgentOrchestrator::is_code_file(&PathBuf::from("test.png")));
    }

    #[test]
    fn test_invalid_config() {
        let mut config = AgentSpawnConfig::default();
        config.agent_count = 0;

        let result = AgentOrchestrator::new(config);
        assert!(result.is_err());
    }
}
