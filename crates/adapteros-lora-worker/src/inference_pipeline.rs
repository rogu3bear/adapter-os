//! Base model inference pipeline for Qwen2.5-7B
//!
//! This module provides the complete inference pipeline integrating:
//! - Model loading and configuration
//! - Tokenization with chat templates
//! - K-sparse LoRA routing
//! - Autoregressive generation
//! - Evidence-grounded responses
//! - Policy enforcement
//! - Telemetry and tracing

use adapteros_config::ModelConfig;
use adapteros_core::{
    derive_seed, emit_observability_event, policy_override_event, AosError, B3Hash, CircuitBreaker,
    Result, StandardCircuitBreaker,
};
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers};
use adapteros_lora_router::{
    policy_mask::PolicyMask, AbstainContext, AdapterInfo, Router, ROUTER_GATE_Q15_MAX,
};
use adapteros_policy::{PolicyDecisionChain, PolicyEngine, QuarantineManager, QuarantineOperation};
use adapteros_telemetry::events::{
    AbstainEvent, PerformanceBudgetViolationEvent, RouterCandidate, RouterDecisionEvent,
};
use adapteros_telemetry::TelemetryWriter;
use smallvec::SmallVec;
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::task::yield_now;
use tracing::{debug, info, warn};

use crate::backend_factory::KernelBox;
use crate::generation::Generator;
use crate::reasoning_router::{
    FastEmbedder, ReasoningRouterConfig, ReasoningScorer, StreamInspector, ThoughtTransition,
};
use crate::router_bridge::decision_to_router_ring;
use crate::stop_controller::StopController;
use crate::tokenizer::QwenTokenizer;
use adapteros_lora_router::filter_decision_by_policy;

/// Configuration for inference pipeline
#[derive(Debug, Clone)]
pub struct InferencePipelineConfig {
    /// Model name
    pub model_name: String,
    /// Vocabulary size
    pub vocab_size: usize,
    /// Maximum sequence length
    pub max_seq_len: usize,
    /// Temperature for sampling
    pub temperature: f32,
    /// Top-k filtering
    pub top_k: Option<usize>,
    /// Top-p (nucleus) filtering
    pub top_p: Option<f32>,
    /// Manifest hash for HKDF seed derivation (determinism)
    pub manifest_hash: Option<B3Hash>,
    /// Optional allowlist of adapter IDs permitted for routing
    pub allowed_adapters: Option<Vec<String>>,
}

impl Default for InferencePipelineConfig {
    fn default() -> Self {
        let model_config = ModelConfig::from_env().unwrap_or_default();
        Self {
            model_name: model_config.architecture.clone(),
            vocab_size: model_config.vocab_size,
            max_seq_len: model_config.max_seq_len,
            temperature: 0.7,
            top_k: Some(50),
            top_p: Some(0.95),
            manifest_hash: None,
            allowed_adapters: None,
        }
    }
}

impl InferencePipelineConfig {
    /// Create from unified ModelConfig
    pub fn from_model_config(model_config: &ModelConfig) -> Self {
        Self {
            model_name: model_config.architecture.clone(),
            vocab_size: model_config.vocab_size,
            max_seq_len: model_config.max_seq_len,
            temperature: 0.7,
            top_k: Some(50),
            top_p: Some(0.95),
            manifest_hash: None,
            allowed_adapters: None,
        }
    }

    /// Create from unified ModelConfig with sampling parameters
    pub fn from_model_config_with_sampling(
        model_config: &ModelConfig,
        temperature: f32,
        top_k: Option<usize>,
        top_p: Option<f32>,
    ) -> Self {
        Self {
            model_name: model_config.architecture.clone(),
            vocab_size: model_config.vocab_size,
            max_seq_len: model_config.max_seq_len,
            temperature,
            top_k,
            top_p,
            manifest_hash: None,
            allowed_adapters: None,
        }
    }

    /// Set the manifest hash for deterministic HKDF seed derivation
    ///
    /// This should be called with the manifest's hash to ensure reproducible
    /// inference across runs. Without this, a warning is logged and a default
    /// seed is used.
    pub fn with_manifest_hash(mut self, hash: B3Hash) -> Self {
        self.manifest_hash = Some(hash);
        self
    }
}

/// Backend-fixed quantization description
#[derive(Debug, Clone, Copy)]
pub enum BackendQuantization {
    /// Backend enforces fp16/bf16 kernels (Metal/CoreML)
    BackendFixedFp16Bf16,
    /// Backend uses model/manifest-provided quantized weights (MLX int4/int8/fp16)
    BackendUsesModelQuantization,
    /// Backend fallback when unknown (treated as backend-fixed)
    Unknown,
}

impl BackendQuantization {
    fn from_backend_type(backend: BackendType) -> Self {
        match backend {
            BackendType::Metal | BackendType::CoreML => BackendQuantization::BackendFixedFp16Bf16,
            BackendType::MLX => BackendQuantization::BackendUsesModelQuantization,
            _ => BackendQuantization::Unknown,
        }
    }
}

impl std::fmt::Display for BackendQuantization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendQuantization::BackendFixedFp16Bf16 => {
                write!(f, "fp16/bf16 (backend-fixed)")
            }
            BackendQuantization::BackendUsesModelQuantization => {
                write!(f, "model/manifest quantization (e.g., int4/int8/fp16)")
            }
            BackendQuantization::Unknown => write!(f, "backend-fixed (unspecified)"),
        }
    }
}

/// Inference request (re-export unified request type used across the worker).
pub type InferenceRequest = crate::request_types::InferenceRequest;

/// Inference response with trace
#[derive(Debug, Clone)]
pub struct InferenceResponse {
    /// Generated text
    pub text: String,
    /// Token count
    pub token_count: usize,
    /// Inference latency
    pub latency_ms: u64,
    /// Trace for reproducibility
    pub trace: InferenceTrace,
    /// Optional per-token adapter coloring for Rainbow Cache tracing
    pub rainbow_trace: Option<Vec<Option<String>>>,
    /// Reasoning transitions observed during streaming
    pub reasoning_transitions: Option<Vec<ThoughtTransition>>,
    /// Stack ID for telemetry correlation (PRD-03)
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation (PRD-03)
    pub stack_version: Option<i64>,
    // Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
    /// Stop reason code explaining why generation terminated
    pub stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
    /// Token index at which the stop decision was made
    pub stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used
    pub stop_policy_digest_b3: Option<adapteros_core::B3Hash>,
}

/// Streaming token emitted by generate_stream with optional reasoning metadata.
#[derive(Debug, Clone)]
pub struct StreamToken {
    pub token_id: u32,
    pub token_text: String,
    pub adapter_color: Option<String>,
    pub transition: Option<ThoughtTransition>,
}

/// Trace information for reproducible inference
#[derive(Debug, Clone)]
pub struct InferenceTrace {
    /// Control plane ID
    pub cpid: String,
    /// Input tokens
    pub input_tokens: Vec<u32>,
    /// Generated tokens
    pub generated_tokens: Vec<u32>,
    /// Router decisions per step
    pub router_decisions: Vec<RouterDecision>,
    /// Evidence used (if RAG enabled)
    pub evidence: Vec<String>,
}

pub type RouterDecision = RouterDecisionEvent;

/// Performance budget tracker for inference pipeline
struct BudgetTracker {
    /// Rolling window of latency samples (microseconds)
    latency_samples: VecDeque<u64>,
    /// Maximum samples to keep (for p95 calculation)
    max_samples: usize,
    /// Router duration in microseconds (per request)
    router_duration_us: u64,
    /// Total inference duration in microseconds (per request)
    total_inference_us: u64,
}

impl BudgetTracker {
    /// Create a new budget tracker with rolling window of 20 samples
    fn new() -> Self {
        Self {
            latency_samples: VecDeque::new(),
            max_samples: 20,
            router_duration_us: 0,
            total_inference_us: 0,
        }
    }

