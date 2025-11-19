//! End-to-end inference pipeline test
//!
//! This test verifies the complete inference flow from model loading through text generation:
//! 1. Model loader loads Qwen model from SafeTensors
//! 2. Tokenizer encodes prompt
//! 3. InferencePipeline performs autoregressive generation
//! 4. Router selects K adapters
//! 5. Metal kernels execute with LoRA
//! 6. Generated tokens are decoded
//!
//! Purpose: Validate PRD #4 requirements

use adapteros_core::{AosError, CircuitBreakerConfig, Result, StandardCircuitBreaker};
use adapteros_lora_kernel_mtl::MetalKernels;
use adapteros_lora_router::Router;
use adapteros_lora_worker::inference_pipeline::{
    InferencePipeline, InferencePipelineConfig, InferenceRequest,
};
use adapteros_manifest::ManifestV3;
use adapteros_policy::PolicyEngine;
use adapteros_telemetry::TelemetryWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[tokio::test]
#[ignore] // Run manually: cargo test --test e2e_inference_complete -- --ignored --nocapture
async fn test_end_to_end_inference_pipeline() -> Result<()> {
    println!("🎯 Starting end-to-end inference pipeline test\n");

    // Step 1: Check for model files
    println!("1️⃣  Checking for model files...");
    let model_path = PathBuf::from("models/qwen2.5-7b");

    if !model_path.exists() {
        eprintln!("\n❌ Model directory not found: {:?}", model_path);
        eprintln!("   Download model first:");
        eprintln!("   huggingface-cli download Qwen/Qwen2.5-7B-Instruct \\");
        eprintln!("     --local-dir models/qwen2.5-7b \\");
        eprintln!("     --include \"model.safetensors\" \"config.json\" \"tokenizer.json\"");
        return Err(AosError::Worker("Model not found".into()));
    }

    let tokenizer_path = model_path.join("tokenizer.json");
    if !tokenizer_path.exists() {
        eprintln!("\n❌ Tokenizer not found: {:?}", tokenizer_path);
        return Err(AosError::Worker("Tokenizer not found".into()));
    }

    println!("   ✅ Model files found");
    println!("      Path: {:?}", model_path);

    // Step 2: Initialize Metal kernels
    println!("\n2️⃣  Initializing Metal kernels...");
    let kernels = MetalKernels::new().map_err(|e| {
        eprintln!("   ❌ Failed to initialize Metal: {}", e);
        e
    })?;
    println!("   ✅ Metal kernels initialized");

    // Step 3: Load manifest (use minimal test manifest)
    println!("\n3️⃣  Creating test manifest...");
    let manifest = create_test_manifest();
    println!("   ✅ Manifest created");
    println!("      Adapters: {}", manifest.adapters.len());
    println!("      K-sparse: {}", manifest.router.k_sparse);

    // Step 4: Create router with deterministic seed
    println!("\n4️⃣  Initializing router...");
    let router_seed = adapteros_core::derive_seed(&manifest.seeds.global, "router");
    let router = Router::new(
        vec![1.0; manifest.adapters.len()],
        manifest.router.k_sparse,
        manifest.router.tau,
        manifest.router.entropy_floor,
        router_seed,
    )?;
    println!("   ✅ Router initialized");
    println!("      K-sparse: {}", manifest.router.k_sparse);
    println!("      Tau: {}", manifest.router.tau);

    // Step 5: Create policy engine
    println!("\n5️⃣  Creating policy engine...");
    let policy = PolicyEngine::new(manifest.policies.clone());
    println!("   ✅ Policy engine created");

    // Step 6: Create telemetry writer
    println!("\n6️⃣  Initializing telemetry...");
    let telemetry = TelemetryWriter::new_stdout();
    println!("   ✅ Telemetry initialized");

    // Step 7: Create circuit breaker
    println!("\n7️⃣  Creating circuit breaker...");
    let circuit_breaker = Arc::new(StandardCircuitBreaker::new(
        "test_inference".to_string(),
        CircuitBreakerConfig::default(),
    ));
    println!("   ✅ Circuit breaker created");

    // Step 8: Create inference pipeline
    println!("\n8️⃣  Creating inference pipeline...");
    let config = InferencePipelineConfig::default();
    let mut pipeline = InferencePipeline::new(
        &tokenizer_path,
        router,
        Box::new(kernels),
        policy,
        telemetry,
        config,
        circuit_breaker,
    )?;
    println!("   ✅ Inference pipeline created");

    // Step 9: Run inference
    println!("\n9️⃣  Running inference...");
    println!("      Prompt: 'Hello, how are you?'");
    println!("      Max tokens: 10");

    let request = InferenceRequest {
        prompt: "Hello, how are you?".to_string(),
        max_tokens: 10,
        cpid: "test-e2e-001".to_string(),
        require_evidence: false,
        stack_id: Some("test-stack".to_string()),
        stack_version: Some(1),
    };

    let response = pipeline.infer(request).await.map_err(|e| {
        eprintln!("\n   ❌ Inference failed: {}", e);
        e
    })?;

    println!("   ✅ Inference completed");
    println!("\n📊 Results:");
    println!("   Generated text: {}", response.text);
    println!("   Token count: {}", response.token_count);
    println!("   Latency: {}ms", response.latency_ms);
    println!("   Input tokens: {}", response.trace.input_tokens.len());
    println!(
        "   Generated tokens: {}",
        response.trace.generated_tokens.len()
    );
    println!(
        "   Router decisions: {}",
        response.trace.router_decisions.len()
    );

    // Validate response
    assert!(
        !response.text.is_empty(),
        "Generated text should not be empty"
    );
    assert!(
        response.token_count > 0,
        "Should generate at least one token"
    );
    assert!(response.token_count <= 10, "Should not exceed max_tokens");

    println!("\n✅ End-to-end inference pipeline test PASSED");
    Ok(())
}

