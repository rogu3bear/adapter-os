//! MLX inference example using adapteros-base-llm with PyO3 integration.
//!
//! # Environment Variables
//! - `AOS_MODEL_PATH` - Path to the model directory (required)
//! - `AOS_MODEL_BACKEND` - Backend preference: auto, coreml, metal, mlx (default: auto)
//! - `AOS_MLX_MODEL` - Legacy: model reference (deprecated, use AOS_MODEL_PATH instead)
//!
//! Run with the `mlx` feature enabled for adapteros-base-llm:
//!
//!   cargo run -p adapteros-base-llm --features mlx --example mlx_inference
//!
//! Configure the model path via environment variable:
//!   export AOS_MODEL_PATH="./models/qwen2.5-7b-mlx"
//!   # Or for backward compatibility:
//!   export AOS_MLX_MODEL="mlx-community/Qwen2.5-7B-Instruct-4bit"

#[cfg(feature = "extended-tests")]
use adapteros_base_llm::{BaseLLM, BaseLLMConfig, BaseLLMFactory, BaseLLMMetadata, ModelType};
#[cfg(feature = "extended-tests")]
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};

// Note: In a real application, you would use:
// use adapteros_config::ModelConfig;

#[cfg(not(feature = "extended-tests"))]
fn main() {
    eprintln!("Enable the `extended-tests` feature to run the AdapterOS MLX inference example.");
}

#[cfg(feature = "extended-tests")]
fn main() -> adapteros_core::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Create metadata (defaults to Qwen2.5-7B)
    let metadata: BaseLLMMetadata = BaseLLMMetadata::default();

    // Load model path from environment
    // Preferred: AOS_MODEL_PATH (unified configuration)
    // Legacy: AOS_MLX_MODEL (backward compatibility)
    let model_path = std::env::var("AOS_MODEL_PATH")
        .or_else(|_| std::env::var("AOS_MLX_MODEL"))
        .ok();

    // Build config with model path from environment
    let cfg = BaseLLMConfig {
        model_type: ModelType::Qwen,
        metadata,
        model_path,
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
    println!(
        "Non-zero entries: {}",
        logits.iter().filter(|v| **v > 0.0).count()
    );

    Ok(())
}
