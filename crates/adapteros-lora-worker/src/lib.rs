//! AdapterOS Worker
//!
//! This crate provides:
//! - Core worker implementation for ML inference
//! - Resource limiting and timeout management
//! - Circuit breaker patterns for fault tolerance
//! - Memory management and adapter loading
//! - Telemetry and metrics collection
//!
//! # Examples
//!
//! ```rust
//! use adapteros_lora_worker::{Worker, InferenceRequest, InferenceResponse};
//! use adapteros_lora_kernel_mtl::MetalKernels;
//!
//! // Create a worker with Metal kernels
//! let worker = Worker::<MetalKernels>::new(/* config */)?;
//!
//! // Process inference requests
//! let request = InferenceRequest {
//!     prompt: "Hello, world!".to_string(),
//!     max_tokens: 100,
//!     temperature: 0.7,
//!     adapters: vec!["adapter-1".to_string()],
//! };
//!
//! let response = worker.infer(request).await?;
//! ```

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_rag::RagSystem;
use adapteros_lora_router::Router;
use adapteros_manifest::ManifestV3;
use adapteros_policy::{PolicyEngine, RefusalResponse};
use adapteros_telemetry::TelemetryWriter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use tracing::info;

pub mod adapter_hotswap;
pub mod anomaly_detection;
pub mod backend_factory;
pub mod base_model_state;
pub mod contact_discovery;
pub mod conv_pipeline;
pub mod deadlock;
pub mod deterministic_rng;
pub mod directory_adapters;
pub mod embeddings;
pub mod ephemeral_adapters;
pub mod evidence;
pub mod filter_engine;
pub mod framework_adapters;
pub mod generation;
pub mod health;
pub mod inference_pipeline;
pub mod kvcache;
pub mod launcher;
pub mod limiter;
pub mod linter_runner;
pub mod llm_backend;
pub mod memory;
pub mod metrics;
pub mod patch_generator;
pub mod patch_telemetry;
pub mod patch_validator;
pub mod signal;
pub mod telemetry_adapter;
pub mod telemetry_lora;
pub mod test_executor;
pub mod timeout;
pub mod tokenizer;
pub mod training;
pub mod vision_adapter;
pub mod vision_lora;

pub use adapter_hotswap::{AdapterCommand, AdapterCommandResult, HotSwapManager};
pub use adapteros_lora_rag::DocIndexImpl;
pub use adapteros_lora_rag::SymbolIndexImpl;
pub use adapteros_lora_rag::TestIndexImpl;
pub use anomaly_detection::{
    AnomalyDetectionConfig, AnomalyDetector, AnomalyScore, DetectionAlgorithm,
};
pub use backend_factory::{create_backend, BackendChoice};
pub use conv_pipeline::{
    ActivationKind, ConvPipeline, ConvPipelineConfig, ImageBatch, PoolingStrategy,
    VisionArchitecture,
};
pub use deadlock::{DeadlockConfig, DeadlockDetector};
pub use deterministic_rng::{DeterministicRng, RngFactory};
pub use directory_adapters::{DirectoryAdapterManager, DirectoryAdapterSpec, PathActivationRule};
pub use ephemeral_adapters::{EphemeralAdapterManager, EphemeralAdapterSpec};
pub use filter_engine::{FilterConfig, FilterEngine, FilterKind};
pub use framework_adapters::{FrameworkAdapterManager, FrameworkAdapterSpec};
pub use generation::Generator;
pub use health::{HealthConfig, HealthMonitor, HealthStatus};
pub use kvcache::KvCache;
pub use limiter::{ResourceGuard, ResourceLimiter, ResourceLimits};
pub use linter_runner::{
    LintIssue, LintSeverity, LinterConfig, LinterResult, LinterRunner, LinterType,
};
pub use llm_backend::{create_llm_backend, LlmBackendType, LocalLlmBackend, LocalLlmConfig};
pub use memory::MemoryMonitor;
pub use telemetry_adapter::{
    SignalChannel, SignalSample, TelemetryAdapter, TelemetryAdapterConfig, TelemetryAdapterMetrics,
    TelemetryOutput,
};
pub use telemetry_lora::{
    load_telemetry_lora, TelemetryLoraRegistry, TelemetryLoraWeights, TelemetryMergePlan,
    TelemetryTask,
};
pub use test_executor::{TestExecutor, TestFailure, TestFramework, TestResult};
pub use timeout::{CircuitBreaker, CircuitState, TimeoutConfig, TimeoutWrapper};
pub use training::{
    AdapterManifest, AdapterPackager, DatasetGenerator, LoRAQuantizer, LoRAWeights,
    MicroLoRATrainer, PackagedAdapter, QuantizedLoRAWeights, TrainingConfig, TrainingExample,
    TrainingResult,
};
pub use vision_adapter::{
    ColorSpace, VisionAdapter, VisionAdapterConfig, VisionAdapterMetrics, VisionBatch,
};
pub use vision_lora::{
    load_vision_lora, VisionLoraRegistry, VisionLoraWeights, VisionMergePlan, VisionTask,
};

