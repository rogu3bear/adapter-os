//! Deadlock detection and recovery mechanisms
//!
//! Implements deadlock detection and recovery to prevent runaway processes.
//! Aligns with Determinism Ruleset #2 and Performance Ruleset #11 from policy enforcement.

use adapteros_core::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{error, warn};

/// Deadlock detection configuration
#[derive(Debug, Clone)]
pub struct DeadlockConfig {
    pub check_interval: Duration,
    pub max_wait_time: Duration,
    pub max_lock_depth: usize,
    pub recovery_timeout: Duration,
}

impl Default for DeadlockConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(5),
            max_wait_time: Duration::from_secs(30),
            max_lock_depth: 10,
            recovery_timeout: Duration::from_secs(10),
        }
    }
}

/// Lock information for deadlock detection
#[derive(Debug, Clone)]
struct LockInfo {
    thread_id: u64,
    lock_id: String,
    acquired_at: Instant,
    /// Stack trace for debugging (reserved for detailed deadlock analysis)
    _stack_trace: String,
}

/// Deadlock detector
pub struct DeadlockDetector {
    config: DeadlockConfig,
    locks: Arc<Mutex<HashMap<String, LockInfo>>>,
    thread_locks: Arc<Mutex<HashMap<u64, Vec<String>>>>,
    deadlock_count: Arc<Mutex<usize>>,
    recovery_in_progress: Arc<Mutex<bool>>,
}

impl DeadlockDetector {
    pub fn new(config: DeadlockConfig) -> Self {
        Self {
            config,
            locks: Arc::new(Mutex::new(HashMap::new())),
            thread_locks: Arc::new(Mutex::new(HashMap::new())),
            deadlock_count: Arc::new(Mutex::new(0)),
            recovery_in_progress: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn start_monitoring(&self) -> Result<()> {
        let mut interval = interval(self.config.check_interval);

        loop {
            interval.tick().await;

            if let Err(e) = self.check_for_deadlocks().await {
                error!("Deadlock detection failed: {}", e);
                // Continue monitoring even if detection fails
            }
        }
    }

    async fn check_for_deadlocks(&self) -> Result<()> {
        // Collect lock info in first scope
        let lock_infos: Vec<_> = {
            let locks = self.locks.lock().await;
            locks.values().cloned().collect()
        };
        // First lock released

        // Collect thread info in second scope
        let thread_info: HashMap<_, _> = {
            let thread_locks = self.thread_locks.lock().await;
            thread_locks.clone()
        };
        // Second lock released

        let now = Instant::now();

        // Now process without holding any locks
        for lock_info in lock_infos {
            if now.duration_since(lock_info.acquired_at) > self.config.max_wait_time {
                warn!(
                    "Lock {} held for {} seconds by thread {}",
                    lock_info.lock_id,
                    now.duration_since(lock_info.acquired_at).as_secs(),
                    lock_info.thread_id
                );

                // Check if this might be a deadlock
                if self.is_potential_deadlock(&lock_info, &thread_info) {
                    error!("Potential deadlock detected on lock {}", lock_info.lock_id);
                    self.trigger_deadlock_recovery(&lock_info.lock_id).await?;
                }
            }
        }

        Ok(())
    }

    fn is_potential_deadlock(
        &self,
        lock_info: &LockInfo,
        thread_locks: &HashMap<u64, Vec<String>>,
    ) -> bool {
        // Simple deadlock detection: check if thread is waiting for locks held by other threads
        if let Some(thread_locks) = thread_locks.get(&lock_info.thread_id) {
            // Check if any of the locks this thread is waiting for are held by other threads
            for waiting_lock in thread_locks {
                if waiting_lock != &lock_info.lock_id {
                    // This is a simplified check - in practice, you'd need more sophisticated cycle detection
                    return true;
                }
            }
        }
        false
    }

    async fn trigger_deadlock_recovery(&self, lock_id: &str) -> Result<()> {
        // Check if recovery is already in progress
        {
            let mut recovery = self.recovery_in_progress.lock().await;
            if *recovery {
                warn!("Deadlock recovery already in progress, skipping");
                return Ok(());
            }
            *recovery = true;
        }

        error!(
            lock_id = %lock_id,
            "Deadlock detected - automatic recovery not implemented"
        );

        // Increment deadlock count for metrics
        {
            let mut count = self.deadlock_count.lock().await;
            *count += 1;
        }

        // Mark recovery as complete (detection succeeded, but recovery failed)
        {
            let mut recovery = self.recovery_in_progress.lock().await;
            *recovery = false;
        }

        // Real deadlock recovery would require:
        // 1. Force release the problematic lock (unsafe - requires FFI or OS APIs)
        // 2. Restart the affected component (requires process management)
        // 3. Validate system state after recovery
        // 4. Log incident for analysis with full stack traces
        //
        // Without these capabilities, deadlocks must be prevented via:
        // - Lock ordering protocols
        // - Timeout-based lock acquisition
        // - Avoiding nested locks

        Err(adapteros_core::AosError::Kernel(format!(
            "Deadlock detected on lock '{}' but automatic recovery is not implemented. \
             Manual intervention required - restart the affected worker process.",
            lock_id
        )))
    }

    pub async fn record_lock_acquisition(&self, lock_id: String, thread_id: u64) {
        let lock_info = LockInfo {
            thread_id,
            lock_id: lock_id.clone(),
            acquired_at: Instant::now(),
            _stack_trace: "".to_string(), // Would capture actual stack trace
        };

        self.locks.lock().await.insert(lock_id.clone(), lock_info);
        self.thread_locks
            .lock()
            .await
            .entry(thread_id)
            .or_insert_with(Vec::new)
            .push(lock_id);
    }

    pub async fn record_lock_release(&self, lock_id: &str, thread_id: u64) {
        self.locks.lock().await.remove(lock_id);
        self.thread_locks
            .lock()
            .await
            .entry(thread_id)
            .and_modify(|locks| locks.retain(|id| id != lock_id));
    }

    pub async fn get_deadlock_count(&self) -> usize {
        *self.deadlock_count.lock().await
    }

    pub async fn is_recovery_in_progress(&self) -> bool {
        *self.recovery_in_progress.lock().await
    }
}

/// Deadlock event for telemetry
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeadlockEvent {
    pub lock_id: String,
    pub thread_id: u64,
    pub wait_time_secs: u64,
    pub recovery_triggered: bool,
    pub total_deadlocks: usize,
    pub timestamp: u64,
}

impl DeadlockEvent {
    pub fn new(
        lock_id: String,
        thread_id: u64,
        wait_time: Duration,
        recovery_triggered: bool,
        total_deadlocks: usize,
    ) -> Self {
        Self {
            lock_id,
            thread_id,
            wait_time_secs: wait_time.as_secs(),
            recovery_triggered,
            total_deadlocks,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_secs(),
        }
    }
}

/// Simplified deadlock-aware lock (without lifetime issues)
pub struct DeadlockAwareLock<T> {
    inner: Arc<Mutex<T>>,
    lock_id: String,
    detector: Arc<DeadlockDetector>,
}

impl<T> DeadlockAwareLock<T> {
    pub fn new(inner: T, lock_id: String, detector: Arc<DeadlockDetector>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
            lock_id,
            detector,
        }
    }

    pub async fn lock(&self) -> Result<tokio::sync::MutexGuard<'_, T>> {
        let thread_id = get_thread_id();
        self.detector
            .record_lock_acquisition(self.lock_id.clone(), thread_id)
            .await;

        // In a real implementation, would check for deadlocks here
        Ok(self.inner.lock().await)
    }
}

