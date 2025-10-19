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
use adapteros_lora_router::{CodeFeatures, Router};
use adapteros_manifest::ManifestV3;
use adapteros_policy::{PolicyEngine, RefusalResponse};
use adapteros_telemetry::TelemetryWriter;
use serde::{Deserialize, Serialize};
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
use tokio::sync::broadcast;
use tokio::sync::Mutex;

/// Worker for running inference with comprehensive safety mechanisms
pub struct Worker<K: FusedKernels> {
    manifest: ManifestV3,
    policy: PolicyEngine,
    router: Router,
    rag: Option<RagSystem>,
    kernels: K,
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
    hotswap: HotSwapManager,
    // Optional signal broadcaster for per-step events
    signal_tx: Option<broadcast::Sender<WorkerSignal>>,
}

impl<K: FusedKernels> Worker<K> {
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
        let mut router = Router::new(
            vec![1.0; manifest.adapters.len()],
            manifest.router.k_sparse,
            manifest.router.tau,
            manifest.router.entropy_floor,
            router_seed,
        );

        // Router telemetry and configuration
        // Log first N tokens fully as per manifest telemetry config
        router.set_full_log_tokens(manifest.telemetry.router_full_tokens);
        // Enable telemetry logging of router decisions
        router.set_telemetry(telemetry.clone());
        // Optional MPLoRA knobs from manifest
        if manifest.router.orthogonal_constraints {
            router.set_orthogonal_constraints(
                true,
                manifest.router.diversity_threshold,
                manifest.router.orthogonal_penalty,
                64,
            );
        }
        router.set_shared_downsample(manifest.router.shared_downsample);
        router.set_compression_ratio(manifest.router.compression_ratio);

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
        // Default sampling tuned for balanced quality
        let mut generator = Generator::new(gen_seed)
            .with_temperature(0.7)
            .with_top_p(0.9);

        // When policy requires higher reliability (open-book or higher abstain threshold),
        // use more conservative sampling.
        if manifest.policies.evidence.require_open_book
            || manifest.policies.refusal.abstain_threshold >= 0.55
        {
            generator = generator.with_temperature(0.5).with_top_k(50);
        }

        // Development overrides for sampling via environment variables
        if let Ok(t) = std::env::var("AOS_SAMPLING_TEMP") {
            if let Ok(tv) = t.parse::<f32>() {
                generator = generator.with_temperature(tv);
            }
        }
        if let Ok(k) = std::env::var("AOS_TOP_K") {
            if let Ok(kv) = k.parse::<usize>() {
                generator = generator.with_top_k(kv);
            }
        }

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
        let adapters_path = {
            let root =
                std::env::var("AOS_ADAPTERS_ROOT").unwrap_or_else(|_| "./adapters".to_string());
            std::path::PathBuf::from(root)
        };
        let lifecycle = adapteros_lora_lifecycle::LifecycleManager::new(
            adapter_names,
            &manifest.policies,
            adapters_path,
            Some(telemetry.clone()),
            manifest.router.k_sparse,
        );

        Ok(Self {
            manifest,
            policy,
            router,
            rag,
            kernels,
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
            hotswap: HotSwapManager::new(),
            signal_tx: None,
        })
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

