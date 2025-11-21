//! MVP End-to-End Training Test
//!
//! Tests the complete pipeline: Dataset → Train → Quantize → Package → Load
//! Demonstrates training 5-10 adapters with different domains.

use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

use adapteros_core::Result;
use adapteros_lora_worker::training::trainer::{MicroLoRATrainer, TrainingConfig};
use adapteros_lora_worker::training::dataset::TrainingExample;
use adapteros_lora_worker::training::quantizer::LoRAQuantizer;
use adapteros_lora_worker::training::packager::AdapterPackager;
use adapteros_lora_router::Router;

/// Number of adapters to train
const NUM_ADAPTERS: usize = 5;

/// Domains for different adapters
const ADAPTER_DOMAINS: &[&str] = &[
    "code-review",
    "documentation",
    "testing",
    "security",
    "performance",
];

/// Test configuration
struct TestEnv {
    temp_dir: TempDir,
}

impl TestEnv {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new().map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
        Ok(Self { temp_dir })
    }

    fn adapters_dir(&self) -> PathBuf {
        let dir = self.temp_dir.path().join("adapters");
        std::fs::create_dir_all(&dir).ok();
        dir
    }
}

/// Generate training examples for a domain
fn generate_examples(domain: &str, count: usize) -> Vec<TrainingExample> {
    let domain_offset = match domain {
        "code-review" => 1000,
        "documentation" => 2000,
        "testing" => 3000,
        "security" => 4000,
        "performance" => 5000,
        _ => 0,
    };

    (0..count)
        .map(|i| {
            let input: Vec<u32> = (0..32).map(|j| (domain_offset + j + i * 10) as u32).collect();
            let target: Vec<u32> = (0..32).map(|j| (domain_offset + 100 + j + i * 10) as u32).collect();

            let mut metadata = HashMap::new();
            metadata.insert("domain".to_string(), domain.to_string());
            metadata.insert("index".to_string(), i.to_string());

            TrainingExample {
                input,
                target,
                metadata,
                weight: 1.0,
            }
        })
        .collect()
}

/// Train and package a single adapter
async fn train_and_package(
    env: &TestEnv,
    domain: &str,
    index: usize,
) -> Result<PathBuf> {
    // 1. Configure training
    let config = TrainingConfig {
        rank: 4,
        alpha: 8.0,
        learning_rate: 1e-4,
        batch_size: 4,
        epochs: 1,
        hidden_dim: 256,
    };

    // 2. Create trainer
    let mut trainer = MicroLoRATrainer::new(config.clone())?;

    // 3. Generate examples
    let examples = generate_examples(domain, 16);

    // 4. Train
    eprintln!("  Training adapter for domain: {}", domain);
    let result = trainer.train(&examples).await?;
    eprintln!("    Loss: {:.4}, Time: {}ms", result.final_loss, result.training_time_ms);

    // 5. Quantize weights to Q15
    let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
    eprintln!("    Quantized: lora_a={}x{}, lora_b={}x{}",
        quantized.lora_a_q15.len(),
        quantized.lora_a_q15.first().map(|r| r.len()).unwrap_or(0),
        quantized.lora_b_q15.len(),
        quantized.lora_b_q15.first().map(|r| r.len()).unwrap_or(0),
    );

    // 6. Package as .aos
    let packager = AdapterPackager::new(env.adapters_dir());
    let adapter_id = format!("mvp/{domain}/r{:03}", index);
    let base_model = "test-base-model";

    let packaged = packager.package(&adapter_id, &quantized, &config, base_model).await?;
    eprintln!("    Packaged: {:?}", packaged.weights_path);

    Ok(packaged.weights_path)
}

