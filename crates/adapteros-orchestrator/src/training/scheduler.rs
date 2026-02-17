//! Train-when-idle scheduler with inference preemption.
//!
//! The `TrainingScheduler` coordinates training jobs with inference workloads,
//! pausing active training when inference requests arrive and resuming after
//! a configurable idle period. This ensures inference latency is not degraded
//! by concurrent training work competing for GPU/memory resources.
//!
//! ## How it works
//!
//! 1. Each training job receives a `pause_token: Arc<AtomicBool>` from the scheduler.
//! 2. When inference becomes active, `notify_inference_active()` sets all pause tokens.
//! 3. The training execution loop checks the pause token at epoch boundaries and
//!    sleeps until the token is cleared.
//! 4. After the last inference request completes, `notify_inference_idle()` starts
//!    an idle timer. When the timer expires without new inference, pause tokens
//!    are cleared and training resumes.
//!
//! ## Granularity
//!
//! Pause is checked at epoch boundaries in the orchestrator's execution layer.
//! The worker's batch-level cancel checks are not used for pausing (worker crate
//! is not modified). Epoch-level granularity is sufficient since inference
//! preemption targets resource contention, not sub-second latency.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, Notify};
use tracing::{debug, info};

/// Default idle timeout before resuming training after inference completes.
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 30;

/// Training scheduler that pauses training during active inference.
pub struct TrainingScheduler {
    /// Pause tokens keyed by job ID. When true, training should pause.
    pause_tokens: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Whether inference is currently active.
    inference_active: AtomicBool,
    /// Idle timeout before resuming training after inference goes quiet.
    idle_timeout: Duration,
    /// Notification channel for idle timer coordination.
    /// Notified when inference becomes active (to cancel pending resume).
    idle_cancel: Notify,
    /// Notification channel for resume events.
    /// Notified when training is unpaused so sleeping loops can wake.
    resume_notify: Notify,
    /// Counter tracking active inference requests for correct idle detection.
    active_inference_count: Mutex<u64>,
}

impl TrainingScheduler {
    /// Create a new scheduler with the default idle timeout.
    pub fn new() -> Self {
        Self::with_idle_timeout(Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS))
    }

    /// Create a new scheduler with a custom idle timeout.
    pub fn with_idle_timeout(idle_timeout: Duration) -> Self {
        Self {
            pause_tokens: Mutex::new(HashMap::new()),
            inference_active: AtomicBool::new(false),
            idle_timeout,
            idle_cancel: Notify::new(),
            resume_notify: Notify::new(),
            active_inference_count: Mutex::new(0),
        }
    }

    /// Register a training job and return its pause token.
    ///
    /// If inference is currently active, the token starts in the paused state
    /// so the job immediately pauses at its first check.
    pub async fn register_job(&self, job_id: &str) -> Arc<AtomicBool> {
        let paused = self.inference_active.load(Ordering::SeqCst);
        let token = Arc::new(AtomicBool::new(paused));

        let mut tokens = self.pause_tokens.lock().await;
        tokens.insert(job_id.to_string(), token.clone());

        if paused {
            info!(
                job_id = %job_id,
                "Training job registered while inference active — starting paused"
            );
        } else {
            debug!(job_id = %job_id, "Training job registered with scheduler");
        }

        token
    }

    /// Remove a training job's pause token (called when job completes/fails/cancels).
    pub async fn deregister_job(&self, job_id: &str) {
        let mut tokens = self.pause_tokens.lock().await;
        tokens.remove(job_id);
        debug!(job_id = %job_id, "Training job deregistered from scheduler");
    }

    /// Notify the scheduler that an inference request has started.
    ///
    /// Immediately pauses all active training jobs by setting their pause tokens.
    /// Cancels any pending idle-resume timer.
    pub async fn notify_inference_active(&self) {
        let mut count = self.active_inference_count.lock().await;
        *count += 1;

        if !self.inference_active.swap(true, Ordering::SeqCst) {
            // Transition from idle to active — pause all training
            let tokens = self.pause_tokens.lock().await;
            let job_count = tokens.len();
            for (job_id, token) in tokens.iter() {
                token.store(true, Ordering::SeqCst);
                debug!(job_id = %job_id, "Pausing training job for inference");
            }
            if job_count > 0 {
                info!(
                    paused_jobs = job_count,
                    "Inference active — paused all training jobs"
                );
            }
        }

        // Cancel any pending resume timer
        self.idle_cancel.notify_waiters();
    }

    /// Notify the scheduler that an inference request has completed.
    ///
    /// When the last active inference request completes, starts the idle timer.
    /// After the idle timeout elapses without new inference, training resumes.
    pub async fn notify_inference_idle(&self) {
        let should_start_timer = {
            let mut count = self.active_inference_count.lock().await;
            *count = count.saturating_sub(1);
            *count == 0
        };

        if !should_start_timer {
            return;
        }

        // Spawn idle timer — if it completes without cancellation, resume training
        let idle_timeout = self.idle_timeout;
        let inference_active = &self.inference_active;
        let pause_tokens = &self.pause_tokens;
        let idle_cancel = &self.idle_cancel;
        let resume_notify = &self.resume_notify;

        // We need to race the timeout against a cancellation signal.
        // Using select! to pick whichever completes first.
        tokio::select! {
            _ = tokio::time::sleep(idle_timeout) => {
                // Idle timeout elapsed — resume training
                inference_active.store(false, Ordering::SeqCst);
                let tokens = pause_tokens.lock().await;
                let job_count = tokens.len();
                for (job_id, token) in tokens.iter() {
                    token.store(false, Ordering::SeqCst);
                    debug!(job_id = %job_id, "Resuming training job after idle timeout");
                }
                if job_count > 0 {
                    info!(
                        resumed_jobs = job_count,
                        idle_secs = idle_timeout.as_secs(),
                        "Inference idle timeout elapsed — resumed all training jobs"
                    );
                }
                resume_notify.notify_waiters();
            }
            _ = idle_cancel.notified() => {
                // New inference arrived before timeout — stay paused
                debug!("Idle resume cancelled by new inference request");
            }
        }
    }

    /// Check if training is currently paused.
    pub fn is_training_paused(&self) -> bool {
        self.inference_active.load(Ordering::SeqCst)
    }

    /// Wait until the given pause token is cleared.
    ///
    /// Called by the training execution loop at epoch boundaries. If not paused,
    /// returns immediately. If paused, sleeps in a loop checking the token,
    /// waking on resume notifications to avoid spinning.
    ///
    /// Returns `true` if training was paused and has now resumed,
    /// `false` if training was never paused.
    pub async fn wait_if_paused(&self, pause_token: &AtomicBool, job_id: &str) -> bool {
        if !pause_token.load(Ordering::SeqCst) {
            return false;
        }

        info!(
            job_id = %job_id,
            "Training paused — waiting for inference to complete"
        );

        loop {
            // Wait for a resume notification or check periodically as a safety net
            tokio::select! {
                _ = self.resume_notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }

            if !pause_token.load(Ordering::SeqCst) {
                info!(job_id = %job_id, "Training resumed after pause");
                return true;
            }
        }
    }

    /// Get the number of currently registered training jobs.
    pub async fn active_job_count(&self) -> usize {
        let tokens = self.pause_tokens.lock().await;
        tokens.len()
    }
}

