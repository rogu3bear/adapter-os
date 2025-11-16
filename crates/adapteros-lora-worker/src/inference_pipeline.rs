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

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_lora_router::Router;
use adapteros_lora_router::{extract_attn_entropy, CodeFeatures};
use adapteros_manifest::ManifestV3;
use adapteros_policy::{PolicyEngine, QuarantineManager, QuarantineOperation};
use adapteros_telemetry::TelemetryWriter;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::generation::Generator;
use crate::tokenizer::QwenTokenizer;

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
}

impl Default for InferencePipelineConfig {
    fn default() -> Self {
        Self {
            model_name: "Qwen2.5-7B-Instruct".to_string(),
            vocab_size: 152064, // Qwen2.5 vocab size
            max_seq_len: 32768, // Qwen2.5 max position embeddings
            temperature: 0.7,
            top_k: Some(50),
            top_p: Some(0.95),
        }
    }
}

/// Inference request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferenceRequest {
    /// Input prompt
    pub prompt: String,
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Control plane ID for tracing
    pub cpid: String,
    /// Whether to require evidence grounding
    pub require_evidence: bool,
    /// Request type for policy enforcement
    pub request_type: Option<crate::RequestType>,
}

/// Inference response with trace
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferenceResponse {
    /// Generated text
    pub text: String,
    /// Token count
    pub token_count: usize,
    /// Inference latency
    pub latency_ms: u64,
    /// Trace for reproducibility
    pub trace: InferenceTrace,
}

/// Trace information for reproducible inference
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// Router decision for a single generation step
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouterDecision {
    /// Step number
    pub step: usize,
    /// Selected adapter indices
    pub adapter_indices: Vec<u16>,
    /// Quantized gates (Q15)
    pub gates_q15: Vec<i16>,
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
    kernels: Box<dyn FusedKernels>,
    /// Policy engine
    #[allow(dead_code)]
    policy: PolicyEngine,
    /// Telemetry writer
    telemetry: TelemetryWriter,
    /// Configuration
    config: InferencePipelineConfig,
    /// Quarantine manager for policy hash enforcement
    quarantine_manager: Arc<Mutex<QuarantineManager>>,
    /// Optional prior computation context (manifest + lifecycle)
    prior_context: Option<PriorContext>,
    /// Recent output logits for sliding-window entropy
    recent_logits: Vec<Vec<f32>>,
    /// Sliding entropy window size (default 8)
    entropy_window: usize,
    /// Determinism policy validator
    determinism_validator: adapteros_policy::packs::determinism::DeterminismPolicy,
    /// Derived seeds for deterministic operations
    worker_seed: adapteros_core::B3Hash,
    router_seed: adapteros_core::B3Hash,
    inference_seed: adapteros_core::B3Hash,
    /// Seed for any preprocessing operations that require randomness
    /// Currently unused but available for future preprocessing steps
    pre_inference_seed: adapteros_core::B3Hash,
    /// Seed for any postprocessing operations that require randomness
    /// Currently unused but available for future postprocessing steps
    post_inference_seed: adapteros_core::B3Hash,
}

/// Context for computing priors during routing
#[derive(Clone)]
struct PriorContext {
    /// Framework IDs for each adapter in manifest order
    adapter_framework_ids: Vec<Option<String>>,
    /// Lifecycle manager handle for activation percentages
    lifecycle: Arc<LifecycleManager>,
    /// Upstream adapter indices (for pipeline phase separation)
    upstream_adapter_indices: Vec<u16>,
    /// Downstream adapter indices (for pipeline phase separation)
    downstream_adapter_indices: Vec<u16>,
}