    /// Record a kernel latency sample
    fn record_latency(&mut self, latency_us: u64) {
        self.latency_samples.push_back(latency_us);
        if self.latency_samples.len() > self.max_samples {
            self.latency_samples.pop_front();
        }
    }

    /// Calculate p95 latency in milliseconds
    fn p95_latency_ms(&self) -> Option<f64> {
        if self.latency_samples.is_empty() {
            return None;
        }

        let mut sorted: Vec<u64> = self.latency_samples.iter().copied().collect();
        sorted.sort_unstable();

        let idx = ((sorted.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
        Some(sorted[idx] as f64 / 1000.0) // Convert to milliseconds
    }

    /// Record router and total inference times
    fn record_router_timing(&mut self, router_us: u64, total_us: u64) {
        self.router_duration_us = router_us;
        self.total_inference_us = total_us;
    }

    /// Calculate router overhead percentage
    fn router_overhead_pct(&self) -> Option<f64> {
        if self.total_inference_us == 0 {
            return None;
        }
        Some((self.router_duration_us as f64 / self.total_inference_us as f64) * 100.0)
    }
}

/// Base model inference pipeline
pub struct InferencePipeline {
    /// Tokenizer
    tokenizer: QwenTokenizer,
    /// Text generator
    generator: Generator,
    /// Router for K-sparse selection
    router: Router,
    /// Kernel backend
    kernels: KernelBox,
    /// Backend type (for logging/telemetry)
    #[allow(dead_code)]
    backend_type: BackendType,
    /// Fixed backend quantization (no per-token overrides)
    #[allow(dead_code)]
    backend_quantization: BackendQuantization,
    /// Policy engine (canonical policy packs)
    policy: PolicyEngine,
    /// Telemetry writer
    telemetry: TelemetryWriter,
    /// Configuration
    config: InferencePipelineConfig,
    /// Quarantine manager for policy hash enforcement
    quarantine_manager: Arc<Mutex<QuarantineManager>>,
    /// Circuit breaker for inference stability
    circuit_breaker: Arc<StandardCircuitBreaker>,
    /// Maximum adapter count for router bridge bounds checking
    max_adapter_count: u16,
    /// Performance budget tracker
    budget_tracker: BudgetTracker,
}

impl InferencePipeline {
    /// Default maximum adapter count for bounds checking
    const DEFAULT_MAX_ADAPTER_COUNT: u16 = 256;

    /// Create new inference pipeline
    pub fn new(
        tokenizer_path: &Path,
        router: Router,
        kernels: KernelBox,
        policy: PolicyEngine,
        telemetry: TelemetryWriter,
        config: InferencePipelineConfig,
        circuit_breaker: Arc<StandardCircuitBreaker>,
    ) -> Result<Self> {
        Self::with_adapter_count(
            tokenizer_path,
            router,
            kernels,
            policy,
            telemetry,
            config,
            circuit_breaker,
            Self::DEFAULT_MAX_ADAPTER_COUNT,
        )
    }

    /// Create new inference pipeline with explicit adapter count
    #[allow(clippy::too_many_arguments)]
    pub fn with_adapter_count(
        tokenizer_path: &Path,
        router: Router,
        kernels: KernelBox,
        policy: PolicyEngine,
        telemetry: TelemetryWriter,
        config: InferencePipelineConfig,
        circuit_breaker: Arc<StandardCircuitBreaker>,
        max_adapter_count: u16,
    ) -> Result<Self> {
        // Validate backend determinism before constructing pipeline
        let report = kernels.attest_determinism()?;
        policy.validate_backend_attestation(&report)?;

        let backend_quantization = BackendQuantization::from_backend_type(report.backend_type);

        info!(
            backend = ?report.backend_type,
            backend_type = ?report.backend_type,
            backend_quantization = %backend_quantization,
            "Backend determinism validated: {}",
            report.summary()
        );

        let mut router = router;
        router.set_abstain_telemetry_writer(Arc::new(telemetry.clone()));

        let mut router = router;
        router.set_abstain_telemetry_writer(Arc::new(telemetry.clone()));

        let tokenizer = QwenTokenizer::from_file(tokenizer_path)?;

        // Create deterministic generator with HKDF-derived seed
        let seed = if let Some(ref manifest_hash) = config.manifest_hash {
            derive_seed(manifest_hash, "inference_generator")
        } else {
            // Fallback to default seed if no manifest hash provided
            warn!("No manifest hash provided for HKDF seeding, using default seed");
            derive_seed(&B3Hash::hash(b"default_manifest"), "inference_generator")
        };

        let generator = Generator::new(seed)
            .with_temperature(config.temperature)
            .with_top_k(config.top_k.unwrap_or(50))
            .with_top_p(config.top_p.unwrap_or(0.9));

        // Initialize quarantine manager
        let quarantine_manager = Arc::new(Mutex::new(QuarantineManager::new()));

        Ok(Self {
            tokenizer,
            generator,
            router,
            kernels,
            backend_type: report.backend_type,
            backend_quantization,
            policy,
            telemetry,
            config,
            quarantine_manager,
            circuit_breaker,
            max_adapter_count,
            budget_tracker: BudgetTracker::new(),
        })
    }

    fn filter_adapters(
        &self,
        adapter_info: &[AdapterInfo],
        priors: &[f32],
    ) -> Result<(Vec<AdapterInfo>, Vec<f32>)> {
        apply_allowlist(self.config.allowed_adapters.as_ref(), adapter_info, priors)
    }

    /// Create new inference pipeline with quarantine manager
    /// This allows external initialization of the quarantine state
    #[allow(clippy::too_many_arguments)]
    pub fn with_quarantine(
        tokenizer_path: &Path,
        router: Router,
        kernels: KernelBox,
        policy: PolicyEngine,
        telemetry: TelemetryWriter,
        config: InferencePipelineConfig,
        quarantine_manager: Arc<Mutex<QuarantineManager>>,
        circuit_breaker: Arc<StandardCircuitBreaker>,
    ) -> Result<Self> {
        // Validate backend determinism before constructing pipeline
        let report = kernels.attest_determinism()?;
        policy.validate_backend_attestation(&report)?;

        let backend_quantization = BackendQuantization::from_backend_type(report.backend_type);

        info!(
            backend = ?report.backend_type,
            backend_type = ?report.backend_type,
            backend_quantization = %backend_quantization,
            "Backend determinism validated: {}",
            report.summary()
        );

        let tokenizer = QwenTokenizer::from_file(tokenizer_path)?;

        // Create deterministic generator with HKDF-derived seed
        let seed = if let Some(ref manifest_hash) = config.manifest_hash {
            derive_seed(manifest_hash, "inference_generator")
        } else {
            // Fallback to default seed if no manifest hash provided
            warn!("No manifest hash provided for HKDF seeding, using default seed");
            derive_seed(&B3Hash::hash(b"default_manifest"), "inference_generator")
        };

        let generator = Generator::new(seed)
            .with_temperature(config.temperature)
            .with_top_k(config.top_k.unwrap_or(50))
            .with_top_p(config.top_p.unwrap_or(0.9));

        Ok(Self {
            tokenizer,
            generator,
            router,
            kernels,
            backend_type: report.backend_type,
            backend_quantization,
            policy,
            telemetry,
            config,
            quarantine_manager,
            circuit_breaker,
            max_adapter_count: Self::DEFAULT_MAX_ADAPTER_COUNT,
            budget_tracker: BudgetTracker::new(),
        })
    }

    fn policy_metadata(
        &self,
        request: &InferenceRequest,
        prompt_hash: &B3Hash,
        stage: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "stage": stage,
            "prompt_chars": request.prompt.len(),
            "prompt_hash_b3": prompt_hash.to_hex(),
            "max_tokens": request.max_tokens,
            "stack_id": request.stack_id,
            "stack_version": request.stack_version,
            "require_evidence": request.require_evidence,
            "routing_policy_present": request.routing_policy.is_some(),
            "stop_policy_present": request.stop_policy.is_some(),
            "runtime_mode": std::env::var("AOS_RUNTIME_MODE").unwrap_or_else(|_| "prod".to_string()),
        })
    }

