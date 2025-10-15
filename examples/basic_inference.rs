//! Basic MLX inference example
//!
//! This example demonstrates:
//! 1. Loading a model from MLX format
//! 2. Loading LoRA adapters
//! 3. Running inference with K-sparse routing
//!
//! # Prerequisites
//!
//! - Python 3.9+ with MLX installed
//! - Model files in `models/qwen2.5-7b-mlx/`
//! - LoRA adapter files in `adapters/`
//!
//! # Usage
//!
//! ```bash
//! cargo run --example basic_inference
//! ```

use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use mplora_mlx::{LoRAAdapter, LoRAConfig, MLXBackend, MLXModel};

fn main() -> anyhow::Result<()> {
    // Initialize Python runtime
    pyo3::prepare_freethreaded_python();

    println!("🚀 MPLoRA Basic Inference Example\n");

    // 1. Load model
    println!("📦 Loading model...");
    let model_path = "models/qwen2.5-7b-mlx";
    let model = MLXModel::load(model_path)?;

    println!("✅ Model loaded:");
    println!("   Hidden size: {}", model.hidden_size());
    println!("   Vocab size: {}", model.vocab_size());

    // 2. Create MLX backend
    println!("\n🔧 Creating MLX backend...");
    let mut backend = MLXBackend::new(model);
    println!("✅ Backend ready");

    // 3. Load LoRA adapters (optional)
    println!("\n📦 Loading LoRA adapters...");

    // Example adapter configuration
    let config = LoRAConfig {
        rank: 16,
        alpha: 32.0,
        target_modules: vec![
            "q_proj".to_string(),
            "k_proj".to_string(),
            "v_proj".to_string(),
            "o_proj".to_string(),
        ],
        dropout: 0.0,
    };

    // Try to load adapters (skip if not present)
    let adapter_paths = vec![
        "adapters/adapter1.safetensors",
        "adapters/adapter2.safetensors",
        "adapters/adapter3.safetensors",
    ];

    let mut loaded_adapters = 0;
    for (id, path) in adapter_paths.iter().enumerate() {
        if std::path::Path::new(path).exists() {
            match LoRAAdapter::load(path, format!("adapter{}", id), config.clone()) {
                Ok(adapter) => {
                    backend.register_adapter(id as u16, adapter)?;
                    loaded_adapters += 1;
                    println!("   ✅ Loaded adapter {}: {}", id, path);
                }
                Err(e) => {
                    println!("   ⚠️  Failed to load {}: {}", path, e);
                }
            }
        }
    }

    if loaded_adapters == 0 {
        println!("   ℹ️  No adapters loaded (optional)");
    } else {
        println!("   ✅ Loaded {} adapters", loaded_adapters);
    }

    // 4. Prepare inference
    println!("\n🔮 Running inference...");

    let vocab_size = 151936; // Qwen 2.5 vocab size
    let mut io = IoBuffers::new(vocab_size);

    // Example input tokens (replace with actual tokenized input)
    io.input_ids = vec![1, 2, 3]; // Dummy tokens
    io.position = 0;

    // Router decision (K-sparse routing)
    let mut ring = RouterRing::new(3);
    if loaded_adapters > 0 {
        // Example: activate adapters 0, 1, 2 with Q15 gates
        ring.set(
            &[0, 1, 2],
            &[15000, 10000, 7767], // Q15 quantized gates (sum ≈ 32767)
        );
    }

    // Run single inference step
    backend.run_step(&ring, &mut io)?;

    println!("✅ Inference complete!");
    println!("   Position: {}", io.position);
    println!("   Output logits: {} values", io.output_logits.len());

    // Get top-5 predictions
    let mut indexed_logits: Vec<(usize, f32)> = io
        .output_logits
        .iter()
        .enumerate()
        .map(|(i, &v)| (i, v))
        .collect();
    indexed_logits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("\n📊 Top-5 predictions:");
    for (i, (token_id, logit)) in indexed_logits.iter().take(5).enumerate() {
        println!("   {}. Token {}: {:.4}", i + 1, token_id, logit);
    }

    println!("\n✅ Example complete!");
    println!("\n💡 Next steps:");
    println!("   - Add actual tokenizer for text input");
    println!("   - Implement autoregressive generation loop");
    println!("   - Train and load custom LoRA adapters");
    println!("   - Experiment with different K values");

    Ok(())
}
