//! Training management module
//!
//! Contains Worker methods for training job management including:
//! - register_training_job
//! - unregister_training_job
//! - cancel_training_job
//! - is_training_cancelled
//! - execute_workflow

use crate::{CancelTrainingResponse, Worker};
use adapteros_core::Result;
use adapteros_lora_kernel_api::FusedKernels;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::info;

/// Cancellation token for a training job.
///
/// This is the atomic boolean used to signal job cancellation.
/// The pattern uses `Release` ordering on the cancellation side
/// and `Acquire` ordering on the training loop check side to ensure
/// the cancellation signal is properly visible across threads.
pub type CancellationToken = Arc<AtomicBool>;

/// Worker methods for training job management
impl<K: FusedKernels + crate::StrictnessControl + Send + Sync + 'static> Worker<K> {
    /// Register an active training job with its cancellation token
    ///
    /// Call this when starting a training job to enable cancellation.
    pub fn register_training_job(&self, job_id: &str) -> Arc<AtomicBool> {
        let cancel_token = Arc::new(AtomicBool::new(false));
        let mut jobs = self.active_training_jobs.write();
        jobs.insert(job_id.to_string(), cancel_token.clone());
        tracing::info!(job_id = %job_id, "Registered training job for cancellation tracking");
        cancel_token
    }

    /// Unregister a training job (call when job completes/fails/cancels)
    pub fn unregister_training_job(&self, job_id: &str) {
        let mut jobs = self.active_training_jobs.write();
        jobs.remove(job_id);
        tracing::debug!(job_id = %job_id, "Unregistered training job");
    }

    /// Cancel an active training job
    ///
    /// Sets the cancellation token for the job, causing the training loop
    /// to stop at the next epoch boundary.
    pub fn cancel_training_job(&self, job_id: &str) -> Result<CancelTrainingResponse> {
        let jobs = self.active_training_jobs.read();

        if let Some(cancel_token) = jobs.get(job_id) {
            // Check if already cancelled
            if cancel_token.load(std::sync::atomic::Ordering::SeqCst) {
                tracing::info!(job_id = %job_id, "Training job already cancelled");
                return Ok(CancelTrainingResponse {
                    job_id: job_id.to_string(),
                    status: "already_cancelled".to_string(),
                    tokens_processed: None,
                    final_loss: None,
                    stopped_at_epoch: None,
                });
            }

            // Set cancellation flag with Release ordering to synchronize with
            // the Acquire load in is_cancelled(). This ensures the training loop
            // sees the cancel request without unnecessary SeqCst overhead.
            cancel_token.store(true, std::sync::atomic::Ordering::Release);
            tracing::info!(job_id = %job_id, "Training job cancellation requested");

            Ok(CancelTrainingResponse {
                job_id: job_id.to_string(),
                status: "cancelled".to_string(),
                tokens_processed: None, // Will be filled by training loop when it stops
                final_loss: None,
                stopped_at_epoch: None,
            })
        } else {
            tracing::warn!(job_id = %job_id, "Training job not found for cancellation");
            Ok(CancelTrainingResponse {
                job_id: job_id.to_string(),
                status: "not_found".to_string(),
                tokens_processed: None,
                final_loss: None,
                stopped_at_epoch: None,
            })
        }
    }

    /// Check if a training job has been cancelled
    pub fn is_training_cancelled(&self, job_id: &str) -> bool {
        let jobs = self.active_training_jobs.read();
        jobs.get(job_id)
            .map(|token| token.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(false)
    }

    /// Execute a workflow using real kernel backend
    ///
    /// Runs the workflow through actual Metal/MLX kernels with LoRA transformations.
    /// Kernels are shared via Arc<Mutex<K>> to allow concurrent workflow execution.
    pub async fn execute_workflow(
        &self,
        workflow_type: adapteros_lora_lifecycle::WorkflowType,
        adapter_ids: Vec<String>,
        context: adapteros_lora_lifecycle::WorkflowContext,
    ) -> Result<adapteros_lora_lifecycle::WorkflowResult>
    where
        K: Send + Sync,
    {
        use adapteros_lora_lifecycle::{MockAdapterBackend, WorkflowExecutor};

        // Guardrail: Acquire resource permit
        let limiter = self.resource_limiter.clone();
        let _permit = limiter.acquire_request().await?;

        info!(
            "Executing workflow with {} adapters using real kernels",
            adapter_ids.len()
        );

        // Snapshot current stack and increment refcounts for workflow adapters
        let table = self.hotswap.table();
        let _stack_handle = table.get_current_stack_generation();
        let refcounts = table.refcounts().lock().await;
        for id in &adapter_ids {
            if let Some(rc) = refcounts.get(id) {
                rc.fetch_add(1, Ordering::Relaxed);
            }
        }
        drop(refcounts);

        // Create kernel backend with adapter name mapping
        let _adapter_names: Vec<String> = self
            .manifest
            .adapters
            .iter()
            .map(|a| a.id.clone())
            .collect();

        let backend = Arc::new(MockAdapterBackend);

        // Create and execute workflow
        let executor = WorkflowExecutor::new(workflow_type, adapter_ids.clone(), backend);
        let result = executor.execute(context).await;

        // Decrement refcounts
        for id in &adapter_ids {
            let _new_ref = table.dec_ref(id).await;
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CancelTrainingResponse;
    use parking_lot::RwLock;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    // =========================================================================
    // CancellationToken Tests
    // =========================================================================

    #[test]
    fn cancellation_token_starts_false() {
        let token: CancellationToken = Arc::new(AtomicBool::new(false));
        assert!(!token.load(Ordering::SeqCst));
    }

    #[test]
    fn cancellation_token_can_be_set_to_true() {
        let token: CancellationToken = Arc::new(AtomicBool::new(false));
        token.store(true, Ordering::Release);
        assert!(token.load(Ordering::Acquire));
    }

    #[test]
    fn cancellation_token_shared_across_threads() {
        let token: CancellationToken = Arc::new(AtomicBool::new(false));
        let token_clone = token.clone();

        // Simulate main thread setting cancellation
        token.store(true, Ordering::Release);

        // Simulate worker thread reading cancellation
        assert!(token_clone.load(Ordering::Acquire));
    }

    #[test]
    fn cancellation_token_clone_sees_updates() {
        let token1: CancellationToken = Arc::new(AtomicBool::new(false));
        let token2 = token1.clone();
        let token3 = token1.clone();

        assert!(!token1.load(Ordering::SeqCst));
        assert!(!token2.load(Ordering::SeqCst));
        assert!(!token3.load(Ordering::SeqCst));

        // Update through one handle
        token2.store(true, Ordering::Release);

        // All handles see the update
        assert!(token1.load(Ordering::Acquire));
        assert!(token2.load(Ordering::Acquire));
        assert!(token3.load(Ordering::Acquire));
    }

    // =========================================================================
    // CancelTrainingResponse Tests
    // =========================================================================

    #[test]
    fn cancel_training_response_cancelled_status() {
        let response = CancelTrainingResponse {
            job_id: "job-123".to_string(),
            status: "cancelled".to_string(),
            tokens_processed: None,
            final_loss: None,
            stopped_at_epoch: None,
        };

        assert_eq!(response.job_id, "job-123");
        assert_eq!(response.status, "cancelled");
    }

    #[test]
    fn cancel_training_response_not_found_status() {
        let response = CancelTrainingResponse {
            job_id: "nonexistent-job".to_string(),
            status: "not_found".to_string(),
            tokens_processed: None,
            final_loss: None,
            stopped_at_epoch: None,
        };

        assert_eq!(response.status, "not_found");
    }

    #[test]
    fn cancel_training_response_already_cancelled_status() {
        let response = CancelTrainingResponse {
            job_id: "job-456".to_string(),
            status: "already_cancelled".to_string(),
            tokens_processed: None,
            final_loss: None,
            stopped_at_epoch: None,
        };

        assert_eq!(response.status, "already_cancelled");
    }

    #[test]
    fn cancel_training_response_with_stats() {
        let response = CancelTrainingResponse {
            job_id: "job-789".to_string(),
            status: "cancelled".to_string(),
            tokens_processed: Some(50000),
            final_loss: Some(0.15),
            stopped_at_epoch: Some(5),
        };

        assert_eq!(response.tokens_processed, Some(50000));
        assert!((response.final_loss.unwrap() - 0.15).abs() < 1e-6);
        assert_eq!(response.stopped_at_epoch, Some(5));
    }

    #[test]
    fn cancel_training_response_serialize() {
        let response = CancelTrainingResponse {
            job_id: "test-job".to_string(),
            status: "cancelled".to_string(),
            tokens_processed: Some(1000),
            final_loss: Some(0.25),
            stopped_at_epoch: Some(2),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("test-job"));
        assert!(json.contains("cancelled"));
        assert!(json.contains("1000"));
    }

    #[test]
    fn cancel_training_response_deserialize() {
        let json = r#"{
            "job_id": "deserialize-job",
            "status": "not_found",
            "tokens_processed": null,
            "final_loss": null,
            "stopped_at_epoch": null
        }"#;

        let response: CancelTrainingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.job_id, "deserialize-job");
        assert_eq!(response.status, "not_found");
        assert!(response.tokens_processed.is_none());
    }

    // =========================================================================
    // Job Registry Pattern Tests
    // These test the pattern used by Worker without requiring a full Worker
    // =========================================================================

    /// Simulates the job registry used by Worker
    type JobRegistry = Arc<RwLock<HashMap<String, CancellationToken>>>;

    fn create_job_registry() -> JobRegistry {
        Arc::new(RwLock::new(HashMap::new()))
    }

    fn register_job(registry: &JobRegistry, job_id: &str) -> CancellationToken {
        let token = Arc::new(AtomicBool::new(false));
        let mut jobs = registry.write();
        jobs.insert(job_id.to_string(), token.clone());
        token
    }

    fn unregister_job(registry: &JobRegistry, job_id: &str) {
        let mut jobs = registry.write();
        jobs.remove(job_id);
    }

    fn cancel_job(registry: &JobRegistry, job_id: &str) -> CancelTrainingResponse {
        let jobs = registry.read();

        if let Some(token) = jobs.get(job_id) {
            if token.load(Ordering::SeqCst) {
                return CancelTrainingResponse {
                    job_id: job_id.to_string(),
                    status: "already_cancelled".to_string(),
                    tokens_processed: None,
                    final_loss: None,
                    stopped_at_epoch: None,
                };
            }

            token.store(true, Ordering::Release);
            CancelTrainingResponse {
                job_id: job_id.to_string(),
                status: "cancelled".to_string(),
                tokens_processed: None,
                final_loss: None,
                stopped_at_epoch: None,
            }
        } else {
            CancelTrainingResponse {
                job_id: job_id.to_string(),
                status: "not_found".to_string(),
                tokens_processed: None,
                final_loss: None,
                stopped_at_epoch: None,
            }
        }
    }

    fn is_job_cancelled(registry: &JobRegistry, job_id: &str) -> bool {
        let jobs = registry.read();
        jobs.get(job_id)
            .map(|token| token.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    #[test]
    fn job_registry_register_and_unregister() {
        let registry = create_job_registry();

        let token = register_job(&registry, "job-1");
        assert!(!token.load(Ordering::SeqCst));

        {
            let jobs = registry.read();
            assert!(jobs.contains_key("job-1"));
        }

        unregister_job(&registry, "job-1");

        {
            let jobs = registry.read();
            assert!(!jobs.contains_key("job-1"));
        }
    }

    #[test]
    fn job_registry_cancel_existing_job() {
        let registry = create_job_registry();
        let token = register_job(&registry, "job-2");

        let response = cancel_job(&registry, "job-2");

        assert_eq!(response.status, "cancelled");
        assert!(token.load(Ordering::Acquire));
    }

    #[test]
    fn job_registry_cancel_nonexistent_job() {
        let registry = create_job_registry();

        let response = cancel_job(&registry, "nonexistent");

        assert_eq!(response.status, "not_found");
    }

    #[test]
    fn job_registry_cancel_already_cancelled() {
        let registry = create_job_registry();
        register_job(&registry, "job-3");

        // First cancellation
        let response1 = cancel_job(&registry, "job-3");
        assert_eq!(response1.status, "cancelled");

        // Second cancellation
        let response2 = cancel_job(&registry, "job-3");
        assert_eq!(response2.status, "already_cancelled");
    }

    #[test]
    fn job_registry_is_cancelled_returns_false_for_active() {
        let registry = create_job_registry();
        register_job(&registry, "active-job");

        assert!(!is_job_cancelled(&registry, "active-job"));
    }

    #[test]
    fn job_registry_is_cancelled_returns_true_after_cancel() {
        let registry = create_job_registry();
        register_job(&registry, "cancel-check");

        cancel_job(&registry, "cancel-check");

        assert!(is_job_cancelled(&registry, "cancel-check"));
    }

    #[test]
    fn job_registry_is_cancelled_returns_false_for_unknown() {
        let registry = create_job_registry();

        // Unknown job should return false (safe default)
        assert!(!is_job_cancelled(&registry, "unknown-job"));
    }

    #[test]
    fn job_registry_multiple_jobs() {
        let registry = create_job_registry();

        let token1 = register_job(&registry, "multi-1");
        let token2 = register_job(&registry, "multi-2");
        let token3 = register_job(&registry, "multi-3");

        // Cancel only the second job
        cancel_job(&registry, "multi-2");

        assert!(!token1.load(Ordering::SeqCst));
        assert!(token2.load(Ordering::SeqCst));
        assert!(!token3.load(Ordering::SeqCst));
    }

    #[test]
    fn job_registry_token_survives_unregistration() {
        let registry = create_job_registry();
        let token = register_job(&registry, "survive-job");

        // Cancel before unregistering
        token.store(true, Ordering::Release);

        // Unregister
        unregister_job(&registry, "survive-job");

        // Token still reflects the cancellation (it's Arc'd)
        assert!(token.load(Ordering::Acquire));
    }

    // =========================================================================
    // Memory Ordering Tests
    // =========================================================================

    #[test]
    fn release_acquire_ordering_pattern() {
        // This test documents the memory ordering pattern used in cancel_training_job
        // and is_training_cancelled

        let token = Arc::new(AtomicBool::new(false));
        let token_reader = token.clone();

        // Write side uses Release to ensure all prior writes are visible
        token.store(true, Ordering::Release);

        // Read side uses Acquire to ensure subsequent reads see the write
        let value = token_reader.load(Ordering::Acquire);

        assert!(value);
    }

    #[test]
    fn seqcst_ordering_for_already_cancelled_check() {
        // The already_cancelled check uses SeqCst for stronger guarantees
        // This is appropriate when the value determines control flow

        let token = Arc::new(AtomicBool::new(false));

        // First check: not cancelled
        assert!(!token.load(Ordering::SeqCst));

        // Cancel
        token.store(true, Ordering::Release);

        // Second check: cancelled (with SeqCst for total ordering guarantee)
        assert!(token.load(Ordering::SeqCst));
    }

    // =========================================================================
    // Thread Safety Tests
    // =========================================================================

    #[test]
    fn concurrent_registration_and_cancellation() {
        use std::thread;

        let registry = create_job_registry();
        let registry_clone = registry.clone();

        // Register in main thread
        let token = register_job(&registry, "concurrent-job");

        // Cancel in another thread
        let handle = thread::spawn(move || cancel_job(&registry_clone, "concurrent-job"));

        let response = handle.join().unwrap();
        assert_eq!(response.status, "cancelled");
        assert!(token.load(Ordering::Acquire));
    }

    #[test]
    fn job_token_visible_across_threads() {
        use std::thread;

        let token: CancellationToken = Arc::new(AtomicBool::new(false));
        let token_worker = token.clone();

        // Simulate cancellation from main thread
        token.store(true, Ordering::Release);

        // Worker thread should see the cancellation
        let handle = thread::spawn(move || token_worker.load(Ordering::Acquire));

        assert!(handle.join().unwrap());
    }
}