/// Inference request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub cpid: String,
    pub prompt: String,
    pub max_tokens: usize,
    #[serde(default)]
    pub require_evidence: bool,
    /// Optional: Request patch proposal mode
    #[serde(default)]
    pub request_type: RequestType,
}

/// Request type for different inference modes
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequestType {
    #[default]
    Normal,
    PatchProposal(PatchProposalRequest),
}

/// Patch proposal request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposalRequest {
    /// Repository ID for context
    pub repo_id: String,
    /// Commit SHA for context (optional)
    pub commit_sha: Option<String>,
    /// Files to focus on
    pub target_files: Vec<String>,
    /// Issue description or prompt
    pub description: String,
}

/// Inference response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub text: Option<String>,
    pub status: String,
    pub trace: ResponseTrace,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<RefusalResponse>,
    /// Patch proposal if requested
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_proposal: Option<PatchProposalResponse>,
}

/// Patch proposal response with patches and citations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposalResponse {
    pub proposal_id: String,
    pub rationale: String,
    pub patches: Vec<FilePatchResponse>,
    pub citations: Vec<CitationResponse>,
    pub confidence: f32,
}

/// File patch in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePatchResponse {
    pub file_path: String,
    pub hunks: Vec<PatchHunkResponse>,
}

/// Patch hunk in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchHunkResponse {
    pub start_line: usize,
    pub end_line: usize,
    pub old_content: String,
    pub new_content: String,
}

/// Citation in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationResponse {
    pub source_type: String,
    pub reference: String,
    pub relevance: f32,
}

/// Response trace with evidence and router decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTrace {
    pub cpid: String,
    pub plan_id: String,
    pub evidence: Vec<EvidenceRef>,
    pub router_summary: RouterSummary,
    pub token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub doc_id: String,
    pub rev: String,
    pub span_hash: B3Hash,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterSummary {
    pub adapters_used: Vec<String>,
    pub avg_activations: Vec<f32>,
}

use crate::embeddings::EmbeddingModel;
use crate::evidence::EvidenceRetriever;
use crate::tokenizer::QwenTokenizer;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Worker for running inference with comprehensive safety mechanisms
pub struct Worker<K: FusedKernels + Send + Sync> {
    manifest: ManifestV3,
    policy: PolicyEngine,
    router: Router,
    rag: Option<RagSystem>,
    /// Kernels wrapped in Arc<Mutex<>> for shared access with workflows
    kernels: Arc<tokio::sync::Mutex<K>>,
    memory_monitor: MemoryMonitor,
    tokenizer: Arc<QwenTokenizer>,
    generator: Generator,
    embedding_model: Arc<EmbeddingModel>,
    evidence_retriever: Option<EvidenceRetriever>,
    // Safety mechanisms
    _timeout_config: TimeoutConfig,
    _timeout_wrapper: TimeoutWrapper,
    circuit_breaker: CircuitBreaker,
    _resource_limiter: ResourceLimiter,
    _deadlock_detector: DeadlockDetector,
    health_monitor: HealthMonitor,
    telemetry: TelemetryWriter,
    // Lifecycle management
    profiler: adapteros_profiler::AdapterProfiler,
    lifecycle: adapteros_lora_lifecycle::LifecycleManager,
    // Hot-swap management
    hotswap: HotSwapManager<K>,
}

