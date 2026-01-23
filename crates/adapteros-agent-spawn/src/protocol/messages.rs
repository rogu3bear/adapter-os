//! Protocol message types for agent communication
//!
//! Defines the request/response types exchanged between orchestrator and agents
//! over Unix Domain Sockets.

use adapteros_core::serde_helpers;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Request sent from orchestrator to agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentRequest {
    /// Assign a task chunk to the agent
    AssignTask(Box<TaskAssignment>),

    /// Request current status
    StatusQuery,

    /// Request agent to synchronize at barrier
    SyncBarrier {
        /// Current tick number
        tick: u64,
        /// Barrier identifier
        barrier_id: String,
    },

    /// Cancel current task
    CancelTask {
        /// Reason for cancellation
        reason: String,
    },

    /// Shutdown the agent gracefully
    Shutdown {
        /// Time to wait for graceful shutdown (ms)
        drain_timeout_ms: u64,
    },

    /// Ping to check agent health
    Ping {
        /// Sequence number for correlation
        sequence: u64,
    },
}

/// Task assignment sent to an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignment {
    /// Unique task ID (BLAKE3 hash)
    #[serde(with = "serde_helpers::hex_bytes")]
    pub task_id: [u8; 32],

    /// Global sequence number for ordering
    pub sequence: u64,

    /// The high-level objective
    pub objective: String,

    /// Files/regions this agent owns
    pub scope: TaskScope,

    /// Constraints on what agent can propose
    pub constraints: TaskConstraints,

    /// Additional context (shared readonly data)
    #[serde(default)]
    pub context: serde_json::Value,
}

/// Defines what code region the agent owns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskScope {
    /// Files the agent can analyze/modify
    pub owned_files: Vec<PathBuf>,

    /// Read-only files for context
    #[serde(default)]
    pub context_files: Vec<PathBuf>,

    /// Root directory for relative paths
    pub root_dir: PathBuf,

    /// AST node IDs if using AST-based distribution
    #[serde(default)]
    pub ast_nodes: Option<Vec<String>>,

    /// Function/method names if using semantic distribution
    #[serde(default)]
    pub semantic_scope: Option<Vec<String>>,
}

/// Constraints on what an agent can propose
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskConstraints {
    /// Maximum number of file modifications
    #[serde(default)]
    pub max_modifications: Option<u32>,

    /// Maximum lines changed per file
    #[serde(default)]
    pub max_lines_per_file: Option<u32>,

    /// File patterns that cannot be modified
    #[serde(default)]
    pub excluded_patterns: Vec<String>,

    /// Require rationale for each change
    #[serde(default = "default_true")]
    pub require_rationale: bool,

    /// Minimum confidence score required
    #[serde(default)]
    pub min_confidence: Option<f32>,
}

fn default_true() -> bool {
    true
}

/// Response sent from agent to orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentResponse {
    /// Agent accepted task
    TaskAccepted {
        /// ID of the accepted task
        #[serde(with = "serde_helpers::hex_bytes")]
        task_id: [u8; 32],
    },

    /// Progress update during task execution
    Progress(TaskProgress),

    /// Task completed with proposal
    TaskComplete(TaskProposal),

    /// Task failed
    TaskFailed {
        /// ID of the failed task
        #[serde(with = "serde_helpers::hex_bytes")]
        task_id: [u8; 32],
        /// Error message
        error: String,
    },

    /// Status response
    Status(AgentStatus),

    /// Barrier reached
    BarrierReached {
        /// Tick number reached
        tick: u64,
        /// Barrier identifier
        barrier_id: String,
    },

    /// Agent shutting down
    ShuttingDown,

    /// Pong response to ping
    Pong {
        /// Sequence number from ping
        sequence: u64,
    },

    /// Error response
    Error {
        /// Error message
        message: String,
        /// Error code (if applicable)
        code: Option<String>,
    },
}

/// Progress update from an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    /// ID of the task
    #[serde(with = "serde_helpers::hex_bytes")]
    pub task_id: [u8; 32],

    /// Progress percentage (0-100)
    pub percent: u8,

    /// Current stage description
    pub stage: String,

    /// Optional detailed message
    #[serde(default)]
    pub message: Option<String>,

    /// Files being processed
    #[serde(default)]
    pub current_files: Vec<PathBuf>,
}

