//! Handshake protocol for agent registration
//!
//! When an agent process starts, it must complete a handshake with the orchestrator
//! before receiving tasks. This ensures the agent is properly initialized and
//! the UDS connection is established.

use serde::{Deserialize, Serialize};

/// Handshake request sent by agent to orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeRequest {
    /// Agent identifier
    pub agent_id: String,

    /// Protocol version supported by agent
    pub protocol_version: u32,

    /// Agent capabilities
    pub capabilities: AgentCapabilities,

    /// Process ID of the agent
    pub pid: u32,

    /// Timestamp when agent started
    pub started_at: chrono::DateTime<chrono::Utc>,
}

/// Agent capabilities declared during handshake
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentCapabilities {
    /// Agent supports streaming responses
    #[serde(default)]
    pub supports_streaming: bool,

    /// Agent supports barrier synchronization
    #[serde(default = "default_true")]
    pub supports_barrier: bool,

    /// Agent supports task cancellation
    #[serde(default = "default_true")]
    pub supports_cancel: bool,

    /// Maximum concurrent tasks (usually 1)
    #[serde(default = "default_one")]
    pub max_concurrent_tasks: u32,

    /// Agent supports code analysis
    #[serde(default = "default_true")]
    pub supports_analysis: bool,

    /// Agent supports code modification proposals
    #[serde(default = "default_true")]
    pub supports_modification: bool,
}

fn default_true() -> bool {
    true
}

fn default_one() -> u32 {
    1
}

/// Handshake response sent by orchestrator to agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResponse {
    /// Handshake status
    pub status: HandshakeStatus,

    /// Session ID assigned to this agent session
    #[serde(default)]
    pub session_id: Option<String>,

    /// Global seed for deterministic execution (if enabled)
    #[serde(default)]
    pub global_seed: Option<String>,

    /// Path to worker socket for inference
    #[serde(default)]
    pub worker_socket: Option<String>,

    /// Error message (if status is rejected)
    #[serde(default)]
    pub error: Option<String>,

    /// Configuration overrides from orchestrator
    #[serde(default)]
    pub config: serde_json::Value,
}

/// Handshake status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandshakeStatus {
    /// Handshake accepted, agent can proceed
    Accepted,
    /// Handshake rejected, agent should exit
    Rejected,
    /// Protocol version mismatch
    VersionMismatch,
    /// Agent ID conflict (already registered)
    IdConflict,
}

impl HandshakeRequest {
    /// Current protocol version
    pub const PROTOCOL_VERSION: u32 = 1;

    /// Create a new handshake request
    pub fn new(agent_id: String, pid: u32) -> Self {
        Self {
            agent_id,
            protocol_version: Self::PROTOCOL_VERSION,
            capabilities: AgentCapabilities::default(),
            pid,
            started_at: chrono::Utc::now(),
        }
    }

    /// Builder pattern for capabilities
    pub fn with_capabilities(mut self, capabilities: AgentCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }
}

impl HandshakeResponse {
    /// Create an accepted response
    pub fn accepted(session_id: String) -> Self {
        Self {
            status: HandshakeStatus::Accepted,
            session_id: Some(session_id),
            global_seed: None,
            worker_socket: None,
            error: None,
            config: serde_json::Value::Null,
        }
    }

    /// Create a rejected response
    pub fn rejected(error: impl Into<String>) -> Self {
        Self {
            status: HandshakeStatus::Rejected,
            session_id: None,
            global_seed: None,
            worker_socket: None,
            error: Some(error.into()),
            config: serde_json::Value::Null,
        }
    }

    /// Create a version mismatch response
    pub fn version_mismatch(expected: u32, got: u32) -> Self {
        Self {
            status: HandshakeStatus::VersionMismatch,
            session_id: None,
            global_seed: None,
            worker_socket: None,
            error: Some(format!(
                "Protocol version mismatch: expected {}, got {}",
                expected, got
            )),
            config: serde_json::Value::Null,
        }
    }

    /// Set the global seed
    pub fn with_global_seed(mut self, seed: [u8; 32]) -> Self {
        self.global_seed = Some(hex::encode(seed));
        self
    }

    /// Set the worker socket path
    pub fn with_worker_socket(mut self, path: impl Into<String>) -> Self {
        self.worker_socket = Some(path.into());
        self
    }

    /// Set configuration overrides
    pub fn with_config(mut self, config: serde_json::Value) -> Self {
        self.config = config;
        self
    }

    /// Check if handshake was successful
    pub fn is_success(&self) -> bool {
        self.status == HandshakeStatus::Accepted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handshake_request() {
        let request = HandshakeRequest::new("agent-01".into(), 12345);
        assert_eq!(request.agent_id, "agent-01");
        assert_eq!(request.protocol_version, HandshakeRequest::PROTOCOL_VERSION);
        assert!(request.capabilities.supports_barrier);
    }

    #[test]
    fn test_handshake_response_accepted() {
        let response = HandshakeResponse::accepted("session-123".into())
            .with_global_seed([42u8; 32])
            .with_worker_socket("/var/run/worker.sock");

        assert!(response.is_success());
        assert_eq!(response.session_id, Some("session-123".into()));
        assert!(response.global_seed.is_some());
    }

    #[test]
    fn test_handshake_response_rejected() {
        let response = HandshakeResponse::rejected("Agent limit reached");
        assert!(!response.is_success());
        assert_eq!(response.status, HandshakeStatus::Rejected);
        assert!(response.error.is_some());
    }

    #[test]
    fn test_serialization() {
        let request = HandshakeRequest::new("agent-01".into(), 12345);
        let json = serde_json::to_string(&request).unwrap();
        let parsed: HandshakeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id, "agent-01");
    }
}
