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

use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_mlx_ffi::{LoRAAdapter, LoRAConfig, MLXBackend, MLXModel}; // Fix to correct module

#[cfg(not(feature = "extended-tests"))]
fn main() {
    eprintln!("Enable the `extended-tests` feature to run the AdapterOS basic inference example.");
}

#[cfg(feature = "extended-tests")]
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("\n🚀 AdapterOS Basic Inference Example\n");

    // Load manifest
    let manifest_path = "manifests/qwen7b.json"; // Example path
    if !std::path::Path::new(manifest_path).exists() {
        eprintln!("❌ Manifest not found: {}", manifest_path);
        return Err(AosError::Config(format!(
            "Manifest not found: {}",
            manifest_path
        )));
    }
    let manifest_content = fs::read_to_string(manifest_path)
        .map_err(|e| AosError::Config(format!("Failed to read manifest: {}", e)))?;
    let manifest: ManifestV3 = serde_json::from_str(&manifest_content)
        .map_err(|e| AosError::Config(format!("Failed to parse manifest: {}", e)))?;

    println!(
        "✅ Manifest loaded: {} ({} adapters)",
        manifest.base.model_id,
        manifest.adapters.len()
    );
    println!("ℹ️  This example demonstrates manifest parsing and setup.\n     For full inference, start the server and use the Worker with Metal kernels.");

    Ok(())
}