/// A proposal for code modifications from an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProposal {
    /// Task this proposal relates to
    #[serde(with = "serde_helpers::hex_bytes")]
    pub task_id: [u8; 32],

    /// Agent that created this proposal
    pub agent_id: String,

    /// Proposed file modifications
    pub modifications: Vec<FileModification>,

    /// Rationale for the changes
    pub rationale: String,

    /// Confidence score (0.0-1.0)
    pub confidence: f32,

    /// Dependencies on other agents' proposals (by task_id)
    #[serde(default)]
    pub depends_on: Vec<[u8; 32]>,

    /// Conflicts with other proposals (detected by agent)
    #[serde(default)]
    pub conflicts_with: Vec<[u8; 32]>,

    /// BLAKE3 hash of proposal content for integrity
    #[serde(with = "serde_helpers::hex_bytes")]
    pub content_hash: [u8; 32],

    /// Timestamp when proposal was created
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl TaskProposal {
    /// Compute the content hash for this proposal
    pub fn compute_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.agent_id.as_bytes());
        hasher.update(&self.task_id);
        for m in &self.modifications {
            hasher.update(m.file_path.to_string_lossy().as_bytes());
            if let Some(ref content) = m.new_content {
                hasher.update(content.as_bytes());
            }
        }
        hasher.update(self.rationale.as_bytes());
        *hasher.finalize().as_bytes()
    }
}

/// A proposed file modification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileModification {
    /// Path to the file
    pub file_path: PathBuf,

    /// Type of modification
    pub modification_type: ModificationType,

    /// Hash of original content (for conflict detection)
    #[serde(default, with = "serde_helpers::option_hex_bytes")]
    pub original_content_hash: Option<[u8; 32]>,

    /// New content (for create/modify)
    #[serde(default)]
    pub new_content: Option<String>,

    /// Unified diff format (alternative to new_content)
    #[serde(default)]
    pub diff: Option<String>,

    /// Line range affected (1-indexed, inclusive)
    #[serde(default)]
    pub line_range: Option<(u32, u32)>,

    /// Explanation for this specific change
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Type of file modification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModificationType {
    /// Create a new file
    Create,
    /// Modify existing file
    Modify,
    /// Delete file
    Delete,
    /// Rename file
    Rename,
}

/// Agent status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    /// Agent identifier
    pub agent_id: String,

    /// Current state
    pub state: AgentState,

    /// Current task (if any)
    #[serde(default, with = "serde_helpers::option_hex_bytes")]
    pub current_task: Option<[u8; 32]>,

    /// Tasks completed in this session
    pub tasks_completed: u32,

    /// Uptime in seconds
    pub uptime_secs: u64,

    /// Memory usage in bytes
    #[serde(default)]
    pub memory_bytes: Option<u64>,

    /// Last activity timestamp
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

/// Agent state enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Agent is starting up
    Starting,
    /// Agent is ready for tasks
    Ready,
    /// Agent is working on a task
    Working,
    /// Agent is waiting at barrier
    WaitingAtBarrier,
    /// Agent completed all tasks
    Completed,
    /// Agent failed
    Failed,
    /// Agent is shutting down
    ShuttingDown,
}

impl AgentState {
    /// Check if agent is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::ShuttingDown)
    }

    /// Check if agent can accept new tasks
    pub fn can_accept_task(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_request_serialization() {
        let request = AgentRequest::Ping { sequence: 42 };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("ping"));
        assert!(json.contains("42"));

        let parsed: AgentRequest = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentRequest::Ping { sequence } => assert_eq!(sequence, 42),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_task_assignment_serialization() {
        let assignment = TaskAssignment {
            task_id: [0u8; 32],
            sequence: 1,
            objective: "Add error handling".into(),
            scope: TaskScope {
                owned_files: vec![PathBuf::from("src/lib.rs")],
                context_files: vec![],
                root_dir: PathBuf::from("."),
                ast_nodes: None,
                semantic_scope: None,
            },
            constraints: TaskConstraints::default(),
            context: serde_json::Value::Null,
        };

        let json = serde_json::to_string_pretty(&assignment).unwrap();
        let parsed: TaskAssignment = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.objective, "Add error handling");
    }

    #[test]
    fn test_agent_state() {
        assert!(AgentState::Ready.can_accept_task());
        assert!(!AgentState::Working.can_accept_task());
        assert!(AgentState::Failed.is_terminal());
        assert!(!AgentState::Ready.is_terminal());
    }

    #[test]
    fn test_proposal_hash() {
        let proposal = TaskProposal {
            task_id: [1u8; 32],
            agent_id: "agent-01".into(),
            modifications: vec![],
            rationale: "Test".into(),
            confidence: 0.9,
            depends_on: vec![],
            conflicts_with: vec![],
            content_hash: [0u8; 32],
            created_at: chrono::Utc::now(),
        };

        let hash = proposal.compute_hash();
        assert_ne!(hash, [0u8; 32]);

        // Hash should be deterministic
        assert_eq!(hash, proposal.compute_hash());
    }
}