/// Get current thread ID (simplified implementation)
fn get_thread_id() -> u64 {
    // In a real implementation, would use platform-specific thread ID
    // For now, use a hash of the thread ID
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    std::thread::current().id().hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_deadlock_detector_creation() {
        let config = DeadlockConfig::default();
        let detector = DeadlockDetector::new(config);

        assert_eq!(detector.get_deadlock_count().await, 0);
        assert!(!detector.is_recovery_in_progress().await);
    }

    #[tokio::test]
    async fn test_lock_tracking() {
        let config = DeadlockConfig::default();
        let detector = DeadlockDetector::new(config);

        detector
            .record_lock_acquisition("test_lock".to_string(), 1)
            .await;
        detector.record_lock_release("test_lock", 1).await;

        // Should not panic
        assert_eq!(detector.get_deadlock_count().await, 0);
    }

    #[tokio::test]
    async fn test_deadlock_aware_lock() {
        let config = DeadlockConfig::default();
        let detector = Arc::new(DeadlockDetector::new(config));

        let lock = DeadlockAwareLock::new(42, "test_lock".to_string(), detector.clone());
        let guard = lock
            .lock()
            .await
            .expect("Test lock acquisition should succeed");

        assert_eq!(*guard, 42);
        // Guard will be dropped here, releasing the lock
    }

    #[test]
    fn test_deadlock_event_creation() {
        let event =
            DeadlockEvent::new("test_lock".to_string(), 1, Duration::from_secs(30), true, 1);

        assert_eq!(event.lock_id, "test_lock");
        assert_eq!(event.thread_id, 1);
        assert_eq!(event.wait_time_secs, 30);
        assert!(event.recovery_triggered);
        assert_eq!(event.total_deadlocks, 1);
        assert!(event.timestamp > 0);
    }
}
