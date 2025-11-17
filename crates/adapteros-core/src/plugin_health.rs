//! Plugin health monitoring and restart policy
//!
//! Implements watchdog, circuit breaker, and restart backoff for plugin isolation.
//! Citation: PRD 7 - Operator / Plugin Isolation

use crate::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Maximum restart attempts before giving up
const DEFAULT_MAX_RESTART_ATTEMPTS: u32 = 3;

/// Initial backoff duration
const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_secs(5);

/// Maximum backoff duration
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(300); // 5 minutes

/// Circuit breaker failure threshold
const DEFAULT_FAILURE_THRESHOLD: u32 = 5;

/// Circuit breaker recovery window
const DEFAULT_RECOVERY_WINDOW: Duration = Duration::from_secs(60);

/// Restart policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartPolicy {
    /// Maximum number of restart attempts
    pub max_attempts: u32,

    /// Initial backoff duration
    pub initial_backoff: Duration,

    /// Maximum backoff duration
    pub max_backoff: Duration,

    /// Backoff multiplier (exponential backoff)
    pub backoff_multiplier: f64,

    /// Enable circuit breaker
    pub enable_circuit_breaker: bool,

    /// Circuit breaker failure threshold
    pub failure_threshold: u32,

    /// Circuit breaker recovery window
    pub recovery_window: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_MAX_RESTART_ATTEMPTS,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            backoff_multiplier: 2.0,
            enable_circuit_breaker: true,
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            recovery_window: DEFAULT_RECOVERY_WINDOW,
        }
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests are rejected
    Open,
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker for preventing cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<RwLock<u32>>,
    last_failure: Arc<RwLock<Option<Instant>>>,
    config: RestartPolicy,
}

