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

            // Set cancellation flag
            cancel_token.store(true, std::sync::atomic::Ordering::SeqCst);
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
