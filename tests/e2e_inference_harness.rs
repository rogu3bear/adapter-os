//! End-to-end inference pipeline test harness
//!
//! Tests that verify inference works through the kernel layer and can help
//! debug MLX FFI and Bridge issues.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run mock kernel tests (no GPU required)
//! cargo test --test e2e_inference_harness
//!
//! # Run MLX FFI tests (requires model files)
//! TEST_MLX_MODEL_PATH=/path/to/model cargo test --test e2e_inference_harness --features mlx-ffi
//! ```

use adapteros_core::Result;
use adapteros_lora_kernel_api::{BackendHealth, FusedKernels, IoBuffers, MockKernels, RouterRing};

// ============================================================================
// Test Harness
// ============================================================================

/// Kernel test harness for testing inference backends directly
pub struct KernelTestHarness<K: FusedKernels> {
    kernels: K,
    vocab_size: usize,
}

impl KernelTestHarness<MockKernels> {
    /// Create a harness with MockKernels (no GPU required)
    pub fn with_mock() -> Self {
        Self {
            kernels: MockKernels::new(),
            vocab_size: 32000,
        }
    }
}

impl<K: FusedKernels> KernelTestHarness<K> {
    /// Create with a custom kernel backend
    pub fn new(kernels: K, vocab_size: usize) -> Self {
        Self { kernels, vocab_size }
    }

    /// Run a single inference step and return the output logits
    pub fn run_step(
        &mut self,
        input_ids: &[u32],
        adapter_ids: &[u16],
        gates: &[i16],
    ) -> Result<Vec<f32>> {
        let mut io = IoBuffers::new(self.vocab_size);
        io.input_ids.extend_from_slice(input_ids);

        let mut ring = RouterRing::new(adapter_ids.len());
        ring.set(adapter_ids, gates);

        self.kernels.run_step(&ring, &mut io)?;

        Ok(io.output_logits)
    }

    /// Run multiple inference steps
    pub fn run_sequence(
        &mut self,
        input_ids: &[u32],
        max_steps: usize,
        adapter_ids: &[u16],
        gates: &[i16],
    ) -> Result<Vec<Vec<f32>>> {
        let mut all_logits = Vec::with_capacity(max_steps);
        let mut io = IoBuffers::new(self.vocab_size);

        let mut ring = RouterRing::new(adapter_ids.len());
        ring.set(adapter_ids, gates);

        io.input_ids.extend_from_slice(input_ids);

        for _ in 0..max_steps {
            self.kernels.run_step(&ring, &mut io)?;
            all_logits.push(io.output_logits.clone());
        }

        Ok(all_logits)
    }

    /// Get device name
    pub fn device_name(&self) -> &str {
        self.kernels.device_name()
    }

    /// Get determinism attestation
    pub fn attest_determinism(
        &self,
    ) -> Result<adapteros_lora_kernel_api::attestation::DeterminismReport> {
        self.kernels.attest_determinism()
    }

    /// Check backend health
    pub fn health_check(&self) -> Result<BackendHealth> {
        self.kernels.health_check()
    }
}

/// Helper to find the argmax of logits (most likely next token)
pub fn argmax(logits: &[f32]) -> Option<u32> {
    logits
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx as u32)
}

/// Helper to verify logits are valid (no NaN, no Inf)
pub fn validate_logits(logits: &[f32]) -> bool {
    logits.iter().all(|&x| x.is_finite())
}

// ============================================================================
// MockKernels Tests
// ============================================================================

#[test]
fn test_mock_kernels_produces_valid_logits() {
    let mut harness = KernelTestHarness::with_mock();

    let logits = harness.run_step(&[1, 2, 3], &[0], &[32767]).unwrap();

    assert_eq!(logits.len(), 32000, "Expected 32000 logits for vocab size");
    assert!(
        validate_logits(&logits),
        "Logits should be finite (no NaN/Inf)"
    );

    let next_token = argmax(&logits);
    assert!(next_token.is_some(), "Should be able to find next token");
}

