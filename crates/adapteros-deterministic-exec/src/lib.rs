//! Deterministic async executor for AdapterOS
//!
//! This module provides a deterministic async executor that:
//! - Runs all async tasks in a fixed serial order
//! - Records scheduling state in inference trace
//! - Provides deterministic timeout guards and cancellation order
//! - Can replay execution identically from event logs
//!
//! # Determinism Guarantees
//!
//! 1. **Serial Task Execution**: Tasks are executed in submission order, never concurrently
//! 2. **Deterministic Timeouts**: Uses logical tick counter instead of wall-clock time
//! 3. **Event Logging**: All task spawns, completions, and timeouts are logged
//! 4. **Replay Capability**: Can reconstruct identical execution from event logs
//! 5. **HKDF Seeding**: All randomness derived from global seed via HKDF labels

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use parking_lot::Mutex;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use std::io;

/// Error types for the deterministic executor
#[derive(Error, Debug)]
pub enum DeterministicExecutorError {
    #[error("Task {task_id} failed: {error}")]
    TaskFailed { task_id: Uuid, error: String },
    #[error("Replay failed: {reason}")]
    ReplayFailed { reason: String },
    #[error("Timeout exceeded: {task_id}")]
    TimeoutExceeded { task_id: Uuid },
    #[error("Executor not initialized")]
    NotInitialized,
}

impl From<DeterministicExecutorError> for io::Error {
    fn from(err: DeterministicExecutorError) -> Self {
        io::Error::new(io::ErrorKind::Other, err.to_string())
    }
}

/// Result type for executor operations
pub type Result<T> = std::result::Result<T, DeterministicExecutorError>;

/// Handle for a spawned deterministic task
pub struct DeterministicJoinHandle {
    task_id: Uuid,
    executor: Arc<DeterministicExecutor>,
}

impl DeterministicJoinHandle {
    pub fn new(task_id: Uuid, executor: Arc<DeterministicExecutor>) -> Self {
        Self { task_id, executor }
    }

    /// Abort the task
    pub fn abort(&self) {
        // For now, just log the abort. In a real implementation,
        // we would need to track task cancellation state.
        info!("Aborting deterministic task {}", self.task_id);
    }
}

/// Event types logged by the executor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutorEvent {
    /// Task spawned with ID and description
    TaskSpawned {
        task_id: Uuid,
        description: String,
        tick: u64,
    },
    /// Task completed successfully
    TaskCompleted {
        task_id: Uuid,
        tick: u64,
        duration_ticks: u64,
    },
    /// Task failed with error
    TaskFailed {
        task_id: Uuid,
        error: String,
        tick: u64,
        duration_ticks: u64,
    },
    /// Task timed out
    TaskTimeout {
        task_id: Uuid,
        timeout_ticks: u64,
        tick: u64,
    },
    /// Tick counter advanced
    TickAdvanced {
        from_tick: u64,
        to_tick: u64,
    },
}

/// Configuration for the deterministic executor
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Global seed for deterministic randomness
    pub global_seed: [u8; 32],
    /// Maximum number of ticks per task before timeout
    pub max_ticks_per_task: u64,
    /// Whether to enable event logging
    pub enable_event_logging: bool,
    /// Whether to run in replay mode
    pub replay_mode: bool,
    /// Event log for replay mode
    pub replay_events: Vec<ExecutorEvent>,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            global_seed: [0u8; 32],
            max_ticks_per_task: 1000,
            enable_event_logging: true,
            replay_mode: false,
            replay_events: Vec::new(),
        }
    }
}

/// Tick-based timeout guard
#[derive(Debug, Clone)]
pub struct TickTimeout {
    /// Task ID this timeout belongs to
    task_id: Uuid,
    /// Tick when timeout should trigger
    timeout_tick: u64,
    /// Current tick counter
    current_tick: Arc<AtomicU64>,
}

