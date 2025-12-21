//! Multi-agent coordination primitives
//!
//! Provides tick-synchronized barriers and global sequencing for multi-agent workflows.

use adapteros_core::identity::IdentityEnvelope;
use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder, TelemetryWriter};
use parking_lot::Mutex;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Notify;
use tracing::{debug, info, warn};

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
///
/// ## Concurrency Fixes (2025-11-16)
/// - Replaced busy-wait spin loop with Notify-based signaling (Issue C-2)
/// - Added failure broadcast mechanism to prevent deadlock on timeout (Issue C-5)
/// - Fixed CAS race condition handling (Issue C-1)
/// - Fixed memory ordering from Relaxed to Acquire (Issue C-7)
pub struct AgentBarrier {
    /// Expected agent IDs
    agent_ids: Vec<String>,
    /// Current tick for each agent
    agent_ticks: Arc<Mutex<HashMap<String, u64>>>,
    /// Barrier generation (increments on each sync)
    generation: Arc<AtomicU64>,
    /// Notification mechanism for efficient waiting
    notify: Arc<Notify>,
    /// Failure flag - set when barrier times out
    failed: Arc<AtomicBool>,
    /// Dead agents (Issue C-8: explicit agent removal for failure handling)
    dead_agents: Arc<Mutex<HashSet<String>>>,
    /// Telemetry writer for barrier events
    telemetry: Option<Arc<TelemetryWriter>>,
    /// Tenant ID for telemetry identity
    tenant_id: String,
}

impl std::fmt::Debug for AgentBarrier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentBarrier")
            .field("agent_ids", &self.agent_ids)
            .field("generation", &self.generation.load(Ordering::Relaxed))
            .field("failed", &self.failed.load(Ordering::Relaxed))
            .field("has_telemetry", &self.telemetry.is_some())
            .finish_non_exhaustive()
    }
}

impl AgentBarrier {
    /// Create a new agent barrier
    pub fn new(agent_ids: Vec<String>) -> Self {
        Self::with_telemetry(agent_ids, None)
    }

    /// Create a new agent barrier with telemetry
    pub fn with_telemetry(agent_ids: Vec<String>, telemetry: Option<Arc<TelemetryWriter>>) -> Self {
        Self::with_config(agent_ids, telemetry, "default".to_string())
    }

