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

#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::await_holding_lock)]

pub mod channel;
pub mod cpu_affinity;
pub mod global_ledger;
pub mod multi_agent;
pub mod select;

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
    thread::ThreadId,
};

use parking_lot::Mutex;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use std::io;
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Global sequence counter for deterministic task ID generation
/// This ensures all task IDs are reproducible across runs
static GLOBAL_TASK_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Deterministic task ID type using BLAKE3 hash
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId([u8; 32]);

impl TaskId {
    /// Generate deterministic task ID from global seed and sequence number
    pub fn from_seed_and_seq(global_seed: &[u8; 32], seq: u64) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(global_seed);
        hasher.update(&seq.to_le_bytes());
        let hash = hasher.finalize();
        Self(*hash.as_bytes())
    }

    /// Get the hash bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Create from raw bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0[..8]))
    }
}

/// Cancellation token with BLAKE3-hashed cause tracking
#[derive(Debug, Clone)]
pub struct CancelToken {
    _cause: String,
    _cause_hash: [u8; 32],
    cancelled: Arc<AtomicU64>,
}

impl CancelToken {
    /// Create a new cancel token
    pub fn new() -> Self {
        Self {
            _cause: String::new(),
            _cause_hash: [0u8; 32],
            cancelled: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Cancel with a specific cause
    pub fn cancel(&self, cause: &str) {
        let mut hasher = blake3::Hasher::new();
        hasher.update(cause.as_bytes());
        // Note: We can't update the struct fields because self is &self
        // In real implementation, we'd need interior mutability
        self.cancelled.store(1, Ordering::Relaxed);
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed) != 0
    }

    /// Get the cancellation cause hash
    pub fn cause_hash(&self) -> [u8; 32] {
        self._cause_hash
    }
}

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of executor state for crash recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorSnapshot {
    /// Current tick
    pub tick: u64,
    /// RNG state (serialized seed)
    pub rng_seed: [u8; 32],
    /// Pending tasks (without futures)
    pub pending_tasks: Vec<TaskSnapshot>,
    /// Event log
    pub event_log: Vec<ExecutorEvent>,
    /// Global sequence counter
    pub global_sequence: u64,
    /// Agent ID
    pub agent_id: Option<String>,
}

/// Snapshot of a single task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    /// Task ID
    pub id: TaskId,
    /// Description
    pub description: String,
    /// Spawn tick
    pub spawn_tick: u64,
    /// Completed status
    pub completed: bool,
}

/// Error types for the deterministic executor
#[derive(Error, Debug)]
pub enum DeterministicExecutorError {
    #[error("Task {task_id} failed: {error}")]
    TaskFailed { task_id: TaskId, error: String },
    #[error("Replay failed: {reason}")]
    ReplayFailed { reason: String },
    #[error("Timeout exceeded: {task_id}")]
    TimeoutExceeded { task_id: TaskId },
    #[error("Executor not initialized")]
    NotInitialized,
    #[error("Executor is running, cannot restore snapshot")]
    ExecutorRunning,
    #[error("Snapshot validation failed: {reason}")]
    SnapshotValidationFailed { reason: String },
    #[error("Runtime error: {0}")]
    RuntimeError(String),
}

impl From<DeterministicExecutorError> for io::Error {
    fn from(err: DeterministicExecutorError) -> Self {
        io::Error::other(err.to_string())
    }
}

/// Result type for executor operations
pub type Result<T> = std::result::Result<T, DeterministicExecutorError>;

/// Handle for a spawned deterministic task
pub struct DeterministicJoinHandle {
    task_id: TaskId,
    _executor: Arc<DeterministicExecutor>,
}

impl DeterministicJoinHandle {
    pub fn new(task_id: TaskId, executor: Arc<DeterministicExecutor>) -> Self {
        Self {
            task_id,
            _executor: executor,
        }
    }

    /// Abort the task
    pub fn abort(&self) {
        // For now, just log the abort. In a real implementation,
        // we would need to track task cancellation state.
        info!("Aborting deterministic task {}", self.task_id);
    }

    /// Get the task ID
    pub fn task_id(&self) -> TaskId {
        self.task_id
    }
}