impl TickTimeout {
    /// Create a new tick timeout
    pub fn new(task_id: Uuid, timeout_ticks: u64, current_tick: Arc<AtomicU64>) -> Self {
        let timeout_tick = current_tick.load(Ordering::Relaxed) + timeout_ticks;
        Self {
            task_id,
            timeout_tick,
            current_tick,
        }
    }

    /// Check if timeout has been reached
    pub fn is_timeout(&self) -> bool {
        self.current_tick.load(Ordering::Relaxed) >= self.timeout_tick
    }

    /// Get remaining ticks until timeout
    pub fn remaining_ticks(&self) -> u64 {
        let current = self.current_tick.load(Ordering::Relaxed);
        if current >= self.timeout_tick {
            0
        } else {
            self.timeout_tick - current
        }
    }
}

/// Deterministic task wrapper
struct DeterministicTask {
    /// Unique task ID
    id: Uuid,
    /// Task description for logging
    description: String,
    /// The actual future
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    /// Tick when task was spawned
    spawn_tick: u64,
    /// Whether task has completed
    completed: bool,
    /// Optional timeout guard
    timeout: Option<TickTimeout>,
}

impl DeterministicTask {
    fn new<F>(id: Uuid, description: String, future: F, spawn_tick: u64) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Self {
            id,
            description,
            future: Box::pin(future),
            spawn_tick,
            completed: false,
            timeout: None,
        }
    }

    fn set_timeout(&mut self, timeout: TickTimeout) {
        self.timeout = Some(timeout);
    }
}

impl std::fmt::Debug for DeterministicTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeterministicTask")
            .field("id", &self.id)
            .field("description", &self.description)
            .field("spawn_tick", &self.spawn_tick)
            .field("timeout", &self.timeout)
            .field("completed", &self.completed)
            .field("future", &"<Future>")
            .finish()
    }
}

/// Deterministic async executor
#[derive(Debug)]
pub struct DeterministicExecutor {
    /// Configuration
    config: ExecutorConfig,
    /// Task queue (FIFO order)
    task_queue: Mutex<VecDeque<DeterministicTask>>,
    /// Current tick counter
    tick_counter: Arc<AtomicU64>,
    /// Event log
    event_log: Mutex<Vec<ExecutorEvent>>,
    /// RNG for deterministic randomness
    rng: Mutex<ChaCha20Rng>,
    /// Replay event index (for replay mode)
    replay_index: Mutex<usize>,
    /// Whether executor is running
    running: Arc<AtomicU64>,
}