    fn evaluate_policies_or_fail(
        &self,
        request: &InferenceRequest,
        prompt_hash: &B3Hash,
        stage: &str,
    ) -> Result<PolicyDecisionChain> {
        let metadata = self.policy_metadata(request, prompt_hash, stage);
        let chain = self
            .policy
            .evaluate_inference_policies(&request.cpid, metadata)?;
        self.log_policy_decision_chain(request, stage, &chain);

        if !chain.validation.valid {
            let reason = chain
                .validation
                .violations
                .iter()
                .map(|v| v.message.clone())
                .collect::<Vec<_>>()
                .join("; ");
            self.emit_policy_violation_event(request, stage, &reason);
            return Err(AosError::PolicyViolation(reason));
        }

        Ok(chain)
    }

    fn log_policy_decision_chain(
        &self,
        request: &InferenceRequest,
        stage: &str,
        chain: &PolicyDecisionChain,
    ) {
        if let Err(e) = self.telemetry.log(
            "policy.decision_chain",
            serde_json::json!({
                "cpid": request.cpid,
                "stage": stage,
                "digest_b3": chain.digest.to_hex(),
                "valid": chain.validation.valid,
                "violations": chain.validation.violations,
                "warnings": chain.validation.warnings,
                "decisions": chain.decisions,
            }),
        ) {
            debug!(
                target: "telemetry",
                error = %e,
                "Telemetry emit failed (non-fatal)"
            );
        }
    }

    fn emit_policy_violation_event(&self, request: &InferenceRequest, stage: &str, reason: &str) {
        let event = policy_override_event(
            Some(stage.to_string()),
            Some("policy_engine".to_string()),
            reason.to_string(),
            request.stack_id.clone(),
            Some(request.cpid.clone()),
        );
        emit_observability_event(&event);
        if let Err(e) = self.telemetry.log(
            "policy.violation",
            serde_json::json!({
                "cpid": request.cpid,
                "stage": stage,
                "reason": reason,
                "stack_id": request.stack_id,
                "stack_version": request.stack_version,
            }),
        ) {
            debug!(
                target: "telemetry",
                error = %e,
                "Telemetry emit failed (non-fatal)"
            );
        }
    }

    fn apply_abstain_context(&mut self, request: &InferenceRequest, prompt_hash: &B3Hash) {
        let ctx = AbstainContext {
            request_id: Some(request.cpid.clone()),
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
            prompt_digest_b3: Some(prompt_hash.to_hex()),
            prompt_chars: Some(request.prompt.len()),
            prompt: None,
            tenant_id: None,
        };
        self.router.set_abstain_context(ctx);
    }

    fn handle_abstain_events(
        &self,
        request: &InferenceRequest,
        stage: &str,
        step: usize,
        events: &[AbstainEvent],
    ) -> AosError {
        let msg = "Router abstained due to policy thresholds";
        self.emit_policy_violation_event(request, stage, msg);
        if let Err(e) = self.telemetry.log(
            "policy.abstain",
            serde_json::json!({
                "cpid": request.cpid,
                "stage": stage,
                "step": step,
                "events": events,
            }),
        ) {
            debug!(
                target: "telemetry",
                error = %e,
                "Telemetry emit failed (non-fatal)"
            );
        }
        AosError::PolicyViolation(msg.to_string())
    }

    /// Run inference on a prompt with circuit breaker protection
    pub async fn infer(&mut self, request: InferenceRequest) -> Result<InferenceResponse> {
        // Use circuit breaker with timeout protection
        let _timeout_duration = Duration::from_secs(30); // 30 second timeout for inference

        let start_time = Instant::now();

        // Check quarantine before serving (Determinism Ruleset #2)
        {
            let quarantine = self.quarantine_manager.lock().await;
            quarantine.check_operation(QuarantineOperation::Inference)?;
        }

        info!(
            "Starting inference: prompt_len={}, max_tokens={}",
            request.prompt.len(),
            request.max_tokens
        );

        // Run inference directly without circuit breaker call wrapper
        // Circuit breaker state is checked via state() method
        if matches!(
            self.circuit_breaker.state(),
            adapteros_core::CircuitState::Open { .. }
        ) {
            return Err(AosError::Worker("Circuit breaker is open".to_string()));
        }

        self.infer_inner(request, start_time).await
    }

    /// Streaming inference with reasoning-aware adapter hot-swaps.
    pub async fn generate_stream<F>(
        &mut self,
        request: InferenceRequest,
        on_token: F,
        reasoning_config: Option<ReasoningRouterConfig>,
    ) -> Result<InferenceResponse>
    where
        F: FnMut(StreamToken) -> Result<()>,
    {
        let start_time = Instant::now();

        {
            let quarantine = self.quarantine_manager.lock().await;
            quarantine.check_operation(QuarantineOperation::Inference)?;
        }

        if matches!(
            self.circuit_breaker.state(),
            adapteros_core::CircuitState::Open { .. }
        ) {
            return Err(AosError::Worker("Circuit breaker is open".to_string()));
        }

        self.generate_stream_inner(
            request,
            start_time,
            on_token,
            reasoning_config.unwrap_or_default(),
        )
        .await
    }