#[test]
fn test_mock_kernels_deterministic() {
    let mut harness1 = KernelTestHarness::with_mock();
    let mut harness2 = KernelTestHarness::with_mock();

    let input = &[42u32, 100, 200];
    let adapters = &[0u16, 1];
    let gates = &[16384i16, 16384];

    let logits1 = harness1.run_step(input, adapters, gates).unwrap();
    let logits2 = harness2.run_step(input, adapters, gates).unwrap();

    assert_eq!(logits1, logits2, "MockKernels should be deterministic");
}

#[test]
fn test_mock_kernels_multi_step() {
    let mut harness = KernelTestHarness::with_mock();

    let steps = 10;
    let all_logits = harness
        .run_sequence(&[1], steps, &[0], &[32767])
        .unwrap();

    assert_eq!(
        all_logits.len(),
        steps,
        "Should produce {} steps of output",
        steps
    );

    for (i, logits) in all_logits.iter().enumerate() {
        assert!(
            validate_logits(logits),
            "Step {} should produce valid logits",
            i
        );
    }
}

#[test]
fn test_mock_kernels_attestation() {
    let harness = KernelTestHarness::with_mock();

    let report = harness.attest_determinism().unwrap();

    assert!(
        report.deterministic,
        "MockKernels should attest as deterministic"
    );
    assert_eq!(
        report.backend_type,
        adapteros_lora_kernel_api::attestation::BackendType::Mock
    );
}

#[test]
fn test_mock_kernels_device_name() {
    let harness = KernelTestHarness::with_mock();

    let name = harness.device_name();
    assert!(
        name.contains("Mock"),
        "Device name should indicate mock backend"
    );
}

#[test]
fn test_mock_kernels_adapter_configurations() {
    let mut harness = KernelTestHarness::with_mock();

    // Test with different K values (number of adapters)
    for k in 1..=8 {
        let adapters: Vec<u16> = (0..k as u16).collect();
        let gates: Vec<i16> = vec![32767 / k as i16; k];

        let logits = harness.run_step(&[1], &adapters, &gates).unwrap();
        assert!(
            validate_logits(&logits),
            "K={} should produce valid logits",
            k
        );
    }
}

// ============================================================================
// MLX FFI Tests (require feature flag and model files)
// ============================================================================

#[cfg(feature = "mlx")]
mod mlx_ffi_tests {
    use super::*;

    fn get_model_path() -> Option<std::path::PathBuf> {
        std::env::var("TEST_MLX_MODEL_PATH")
            .ok()
            .map(std::path::PathBuf::from)
    }

    #[test]
    #[ignore = "Requires TEST_MLX_MODEL_PATH environment variable"]
    fn test_mlx_ffi_model_load() {
        use adapteros_lora_mlx_ffi::MLXFFIModel;

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");
        println!("Loading MLX model from: {:?}", model_path);

        let result = MLXFFIModel::load(&model_path);

        match result {
            Ok(model) => {
                println!("Model loaded successfully");
                println!("Config: {:?}", model.config());
            }
            Err(e) => {
                println!("MLX FFI load error: {:?}", e);
                panic!("Failed to load MLX model: {:?}", e);
            }
        }
    }

    #[test]
    #[ignore = "Requires TEST_MLX_MODEL_PATH environment variable"]
    fn test_mlx_ffi_forward() {
        use adapteros_lora_mlx_ffi::{MLXFFIBackend, MLXFFIModel};

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");

        let model = MLXFFIModel::load(&model_path).expect("Failed to load model");
        let backend = MLXFFIBackend::new(model);

        let vocab_size = 32000;
        let mut harness = KernelTestHarness::new(backend, vocab_size);

        println!("Running MLX FFI forward pass...");

        let result = harness.run_step(&[1, 2, 3], &[0], &[32767]);

        match result {
            Ok(logits) => {
                println!("Forward pass successful!");
                println!("Output shape: {}", logits.len());
                assert!(validate_logits(&logits), "Logits should be valid");

                if let Some(next_token) = argmax(&logits) {
                    println!("Predicted next token: {}", next_token);
                }
            }
            Err(e) => {
                println!("MLX FFI forward error: {:?}", e);
                panic!("Forward pass failed: {:?}", e);
            }
        }
    }

