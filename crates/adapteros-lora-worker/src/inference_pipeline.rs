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
        let inference_seed_bytes = adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "inference");
        let pre_inference_seed_bytes = adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "pre_inference");
        let post_inference_seed_bytes = adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "post_inference");

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
        let inference_seed_bytes = adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "inference");
        let pre_inference_seed_bytes = adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "pre_inference");
        let post_inference_seed_bytes = adapteros_core::derive_seed(&B3Hash::from_bytes(worker_seed_bytes), "post_inference");

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

        self.prior_context = Some(PriorContext {
            adapter_framework_ids,
            lifecycle,
        });
        self
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
        self.determinism_validator.validate_router_entropy(avg_router_entropy)?;

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
}
