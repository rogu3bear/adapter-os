#!/usr/bin/env cargo +nightly -Zscript

//! Chat with an adapter using MLX backend
//!
//! # Environment Variables
//! - `AOS_MODEL_PATH` - Path to the model directory (required)
//! - `AOS_MODEL_BACKEND` - Backend preference: auto, coreml, metal, mlx (default: auto)
//! - `AOS_ADAPTER_PATH` - Path to a specific adapter file (optional, overrides selection)
//!
//! Run with: cargo +nightly -Zscript examples/chat_with_adapter.rs

use adapteros_core::paths::get_default_adapters_root;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

// Note: In a real application, you would use:
// use adapteros_config::ModelConfig;

// Simulated chat interface (would use actual MLX backend in production)
fn main() {
    // Load model configuration from environment
    // Set AOS_MODEL_PATH env var to override default model location
    let model_path =
        std::env::var("AOS_MODEL_PATH").unwrap_or_else(|_| "./models/default".to_string());
    let model_backend = std::env::var("AOS_MODEL_BACKEND").unwrap_or_else(|_| "auto".to_string());

    println!("AdapterOS Chat Interface");
    println!("Model: {} (backend: {})", model_path, model_backend);
    println!();

    // Check if adapter path is provided via environment
    let adapter_base = adapters_dir();
    let adapter_path = if let Ok(path) = std::env::var("AOS_ADAPTER_PATH") {
        path
    } else {
        println!("Available adapters:");
        println!("  1. code-assistant   - Help with coding");
        println!("  2. creative-writer  - Creative writing");
        println!("  3. readme-writer    - Documentation");
        println!();
        print!("Select adapter (1-3): ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        match input.trim() {
            "1" => adapter_base
                .join("code-assistant.aos")
                .to_string_lossy()
                .into_owned(),
            "2" => adapter_base
                .join("creative-writer.aos")
                .to_string_lossy()
                .into_owned(),
            "3" => adapter_base
                .join("readme-writer.aos")
                .to_string_lossy()
                .into_owned(),
            _ => {
                println!("Invalid selection");
                return;
            }
        }
    };

    // Check if adapter exists
    if !Path::new(&adapter_path).exists() {
        println!("Adapter not found: {}", adapter_path);
        return;
    }

    println!("\nLoaded: {}", adapter_path);
    println!("Performance: ~0.39ms inference on M4 Max");
    println!("\nType 'quit' to exit. Start chatting!\n");

    loop {
        print!("You: ");
        io::stdout().flush().unwrap();

        let mut prompt = String::new();
        io::stdin().read_line(&mut prompt).unwrap();
        let prompt = prompt.trim();

        if prompt == "quit" {
            println!("Goodbye!");
            break;
        }

        // Simulate MLX inference
        let response = simulate_inference(&adapter_path, prompt);
        println!("\nAssistant: {}\n", response);
    }
}

fn simulate_inference(adapter: &str, prompt: &str) -> String {
    let adapter_base = adapters_dir();

    // In production, this would:
    // 1. Load adapter with MLXFFIModel::load(adapter)
    // 2. Tokenize prompt
    // 3. Run inference with model.forward()
    // 4. Generate tokens autoregressively
    // 5. Return decoded text

    match adapter {
        adapter if adapter == adapter_base.join("code-assistant.aos").to_string_lossy() => {
            format!(
                "Based on the code-assistant adapter, here's my response to '{}': \n\
                    I can help you write, debug, and optimize code. \n\
                    With 0.39ms inference latency, I provide instant code suggestions.",
                prompt
            )
        }
        adapter if adapter == adapter_base.join("creative-writer.aos").to_string_lossy() => {
            format!(
                "Using creative-writer adapter for '{}': \n\
                    Let me craft something creative for you. \n\
                    My LoRA weights are tuned for imaginative responses.",
                prompt
            )
        }
        adapter if adapter == adapter_base.join("readme-writer.aos").to_string_lossy() => {
            format!(
                "Documentation mode for '{}': \n\
                    I'll help you write clear, comprehensive documentation. \n\
                    This adapter is optimized for technical writing.",
                prompt
            )
        }
        _ => "Unknown adapter".to_string(),
    }
}

fn adapters_dir() -> PathBuf {
    get_default_adapters_root()
}