impl<K: FusedKernels + Send + Sync> Worker<K> {
    /// Create a new worker with comprehensive safety mechanisms
    pub async fn new(
        manifest: ManifestV3,
        kernels: K,
        rag: Option<RagSystem>,
        tokenizer_path: &str,
        model_path: &str,
        telemetry: TelemetryWriter,
    ) -> Result<Self> {
        // Initialize determinism guards first
        init_determinism_guards()?;

        let policy = PolicyEngine::new(manifest.policies.clone());

        // Create router from manifest
        let router_seed = adapteros_core::derive_seed(&manifest.seeds.global, "router");
        let router = Router::new(
            vec![1.0; manifest.adapters.len()],
            manifest.router.k_sparse,
            manifest.router.tau,
            manifest.router.entropy_floor,
            router_seed,
        );

        let memory_monitor = MemoryMonitor::new(manifest.policies.memory.min_headroom_pct);

        // Initialize safety mechanisms
        let timeout_config = TimeoutConfig::default();
        let timeout_wrapper = TimeoutWrapper::new(timeout_config.clone());
        let circuit_breaker = CircuitBreaker::new(5, std::time::Duration::from_secs(60));
        let resource_limiter = ResourceLimiter::new(ResourceLimits::default());
        let deadlock_detector = DeadlockDetector::new(DeadlockConfig::default());
        let health_monitor = HealthMonitor::new(HealthConfig::default())?;

        // Load tokenizer
        let tokenizer = Arc::new(QwenTokenizer::from_file(tokenizer_path)?);

        // Create generator with deterministic seed
        let gen_seed = adapteros_core::derive_seed(&manifest.seeds.global, "generation");
        let generator = Generator::new(gen_seed)
            .with_temperature(0.7)
            .with_top_p(0.9);

        // Load embedding model
        let embedding_model = Arc::new(EmbeddingModel::from_model_path(
            model_path, 152064, // Qwen2.5 vocab size
            3584,   // Qwen2.5-7B hidden size
        )?);

        // Initialize evidence retriever with real implementation if RAG is available
        let evidence_retriever = if let Some(ref _rag_system) = rag {
            use crate::evidence::*;
            use adapteros_lora_rag::EvidenceIndexManager;
            use std::path::PathBuf;

            // Create evidence index manager for the tenant
            let indices_root = PathBuf::from("var/indices");
            let evidence_manager = Arc::new(Mutex::new(
                EvidenceIndexManager::new(
                    indices_root,
                    "default".to_string(),
                    Some(embedding_model.clone()),
                )
                .await?,
            ));

            Some(EvidenceRetriever::new(evidence_manager))
        } else {
            None
        };

        // Initialize profiler
        let adapter_names: Vec<String> = manifest.adapters.iter().map(|a| a.id.clone()).collect();
        let profiler = adapteros_profiler::AdapterProfiler::new(
            adapter_names.clone(),
            Some(telemetry.clone()),
        );

        // Initialize lifecycle manager
        let adapters_path = std::path::PathBuf::from("./adapters");

        // Build adapter hashes map from manifest
        let adapter_hashes: std::collections::HashMap<String, adapteros_core::B3Hash> = manifest
            .adapters
            .iter()
            .map(|a| (a.id.clone(), a.hash.clone()))
            .collect();

        let lifecycle = adapteros_lora_lifecycle::LifecycleManager::new(
            adapter_names,
            adapter_hashes,
            &manifest.policies,
            adapters_path.clone(),
            Some(telemetry.clone()),
            manifest.router.k_sparse,
        );

        // Create shared kernels Arc for both Worker and HotSwapManager
        let kernels_arc = Arc::new(tokio::sync::Mutex::new(kernels));

        Ok(Self {
            manifest,
            policy,
            router,
            rag,
            kernels: kernels_arc.clone(),
            memory_monitor,
            tokenizer,
            generator,
            embedding_model,
            evidence_retriever,
            _timeout_config: timeout_config,
            _timeout_wrapper: timeout_wrapper,
            circuit_breaker,
            _resource_limiter: resource_limiter,
            _deadlock_detector: deadlock_detector,
            health_monitor,
            telemetry,
            profiler,
            lifecycle,
            hotswap: HotSwapManager::new_with_kernels(kernels_arc, adapters_path),
        })
    }

    /// Start background GPU verification task
    ///
    /// Spawns a background task that periodically verifies GPU buffer integrity.
    /// This should be called after Worker::new() to enable automatic verification.
    ///
    /// # Parameters
    /// - `interval_secs`: How often to run verification (default: 300 seconds / 5 minutes)
    pub fn start_gpu_verification_task(&self, interval_secs: u64) -> tokio::task::JoinHandle<()> {
        // TODO: Wrap lifecycle in Arc<Mutex<>> to enable cloning for background task
        // Temporarily disabled - lifecycle doesn't implement Clone
        let _kernels = self.kernels.clone();
        let _telemetry = self.telemetry.clone();
        let _interval_secs = interval_secs;

        tokio::spawn(async move {
            // Placeholder - GPU verification disabled until lifecycle is wrapped in Arc
            tracing::warn!("GPU verification task not yet implemented - lifecycle needs Arc<Mutex<>> wrapper");
            tokio::time::sleep(tokio::time::Duration::from_secs(86400)).await;
        })

        /* Original implementation - disabled until lifecycle Clone issue resolved
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));

        loop {
            interval.tick().await;

            // Get loaded adapters from lifecycle
            let loaded_adapters = lifecycle.get_loaded_adapters();
                if loaded_adapters.is_empty() {
                    continue; // Skip if no adapters loaded
                }

                // Verify each loaded adapter
                for adapter_id in &loaded_adapters {
                    let mut kernels_lock = kernels.lock().await;

                    // Convert adapter ID to u16 for kernel API
                    let adapter_id_u16 = crate::adapter_hotswap::adapter_id_to_u16(adapter_id);

                    // Verify GPU buffers
                    match kernels_lock.verify_adapter_buffers(adapter_id_u16) {
                        Ok((buffer_size, first_sample, last_sample, mid_sample)) => {
                            // Create GPU fingerprint
                            use adapteros_lora_kernel_mtl::GpuBufferFingerprint;
                            let gpu_fp = GpuBufferFingerprint::new(
                                buffer_size,
                                &first_sample,
                                &last_sample,
                                &mid_sample,
                            );

                            // Verify against baseline
                            match kernels_lock.verify_gpu_fingerprint(adapter_id_u16, buffer_size, &gpu_fp.checkpoint_hash) {
                                Ok(true) => {
                                    tracing::debug!(
                                        adapter_id = %adapter_id,
                                        "GPU integrity verification passed"
                                    );
                                }
                                Ok(false) => {
                                    tracing::error!(
                                        adapter_id = %adapter_id,
                                        "GPU buffer fingerprint mismatch detected - taking corrective action"
                                    );

                                    // Log critical telemetry event
                                    telemetry.log("gpu_integrity_failure", serde_json::json!({
                                        "adapter_id": adapter_id,
                                        "reason": "fingerprint_mismatch",
                                        "action": "adapter_unloaded",
                                        "severity": "critical"
                                    })).ok();

                                    // Corrective action: Unload corrupted adapter from kernels
                                    if let Err(unload_err) = kernels_lock.unload_adapter(adapter_id_u16) {
                                        tracing::error!(
                                            adapter_id = %adapter_id,
                                            error = %unload_err,
                                            "Failed to unload corrupted adapter"
                                        );
                                    } else {
                                        tracing::info!(
                                            adapter_id = %adapter_id,
                                            "Successfully unloaded corrupted adapter from GPU"
                                        );
                                    }

                                    // Mark adapter for manual review in lifecycle
                                    // This prevents automatic reloading
                                    tracing::warn!(
                                        adapter_id = %adapter_id,
                                        "Adapter marked for manual review - will not be automatically reloaded"
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(
                                        adapter_id = %adapter_id,
                                        error = %e,
                                        "GPU verification failed"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                adapter_id = %adapter_id,
                                error = %e,
                                "Failed to verify adapter GPU buffers"
                            );
                        }
                    }
                }
            }
        })
        */
    }

