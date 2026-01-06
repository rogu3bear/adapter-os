//! Error types for the multi-agent spawn system

use std::path::PathBuf;
use thiserror::Error;

/// Result type for agent spawn operations
pub type Result<T> = std::result::Result<T, AgentSpawnError>;

/// Errors that can occur in the multi-agent spawn system
#[derive(Debug, Error)]
pub enum AgentSpawnError {
    /// Failed to spawn an agent process
    #[error("Failed to spawn agent {agent_id}: {message}")]
    SpawnFailed { agent_id: String, message: String },

    /// Agent process exited unexpectedly
    #[error("Agent {agent_id} crashed with exit code {exit_code:?}")]
    AgentCrashed {
        agent_id: String,
        exit_code: Option<i32>,
    },

    /// Communication with agent failed
    #[error("Agent {agent_id} communication failed: {message}")]
    CommunicationFailed { agent_id: String, message: String },

    /// UDS socket error
    #[error("Socket error for {path}: {message}")]
    SocketError { path: PathBuf, message: String },

    /// Agent barrier timeout
    #[error("Barrier timeout at tick {tick}: agents {missing:?} did not synchronize")]
    BarrierTimeout { tick: u64, missing: Vec<String> },

    /// Barrier coordination failed
    #[error("Barrier coordination failed: {reason}")]
    BarrierFailed { reason: String },

    /// Task distribution failed
    #[error("Task distribution failed: {reason}")]
    DistributionFailed { reason: String },

    /// Merge conflict could not be resolved
    #[error("Unresolvable merge conflicts: {count} conflicts remain")]
    UnresolvableConflict { count: usize },

    /// Protocol error (invalid message format)
    #[error("Protocol error: {message}")]
    ProtocolError { message: String },

    /// Configuration error
    #[error("Configuration error: {message}")]
    ConfigError { message: String },

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Timeout waiting for operation
    #[error("Operation timed out after {duration_secs}s: {operation}")]
    Timeout {
        operation: String,
        duration_secs: u64,
    },

    /// All agents failed
    #[error("All agents failed - no results to merge")]
    AllAgentsFailed,

    /// Session not found
    #[error("Session {session_id} not found")]
    SessionNotFound { session_id: String },

    /// Worker not available
    #[error("Worker not available at {path}")]
    WorkerUnavailable { path: PathBuf },

    /// Agent already exists
    #[error("Agent {agent_id} already exists")]
    AgentAlreadyExists { agent_id: String },

    /// Maximum restarts exceeded
    #[error("Agent {agent_id} exceeded maximum restart attempts ({attempts})")]
    MaxRestartsExceeded { agent_id: String, attempts: u32 },
}

impl AgentSpawnError {
    /// Create a spawn failed error
    pub fn spawn_failed(agent_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::SpawnFailed {
            agent_id: agent_id.into(),
            message: message.into(),
        }
    }

    /// Create a communication failed error
    pub fn communication_failed(agent_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::CommunicationFailed {
            agent_id: agent_id.into(),
            message: message.into(),
        }
    }

    /// Create a socket error
    pub fn socket_error(path: PathBuf, message: impl Into<String>) -> Self {
        Self::SocketError {
            path,
            message: message.into(),
        }
    }

    /// Create a barrier timeout error
    pub fn barrier_timeout(tick: u64, missing: Vec<String>) -> Self {
        Self::BarrierTimeout { tick, missing }
    }

    /// Create a protocol error
    pub fn protocol_error(message: impl Into<String>) -> Self {
        Self::ProtocolError {
            message: message.into(),
        }
    }

    /// Create a config error
    pub fn config_error(message: impl Into<String>) -> Self {
        Self::ConfigError {
            message: message.into(),
        }
    }

    /// Create a timeout error
    pub fn timeout(operation: impl Into<String>, duration_secs: u64) -> Self {
        Self::Timeout {
            operation: operation.into(),
            duration_secs,
        }
    }

    /// Check if this error is recoverable (agent can be restarted)
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::AgentCrashed { .. }
                | Self::CommunicationFailed { .. }
                | Self::SocketError { .. }
                | Self::Timeout { .. }
        )
    }

    /// Check if this error is fatal (session should be aborted)
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::AllAgentsFailed
                | Self::ConfigError { .. }
                | Self::WorkerUnavailable { .. }
                | Self::MaxRestartsExceeded { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AgentSpawnError::spawn_failed("agent-01", "process fork failed");
        assert!(err.to_string().contains("agent-01"));
        assert!(err.to_string().contains("process fork failed"));
    }

    #[test]
    fn test_error_recoverable() {
        let recoverable = AgentSpawnError::AgentCrashed {
            agent_id: "agent-01".into(),
            exit_code: Some(1),
        };
        assert!(recoverable.is_recoverable());

        let fatal = AgentSpawnError::AllAgentsFailed;
        assert!(!fatal.is_recoverable());
        assert!(fatal.is_fatal());
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: AgentSpawnError = io_err.into();
        assert!(matches!(err, AgentSpawnError::Io(_)));
    }
}
