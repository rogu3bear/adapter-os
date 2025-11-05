#![allow(dead_code, unused_variables, unused_imports)]

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_lora_rag::RagSystem;
use adapteros_lora_router::Router;
use adapteros_manifest::ManifestV3;
use adapteros_policy::PolicyEngine;
use adapteros_telemetry::TelemetryWriter;
use std::marker::PhantomData;
use std::sync::Arc;

// Module declarations
pub mod adapter_hotswap;
pub mod backend_factory;
pub mod generation;
pub mod inference_pipeline;
pub mod launcher;
pub mod linter_runner;
pub mod signal;
pub mod test_executor;
pub mod tokenizer;
pub mod training;
pub mod uds_server;

// Stub types for compilation compatibility
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatchProposalRequest {
    pub repo_id: String,
    pub commit_sha: Option<String>,
    pub target_files: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum RequestType {
    Inference,
    AdapterOperation,
    MemoryOperation,
    TrainingOperation,
    PolicyUpdate,
    PatchProposal(PatchProposalRequest),
}

// Re-exports for external crates
pub use adapter_hotswap::{AdapterCommand, AdapterCommandResult};
pub use backend_factory::{BackendChoice, create_backend};
pub use inference_pipeline::{InferenceRequest, InferenceResponse, InferencePipelineConfig};
pub use linter_runner::LinterResult;
pub use signal::WorkerSignal;
pub use test_executor::TestResult;
pub use tokenizer::QwenTokenizer;
pub use training::{DatasetGenerator, LoRAWeights, LoRAQuantizer, AdapterPackager, TrainingConfig, TrainingExample};

// Worker struct
pub struct Worker {
    manifest: ManifestV3,
    policy_engine: PolicyEngine,
    router: Router,
    rag_system: RagSystem,
    telemetry_writer: TelemetryWriter,
    lifecycle_manager: Option<Arc<tokio::sync::Mutex<adapteros_lora_lifecycle::LifecycleManager>>>,
    kernels: Option<Box<dyn FusedKernels + Send>>,
    tokenizer_path: Option<String>,
    model_path: Option<String>,
}

impl Worker {
    /// Create a new worker with the specified components
    pub async fn new(
        manifest: ManifestV3,
        kernels: Box<dyn FusedKernels + Send>,
        rag_system: Option<RagSystem>,
        tokenizer_path: &str,
        model_path: &str,
        telemetry_writer: TelemetryWriter,
    ) -> Result<Self> {
        let default_rag = RagSystem::new("./tmp", adapteros_core::B3Hash::hash(b"stub"))
            .unwrap_or_else(|_| panic!("Failed to create default RAG system"));

        Ok(Self {
            manifest,
            policy_engine: PolicyEngine::new(adapteros_manifest::Policies::default()),
            router: Router::new(vec![1.0; 10], 3, 1.0, 0.02, [42u8; 32]),
            rag_system: rag_system.unwrap_or(default_rag),
            telemetry_writer,
            lifecycle_manager: None,
            kernels: Some(kernels),
            tokenizer_path: Some(tokenizer_path.to_string()),
            model_path: Some(model_path.to_string()),
        })
    }

    /// Set signal transmitter (stub implementation)
    pub fn set_signal_tx(&mut self, _tx: tokio::sync::broadcast::Sender<WorkerSignal>) {
        // Stub implementation
    }

    /// Check if policy requires open book
    pub fn policy_requires_open_book(&self) -> bool {
        false // Stub implementation
    }

    /// Get policy abstain threshold
    pub fn policy_abstain_threshold(&self) -> f32 {
        0.5 // Stub implementation
    }

    /// List adapter states view
    pub fn list_adapter_states_view(&self) -> Vec<serde_json::Value> {
        if let Some(ref manager) = self.lifecycle_manager {
            // This would need to be async, but for now return empty
            // In a real implementation, we'd need to make this async
            vec![]
        } else {
            vec![]
        }
    }

    /// Get adapter profile view (stub implementation)
    pub async fn adapter_profile_view(&self, _adapter_id: &str) -> Option<serde_json::Value> {
        None
    }

    /// Promote adapter by ID
    pub fn promote_adapter_by_id(&self, adapter_id: &str) -> Result<()> {
        if let Some(ref manager) = self.lifecycle_manager {
            // Parse adapter_id as u16
            let adapter_idx: u16 = adapter_id.parse().map_err(|_| {
                AosError::Validation(format!("Invalid adapter ID: {}", adapter_id))
            })?;
            // This would need to be async in a real implementation
            Ok(())
        } else {
            Err(AosError::NotFound("Lifecycle manager not available".to_string()))
        }
    }

    /// Demote adapter by ID
    pub fn demote_adapter_by_id(&self, adapter_id: &str) -> Result<()> {
        if let Some(ref manager) = self.lifecycle_manager {
            let adapter_idx: u16 = adapter_id.parse().map_err(|_| {
                AosError::Validation(format!("Invalid adapter ID: {}", adapter_id))
            })?;
            Ok(())
        } else {
            Err(AosError::NotFound("Lifecycle manager not available".to_string()))
        }
    }

    /// Pin adapter by ID
    pub fn pin_adapter_by_id(&self, adapter_id: &str) -> Result<()> {
        if let Some(ref manager) = self.lifecycle_manager {
            let adapter_idx: u16 = adapter_id.parse().map_err(|_| {
                AosError::Validation(format!("Invalid adapter ID: {}", adapter_id))
            })?;
            Ok(())
        } else {
            Err(AosError::NotFound("Lifecycle manager not available".to_string()))
        }
    }

    /// Unpin adapter by ID
    pub fn unpin_adapter_by_id(&self, adapter_id: &str) -> Result<()> {
        if let Some(ref manager) = self.lifecycle_manager {
            let adapter_idx: u16 = adapter_id.parse().map_err(|_| {
                AosError::Validation(format!("Invalid adapter ID: {}", adapter_id))
            })?;
            Ok(())
        } else {
            Err(AosError::NotFound("Lifecycle manager not available".to_string()))
        }
    }

    /// Get profiling snapshot as JSON
    pub fn profiling_snapshot_json(&self) -> serde_json::Value {
        serde_json::json!({
            "worker_status": "active",
            "timestamp": chrono::Utc::now().timestamp(),
            "manifest_schema": self.manifest.schema,
            "router_active": true,
            "telemetry_enabled": true
        })
    }

    /// Warmup the worker
    pub async fn warmup(&self) -> Result<serde_json::Value> {
        // Simulate warmup operations
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        Ok(serde_json::json!({
            "status": "warmup_complete",
            "steps": 1,
            "duration_ms": 10,
            "timestamp": chrono::Utc::now().timestamp(),
            "components_ready": ["router", "rag_system", "telemetry"]
        }))
    }

    /// Execute adapter command (stub implementation)
    pub async fn execute_adapter_command(&self, _command: AdapterCommand) -> Result<AdapterCommandResult> {
        Ok(AdapterCommandResult {
            success: true,
            message: "Command executed".to_string(),
            vram_delta_mb: Some(0),
            duration_ms: 10,
            stack_hash: None,
        })
    }

    /// Propose a patch using the worker (stub implementation)
    pub async fn propose_patch(&self, _request: InferenceRequest, _patch_req: &PatchProposalRequest) -> Result<serde_json::Value> {
        Ok(serde_json::json!({ "status": "patch_proposed", "message": "Stub implementation" }))
    }

    /// Infer using the worker
    pub async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        // For now, return a basic response. In a real implementation,
        // this would use the inference pipeline and tokenizer
        Ok(InferenceResponse {
            text: format!("Response to: {}", request.prompt),
            token_count: request.prompt.split_whitespace().count() + 2,
            latency_ms: 50,
            trace: inference_pipeline::InferenceTrace {
                cpid: request.cpid,
                input_tokens: vec![], // Would be filled by tokenizer
                generated_tokens: vec![1, 2], // Mock tokens
                router_decisions: vec![], // Would be filled by router
                evidence: vec![], // Would be filled if RAG is used
            },
        })
    }
}

// Add unsafe impl Send and Sync
unsafe impl Send for Worker {}
unsafe impl Sync for Worker {}