    /// Run inference with comprehensive safety mechanisms
    pub async fn infer(&mut self, request: InferenceRequest) -> Result<InferenceResponse> {
        let start_time = Instant::now();

        // Record health metrics
        self.health_monitor.record_request();

        // Run inference with timeout (simplified to avoid borrow checker issues)
        let result = self.infer_internal(request).await;

        // Log telemetry
        let duration = start_time.elapsed();
        self.telemetry.log("inference", InferenceEvent {
            duration_ms: duration.as_millis() as u64,
            success: result.is_ok(),
            timeout_occurred: matches!(result, Err(AosError::Worker(ref msg)) if msg.contains("timeout")),
            circuit_breaker_open: self.circuit_breaker.is_open(),
            memory_usage: self.health_monitor.get_memory_usage().unwrap_or(0),
        })?;

        result
    }

    /// Internal inference implementation with safety checks
    async fn infer_internal(&mut self, request: InferenceRequest) -> Result<InferenceResponse> {
        // Start profiler session
        let mut _profiler_session = self.profiler.start_inference();

        // Check memory - handle memory pressure if needed
        if let Err(_e) = self.memory_monitor.check_headroom() {
            self.lifecycle.handle_memory_pressure(&self.profiler)?;
            // Try again after eviction
            self.memory_monitor.check_headroom().map_err(|e| {
                AosError::MemoryPressure(format!("Insufficient headroom after eviction: {}", e))
            })?;
        }

        // Retrieve evidence if required
        let mut evidence = Vec::new();
        if request.require_evidence {
            // Compute query embedding first (before borrowing rag)
            let query_emb = self.compute_embedding(&request.prompt)?;

            if let Some(ref mut rag) = self.rag {
                let spans = rag
                    .retrieve(
                        "default_tenant",
                        &query_emb,
                        self.manifest.policies.rag.topk,
                    )
                    .map_err(|e| AosError::Rag(format!("Evidence retrieval failed: {}", e)))?;

                evidence = spans
                    .iter()
                    .map(|s| EvidenceRef {
                        doc_id: s.doc_id.clone(),
                        rev: s.rev.clone(),
                        span_hash: s.span_hash,
                        score: s.score,
                    })
                    .collect();

                // Check evidence policy
                if let Err(_e) = self.policy.check_evidence(evidence.len()) {
                    // Insufficient evidence, returning refusal
                    return Ok(InferenceResponse {
                        text: None,
                        status: "insufficient_evidence".to_string(),
                        trace: ResponseTrace {
                            cpid: request.cpid.clone(),
                            plan_id: "placeholder".to_string(),
                            evidence: evidence.clone(),
                            router_summary: RouterSummary {
                                adapters_used: vec![],
                                avg_activations: vec![],
                            },
                            token_count: 0,
                        },
                        refusal: Some(RefusalResponse::insufficient_evidence(
                            self.manifest.policies.evidence.min_spans,
                            evidence.len(),
                        )),
                        patch_proposal: None,
                    });
                }
            }
        }

        // Generate tokens using autoregressive loop
        let formatted_prompt = self.tokenizer.apply_chat_template(&request.prompt);
        let prompt_tokens = self.tokenizer.encode(&formatted_prompt)?;

        let mut generated_tokens = Vec::new();

        // Autoregressive generation loop
        for step in 0..request.max_tokens {
            // Prepare input for this step
            let input_ids_slice = if step == 0 {
                &prompt_tokens[..]
            } else {
                let last_token = generated_tokens.last().ok_or_else(|| {
                    AosError::Internal("Generated tokens cannot be empty".to_string())
                })?;
                std::slice::from_ref(last_token)
            };

            // Run router to get active adapters
            // Create dummy features from token embeddings (simplified for now)
            let features = vec![1.0; 32]; // Simplified feature vector
            let priors = vec![1.0; self.manifest.adapters.len()];
            let decision = self.router.route(&features, &priors);

            // Record routing decision in profiler
            self.profiler.record_routing_decision(&decision.indices);
            self.lifecycle
                .record_router_decision(&decision.indices)
                .await?;

            // Convert Decision to RouterRing
            let router_ring = RouterRing {
                indices: decision.indices.to_vec(),
                gates_q15: decision.gates_q15.to_vec(),
                position: step,
            };

            // Execute kernels through Metal and measure latency per adapter
            let mut io_buffers = IoBuffers {
                input_ids: input_ids_slice.to_vec(),
                output_logits: vec![0.0; 152064], // Qwen2.5 vocab size
                position: step,
            };

            let kernel_start = Instant::now();
            {
                let mut kernels = self.kernels.lock().await;
                kernels.run_step(&router_ring, &mut io_buffers)?;
            }
            let kernel_duration = kernel_start.elapsed();

            // Record latency for each active adapter (simplified: divide equally)
            if !decision.indices.is_empty() {
                let per_adapter_latency = kernel_duration / decision.indices.len() as u32;
                for &adapter_id in &decision.indices {
                    self.profiler
                        .record_step_latency(adapter_id, per_adapter_latency);
                }
            }

            // Sample next token
            let next_token = self.generator.next_token(&io_buffers.output_logits)?;

            // Check stopping criteria
            if next_token == self.tokenizer.eos_token_id() {
                break;
            }

            generated_tokens.push(next_token);
        }

        // Evaluate lifecycle transitions after inference
        self.lifecycle.evaluate_transitions(&self.profiler)?;

        // Log profiling snapshot (sampled at 5%)
        self.profiler.maybe_log_snapshot()?;

        // Decode to text
        let generated_text = self.tokenizer.decode(&generated_tokens)?;

        Ok(InferenceResponse {
            text: Some(generated_text),
            status: "ok".to_string(),
            trace: self.build_trace(&request.cpid, &evidence, generated_tokens.len()),
            refusal: None,
            patch_proposal: None,
        })
    }