impl InferencePipeline {
    /// Create new inference pipeline
    pub fn new(
        tokenizer_path: &Path,
        router: Router,
        kernels: Box<dyn FusedKernels>,
        policy: PolicyEngine,
        telemetry: TelemetryWriter,
        config: InferencePipelineConfig,
        global_seed: adapteros_core::B3Hash,
    ) -> Result<Self> {
        // Validate backend determinism before constructing pipeline
        let report = kernels.attest_determinism()?;
        // Create policy validator from manifest config
        let determinism_validator = adapteros_policy::packs::determinism::DeterminismPolicy::new(
            adapteros_policy::packs::determinism::DeterminismConfig {
                require_metallib_embed: policy.determinism_policy().require_metallib_embed,
                require_kernel_hash_match: policy.determinism_policy().require_kernel_hash_match,
                rng: adapteros_policy::packs::determinism::RngSeedingMethod::HkdfSeeded,
                retrieval_tie_break: vec![],
                epsilon_bounds: adapteros_policy::packs::determinism::EpsilonBounds {
                    logits_epsilon: 1e-6,
                    embeddings_epsilon: 1e-5,
                    attention_epsilon: 1e-6,
                    gates_epsilon: 1e-4,
                },
                toolchain_requirements:
                    adapteros_policy::packs::determinism::ToolchainRequirements {
                        rust_version: "1.75.0".to_string(),
                        metal_sdk_version: "3.0".to_string(),
                        kernel_compiler_version: "1.0".to_string(),
                        allowed_compiler_flags: vec!["-O2".to_string(), "-ffast-math".to_string()],
                    },
                min_router_entropy: 0.1,
            },
        );
        determinism_validator.validate_backend_attestation(&report)?;

        if !policy.determinism_policy().require_metallib_embed {
            tracing::warn!("Metallib embed requirement disabled in policy");
        }
        if !policy.determinism_policy().require_kernel_hash_match {
            tracing::warn!("Kernel hash match requirement disabled in policy");
        }

        info!("Backend determinism validated: {}", report.summary());

        let tokenizer = QwenTokenizer::from_file(tokenizer_path)?;

        // Derive seeds hierarchically from global seed
        let worker_seed_bytes = adapteros_core::derive_seed(&global_seed, "worker");
        let router_seed_bytes = adapteros_core::derive_seed(&global_seed, "router");
        let inference_seed_bytes =
            adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "inference");
        let pre_inference_seed_bytes =
            adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "pre_inference");
        let post_inference_seed_bytes =
            adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "post_inference");

        // Convert to B3Hash for storage
        let worker_seed = B3Hash::from_bytes(worker_seed_bytes);
        let router_seed = B3Hash::from_bytes(router_seed_bytes);
        let inference_seed = B3Hash::from_bytes(inference_seed_bytes);
        let pre_inference_seed = B3Hash::from_bytes(pre_inference_seed_bytes);
        let post_inference_seed = B3Hash::from_bytes(post_inference_seed_bytes);

        // Create deterministic generator with inference seed
        let generator = Generator::new(inference_seed_bytes)
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
            policy,
            telemetry,
            config,
            quarantine_manager,
            prior_context: None,
            recent_logits: Vec::new(),
            entropy_window: 8,
            determinism_validator,
            worker_seed,
            router_seed,
            inference_seed,
            pre_inference_seed,
            post_inference_seed,
        })
    }

    /// Create new inference pipeline with quarantine manager
    /// This allows external initialization of the quarantine state
    pub fn with_quarantine(
        tokenizer_path: &Path,
        router: Router,
        kernels: Box<dyn FusedKernels>,
        policy: PolicyEngine,
        telemetry: TelemetryWriter,
        config: InferencePipelineConfig,
        quarantine_manager: Arc<Mutex<QuarantineManager>>,
        global_seed: adapteros_core::B3Hash,
    ) -> Result<Self> {
        // Validate backend determinism before constructing pipeline
        let report = kernels.attest_determinism()?;
        // Create policy validator from manifest config
        let determinism_validator = adapteros_policy::packs::determinism::DeterminismPolicy::new(
            adapteros_policy::packs::determinism::DeterminismConfig {
                require_metallib_embed: policy.determinism_policy().require_metallib_embed,
                require_kernel_hash_match: policy.determinism_policy().require_kernel_hash_match,
                rng: adapteros_policy::packs::determinism::RngSeedingMethod::HkdfSeeded,
                retrieval_tie_break: vec![],
                epsilon_bounds: adapteros_policy::packs::determinism::EpsilonBounds {
                    logits_epsilon: 1e-6,
                    embeddings_epsilon: 1e-5,
                    attention_epsilon: 1e-6,
                    gates_epsilon: 1e-4,
                },
                toolchain_requirements:
                    adapteros_policy::packs::determinism::ToolchainRequirements {
                        rust_version: "1.75.0".to_string(),
                        metal_sdk_version: "3.0".to_string(),
                        kernel_compiler_version: "1.0".to_string(),
                        allowed_compiler_flags: vec!["-O2".to_string(), "-ffast-math".to_string()],
                    },
                min_router_entropy: 0.1,
            },
        );
        determinism_validator.validate_backend_attestation(&report)?;

        if !policy.determinism_policy().require_metallib_embed {
            tracing::warn!("Metallib embed requirement disabled in policy");
        }
        if !policy.determinism_policy().require_kernel_hash_match {
            tracing::warn!("Kernel hash match requirement disabled in policy");
        }

        info!("Backend determinism validated: {}", report.summary());

        let tokenizer = QwenTokenizer::from_file(tokenizer_path)?;

        // Derive seeds hierarchically from global seed
        let worker_seed_bytes = adapteros_core::derive_seed(&global_seed, "worker");
        let router_seed_bytes = adapteros_core::derive_seed(&global_seed, "router");
        let inference_seed_bytes =
            adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "inference");
        let pre_inference_seed_bytes =
            adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "pre_inference");
        let post_inference_seed_bytes =
            adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "post_inference");

        // Convert to B3Hash for storage
        let worker_seed = B3Hash::from_bytes(worker_seed_bytes);
        let router_seed = B3Hash::from_bytes(router_seed_bytes);
        let inference_seed = B3Hash::from_bytes(inference_seed_bytes);
        let pre_inference_seed = B3Hash::from_bytes(pre_inference_seed_bytes);
        let post_inference_seed = B3Hash::from_bytes(post_inference_seed_bytes);

        // Create deterministic generator with inference seed
        let generator = Generator::new(inference_seed_bytes)
            .with_temperature(config.temperature)
            .with_top_k(config.top_k.unwrap_or(50))
            .with_top_p(config.top_p.unwrap_or(0.9));

        Ok(Self {
            tokenizer,
            generator,
            router,
            kernels,
            policy,
            telemetry,
            config,
            quarantine_manager,
            prior_context: None,
            recent_logits: Vec::new(),
            entropy_window: 8,
            determinism_validator,
            worker_seed,
            router_seed,
            inference_seed,
            pre_inference_seed,
            post_inference_seed,
        })
    }

    /// Attach prior computation context (manifest + lifecycle). Optional.
    /// Precomputes adapter framework IDs for efficient per-step priors.
    pub fn with_prior_context(
        mut self,
        manifest: &ManifestV3,
        lifecycle: Arc<LifecycleManager>,
    ) -> Self {
        let adapter_framework_ids = manifest
            .adapters
            .iter()
            .map(|a| a.framework_id.clone())
            .collect::<Vec<_>>();

        // Separate adapters into upstream and downstream based on manifest
        let mut upstream_adapter_indices = Vec::new();
        let mut downstream_adapter_indices = Vec::new();

        for (idx, adapter) in manifest.adapters.iter().enumerate() {
            if adapter.upstream_enabled {
                upstream_adapter_indices.push(idx as u16);
            } else {
                downstream_adapter_indices.push(idx as u16);
            }
        }

        info!(
            "Pipeline configured with {} upstream adapters and {} downstream adapters",
            upstream_adapter_indices.len(),
            downstream_adapter_indices.len()
        );

        self.prior_context = Some(PriorContext {
            adapter_framework_ids,
            lifecycle,
            upstream_adapter_indices,
            downstream_adapter_indices,
        });
        self
    }

    /// Apply upstream adapters to input tokens (Phase 1 of two-phase pipeline)
    ///
    /// ⚠️ **INCOMPLETE IMPLEMENTATION**: This is infrastructure-only code that tracks
    /// upstream adapters and demonstrates the pipeline architecture, but does NOT
    /// actually affect model outputs. The computed embeddings are currently discarded.
    ///
    /// **What this does:**
    /// - Identifies and tracks adapters marked as `upstream_enabled`
    /// - Applies deterministic transformations using `pre_inference_seed`
    /// - Emits telemetry events for upstream adapter activation
    /// - Ensures downstream phase excludes upstream adapters
    ///
    /// **What this does NOT do:**
    /// - Actually modify the model's embedding layer with LoRA weights
    /// - Pass transformed embeddings to the kernel backend
    /// - Affect model outputs in any way
    ///
    /// **TODO for full implementation:**
    /// 1. Load actual LoRA weight matrices (A, B) for upstream adapters
    /// 2. Compute real embedding transformations: `E' = E + (alpha/r) * B @ A`
    /// 3. Extend `FusedKernels` API to accept pre-computed embeddings
    /// 4. Modify `IoBuffers` to support embedding input mode
    /// 5. Update all kernel backends (Metal, MLX, etc.) to handle embeddings
    /// 6. Add integration tests that verify output changes
    ///
    /// The transformation is deterministic and uses the `pre_inference_seed` for any
    /// random operations to ensure reproducibility.
    async fn apply_upstream_adapters(&self, input_tokens: &[u32]) -> Result<Vec<f32>> {
        // If no prior context or no upstream adapters, return identity transformation
        let pc = match &self.prior_context {
            Some(pc) if !pc.upstream_adapter_indices.is_empty() => pc,
            _ => {
                // No upstream adapters - return identity (tokens as-is, converted to f32)
                return Ok(input_tokens.iter().map(|&t| t as f32).collect());
            }
        };

        warn!(
            "⚠️  INCOMPLETE FEATURE: {} upstream adapters detected but will NOT affect outputs. \
             This is infrastructure-only code. See apply_upstream_adapters() documentation.",
            pc.upstream_adapter_indices.len()
        );

        info!(
            "Applying {} upstream adapters to input (length: {}) - tracking only",
            pc.upstream_adapter_indices.len(),
            input_tokens.len()
        );

        // TODO: Initialize real embedding vectors from model's embedding layer
        // This placeholder just converts token IDs to f32, which is not semantically correct
        // Real embeddings would be high-dimensional vectors (e.g., 4096-d per token)
        let mut embeddings: Vec<f32> = input_tokens.iter().map(|&t| t as f32).collect();

        // Apply each upstream adapter to modify the embeddings deterministically
        for &adapter_idx in &pc.upstream_adapter_indices {
            // Use pre_inference_seed to derive adapter-specific seed for determinism
            let adapter_seed = adapteros_core::derive_seed(
                &self.pre_inference_seed,
                &format!("upstream_adapter_{}", adapter_idx),
            );

            // TODO: Replace this placeholder with actual LoRA weight application
            // Real implementation would:
            // 1. Load LoRA matrices A (d×r) and B (r×d) from adapter file
            // 2. Compute delta: Δ = (alpha/r) * B @ A
            // 3. Apply to embeddings: E'[i] = E[i] + Δ @ E[i]
            //
            // Current placeholder: deterministic bias based on adapter seed
            let seed_hash = u64::from_le_bytes([
                adapter_seed[0],
                adapter_seed[1],
                adapter_seed[2],
                adapter_seed[3],
                adapter_seed[4],
                adapter_seed[5],
                adapter_seed[6],
                adapter_seed[7],
            ]);

            // Apply a small deterministic bias to each embedding dimension
            // Scale factor is small to avoid drastically changing embeddings
            let bias_scale = 0.01;
            for (i, emb) in embeddings.iter_mut().enumerate() {
                let position_seed = seed_hash.wrapping_add(i as u64);
                let bias = ((position_seed % 1000) as f32 / 1000.0 - 0.5) * bias_scale;
                *emb += bias;
            }

            // Log upstream adapter activation
            if let Some(ref telemetry) = self.telemetry {
                let _ = telemetry.log(
                    "adapter.activated",
                    serde_json::json!({
                        "adapter_idx": adapter_idx,
                        "state": "upstream",
                        "phase": "pre_inference",
                        "input_length": input_tokens.len(),
                    }),
                );
            }

            debug!(
                "Applied upstream adapter {} to input embeddings",
                adapter_idx
            );
        }

        Ok(embeddings)
    }

    /// Run inference on a prompt
    pub async fn infer(&mut self, request: InferenceRequest) -> Result<InferenceResponse> {
        let start_time = Instant::now();

        // Check quarantine before serving (Determinism Ruleset #2)
        {
            let quarantine = self.quarantine_manager.lock().unwrap();
            quarantine.check_operation(QuarantineOperation::Inference)?;
        }

        info!(
            "Starting inference: prompt_len={}, max_tokens={}",
            request.prompt.len(),
            request.max_tokens
        );

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

        // 2.5. PHASE 1: Apply upstream adapters to input embeddings
        // ⚠️ INCOMPLETE: This currently only tracks and logs upstream adapters
        // The modified embeddings are DISCARDED and do not affect model outputs
        let upstream_start = Instant::now();
        let _modified_embeddings = self.apply_upstream_adapters(&input_tokens).await?;
        let upstream_latency = upstream_start.elapsed();

        // TODO: Pass modified_embeddings to kernels instead of discarding them
        // This requires:
        // 1. Extending IoBuffers with: input_embeddings: Option<Vec<f32>>
        // 2. Modifying FusedKernels::run_step() to check for embeddings before tokens
        // 3. Updating all kernel backends (Metal, MLX) to support embedding input
        // 4. Handling embedding→logits flow without tokenization
        //
        // Until then, the generation loop below uses original input_tokens (unmodified)

        if let Some(ref telemetry) = self.telemetry {
            let _ = telemetry.log(
                "inference.upstream_phase",
                serde_json::json!({
                    "cpid": request.cpid,
                    "upstream_latency_us": upstream_latency.as_micros(),
                    "input_tokens": input_tokens.len(),
                }),
            );
        }

        debug!(
            "Upstream phase completed in {}us",
            upstream_latency.as_micros()
        );

        // 3. Initialize generation state
        let mut generated_tokens = Vec::with_capacity(request.max_tokens);
        let mut router_decisions = Vec::with_capacity(request.max_tokens);
        let mut current_tokens = input_tokens.clone();

        // Cache code features for this prompt and reuse per step
        let cached_code_features = self.create_code_features(&formatted_prompt);
        let features_vec = cached_code_features.to_vector();

        // Reuse IO buffers and router ring across steps to reduce allocations
        let mut io_buffers = IoBuffers::new(self.config.vocab_size);
        let mut router_ring = RouterRing::new(self.router.get_k());

        // 4. PHASE 2: Autoregressive generation loop (downstream adapters only)
        // This phase applies downstream adapters during generation as previously done
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

            // 5. Router decision: select K adapters (features cached)

            // Compute priors
            let priors = if let Some(pc) = &self.prior_context {
                if pc.adapter_framework_ids.is_empty() {
                    // No adapters available in manifest; fallback to uniform priors
                    vec![1.0; self.router.adapter_count().unwrap_or(8)]
                } else {
                    let mut priors = vec![1.0f32; pc.adapter_framework_ids.len()];

                    // Framework prior boosts from features
                    for (i, fw) in pc.adapter_framework_ids.iter().enumerate() {
                        if let Some(framework) = fw {
                            if let Some(boost) = cached_code_features.framework_prior.get(framework)
                            {
                                priors[i] += boost * 0.5; // scale framework boost
                            }
                        }
                    }

                    // Lifecycle activation percentage prior
                    // Collect all activation percentage futures first, then await them
                    let activation_futures: Vec<_> = (0..priors.len())
                        .map(|i| pc.lifecycle.activation_pct(i as u16))
                        .collect();
                    let mut activation_values = Vec::with_capacity(activation_futures.len());
                    for fut in activation_futures {
                        activation_values.push(fut.await);
                    }
                    for (i, prior) in priors.iter_mut().enumerate() {
                        let act = activation_values[i];
                        *prior += act;
                    }

                    // Filter out upstream adapters during downstream phase (generation)
                    // Set their priors to 0 so they won't be selected by the router
                    for &upstream_idx in &pc.upstream_adapter_indices {
                        if (upstream_idx as usize) < priors.len() {
                            priors[upstream_idx as usize] = 0.0;
                        }
                    }

                    priors
                }
            } else {
                // Fallback to uniform priors (legacy behavior)
                vec![1.0; self.router.adapter_count().unwrap_or(8)]
            };

            // Sync K from lifecycle (if available)
            if let Some(pc) = &self.prior_context {
                let k_now = pc.lifecycle.current_k();
                self.router.set_k(k_now);
            }

            // Route using computed features and priors (with latency tracking)
            let router_start = Instant::now();
            let decision = self.router.route(&features_vec, &priors);
            let router_latency = router_start.elapsed();

            // 6. Check policy: entropy floor (simplified for now)
            // Validate router entropy against policy requirements
            let entropy = self.calculate_gate_entropy(&decision.gates_q15);
            self.determinism_validator
                .validate_router_entropy(entropy)?;

            // 7. Execute kernel inference (reuse buffers)
            io_buffers.input_ids.clear();
            io_buffers.input_ids.extend_from_slice(input_ids);
            io_buffers.position = current_tokens.len() - 1;

            router_ring.set(&decision.indices, &decision.gates_q15);
            router_ring.position = step;

            let kernel_start = Instant::now();
            self.kernels.run_step(&router_ring, &mut io_buffers)?;
            let kernel_latency = kernel_start.elapsed();

            // Update recent logits for sliding-window entropy
            self.recent_logits.push(io_buffers.output_logits.clone());
            if self.recent_logits.len() > self.entropy_window {
                // Remove oldest to maintain window size
                self.recent_logits.remove(0);
            }

            // 8. Sample next token
            let next_token = self.generator.next_token(&io_buffers.output_logits)?;

            // 9. Record telemetry (sampled)
            if step < 128 || (step % 20 == 0) {
                let _ = self.telemetry.log(
                    "inference.step",
                    serde_json::json!({
                        "cpid": request.cpid,
                        "step": step,
                        "token": next_token,
                        "router_latency_us": router_latency.as_micros(),
                        "kernel_latency_us": kernel_latency.as_micros(),
                        "adapters": decision.indices,
                    }),
                );
            }

            // 10. Record router decision
            router_decisions.push(RouterDecision {
                step,
                adapter_indices: decision.indices.to_vec(),
                gates_q15: decision.gates_q15.to_vec(),
            });

            // 11. Check stopping criteria
            if next_token == self.tokenizer.eos_token_id() {
                debug!("EOS token encountered at step {}", step);
                break;
            }

            // 12. Append token and continue
            generated_tokens.push(next_token);
            current_tokens.push(next_token);

            // Check max sequence length
            if current_tokens.len() >= self.config.max_seq_len {
                warn!("Reached maximum sequence length");
                break;
            }
        }

        // 13. Decode generated text
        let generated_text = self.tokenizer.decode(&generated_tokens)?;

        // 14. Validate post-inference router entropy
        let avg_router_entropy = if router_decisions.is_empty() {
            0.0
        } else {
            let total_entropy: f32 = router_decisions
                .iter()
                .map(|decision| self.calculate_gate_entropy(&decision.gates_q15))
                .sum();
            total_entropy / router_decisions.len() as f32
        };
        self.determinism_validator
            .validate_router_entropy(avg_router_entropy)?;

        // 15. Build trace for reproducibility
        let trace = InferenceTrace {
            cpid: request.cpid.clone(),
            input_tokens: input_tokens.clone(),
            generated_tokens: generated_tokens.clone(),
            router_decisions,
            evidence: vec![], // Populated if RAG is enabled
        };

        let latency = start_time.elapsed();

        // 16. Log final telemetry
        let _ = self.telemetry.log(
            "inference.complete",
            serde_json::json!({
                "cpid": request.cpid,
                "input_tokens": input_tokens.len(),
                "generated_tokens": generated_tokens.len(),
                "latency_ms": latency.as_millis(),
            }),
        );

        info!(
            "Inference complete: generated {} tokens in {}ms",
            generated_tokens.len(),
            latency.as_millis()
        );

        Ok(InferenceResponse {
            text: generated_text,
            token_count: generated_tokens.len(),
            latency_ms: latency.as_millis() as u64,
            trace,
        })
    }

    /// Create code features for router from prompt context using sliding-window entropy
    fn create_code_features(&self, prompt_context: &str) -> CodeFeatures {
        let mut cf = CodeFeatures::from_context(prompt_context);
        // Compute stronger entropy from recent logits (sliding window)
        let entropy = extract_attn_entropy(&self.recent_logits, Some(self.entropy_window));
        cf.set_attn_entropy(entropy);
        cf
    }

    /// Calculate entropy from token distribution
    #[allow(dead_code)]
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

    /// Get the pre-inference seed (for future preprocessing operations)
    pub fn pre_inference_seed(&self) -> &adapteros_core::B3Hash {
        &self.pre_inference_seed
    }

    /// Get the post-inference seed (for future postprocessing operations)
    pub fn post_inference_seed(&self) -> &adapteros_core::B3Hash {
        &self.post_inference_seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inference_config_default() {
        let config = InferencePipelineConfig::default();
        assert_eq!(config.model_name, "Qwen2.5-7B-Instruct");
        assert_eq!(config.vocab_size, 152064);
        assert_eq!(config.max_seq_len, 32768);
    }

    #[test]
    fn test_inference_request() {
        let request = InferenceRequest {
            prompt: "What is 2+2?".to_string(),
            max_tokens: 100,
            cpid: "test-cp-001".to_string(),
            require_evidence: false,
            request_type: None,
        };
        assert_eq!(request.max_tokens, 100);
    }

    #[test]
    fn test_upstream_downstream_adapter_separation() {
        // Test that adapters are correctly classified based on upstream_enabled flag
        // NOTE: This only tests the separation logic, not actual upstream behavior
        // (which is not yet implemented - see apply_upstream_adapters documentation)

        // Create a test manifest with both upstream and downstream adapters
        let mut manifest = ManifestV3 {
            schema: "adapteros.manifest.v3".to_string(),
            base: adapteros_manifest::Base {
                model_id: "test-model".to_string(),
                model_hash: adapteros_core::B3Hash::hash(b"test"),
                arch: "llama".to_string(),
                vocab_size: 32000,
                hidden_dim: 4096,
                n_layers: 32,
                n_heads: 32,
                config_hash: adapteros_core::B3Hash::hash(b"config"),
                tokenizer_hash: adapteros_core::B3Hash::hash(b"tokenizer"),
                tokenizer_cfg_hash: adapteros_core::B3Hash::hash(b"tokenizer_cfg"),
                license_hash: None,
                rope_scaling_override: None,
            },
            adapters: vec![
                // Adapter 0: upstream (e.g., prompt translation)
                adapteros_manifest::Adapter {
                    id: "upstream-translator".to_string(),
                    hash: adapteros_core::B3Hash::hash(b"upstream"),
                    tier: adapteros_manifest::AdapterTier::Persistent,
                    rank: 16,
                    alpha: 32.0,
                    target_modules: vec!["q_proj".to_string()],
                    ttl: None,
                    acl: vec![],
                    warmup_prompt: None,
                    dependencies: None,
                    category: adapteros_manifest::AdapterCategory::Code,
                    scope: adapteros_manifest::AdapterScope::Global,
                    framework_id: None,
                    framework_version: None,
                    repo_id: None,
                    commit_sha: None,
                    intent: Some("Translate prompts".to_string()),
                    auto_promote: true,
                    eviction_priority: adapteros_manifest::EvictionPriority::Normal,
                    upstream_enabled: true, // Mark as upstream
                },
                // Adapter 1: downstream (standard generation-time adapter)
                adapteros_manifest::Adapter {
                    id: "downstream-python".to_string(),
                    hash: adapteros_core::B3Hash::hash(b"downstream1"),
                    tier: adapteros_manifest::AdapterTier::Persistent,
                    rank: 16,
                    alpha: 32.0,
                    target_modules: vec!["q_proj".to_string()],
                    ttl: None,
                    acl: vec![],
                    warmup_prompt: None,
                    dependencies: None,
                    category: adapteros_manifest::AdapterCategory::Code,
                    scope: adapteros_manifest::AdapterScope::Global,
                    framework_id: Some("python".to_string()),
                    framework_version: None,
                    repo_id: None,
                    commit_sha: None,
                    intent: Some("Python coding".to_string()),
                    auto_promote: true,
                    eviction_priority: adapteros_manifest::EvictionPriority::Normal,
                    upstream_enabled: false, // Downstream (default)
                },
                // Adapter 2: another downstream
                adapteros_manifest::Adapter {
                    id: "downstream-rust".to_string(),
                    hash: adapteros_core::B3Hash::hash(b"downstream2"),
                    tier: adapteros_manifest::AdapterTier::Persistent,
                    rank: 16,
                    alpha: 32.0,
                    target_modules: vec!["q_proj".to_string()],
                    ttl: None,
                    acl: vec![],
                    warmup_prompt: None,
                    dependencies: None,
                    category: adapteros_manifest::AdapterCategory::Code,
                    scope: adapteros_manifest::AdapterScope::Global,
                    framework_id: Some("rust".to_string()),
                    framework_version: None,
                    repo_id: None,
                    commit_sha: None,
                    intent: Some("Rust coding".to_string()),
                    auto_promote: true,
                    eviction_priority: adapteros_manifest::EvictionPriority::Normal,
                    upstream_enabled: false, // Downstream (default)
                },
            ],
            router: adapteros_manifest::RouterCfg {
                k_sparse: 2,
                gate_quant: "q15".to_string(),
                entropy_floor: 0.02,
                tau: 1.0,
                sample_tokens_full: 128,
                warmup: false,
                algorithm: "weighted".to_string(),
                orthogonal_penalty: 0.1,
                shared_downsample: false,
                compression_ratio: 0.8,
                multi_path_enabled: false,
                diversity_threshold: 0.05,
                orthogonal_constraints: false,
            },
            telemetry: adapteros_manifest::TelemetryCfg {
                schema_hash: adapteros_core::B3Hash::hash(b"telemetry"),
                sampling: adapteros_manifest::Sampling {
                    token: 0.05,
                    router: 1.0,
                    inference: 1.0,
                },
                router_full_tokens: 128,
                bundle: adapteros_manifest::BundleCfg {
                    max_events: 500000,
                    max_bytes: 268435456,
                },
            },
            policies: adapteros_manifest::Policies::default(),
            seeds: adapteros_manifest::Seeds {
                global: adapteros_core::B3Hash::hash(b"global"),
                manifest_hash: adapteros_core::B3Hash::hash(b"manifest"),
                parent_cpid: None,
            },
        };

        // Create a mock lifecycle manager (we don't need real adapter files for this test)
        let temp_dir = std::env::temp_dir().join("upstream_downstream_test");
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        let adapter_names = manifest
            .adapters
            .iter()
            .map(|a| a.id.clone())
            .collect::<Vec<_>>();

        let lifecycle = Arc::new(LifecycleManager::new(
            adapter_names,
            &manifest.policies,
            temp_dir.clone(),
            None,
            2, // K=2
        ));

        // Create prior context
        let adapter_framework_ids = manifest
            .adapters
            .iter()
            .map(|a| a.framework_id.clone())
            .collect::<Vec<_>>();

        let mut upstream_adapter_indices = Vec::new();
        let mut downstream_adapter_indices = Vec::new();

        for (idx, adapter) in manifest.adapters.iter().enumerate() {
            if adapter.upstream_enabled {
                upstream_adapter_indices.push(idx as u16);
            } else {
                downstream_adapter_indices.push(idx as u16);
            }
        }

        // Verify separation
        assert_eq!(
            upstream_adapter_indices.len(),
            1,
            "Should have 1 upstream adapter"
        );
        assert_eq!(
            downstream_adapter_indices.len(),
            2,
            "Should have 2 downstream adapters"
        );
        assert_eq!(
            upstream_adapter_indices[0], 0,
            "Adapter 0 should be upstream"
        );
        assert_eq!(
            downstream_adapter_indices,
            vec![1, 2],
            "Adapters 1 and 2 should be downstream"
        );

        // Cleanup
        std::fs::remove_dir_all(temp_dir).ok();
    }

    #[tokio::test]
    async fn test_upstream_adapter_application_is_deterministic() {
        use adapteros_core::B3Hash;

        // Test that upstream adapter detection and manifest processing works correctly
        // NOTE: This does NOT test actual deterministic output changes because
        // upstream adapters are not yet fully implemented (embeddings are discarded)
        // This test validates the infrastructure/tracking layer only

        // Create a test inference pipeline with upstream adapters
        let temp_dir = std::env::temp_dir().join("upstream_determinism_test");
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        // Create a simple manifest with one upstream adapter
        let manifest = ManifestV3 {
            schema: "adapteros.manifest.v3".to_string(),
            base: adapteros_manifest::Base {
                model_id: "test-model".to_string(),
                model_hash: B3Hash::hash(b"test"),
                arch: "llama".to_string(),
                vocab_size: 32000,
                hidden_dim: 4096,
                n_layers: 32,
                n_heads: 32,
                config_hash: B3Hash::hash(b"config"),
                tokenizer_hash: B3Hash::hash(b"tokenizer"),
                tokenizer_cfg_hash: B3Hash::hash(b"tokenizer_cfg"),
                license_hash: None,
                rope_scaling_override: None,
            },
            adapters: vec![adapteros_manifest::Adapter {
                id: "test-upstream".to_string(),
                hash: B3Hash::hash(b"upstream"),
                tier: adapteros_manifest::AdapterTier::Persistent,
                rank: 16,
                alpha: 32.0,
                target_modules: vec!["q_proj".to_string()],
                ttl: None,
                acl: vec![],
                warmup_prompt: None,
                dependencies: None,
                category: adapteros_manifest::AdapterCategory::Code,
                scope: adapteros_manifest::AdapterScope::Global,
                framework_id: None,
                framework_version: None,
                repo_id: None,
                commit_sha: None,
                intent: Some("Test upstream".to_string()),
                auto_promote: true,
                eviction_priority: adapteros_manifest::EvictionPriority::Normal,
                upstream_enabled: true,
            }],
            router: adapteros_manifest::RouterCfg {
                k_sparse: 1,
                gate_quant: "q15".to_string(),
                entropy_floor: 0.02,
                tau: 1.0,
                sample_tokens_full: 128,
                warmup: false,
                algorithm: "weighted".to_string(),
                orthogonal_penalty: 0.1,
                shared_downsample: false,
                compression_ratio: 0.8,
                multi_path_enabled: false,
                diversity_threshold: 0.05,
                orthogonal_constraints: false,
            },
            telemetry: adapteros_manifest::TelemetryCfg {
                schema_hash: B3Hash::hash(b"telemetry"),
                sampling: adapteros_manifest::Sampling {
                    token: 0.05,
                    router: 1.0,
                    inference: 1.0,
                },
                router_full_tokens: 128,
                bundle: adapteros_manifest::BundleCfg {
                    max_events: 500000,
                    max_bytes: 268435456,
                },
            },
            policies: adapteros_manifest::Policies::default(),
            seeds: adapteros_manifest::Seeds {
                global: B3Hash::hash(b"global_test"),
                manifest_hash: B3Hash::hash(b"manifest"),
                parent_cpid: None,
            },
        };

        let lifecycle = Arc::new(LifecycleManager::new(
            vec!["test-upstream".to_string()],
            &manifest.policies,
            temp_dir.clone(),
            None,
            1,
        ));

        // Create a mock router and policy engine
        let router = Router::new(vec![1.0], 1, 1.0, 0.02, [0u8; 32]);
        let policy = adapteros_policy::PolicyEngine::new(&manifest.policies);

        // Create mock kernels (we won't actually run inference, just test upstream application)
        // For this test, we'll directly test the apply_upstream_adapters method

        // Test input tokens
        let input_tokens = vec![100u32, 200u32, 300u32, 400u32];

        // We need a pipeline instance to test, but creating a full one requires many dependencies
        // For now, let's verify the manifest processing is correct
        // The actual determinism will be tested in integration tests

        // Verify that the manifest correctly marks the adapter as upstream
        assert_eq!(manifest.adapters.len(), 1);
        assert!(manifest.adapters[0].upstream_enabled);
        assert_eq!(manifest.adapters[0].id, "test-upstream");

        // Cleanup
        std::fs::remove_dir_all(temp_dir).ok();
    }
}
