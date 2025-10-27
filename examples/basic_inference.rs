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

use adapteros_base_llm::mlx_backend::load_mlx_model;
use pyo3::Python;

#[tokio::main]
async fn main() -> Result<()> {
    let fixed_seed = [0u8; 8]; // Deterministic seed
    Python::with_gil(|py| {
        let model = load_mlx_model("path/to/model", &fixed_seed)?;
        // Perform inference
        let result = py.eval_bound("model.generate('prompt')", None, None)?;
        println!("Inference result: {}", result);
        Ok(())
    })
}
