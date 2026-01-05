//! Task router for work distribution
//!
//! Distributes work among agents using various strategies.

use crate::config::DistributionStrategy;
use crate::error::{AgentSpawnError, Result};
use crate::protocol::{TaskAssignment, TaskConstraints, TaskScope};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// High-level planning task
#[derive(Debug, Clone)]
pub struct PlanningTask {
    /// Unique task ID
    pub id: [u8; 32],

    /// Human-readable objective
    pub objective: String,

    /// Root directory of the codebase
    pub root_dir: PathBuf,

    /// Files to consider (empty = all files)
    pub target_files: Vec<PathBuf>,

    /// File patterns to exclude
    pub exclude_patterns: Vec<String>,

    /// Global constraints
    pub constraints: TaskConstraints,

    /// Additional context (passed to all agents)
    pub context: serde_json::Value,
}

impl PlanningTask {
    /// Create a new planning task
    pub fn new(objective: impl Into<String>) -> Self {
        let objective = objective.into();
        let id = Self::compute_id(&objective);

        Self {
            id,
            objective,
            root_dir: PathBuf::from("."),
            target_files: Vec::new(),
            exclude_patterns: Vec::new(),
            constraints: TaskConstraints::default(),
            context: serde_json::Value::Null,
        }
    }

    /// Compute task ID from objective
    fn compute_id(objective: &str) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(objective.as_bytes());
        hasher.update(
            &chrono::Utc::now()
                .timestamp_nanos_opt()
                .unwrap_or(0)
                .to_le_bytes(),
        );
        *hasher.finalize().as_bytes()
    }

    /// Set the root directory
    pub fn with_root_dir(mut self, path: PathBuf) -> Self {
        self.root_dir = path;
        self
    }

    /// Set target files
    pub fn with_target_files(mut self, files: Vec<PathBuf>) -> Self {
        self.target_files = files;
        self
    }

    /// Set exclude patterns
    pub fn with_exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    /// Set constraints
    pub fn with_constraints(mut self, constraints: TaskConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    /// Set context
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = context;
        self
    }
}

/// Codebase context used for intelligent distribution
#[derive(Debug, Clone, Default)]
pub struct CodebaseContext {
    /// All files in scope
    pub files: Vec<PathBuf>,

    /// File sizes (for load balancing)
    pub file_sizes: HashMap<PathBuf, u64>,

    /// File dependencies (file -> files it imports)
    pub dependencies: HashMap<PathBuf, Vec<PathBuf>>,

    /// Semantic clusters (related files grouped together)
    pub clusters: Vec<Vec<PathBuf>>,
}

impl CodebaseContext {
    /// Create context from a list of files
    pub fn from_files(files: Vec<PathBuf>) -> Self {
        Self {
            files,
            ..Default::default()
        }
    }
}

/// Routes work to agents based on distribution strategy
pub struct TaskRouter {
    strategy: DistributionStrategy,
}

impl TaskRouter {
    /// Create a new task router with the given strategy
    pub fn new(strategy: DistributionStrategy) -> Self {
        Self { strategy }
    }

    /// Create task assignments for all agents
    pub fn create_assignments(
        &self,
        task: &PlanningTask,
        context: &CodebaseContext,
        agent_ids: &[String],
    ) -> Result<Vec<TaskAssignment>> {
        info!(
            strategy = %self.strategy,
            agent_count = agent_ids.len(),
            file_count = context.files.len(),
            "Creating task assignments"
        );

        if agent_ids.is_empty() {
            return Err(AgentSpawnError::DistributionFailed {
                reason: "No agents available".into(),
            });
        }

        let scopes = self.split_work(task, context, agent_ids.len())?;

        let assignments: Vec<_> = agent_ids
            .iter()
            .zip(scopes)
            .enumerate()
            .map(|(seq, (agent_id, scope))| {
                let task_id = self.compute_subtask_id(&task.id, agent_id);

                TaskAssignment {
                    task_id,
                    sequence: seq as u64,
                    objective: task.objective.clone(),
                    scope,
                    constraints: task.constraints.clone(),
                    context: task.context.clone(),
                }
            })
            .collect();

        debug!(assignments = assignments.len(), "Created task assignments");

        Ok(assignments)
    }