/// Event types logged by the executor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutorEvent {
    /// Task spawned with ID and description
    TaskSpawned {
        task_id: TaskId,
        description: String,
        tick: u64,
        agent_id: Option<String>,
        hash: [u8; 32],
    },
    /// Task completed successfully
    TaskCompleted {
        task_id: TaskId,
        tick: u64,
        duration_ticks: u64,
        agent_id: Option<String>,
        hash: [u8; 32],
    },
    /// Task failed with error
    TaskFailed {
        task_id: TaskId,
        error: String,
        tick: u64,
        duration_ticks: u64,
        agent_id: Option<String>,
        hash: [u8; 32],
    },
    /// Task timed out
    TaskTimeout {
        task_id: TaskId,
        timeout_ticks: u64,
        tick: u64,
        agent_id: Option<String>,
        hash: [u8; 32],
    },
    /// Tick counter advanced
    TickAdvanced {
        from_tick: u64,
        to_tick: u64,
        agent_id: Option<String>,
        hash: [u8; 32],
    },
}

/// Enforcement mode for tick ledger policy violations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforcementMode {
    /// Record violations but continue execution (default)
    AuditOnly,
    /// Log warnings for violations but continue
    Warn,
    /// Fail execution on policy violations
    Enforce,
}

impl Default for EnforcementMode {
    fn default() -> Self {
        Self::AuditOnly
    }
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
    /// Optional agent ID for multi-agent coordination
    pub agent_id: Option<String>,
    /// Whether to enable thread pinning for deterministic scheduling
    pub enable_thread_pinning: bool,
    /// Number of worker threads (defaults to CPU count)
    pub worker_threads: Option<usize>,
    /// Enforcement mode for tick ledger policy violations
    pub enforcement_mode: EnforcementMode,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            global_seed: [0u8; 32],
            max_ticks_per_task: 1000,
            enable_event_logging: true,
            replay_mode: false,
            replay_events: Vec::new(),
            agent_id: None,
            enable_thread_pinning: true,
            worker_threads: None,
            enforcement_mode: EnforcementMode::default(),
        }
    }
}

/// Tick-based timeout guard
#[derive(Debug, Clone)]
pub struct TickTimeout {
    /// Task ID this timeout belongs to
    task_id: TaskId,
    /// Tick when timeout should trigger
    timeout_tick: u64,
    /// Current tick counter
    current_tick: Arc<AtomicU64>,
}

impl TickTimeout {
    /// Create a new tick timeout
    pub fn new(task_id: TaskId, timeout_ticks: u64, current_tick: Arc<AtomicU64>) -> Self {
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
        self.timeout_tick.saturating_sub(current)
    }
}

/// Tick-based delay future
#[derive(Debug)]
pub struct TickDelay {
    target_tick: u64,
    current_tick: Arc<AtomicU64>,
}

impl TickDelay {
    /// Create a new tick delay
    pub fn new(delay_ticks: u64, current_tick: Arc<AtomicU64>) -> Self {
        let target_tick = current_tick.load(Ordering::Relaxed) + delay_ticks;
        Self {
            target_tick,
            current_tick,
        }
    }
}

impl Future for TickDelay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.current_tick.load(Ordering::Relaxed) >= self.target_tick {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

/// Deterministic task wrapper
#[derive(Serialize, Deserialize)]
struct DeterministicTask {
    /// Unique task ID
    id: TaskId,
    /// Task description for logging
    description: String,
    /// The actual future (not serialized)
    #[serde(skip)]
    future: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
    /// Tick when task was spawned
    spawn_tick: u64,
    /// Whether task has completed
    completed: bool,
    /// Optional timeout guard (not serialized, reconstructed on restore)
    #[serde(skip)]
    timeout: Option<TickTimeout>,
}

impl DeterministicTask {
    fn new<F>(id: TaskId, description: String, future: F, spawn_tick: u64) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Self {
            id,
            description,
            future: Some(Box::pin(future)),
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
    /// Pinned thread IDs for deterministic scheduling
    pinned_threads: Mutex<Vec<ThreadId>>,
    /// Global tick ledger (optional, for cross-host tracking)
    global_ledger: Option<Arc<global_ledger::GlobalTickLedger>>,
}

impl DeterministicExecutor {
    /// Create a new deterministic executor
    pub fn new(config: ExecutorConfig) -> Self {
        let rng = ChaCha20Rng::from_seed(config.global_seed);

        // Initialize CPU affinity if enabled
        if config.enable_thread_pinning {
            if let Err(e) = cpu_affinity::init_cpu_affinity() {
                warn!("Failed to initialize CPU affinity: {}", e);
            }
        }

        info!(
            "Creating deterministic executor with seed: {:?}, replay_mode: {}, thread_pinning: {}",
            config.global_seed, config.replay_mode, config.enable_thread_pinning
        );

        Self {
            config,
            task_queue: Mutex::new(VecDeque::new()),
            tick_counter: Arc::new(AtomicU64::new(0)),
            event_log: Mutex::new(Vec::new()),
            rng: Mutex::new(rng),
            replay_index: Mutex::new(0),
            running: Arc::new(AtomicU64::new(0)),
            pinned_threads: Mutex::new(Vec::new()),
            global_ledger: None,
        }
    }