impl Default for TrainingScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_job_returns_unpaused_token() {
        let scheduler = TrainingScheduler::new();
        let token = scheduler.register_job("job-1").await;
        assert!(!token.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_inference_active_pauses_training() {
        let scheduler = TrainingScheduler::new();
        let token = scheduler.register_job("job-1").await;

        scheduler.notify_inference_active().await;

        assert!(token.load(Ordering::SeqCst));
        assert!(scheduler.is_training_paused());
    }

    #[tokio::test]
    async fn test_register_during_inference_starts_paused() {
        let scheduler = TrainingScheduler::new();
        scheduler.notify_inference_active().await;

        let token = scheduler.register_job("job-1").await;
        assert!(token.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_deregister_removes_token() {
        let scheduler = TrainingScheduler::new();
        let _token = scheduler.register_job("job-1").await;
        assert_eq!(scheduler.active_job_count().await, 1);

        scheduler.deregister_job("job-1").await;
        assert_eq!(scheduler.active_job_count().await, 0);
    }

    #[tokio::test]
    async fn test_idle_timeout_resumes_training() {
        let scheduler = Arc::new(TrainingScheduler::with_idle_timeout(Duration::from_millis(
            50,
        )));
        let token = scheduler.register_job("job-1").await;

        scheduler.notify_inference_active().await;
        assert!(token.load(Ordering::SeqCst));

        // Simulate inference completing
        scheduler.notify_inference_idle().await;

        // Token should be cleared after the idle timeout
        assert!(!token.load(Ordering::SeqCst));
        assert!(!scheduler.is_training_paused());
    }

    #[tokio::test]
    async fn test_wait_if_paused_returns_false_when_not_paused() {
        let scheduler = TrainingScheduler::new();
        let token = scheduler.register_job("job-1").await;
        let was_paused = scheduler.wait_if_paused(&token, "job-1").await;
        assert!(!was_paused);
    }

    #[tokio::test]
    async fn test_multiple_inference_requests_need_all_idle() {
        let scheduler = Arc::new(TrainingScheduler::with_idle_timeout(Duration::from_millis(
            50,
        )));
        let token = scheduler.register_job("job-1").await;

        // Two inference requests start
        scheduler.notify_inference_active().await;
        scheduler.notify_inference_active().await;
        assert!(token.load(Ordering::SeqCst));

        // First one completes — still paused because second is active
        let scheduler_clone = scheduler.clone();
        let handle = tokio::spawn(async move {
            scheduler_clone.notify_inference_idle().await;
        });
        // Give the idle handler time to run
        tokio::time::sleep(Duration::from_millis(10)).await;
        // Count is still > 0, so timer should not have started meaningfully
        // The first idle call sees count go to 1, not 0, so it returns immediately
        handle.await.unwrap();

        // Second completes — now the idle timer starts
        scheduler.notify_inference_idle().await;
        assert!(!token.load(Ordering::SeqCst));
    }
}