    /// Create a new agent barrier with full configuration
    pub fn with_config(
        agent_ids: Vec<String>,
        telemetry: Option<Arc<TelemetryWriter>>,
        tenant_id: String,
    ) -> Self {
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
            notify: Arc::new(Notify::new()),
            failed: Arc::new(AtomicBool::new(false)),
            dead_agents: Arc::new(Mutex::new(HashSet::new())),
            telemetry,
            tenant_id,
        }
    }

    /// Mark an agent as dead/removed
    ///
    /// ## Issue C-8: Explicit Dead Agent Removal
    /// Allows the barrier to proceed without waiting for a crashed/dead agent.
    /// Remaining living agents can continue to sync after dead agents are marked.
    ///
    /// ## Safety
    /// - Agent must be in the original agent_ids list
    /// - Dead agents cannot be revived (permanent removal)
    /// - Notifies all waiting threads to re-evaluate barrier condition
    ///
    /// ## Arguments
    /// * `agent_id` - ID of the agent to mark as dead
    ///
    /// ## Errors
    /// Returns `CoordinationError::AgentNotRegistered` if agent was never registered
    pub fn mark_agent_dead(&self, agent_id: &str) -> Result<()> {
        // Safety check: agent must exist in original list
        if !self.agent_ids.contains(&agent_id.to_string()) {
            return Err(CoordinationError::AgentNotRegistered {
                agent_id: agent_id.to_string(),
            });
        }

        let mut dead = self.dead_agents.lock();

        // Warn if already dead
        if dead.contains(agent_id) {
            warn!(
                agent_id = %agent_id,
                "Attempted to mark already-dead agent as dead (no-op)"
            );
            return Ok(());
        }

        // Mark as dead
        dead.insert(agent_id.to_string());
        let remaining_count = self.agent_ids.len() - dead.len();

        info!(
            agent_id = %agent_id,
            dead_count = dead.len(),
            remaining_agents = remaining_count,
            "Agent marked as dead - barrier will proceed without it"
        );

        // Emit barrier.agent_removed telemetry (Phase 3)
        if let Some(ref telemetry) = self.telemetry {
            let identity = IdentityEnvelope::new(
                self.tenant_id.clone(),
                "multi-agent".to_string(),
                "barrier".to_string(),
                IdentityEnvelope::default_revision(),
            );
            let event = TelemetryEventBuilder::new(
                EventType::Custom("barrier.agent_removed".to_string()),
                LogLevel::Warn,
                format!(
                    "Agent {} marked as dead ({} dead, {} remaining)",
                    agent_id,
                    dead.len(),
                    remaining_count
                ),
                identity.clone(),
            )
            .component("adapteros-deterministic-exec".to_string())
            .metadata(json!({
                "agent_id": agent_id,
                "dead_count": dead.len(),
                "remaining_agents": remaining_count,
                "total_agents": self.agent_ids.len(),
                "generation": self.generation.load(Ordering::Acquire),
            }))
            .build()
            .map_err(|e| CoordinationError::Failed {
                reason: e.to_string(),
            })?;

            let _ = telemetry.log_event(event);
        }

        // Release lock before notifying
        drop(dead);

        // Notify all waiting threads to re-evaluate (Issue C-8)
        self.notify.notify_waiters();

        Ok(())
    }

    /// Wait for all agents to reach the same tick
    ///
    /// Returns when all agents have called `wait()` with ticks >= current_tick.
    /// This is a deterministic synchronization point.
    ///
    /// ## Concurrency Guarantees
    /// - Uses Notify instead of busy-wait for efficient coordination
    /// - Properly handles CAS losers when advancing generation
    /// - Broadcasts failure on timeout to prevent deadlock
    /// - Uses Acquire ordering to ensure memory visibility
    pub async fn wait(&self, agent_id: &str, current_tick: u64) -> Result<()> {
        // Check if barrier already failed (Issue C-5: failure broadcast)
        if self.failed.load(Ordering::Acquire) {
            return Err(CoordinationError::Failed {
                reason: "Barrier already failed due to previous timeout".into(),
            });
        }

        debug!(
            agent_id = %agent_id,
            tick = current_tick,
            "Agent entering barrier"
        );

        let wait_start = tokio::time::Instant::now();

        // Emit barrier.wait_start telemetry
        if let Some(ref telemetry) = self.telemetry {
            let identity = IdentityEnvelope::new(
                self.tenant_id.clone(),
                "multi-agent".to_string(),
                "barrier".to_string(),
                IdentityEnvelope::default_revision(),
            );
            let event = TelemetryEventBuilder::new(
                EventType::Custom("barrier.wait_start".to_string()),
                LogLevel::Debug,
                format!(
                    "Agent {} entering barrier at tick {}",
                    agent_id, current_tick
                ),
                identity.clone(),
            )
            .component("adapteros-deterministic-exec".to_string())
            .metadata(json!({
                "agent_id": agent_id,
                "tick": current_tick,
                "generation": self.generation.load(Ordering::Acquire),
                "total_agents": self.agent_ids.len(),
            }))
            .build()
            .map_err(|e| {
                warn!(error = %e, "Failed to build barrier wait_start event");
                CoordinationError::Failed {
                    reason: e.to_string(),
                }
            });

            if let Ok(event) = event {
                let _ = telemetry.log_event(event);
            }
        }

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

        // Notify other waiters that we've updated our tick
        self.notify.notify_waiters();

        // Issue C-7 fix: Use Acquire ordering instead of Relaxed
        let gen = self.generation.load(Ordering::Acquire);
        let timeout_duration = Duration::from_secs(30); // 30 second timeout
        let start = tokio::time::Instant::now();

        loop {
            // Check for timeout (Issue C-5: broadcast failure)
            if start.elapsed() > timeout_duration {
                warn!(
                    agent_id = %agent_id,
                    tick = current_tick,
                    "Barrier timeout - broadcasting failure to all agents"
                );

                // Emit barrier.timeout telemetry
                if let Some(ref telemetry) = self.telemetry {
                    let identity = IdentityEnvelope::new(
                        self.tenant_id.clone(),
                        "multi-agent".to_string(),
                        "barrier".to_string(),
                        IdentityEnvelope::default_revision(),
                    );
                    let event = TelemetryEventBuilder::new(
                        EventType::Custom("barrier.timeout".to_string()),
                        LogLevel::Error,
                        format!(
                            "Agent {} timeout after {:?} at tick {}",
                            agent_id, timeout_duration, current_tick
                        ),
                        identity.clone(),
                    )
                    .component("adapteros-deterministic-exec".to_string())
                    .metadata(json!({
                        "agent_id": agent_id,
                        "tick": current_tick,
                        "generation": self.generation.load(Ordering::Acquire),
                        "wait_duration_ms": wait_start.elapsed().as_millis() as u64,
                        "total_agents": self.agent_ids.len(),
                        "timeout_seconds": timeout_duration.as_secs(),
                    }))
                    .build();

                    if let Ok(event) = event {
                        let _ = telemetry.log_event(event);
                    }
                }

                self.failed.store(true, Ordering::Release);
                self.notify.notify_waiters(); // Wake up all waiting agents
                return Err(CoordinationError::Timeout {
                    ticks: current_tick,
                });
            }

            // Check if barrier failed while we were waiting
            if self.failed.load(Ordering::Acquire) {
                return Err(CoordinationError::Failed {
                    reason: "Barrier failed due to timeout from another agent".into(),
                });
            }

            // Issue C-8: Skip dead agents when checking barrier condition
            let all_ready = {
                let ticks = self.agent_ticks.lock();
                let dead = self.dead_agents.lock();

                // Check if all LIVING agents have reached at least current_tick
                ticks
                    .iter()
                    .all(|(agent, &tick)| dead.contains(agent) || tick >= current_tick)
            };

            if all_ready {
                // All agents ready, try to advance generation
                // Issue C-1 fix: Properly handle CAS losers
                match self.generation.compare_exchange(
                    gen,
                    gen + 1,
                    Ordering::SeqCst,
                    Ordering::Acquire, // Use Acquire for losers to see new state
                ) {
                    Ok(_) => {
                        // We won the CAS - we're the one who advances the generation
                        info!(
                            agent_id = %agent_id,
                            tick = current_tick,
                            generation = gen + 1,
                            "All agents synchronized, generation advanced"
                        );

                        // Emit barrier.generation_advanced telemetry
                        if let Some(ref telemetry) = self.telemetry {
                            let dead_count = self.dead_agents.lock().len();
                            let living_count = self.agent_ids.len() - dead_count;
                            let cas_identity = IdentityEnvelope::new(
                                self.tenant_id.clone(),
                                "multi-agent".to_string(),
                                "cas".to_string(),
                                IdentityEnvelope::default_revision(),
                            );
                            let event =
                                TelemetryEventBuilder::new(
                                    EventType::Custom("barrier.generation_advanced".to_string()),
                                    LogLevel::Info,
                                    format!(
                                    "Agent {} won CAS race, generation {} → {} ({} living agents)",
                                    agent_id, gen, gen + 1, living_count
                                ),
                                    cas_identity.clone(),
                                )
                                .component("adapteros-deterministic-exec".to_string())
                                .metadata(json!({
                                    "agent_id": agent_id,
                                    "tick": current_tick,
                                    "generation": gen + 1,
                                    "wait_duration_ms": wait_start.elapsed().as_millis() as u64,
                                    "total_agents": self.agent_ids.len(),
                                    "living_agents": living_count,
                                    "dead_agents": dead_count,
                                }))
                                .build();

                            if let Ok(event) = event {
                                let _ = telemetry.log_event(event);
                            }
                        }

                        self.notify.notify_waiters(); // Wake everyone up
                        return Ok(());
                    }
                    Err(actual_gen) => {
                        // Another agent already advanced the generation
                        if actual_gen > gen {
                            debug!(
                                agent_id = %agent_id,
                                tick = current_tick,
                                expected_gen = gen,
                                actual_gen = actual_gen,
                                "Lost CAS race, but generation already advanced - proceeding"
                            );

                            // Emit barrier.cas_loser_proceed telemetry
                            if let Some(ref telemetry) = self.telemetry {
                                let identity = IdentityEnvelope::new(
                                    self.tenant_id.clone(),
                                    "multi-agent".to_string(),
                                    "cas".to_string(),
                                    IdentityEnvelope::default_revision(),
                                );
                                let event = TelemetryEventBuilder::new(
                                    EventType::Custom("barrier.cas_loser_proceed".to_string()),
                                    LogLevel::Debug,
                                    format!(
                                        "Agent {} lost CAS but generation advanced, proceeding",
                                        agent_id
                                    ),
                                    identity.clone(),
                                )
                                .component("adapteros-deterministic-exec".to_string())
                                .metadata(json!({
                                    "agent_id": agent_id,
                                    "tick": current_tick,
                                    "expected_gen": gen,
                                    "actual_gen": actual_gen,
                                    "wait_duration_ms": wait_start.elapsed().as_millis() as u64,
                                }))
                                .build();

                                if let Ok(event) = event {
                                    let _ = telemetry.log_event(event);
                                }
                            }

                            return Ok(());
                        }
                        // Generation changed but didn't advance - should retry
                        debug!(
                            agent_id = %agent_id,
                            "Generation changed unexpectedly, retrying"
                        );
                    }
                }
            }

            // Issue C-2 fix: Use Notify instead of busy-wait spin loop
            // Wait for notification with timeout to check failure flag periodically
            tokio::select! {
                _ = self.notify.notified() => {
                    // Another agent updated state, check again
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Periodic wakeup to check timeout and failure flag
                }
            }
        }
    }

    /// Get current generation
    pub fn generation(&self) -> u64 {
        // Use Acquire to ensure we see all updates that happened before generation increment
        self.generation.load(Ordering::Acquire)
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
        self.generation.store(0, Ordering::Release);
        self.failed.store(false, Ordering::Release);
    }
}