    /// Split work into scopes for each agent
    fn split_work(
        &self,
        task: &PlanningTask,
        context: &CodebaseContext,
        agent_count: usize,
    ) -> Result<Vec<TaskScope>> {
        let files = if task.target_files.is_empty() {
            &context.files
        } else {
            &task.target_files
        };

        match self.strategy {
            DistributionStrategy::FileOwnership => {
                self.split_by_file_ownership(files, agent_count, &task.root_dir)
            }
            DistributionStrategy::RoundRobin => {
                self.split_round_robin(files, agent_count, &task.root_dir)
            }
            DistributionStrategy::Semantic => {
                self.split_semantic(files, context, agent_count, &task.root_dir)
            }
            DistributionStrategy::AstRegion => {
                // Fall back to file ownership for now
                self.split_by_file_ownership(files, agent_count, &task.root_dir)
            }
        }
    }

    /// Split files evenly among agents
    fn split_by_file_ownership(
        &self,
        files: &[PathBuf],
        agent_count: usize,
        root_dir: &Path,
    ) -> Result<Vec<TaskScope>> {
        let chunk_size = files.len().div_ceil(agent_count);

        let scopes: Vec<_> = files
            .chunks(chunk_size.max(1))
            .map(|chunk| TaskScope {
                owned_files: chunk.to_vec(),
                context_files: vec![],
                root_dir: root_dir.to_path_buf(),
                ast_nodes: None,
                semantic_scope: None,
            })
            .collect();

        // Pad with empty scopes if needed
        let mut result = scopes;
        while result.len() < agent_count {
            result.push(TaskScope {
                owned_files: vec![],
                context_files: vec![],
                root_dir: root_dir.to_path_buf(),
                ast_nodes: None,
                semantic_scope: None,
            });
        }

        Ok(result)
    }

    /// Round-robin distribution
    fn split_round_robin(
        &self,
        files: &[PathBuf],
        agent_count: usize,
        root_dir: &Path,
    ) -> Result<Vec<TaskScope>> {
        let mut buckets: Vec<Vec<PathBuf>> = vec![vec![]; agent_count];

        for (i, file) in files.iter().enumerate() {
            buckets[i % agent_count].push(file.clone());
        }

        Ok(buckets
            .into_iter()
            .map(|owned_files| TaskScope {
                owned_files,
                context_files: vec![],
                root_dir: root_dir.to_path_buf(),
                ast_nodes: None,
                semantic_scope: None,
            })
            .collect())
    }

    /// Semantic clustering - keep related files together
    fn split_semantic(
        &self,
        files: &[PathBuf],
        context: &CodebaseContext,
        agent_count: usize,
        root_dir: &Path,
    ) -> Result<Vec<TaskScope>> {
        // If we have pre-computed clusters, use them
        if !context.clusters.is_empty() {
            return self.split_clusters(&context.clusters, agent_count, root_dir);
        }

        // Simple heuristic: group by directory
        let mut dir_groups: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

        for file in files {
            let dir = file.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            dir_groups.entry(dir).or_default().push(file.clone());
        }

        let groups: Vec<Vec<PathBuf>> = dir_groups.into_values().collect();
        self.split_clusters(&groups, agent_count, root_dir)
    }

    /// Distribute clusters among agents
    fn split_clusters(
        &self,
        clusters: &[Vec<PathBuf>],
        agent_count: usize,
        root_dir: &Path,
    ) -> Result<Vec<TaskScope>> {
        let mut buckets: Vec<Vec<PathBuf>> = vec![vec![]; agent_count];
        let mut bucket_sizes: Vec<usize> = vec![0; agent_count];

        // Sort clusters by size (largest first) for better load balancing
        let mut sorted_clusters: Vec<_> = clusters.iter().collect();
        sorted_clusters.sort_by_key(|c| Reverse(c.len()));

        // Assign clusters to the smallest bucket (greedy bin packing)
        for cluster in sorted_clusters {
            let min_idx = bucket_sizes
                .iter()
                .enumerate()
                .min_by_key(|(_, &size)| size)
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            buckets[min_idx].extend(cluster.iter().cloned());
            bucket_sizes[min_idx] += cluster.len();
        }

        Ok(buckets
            .into_iter()
            .map(|owned_files| TaskScope {
                owned_files,
                context_files: vec![],
                root_dir: root_dir.to_path_buf(),
                ast_nodes: None,
                semantic_scope: None,
            })
            .collect())
    }

