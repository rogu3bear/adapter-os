//! Configuration for the multi-agent spawn system

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for spawning and coordinating agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpawnConfig {
    /// Number of agents to spawn (15-30 recommended)
    pub agent_count: u16,

    /// Path to agent binary (default: current executable with `agent worker` subcommand)
    pub agent_binary: Option<PathBuf>,

    /// Base directory for UDS sockets
    pub socket_dir: PathBuf,

    /// Base directory for agent PID files
    pub pid_dir: PathBuf,

    /// Task distribution strategy
    pub distribution_strategy: DistributionStrategy,

    /// Maximum time to wait for agents to spawn (seconds)
    pub spawn_timeout_secs: u64,

    /// Maximum time for task execution (seconds)
    pub task_timeout_secs: u64,

    /// Barrier synchronization timeout (seconds)
    pub barrier_timeout_secs: u64,

    /// Enable deterministic mode (use DeterministicExecutor patterns)
    pub deterministic_mode: bool,

    /// Global seed for deterministic execution (32 bytes, hex-encoded for config)
    pub global_seed: Option<[u8; 32]>,

    /// Path to the LoRA worker UDS socket for inference
    pub worker_socket: PathBuf,

    /// Maximum restart attempts per agent before marking dead
    pub max_agent_restarts: u32,

    /// Health check interval in milliseconds
    pub health_check_interval_ms: u64,
}

impl Default for AgentSpawnConfig {
    fn default() -> Self {
        Self {
            agent_count: 20,
            agent_binary: None,
            socket_dir: PathBuf::from("./var/run/agents"),
            pid_dir: PathBuf::from("./var/run/agents/pids"),
            distribution_strategy: DistributionStrategy::Semantic,
            spawn_timeout_secs: 60,
            task_timeout_secs: 600,
            barrier_timeout_secs: 30,
            deterministic_mode: false,
            global_seed: None,
            worker_socket: PathBuf::from("./var/run/aos-worker.sock"),
            max_agent_restarts: 3,
            health_check_interval_ms: 5000,
        }
    }
}

impl AgentSpawnConfig {
    /// Create a new builder for AgentSpawnConfig
    pub fn builder() -> AgentSpawnConfigBuilder {
        AgentSpawnConfigBuilder::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.agent_count == 0 {
            return Err("agent_count must be at least 1".into());
        }
        if self.agent_count > 100 {
            return Err("agent_count should not exceed 100".into());
        }
        if self.spawn_timeout_secs == 0 {
            return Err("spawn_timeout_secs must be greater than 0".into());
        }
        if self.task_timeout_secs == 0 {
            return Err("task_timeout_secs must be greater than 0".into());
        }
        Ok(())
    }

    /// Get the socket path for a specific agent
    pub fn agent_socket_path(&self, agent_id: &str) -> PathBuf {
        self.socket_dir.join(format!("{}.sock", agent_id))
    }

    /// Get the PID file path for a specific agent
    pub fn agent_pid_path(&self, agent_id: &str) -> PathBuf {
        self.pid_dir.join(format!("{}.pid", agent_id))
    }

    /// Generate agent IDs for the configured agent count
    pub fn generate_agent_ids(&self) -> Vec<String> {
        (0..self.agent_count)
            .map(|i| format!("agent-{:02}", i))
            .collect()
    }
}

/// Builder for AgentSpawnConfig
#[derive(Debug, Default)]
pub struct AgentSpawnConfigBuilder {
    config: AgentSpawnConfig,
}

impl AgentSpawnConfigBuilder {
    /// Set the number of agents to spawn
    pub fn agent_count(mut self, count: u16) -> Self {
        self.config.agent_count = count;
        self
    }

    /// Set the agent binary path
    pub fn agent_binary(mut self, path: PathBuf) -> Self {
        self.config.agent_binary = Some(path);
        self
    }

    /// Set the socket directory
    pub fn socket_dir(mut self, path: PathBuf) -> Self {
        self.config.socket_dir = path;
        self
    }

    /// Set the PID directory
    pub fn pid_dir(mut self, path: PathBuf) -> Self {
        self.config.pid_dir = path;
        self
    }

    /// Set the distribution strategy
    pub fn distribution_strategy(mut self, strategy: DistributionStrategy) -> Self {
        self.config.distribution_strategy = strategy;
        self
    }