/// Get next global sequence number
///
/// This provides a global ordering across all agents and operations.
///
/// ## Issue C-4 Fix: Overflow Detection
/// Warns when approaching u64::MAX to prevent wraparound bugs.
pub fn next_global_seq() -> u64 {
    let seq = GLOBAL_SEQ_COUNTER.fetch_add(1, Ordering::SeqCst);

    // Issue C-4: Warn when approaching overflow
    const OVERFLOW_WARNING_THRESHOLD: u64 = u64::MAX - 1_000_000;
    if seq >= OVERFLOW_WARNING_THRESHOLD {
        warn!(
            sequence = seq,
            remaining = u64::MAX - seq,
            "Global sequence counter approaching overflow! Consider system restart."
        );
    }

    seq
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
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    // Test isolation: Ensures tests touching GLOBAL_SEQ_COUNTER run serially
    // Without this, parallel test execution causes flaky failures due to shared state
    static TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[tokio::test]
    async fn test_agent_barrier_single_agent() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset_global_seq();

        let barrier = AgentBarrier::new(vec!["agent-1".to_string()]);

        // Single agent should pass immediately
        barrier.wait("agent-1", 10).await.unwrap();

        assert_eq!(barrier.generation(), 1);
    }

    #[tokio::test]
    async fn test_agent_barrier_multiple_agents() {
        let _lock = TEST_LOCK.lock().unwrap();
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
        let _lock = TEST_LOCK.lock().unwrap();
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
        let _lock = TEST_LOCK.lock().unwrap();
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

    /// Stress test: 20 agents synchronizing at same tick
    /// Verifies no race conditions or deadlocks under high agent count
    #[tokio::test]
    async fn test_stress_many_agents() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset_global_seq();

        const AGENT_COUNT: usize = 20;
        let agent_ids: Vec<String> = (0..AGENT_COUNT).map(|i| format!("agent-{}", i)).collect();

        let barrier = Arc::new(AgentBarrier::new(agent_ids.clone()));

        // Spawn all agents to wait at tick 1000
        let handles: Vec<_> = agent_ids
            .iter()
            .map(|id| {
                let barrier = barrier.clone();
                let id = id.clone();
                tokio::spawn(async move { barrier.wait(&id, 1000).await })
            })
            .collect();

        // All should complete successfully
        for handle in handles {
            handle.await.unwrap().expect("Agent should synchronize");
        }

        // Generation should be exactly 1 (not AGENT_COUNT!)
        assert_eq!(
            barrier.generation(),
            1,
            "Generation should only increment once despite {} agents",
            AGENT_COUNT
        );
    }

    /// Stress test: Sequential barrier synchronization
    /// Verifies barrier works correctly for sequential sync points
    #[tokio::test]
    async fn test_stress_sequential_sync() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset_global_seq();

        let agent_ids = vec!["agent-1".to_string(), "agent-2".to_string()];

        // Multiple sequential barriers (realistic use case)
        for tick in [100, 200, 300, 400, 500] {
            let barrier = Arc::new(AgentBarrier::new(agent_ids.clone()));

            let handles: Vec<_> = agent_ids
                .iter()
                .map(|id| {
                    let barrier = barrier.clone();
                    let id = id.clone();
                    tokio::spawn(async move { barrier.wait(&id, tick).await })
                })
                .collect();

            for handle in handles {
                handle
                    .await
                    .unwrap()
                    .expect("Agents should synchronize at each tick");
            }

            assert_eq!(
                barrier.generation(),
                1,
                "Each barrier should have generation 1"
            );
        }
    }

    /// Stress test: Agent timeout scenario
    /// Verifies timeout broadcasts failure to all waiting agents
    #[tokio::test]
    async fn test_stress_agent_timeout() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset_global_seq();

        let agent_ids = vec![
            "agent-1".to_string(),
            "agent-2".to_string(),
            "agent-3".to_string(),
        ];
        let barrier = Arc::new(AgentBarrier::new(agent_ids.clone()));

        // Agent 1 and 2 wait, but agent 3 never shows up
        let barrier1 = barrier.clone();
        let handle1 = tokio::spawn(async move { barrier1.wait("agent-1", 100).await });
        let barrier2 = barrier.clone();
        let handle2 = tokio::spawn(async move { barrier2.wait("agent-2", 100).await });

        // Both should eventually fail with timeout or failure broadcast
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        // At least one should report a timeout/failure
        assert!(
            result1.is_err() || result2.is_err(),
            "At least one agent should detect the missing agent and fail"
        );

        // Verify it's a timeout or coordination failure
        if let Err(e) = result1 {
            assert!(
                matches!(
                    e,
                    CoordinationError::Timeout { .. } | CoordinationError::Failed { .. }
                ),
                "Should be timeout or failure, got: {:?}",
                e
            );
        }
        if let Err(e) = result2 {
            assert!(
                matches!(
                    e,
                    CoordinationError::Timeout { .. } | CoordinationError::Failed { .. }
                ),
                "Should be timeout or failure, got: {:?}",
                e
            );
        }
    }

    /// Stress test: Concurrent generation advancement
    /// Verifies only one agent wins the CAS race, others proceed correctly
    #[tokio::test]
    async fn test_stress_concurrent_generation_advancement() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset_global_seq();

        const AGENT_COUNT: usize = 10;
        let agent_ids: Vec<String> = (0..AGENT_COUNT).map(|i| format!("agent-{}", i)).collect();

        let barrier = Arc::new(AgentBarrier::new(agent_ids.clone()));

        // All agents wait at same tick simultaneously
        let handles: Vec<_> = agent_ids
            .iter()
            .map(|id| {
                let barrier = barrier.clone();
                let id = id.clone();
                tokio::spawn(async move { barrier.wait(&id, 500).await })
            })
            .collect();

        // All should complete (both CAS winner and losers)
        for handle in handles {
            handle
                .await
                .unwrap()
                .expect("All agents should synchronize, including CAS losers");
        }

        // Generation should be exactly 1 (only winner advances it)
        assert_eq!(
            barrier.generation(),
            1,
            "Generation should only increment once despite concurrent CAS attempts"
        );
    }

    /// High-frequency sequence counter
    /// Verifies overflow warning doesn't break functionality
    #[tokio::test]
    async fn test_stress_sequence_counter_high_values() {
        let _lock = TEST_LOCK.lock().unwrap();

        // Set counter near overflow threshold to trigger warning
        const NEAR_OVERFLOW: u64 = u64::MAX - 500;
        GLOBAL_SEQ_COUNTER.store(NEAR_OVERFLOW, Ordering::SeqCst);

        // Get sequences - should trigger warning but still work
        let seq1 = next_global_seq();
        let seq2 = next_global_seq();
        let seq3 = next_global_seq();

        assert_eq!(seq1, NEAR_OVERFLOW);
        assert_eq!(seq2, NEAR_OVERFLOW + 1);
        assert_eq!(seq3, NEAR_OVERFLOW + 2);

        // Reset for other tests
        reset_global_seq();
    }

    /// Test basic dead agent functionality
    /// Issue C-8: Verify that marking an agent dead allows remaining agents to proceed
    #[tokio::test]
    async fn test_mark_agent_dead_basic() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset_global_seq();

        let barrier = Arc::new(AgentBarrier::new(vec!["a".into(), "b".into(), "c".into()]));

        // Agent A and B sync, but C is dead
        let b_clone = barrier.clone();
        let agent_a_handle = tokio::spawn(async move {
            b_clone.wait("a", 100).await.unwrap();
        });

        let b_clone = barrier.clone();
        let agent_b_handle = tokio::spawn(async move {
            b_clone.wait("b", 100).await.unwrap();
        });

        // Wait a bit, then mark C as dead
        tokio::time::sleep(Duration::from_millis(100)).await;
        barrier.mark_agent_dead("c").unwrap();

        // A and B should now proceed
        agent_a_handle.await.unwrap();
        agent_b_handle.await.unwrap();

        assert_eq!(
            barrier.generation(),
            1,
            "Barrier should advance with 2/3 agents (C dead)"
        );
    }

    /// Test marking multiple agents dead sequentially
    /// Issue C-8: Verify barrier handles multiple dead agents correctly
    #[tokio::test]
    async fn test_mark_multiple_dead_sequential() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset_global_seq();

        let barrier = Arc::new(AgentBarrier::new(vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
        ]));

        // Only agent A and B alive
        let b_clone = barrier.clone();
        let agent_a_handle = tokio::spawn(async move {
            b_clone.wait("a", 200).await.unwrap();
        });

        let b_clone = barrier.clone();
        let agent_b_handle = tokio::spawn(async move {
            b_clone.wait("b", 200).await.unwrap();
        });

        // Mark C, D, E as dead
        tokio::time::sleep(Duration::from_millis(50)).await;
        barrier.mark_agent_dead("c").unwrap();
        barrier.mark_agent_dead("d").unwrap();
        barrier.mark_agent_dead("e").unwrap();

        // A and B should proceed
        agent_a_handle.await.unwrap();
        agent_b_handle.await.unwrap();

        assert_eq!(
            barrier.generation(),
            1,
            "Barrier should advance with 2/5 agents (3 dead)"
        );
    }

    /// Test 50 agents hitting barrier simultaneously
    /// Validates high-contention synchronization
    #[tokio::test]
    async fn test_barrier_50_agents_simultaneous() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset_global_seq();

        const AGENT_COUNT: usize = 50;
        let barrier = Arc::new(AgentBarrier::new(
            (0..AGENT_COUNT).map(|i| format!("agent-{}", i)).collect(),
        ));

        // All agents hit barrier simultaneously
        let handles: Vec<_> = (0..AGENT_COUNT)
            .map(|i| {
                let b = barrier.clone();
                let agent_id = format!("agent-{}", i);
                tokio::spawn(async move { b.wait(&agent_id, 1000).await.unwrap() })
            })
            .collect();

        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(
            barrier.generation(),
            1,
            "All 50 agents should synchronize successfully"
        );
    }

    /// Test barrier reuse across multiple rounds
    /// Validates barrier can be used for multiple synchronization points
    #[tokio::test]
    async fn test_barrier_multi_round_reuse() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset_global_seq();

        let barrier = Arc::new(AgentBarrier::new(vec!["a".into(), "b".into(), "c".into()]));

        // Run 10 rounds of synchronization
        for round in 0..10 {
            let mut handles = vec![];

            for agent in &["a", "b", "c"] {
                let b = barrier.clone();
                let agent_id = agent.to_string();
                let tick = (round + 1) * 100;
                let handle = tokio::spawn(async move {
                    b.wait(&agent_id, tick).await.unwrap();
                });
                handles.push(handle);
            }

            for h in handles {
                h.await.unwrap();
            }
        }

        assert_eq!(
            barrier.generation(),
            10,
            "Barrier should advance 10 times across 10 rounds"
        );
    }

    /// High-contention stress test: 7 agents, 5 rapid successive barriers
    /// Validates Issue C-1/C-2 fix: CAS losers correctly detect generation advancement
    /// and proceed without timeout even under rapid successive synchronization (<50ms apart).
    ///
    /// Before the fix, stale generation reads in the CAS loop could cause losers to
    /// spin indefinitely or timeout. This test ensures the fix (generation refresh inside
    /// loop + Notify-based waiting) works under realistic high-contention scenarios.
    #[tokio::test]
    async fn test_stress_rapid_successive_barriers_7_agents() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset_global_seq();

        const AGENT_COUNT: usize = 7;
        let agent_ids: Vec<String> = (0..AGENT_COUNT).map(|i| format!("agent-{}", i)).collect();

        let barrier = Arc::new(AgentBarrier::new(agent_ids.clone()));

        // Execute 5 rapid barrier synchronizations (ticks 100→104)
        // Target: <50ms between each barrier to maximize CAS contention
        for target_tick in 100..=104 {
            let handles: Vec<_> = agent_ids
                .iter()
                .map(|id| {
                    let b = barrier.clone();
                    let agent_id = id.clone();
                    tokio::spawn(async move {
                        b.wait(&agent_id, target_tick).await.expect(&format!(
                            "Agent {} should synchronize at tick {} without timeout",
                            agent_id, target_tick
                        ))
                    })
                })
                .collect();

            // Wait for all agents to complete this barrier
            for h in handles {
                h.await.unwrap();
            }
        }

        // Verify exactly 5 generations (one per synchronization)
        assert_eq!(
            barrier.generation(),
            5,
            "Generation should increment exactly 5 times for 5 successive barriers"
        );
    }
}