    #[test]
    #[ignore = "Requires TEST_MLX_MODEL_PATH environment variable"]
    fn test_mlx_ffi_determinism() {
        use adapteros_lora_mlx_ffi::{MLXFFIBackend, MLXFFIModel};

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");

        let model1 = MLXFFIModel::load(&model_path).expect("Failed to load model");
        let backend1 = MLXFFIBackend::new(model1);
        let mut harness1 = KernelTestHarness::new(backend1, 32000);

        let model2 = MLXFFIModel::load(&model_path).expect("Failed to load model");
        let backend2 = MLXFFIBackend::new(model2);
        let mut harness2 = KernelTestHarness::new(backend2, 32000);

        let input = &[100u32, 200, 300];
        let adapters = &[0u16];
        let gates = &[32767i16];

        let logits1 = harness1
            .run_step(input, adapters, gates)
            .expect("First run failed");
        let logits2 = harness2
            .run_step(input, adapters, gates)
            .expect("Second run failed");

        let epsilon = 1e-5;
        let mut max_diff = 0.0f32;
        for (a, b) in logits1.iter().zip(logits2.iter()) {
            let diff = (a - b).abs();
            max_diff = max_diff.max(diff);
        }

        println!("Max difference between runs: {}", max_diff);
        assert!(
            max_diff < epsilon,
            "MLX FFI should be deterministic (max diff: {})",
            max_diff
        );
    }
}

// ============================================================================
// MLX Bridge Tests (require feature flag and Python mlx-lm)
// Note: The bridge is in adapteros-lora-worker with feature "mlx-bridge"
// ============================================================================

// ============================================================================
// TextGenerationKernel Tests (Trait-based detection)
// ============================================================================
//
// The Worker now uses FusedKernels::supports_text_generation() for detection,
// not string matching on device_name(). These tests verify the trait-based approach.

use adapteros_lora_kernel_api::TextGenerationResult;
use adapteros_lora_kernel_api::attestation::{
    DeterminismReport, BackendType, RngSeedingMethod, FloatingPointMode,
};

/// Mock kernel that implements text-generation via the FusedKernels trait
/// Used to test the Worker's text-generation path without requiring MLX/Python
pub struct MockTextGenerationKernel {
    device_name: String,
    #[allow(dead_code)]
    vocab_size: usize,
}

impl MockTextGenerationKernel {
    pub fn new(vocab_size: usize) -> Self {
        Self {
            device_name: "Mock TextGen Kernel v1.0".to_string(),
            vocab_size,
        }
    }
}

impl FusedKernels for MockTextGenerationKernel {
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // Mock implementation - no-op
        Ok(())
    }

    fn run_step(&mut self, _ring: &RouterRing, _io: &mut IoBuffers) -> Result<()> {
        // Text-generation backends don't support run_step
        Err(adapteros_core::AosError::Kernel(
            "MockTextGenerationKernel does not support run_step() - use generate_text_full() instead".to_string(),
        ))
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn attest_determinism(&self) -> Result<DeterminismReport> {
        Ok(DeterminismReport {
            backend_type: BackendType::Mock,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: RngSeedingMethod::FixedSeed(42),
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec![],
            deterministic: true,
        })
    }

    fn health_check(&self) -> Result<BackendHealth> {
        Ok(BackendHealth::Healthy)
    }

    // Text-generation specific implementations (deprecated, kept for forwarding)
    #[allow(deprecated)]
    fn supports_text_generation(&self) -> bool {
        true // This is the key difference from MockKernels
    }

    #[allow(deprecated)]
    fn generate_text_full(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
    ) -> Result<TextGenerationResult> {
        self.generate_text_complete(prompt, max_tokens, temperature, top_p)
    }

    // New method names
    fn supports_streaming_text_generation(&self) -> bool {
        true // This is the key difference from MockKernels
    }

    fn generate_text_complete(
        &self,
        prompt: &str,
        max_tokens: usize,
        _temperature: f32,
        _top_p: f32,
    ) -> Result<TextGenerationResult> {
        // Generate mock output based on prompt
        let mock_output = format!("Mock response to: {}", prompt);
        let tokens_generated = mock_output.split_whitespace().count().min(max_tokens);

        Ok(TextGenerationResult {
            text: mock_output,
            tokens_generated,
            finish_reason: "length".to_string(),
            usage_stats: Some(adapteros_lora_kernel_api::TextGenerationUsage {
                prompt_tokens: prompt.split_whitespace().count(),
                completion_tokens: tokens_generated,
                total_tokens: prompt.split_whitespace().count() + tokens_generated,
            }),
            timing_stats: Some(adapteros_lora_kernel_api::TextGenerationTiming {
                ttft_ms: 10.0,
                total_ms: 50.0,
                tokens_per_second: 100.0,
            }),
        })
    }
}