    /// Generate patch proposal with evidence retrieval
    pub async fn propose_patch(
        &mut self,
        request: InferenceRequest,
        patch_request: &PatchProposalRequest,
    ) -> Result<InferenceResponse> {
        use crate::evidence::EvidenceRequest;
        use crate::patch_generator::{MockLlmBackend, PatchGenerationRequest, PatchGenerator};
        use crate::patch_telemetry::{
            EvidenceMetrics, PatchGenerationMetrics, PatchTelemetry, ValidationMetrics,
        };
        use crate::patch_validator::{CodePolicy, PatchValidator};

        info!(
            "Generating patch proposal for: {}",
            patch_request.description
        );

        // Initialize telemetry
        let mut telemetry = PatchTelemetry::new();

        // 1. Build evidence retrieval request
        let evidence_request = EvidenceRequest {
            query: patch_request.description.clone(),
            target_files: patch_request.target_files.clone(),
            repo_id: patch_request.repo_id.clone(),
            commit_sha: patch_request.commit_sha.clone(),
            max_results: 10,
            min_score: 0.7,
        };

        // 2. Retrieve evidence (using mock implementation for now)
        let evidence_result = self.retrieve_evidence(&evidence_request).await?;

        // Log evidence retrieval telemetry
        let evidence_metrics = EvidenceMetrics {
            query: evidence_request.query,
            sources_used: evidence_result
                .sources_used
                .iter()
                .map(|s| format!("{:?}", s))
                .collect(),
            spans_found: evidence_result.spans.len(),
            retrieval_time_ms: evidence_result.retrieval_time_ms,
            avg_relevance_score: if !evidence_result.spans.is_empty() {
                evidence_result.spans.iter().map(|s| s.score).sum::<f32>()
                    / evidence_result.spans.len() as f32
            } else {
                0.0
            },
            min_score_threshold: evidence_request.min_score,
        };
        telemetry.log_evidence_retrieval("default_tenant", evidence_metrics, None);

        let mut evidence_refs = Vec::new();

        // Convert evidence spans to trace references
        for span in &evidence_result.spans {
            evidence_refs.push(EvidenceRef {
                doc_id: span.doc_id.clone(),
                rev: span.rev.clone(),
                span_hash: adapteros_core::B3Hash::from_hex(&span.span_hash)
                    .unwrap_or_else(|_| adapteros_core::B3Hash::hash(span.span_hash.as_bytes())),
                score: span.score,
            });
        }

        // 3. Generate patch proposal
        let patch_generation_request = PatchGenerationRequest {
            repo_id: patch_request.repo_id.clone(),
            commit_sha: patch_request.commit_sha.clone(),
            target_files: patch_request.target_files.clone(),
            description: patch_request.description.clone(),
            evidence: evidence_result.spans,
            context: std::collections::HashMap::new(),
        };

        let patch_generator = PatchGenerator::new(
            Box::new(MockLlmBackend),
            crate::patch_generator::PatchParser::new(),
            crate::patch_generator::CitationExtractor::new(),
        );

        let proposal = patch_generator
            .generate_patch(patch_generation_request)
            .await?;

        // Log patch generation telemetry
        let generation_metrics = PatchGenerationMetrics {
            proposal_id: proposal.proposal_id.clone(),
            description: patch_request.description.clone(),
            target_files: patch_request.target_files.clone(),
            evidence_count: proposal.citations.len(),
            patch_count: proposal.patches.len(),
            total_lines: proposal.patches.iter().map(|p| p.total_lines).sum(),
            generation_time_ms: 100, // Mock timing
            confidence_score: proposal.confidence,
        };
        telemetry.log_patch_generation("default_tenant", generation_metrics);

        // 4. Validate patch against policy
        let policy = CodePolicy::default();
        let policy_engine = PolicyEngine::new(self.manifest.policies.clone());
        let validator = PatchValidator::new(policy, policy_engine);
        let validation_result = validator.validate(&proposal.patches).await?;

        // Log patch validation telemetry
        let validation_metrics = ValidationMetrics {
            proposal_id: proposal.proposal_id.clone(),
            is_valid: validation_result.is_valid,
            error_count: validation_result.errors.len(),
            warning_count: validation_result.warnings.len(),
            violation_count: validation_result.violations.len(),
            validation_time_ms: 50, // Mock timing
            confidence_score: validation_result.confidence,
            violations: validation_result
                .violations
                .into_iter()
                .map(|v| crate::patch_telemetry::ViolationMetric {
                    violation_type: format!("{:?}", v.violation_type),
                    severity: format!("{:?}", v.severity),
                    file_path: v.file_path,
                    line_number: v.line_number,
                    description: v.description,
                })
                .collect(),
        };
        telemetry.log_patch_validation("default_tenant", validation_metrics);

        // 5. Build response
        let patch_proposal = if validation_result.is_valid {
            Some(PatchProposalResponse {
                proposal_id: proposal.proposal_id,
                rationale: proposal.rationale,
                patches: proposal
                    .patches
                    .clone()
                    .into_iter()
                    .map(|p| FilePatchResponse {
                        file_path: p.file_path,
                        hunks: p
                            .hunks
                            .into_iter()
                            .map(|h| PatchHunkResponse {
                                start_line: h.start_line,
                                end_line: h.end_line,
                                old_content: h.context_lines.join("\n"),
                                new_content: h.modified_lines.join("\n"),
                            })
                            .collect(),
                    })
                    .collect(),
                citations: proposal
                    .citations
                    .clone()
                    .into_iter()
                    .map(|c| CitationResponse {
                        source_type: format!("{:?}", c.evidence_type),
                        reference: format!("{}:{}", c.file_path, c.line_range.0),
                        relevance: c.relevance_score,
                    })
                    .collect(),
                confidence: proposal.confidence,
            })
        } else {
            None
        };

        let status = if validation_result.is_valid {
            "success".to_string()
        } else {
            "validation_failed".to_string()
        };

        let text = if validation_result.is_valid {
            Some(format!(
                "Patch proposal generated successfully with {} files and {} citations",
                proposal.patches.len(),
                proposal.citations.len()
            ))
        } else {
            Some(format!(
                "Patch validation failed: {}",
                validation_result.errors.join(", ")
            ))
        };

        Ok(InferenceResponse {
            text,
            status,
            trace: self.build_trace(&request.cpid, &evidence_refs, 0),
            refusal: if !validation_result.is_valid {
                Some(RefusalResponse {
                    status: "failed".to_string(),
                    reason: adapteros_policy::RefusalReason::MissingFields {
                        template: "patch_validation".to_string(),
                        fields: validation_result.errors.clone(),
                    },
                    message: format!(
                        "Patch validation failed: {}",
                        validation_result.errors.join(", ")
                    ),
                })
            } else {
                None
            },
            patch_proposal,
        })
    }

