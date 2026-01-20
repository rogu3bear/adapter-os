//! Agent session state management
//!
//! Manages active multi-agent orchestration sessions, providing a global
//! registry for tracking spawned orchestrators.

use adapteros_agent_spawn::AgentOrchestrator;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Agent session metadata and handle
pub struct AgentSession {
    /// Unique session identifier
    pub id: String,

    /// When the session was started
    pub started_at: std::time::Instant,

    /// The orchestrator managing this session
    pub orchestrator: Arc<tokio::sync::RwLock<AgentOrchestrator>>,

    /// Task objective
    pub objective: String,

    /// Number of agents in this session
    pub agent_count: u16,
}

impl AgentSession {
    /// Create a new agent session
    pub fn new(
        id: String,
        orchestrator: AgentOrchestrator,
        objective: String,
        agent_count: u16,
    ) -> Self {
        Self {
            id,
            started_at: std::time::Instant::now(),
            orchestrator: Arc::new(tokio::sync::RwLock::new(orchestrator)),
            objective,
            agent_count,
        }
    }

    /// Get session uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}

/// Global session store type
pub type SessionStore = Arc<RwLock<HashMap<String, Arc<AgentSession>>>>;

/// Get the global session store
///
/// This is a singleton that maintains all active agent sessions across
/// the lifetime of the CLI process.
pub fn global_session_store() -> SessionStore {
    use once_cell::sync::Lazy;
    static STORE: Lazy<SessionStore> = Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));
    STORE.clone()
}

/// Generate a unique session ID
pub fn generate_session_id() -> String {
    use std::time::SystemTime;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create a short hash from timestamp and random bytes
    let mut hasher = blake3::Hasher::new();
    hasher.update(&timestamp.to_le_bytes());
    hasher.update(&rand::random::<[u8; 8]>());

    let hash = hasher.finalize();
    format!("sess_{}", hex::encode(&hash.as_bytes()[..6]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_generation() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();

        assert!(id1.starts_with("sess_"));
        assert!(id2.starts_with("sess_"));
        assert_ne!(id1, id2); // Should be unique
    }

    #[test]
    fn test_session_store() {
        let store = global_session_store();
        assert_eq!(store.read().len(), 0);
    }
}