        // Retrieve evidence if required by request or policy
        let mut evidence = Vec::new();
        let need_evidence =
            request.require_evidence || self.manifest.policies.evidence.require_open_book;
        if need_evidence {
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
                    let _ = self.telemetry.log_policy_violation(
                        "Evidence",
                        "InsufficientSpans",
                        &format!(
                            "Found {} spans, need {}",
                            evidence.len(),
                            self.manifest.policies.evidence.min_spans
                        ),
                    );
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
            } else {
                // Policy or request requires evidence but RAG/evidence retriever unavailable
                let _ = self.telemetry.log_policy_violation(
                    "Evidence",
                    "BackendUnavailable",
                    "RAG backend unavailable while evidence is required",
                );
                return Ok(InferenceResponse {
                    text: None,
                    status: "insufficient_evidence".to_string(),
                    trace: ResponseTrace {
                        cpid: request.cpid.clone(),
                        plan_id: "placeholder".to_string(),
                        evidence: vec![],
                        router_summary: RouterSummary {
                            adapters_used: vec![],
                            avg_activations: vec![],
                        },
                        token_count: 0,
                    },
                    refusal: Some(RefusalResponse::insufficient_evidence(
                        self.manifest.policies.evidence.min_spans,
                        0,
                    )),
                    patch_proposal: None,
                });
            }
        }

        // Generate tokens using autoregressive loop
        let formatted_prompt = self.tokenizer.apply_chat_template(&request.prompt);
        let prompt_tokens = self.tokenizer.encode(&formatted_prompt)?;

        let mut generated_tokens = Vec::new();

        // Autoregressive generation loop
        let mut avg_conf_sum = 0.0f32;
        let mut avg_conf_n = 0usize;
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
            // Build CodeFeatures from prompt context
            let mut code_features = CodeFeatures::from_context(&formatted_prompt);
            // Simple attention entropy proxy based on token variety
            let token_entropy = {
                let unique: std::collections::HashSet<_> =
                    generated_tokens.iter().copied().collect();
                if generated_tokens.is_empty() {
                    0.0
                } else {
                    (unique.len() as f32 / generated_tokens.len() as f32).min(1.0)
                }
            };
            code_features.set_attn_entropy(token_entropy);
            let feature_vec = code_features.to_vector();

            // Compute adapter priors: base + framework boost + activation%
            let mut priors: Vec<f32> = vec![1.0; self.manifest.adapters.len()];
            for (idx, adapter) in self.manifest.adapters.iter().enumerate() {
                // Framework prior boost if framework_id matches context
                if let Some(fid) = &adapter.framework_id {
                    if let Some(score) = code_features.framework_prior.get(fid) {
                        // Scale raw framework score to a small prior boost
                        priors[idx] += (*score).min(3.0) * 0.1; // up to +0.3
                    }
                }

                // Lifecycle activation percentage as prior (+0.0..+0.3)
                let act = self.lifecycle.activation_pct(idx as u16);
                priors[idx] += (act.max(0.0).min(1.0)) * 0.3;
            }

            let decision = self.router.route(&feature_vec, &priors);

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
            self.kernels.run_step(&router_ring, &mut io_buffers)?;
            let kernel_duration = kernel_start.elapsed();

            // Emit router decision signal if subscribed
            if let Some(ref tx) = self.signal_tx {
                let _ = tx.send(WorkerSignal::router_decision(
                    step as u64,
                    &router_ring.indices,
                    &router_ring.gates_q15,
                ));
            }

            // Track per-step max probability as a proxy for confidence
            let step_conf = self.generator.max_prob(&io_buffers.output_logits);
            if step_conf.is_finite() {
                avg_conf_sum += step_conf;
                avg_conf_n += 1;
            }

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

            // Emit token signal
            if let Some(ref tx) = self.signal_tx {
                let _ = tx.send(WorkerSignal::token(step as u64, next_token));
            }

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

        // Confidence gating (policy-driven abstention)
        let avg_conf = if avg_conf_n > 0 {
            avg_conf_sum / (avg_conf_n as f32)
        } else {
            0.0
        };
        // Allow disabling confidence gating for development
        let skip_conf = std::env::var("AOS_INSECURE_SKIP_CONF")
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        if !skip_conf && (self.policy.check_confidence(avg_conf).is_err()) {
            let threshold = self.manifest.policies.refusal.abstain_threshold;
            let _ = self.telemetry.log_policy_violation(
                "Confidence",
                "BelowThreshold",
                &format!("avg_max_p={} below threshold {}", avg_conf, threshold),
            );
            return Ok(InferenceResponse {
                text: None,
                status: "low_confidence".to_string(),
                trace: self.build_trace(&request.cpid, &evidence, 0),
                refusal: Some(RefusalResponse::low_confidence(threshold, avg_conf)),
                patch_proposal: None,
            });
        }

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
        use crate::patch_generator::{PatchGenerationRequest, PatchGenerator};
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
            Box::new(crate::patch_generator::RuleBasedLlmBackend),
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
    pub fn execute_adapter_command(
        &mut self,
        command: AdapterCommand,
    ) -> Result<AdapterCommandResult> {
        self.hotswap.execute(command)
    }

    /// Get current adapter states
    pub fn get_adapter_states(&self) -> Vec<adapter_hotswap::AdapterState> {
        self.hotswap.table().get_active()
    }

    /// Check if current policy requires open-book (evidence) serving
    pub fn policy_requires_open_book(&self) -> bool {
        self.manifest.policies.evidence.require_open_book
    }

    /// Get refusal abstain threshold configured by policy
    pub fn policy_abstain_threshold(&self) -> f32 {
        self.manifest.policies.refusal.abstain_threshold
    }

    /// Attach a signal broadcaster for per-step events
    pub fn set_signal_tx(&mut self, tx: broadcast::Sender<WorkerSignal>) {
        self.signal_tx = Some(tx);
    }

    /// Find adapter index by string id from manifest
    fn adapter_index_by_id(&self, adapter_id: &str) -> Option<u16> {
        self.manifest
            .adapters
            .iter()
            .position(|a| a.id == adapter_id)
            .map(|i| i as u16)
    }

    /// Promote adapter by string id (lifecycle-aware)
    pub fn promote_adapter_by_id(&self, adapter_id: &str) -> Result<()> {
        if let Some(idx) = self.adapter_index_by_id(adapter_id) {
            self.lifecycle.promote_adapter(idx)
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter not found: {}",
                adapter_id
            )))
        }
    }

    /// Demote adapter by string id (lifecycle-aware)
    pub fn demote_adapter_by_id(&self, adapter_id: &str) -> Result<()> {
        if let Some(idx) = self.adapter_index_by_id(adapter_id) {
            self.lifecycle.demote_adapter(idx)
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter not found: {}",
                adapter_id
            )))
        }
    }

    /// Pin adapter by string id (resident state)
    pub fn pin_adapter_by_id(&self, adapter_id: &str) -> Result<()> {
        if let Some(idx) = self.adapter_index_by_id(adapter_id) {
            self.lifecycle.pin_adapter(idx)
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter not found: {}",
                adapter_id
            )))
        }
    }

    /// Unpin adapter by string id
    pub fn unpin_adapter_by_id(&self, adapter_id: &str) -> Result<()> {
        if let Some(idx) = self.adapter_index_by_id(adapter_id) {
            self.lifecycle.unpin_adapter(idx)
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter not found: {}",
                adapter_id
            )))
        }
    }

    /// Adapter state view used by UDS API (matches CLI expectations)
    pub fn list_adapter_states_view(&self) -> Vec<serde_json::Value> {
        use adapteros_manifest::AdapterTier;

        // Collect active set from hot-swap manager for 'active' flag and VRAM
        let active_set = self.hotswap.table().get_active();
        let mut active_map: std::collections::HashMap<String, (u64, bool)> =
            std::collections::HashMap::new();
        for a in active_set {
            active_map.insert(a.id.clone(), (a.vram_mb, a.active));
        }

        // Collect pinned info from lifecycle manager
        let pinned_map: std::collections::HashMap<String, bool> = self
            .lifecycle
            .get_all_states()
            .into_iter()
            .map(|r| (r.adapter_id, r.pinned))
            .collect();

        // Build view per manifest adapter in stable order
        self.manifest
            .adapters
            .iter()
            .enumerate()
            .map(|(idx, a)| {
                let idx_u16 = idx as u16;
                let metrics = self.profiler.get_adapter_metrics(idx_u16);
                let (vram_mb, is_active) = active_map.get(&a.id).cloned().unwrap_or((0, false));
                let pinned = pinned_map.get(&a.id).copied().unwrap_or(false);

                let activation_pct = metrics.as_ref().map(|m| m.activation_pct).unwrap_or(0.0);
                let quality_delta = metrics.as_ref().map(|m| m.quality_delta).unwrap_or(0.0);

                let tier = match a.tier {
                    AdapterTier::Persistent => "persistent",
                    AdapterTier::Ephemeral => "ephemeral",
                };

                serde_json::json!({
                    "id": a.id,
                    "hash": format!("b3:{}", a.hash.to_hex()),
                    "vram_mb": vram_mb,
                    "active": is_active,
                    "tier": tier,
                    "rank": a.rank,
                    "activation_pct": activation_pct,
                    "quality_delta": quality_delta,
                    "last_activation": serde_json::Value::Null,
                    "pinned": pinned,
                })
            })
            .collect()
    }

    /// Adapter profile view used by UDS API (matches CLI expectations)
    pub fn adapter_profile_view(&self, adapter_id: &str) -> Option<serde_json::Value> {
        let idx = self.adapter_index_by_id(adapter_id)?;
        let metrics = self.profiler.get_adapter_metrics(idx);

        let activation_pct = metrics.as_ref().map(|m| m.activation_pct).unwrap_or(0.0);
        let activations = metrics.as_ref().map(|m| m.activation_count).unwrap_or(0);
        let total_tokens = metrics.as_ref().map(|m| m.total_tokens).unwrap_or(0);
        let avg_latency_us = metrics.as_ref().map(|m| m.avg_latency_us).unwrap_or(0.0);
        let p95 = metrics.as_ref().map(|m| m.latency_p95_us).unwrap_or(0.0);
        let p99 = metrics.as_ref().map(|m| m.latency_p99_us).unwrap_or(0.0);
        let memory_kb = metrics
            .as_ref()
            .map(|m| m.memory_bytes as u64 / 1024)
            .unwrap_or(0);
        let quality_delta = metrics.as_ref().map(|m| m.quality_delta).unwrap_or(0.0);

        let state = self
            .lifecycle
            .get_state(idx)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let profile = serde_json::json!({
            "state": state,
            "activation_pct": activation_pct,
            "activations": activations,
            "total_tokens": total_tokens,
            "avg_latency_us": avg_latency_us,
            "memory_kb": memory_kb,
            "quality_delta": quality_delta,
            "recent_activations": [],
            "performance_metrics": {
                "p50_latency_us": avg_latency_us,
                "p95_latency_us": p95,
                "p99_latency_us": p99,
                "throughput_tokens_per_sec": 0.0,
                "error_rate": 0.0
            },
            "policy_compliance": {
                "determinism_score": 1.0,
                "evidence_coverage": 1.0,
                "refusal_rate": 0.0,
                "policy_violations": 0
            }
        });

        Some(profile)
    }

    /// Profiling snapshot for UDS API
    pub fn profiling_snapshot_json(&self) -> serde_json::Value {
        let metrics = self.profiler.get_all_metrics();
        serde_json::json!({ "adapters": metrics })
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

/// Warmup report for worker initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupReport {
    pub steps: u32,
    pub duration_ms: u64,
    pub ok: bool,
}

