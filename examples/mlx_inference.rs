//! MLX inference example using adapteros-base-llm with PyO3 integration.
//!
//! Run with the `mlx` feature enabled for adapteros-base-llm:
//!
//!   cargo run -p adapteros-base-llm --features mlx --example mlx_inference
//!
//! Configure the MLX model reference via env var (defaults below):
//!   export AOS_MLX_MODEL="mlx-community/Qwen2.5-7B-Instruct-4bit"

use adapteros_base_llm::{BaseLLM, BaseLLMFactory, BaseLLMMetadata, BaseLLMConfig, ModelType};
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};

fn main() -> adapteros_core::Result<()> {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();

    // Create metadata (defaults to Qwen2.5-7B)
    let metadata: BaseLLMMetadata = BaseLLMMetadata::default();

    // Build config (model_path taken from env by the backend)
    let cfg = BaseLLMConfig {
        model_type: ModelType::Qwen,
        metadata,
        model_path: std::env::var("AOS_MLX_MODEL").ok(),
    };

    // Create model instance
    let mut model = BaseLLMFactory::from_config(cfg)?;

    // Deterministic executor
    let mut exec = DeterministicExecutor::new(ExecutorConfig::default());

    // Load (initializes MLX backend when feature is enabled)
    model.load(&mut exec)?;

    // Simple forward pass with mock input IDs
    let logits = model.forward(&[1, 2, 3, 4])?;
    println!("Forward logits len: {} (vocab)", logits.len());
    println!("Non-zero entries: {}", logits.iter().filter(|v| **v > 0.0).count());

    Ok(())
}