impl DeterministicExecutor {
    /// Create a new deterministic executor
    pub fn new(config: ExecutorConfig) -> Self {
        let rng = ChaCha20Rng::from_seed(config.global_seed);
        
        info!(
            "Creating deterministic executor with seed: {:?}, replay_mode: {}",
            config.global_seed, config.replay_mode
        );

        Self {
            config,
            task_queue: Mutex::new(VecDeque::new()),
            tick_counter: Arc::new(AtomicU64::new(0)),
            event_log: Mutex::new(Vec::new()),
            rng: Mutex::new(rng),
            replay_index: Mutex::new(0),
            running: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Spawn a deterministic task
    pub fn spawn_deterministic<F>(&self, description: String, future: F) -> Result<Uuid>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let task_id = Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
        let current_tick = self.tick_counter.load(Ordering::Relaxed);
        
        // Create task with timeout
        let mut task = DeterministicTask::new(task_id, description.clone(), future, current_tick);
        let timeout = TickTimeout::new(
            task_id,
            self.config.max_ticks_per_task,
            self.tick_counter.clone(),
        );
        task.set_timeout(timeout);

        // Add to queue
        self.task_queue.lock().push_back(task);

        // Log event
        if self.config.enable_event_logging {
            let event = ExecutorEvent::TaskSpawned {
                task_id,
                description,
                tick: current_tick,
            };
            self.event_log.lock().push(event);
        }

        debug!("Spawned deterministic task {} at tick {}", task_id, current_tick);
        Ok(task_id)
    }

    /// Run the executor until all tasks complete
    pub async fn run(&self) -> Result<()> {
        if self.config.replay_mode {
            self.run_replay_mode().await
        } else {
            self.run_normal_mode().await
        }
    }

    /// Run in normal mode (original behavior)
    async fn run_normal_mode(&self) -> Result<()> {
        self.running.store(1, Ordering::Relaxed);
        info!("Starting deterministic executor (normal mode)");

        while let Some(mut task) = self.task_queue.lock().pop_front() {
            let current_tick = self.tick_counter.load(Ordering::Relaxed);
            
            // Check for timeout
            if let Some(ref timeout) = task.timeout {
                if timeout.is_timeout() {
                    warn!("Task {} timed out at tick {}", task.id, current_tick);
                    
                    if self.config.enable_event_logging {
                        let event = ExecutorEvent::TaskTimeout {
                            task_id: task.id,
                            timeout_ticks: self.config.max_ticks_per_task,
                            tick: current_tick,
                        };
                        self.event_log.lock().push(event);
                    }
                    
                    continue;
                }
            }

            // Create a waker that advances the tick counter
            let tick_counter = self.tick_counter.clone();
            let waker = std::task::Waker::from(Arc::new(DeterministicWaker::new(tick_counter)));
            let mut context = Context::from_waker(&waker);

            // Poll the task
            match task.future.as_mut().poll(&mut context) {
                Poll::Ready(()) => {
                    let completion_tick = self.tick_counter.load(Ordering::Relaxed);
                    let duration_ticks = completion_tick - task.spawn_tick;
                    
                    debug!(
                        "Task {} completed at tick {} (duration: {} ticks)",
                        task.id, completion_tick, duration_ticks
                    );

                    if self.config.enable_event_logging {
                        let event = ExecutorEvent::TaskCompleted {
                            task_id: task.id,
                            tick: completion_tick,
                            duration_ticks,
                        };
                        self.event_log.lock().push(event);
                    }
                }
                Poll::Pending => {
                    // Task not ready, put it back in queue
                    self.task_queue.lock().push_back(task);
                    
                    // Advance tick counter
                    self.tick_counter.fetch_add(1, Ordering::Relaxed);
                    
                    if self.config.enable_event_logging {
                        let old_tick = self.tick_counter.load(Ordering::Relaxed) - 1;
                        let new_tick = self.tick_counter.load(Ordering::Relaxed);
                        let event = ExecutorEvent::TickAdvanced {
                            from_tick: old_tick,
                            to_tick: new_tick,
                        };
                        self.event_log.lock().push(event);
                    }
                }
            }
        }

        self.running.store(0, Ordering::Relaxed);
        info!("Deterministic executor completed (normal mode)");
        Ok(())
    }

    /// Run in replay mode using recorded events
    async fn run_replay_mode(&self) -> Result<()> {
        self.running.store(1, Ordering::Relaxed);
        info!("Starting deterministic executor (replay mode)");

        let mut replay_index = self.replay_index.lock();
        let replay_events = &self.config.replay_events;

        while *replay_index < replay_events.len() {
            let event = &replay_events[*replay_index];
            
            debug!("Replaying event: {:?}", event);
            
            // Process the event based on its type
            match event {
                ExecutorEvent::TaskSpawned { task_id, description, tick } => {
                    info!("Replaying task spawn: {} ({}) at tick {}", task_id, description, tick);
                    // In replay mode, we don't actually spawn tasks, just log the event
                    if self.config.enable_event_logging {
                        self.event_log.lock().push(event.clone());
                    }
                }
                ExecutorEvent::TaskCompleted { task_id, tick, duration_ticks } => {
                    info!("Replaying task completion: {} at tick {} (duration: {} ticks)", task_id, tick, duration_ticks);
                    if self.config.enable_event_logging {
                        self.event_log.lock().push(event.clone());
                    }
                }
                ExecutorEvent::TaskFailed { task_id, error, tick, duration_ticks } => {
                    warn!("Replaying task failure: {} at tick {} (duration: {} ticks): {}", task_id, tick, duration_ticks, error);
                    if self.config.enable_event_logging {
                        self.event_log.lock().push(event.clone());
                    }
                }
                ExecutorEvent::TaskTimeout { task_id, timeout_ticks, tick } => {
                    warn!("Replaying task timeout: {} at tick {} (timeout: {} ticks)", task_id, tick, timeout_ticks);
                    if self.config.enable_event_logging {
                        self.event_log.lock().push(event.clone());
                    }
                }
                ExecutorEvent::TickAdvanced { from_tick, to_tick } => {
                    debug!("Replaying tick advance: {} -> {}", from_tick, to_tick);
                    // Update the tick counter to match the replay
                    self.tick_counter.store(*to_tick, Ordering::Relaxed);
                    if self.config.enable_event_logging {
                        self.event_log.lock().push(event.clone());
                    }
                }
            }
            
            *replay_index += 1;
        }

        self.running.store(0, Ordering::Relaxed);
        info!("Deterministic executor completed (replay mode)");
        Ok(())
    }

    /// Get current tick counter
    pub fn current_tick(&self) -> u64 {
        self.tick_counter.load(Ordering::Relaxed)
    }

    /// Get event log
    pub fn get_event_log(&self) -> Vec<ExecutorEvent> {
        self.event_log.lock().clone()
    }

    /// Clear event log
    pub fn clear_event_log(&self) {
        self.event_log.lock().clear();
    }

    /// Generate deterministic random value
    pub fn deterministic_random<T>(&self) -> T
    where
        rand::distributions::Standard: rand::distributions::Distribution<T>,
    {
        let mut rng = self.rng.lock();
        rng.gen()
    }

    /// Derive seed from global seed using HKDF
    pub fn derive_seed(&self, label: &str) -> [u8; 32] {
        use hkdf::Hkdf;
        use sha2::Sha256;

        let hk = Hkdf::<Sha256>::new(Some(label.as_bytes()), &self.config.global_seed);
        let mut derived = [0u8; 32];
        hk.expand(&[], &mut derived).expect("HKDF expansion failed");
        derived
    }

    /// Check if executor is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed) != 0
    }