    /// Compute subtask ID from parent task and agent
    fn compute_subtask_id(&self, parent_id: &[u8; 32], agent_id: &str) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(parent_id);
        hasher.update(agent_id.as_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Rebalance work when an agent fails
    pub fn rebalance(
        &self,
        failed_agent: &str,
        failed_assignment: &TaskAssignment,
        remaining_agents: &[String],
    ) -> Result<Vec<(String, TaskAssignment)>> {
        if remaining_agents.is_empty() {
            return Err(AgentSpawnError::DistributionFailed {
                reason: "No remaining agents for rebalancing".into(),
            });
        }

        info!(
            failed_agent = %failed_agent,
            files = failed_assignment.scope.owned_files.len(),
            remaining = remaining_agents.len(),
            "Rebalancing failed agent's work"
        );

        let files = &failed_assignment.scope.owned_files;
        let chunk_size = files.len().div_ceil(remaining_agents.len());

        let reassignments: Vec<_> = remaining_agents
            .iter()
            .zip(files.chunks(chunk_size.max(1)))
            .map(|(agent_id, chunk)| {
                let task_id = self.compute_subtask_id(&failed_assignment.task_id, agent_id);

                let assignment = TaskAssignment {
                    task_id,
                    sequence: failed_assignment.sequence,
                    objective: failed_assignment.objective.clone(),
                    scope: TaskScope {
                        owned_files: chunk.to_vec(),
                        context_files: failed_assignment.scope.context_files.clone(),
                        root_dir: failed_assignment.scope.root_dir.clone(),
                        ast_nodes: None,
                        semantic_scope: None,
                    },
                    constraints: failed_assignment.constraints.clone(),
                    context: failed_assignment.context.clone(),
                };

                (agent_id.clone(), assignment)
            })
            .collect();

        Ok(reassignments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planning_task_creation() {
        let task = PlanningTask::new("Add error handling");
        assert!(!task.objective.is_empty());
        assert_ne!(task.id, [0u8; 32]);
    }

    #[test]
    fn test_file_ownership_distribution() {
        let router = TaskRouter::new(DistributionStrategy::FileOwnership);

        let files: Vec<PathBuf> = (0..10)
            .map(|i| PathBuf::from(format!("file{}.rs", i)))
            .collect();

        let context = CodebaseContext::from_files(files.clone());
        let task = PlanningTask::new("Test").with_target_files(files);

        let agent_ids: Vec<String> = (0..3).map(|i| format!("agent-{:02}", i)).collect();

        let assignments = router
            .create_assignments(&task, &context, &agent_ids)
            .unwrap();

        assert_eq!(assignments.len(), 3);

        // All files should be assigned
        let total_files: usize = assignments.iter().map(|a| a.scope.owned_files.len()).sum();
        assert_eq!(total_files, 10);
    }

    #[test]
    fn test_round_robin_distribution() {
        let router = TaskRouter::new(DistributionStrategy::RoundRobin);

        let files: Vec<PathBuf> = (0..6)
            .map(|i| PathBuf::from(format!("file{}.rs", i)))
            .collect();

        let context = CodebaseContext::from_files(files.clone());
        let task = PlanningTask::new("Test").with_target_files(files);

        let agent_ids: Vec<String> = (0..3).map(|i| format!("agent-{:02}", i)).collect();

        let assignments = router
            .create_assignments(&task, &context, &agent_ids)
            .unwrap();

        // Each agent should have 2 files with round-robin
        for assignment in &assignments {
            assert_eq!(assignment.scope.owned_files.len(), 2);
        }
    }

    #[test]
    fn test_rebalance() {
        let router = TaskRouter::new(DistributionStrategy::FileOwnership);

        let files: Vec<PathBuf> = (0..6)
            .map(|i| PathBuf::from(format!("file{}.rs", i)))
            .collect();

        let failed_assignment = TaskAssignment {
            task_id: [1u8; 32],
            sequence: 0,
            objective: "Test".into(),
            scope: TaskScope {
                owned_files: files,
                context_files: vec![],
                root_dir: PathBuf::from("."),
                ast_nodes: None,
                semantic_scope: None,
            },
            constraints: TaskConstraints::default(),
            context: serde_json::Value::Null,
        };

        let remaining = vec!["agent-01".to_string(), "agent-02".to_string()];

        let reassignments = router
            .rebalance("agent-00", &failed_assignment, &remaining)
            .unwrap();

        assert_eq!(reassignments.len(), 2);

        let total: usize = reassignments
            .iter()
            .map(|(_, a)| a.scope.owned_files.len())
            .sum();
        assert_eq!(total, 6);
    }
}