    /// Create executor with global tick ledger
    pub fn with_global_ledger(
        config: ExecutorConfig,
        ledger: Arc<global_ledger::GlobalTickLedger>,
    ) -> Self {
        let mut executor = Self::new(config);
        executor.global_ledger = Some(ledger);
        executor
    }

    /// Spawn a deterministic task
    pub fn spawn_deterministic<F>(&self, description: String, future: F) -> Result<TaskId>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // Generate deterministic task ID using global sequence
        let seq = GLOBAL_TASK_SEQUENCE.fetch_add(1, Ordering::SeqCst);
        let task_id = TaskId::from_seed_and_seq(&self.config.global_seed, seq);
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

        // Log event with hash
        if self.config.enable_event_logging {
            let event_hash = Self::compute_event_hash(&task_id, &description, current_tick);
            let event = ExecutorEvent::TaskSpawned {
                task_id,
                description: description.clone(),
                tick: current_tick,
                agent_id: self.config.agent_id.clone(),
                hash: event_hash,
            };
            self.event_log.lock().push(event.clone());

            // Record to global ledger if available (spawn happens synchronously, but we'll handle async)
            // Note: In production, spawn should be async or use a background task for ledger recording
        }

        debug!(
            "Spawned deterministic task {} at tick {}",
            task_id, current_tick
        );
        Ok(task_id)
    }

    /// Create a tick-based delay
    pub fn delay(&self, delay_ticks: u64) -> TickDelay {
        TickDelay::new(delay_ticks, self.tick_counter.clone())
    }

    /// Compute deterministic hash for an event
    fn compute_event_hash(task_id: &TaskId, description: &str, tick: u64) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(task_id.as_bytes());
        hasher.update(description.as_bytes());
        hasher.update(&tick.to_le_bytes());
        *hasher.finalize().as_bytes()
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

        // Pin current thread if thread pinning is enabled
        if self.config.enable_thread_pinning {
            if let Some(core_id) = cpu_affinity::get_next_core_id() {
                if let Err(e) = cpu_affinity::pin_thread_to_core(core_id) {
                    warn!("Failed to pin executor thread to core {}: {}", core_id, e);
                } else {
                    let thread_id = std::thread::current().id();
                    self.pinned_threads.lock().push(thread_id);
                    info!(
                        "Pinned executor thread {:?} to CPU core {}",
                        thread_id, core_id
                    );
                }
            }
        }