    async fn generate_stream_inner<F>(
        &mut self,
        request: InferenceRequest,
        start_time: Instant,
        mut on_token: F,
        reasoning_config: ReasoningRouterConfig,
    ) -> Result<InferenceResponse>
    where
        F: FnMut(StreamToken) -> Result<()>,
    {
        // 0. Enforce routing policy preconditions deterministically before work
        if let Some(policy) = &request.routing_policy {
            if policy.require_stack && request.stack_id.is_none() {
                return Err(AosError::PolicyViolation(
                    "Routing policy requires stack_id on request".to_string(),
                ));
            }

            if let Some(allowed_stacks) = &policy.allowed_stack_ids {
                let stack = request.stack_id.as_ref().ok_or_else(|| {
                    AosError::PolicyViolation(
                        "Routing policy requires stack_id for stack allowlist".to_string(),
                    )
                })?;
                if !allowed_stacks.contains(stack) {
                    return Err(AosError::PolicyViolation(format!(
                        "Routing policy denied stack '{}'",
                        stack
                    )));
                }
            }

            if policy.require_pins {
                return Err(AosError::PolicyViolation(
                    "Routing policy requires pinned adapters; none provided".to_string(),
                ));
            }
        }

        let prompt_hash = B3Hash::hash(request.prompt.as_bytes());
        self.apply_abstain_context(&request, &prompt_hash);
        let _ = self.evaluate_policies_or_fail(&request, &prompt_hash, "pre_inference_stream")?;

        // 1. Apply chat template and tokenize
        let formatted_prompt = self.tokenizer.apply_chat_template(&request.prompt);
        let input_tokens = self.tokenizer.encode(&formatted_prompt)?;

        debug!(
            "Tokenized prompt: {} tokens (streaming)",
            input_tokens.len()
        );

        // 2. Validate sequence length
        if input_tokens.len() > self.config.max_seq_len {
            return Err(AosError::Worker(format!(
                "Input too long: {} tokens exceeds max {}",
                input_tokens.len(),
                self.config.max_seq_len
            )));
        }

        // 3. Initialize generation state
        let mut generated_tokens = Vec::new();
        let mut router_decisions = Vec::new();
        let mut current_tokens = input_tokens.clone();
        let mut total_router_time_us = 0u64;

        // 3.5 Initialize stop controller
        let stop_sequences_tokens = match request.stop_policy.as_ref() {
            Some(policy) => {
                let mut sequences = Vec::new();
                for sequence in &policy.stop_sequences {
                    if sequence.is_empty() {
                        continue;
                    }
                    let tokens = self.tokenizer.encode(sequence)?;
                    if tokens.is_empty() {
                        continue;
                    }
                    sequences.push(tokens);
                }
                sequences
            }
            None => Vec::new(),
        };
        let mut stop_controller = StopController::from_policy_or_default_with_stop_sequences(
            request.stop_policy.clone(),
            request.max_tokens as u32,
            stop_sequences_tokens,
        );
        let stop_policy_digest = *stop_controller.policy_digest();
        let mut stop_reason_code = None;
        let mut stop_reason_token_index = None;
        let admin_override = request.admin_override;
        let max_reasoning_depth = request
            .routing_policy
            .as_ref()
            .and_then(|p| p.max_reasoning_depth)
            .unwrap_or(usize::MAX);
        let cluster_fallback = request
            .routing_policy
            .as_ref()
            .map(|p| p.cluster_fallback.as_str())
            .unwrap_or("stay_on_current");
        let mut transition_count: usize = 0;
        let mut previous_decision: Option<adapteros_lora_router::Decision> = None;

        // Build adapter set once for streaming loop
        let base_adapter_info: Vec<AdapterInfo> = (0..8)
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![0], // Default language
                tier: "persistent".to_string(),
                ..Default::default()
            })
            .collect();
        let base_priors = vec![1.0; base_adapter_info.len()];
        let (adapter_info, base_priors) = self.filter_adapters(&base_adapter_info, &base_priors)?;
        let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
        let adapter_clusters: Vec<Option<String>> = adapter_info
            .iter()
            .map(|a| derive_cluster_from_id(&a.id))
            .collect();

        let policy_digest_seed = request
            .routing_policy
            .as_ref()
            .and_then(|policy| serde_json::to_vec(policy).ok())
            .map(|bytes| B3Hash::hash(&bytes));
        let policy_mask = PolicyMask::build(
            &adapter_ids,
            request
                .routing_policy
                .as_ref()
                .and_then(|p| p.allowed_adapter_ids.as_deref()),
            request
                .routing_policy
                .as_ref()
                .and_then(|p| p.denied_adapter_ids.as_deref()),
            None,
            None,
            policy_digest_seed,
        );

        // Reasoning router setup
        let embedder = FastEmbedder::default_quantized();
        let scorer = ReasoningScorer::from_adapter_ids(&adapter_ids, &embedder);
        let initial_cluster = adapter_ids
            .first()
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        let mut inspector =
            StreamInspector::new(initial_cluster, scorer, embedder, reasoning_config);
        let mut adapter_bias: Option<String> = None;
        let mut rainbow_trace: Vec<Option<String>> = Vec::new();
        let mut reasoning_transitions: Vec<ThoughtTransition> = Vec::new();

        info!(
            "Starting streaming inference: prompt_len={}, max_tokens={}",
            request.prompt.len(),
            request.max_tokens
        );

        // 4. Autoregressive generation loop
        for step in 0..request.max_tokens {
            let input_ids = if step == 0 {
                &current_tokens[..]
            } else {
                let last_token = generated_tokens.last().ok_or_else(|| {
                    AosError::Worker("Generated tokens cannot be empty".to_string())
                })?;
                std::slice::from_ref(last_token)
            };
            let input_token_id = input_ids.last().copied();

            // 5. Router decision
            let features = self.create_feature_vector(&current_tokens);
            let mut priors = base_priors.clone();
            if let Some(ref target) = adapter_bias {
                for (idx, info) in adapter_info.iter().enumerate() {
                    if info.id == *target {
                        priors[idx] = (priors[idx] * 1.5).max(1.0);
                    } else {
                        priors[idx] *= 0.95;
                    }
                }
            }

            let router_start = Instant::now();
            let decision = self.router.route_with_adapter_info(
                &features,
                &priors,
                &adapter_info,
                &policy_mask,
            )?;

            let mut decision = match enforce_routing_policy_on_decision(
                decision,
                &adapter_info,
                &adapter_clusters,
                request.routing_policy.as_ref(),
            ) {
                Ok(decision) => decision,
                Err(err) => {
                    if admin_override {
                        // Admin override: allow by sticking with previous or first adapter
                        self.log_blocked_transition(
                            &request,
                            step,
                            "cluster_denied_admin_override",
                            cluster_fallback,
                        );
                        fallback_decision(
                            previous_decision.as_ref(),
                            adapter_info.len(),
                            cluster_fallback,
                        )
                    } else if request.routing_policy.is_some() {
                        self.log_blocked_transition(
                            &request,
                            step,
                            "cluster_denied",
                            cluster_fallback,
                        );
                        fallback_decision(
                            previous_decision.as_ref(),
                            adapter_info.len(),
                            cluster_fallback,
                        )
                    } else {
                        return Err(err);
                    }
                }
            };

            let abstain_events = self.router.take_abstain_events();
            if !abstain_events.is_empty() {
                return Err(self.handle_abstain_events(
                    &request,
                    "pre_kernel_stream",
                    step,
                    &abstain_events,
                ));
            }

            if let Some(prev) = &previous_decision {
                if prev.indices.as_slice() != decision.indices.as_slice() {
                    transition_count = transition_count.saturating_add(1);
                }
            }

            if !admin_override && transition_count > max_reasoning_depth {
                self.log_blocked_transition(
                    &request,
                    step,
                    "max_reasoning_depth",
                    cluster_fallback,
                );
                decision = fallback_decision(
                    previous_decision.as_ref(),
                    adapter_info.len(),
                    cluster_fallback,
                );
                transition_count = max_reasoning_depth;
            }

            previous_decision = Some(decision.clone());
            let router_latency = router_start.elapsed();
            total_router_time_us += router_latency.as_micros() as u64;

            let router_event = RouterDecisionEvent {
                step,
                input_token_id,
                candidate_adapters: decision
                    .candidates
                    .iter()
                    .map(|c| RouterCandidate {
                        adapter_idx: c.adapter_idx,
                        raw_score: c.raw_score,
                        gate_q15: c.gate_q15,
                    })
                    .collect(),
                entropy: decision.entropy,
                tau: self.router.tau(),
                entropy_floor: self.router.eps(),
                stack_hash: self.router.stack_hash(),
                stack_id: request.stack_id.clone(),
                stack_version: request.stack_version,
                model_type: adapteros_types::routing::RouterModelType::Dense,
                active_experts: None,
            };
            if let Err(e) = self.telemetry.log_router_decision(router_event) {
                debug!(
                    target: "telemetry",
                    error = %e,
                    "Telemetry emit failed (non-fatal)"
                );
            }

            let _entropy = self.calculate_gate_entropy(&decision.gates_q15);

            // 7. Execute kernel inference
            let mut io_buffers = IoBuffers {
                input_ids: input_ids.to_vec(),
                output_logits: vec![0.0; self.config.vocab_size],
                position: current_tokens.len() - 1,
            };
            let mut router_ring = decision_to_router_ring(&decision, self.max_adapter_count)?;
            router_ring.position = step;

            let kernel_start = Instant::now();
            self.kernels.run_step(&router_ring, &mut io_buffers)?;
            let kernel_latency = kernel_start.elapsed();

            self.budget_tracker
                .record_latency(kernel_latency.as_micros() as u64);
            if let Some(p95_ms) = self.budget_tracker.p95_latency_ms() {
                if p95_ms > 24.0 {
                    let violation = PerformanceBudgetViolationEvent::p95_latency(p95_ms, None);
                    if let Err(e) = self.telemetry.log_budget_violation(violation) {
                        warn!(error = %e, p95_ms = p95_ms, "Failed to log P95 latency violation");
                    }
                }
            }

            // 8. Sample next token
            let next_token = self.generator.next_token(&io_buffers.output_logits)?;

            // 9. Record telemetry (sampled)
            if step < 128 || (step % 20 == 0) {
                if let Err(e) = self.telemetry.log(
                    "inference.step",
                    serde_json::json!({
                        "cpid": request.cpid,
                        "step": step,
                        "token": next_token,
                        "kernel_latency_us": kernel_latency.as_micros(),
                        "adapters": decision.indices.to_vec(),
                    }),
                ) {
                    debug!(
                        target: "telemetry",
                        error = %e,
                        "Telemetry emit failed (non-fatal)"
                    );
                }
            }

            // 10. Record canonical router decision
            let candidate_adapters: Vec<RouterCandidate> = decision
                .candidates
                .iter()
                .map(|candidate| RouterCandidate {
                    adapter_idx: candidate.adapter_idx,
                    raw_score: candidate.raw_score,
                    gate_q15: candidate.gate_q15,
                })
                .collect();

            let event = RouterDecisionEvent {
                step,
                input_token_id,
                candidate_adapters,
                entropy: decision.entropy,
                tau: self.router.temperature(),
                entropy_floor: self.router.entropy_floor(),
                stack_hash: self.router.stack_hash(),
                stack_id: request.stack_id.clone(),
                stack_version: request.stack_version,
                model_type: adapteros_types::routing::RouterModelType::Dense,
                active_experts: None,
            };

            if let Err(err) = self.telemetry.log_router_decision(event.clone()) {
                warn!("Failed to log router decision: {}", err);
            }

            router_decisions.push(event);

            // 11. Check stopping criteria
            if let Some(decision) = stop_controller.check_stop(
                next_token,
                self.tokenizer.eos_token_id(),
                &io_buffers.output_logits,
            ) {
                stop_reason_code = Some(decision.reason);
                stop_reason_token_index = Some(decision.token_index);
                if decision.trim_tokens > 0 {
                    let trim = decision.trim_tokens.min(generated_tokens.len());
                    for _ in 0..trim {
                        generated_tokens.pop();
                    }
                }
                debug!(
                    step,
                    reason = %decision.reason,
                    token_index = decision.token_index,
                    "Stop controller triggered"
                );
                break;
            }

            // 12. Append token and continue
            generated_tokens.push(next_token);
            current_tokens.push(next_token);

            let token_text = self.tokenizer.decode(std::slice::from_ref(&next_token))?;
            let transition = inspector.on_token(&token_text, step);
            if let Some(ref decision) = transition {
                reasoning_transitions.push(decision.transition.clone());
                if !decision.shadow_mode {
                    info!(
                        from = %decision.transition.from,
                        to = %decision.transition.to,
                        confidence = decision.transition.confidence,
                        "HotSwap interrupt triggered by reasoning router"
                    );
                    adapter_bias = Some(decision.transition.to.clone());
                } else {
                    debug!(
                        from = %decision.transition.from,
                        to = %decision.transition.to,
                        "Shadow mode enabled: logging transition without swap"
                    );
                }
            }
            let adapter_color = adapter_bias.clone();
            rainbow_trace.push(adapter_color.clone());
            let transition_for_callback = transition.as_ref().map(|d| d.transition.clone());
            on_token(StreamToken {
                token_id: next_token,
                token_text: token_text.clone(),
                adapter_color,
                transition: transition_for_callback,
            })?;
            if let Some(decision) = transition {
                if !decision.shadow_mode {
                    yield_now().await;
                }
            }

            // Check max sequence length (fallback for very long sequences)
            if current_tokens.len() >= self.config.max_seq_len {
                warn!("Reached maximum sequence length");
                stop_reason_code = Some(adapteros_api_types::inference::StopReasonCode::BudgetMax);
                stop_reason_token_index = Some(step as u32);
                break;
            }
        }

        // 13. Decode generated text
        let generated_text = self.tokenizer.decode(&generated_tokens)?;

        // 14. Build trace for reproducibility
        let trace = InferenceTrace {
            cpid: request.cpid.clone(),
            input_tokens: input_tokens.clone(),
            generated_tokens: generated_tokens.clone(),
            router_decisions,
            evidence: vec![],
        };

        let latency = start_time.elapsed();

        // Check router overhead budget (8% threshold)
        let total_inference_us = latency.as_micros() as u64;
        self.budget_tracker
            .record_router_timing(total_router_time_us, total_inference_us);
        if let Some(overhead_pct) = self.budget_tracker.router_overhead_pct() {
            if overhead_pct > 8.0 {
                let violation = PerformanceBudgetViolationEvent::router_overhead(overhead_pct);
                if let Err(e) = self.telemetry.log_budget_violation(violation) {
                    warn!(error = %e, overhead_pct = overhead_pct, "Failed to log router overhead violation");
                }
            }
        }

        if let Err(e) = self.telemetry.log(
            "inference.complete",
            serde_json::json!({
                "cpid": request.cpid,
                "input_tokens": input_tokens.len(),
                "generated_tokens": generated_tokens.len(),
                "latency_ms": latency.as_millis(),
            }),
        ) {
            debug!(
                target: "telemetry",
                error = %e,
                "Telemetry emit failed (non-fatal)"
            );
        }

        info!(
            "Streaming inference complete: generated {} tokens in {}ms",
            generated_tokens.len(),
            latency.as_millis()
        );

        self.router.clear_abstain_context();

        Ok(InferenceResponse {
            text: generated_text,
            token_count: generated_tokens.len(),
            latency_ms: latency.as_millis() as u64,
            trace,
            rainbow_trace: Some(rainbow_trace),
            reasoning_transitions: if reasoning_transitions.is_empty() {
                None
            } else {
                Some(reasoning_transitions)
            },
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
            stop_reason_code,
            stop_reason_token_index,
            stop_policy_digest_b3: Some(stop_policy_digest),
        })
    }

    /// Internal inference implementation without circuit breaker
    async fn infer_inner(
        &mut self,
        request: InferenceRequest,
        start_time: Instant,
    ) -> Result<InferenceResponse> {
        // 0. Enforce routing policy preconditions deterministically before work
        if let Some(policy) = &request.routing_policy {
            if policy.require_stack && request.stack_id.is_none() {
                return Err(AosError::PolicyViolation(
                    "Routing policy requires stack_id on request".to_string(),
                ));
            }

            if let Some(allowed_stacks) = &policy.allowed_stack_ids {
                let stack = request.stack_id.as_ref().ok_or_else(|| {
                    AosError::PolicyViolation(
                        "Routing policy requires stack_id for stack allowlist".to_string(),
                    )
                })?;
                if !allowed_stacks.contains(stack) {
                    return Err(AosError::PolicyViolation(format!(
                        "Routing policy denied stack '{}'",
                        stack
                    )));
                }
            }

            if policy.require_pins {
                return Err(AosError::PolicyViolation(
                    "Routing policy requires pinned adapters; none provided".to_string(),
                ));
            }
        }

        let prompt_hash = B3Hash::hash(request.prompt.as_bytes());
        self.apply_abstain_context(&request, &prompt_hash);
        let _ = self.evaluate_policies_or_fail(&request, &prompt_hash, "pre_inference")?;

        // 1. Apply chat template and tokenize
        let formatted_prompt = self.tokenizer.apply_chat_template(&request.prompt);
        let input_tokens = self.tokenizer.encode(&formatted_prompt)?;

        debug!("Tokenized prompt: {} tokens", input_tokens.len());

        // 2. Validate sequence length
        if input_tokens.len() > self.config.max_seq_len {
            return Err(AosError::Worker(format!(
                "Input too long: {} tokens exceeds max {}",
                input_tokens.len(),
                self.config.max_seq_len
            )));
        }

        // 3. Initialize generation state
        let mut generated_tokens = Vec::new();
        let mut router_decisions = Vec::new();
        let mut current_tokens = input_tokens.clone();
        let mut total_router_time_us = 0u64;

        // 3.5 Initialize stop controller (PRD: Hard Deterministic Stop Controller)
        let stop_sequences_tokens = match request.stop_policy.as_ref() {
            Some(policy) => {
                let mut sequences = Vec::new();
                for sequence in &policy.stop_sequences {
                    if sequence.is_empty() {
                        continue;
                    }
                    let tokens = self.tokenizer.encode(sequence)?;
                    if tokens.is_empty() {
                        continue;
                    }
                    sequences.push(tokens);
                }
                sequences
            }
            None => Vec::new(),
        };
        let mut stop_controller = StopController::from_policy_or_default_with_stop_sequences(
            request.stop_policy.clone(),
            request.max_tokens as u32,
            stop_sequences_tokens,
        );
        let stop_policy_digest = *stop_controller.policy_digest();
        let mut stop_reason_code = None;
        let mut stop_reason_token_index = None;
        let admin_override = request.admin_override;
        let max_reasoning_depth = request
            .routing_policy
            .as_ref()
            .and_then(|policy| policy.max_reasoning_depth)
            .unwrap_or(usize::MAX);
        let cluster_fallback = request
            .routing_policy
            .as_ref()
            .map(|p| p.cluster_fallback.as_str())
            .unwrap_or("stay_on_current");
        let mut transition_count: usize = 0;
        let mut previous_decision: Option<adapteros_lora_router::Decision> = None;

        // 4. Autoregressive generation loop
        for step in 0..request.max_tokens {
            // Prepare input for this step
            let input_ids = if step == 0 {
                // First step: use full prompt
                &current_tokens[..]
            } else {
                // Subsequent steps: use last token
                let last_token = generated_tokens.last().ok_or_else(|| {
                    AosError::Worker("Generated tokens cannot be empty".to_string())
                })?;
                std::slice::from_ref(last_token)
            };
            let input_token_id = input_ids.last().copied();

            // 5. Router decision: select K adapters
            // Create feature vector from token embeddings (simplified for now)
            let features = self.create_feature_vector(&current_tokens);
            let priors = vec![1.0; 8]; // Uniform priors for all adapters
                                       // Create dummy adapter info for route_with_adapter_info
            let adapter_info: Vec<AdapterInfo> = (0..8)
                .map(|i| AdapterInfo {
                    id: format!("adapter_{}", i),
                    framework: None,
                    languages: vec![0], // Default language
                    tier: "persistent".to_string(),
                    ..Default::default()
                })
                .collect();

            let (adapter_info, priors) = self.filter_adapters(&adapter_info, &priors)?;
            let adapter_clusters: Vec<Option<String>> = adapter_info
                .iter()
                .map(|a| derive_cluster_from_id(&a.id))
                .collect();

            let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
            let policy_digest_seed = request
                .routing_policy
                .as_ref()
                .and_then(|policy| serde_json::to_vec(policy).ok())
                .map(|bytes| B3Hash::hash(&bytes));
            let policy_mask = PolicyMask::build(
                &adapter_ids,
                request
                    .routing_policy
                    .as_ref()
                    .and_then(|p| p.allowed_adapter_ids.as_deref()),
                request
                    .routing_policy
                    .as_ref()
                    .and_then(|p| p.denied_adapter_ids.as_deref()),
                None,
                None,
                policy_digest_seed,
            );

            let router_start = Instant::now();
            let decision = self.router.route_with_adapter_info(
                &features,
                &priors,
                &adapter_info,
                &policy_mask,
            )?;

            // Enforce resolved routing policy deterministically before kernels run
            let mut decision = match enforce_routing_policy_on_decision(
                decision,
                &adapter_info,
                &adapter_clusters,
                request.routing_policy.as_ref(),
            ) {
                Ok(decision) => decision,
                Err(err) => {
                    if admin_override {
                        self.log_blocked_transition(
                            &request,
                            step,
                            "cluster_denied_admin_override",
                            cluster_fallback,
                        );
                        fallback_decision(
                            previous_decision.as_ref(),
                            adapter_info.len(),
                            cluster_fallback,
                        )
                    } else if request.routing_policy.is_some() {
                        self.log_blocked_transition(
                            &request,
                            step,
                            "cluster_denied",
                            cluster_fallback,
                        );
                        fallback_decision(
                            previous_decision.as_ref(),
                            adapter_info.len(),
                            cluster_fallback,
                        )
                    } else {
                        return Err(err);
                    }
                }
            };

            let abstain_events = self.router.take_abstain_events();
            if !abstain_events.is_empty() {
                return Err(self.handle_abstain_events(
                    &request,
                    "pre_kernel",
                    step,
                    &abstain_events,
                ));
            }

            if let Some(prev) = &previous_decision {
                if prev.indices.as_slice() != decision.indices.as_slice() {
                    transition_count = transition_count.saturating_add(1);
                }
            }

            if !admin_override && transition_count > max_reasoning_depth {
                self.log_blocked_transition(
                    &request,
                    step,
                    "max_reasoning_depth",
                    cluster_fallback,
                );
                decision = fallback_decision(
                    previous_decision.as_ref(),
                    adapter_info.len(),
                    cluster_fallback,
                );
                transition_count = max_reasoning_depth;
            }

            previous_decision = Some(decision.clone());
            let router_latency = router_start.elapsed();
            total_router_time_us += router_latency.as_micros() as u64;

            // Emit router decision telemetry
            let router_event = RouterDecisionEvent {
                step,
                input_token_id,
                candidate_adapters: decision
                    .candidates
                    .iter()
                    .map(|c| RouterCandidate {
                        adapter_idx: c.adapter_idx,
                        raw_score: c.raw_score,
                        gate_q15: c.gate_q15,
                    })
                    .collect(),
                entropy: decision.entropy,
                tau: self.router.tau(),
                entropy_floor: self.router.eps(),
                stack_hash: self.router.stack_hash(),
                stack_id: request.stack_id.clone(),
                stack_version: request.stack_version,
                model_type: adapteros_types::routing::RouterModelType::Dense,
                active_experts: None,
            };
            if let Err(e) = self.telemetry.log_router_decision(router_event) {
                debug!(
                    target: "telemetry",
                    error = %e,
                    "Telemetry emit failed (non-fatal)"
                );
            }

            // 6. Check policy: entropy floor (Router Ruleset #7)
            // Inline enforcement happens before kernel execution via policy engine + abstain checks.
            let entropy = self.calculate_gate_entropy(&decision.gates_q15);
            let _ = entropy; // Reserved for policy check
                             // if let Err(e) = self._policy.check_router_entropy(entropy) {
                             //     warn!("Router entropy policy violation: {}", e);
                             //     // Continue with warning rather than failing - entropy floor is advisory
                             // }

            // 7. Execute kernel inference
            let mut io_buffers = IoBuffers {
                input_ids: input_ids.to_vec(),
                output_logits: vec![0.0; self.config.vocab_size],
                position: current_tokens.len() - 1,
            };

            // Convert router decision to RouterRing using explicit bridge (PRD-02)
            // This provides bounds checking and preserves decision order
            let mut router_ring = decision_to_router_ring(&decision, self.max_adapter_count)?;
            router_ring.position = step;

            let kernel_start = Instant::now();
            self.kernels.run_step(&router_ring, &mut io_buffers)?;
            let kernel_latency = kernel_start.elapsed();

            // Track kernel latency for p95 calculation
            self.budget_tracker
                .record_latency(kernel_latency.as_micros() as u64);

            // Check p95 latency budget (24ms threshold)
            if let Some(p95_ms) = self.budget_tracker.p95_latency_ms() {
                if p95_ms > 24.0 {
                    let violation = PerformanceBudgetViolationEvent::p95_latency(p95_ms, None);
                    if let Err(e) = self.telemetry.log_budget_violation(violation) {
                        warn!(error = %e, p95_ms = p95_ms, "Failed to log P95 latency violation");
                    }
                }
            }

            // 8. Sample next token
            let next_token = self.generator.next_token(&io_buffers.output_logits)?;

            // 9. Record telemetry (sampled)
            if step < 128 || (step % 20 == 0) {
                if let Err(e) = self.telemetry.log(
                    "inference.step",
                    serde_json::json!({
                        "cpid": request.cpid,
                        "step": step,
                        "token": next_token,
                        "kernel_latency_us": kernel_latency.as_micros(),
                        "adapters": decision.indices.to_vec(),
                    }),
                ) {
                    debug!(
                        target: "telemetry",
                        error = %e,
                        "Telemetry emit failed (non-fatal)"
                    );
                }
            }

            // 10. Record canonical router decision
            let candidate_adapters: Vec<RouterCandidate> = decision
                .candidates
                .iter()
                .map(|candidate| RouterCandidate {
                    adapter_idx: candidate.adapter_idx,
                    raw_score: candidate.raw_score,
                    gate_q15: candidate.gate_q15,
                })
                .collect();

            let event = RouterDecisionEvent {
                step,
                input_token_id,
                candidate_adapters,
                entropy: decision.entropy,
                tau: self.router.temperature(),
                entropy_floor: self.router.entropy_floor(),
                stack_hash: self.router.stack_hash(),
                stack_id: request.stack_id.clone(),
                stack_version: request.stack_version,
                model_type: adapteros_types::routing::RouterModelType::Dense,
                active_experts: None,
            };

            if let Err(err) = self.telemetry.log_router_decision(event.clone()) {
                warn!("Failed to log router decision: {}", err);
            }

            router_decisions.push(event);

            // 11. Check stopping criteria using StopController (PRD: Hard Deterministic Stop Controller)
            if let Some(decision) = stop_controller.check_stop(
                next_token,
                self.tokenizer.eos_token_id(),
                &io_buffers.output_logits,
            ) {
                stop_reason_code = Some(decision.reason);
                stop_reason_token_index = Some(decision.token_index);
                if decision.trim_tokens > 0 {
                    let trim = decision.trim_tokens.min(generated_tokens.len());
                    for _ in 0..trim {
                        generated_tokens.pop();
                    }
                }
                debug!(
                    step,
                    reason = %decision.reason,
                    token_index = decision.token_index,
                    "Stop controller triggered"
                );
                // For LENGTH stop (EOS token), don't include the EOS in output.
                // For STOP_SEQUENCE, trim previously emitted tokens that form the sequence.
                // For other reasons, we've already decided to stop before appending.
                break;
            }

            // 12. Append token and continue
            generated_tokens.push(next_token);
            current_tokens.push(next_token);

            // Check max sequence length (fallback for very long sequences)
            if current_tokens.len() >= self.config.max_seq_len {
                warn!("Reached maximum sequence length");
                stop_reason_code = Some(adapteros_api_types::inference::StopReasonCode::BudgetMax);
                stop_reason_token_index = Some(step as u32);
                break;
            }
        }

        // 13. Decode generated text
        let generated_text = self.tokenizer.decode(&generated_tokens)?;

        // 14. Build trace for reproducibility
        let trace = InferenceTrace {
            cpid: request.cpid.clone(),
            input_tokens: input_tokens.clone(),
            generated_tokens: generated_tokens.clone(),
            router_decisions,
            evidence: vec![], // Populated if RAG is enabled
        };

        let latency = start_time.elapsed();

        // Check router overhead budget (8% threshold)
        let total_inference_us = latency.as_micros() as u64;
        self.budget_tracker
            .record_router_timing(total_router_time_us, total_inference_us);
        if let Some(overhead_pct) = self.budget_tracker.router_overhead_pct() {
            if overhead_pct > 8.0 {
                let violation = PerformanceBudgetViolationEvent::router_overhead(overhead_pct);
                if let Err(e) = self.telemetry.log_budget_violation(violation) {
                    warn!(error = %e, overhead_pct = overhead_pct, "Failed to log router overhead violation");
                }
            }
        }

        // 15. Log final telemetry
        if let Err(e) = self.telemetry.log(
            "inference.complete",
            serde_json::json!({
                "cpid": request.cpid,
                "input_tokens": input_tokens.len(),
                "generated_tokens": generated_tokens.len(),
                "latency_ms": latency.as_millis(),
            }),
        ) {
            debug!(
                target: "telemetry",
                error = %e,
                "Telemetry emit failed (non-fatal)"
            );
        }

        info!(
            "Inference complete: generated {} tokens in {}ms",
            generated_tokens.len(),
            latency.as_millis()
        );

        self.router.clear_abstain_context();

        Ok(InferenceResponse {
            text: generated_text,
            token_count: generated_tokens.len(),
            latency_ms: latency.as_millis() as u64,
            trace,
            rainbow_trace: None,
            reasoning_transitions: None,
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
            stop_reason_code,
            stop_reason_token_index,
            stop_policy_digest_b3: Some(stop_policy_digest),
        })
    }

    /// Create feature vector for router from tokens
    fn create_feature_vector(&self, tokens: &[u32]) -> Vec<f32> {
        // Simplified feature extraction
        // In production, this would use token embeddings and more sophisticated features
        let mut features = vec![0.0; 22]; // 22-dimensional feature vector

        // Language detection (one-hot, 8 dims)
        features[0] = 1.0; // Assume English for now

        // Framework scores (3 dims)
        features[8] = 0.5; // Generic framework score

        // Symbol hits (1 dim)
        features[11] = 0.0;

        // Path tokens (1 dim)
        features[12] = 0.0;

        // Prompt verb (one-hot, 8 dims)
        features[13] = 1.0; // Generic verb

        // Attention entropy (1 dim)
        features[21] = self.calculate_token_entropy(tokens);

        features
    }

    /// Calculate entropy from token distribution
    fn calculate_token_entropy(&self, tokens: &[u32]) -> f32 {
        if tokens.is_empty() {
            return 0.0;
        }

        // Simple entropy calculation based on token variety
        let unique_tokens: std::collections::HashSet<_> = tokens.iter().collect();
        let variety = unique_tokens.len() as f32 / tokens.len() as f32;

        // Normalize to [0, 1]
        variety.min(1.0)
    }

    /// Calculate entropy from Q15 gate distribution
    fn calculate_gate_entropy(&self, gates_q15: &[i16]) -> f32 {
        if gates_q15.is_empty() {
            return 0.0;
        }

        // Convert Q15 gates to probabilities (handle negative as 0)
        let sum: f32 = gates_q15.iter().map(|&g| g.max(0) as f32).sum();
        if sum == 0.0 {
            return 0.0;
        }

        // Calculate Shannon entropy
        let mut entropy = 0.0;
        for &gate in gates_q15 {
            if gate > 0 {
                let p = gate as f32 / sum;
                entropy -= p * p.log2();
            }
        }

        // Normalize to [0, 1]
        let max_entropy = (gates_q15.len() as f32).log2();
        if max_entropy > 0.0 {
            entropy / max_entropy
        } else {
            0.0
        }
    }

    fn log_blocked_transition(
        &self,
        request: &InferenceRequest,
        step: usize,
        reason: &str,
        fallback: &str,
    ) {
        if let Err(e) = self.telemetry.log(
            "policy.blocked_transition",
            serde_json::json!({
                "cpid": request.cpid,
                "step": step,
                "reason": reason,
                "fallback": fallback,
                "admin_override": request.admin_override,
                "routing_policy": request.routing_policy.as_ref().map(|p| {
                    serde_json::json!({
                        "allowed_clusters": p.allowed_clusters,
                        "denied_clusters": p.denied_clusters,
                        "max_reasoning_depth": p.max_reasoning_depth,
                    })
                }),
            }),
        ) {
            debug!(
                target: "telemetry",
                error = %e,
                "Telemetry emit failed (non-fatal)"
            );
        }
    }

    /// Batch inference for multiple prompts
    pub async fn infer_batch(
        &mut self,
        requests: Vec<InferenceRequest>,
    ) -> Result<Vec<InferenceResponse>> {
        let mut responses = Vec::with_capacity(requests.len());

        for request in requests {
            let response = self.infer(request).await?;
            responses.push(response);
        }

        Ok(responses)
    }

    /// Get model configuration
    pub fn config(&self) -> &InferencePipelineConfig {
        &self.config
    }
}

