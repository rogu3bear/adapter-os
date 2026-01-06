//! Agent supervisor for lifecycle management
//!
//! Manages multiple agents, handles health monitoring, and coordinates
//! barrier synchronization.

use crate::agent::AgentHandle;
use crate::config::AgentSpawnConfig;
use crate::error::{AgentSpawnError, Result};
use crate::protocol::{AgentRequest, AgentResponse};
use adapteros_deterministic_exec::multi_agent::AgentBarrier;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Supervises multiple agents, handling lifecycle and failure recovery
pub struct AgentSupervisor {
    /// Active agent handles by ID
    agents: Arc<RwLock<HashMap<String, Arc<AgentHandle>>>>,

    /// Configuration
    config: AgentSpawnConfig,

    /// Agent barrier for synchronization
    barrier: Arc<AgentBarrier>,

    /// Restart counts per agent
    restart_counts: Arc<RwLock<HashMap<String, u32>>>,

    /// Health check task handle (reserved for future health monitoring)
    #[allow(dead_code)]
    health_task: RwLock<Option<JoinHandle<()>>>,

    /// Shutdown notification
    shutdown_notify: Arc<Notify>,

    /// Expected agent IDs
    agent_ids: Vec<String>,
}

impl AgentSupervisor {
    /// Create a new supervisor
    pub fn new(config: AgentSpawnConfig) -> Self {
        let agent_ids = config.generate_agent_ids();
        let barrier = Arc::new(AgentBarrier::new(agent_ids.clone()));

        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            config,
            barrier,
            restart_counts: Arc::new(RwLock::new(HashMap::new())),
            health_task: RwLock::new(None),
            shutdown_notify: Arc::new(Notify::new()),
            agent_ids,
        }
    }

    /// Create a supervisor with an existing barrier
    pub fn with_barrier(config: AgentSpawnConfig, barrier: Arc<AgentBarrier>) -> Self {
        let agent_ids = config.generate_agent_ids();

        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            config,
            barrier,
            restart_counts: Arc::new(RwLock::new(HashMap::new())),
            health_task: RwLock::new(None),
            shutdown_notify: Arc::new(Notify::new()),
            agent_ids,
        }
    }

    /// Get the list of expected agent IDs
    pub fn agent_ids(&self) -> &[String] {
        &self.agent_ids
    }

    /// Spawn all configured agents
    pub async fn spawn_all(&self) -> Result<()> {
        info!(agent_count = self.agent_ids.len(), "Spawning all agents");

        let spawn_timeout = Duration::from_secs(self.config.spawn_timeout_secs);

        // Spawn agents in parallel
        let spawn_futures: Vec<_> = self
            .agent_ids
            .iter()
            .map(|id| {
                let id = id.clone();
                let config = self.config.clone();
                async move {
                    let handle = AgentHandle::spawn(id.clone(), &config).await?;
                    handle.wait_ready(spawn_timeout).await?;
                    Ok::<_, AgentSpawnError>((id, Arc::new(handle)))
                }
            })
            .collect();

        let results = futures::future::join_all(spawn_futures).await;

        // Collect successful spawns
        let mut agents = self.agents.write();
        let mut failed_count = 0;

        for result in results {
            match result {
                Ok((id, handle)) => {
                    agents.insert(id.clone(), handle);
                    self.restart_counts.write().insert(id, 0);
                }
                Err(e) => {
                    error!(error = %e, "Failed to spawn agent");
                    failed_count += 1;
                }
            }
        }

        if failed_count == self.agent_ids.len() {
            return Err(AgentSpawnError::AllAgentsFailed);
        }

        info!(
            spawned = agents.len(),
            failed = failed_count,
            "Agent spawn complete"
        );

        Ok(())
    }

    /// Start health monitoring loop
    pub fn start_health_monitor(&self) -> JoinHandle<()> {
        let agents = self.agents.clone();
        let shutdown = self.shutdown_notify.clone();
        let barrier = self.barrier.clone();
        let restart_counts = self.restart_counts.clone();
        let config = self.config.clone();
        let max_restarts = self.config.max_agent_restarts;
        let interval = Duration::from_millis(self.config.health_check_interval_ms);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown.notified() => {
                        info!("Health monitor shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(interval) => {
                        // Check each agent's health
                        let agent_list: Vec<_> = agents.read().iter()
                            .map(|(id, handle)| (id.clone(), handle.clone()))
                            .collect();

                        for (id, handle) in agent_list {
                            if !handle.is_alive().await {
                                warn!(agent_id = %id, "Agent not alive");

                                let restart_count = *restart_counts.read().get(&id).unwrap_or(&0);

                                if restart_count >= max_restarts {
                                    error!(agent_id = %id, attempts = restart_count, "Agent exceeded max restarts, marking dead");
                                    if let Err(e) = barrier.mark_agent_dead(&id) {
                                        error!(agent_id = %id, error = %e, "Failed to mark agent dead in barrier");
                                    }
                                    agents.write().remove(&id);
                                } else {
                                    // Attempt restart
                                    info!(agent_id = %id, attempt = restart_count + 1, "Attempting to restart agent");
                                    *restart_counts.write().entry(id.clone()).or_insert(0) += 1;

                                    match AgentHandle::spawn(id.clone(), &config).await {
                                        Ok(new_handle) => {
                                            let timeout = Duration::from_secs(config.spawn_timeout_secs);
                                            if let Err(e) = new_handle.wait_ready(timeout).await {
                                                error!(agent_id = %id, error = %e, "Restarted agent failed to become ready");
                                            } else {
                                                agents.write().insert(id.clone(), Arc::new(new_handle));
                                                info!(agent_id = %id, "Agent restarted successfully");
                                            }
                                        }
                                        Err(e) => {
                                            error!(agent_id = %id, error = %e, "Failed to restart agent");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    /// Get an agent by ID
    pub fn get_agent(&self, agent_id: &str) -> Option<Arc<AgentHandle>> {
        self.agents.read().get(agent_id).cloned()
    }

    /// Get all active agents
    pub fn get_all_agents(&self) -> Vec<Arc<AgentHandle>> {
        self.agents.read().values().cloned().collect()
    }

    /// Get the number of active agents
    pub fn active_count(&self) -> usize {
        self.agents.read().len()
    }

    /// Broadcast a request to all agents
    pub async fn broadcast(&self, request: AgentRequest) -> Vec<Result<()>> {
        let agents: Vec<_> = self.agents.read().values().cloned().collect();

        let futures: Vec<_> = agents
            .iter()
            .map(|handle| {
                let req = request.clone();
                let h = handle.clone();
                async move { h.send(req).await }
            })
            .collect();

        futures::future::join_all(futures).await
    }

    /// Broadcast a request and collect responses
    pub async fn broadcast_with_responses(
        &self,
        request: AgentRequest,
        timeout: Duration,
    ) -> Vec<(String, Result<AgentResponse>)> {
        let agents: Vec<_> = self
            .agents
            .read()
            .iter()
            .map(|(id, h)| (id.clone(), h.clone()))
            .collect();

        let futures: Vec<_> = agents
            .iter()
            .map(|(id, handle)| {
                let req = request.clone();
                let h = handle.clone();
                let agent_id = id.clone();
                async move {
                    let result = h.request(req, timeout).await;
                    (agent_id, result)
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }

    /// Wait for all agents to reach a barrier
    pub async fn sync_barrier(&self, tick: u64, barrier_id: &str) -> Result<()> {
        info!(tick = tick, barrier_id = %barrier_id, "Synchronizing agents at barrier");

        // Send barrier request to all agents
        let request = AgentRequest::SyncBarrier {
            tick,
            barrier_id: barrier_id.to_string(),
        };

        let results = self.broadcast(request).await;

        // Check for failures
        for result in results {
            if let Err(e) = result {
                warn!(error = %e, "Failed to send barrier request to agent");
            }
        }

        // Wait for barrier (agents will call barrier.wait() in response)
        // In a real implementation, we'd have the orchestrator wait at the barrier too
        let timeout = Duration::from_secs(self.config.barrier_timeout_secs);
        let barrier_result =
            tokio::time::timeout(timeout, self.wait_for_barrier_responses(tick, barrier_id)).await;

        match barrier_result {
            Ok(Ok(())) => {
                info!(tick = tick, barrier_id = %barrier_id, "All agents synchronized");
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                let missing: Vec<_> = self
                    .agents
                    .read()
                    .keys()
                    .filter(|id| self.barrier.agent_tick(id).is_none_or(|t| t < tick))
                    .cloned()
                    .collect();

                Err(AgentSpawnError::barrier_timeout(tick, missing))
            }
        }
    }

    /// Wait for all agents to respond to barrier
    async fn wait_for_barrier_responses(&self, tick: u64, barrier_id: &str) -> Result<()> {
        let timeout = Duration::from_secs(self.config.barrier_timeout_secs);

        let agents: Vec<_> = self.agents.read().values().cloned().collect();

        for handle in agents {
            match handle.recv(timeout).await {
                Ok(AgentResponse::BarrierReached {
                    tick: t,
                    barrier_id: b,
                }) => {
                    if t != tick || b != barrier_id {
                        warn!(
                            agent_id = %handle.id,
                            expected_tick = tick,
                            got_tick = t,
                            "Barrier response mismatch"
                        );
                    }
                    debug!(agent_id = %handle.id, tick = t, "Agent reached barrier");
                }
                Ok(other) => {
                    warn!(
                        agent_id = %handle.id,
                        response = ?other,
                        "Unexpected response while waiting for barrier"
                    );
                }
                Err(e) => {
                    warn!(agent_id = %handle.id, error = %e, "Error receiving barrier response");
                }
            }
        }

        Ok(())
    }

    /// Shutdown all agents gracefully
    pub async fn shutdown_all(&self, drain_timeout: Duration) -> Result<()> {
        info!("Shutting down all agents");

        // Signal health monitor to stop
        self.shutdown_notify.notify_waiters();

        // Get all agents
        let agents: Vec<_> = self.agents.write().drain().map(|(_, h)| h).collect();

        // Shutdown each agent
        let shutdown_futures: Vec<_> = agents
            .iter()
            .map(|handle| handle.shutdown(drain_timeout))
            .collect();

        let results = futures::future::join_all(shutdown_futures).await;

        let failed = results.iter().filter(|r| r.is_err()).count();
        if failed > 0 {
            warn!(
                failed_count = failed,
                "Some agents failed to shutdown gracefully"
            );
        }

        info!("All agents shutdown complete");
        Ok(())
    }

    /// Get the barrier for external synchronization
    pub fn barrier(&self) -> Arc<AgentBarrier> {
        self.barrier.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supervisor_creation() {
        let config = AgentSpawnConfig::builder().agent_count(5).build();

        let supervisor = AgentSupervisor::new(config);
        assert_eq!(supervisor.agent_ids().len(), 5);
        assert_eq!(supervisor.active_count(), 0);
    }
}