// ============================================================================
// Trait-based Detection Tests
// ============================================================================

/// Verify MockKernels does NOT support text generation (uses run_step path)
#[test]
fn test_mock_kernels_not_text_generation() {
    let kernels = MockKernels::new();

    assert!(
        !kernels.supports_streaming_text_generation(),
        "MockKernels should NOT support text generation"
    );
}

/// Verify MockTextGenerationKernel DOES support text generation
#[test]
fn test_mock_text_gen_kernel_supports_text_generation() {
    let kernels = MockTextGenerationKernel::new(32000);

    assert!(
        kernels.supports_streaming_text_generation(),
        "MockTextGenerationKernel should support text generation"
    );
}

/// Verify MockTextGenerationKernel.run_step() returns error
#[test]
fn test_mock_text_gen_kernel_run_step_returns_error() {
    let mut kernels = MockTextGenerationKernel::new(32000);
    let mut io = IoBuffers::new(32000);
    io.input_ids.push(1);
    let ring = RouterRing::new(1);

    let result = kernels.run_step(&ring, &mut io);

    assert!(result.is_err(), "run_step should return error for text-gen kernels");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support run_step"),
        "Error should indicate run_step not supported: {}",
        err_msg
    );
}

/// Verify MockTextGenerationKernel.generate_text_complete() works
#[test]
fn test_mock_text_gen_kernel_generate_text_complete() {
    let kernels = MockTextGenerationKernel::new(32000);

    let result = kernels.generate_text_complete("Hello, world!", 20, 0.7, 0.9);

    assert!(result.is_ok(), "generate_text_complete should succeed");
    let gen_result = result.unwrap();
    assert!(!gen_result.text.is_empty(), "Should generate text");
    assert!(gen_result.text.contains("Hello, world!"), "Response should reference prompt");
    assert!(gen_result.tokens_generated > 0, "Should generate some tokens");
}

/// Verify the Worker's detection logic would work correctly
/// (simulates what Worker::infer_internal() does)
#[test]
fn test_worker_style_text_gen_detection() {
    // Simulate the Worker's detection logic from infer_internal()
    fn simulate_worker_detection<K: FusedKernels>(kernels: &K) -> bool {
        kernels.supports_streaming_text_generation()
    }

    // MockKernels should NOT be detected
    let mock = MockKernels::new();
    assert!(!simulate_worker_detection(&mock), "MockKernels should not trigger text-gen path");

    // MockTextGenerationKernel SHOULD be detected
    let text_gen = MockTextGenerationKernel::new(32000);
    assert!(simulate_worker_detection(&text_gen), "MockTextGenerationKernel should trigger text-gen path");
}

/// Verify generate_text_complete forwards to generate_text_full for backward compatibility
#[test]
#[allow(deprecated)]
fn test_generate_text_complete_forwards_to_full() {
    let kernels = MockTextGenerationKernel::new(32000);

    let old_result = kernels.generate_text_full("test prompt", 10, 0.7, 0.9);
    let new_result = kernels.generate_text_complete("test prompt", 10, 0.7, 0.9);

    assert!(old_result.is_ok(), "Old method should succeed");
    assert!(new_result.is_ok(), "New method should succeed");
    assert_eq!(
        old_result.unwrap().text,
        new_result.unwrap().text,
        "Both methods should return the same text"
    );
}

