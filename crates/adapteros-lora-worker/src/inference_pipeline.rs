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

use adapteros_core::{AosError, CircuitBreaker, Result, StandardCircuitBreaker, TimeoutExt};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_router::Router;
use adapteros_policy::{PolicyEngine, QuarantineManager, QuarantineOperation};
use adapteros_telemetry::events::{RouterCandidate, RouterDecisionEvent};
use adapteros_telemetry::TelemetryWriter;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
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
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    /// Input prompt
    pub prompt: String,
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Control plane ID for tracing
    pub cpid: String,
    /// Whether to require evidence grounding
    pub require_evidence: bool,
    /// Stack ID for telemetry correlation (PRD-03)
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation (PRD-03)
    pub stack_version: Option<i64>,
}

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
    /// Stack ID for telemetry correlation (PRD-03)
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation (PRD-03)
    pub stack_version: Option<i64>,
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
    policy: PolicyEngine,
    /// Telemetry writer
    telemetry: TelemetryWriter,
    /// Configuration
    config: InferencePipelineConfig,
    /// Quarantine manager for policy hash enforcement
    quarantine_manager: Arc<Mutex<QuarantineManager>>,
    /// Circuit breaker for inference stability
    circuit_breaker: Arc<StandardCircuitBreaker>,
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
        circuit_breaker: Arc<StandardCircuitBreaker>,
    ) -> Result<Self> {
        // Validate backend determinism before constructing pipeline
        let report = kernels.attest_determinism()?;
        // TODO: Add validate_backend_attestation to policy engine
        // policy.determinism_policy().validate_backend_attestation(&report)?;

        info!("Backend determinism validated: {}", report.summary());

        let tokenizer = QwenTokenizer::from_file(tokenizer_path)?;

        // Create deterministic generator with seed
        let seed = [0u8; 32]; // TODO: Get from manifest or policy
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
            policy,
            telemetry,
            config,
            quarantine_manager,
            circuit_breaker,
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
        circuit_breaker: Arc<StandardCircuitBreaker>,
    ) -> Result<Self> {
        // Validate backend determinism before constructing pipeline
        let report = kernels.attest_determinism()?;
        // TODO: Add validate_backend_attestation to policy engine
        // policy.determinism_policy().validate_backend_attestation(&report)?;

        info!("Backend determinism validated: {}", report.summary());

        let tokenizer = QwenTokenizer::from_file(tokenizer_path)?;

        // Create deterministic generator with seed
        let seed = [0u8; 32]; // TODO: Get from manifest or policy
        let generator = Generator::new(seed)
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
            circuit_breaker,
        })
    }

    /// Run inference on a prompt with circuit breaker protection
    pub async fn infer(&mut self, request: InferenceRequest) -> Result<InferenceResponse> {
        // Use circuit breaker with timeout protection
        let timeout_duration = Duration::from_secs(30); // 30 second timeout for inference

        self.circuit_breaker.call(async {
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

            self.infer_inner(request, start_time).await
        }.with_timeout(timeout_duration)).await?
    }

    /// Internal inference implementation without circuit breaker
    async fn infer_inner(&mut self, request: InferenceRequest, start_time: Instant) -> Result<InferenceResponse> {
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
            let decision = self.router.route(&features, &priors);

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
            };
            let _ = self.telemetry.log_router_decision(router_event);

            // 6. Check policy: entropy floor (simplified for now)
            // TODO: Implement router entropy check in PolicyEngine
            let entropy = self.calculate_gate_entropy(&decision.gates_q15);
            if entropy < 0.02 {
                warn!("Router entropy below floor: {:.4}", entropy);
            }

            // 7. Execute kernel inference
            let mut io_buffers = IoBuffers {
                input_ids: input_ids.to_vec(),
                output_logits: vec![0.0; self.config.vocab_size],
                position: current_tokens.len() - 1,
            };

            let mut router_ring = RouterRing::from(&decision);
            router_ring.position = step;

            let kernel_start = Instant::now();
            self.kernels.run_step(&router_ring, &mut io_buffers)?;
            let kernel_latency = kernel_start.elapsed();

            // 8. Sample next token
            let next_token = self.generator.next_token(&io_buffers.output_logits)?;

            // 9. Record telemetry (sampled)
            if step < 128 || (step % 20 == 0) {
                self.telemetry.log(
                    "inference.step",
                    serde_json::json!({
                        "cpid": request.cpid,
                        "step": step,
                        "token": next_token,
                        "kernel_latency_us": kernel_latency.as_micros(),
                        "adapters": decision.indices,
                    }),
                );
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
            };

            if let Err(err) = self.telemetry.log_router_decision(event.clone()) {
                warn!("Failed to log router decision: {}", err);
            }

            router_decisions.push(event);

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

        // 14. Build trace for reproducibility
        let trace = InferenceTrace {
            cpid: request.cpid.clone(),
            input_tokens: input_tokens.clone(),
            generated_tokens: generated_tokens.clone(),
            router_decisions,
            evidence: vec![], // Populated if RAG is enabled
        };

        let latency = start_time.elapsed();

        // 15. Log final telemetry
        self.telemetry.log(
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
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
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
            stack_id: None,
            stack_version: None,
        };
        assert_eq!(request.max_tokens, 100);
    }
}