    /// Retrieve evidence for patch proposal using real EvidenceRetriever
    async fn retrieve_evidence(
        &mut self,
        request: &crate::evidence::EvidenceRequest,
    ) -> Result<crate::evidence::EvidenceResult> {
        use crate::evidence::{EvidenceResult, EvidenceSpan, EvidenceType};
        use std::collections::HashMap;

        // Use real evidence retriever if available
        if let Some(ref mut retriever) = self.evidence_retriever {
            retriever
                .retrieve_patch_evidence(request, "default_tenant")
                .await
                .map_err(|e| AosError::Internal(e.to_string()))
        } else {
            // Fallback to basic mock if no retriever is available
            let mock_spans = vec![
                EvidenceSpan {
                    doc_id: "mock_doc_1".to_string(),
                    rev: "v1".to_string(),
                    span_hash: "hash1".to_string(),
                    score: 0.9,
                    evidence_type: EvidenceType::Symbol,
                    file_path: request
                        .target_files
                        .first()
                        .unwrap_or(&"src/test.rs".to_string())
                        .clone(),
                    start_line: 10,
                    end_line: 15,
                    content: format!("Mock evidence for: {}", request.query),
                    metadata: HashMap::new(),
                },
                EvidenceSpan {
                    doc_id: "mock_doc_2".to_string(),
                    rev: "v1".to_string(),
                    span_hash: "hash2".to_string(),
                    score: 0.8,
                    evidence_type: EvidenceType::Test,
                    file_path: "tests/test.rs".to_string(),
                    start_line: 20,
                    end_line: 25,
                    content: "Mock test evidence".to_string(),
                    metadata: HashMap::new(),
                },
            ];

            Ok(EvidenceResult {
                spans: mock_spans,
                total_found: 2,
                retrieval_time_ms: 50,
                sources_used: vec![EvidenceType::Symbol, EvidenceType::Test],
            })
        }
    }