    /// Set the spawn timeout
    pub fn spawn_timeout_secs(mut self, secs: u64) -> Self {
        self.config.spawn_timeout_secs = secs;
        self
    }

    /// Set the task timeout
    pub fn task_timeout_secs(mut self, secs: u64) -> Self {
        self.config.task_timeout_secs = secs;
        self
    }

    /// Set the barrier timeout
    pub fn barrier_timeout_secs(mut self, secs: u64) -> Self {
        self.config.barrier_timeout_secs = secs;
        self
    }

    /// Enable deterministic mode
    pub fn deterministic_mode(mut self, enabled: bool) -> Self {
        self.config.deterministic_mode = enabled;
        self
    }

    /// Set the global seed for deterministic execution
    pub fn global_seed(mut self, seed: [u8; 32]) -> Self {
        self.config.global_seed = Some(seed);
        self
    }

    /// Set the worker socket path
    pub fn worker_socket(mut self, path: PathBuf) -> Self {
        self.config.worker_socket = path;
        self
    }

    /// Set the maximum restart attempts per agent
    pub fn max_agent_restarts(mut self, count: u32) -> Self {
        self.config.max_agent_restarts = count;
        self
    }

    /// Set the health check interval
    pub fn health_check_interval_ms(mut self, ms: u64) -> Self {
        self.config.health_check_interval_ms = ms;
        self
    }

    /// Build the configuration
    pub fn build(self) -> AgentSpawnConfig {
        self.config
    }
}

/// Strategy for distributing work among agents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DistributionStrategy {
    /// Split work by file ownership (each agent owns specific files)
    FileOwnership,

    /// Split by AST regions (functions, modules, classes)
    AstRegion,

    /// Round-robin distribution of tasks
    RoundRobin,

    /// Semantic clustering (related code together)
    #[default]
    Semantic,
}

impl std::fmt::Display for DistributionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileOwnership => write!(f, "file"),
            Self::AstRegion => write!(f, "ast"),
            Self::RoundRobin => write!(f, "round-robin"),
            Self::Semantic => write!(f, "semantic"),
        }
    }
}

impl std::str::FromStr for DistributionStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "file" | "file_ownership" | "file-ownership" => Ok(Self::FileOwnership),
            "ast" | "ast_region" | "ast-region" => Ok(Self::AstRegion),
            "round-robin" | "round_robin" | "roundrobin" => Ok(Self::RoundRobin),
            "semantic" => Ok(Self::Semantic),
            _ => Err(format!("Unknown distribution strategy: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentSpawnConfig::default();
        assert_eq!(config.agent_count, 20);
        assert_eq!(config.distribution_strategy, DistributionStrategy::Semantic);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_builder() {
        let config = AgentSpawnConfig::builder()
            .agent_count(25)
            .distribution_strategy(DistributionStrategy::FileOwnership)
            .deterministic_mode(true)
            .build();

        assert_eq!(config.agent_count, 25);
        assert_eq!(
            config.distribution_strategy,
            DistributionStrategy::FileOwnership
        );
        assert!(config.deterministic_mode);
    }

    #[test]
    fn test_agent_paths() {
        let config = AgentSpawnConfig::default();
        let socket = config.agent_socket_path("agent-05");
        let pid = config.agent_pid_path("agent-05");

        assert!(socket.to_string_lossy().contains("agent-05.sock"));
        assert!(pid.to_string_lossy().contains("agent-05.pid"));
    }

    #[test]
    fn test_generate_agent_ids() {
        let mut config = AgentSpawnConfig::default();
        config.agent_count = 5;
        let ids = config.generate_agent_ids();

        assert_eq!(ids.len(), 5);
        assert_eq!(ids[0], "agent-00");
        assert_eq!(ids[4], "agent-04");
    }

    #[test]
    fn test_distribution_strategy_parse() {
        assert_eq!(
            "file".parse::<DistributionStrategy>().unwrap(),
            DistributionStrategy::FileOwnership
        );
        assert_eq!(
            "semantic".parse::<DistributionStrategy>().unwrap(),
            DistributionStrategy::Semantic
        );
        assert!("invalid".parse::<DistributionStrategy>().is_err());
    }

    #[test]
    fn test_validation() {
        let mut config = AgentSpawnConfig::default();
        assert!(config.validate().is_ok());

        config.agent_count = 0;
        assert!(config.validate().is_err());

        config.agent_count = 101;
        assert!(config.validate().is_err());
    }
}
