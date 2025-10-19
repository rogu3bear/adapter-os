//! Multi-agent coordination primitives
//!
//! Provides tick-synchronized barriers and global sequencing for multi-agent workflows.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use thiserror::Error;
use tracing::{debug, info};

/// Global cross-agent sequence counter
/// This ensures deterministic ordering across all agents
static GLOBAL_SEQ_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Error types for multi-agent coordination
#[derive(Error, Debug)]
pub enum CoordinationError {
    #[error("Agent {agent_id} not registered in barrier")]
    AgentNotRegistered { agent_id: String },
    #[error("Barrier timeout after {ticks} ticks")]
    Timeout { ticks: u64 },
    #[error("Barrier failed: {reason}")]
    Failed { reason: String },
}

/// Result type for coordination operations
pub type Result<T> = std::result::Result<T, CoordinationError>;

/// Tick-synchronized barrier for multi-agent coordination
///
/// All agents must reach the same logical tick before any can proceed.
/// This ensures deterministic cross-agent synchronization.
#[derive(Debug)]
pub struct AgentBarrier {
    /// Expected agent IDs
    #[allow(dead_code)]
    agent_ids: Vec<String>,
    /// Current tick for each agent
    agent_ticks: Arc<Mutex<HashMap<String, u64>>>,
    /// Barrier generation (increments on each sync)
    generation: Arc<AtomicU64>,
}

impl AgentBarrier {
    /// Create a new agent barrier
    pub fn new(agent_ids: Vec<String>) -> Self {
        info!(
            "Creating agent barrier with {} agents: {:?}",
            agent_ids.len(),
            agent_ids
        );

        let mut agent_ticks = HashMap::new();
        for agent_id in &agent_ids {
            agent_ticks.insert(agent_id.clone(), 0);
        }

        Self {
            agent_ids,
            agent_ticks: Arc::new(Mutex::new(agent_ticks)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Wait for all agents to reach the same tick
    ///
    /// Returns when all agents have called `wait()` with ticks >= current_tick.
    /// This is a deterministic synchronization point.
    pub async fn wait(&self, agent_id: &str, current_tick: u64) -> Result<()> {
        debug!("Agent {} waiting at tick {}", agent_id, current_tick);

        // Record this agent's tick
        {
            let mut ticks = self.agent_ticks.lock();

            if !ticks.contains_key(agent_id) {
                return Err(CoordinationError::AgentNotRegistered {
                    agent_id: agent_id.to_string(),
                });
            }

            ticks.insert(agent_id.to_string(), current_tick);
        }

        // Poll until all agents have reached this generation
        let gen = self.generation.load(Ordering::Relaxed);
        let mut iterations = 0;
        const MAX_ITERATIONS: u32 = 10000;

        loop {
            if iterations >= MAX_ITERATIONS {
                return Err(CoordinationError::Timeout {
                    ticks: current_tick,
                });
            }

            let all_ready = {
                let ticks = self.agent_ticks.lock();

                // Check if all agents have reached at least current_tick
                ticks.values().all(|&tick| tick >= current_tick)
            };

            if all_ready {
                // All agents ready, advance generation
                let old_gen = self.generation.compare_exchange(
                    gen,
                    gen + 1,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                );

                if old_gen.is_ok() {
                    info!(
                        "All agents synchronized at tick {}, generation {}",
                        current_tick,
                        gen + 1
                    );
                    return Ok(());
                }
            }

            // Yield to other tasks
            tokio::task::yield_now().await;
            iterations += 1;
        }
    }

    /// Get current generation
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Get current tick for an agent
    pub fn agent_tick(&self, agent_id: &str) -> Option<u64> {
        self.agent_ticks.lock().get(agent_id).copied()
    }

    /// Reset barrier (for testing)
    #[cfg(test)]
    pub fn reset(&self) {
        let mut ticks = self.agent_ticks.lock();
        for tick in ticks.values_mut() {
            *tick = 0;
        }
        self.generation.store(0, Ordering::Relaxed);
    }
}

/// Get next global sequence number
///
/// This provides a global ordering across all agents and operations.
pub fn next_global_seq() -> u64 {
    GLOBAL_SEQ_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Get current global sequence counter value
pub fn current_global_seq() -> u64 {
    GLOBAL_SEQ_COUNTER.load(Ordering::SeqCst)
}

/// Reset global sequence counter (for testing)
#[cfg(test)]
pub fn reset_global_seq() {
    GLOBAL_SEQ_COUNTER.store(0, Ordering::SeqCst);
}

/// Coordinated action with global sequence tracking
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoordinatedAction {
    /// Global sequence number
    pub sequence: u64,
    /// Agent ID that initiated the action
    pub agent_id: String,
    /// Tick when action was initiated
    pub tick: u64,
    /// Action payload (serialized)
    pub payload: Vec<u8>,
}

impl CoordinatedAction {
    /// Create a new coordinated action
    pub fn new(agent_id: String, tick: u64, payload: Vec<u8>) -> Self {
        let sequence = next_global_seq();
        Self {
            sequence,
            agent_id,
            tick,
            payload,
        }
    }

    /// Compute deterministic hash of the action
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.sequence.to_le_bytes());
        hasher.update(self.agent_id.as_bytes());
        hasher.update(&self.tick.to_le_bytes());
        hasher.update(&self.payload);
        *hasher.finalize().as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_barrier_single_agent() {
        reset_global_seq();

        let barrier = AgentBarrier::new(vec!["agent-1".to_string()]);

        // Single agent should pass immediately
        barrier.wait("agent-1", 10).await.unwrap();

        assert_eq!(barrier.generation(), 1);
    }

    #[tokio::test]
    async fn test_agent_barrier_multiple_agents() {
        reset_global_seq();

        let barrier = Arc::new(AgentBarrier::new(vec![
            "agent-1".to_string(),
            "agent-2".to_string(),
        ]));

        let barrier1 = barrier.clone();
        let barrier2 = barrier.clone();

        let handle1 = tokio::spawn(async move { barrier1.wait("agent-1", 100).await });

        let handle2 = tokio::spawn(async move { barrier2.wait("agent-2", 100).await });

        // Both should complete
        handle1.await.unwrap().unwrap();
        handle2.await.unwrap().unwrap();

        assert_eq!(barrier.generation(), 1);
    }

    #[tokio::test]
    async fn test_global_sequence() {
        reset_global_seq();

        let seq1 = next_global_seq();
        let seq2 = next_global_seq();
        let seq3 = next_global_seq();

        assert_eq!(seq1, 0);
        assert_eq!(seq2, 1);
        assert_eq!(seq3, 2);
    }

    #[tokio::test]
    async fn test_coordinated_action() {
        reset_global_seq();

        let action1 = CoordinatedAction::new("agent-1".to_string(), 100, vec![1, 2, 3]);

        let action2 = CoordinatedAction::new("agent-2".to_string(), 100, vec![4, 5, 6]);

        assert_eq!(action1.sequence, 0);
        assert_eq!(action2.sequence, 1);

        // Hashes should be deterministic
        let hash1 = action1.hash();
        let hash2 = action1.hash();
        assert_eq!(hash1, hash2);
    }
}