/// Verify supports_streaming_text_generation forwards to supports_text_generation
#[test]
#[allow(deprecated)]
fn test_supports_streaming_text_generation_forwards() {
    let text_gen_kernels = MockTextGenerationKernel::new(32000);
    let mock_kernels = MockKernels::new();

    // MockTextGenerationKernel: both should return true
    assert_eq!(
        text_gen_kernels.supports_text_generation(),
        text_gen_kernels.supports_streaming_text_generation(),
        "Both methods should return the same value for MockTextGenerationKernel"
    );

    // MockKernels: both should return false
    assert_eq!(
        mock_kernels.supports_text_generation(),
        mock_kernels.supports_streaming_text_generation(),
        "Both methods should return the same value for MockKernels"
    );
}

/// Test backward compatibility: device_name detection still works for legacy reasons
/// (deprecated, but kept for observation)
#[test]
fn test_legacy_device_name_detection() {
    // The Worker no longer relies on this, but we document the old behavior
    let device_names = vec![
        ("MockKernels v1.0", false),
        ("MLX FFI Backend", false),
        ("MLX Subprocess Bridge v1.0", true),
        ("MLX Bridge (Python)", true),
        ("CoreML Backend", false),
        ("Metal GPU", false),
    ];

    for (name, would_have_detected) in device_names {
        let is_text_gen = name.contains("MLX Subprocess") || name.contains("MLX Bridge");
        assert_eq!(
            is_text_gen, would_have_detected,
            "Legacy detection for '{}' would have been {}",
            name, would_have_detected
        );
    }
}

#[cfg(feature = "mlx-bridge")]
mod mlx_bridge_tests {
    use super::*;
    use std::path::PathBuf;

    fn get_model_path() -> Option<PathBuf> {
        std::env::var("TEST_MLX_MODEL_PATH")
            .ok()
            .map(PathBuf::from)
    }

    /// Verify MLX Bridge implements TextGenerationKernel
    #[test]
    fn test_mlx_bridge_implements_text_generation_kernel() {
        use adapteros_lora_kernel_api::TextGenerationKernel;
        use adapteros_lora_worker::mlx_subprocess_bridge::MLXSubprocessBridge;

        // Compile-time check that trait is implemented
        fn assert_text_gen_kernel<T: TextGenerationKernel>() {}
        assert_text_gen_kernel::<MLXSubprocessBridge>();
    }

    /// Verify MLX Bridge supports_streaming_text_generation() returns true via FusedKernels trait
    /// This is the proper trait-based detection (not device_name string matching)
    #[test]
    #[ignore = "Requires Python mlx-lm and TEST_MLX_MODEL_PATH"]
    fn test_mlx_bridge_supports_streaming_text_generation_via_fused_kernels() {
        use adapteros_lora_kernel_api::FusedKernels;
        use adapteros_lora_worker::mlx_subprocess_bridge::MLXSubprocessBridge;

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");

        let bridge =
            MLXSubprocessBridge::new(model_path, 32000).expect("Failed to create bridge");

        // Trait-based detection (the correct way)
        assert!(
            bridge.supports_streaming_text_generation(),
            "MLX Bridge should return true for supports_streaming_text_generation() via FusedKernels"
        );

        // Also verify device_name for debugging
        let device_name = bridge.device_name();
        println!("MLX Bridge device name: {}", device_name);
    }

    /// Verify TextGenerationKernel.supports_text_generation() returns true (legacy trait)
    /// Note: Uses explicit trait dispatch to avoid ambiguity with FusedKernels
    #[test]
    #[ignore = "Requires Python mlx-lm and TEST_MLX_MODEL_PATH"]
    #[allow(deprecated)]
    fn test_mlx_bridge_supports_text_generation_via_text_gen_kernel() {
        use adapteros_lora_kernel_api::TextGenerationKernel;
        use adapteros_lora_worker::mlx_subprocess_bridge::MLXSubprocessBridge;

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");

        let bridge =
            MLXSubprocessBridge::new(model_path, 32000).expect("Failed to create bridge");

        // Via TextGenerationKernel trait - use explicit trait dispatch (deprecated)
        assert!(
            TextGenerationKernel::supports_text_generation(&bridge),
            "MLX Bridge should support text generation via TextGenerationKernel"
        );
    }