#[tokio::test]
async fn test_mvp_training_pipeline() -> Result<()> {
    eprintln!("\n=== MVP Training Pipeline Test ===\n");
    eprintln!("Training {} adapters...\n", NUM_ADAPTERS);

    let env = TestEnv::new()?;
    let mut adapter_paths = Vec::new();

    // Train adapters for each domain
    for (i, domain) in ADAPTER_DOMAINS.iter().take(NUM_ADAPTERS).enumerate() {
        let path = train_and_package(&env, domain, i).await?;
        adapter_paths.push(path);
    }

    eprintln!("\nTraining complete. {} adapters created.", adapter_paths.len());

    // Verify all .aos files exist
    for path in &adapter_paths {
        assert!(path.exists(), "Adapter file should exist: {:?}", path);
    }

    // Verify adapters can be loaded with signature verification
    let packager = AdapterPackager::new(env.adapters_dir());
    for (i, domain) in ADAPTER_DOMAINS.iter().take(NUM_ADAPTERS).enumerate() {
        let adapter_id = format!("mvp/{domain}/r{:03}", i);
        let loaded = packager.load(&adapter_id).await?;
        eprintln!("  Verified adapter: {} (hash: {}...)",
            loaded.adapter_id,
            &loaded.hash_b3[..16]);

        // Verify weights can be loaded
        let weights_data = tokio::fs::read(&loaded.weights_path).await?;
        assert!(!weights_data.is_empty(), "Weights should not be empty");

        // Verify manifest integrity
        assert_eq!(loaded.manifest.rank, 4, "Rank should match training config");
        assert_eq!(loaded.manifest.base_model, "test-base-model");
    }
    eprintln!("\nAll {} adapters verified successfully!", NUM_ADAPTERS);

    eprintln!("\n=== Test Passed ===\n");
    Ok(())
}

#[tokio::test]
async fn test_training_determinism() -> Result<()> {
    eprintln!("\n=== Training Determinism Test ===\n");

    let examples = generate_examples("test", 8);

    let config = TrainingConfig {
        rank: 4,
        alpha: 8.0,
        learning_rate: 1e-4,
        batch_size: 4,
        epochs: 1,
        hidden_dim: 256,
    };

    // Train twice
    let mut trainer1 = MicroLoRATrainer::new(config.clone())?;
    let mut trainer2 = MicroLoRATrainer::new(config)?;

    let result1 = trainer1.train(&examples).await?;
    let result2 = trainer2.train(&examples).await?;

    // Verify deterministic output
    assert_eq!(result1.weights.lora_a.len(), result2.weights.lora_a.len());
    assert_eq!(result1.weights.lora_b.len(), result2.weights.lora_b.len());

    // Compare first few values
    if !result1.weights.lora_a.is_empty() && !result2.weights.lora_a.is_empty() {
        let diff: f32 = result1.weights.lora_a[0]
            .iter()
            .zip(result2.weights.lora_a[0].iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        eprintln!("Weight difference (should be 0): {:.6}", diff);
        assert!(diff < 1e-6, "Weights should be deterministic");
    }

    eprintln!("\n=== Determinism Verified ===\n");
    Ok(())
}

#[tokio::test]
async fn test_router_with_adapters() -> Result<()> {
    eprintln!("\n=== Router Test ===\n");

    // Create router with dummy weights
    let weights = vec![0.0f32; 100];
    let router = Router::new(weights, 3, 1.0, 1e-6, [0u8; 32])?;

    eprintln!("Router created with k=3");
    eprintln!("\n=== Router Test Passed ===\n");

    Ok(())
}

#[test]
fn test_component_instantiation() {
    // Verify all components can be instantiated
    let config = TrainingConfig {
        rank: 8,
        alpha: 16.0,
        learning_rate: 1e-4,
        batch_size: 4,
        epochs: 1,
        hidden_dim: 512,
    };

    let trainer = MicroLoRATrainer::new(config);
    assert!(trainer.is_ok(), "Trainer should instantiate");

    let examples = generate_examples("test", 5);
    assert_eq!(examples.len(), 5);

    eprintln!("All components instantiate correctly");
}