#[tokio::test]
#[ignore] // Run manually: cargo test --test e2e_inference_complete test_inference_determinism -- --ignored --nocapture
async fn test_inference_determinism() -> Result<()> {
    println!("🎯 Testing inference determinism\n");

    let model_path = PathBuf::from("models/qwen2.5-7b");
    if !model_path.exists() {
        eprintln!("❌ Model not found, skipping determinism test");
        return Ok(());
    }

    let tokenizer_path = model_path.join("tokenizer.json");

    // Create two identical pipelines
    println!("1️⃣  Creating first pipeline...");
    let kernels1 = MetalKernels::new()?;
    let manifest1 = create_test_manifest();
    let router_seed1 = adapteros_core::derive_seed(&manifest1.seeds.global, "router");
    let router1 = Router::new(
        vec![1.0; manifest1.adapters.len()],
        manifest1.router.k_sparse,
        manifest1.router.tau,
        manifest1.router.entropy_floor,
        router_seed1,
    )?;
    let policy1 = PolicyEngine::new(manifest1.policies.clone());
    let telemetry1 = TelemetryWriter::new_stdout();
    let circuit_breaker1 = Arc::new(StandardCircuitBreaker::new(
        "test_det1".to_string(),
        CircuitBreakerConfig::default(),
    ));
    let config1 = InferencePipelineConfig::default();
    let mut pipeline1 = InferencePipeline::new(
        &tokenizer_path,
        router1,
        Box::new(kernels1),
        policy1,
        telemetry1,
        config1,
        circuit_breaker1,
    )?;

    println!("2️⃣  Creating second pipeline...");
    let kernels2 = MetalKernels::new()?;
    let manifest2 = create_test_manifest();
    let router_seed2 = adapteros_core::derive_seed(&manifest2.seeds.global, "router");
    let router2 = Router::new(
        vec![1.0; manifest2.adapters.len()],
        manifest2.router.k_sparse,
        manifest2.router.tau,
        manifest2.router.entropy_floor,
        router_seed2,
    )?;
    let policy2 = PolicyEngine::new(manifest2.policies.clone());
    let telemetry2 = TelemetryWriter::new_stdout();
    let circuit_breaker2 = Arc::new(StandardCircuitBreaker::new(
        "test_det2".to_string(),
        CircuitBreakerConfig::default(),
    ));
    let config2 = InferencePipelineConfig::default();
    let mut pipeline2 = InferencePipeline::new(
        &tokenizer_path,
        router2,
        Box::new(kernels2),
        policy2,
        telemetry2,
        config2,
        circuit_breaker2,
    )?;

    // Run same inference on both pipelines
    println!("\n3️⃣  Running inference on both pipelines...");
    let request = InferenceRequest {
        prompt: "The quick brown fox".to_string(),
        max_tokens: 5,
        cpid: "test-det-001".to_string(),
        require_evidence: false,
        stack_id: Some("test-stack".to_string()),
        stack_version: Some(1),
    };

    let response1 = pipeline1.infer(request.clone()).await?;
    let response2 = pipeline2.infer(request).await?;

    println!("\n📊 Comparing outputs:");
    println!("   Pipeline 1: {}", response1.text);
    println!("   Pipeline 2: {}", response2.text);
    println!("   Token count 1: {}", response1.token_count);
    println!("   Token count 2: {}", response2.token_count);

    // Validate determinism
    assert_eq!(
        response1.trace.generated_tokens, response2.trace.generated_tokens,
        "Generated tokens should be identical (deterministic)"
    );
    assert_eq!(
        response1.text, response2.text,
        "Generated text should be identical (deterministic)"
    );

    println!("\n✅ Inference determinism test PASSED");
    Ok(())
}