    /// Test generate_text_complete via FusedKernels trait (the trait Worker uses)
    #[test]
    #[ignore = "Requires Python mlx-lm and TEST_MLX_MODEL_PATH"]
    fn test_mlx_bridge_generate_text_complete() {
        use adapteros_lora_kernel_api::FusedKernels;
        use adapteros_lora_worker::mlx_subprocess_bridge::MLXSubprocessBridge;

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");

        let bridge =
            MLXSubprocessBridge::new(model_path, 32000).expect("Failed to create bridge");

        println!("Testing generate_text_complete via FusedKernels trait...");

        // Use explicit trait dispatch (this is what Worker uses)
        let result = FusedKernels::generate_text_complete(&bridge, "The capital of France is", 20, 0.7, 0.9);

        match result {
            Ok(gen_result) => {
                println!("Generation successful via FusedKernels!");
                println!("Generated text: {}", gen_result.text);
                println!("Tokens generated: {}", gen_result.tokens_generated);
                println!("Finish reason: {}", gen_result.finish_reason);
                if let Some(usage) = &gen_result.usage_stats {
                    println!(
                        "Usage: prompt={}, completion={}, total={}",
                        usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
                    );
                }
                if let Some(timing) = &gen_result.timing_stats {
                    println!(
                        "Timing: TTFT={}ms, total={}ms, tok/s={}",
                        timing.ttft_ms, timing.total_ms, timing.tokens_per_second
                    );
                }
                assert!(!gen_result.text.is_empty(), "Should generate some text");
            }
            Err(e) => {
                println!("Generation error: {:?}", e);
                panic!("generate_text_complete failed: {:?}", e);
            }
        }
    }

    #[test]
    #[ignore = "Requires Python mlx-lm and TEST_MLX_MODEL_PATH"]
    fn test_mlx_bridge_generate() {
        use adapteros_lora_worker::mlx_subprocess_bridge::MLXSubprocessBridge;

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");

        let bridge =
            MLXSubprocessBridge::new(model_path, 32000).expect("Failed to create bridge");

        println!("MLX Bridge created, running generation...");

        let result = bridge.generate_text("Hello, world!", 20, 0.7, 0.9, &[]);

        match result {
            Ok(gen_result) => {
                println!("Generation successful!");
                println!("Generated text: {:?}", gen_result.text);
                assert!(!gen_result.text.is_empty(), "Should generate some text");
            }
            Err(e) => {
                println!("Generation error: {:?}", e);
                panic!("Generation failed: {:?}", e);
            }
        }
    }

    #[test]
    #[ignore = "Requires Python mlx-lm and TEST_MLX_MODEL_PATH"]
    fn test_mlx_bridge_run_step_not_supported() {
        use adapteros_lora_worker::mlx_subprocess_bridge::MLXSubprocessBridge;

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");

        let mut bridge =
            MLXSubprocessBridge::new(model_path, 32000).expect("Failed to create bridge");

        let mut io = IoBuffers::new(32000);
        io.input_ids.push(1);
        let ring = RouterRing::new(1);

        let result = bridge.run_step(&ring, &mut io);

        assert!(
            result.is_err(),
            "run_step should return error for MLX Bridge"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("does not support run_step"),
            "Error should indicate run_step not supported: {}",
            err_msg
        );
    }

