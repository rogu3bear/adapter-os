//! Basic inference example demonstrating AdapterOS functionality
//!
//! This example demonstrates:
//! 1. Loading a manifest
//! 2. Initializing a Worker with Metal kernels
//! 3. Running inference with deterministic execution
//!
//! # Prerequisites
//!
//! - macOS with Apple Silicon (M1+)
//! - Model manifest in `manifests/`
//! - LoRA adapters (optional)
//!
//! # Usage
//!
//! ```bash
//! cargo run --example basic_inference
//! ```

use adapteros_core::{AosError, Result};
use adapteros_manifest::ManifestV3;
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🚀 AdapterOS Basic Inference Example\n");

    // Load manifest
    println!("📦 Loading manifest...");
    let manifest_path = "manifests/qwen7b.yaml";
    
    if !std::path::Path::new(manifest_path).exists() {
        eprintln!("❌ Manifest not found: {}", manifest_path);
        eprintln!("   Please create a manifest file or use an existing one.");
        return Err(AosError::Config(format!("Manifest not found: {}", manifest_path)));
    }

    let manifest_content = fs::read_to_string(manifest_path)
        .map_err(|e| AosError::Config(format!("Failed to read manifest: {}", e)))?;
    
    // Parse as JSON (manifests are JSON, not YAML despite .yaml extension for compatibility)
    let manifest: ManifestV3 = serde_json::from_str(&manifest_content)
        .map_err(|e| AosError::Config(format!("Failed to parse manifest: {}", e)))?;

    println!("✅ Manifest loaded:");
    println!("   Model: {}", manifest.base.model_id);
    println!("   Architecture: {}", manifest.base.arch);
    println!("   Adapters: {}", manifest.adapters.len());
    println!("   K-sparse: {}", manifest.router.k_sparse);

    println!("\n📝 Inference configuration:");
    println!("   Prompt: Hello, world!");
    println!("   Max tokens: 50");
    println!("   Require evidence: false");
    
    println!("\n⚠️  Note: Full Worker initialization requires Metal backend compilation");
    println!("   This example demonstrates manifest loading and configuration parsing.");
    println!("   For complete inference, ensure adapteros-lora-kernel-mtl is built.");

    println!("\n✅ Example complete!");
    println!("\n💡 Next steps:");
    println!("   - Build Metal kernels: cd metal && ./build.sh");
    println!("   - Initialize database: ./target/release/aosctl init-tenant --id default");
    println!("   - Register adapters: ./target/release/aosctl register-adapter");
    println!("   - Run full inference with Worker<MetalKernels>");

    Ok(())
}