impl CircuitBreaker {
    pub fn new(config: RestartPolicy) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(RwLock::new(0)),
            last_failure: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Check if circuit breaker allows operation
    pub async fn allow_request(&self) -> bool {
        let state = self.state.read().await.clone();

        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if recovery window has passed
                if let Some(last_failure) = *self.last_failure.read().await {
                    if last_failure.elapsed() > self.config.recovery_window {
                        // Transition to half-open to test recovery
                        *self.state.write().await = CircuitState::HalfOpen;
                        info!("Circuit breaker transitioning to half-open state");
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record successful operation
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;
        let mut failure_count = self.failure_count.write().await;

        if *state == CircuitState::HalfOpen {
            // Recovery successful, close circuit
            *state = CircuitState::Closed;
            *failure_count = 0;
            info!("Circuit breaker closed after successful recovery");
        } else if *state == CircuitState::Closed {
            // Reset failure count on success
            *failure_count = 0;
        }
    }

    /// Record failed operation
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;
        let mut failure_count = self.failure_count.write().await;
        let mut last_failure = self.last_failure.write().await;

        *failure_count += 1;
        *last_failure = Some(Instant::now());

        if *failure_count >= self.config.failure_threshold {
            if *state != CircuitState::Open {
                *state = CircuitState::Open;
                warn!(
                    failure_count = *failure_count,
                    threshold = self.config.failure_threshold,
                    "Circuit breaker opened due to excessive failures"
                );
            }
        }
    }

    /// Get current circuit state
    pub async fn state(&self) -> CircuitState {
        self.state.read().await.clone()
    }

    /// Get current failure count
    pub async fn failure_count(&self) -> u32 {
        *self.failure_count.read().await
    }
}

/// Plugin restart state
#[derive(Debug)]
struct RestartState {
    attempt_count: u32,
    last_restart: Option<Instant>,
    next_backoff: Duration,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl RestartState {
    fn new(policy: &RestartPolicy) -> Self {
        Self {
            attempt_count: 0,
            last_restart: None,
            next_backoff: policy.initial_backoff,
            circuit_breaker: Arc::new(CircuitBreaker::new(policy.clone())),
        }
    }

    fn calculate_backoff(&mut self, policy: &RestartPolicy) -> Duration {
        let backoff = self.next_backoff;

        // Exponential backoff with cap
        let next_secs = (self.next_backoff.as_secs_f64() * policy.backoff_multiplier)
            .min(policy.max_backoff.as_secs_f64());
        self.next_backoff = Duration::from_secs_f64(next_secs);

        backoff
    }

    fn reset(&mut self, policy: &RestartPolicy) {
        self.attempt_count = 0;
        self.last_restart = None;
        self.next_backoff = policy.initial_backoff;
    }
}

/// Plugin watchdog for monitoring and restarting failed plugins
#[derive(Debug)]
pub struct PluginWatchdog {
    restart_states: Arc<RwLock<HashMap<String, RestartState>>>,
    policy: RestartPolicy,
}

impl PluginWatchdog {
    pub fn new(policy: RestartPolicy) -> Self {
        Self {
            restart_states: Arc::new(RwLock::new(HashMap::new())),
            policy,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(RestartPolicy::default())
    }

    /// Check if plugin can be restarted based on policy
    pub async fn can_restart(&self, plugin_name: &str) -> Result<bool> {
        let mut states = self.restart_states.write().await;
        let state = states
            .entry(plugin_name.to_string())
            .or_insert_with(|| RestartState::new(&self.policy));

        // Check circuit breaker
        if self.policy.enable_circuit_breaker {
            if !state.circuit_breaker.allow_request().await {
                warn!(
                    plugin_name,
                    circuit_state = ?state.circuit_breaker.state().await,
                    "Plugin restart blocked by circuit breaker"
                );
                return Ok(false);
            }
        }

        // Check restart attempts
        if state.attempt_count >= self.policy.max_attempts {
            error!(
                plugin_name,
                attempt_count = state.attempt_count,
                max_attempts = self.policy.max_attempts,
                "Plugin exceeded maximum restart attempts"
            );
            return Ok(false);
        }

        // Check backoff period
        if let Some(last_restart) = state.last_restart {
            let backoff = state.calculate_backoff(&self.policy);
            if last_restart.elapsed() < backoff {
                let remaining = backoff.saturating_sub(last_restart.elapsed());
                info!(
                    plugin_name,
                    remaining_secs = remaining.as_secs(),
                    "Plugin restart delayed due to backoff"
                );
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Record restart attempt
    pub async fn record_restart(&self, plugin_name: &str) -> Result<()> {
        let mut states = self.restart_states.write().await;
        let state = states
            .entry(plugin_name.to_string())
            .or_insert_with(|| RestartState::new(&self.policy));

        state.attempt_count += 1;
        state.last_restart = Some(Instant::now());

        info!(
            plugin_name,
            attempt = state.attempt_count,
            max_attempts = self.policy.max_attempts,
            next_backoff_secs = state.next_backoff.as_secs(),
            "Plugin restart attempt recorded"
        );

        Ok(())
    }

    /// Record successful restart (resets attempt counter)
    pub async fn record_success(&self, plugin_name: &str) -> Result<()> {
        let mut states = self.restart_states.write().await;
        if let Some(state) = states.get_mut(plugin_name) {
            info!(
                plugin_name,
                previous_attempts = state.attempt_count,
                "Plugin restart successful, resetting counters"
            );
            state.reset(&self.policy);

            // Record circuit breaker success
            if self.policy.enable_circuit_breaker {
                state.circuit_breaker.record_success().await;
            }
        }
        Ok(())
    }

    /// Record failed restart
    pub async fn record_failure(&self, plugin_name: &str) -> Result<()> {
        let states = self.restart_states.read().await;
        if let Some(state) = states.get(plugin_name) {
            warn!(
                plugin_name,
                attempt = state.attempt_count,
                "Plugin restart failed"
            );

            // Record circuit breaker failure
            if self.policy.enable_circuit_breaker {
                state.circuit_breaker.record_failure().await;
            }
        }
        Ok(())
    }

    /// Get restart state for plugin
    pub async fn get_state(&self, plugin_name: &str) -> Option<(u32, CircuitState)> {
        let states = self.restart_states.read().await;
        if let Some(state) = states.get(plugin_name) {
            Some((
                state.attempt_count,
                state.circuit_breaker.state().await,
            ))
        } else {
            None
        }
    }

    /// Reset restart state for plugin (admin operation)
    pub async fn reset(&self, plugin_name: &str) -> Result<()> {
        let mut states = self.restart_states.write().await;
        if let Some(state) = states.get_mut(plugin_name) {
            info!(plugin_name, "Manually resetting plugin restart state");
            state.reset(&self.policy);

            // Reset circuit breaker
            if self.policy.enable_circuit_breaker {
                *state.circuit_breaker.state.write().await = CircuitState::Closed;
                *state.circuit_breaker.failure_count.write().await = 0;
            }
        }
        Ok(())
    }

    /// Quiesce plugin (prevent further restarts)
    pub async fn quiesce(&self, plugin_name: &str) -> Result<()> {
        let mut states = self.restart_states.write().await;
        if let Some(state) = states.get_mut(plugin_name) {
            warn!(plugin_name, "Quiescing plugin, no further restarts allowed");
            state.attempt_count = self.policy.max_attempts; // Prevent restarts

            // Open circuit breaker
            if self.policy.enable_circuit_breaker {
                *state.circuit_breaker.state.write().await = CircuitState::Open;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_transitions() {
        let policy = RestartPolicy {
            failure_threshold: 3,
            recovery_window: Duration::from_millis(100),
            ..Default::default()
        };
        let cb = CircuitBreaker::new(policy);

        // Initially closed
        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.allow_request().await);

        // Record failures to open circuit
        cb.record_failure().await;
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
        assert!(!cb.allow_request().await);

        // Wait for recovery window
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should transition to half-open
        assert!(cb.allow_request().await);
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Success closes circuit
        cb.record_success().await;
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_watchdog_restart_limits() {
        let policy = RestartPolicy {
            max_attempts: 2,
            initial_backoff: Duration::from_millis(10),
            ..Default::default()
        };
        let watchdog = PluginWatchdog::new(policy);

        // First restart should be allowed
        assert!(watchdog.can_restart("test").await.unwrap());
        watchdog.record_restart("test").await.unwrap();

        // Second restart should be allowed after backoff
        tokio::time::sleep(Duration::from_millis(15)).await;
        assert!(watchdog.can_restart("test").await.unwrap());
        watchdog.record_restart("test").await.unwrap();

        // Third restart should be denied (max attempts reached)
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(!watchdog.can_restart("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_watchdog_backoff() {
        let policy = RestartPolicy {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(10),
            backoff_multiplier: 2.0,
            ..Default::default()
        };
        let watchdog = PluginWatchdog::new(policy);

        // First restart
        watchdog.record_restart("test").await.unwrap();

        // Should be blocked before backoff expires
        assert!(!watchdog.can_restart("test").await.unwrap());

        // Wait for first backoff (10ms)
        tokio::time::sleep(Duration::from_millis(15)).await;
        assert!(watchdog.can_restart("test").await.unwrap());

        // Second restart
        watchdog.record_restart("test").await.unwrap();

        // Wait for second backoff (20ms due to 2x multiplier)
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(watchdog.can_restart("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_watchdog_reset() {
        let policy = RestartPolicy {
            max_attempts: 2,
            initial_backoff: Duration::from_millis(10),
            ..Default::default()
        };
        let watchdog = PluginWatchdog::new(policy);

        // Exhaust restart attempts
        watchdog.record_restart("test").await.unwrap();
        tokio::time::sleep(Duration::from_millis(15)).await;
        watchdog.record_restart("test").await.unwrap();
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(!watchdog.can_restart("test").await.unwrap());

        // Record success should reset
        watchdog.record_success("test").await.unwrap();
        assert!(watchdog.can_restart("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_watchdog_quiesce() {
        let watchdog = PluginWatchdog::with_defaults();

        // Initially can restart
        assert!(watchdog.can_restart("test").await.unwrap());

        // Quiesce plugin
        watchdog.quiesce("test").await.unwrap();

        // Should not allow restart
        assert!(!watchdog.can_restart("test").await.unwrap());
    }
}