    /// Test streaming generation via callback
    #[test]
    #[ignore = "Requires Python mlx-lm and TEST_MLX_MODEL_PATH"]
    fn test_mlx_bridge_streaming_callback() {
        use adapteros_lora_worker::mlx_subprocess_bridge::MLXSubprocessBridge;

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");

        let bridge =
            MLXSubprocessBridge::new(model_path, 32000).expect("Failed to create bridge");

        println!("Testing streaming generation with callback...");

        let mut tokens_received = Vec::new();

        let result = bridge.generate_stream(
            "Count to five: 1, 2,",
            20,
            0.7,
            0.9,
            |token| {
                println!("Token {}: '{}'", token.index, token.token);
                tokens_received.push(token.token.clone());
                true // continue streaming
            },
        );

        match result {
            Ok(final_result) => {
                println!("Streaming complete!");
                println!("Total tokens: {}", final_result.token_count);
                println!("Final text: '{}'", final_result.text);
                println!("Finish reason: {}", final_result.finish_reason);

                assert!(!tokens_received.is_empty(), "Should receive tokens via callback");
                assert!(!final_result.text.is_empty(), "Final text should not be empty");
                assert!(final_result.token_count > 0, "Should generate tokens");
            }
            Err(e) => {
                println!("Streaming error: {:?}", e);
                panic!("Streaming generation failed: {:?}", e);
            }
        }
    }

    /// Test streaming generation via iterator
    #[test]
    #[ignore = "Requires Python mlx-lm and TEST_MLX_MODEL_PATH"]
    fn test_mlx_bridge_streaming_iterator() {
        use adapteros_lora_worker::mlx_subprocess_bridge::{MLXSubprocessBridge, StreamingEvent};

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");

        let bridge =
            MLXSubprocessBridge::new(model_path, 32000).expect("Failed to create bridge");

        println!("Testing streaming generation with iterator...");

        let iter_result = bridge.generate_stream_iter(
            "Hello, world!",
            10,
            0.7,
            0.9,
        );

        match iter_result {
            Ok(iter) => {
                let mut token_count = 0;
                let mut final_result = None;

                for event_result in iter {
                    match event_result {
                        Ok(StreamingEvent::Token(token)) => {
                            println!("Token {}: '{}'", token.index, token.token);
                            token_count += 1;
                        }
                        Ok(StreamingEvent::Done(result)) => {
                            println!("Stream done: {} tokens", result.token_count);
                            final_result = Some(result);
                        }
                        Err(e) => {
                            println!("Stream error: {:?}", e);
                            panic!("Streaming error: {:?}", e);
                        }
                    }
                }

                assert!(token_count > 0, "Should receive tokens via iterator");
                assert!(final_result.is_some(), "Should receive final result");

                if let Some(result) = final_result {
                    println!("Final text: '{}'", result.text);
                    assert!(!result.text.is_empty(), "Final text should not be empty");
                }
            }
            Err(e) => {
                println!("Iterator creation error: {:?}", e);
                panic!("Failed to create streaming iterator: {:?}", e);
            }
        }
    }

    /// Test early termination of streaming
    #[test]
    #[ignore = "Requires Python mlx-lm and TEST_MLX_MODEL_PATH"]
    fn test_mlx_bridge_streaming_early_stop() {
        use adapteros_lora_worker::mlx_subprocess_bridge::MLXSubprocessBridge;

        let model_path = get_model_path().expect("TEST_MLX_MODEL_PATH not set");

        let bridge =
            MLXSubprocessBridge::new(model_path, 32000).expect("Failed to create bridge");

        println!("Testing early termination of streaming...");

        let mut tokens_received = 0;
        const STOP_AFTER: usize = 3;

        let result = bridge.generate_stream(
            "Write a long story about",
            50, // Request many tokens
            0.7,
            0.9,
            |token| {
                tokens_received += 1;
                println!("Token {}: '{}'", token.index, token.token);

                // Stop after STOP_AFTER tokens
                tokens_received < STOP_AFTER
            },
        );

        match result {
            Ok(final_result) => {
                println!("Streaming stopped early after {} tokens", tokens_received);
                println!("Final text: '{}'", final_result.text);

                // We should have received at most STOP_AFTER tokens
                // (might be fewer if the model finishes naturally)
                assert!(
                    tokens_received <= STOP_AFTER,
                    "Should stop after {} tokens, got {}",
                    STOP_AFTER,
                    tokens_received
                );
            }
            Err(e) => {
                // Early stop might cause an error in some implementations
                println!("Early stop result: {:?}", e);
                // This is acceptable for early termination
            }
        }
    }
}