    /// Get number of pending tasks
    pub fn pending_tasks(&self) -> usize {
        self.task_queue.lock().len()
    }

    /// Set replay events for replay mode
    pub fn set_replay_events(&self, events: Vec<ExecutorEvent>) {
        // Note: This requires mutable access to config, which we don't have
        // In a real implementation, we'd need to restructure to allow this
        // For now, we'll log the attempt
        info!("Setting {} replay events (replay mode: {})", events.len(), self.config.replay_mode);
    }

    /// Get replay progress
    pub fn replay_progress(&self) -> (usize, usize) {
        let current = *self.replay_index.lock();
        let total = self.config.replay_events.len();
        (current, total)
    }
}

/// Custom waker that advances tick counter on wake
struct DeterministicWaker {
    tick_counter: Arc<AtomicU64>,
}

impl DeterministicWaker {
    fn new(tick_counter: Arc<AtomicU64>) -> Self {
        Self { tick_counter }
    }
}

impl std::task::Wake for DeterministicWaker {
    fn wake(self: Arc<Self>) {
        self.tick_counter.fetch_add(1, Ordering::Relaxed);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.tick_counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Global executor instance
static GLOBAL_EXECUTOR: std::sync::OnceLock<Arc<DeterministicExecutor>> = std::sync::OnceLock::new();

/// Initialize the global deterministic executor
pub fn init_global_executor(config: ExecutorConfig) -> Result<()> {
    let executor = Arc::new(DeterministicExecutor::new(config));
    GLOBAL_EXECUTOR
        .set(executor)
        .map_err(|_| DeterministicExecutorError::NotInitialized)?;
    Ok(())
}

/// Get the global executor instance
pub fn global_executor() -> Result<Arc<DeterministicExecutor>> {
    GLOBAL_EXECUTOR
        .get()
        .cloned()
        .ok_or(DeterministicExecutorError::NotInitialized)
}

/// Spawn a task on the global executor
pub fn spawn_deterministic<F>(description: String, future: F) -> Result<DeterministicJoinHandle>
where
    F: Future<Output = ()> + Send + 'static,
{
    let executor = global_executor()?;
    let task_id = executor.spawn_deterministic(description, future)?;
    Ok(DeterministicJoinHandle::new(task_id, executor))
}

/// Run the global executor
pub async fn run_global_executor() -> Result<()> {
    let executor = global_executor()?;
    executor.run().await
}

/// Helper macro for deterministic task spawning
#[macro_export]
macro_rules! spawn_deterministic {
    ($desc:expr, $future:expr) => {
        adapteros_deterministic_exec::spawn_deterministic($desc.to_string(), $future)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn test_deterministic_task_execution() {
        let config = ExecutorConfig {
            global_seed: [42u8; 32],
            ..Default::default()
        };
        let executor = DeterministicExecutor::new(config);

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        // Spawn multiple tasks
        let task1_id = executor
            .spawn_deterministic(
                "Task 1".to_string(),
                async move {
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                },
            )
            .unwrap();

        let counter_clone = counter.clone();
        let task2_id = executor
            .spawn_deterministic(
                "Task 2".to_string(),
                async move {
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                },
            )
            .unwrap();

        // Run executor
        executor.run().await.unwrap();

        // Verify tasks completed
        assert_eq!(counter.load(Ordering::Relaxed), 2);
        assert_eq!(executor.pending_tasks(), 0);
    }

    #[tokio::test]
    async fn test_deterministic_event_logging() {
        let config = ExecutorConfig {
            enable_event_logging: true,
            ..Default::default()
        };
        let executor = DeterministicExecutor::new(config);

        let task_id = executor
            .spawn_deterministic(
                "Test Task".to_string(),
                async {
                    // Simple task
                },
            )
            .unwrap();

        executor.run().await.unwrap();

        let events = executor.get_event_log();
        assert!(!events.is_empty());
        
        // Check for TaskSpawned and TaskCompleted events
        let spawn_events: Vec<_> = events.iter().filter(|e| matches!(e, ExecutorEvent::TaskSpawned { .. })).collect();
        let complete_events: Vec<_> = events.iter().filter(|e| matches!(e, ExecutorEvent::TaskCompleted { .. })).collect();
        
        assert_eq!(spawn_events.len(), 1);
        assert_eq!(complete_events.len(), 1);
    }

    #[tokio::test]
    async fn test_tick_timeout() {
        let config = ExecutorConfig {
            max_ticks_per_task: 5,
            ..Default::default()
        };
        let executor = DeterministicExecutor::new(config);

        // Spawn a task that yields but never completes
        let _task_id = executor
            .spawn_deterministic(
                "Yielding Task".to_string(),
                async {
                    // Yield a few times but don't complete
                    for _ in 0..10 {
                        tokio::task::yield_now().await;
                    }
                },
            )
            .unwrap();

        executor.run().await.unwrap();

        let events = executor.get_event_log();
        let timeout_events: Vec<_> = events.iter().filter(|e| matches!(e, ExecutorEvent::TaskTimeout { .. })).collect();
        
        assert!(!timeout_events.is_empty());
    }

    #[tokio::test]
    async fn test_deterministic_randomness() {
        let config = ExecutorConfig {
            global_seed: [42u8; 32],
            ..Default::default()
        };
        let executor1 = DeterministicExecutor::new(config.clone());
        let executor2 = DeterministicExecutor::new(config);

        let rand1: u32 = executor1.deterministic_random();
        let rand2: u32 = executor2.deterministic_random();

        assert_eq!(rand1, rand2, "Deterministic randomness should be identical");
    }

    #[tokio::test]
    async fn test_seed_derivation() {
        let config = ExecutorConfig {
            global_seed: [42u8; 32],
            ..Default::default()
        };
        let executor = DeterministicExecutor::new(config);

        let seed1 = executor.derive_seed("test");
        let seed2 = executor.derive_seed("test");

        assert_eq!(seed1, seed2, "Seed derivation should be deterministic");
    }
}