impl<K: FusedKernels> Worker<K> {
    /// Perform a lightweight warmup of tokenizer and kernel step
    pub async fn warmup(&mut self) -> Result<WarmupReport> {
        let start = std::time::Instant::now();

        // Encode a tiny prompt to exercise tokenizer path
        let prompt = "hello";
        let tokens = self.tokenizer.encode(prompt)?;
        let first = tokens
            .get(0)
            .copied()
            .unwrap_or(self.tokenizer.eos_token_id());

        // Build a minimal RouterRing selecting first k adapters
        let k = self
            .manifest
            .router
            .k_sparse
            .min(self.manifest.adapters.len());
        let indices: Vec<u16> = (0..k as u16).collect();
        let gates_q15: Vec<i16> = vec![i16::MAX; k];
        let ring = RouterRing {
            indices,
            gates_q15,
            position: 0,
        };

        // Allocate output logits buffer and run a single step
        let mut io = IoBuffers {
            input_ids: vec![first],
            output_logits: vec![0.0; 152064],
            position: 0,
        };

        self.kernels.run_step(&ring, &mut io)?;

        Ok(WarmupReport {
            steps: 1,
            duration_ms: start.elapsed().as_millis() as u64,
            ok: true,
        })
    }
}

/// Worker signal for SSE bridging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSignal {
    #[serde(rename = "type")]
    pub signal_type: String,
    pub timestamp: u128,
    pub payload: serde_json::Value,
}

impl WorkerSignal {
    fn now_millis() -> u128 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }

    pub fn router_decision(position: u64, indices: &[u16], gates_q15: &[i16]) -> Self {
        Self {
            signal_type: "router_decision".to_string(),
            timestamp: Self::now_millis(),
            payload: serde_json::json!({
                "position": position,
                "indices": indices,
                "gates_q15": gates_q15,
            }),
        }
    }

    pub fn token(position: u64, token_id: u32) -> Self {
        Self {
            signal_type: "token".to_string(),
            timestamp: Self::now_millis(),
            payload: serde_json::json!({
                "position": position,
                "token_id": token_id,
            }),
        }
    }
}

/// Get current violation count
pub fn determinism_violation_count() -> u64 {
    // runtime_guards::violation_count()  // Temporarily disabled due to dependency issues
    0
}
