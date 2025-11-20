//! Example: Loading LoRA adapters from .aos archives into MLX backend
//!
//! This example demonstrates:
//! 1. Loading a single adapter from .aos
//! 2. Loading multiple adapters in batch
//! 3. Integrating with MLXFFIBackend
//! 4. Memory usage tracking
//!
//! Run with:
//! ```bash
//! cargo run --example aos_loading_example --features mmap
//! ```

#[cfg(feature = "mmap")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use adapteros_aos::aos2_writer::AOS2Writer;
    use adapteros_aos::aos_v2_parser::AosV2Manifest;
    use adapteros_core::B3Hash;
    use adapteros_lora_mlx_ffi::aos_loader::{AosLoader, MlxBackendAosExt};
    use adapteros_lora_mlx_ffi::lora::{LoRAAdapter, LoRAConfig};
    use std::collections::HashMap;
    use tempfile::TempDir;
    use tracing_subscriber;

    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== AOS Loader Example ===\n");

    // Create temporary directory for test files
    let temp_dir = TempDir::new()?;
    println!("Created temp directory: {}", temp_dir.path().display());

    // Step 1: Create sample .aos files
    println!("\n--- Step 1: Creating sample .aos files ---");
    let adapter1_path = temp_dir.path().join("code_review.aos");
    let adapter2_path = temp_dir.path().join("docs_gen.aos");

    create_sample_aos(&adapter1_path, "code-review", 8)?;
    create_sample_aos(&adapter2_path, "docs-generation", 16)?;

    println!("✓ Created code_review.aos (rank=8)");
    println!("✓ Created docs_gen.aos (rank=16)");

    // Step 2: Load single adapter
    println!("\n--- Step 2: Loading single adapter ---");
    let loader = AosLoader::new();
    let adapter1 = loader.load_from_aos(&adapter1_path)?;

    println!("Loaded adapter: {}", adapter1.id());
    println!("  Rank: {}", adapter1.config().rank);
    println!("  Alpha: {}", adapter1.config().alpha);
    println!("  Target modules: {:?}", adapter1.config().target_modules);
    println!("  Parameters: {}", adapter1.parameter_count());
    println!(
        "  Memory: {:.2} MB",
        adapter1.memory_usage() as f32 / (1024.0 * 1024.0)
    );

    // Step 3: Load multiple adapters
    println!("\n--- Step 3: Batch loading multiple adapters ---");
    let adapter_paths = vec![
        (1u16, adapter1_path.as_path()),
        (2u16, adapter2_path.as_path()),
    ];

    let adapters = loader.load_multiple(&adapter_paths)?;
    println!("✓ Loaded {} adapters", adapters.len());

    for (id, adapter) in &adapters {
        println!(
            "  [{}] {} - rank={}, params={}",
            id,
            adapter.id(),
            adapter.config().rank,
            adapter.parameter_count()
        );
    }

    // Step 4: Calculate total memory usage
    println!("\n--- Step 4: Memory usage analysis ---");
    let total_params: usize = adapters.values().map(|a| a.parameter_count()).sum();
    let total_memory: usize = adapters.values().map(|a| a.memory_usage()).sum();

    println!("Total parameters: {}", total_params);
    println!(
        "Total memory: {:.2} MB",
        total_memory as f32 / (1024.0 * 1024.0)
    );

    // Step 5: Hash verification
    println!("\n--- Step 5: Hash verification ---");
    let adapter1_reloaded = loader.load_from_aos(&adapter1_path)?;
    let expected_hash = adapter1_reloaded.hash();

    match loader.load_and_verify(&adapter1_path, &expected_hash) {
        Ok(adapter) => {
            println!("✓ Hash verification passed");
            println!("  Hash: {}", adapter.hash().to_short_hex());
        }
        Err(e) => {
            println!("✗ Hash verification failed: {}", e);
        }
    }

    // Step 6: Integration example (would require actual MLX model)
    println!("\n--- Step 6: MLX backend integration (mock) ---");
    println!("In production, you would:");
    println!("  1. Load MLX model: MLXFFIModel::load(\"model/\")");
    println!("  2. Create backend: MLXFFIBackend::new(model)");
    println!("  3. Load adapters: backend.load_adapter_from_aos(id, path)");
    println!("  4. Run inference with adapter hot-swapping");

    // Example code (requires actual model):
    println!(
        r#"
Example code:
```rust
use adapteros_lora_mlx_ffi::{{MLXFFIModel, backend::MLXFFIBackend}};
use adapteros_lora_mlx_ffi::aos_loader::MlxBackendAosExt;

let model = MLXFFIModel::load("model/")?;
let backend = MLXFFIBackend::new(model);

// Load adapters from .aos
backend.load_adapter_from_aos(1, "code_review.aos")?;
backend.load_adapter_from_aos(2, "docs_gen.aos")?;

// Or batch load
let paths = vec![(1, "a1.aos"), (2, "a2.aos")];
backend.load_adapters_from_aos(&paths)?;

println!("Backend has {} adapters", backend.adapter_count());
```
"#
    );

    println!("\n=== Example completed successfully ===");

    Ok(())
}