fn derive_cluster_from_id(id: &str) -> Option<String> {
    id.split(['-', '_', '.'])
        .next()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

fn apply_allowlist(
    allowlist: Option<&Vec<String>>,
    adapter_info: &[AdapterInfo],
    priors: &[f32],
) -> Result<(Vec<AdapterInfo>, Vec<f32>)> {
    if adapter_info.len() != priors.len() {
        return Err(AosError::PolicyViolation(
            "adapter_info and priors length mismatch".to_string(),
        ));
    }

    if let Some(allow) = allowlist {
        let allowed: HashSet<&String> = allow.iter().collect();
        let mut filtered_info = Vec::new();
        let mut filtered_priors = Vec::new();

        for (info, prior) in adapter_info.iter().zip(priors.iter()) {
            if allowed.contains(&info.id) {
                filtered_info.push(info.clone());
                filtered_priors.push(*prior);
            }
        }

        if filtered_info.is_empty() {
            return Err(AosError::PolicyViolation(
                "No adapters allowed by routing policy".to_string(),
            ));
        }

        Ok((filtered_info, filtered_priors))
    } else {
        Ok((adapter_info.to_vec(), priors.to_vec()))
    }
}

fn enforce_routing_policy_on_decision(
    decision: adapteros_lora_router::Decision,
    adapter_info: &[AdapterInfo],
    adapter_clusters: &[Option<String>],
    policy: Option<&adapteros_api_types::RoutingPolicy>,
) -> Result<adapteros_lora_router::Decision> {
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    filter_decision_by_policy(decision, &adapter_ids, adapter_clusters, policy)
}

fn fallback_decision(
    previous: Option<&adapteros_lora_router::Decision>,
    adapter_count: usize,
    mode: &str,
) -> adapteros_lora_router::Decision {
    if adapter_count == 0 {
        return adapteros_lora_router::Decision {
            indices: SmallVec::new(),
            gates_q15: SmallVec::new(),
            entropy: 0.0,
            candidates: Vec::new(),
            decision_hash: None,
            policy_mask_digest_b3: None,
            policy_overrides_applied: None,
        };
    }

    match mode {
        "fallback_to_base" => adapteros_lora_router::Decision {
            indices: SmallVec::from_slice(&[0u16]),
            gates_q15: SmallVec::from_slice(&[ROUTER_GATE_Q15_MAX]),
            entropy: 0.0,
            candidates: Vec::new(),
            decision_hash: None,
            policy_mask_digest_b3: None,
            policy_overrides_applied: None,
        },
        _ => previous
            .cloned()
            .unwrap_or_else(|| adapteros_lora_router::Decision {
                indices: SmallVec::from_slice(&[0u16]),
                gates_q15: SmallVec::from_slice(&[ROUTER_GATE_Q15_MAX]),
                entropy: 0.0,
                candidates: Vec::new(),
                decision_hash: None,
                policy_mask_digest_b3: None,
                policy_overrides_applied: None,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_types::RequestType;

    #[test]
    fn test_inference_config_default() {
        let config = InferencePipelineConfig::default();
        // Default values come from ModelConfig::from_env() or ModelConfig::default()
        // which uses Qwen2.5 defaults
        let model_config = adapteros_config::ModelConfig::default();
        assert_eq!(config.model_name, model_config.architecture);
        assert_eq!(config.vocab_size, model_config.vocab_size);
        assert_eq!(config.max_seq_len, model_config.max_seq_len);
    }

    #[test]
    fn test_inference_config_from_model_config() {
        let model_config = adapteros_config::ModelConfig::default();
        let config = InferencePipelineConfig::from_model_config(&model_config);
        assert_eq!(config.model_name, model_config.architecture);
        assert_eq!(config.vocab_size, model_config.vocab_size);
        assert_eq!(config.max_seq_len, model_config.max_seq_len);
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.top_k, Some(50));
        assert_eq!(config.top_p, Some(0.95));
    }

    #[test]
    fn test_inference_config_from_model_config_with_sampling() {
        let model_config = adapteros_config::ModelConfig::default();
        let config = InferencePipelineConfig::from_model_config_with_sampling(
            &model_config,
            0.9,
            Some(100),
            Some(0.8),
        );
        assert_eq!(config.model_name, model_config.architecture);
        assert_eq!(config.vocab_size, model_config.vocab_size);
        assert_eq!(config.max_seq_len, model_config.max_seq_len);
        assert_eq!(config.temperature, 0.9);
        assert_eq!(config.top_k, Some(100));
        assert_eq!(config.top_p, Some(0.8));
    }

    #[test]
    fn test_inference_request() {
        let request = InferenceRequest {
            prompt: "What is 2+2?".to_string(),
            max_tokens: 100,
            request_id: None,
            run_envelope: None,
            request_type: RequestType::default(),
            reasoning_mode: false,
            cpid: "test-cp-001".to_string(),
            require_evidence: false,
            stack_id: None,
            stack_version: None,
            domain_hint: None,
            temperature: None,
            top_k: None,
            top_p: None,
            seed: None,
            router_seed: None,
            seed_mode: None,
            request_seed: None,
            determinism: None,
            fusion_interval: None,
            backend_profile: None,
            coreml_mode: None,
            pinned_adapter_ids: None,
            determinism_mode: "strict".to_string(),
            routing_determinism_mode: None,
            strict_mode: false,
            adapter_strength_overrides: None,
            effective_adapter_ids: None,
            placement: None,
            routing_policy: None,
            stop_policy: None,
            policy_mask_digest_b3: None,
            utf8_healing: true,
            admin_override: false,
            arrival_instant: None,
        };
        assert_eq!(request.max_tokens, 100);
    }

    #[test]
    fn test_apply_allowlist_filters() {
        let adapters: Vec<AdapterInfo> = (0..3)
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![0],
                tier: "persistent".to_string(),
                ..Default::default()
            })
            .collect();
        let priors = vec![1.0, 2.0, 3.0];
        let allow = vec!["adapter_1".to_string(), "adapter_2".to_string()];

        let (filtered, priors_filtered) =
            apply_allowlist(Some(&allow), &adapters, &priors).expect("filter should pass");

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].id, "adapter_1");
        assert_eq!(priors_filtered, vec![2.0, 3.0]);
    }

    #[test]
    fn test_apply_allowlist_empty_fails() {
        let adapters: Vec<AdapterInfo> = (0..1)
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![0],
                tier: "persistent".to_string(),
                ..Default::default()
            })
            .collect();
        let priors = vec![1.0];
        let allow = vec!["other".to_string()];

        let result = apply_allowlist(Some(&allow), &adapters, &priors);
        assert!(matches!(result, Err(AosError::PolicyViolation(_))));
    }
}