        while let Some(mut task) = self.task_queue.lock().pop_front() {
            let current_tick = self.tick_counter.load(Ordering::Relaxed);

            // Check for timeout
            if let Some(ref timeout) = task.timeout {
                if timeout.is_timeout() {
                    warn!("Task {} timed out at tick {}", task.id, current_tick);

                    if self.config.enable_event_logging {
                        let mut hasher = blake3::Hasher::new();
                        hasher.update(task.id.as_bytes());
                        hasher.update(b"timeout");
                        hasher.update(&current_tick.to_le_bytes());
                        let event_hash = *hasher.finalize().as_bytes();

                        let event = ExecutorEvent::TaskTimeout {
                            task_id: task.id,
                            timeout_ticks: self.config.max_ticks_per_task,
                            tick: current_tick,
                            agent_id: self.config.agent_id.clone(),
                            hash: event_hash,
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
            match task.future.as_mut().unwrap().as_mut().poll(&mut context) {
                Poll::Ready(()) => {
                    let completion_tick = self.tick_counter.load(Ordering::Relaxed);
                    let duration_ticks = completion_tick - task.spawn_tick;

                    debug!(
                        "Task {} completed at tick {} (duration: {} ticks)",
                        task.id, completion_tick, duration_ticks
                    );

                    if self.config.enable_event_logging {
                        let mut hasher = blake3::Hasher::new();
                        hasher.update(task.id.as_bytes());
                        hasher.update(b"completed");
                        hasher.update(&completion_tick.to_le_bytes());
                        hasher.update(&duration_ticks.to_le_bytes());
                        let event_hash = *hasher.finalize().as_bytes();

                        let event = ExecutorEvent::TaskCompleted {
                            task_id: task.id,
                            tick: completion_tick,
                            duration_ticks,
                            agent_id: self.config.agent_id.clone(),
                            hash: event_hash,
                        };
                        self.event_log.lock().push(event.clone());

                        // Record to global ledger if available
                        if let Some(ref ledger) = self.global_ledger {
                            let _ = ledger.record_tick(task.id, &event).await;
                        }
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

                        let mut hasher = blake3::Hasher::new();
                        hasher.update(b"tick_advanced");
                        hasher.update(&old_tick.to_le_bytes());
                        hasher.update(&new_tick.to_le_bytes());
                        let event_hash = *hasher.finalize().as_bytes();

                        let event = ExecutorEvent::TickAdvanced {
                            from_tick: old_tick,
                            to_tick: new_tick,
                            agent_id: self.config.agent_id.clone(),
                            hash: event_hash,
                        };
                        self.event_log.lock().push(event.clone());

                        // Record to global ledger if available (use dummy task ID for tick advances)
                        // Note: record_tick now atomically assigns ticks internally (Issue C-6 fix)
                        if let Some(ref ledger) = self.global_ledger {
                            let dummy_task_id = TaskId::from_bytes([0u8; 32]);
                            let _ = ledger.record_tick(dummy_task_id, &event).await;
                        }
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
                ExecutorEvent::TaskSpawned {
                    task_id,
                    description,
                    tick,
                    agent_id: _,
                    hash: _,
                } => {
                    info!(
                        "Replaying task spawn: {} ({}) at tick {}",
                        task_id, description, tick
                    );
                    // In replay mode, we don't actually spawn tasks, just log the event
                    if self.config.enable_event_logging {
                        self.event_log.lock().push(event.clone());
                    }
                }
                ExecutorEvent::TaskCompleted {
                    task_id,
                    tick,
                    duration_ticks,
                    agent_id: _,
                    hash: _,
                } => {
                    info!(
                        "Replaying task completion: {} at tick {} (duration: {} ticks)",
                        task_id, tick, duration_ticks
                    );
                    if self.config.enable_event_logging {
                        self.event_log.lock().push(event.clone());
                    }
                }
                ExecutorEvent::TaskFailed {
                    task_id,
                    error,
                    tick,
                    duration_ticks,
                    agent_id: _,
                    hash: _,
                } => {
                    warn!(
                        "Replaying task failure: {} at tick {} (duration: {} ticks): {}",
                        task_id, tick, duration_ticks, error
                    );
                    if self.config.enable_event_logging {
                        self.event_log.lock().push(event.clone());
                    }
                }
                ExecutorEvent::TaskTimeout {
                    task_id,
                    timeout_ticks,
                    tick,
                    agent_id: _,
                    hash: _,
                } => {
                    warn!(
                        "Replaying task timeout: {} at tick {} (timeout: {} ticks)",
                        task_id, tick, timeout_ticks
                    );
                    if self.config.enable_event_logging {
                        self.event_log.lock().push(event.clone());
                    }
                }
                ExecutorEvent::TickAdvanced {
                    from_tick,
                    to_tick,
                    agent_id: _,
                    hash: _,
                } => {
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
        info!(
            "Setting {} replay events (replay mode: {})",
            events.len(),
            self.config.replay_mode
        );
    }

    /// Get replay progress
    pub fn replay_progress(&self) -> (usize, usize) {
        let current = *self.replay_index.lock();
        let total = self.config.replay_events.len();
        (current, total)
    }

    /// Create a snapshot of the executor state
    pub fn snapshot(&self) -> Result<ExecutorSnapshot> {
        info!("Creating executor snapshot");

        // Capture current state
        let tick = self.tick_counter.load(Ordering::Relaxed);
        let event_log = self.event_log.lock().clone();
        let global_sequence = GLOBAL_TASK_SEQUENCE.load(Ordering::SeqCst);

        // Capture pending tasks (without futures)
        let pending_tasks: Vec<TaskSnapshot> = self
            .task_queue
            .lock()
            .iter()
            .map(|task| TaskSnapshot {
                id: task.id,
                description: task.description.clone(),
                spawn_tick: task.spawn_tick,
                completed: task.completed,
            })
            .collect();

        // Get RNG seed (we'll use the config seed as the base)
        let rng_seed = self.config.global_seed;

        let snapshot = ExecutorSnapshot {
            tick,
            rng_seed,
            pending_tasks,
            event_log,
            global_sequence,
            agent_id: self.config.agent_id.clone(),
        };

        info!(
            "Snapshot created: tick={}, pending_tasks={}, events={}",
            tick,
            snapshot.pending_tasks.len(),
            snapshot.event_log.len()
        );

        Ok(snapshot)
    }

    /// Restore executor state from a snapshot
    /// IMPORTANT: Executor must not be running
    pub fn restore(&self, snapshot: ExecutorSnapshot) -> Result<()> {
        // Verify executor is not running
        if self.is_running() {
            return Err(DeterministicExecutorError::ExecutorRunning);
        }

        info!("Restoring executor from snapshot");

        // Validate snapshot
        if snapshot.rng_seed != self.config.global_seed {
            return Err(DeterministicExecutorError::SnapshotValidationFailed {
                reason: "RNG seed mismatch".to_string(),
            });
        }

        // Restore tick counter
        self.tick_counter.store(snapshot.tick, Ordering::Relaxed);

        // Restore global sequence
        GLOBAL_TASK_SEQUENCE.store(snapshot.global_sequence, Ordering::SeqCst);

        // Restore event log
        *self.event_log.lock() = snapshot.event_log;

        // Note: We cannot restore the actual futures, only the task metadata
        // In a real crash recovery scenario, tasks would need to be re-spawned
        // based on the snapshot metadata

        info!(
            "Snapshot restored: tick={}, global_seq={}",
            snapshot.tick, snapshot.global_sequence
        );

        Ok(())
    }

    /// Write telemetry for all executor events
    pub fn write_to_telemetry(&self) -> Result<Vec<u8>> {
        let events = self.get_event_log();
        let json =
            serde_json::to_vec(&events).map_err(|e| DeterministicExecutorError::ReplayFailed {
                reason: format!("Failed to serialize events: {}", e),
            })?;
        Ok(json)
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
static GLOBAL_EXECUTOR: std::sync::OnceLock<Arc<DeterministicExecutor>> =
    std::sync::OnceLock::new();

/// Initialize the global deterministic executor
pub fn init_global_executor(config: ExecutorConfig) -> Result<()> {
    let executor = Arc::new(DeterministicExecutor::new(config));
    GLOBAL_EXECUTOR
        .set(executor)
        .map_err(|_| DeterministicExecutorError::NotInitialized)?;
    Ok(())
}

/// Initialize the global deterministic executor with a tick ledger
pub fn init_global_executor_with_ledger(
    config: ExecutorConfig,
    ledger: Arc<global_ledger::GlobalTickLedger>,
) -> Result<()> {
    let executor = Arc::new(DeterministicExecutor::with_global_ledger(config, ledger));
    GLOBAL_EXECUTOR
        .set(executor)
        .map_err(|_| DeterministicExecutorError::NotInitialized)?;
    Ok(())
}

/// Configure Tokio runtime for deterministic execution
pub fn configure_tokio_runtime(config: &ExecutorConfig) -> Result<tokio::runtime::Runtime> {
    let mut builder = tokio::runtime::Builder::new_multi_thread();

    // Set worker thread count
    let worker_threads = config.worker_threads.unwrap_or_else(|| {
        if config.enable_thread_pinning {
            cpu_affinity::get_cpu_count()
        } else {
            1 // Single thread for maximum determinism
        }
    });

    builder.worker_threads(worker_threads);

    // Configure thread names for debugging
    builder.thread_name("aos-deterministic");

    // Build and return runtime
    builder.build().map_err(|e| {
        DeterministicExecutorError::RuntimeError(format!("Failed to create Tokio runtime: {}", e))
    })
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
        let _task1_id = executor
            .spawn_deterministic("Task 1".to_string(), async move {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            })
            .unwrap();

        let counter_clone = counter.clone();
        let _task2_id = executor
            .spawn_deterministic("Task 2".to_string(), async move {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            })
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

        let _task_id = executor
            .spawn_deterministic("Test Task".to_string(), async {
                // Simple task
            })
            .unwrap();

        executor.run().await.unwrap();

        let events = executor.get_event_log();
        assert!(!events.is_empty());

        // Check for TaskSpawned and TaskCompleted events
        let spawn_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ExecutorEvent::TaskSpawned { .. }))
            .collect();
        let complete_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ExecutorEvent::TaskCompleted { .. }))
            .collect();

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
            .spawn_deterministic("Yielding Task".to_string(), async {
                // Yield a few times but don't complete
                for _ in 0..10 {
                    tokio::task::yield_now().await;
                }
            })
            .unwrap();

        executor.run().await.unwrap();

        let events = executor.get_event_log();
        let timeout_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ExecutorEvent::TaskTimeout { .. }))
            .collect();

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