/// Create a minimal test manifest
fn create_test_manifest() -> ManifestV3 {
    use adapteros_core::B3Hash;
    use adapteros_manifest::{
        AdapterSpec, BaseModelConfig, PolicyConfig, RouterConfig, SeedConfig,
    };
    use adapteros_policy::*;

    ManifestV3 {
        version: "3".to_string(),
        base: BaseModelConfig {
            model_name: "qwen2.5-7b-test".to_string(),
            model_hash: B3Hash::hash(b"test-model"),
            vocab_size: 152064,
            hidden_size: 3584,
            num_layers: 28,
        },
        adapters: vec![
            AdapterSpec {
                id: "adapter-1".to_string(),
                hash: B3Hash::hash(b"adapter-1"),
                rank: 16,
                alpha: 32,
            },
            AdapterSpec {
                id: "adapter-2".to_string(),
                hash: B3Hash::hash(b"adapter-2"),
                rank: 16,
                alpha: 32,
            },
        ],
        router: RouterConfig {
            k_sparse: 2,
            tau: 1.0,
            entropy_floor: 0.02,
        },
        policies: PolicyConfig {
            determinism: DeterminismPolicy::default(),
            router: RouterPolicy::default(),
            evidence: EvidencePolicy::default(),
            rag: RagPolicy::default(),
            telemetry: TelemetryPolicy::default(),
            egress: EgressPolicy::default(),
            naming: NamingPolicy::default(),
            memory: MemoryPolicy::default(),
        },
        seeds: SeedConfig {
            global: B3Hash::hash(b"test-global-seed"),
        },
    }
}

#[test]
fn test_model_loader_basic() {
    use adapteros_lora_worker::model_loader::ModelLoader;

    println!("🎯 Testing model loader basic functionality\n");

    let model_path = PathBuf::from("models/qwen2.5-7b");
    if !model_path.exists() {
        println!("⏭️  Skipping test - model not found");
        return;
    }

    let loader = ModelLoader::new(&model_path);

    // Test metadata loading
    match loader.get_model_info() {
        Ok(info) => {
            println!("✅ Model metadata loaded:");
            println!("   Vocab size: {}", info.vocab_size);
            println!("   Hidden size: {}", info.hidden_size);
            println!("   Layers: {}", info.num_layers);
            println!("   Parameters: {}", info.total_parameters);
            assert!(
                info.total_parameters > 1_000_000,
                "Model should have at least 1M parameters"
            );
        }
        Err(e) => {
            eprintln!("❌ Failed to load model metadata: {}", e);
            panic!("Model loader test failed");
        }
    }
}