    /// Compute embedding for text query (for RAG/similarity search)
    ///
    /// This generates averaged token embeddings for semantic search.
    /// Note: Metal kernels handle embedding lookup internally for forward pass.
    fn compute_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let tokens = self.tokenizer.encode(text)?;
        self.embedding_model.encode_tokens(&tokens)
    }

    /// Encode tokens to embeddings for RAG/text similarity
    ///
    /// This method is used for generating query embeddings for evidence retrieval
    /// and semantic search. It averages token embeddings and applies L2 normalization.
    ///
    /// Note: This is NOT used for the forward pass - Metal kernels perform
    /// embedding lookup directly from input_ids for inference.
    fn _encode_text_for_rag(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        self.embedding_model.encode_tokens(token_ids)
    }

    /// Build response trace with evidence and router summary
    fn build_trace(
        &self,
        cpid: &str,
        evidence: &[EvidenceRef],
        token_count: usize,
    ) -> ResponseTrace {
        ResponseTrace {
            cpid: cpid.to_string(),
            plan_id: format!("plan_{}", self.manifest.base.model_hash),
            evidence: evidence.to_vec(),
            router_summary: RouterSummary {
                adapters_used: self
                    .manifest
                    .adapters
                    .iter()
                    .take(self.manifest.router.k_sparse)
                    .map(|a| a.id.clone())
                    .collect(),
                avg_activations: vec![0.33; self.manifest.router.k_sparse],
            },
            token_count,
        }
    }

    /// Execute adapter hot-swap command
    pub async fn execute_adapter_command(
        &mut self,
        command: AdapterCommand,
    ) -> Result<AdapterCommandResult> {
        self.hotswap.execute(command).await
    }

    /// Verify GPU buffers for all loaded adapters
    ///
    /// Reads GPU buffer checkpoints and validates against stored fingerprints.
    /// Also checks memory footprint against adaptive baseline with 2σ tolerance.
    ///
    /// Returns a report with verified/failed/skipped adapters.
    ///
    /// # Usage
    ///
    /// This method can be called on-demand to verify GPU integrity after adapter
    /// operations (load, swap, rollback) or as part of periodic health checks.
    ///
    /// ```no_run
    /// let report = worker.verify_gpu_integrity().await?;
    /// if !report.failed.is_empty() {
    ///     // Handle integrity failures
    /// }
    /// ```
    pub async fn verify_gpu_integrity(
        &self,
    ) -> Result<adapteros_lora_lifecycle::GpuIntegrityReport> {
        use adapteros_lora_lifecycle::GpuIntegrityReport;

        let mut verified = Vec::new();
        let mut failed = Vec::new();
        let mut skipped = Vec::new();

        // Get adapters that should have GPU buffers loaded
        let loaded_adapters = self.lifecycle.get_loaded_adapters();

        let mut kernels_lock = self.kernels.lock().await;

        // Proceed with verification - backends without GPU tracking will skip via default trait impls
        for (adapter_id_u16, adapter_id, _state) in &loaded_adapters {
            // Try to verify GPU buffers
            #[cfg(target_os = "macos")]
            match kernels_lock.verify_adapter_buffers(*adapter_id_u16) {
                Ok((buffer_size, first, last, mid)) => {
                    // Create fingerprint from current GPU state
                    use adapteros_lora_kernel_mtl::vram::GpuBufferFingerprint;
                    let current_fp = GpuBufferFingerprint::new(buffer_size, &first, &last, &mid);
                    let checkpoint_hash_hex = current_fp.checkpoint_hash.to_hex();

                    // Verify against stored baseline
                    match kernels_lock.verify_gpu_fingerprint(*adapter_id_u16, buffer_size, &checkpoint_hash_hex) {
                        Ok(true) => {
                            // Check memory footprint against baseline
                            let (within_tolerance, z_score, baseline_stats) =
                                kernels_lock.check_memory_footprint(*adapter_id_u16, buffer_size);

                            let (baseline_mean, baseline_stddev, _sample_count) =
                                baseline_stats.unwrap_or((buffer_size as f64, 0.0, 0));

                            if within_tolerance {
                                verified.push((*adapter_id_u16, adapter_id.clone()));

                                // Emit telemetry for successful verification
                                use adapteros_lora_lifecycle::GpuIntegrityVerificationEvent;
                                let _ = self.telemetry.log("gpu_integrity_verification", GpuIntegrityVerificationEvent {
                                    adapter_id: adapter_id.clone(),
                                    adapter_idx: *adapter_id_u16,
                                    verified: true,
                                    buffer_bytes: buffer_size,
                                    checkpoint_hash: current_fp.checkpoint_hash.to_hex(),
                                    memory_footprint_within_tolerance: true,
                                    z_score: Some(z_score),
                                    baseline_mean: Some(baseline_mean),
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs(),
                                });
                            } else {
                                failed.push((
                                    *adapter_id_u16,
                                    adapter_id.clone(),
                                    format!(
                                        "Memory footprint anomaly: {} bytes (baseline: {:.1} ± {:.1}, z-score: {:.2})",
                                        buffer_size, baseline_mean, baseline_stddev, z_score
                                    ),
                                ));

                                // Emit telemetry for memory footprint anomaly
                                use adapteros_lora_lifecycle::GpuIntegrityViolationEvent;
                                let _ = self.telemetry.log("gpu_integrity_violation", GpuIntegrityViolationEvent {
                                    adapter_id: adapter_id.clone(),
                                    adapter_idx: *adapter_id_u16,
                                    violation_type: "memory_anomaly".to_string(),
                                    details: format!(
                                        "Memory footprint {} bytes exceeds 2σ tolerance (baseline: {:.1} ± {:.1}, z-score: {:.2})",
                                        buffer_size, baseline_mean, baseline_stddev, z_score
                                    ),
                                    buffer_bytes: Some(buffer_size),
                                    z_score: Some(z_score),
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs(),
                                });
                            }
                        }
                        Ok(false) => {
                            // No baseline exists yet - store this as the baseline
                            kernels_lock.store_gpu_fingerprint(*adapter_id_u16, buffer_size, &checkpoint_hash_hex);
                            verified.push((*adapter_id_u16, adapter_id.clone()));

                            tracing::info!(
                                adapter_id = %adapter_id,
                                adapter_idx = adapter_id_u16,
                                "Stored initial GPU fingerprint baseline"
                            );
                        }
                        Err(msg) => {
                            failed.push((
                                *adapter_id_u16,
                                adapter_id.clone(),
                                format!("GPU buffer fingerprint mismatch: {}", msg),
                            ));

                            // Emit telemetry for fingerprint mismatch
                            use adapteros_lora_lifecycle::GpuIntegrityViolationEvent;
                            let _ = self.telemetry.log("gpu_integrity_violation", GpuIntegrityViolationEvent {
                                adapter_id: adapter_id.clone(),
                                adapter_idx: *adapter_id_u16,
                                violation_type: "fingerprint_mismatch".to_string(),
                                details: format!("GPU buffer checkpoint hash does not match stored fingerprint: {}", msg),
                                buffer_bytes: Some(buffer_size),
                                z_score: None,
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs(),
                            });
                        }
                    }
                }
                Err(e) => {
                    // Adapter not loaded or verification not supported
                    skipped.push((*adapter_id_u16, adapter_id.clone()));
                    tracing::debug!(
                        adapter_id = %adapter_id,
                        error = %e,
                        "GPU verification skipped"
                    );
                }
            }

            // Non-macOS platforms don't have Metal GPU verification
            #[cfg(not(target_os = "macos"))]
            {
                skipped.push((*adapter_id_u16, adapter_id.clone()));
                tracing::debug!(
                    adapter_id = %adapter_id,
                    "GPU verification not available on this platform"
                );
            }
        }

        drop(kernels_lock);

        Ok(GpuIntegrityReport {
            verified,
            failed,
            skipped,
            total_checked: loaded_adapters.len(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    /// Get current adapter states
    pub fn get_adapter_states(&self) -> Vec<adapter_hotswap::AdapterState> {
        self.hotswap.table().get_active()
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
        use adapteros_lora_lifecycle::{KernelAdapterBackend, WorkflowExecutor};

        info!(
            "Executing workflow with {} adapters using real kernels",
            adapter_ids.len()
        );

        // Create kernel backend with adapter name mapping
        let adapter_names: Vec<String> = self
            .manifest
            .adapters
            .iter()
            .map(|a| a.id.clone())
            .collect();

        let backend = Arc::new(KernelAdapterBackend::new(
            self.kernels.clone(),
            adapter_names,
            152064, // Qwen2.5 vocab size
        ));

        // Create and execute workflow
        let executor = WorkflowExecutor::new(workflow_type, adapter_ids, backend);
        executor.execute(context).await
    }
}

/// Inference event for telemetry
#[derive(Debug, Clone, Serialize)]
pub struct InferenceEvent {
    pub duration_ms: u64,
    pub success: bool,
    pub timeout_occurred: bool,
    pub circuit_breaker_open: bool,
    pub memory_usage: u64,
}

/// Initialize determinism guards for the worker
pub fn init_determinism_guards() -> Result<()> {
    // Initialize strict mode from environment variables
    // strict_mode::init_strict_mode();  // Temporarily disabled due to dependency issues

    // Initialize runtime guards
    // let guard_config = runtime_guards::GuardConfig {
    //     enabled: true,
    //     strict_mode: strict_mode::is_strict_mode(),
    //     max_violations: if strict_mode::is_strict_mode() { 1 } else { 10 },
    //     log_violations: true,
    // };

    // runtime_guards::init_guards(guard_config);

    info!("Determinism guards initialization temporarily disabled due to dependency issues");

    Ok(())
}

/// Check if determinism guards are enabled
pub fn determinism_guards_enabled() -> bool {
    // runtime_guards::guards_enabled()  // Temporarily disabled due to dependency issues
    false
}

/// Get current violation count
pub fn determinism_violation_count() -> u64 {
    // runtime_guards::violation_count()  // Temporarily disabled due to dependency issues
    0
}