#[cfg(feature = "mmap")]
fn create_sample_aos(
    path: &std::path::Path,
    adapter_id: &str,
    rank: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    use adapteros_aos::aos2_writer::AOS2Writer;
    use adapteros_aos::aos_v2_parser::AosV2Manifest;
    use adapteros_core::B3Hash;
    use std::collections::HashMap;

    // Create safetensors weights
    let weights_data = create_sample_safetensors(rank)?;
    let weights_hash = B3Hash::hash(&weights_data);

    // Create manifest
    let manifest = AosV2Manifest {
        version: "2.0".to_string(),
        adapter_id: adapter_id.to_string(),
        rank,
        weights_hash: Some(weights_hash),
        tensor_shapes: Some({
            let mut shapes = HashMap::new();
            shapes.insert("q_proj.lora_A".to_string(), vec![768, rank as usize]);
            shapes.insert("q_proj.lora_B".to_string(), vec![rank as usize, 768]);
            shapes.insert("v_proj.lora_A".to_string(), vec![768, rank as usize]);
            shapes.insert("v_proj.lora_B".to_string(), vec![rank as usize, 768]);
            shapes
        }),
        metadata: {
            let mut m = HashMap::new();
            m.insert("alpha".to_string(), serde_json::json!(16.0));
            m.insert(
                "target_modules".to_string(),
                serde_json::json!(["q_proj", "v_proj"]),
            );
            m.insert("dropout".to_string(), serde_json::json!(0.1));
            m
        },
    };

    // Write archive
    let writer = AOS2Writer::new();
    writer.write_archive(path, &manifest, &weights_data)?;

    Ok(())
}

#[cfg(feature = "mmap")]
fn create_sample_safetensors(rank: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let rank = rank as usize;
    let hidden_dim = 768;

    // Calculate tensor sizes (f32 = 4 bytes)
    let a_size = hidden_dim * rank * 4;
    let b_size = rank * hidden_dim * 4;

    // Create safetensors header
    let header_json = serde_json::json!({
        "q_proj.lora_A": {
            "dtype": "F32",
            "shape": [hidden_dim, rank],
            "data_offsets": [0, a_size]
        },
        "q_proj.lora_B": {
            "dtype": "F32",
            "shape": [rank, hidden_dim],
            "data_offsets": [a_size, a_size + b_size]
        },
        "v_proj.lora_A": {
            "dtype": "F32",
            "shape": [hidden_dim, rank],
            "data_offsets": [a_size + b_size, a_size + b_size + a_size]
        },
        "v_proj.lora_B": {
            "dtype": "F32",
            "shape": [rank, hidden_dim],
            "data_offsets": [a_size + b_size + a_size, a_size + b_size + a_size + b_size]
        }
    });

    let header_bytes = serde_json::to_vec(&header_json)?;
    let header_size = header_bytes.len() as u64;

    // Build weights data
    let mut weights_data = Vec::new();
    weights_data.extend_from_slice(&header_size.to_le_bytes());
    weights_data.extend_from_slice(&header_bytes);

    // Add tensor data (small random values simulating trained weights)
    for tensor_idx in 0..4 {
        let size = if tensor_idx % 2 == 0 { a_size } else { b_size };
        for i in 0..size / 4 {
            // Deterministic "random" values
            let val = ((i + tensor_idx * 1000) % 100) as f32 / 1000.0;
            weights_data.extend_from_slice(&val.to_le_bytes());
        }
    }

    Ok(weights_data)
}

#[cfg(not(feature = "mmap"))]
fn main() {
    eprintln!("This example requires the 'mmap' feature.");
    eprintln!("Run with: cargo run --example aos_loading_example --features mmap");
    std::process::exit(1);
}
