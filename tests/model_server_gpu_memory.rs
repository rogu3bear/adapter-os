//! GPU Memory Measurement for Model Server
//!
//! Measures actual unified memory consumption when loading models.
//! Uses ioreg to capture Metal GPU memory allocation.
//!
//! Run with:
//! ```bash
//! AOS_MODEL_PATH=/Users/star/Dev/adapter-os/var/models/Llama-3.2-3B-Instruct-4bit \
//!   cargo test --test model_server_gpu_memory -- --ignored --nocapture
//! ```

use std::process::Command;
use std::time::Instant;

/// Get Metal GPU memory from ioreg
fn get_metal_memory() -> Option<(u64, u64)> {
    let output = Command::new("ioreg")
        .args(["-l", "-w0", "-r", "-c", "IOAccelerator"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Look for "In use system memory" and "Alloc system memory"
    let mut alloc_memory = None;
    let mut in_use_memory = None;

    for line in stdout.lines() {
        if line.contains("PerformanceStatistics") {
            // Parse the dictionary-like string
            if let Some(alloc_start) = line.find("\"Alloc system memory\"=") {
                let rest = &line[alloc_start + 22..];
                if let Some(end) = rest.find(',').or_else(|| rest.find('}')) {
                    if let Ok(val) = rest[..end].parse::<u64>() {
                        alloc_memory = Some(val);
                    }
                }
            }
            if let Some(use_start) = line.find("\"In use system memory\"=") {
                let rest = &line[use_start + 23..];
                if let Some(end) = rest.find(',').or_else(|| rest.find('}')) {
                    if let Ok(val) = rest[..end].parse::<u64>() {
                        in_use_memory = Some(val);
                    }
                }
            }
        }
    }

    match (alloc_memory, in_use_memory) {
        (Some(a), Some(u)) => Some((a, u)),
        _ => None,
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

#[test]
#[ignore]
fn test_model_load_gpu_memory() {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║          GPU Memory Test - Model Loading                        ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let model_path = match std::env::var("AOS_MODEL_PATH") {
        Ok(p) => p,
        Err(_) => {
            println!("❌ AOS_MODEL_PATH not set. Example:");
            println!("   AOS_MODEL_PATH=/Users/star/Dev/adapter-os/var/models/Llama-3.2-3B-Instruct-4bit \\");
            println!("     cargo test --test model_server_gpu_memory -- --ignored --nocapture");
            return;
        }
    };

    println!("Model: {}", model_path);

    // Check model file size
    if let Ok(metadata) = std::fs::metadata(format!("{}/model.safetensors", model_path)) {
        println!("Model file size: {}", format_bytes(metadata.len()));
    }

    // Capture GPU memory before
    let before = get_metal_memory();
    if let Some((alloc, in_use)) = before {
        println!("\n--- Before Model Load ---");
        println!("  Metal allocated: {}", format_bytes(alloc));
        println!("  Metal in-use:    {}", format_bytes(in_use));
    } else {
        println!("\n⚠️  Could not read Metal memory via ioreg");
    }

    // Load model using MLX FFI
    #[cfg(feature = "mlx")]
    {
        use adapteros_lora_mlx_ffi::MLXFFIModel;
        use std::path::Path;

        println!("\n--- Loading Model via MLX FFI ---");
        let load_start = Instant::now();

        match MLXFFIModel::load(Path::new(&model_path)) {
            Ok(model) => {
                let load_time = load_start.elapsed();
                println!("✓ Model loaded in {:?}", load_time);

                // Force GPU sync to ensure memory is allocated
                std::thread::sleep(std::time::Duration::from_millis(100));

                // Capture GPU memory after
                let after = get_metal_memory();
                if let Some((alloc_after, in_use_after)) = after {
                    println!("\n--- After Model Load ---");
                    println!("  Metal allocated: {}", format_bytes(alloc_after));
                    println!("  Metal in-use:    {}", format_bytes(in_use_after));

                    if let Some((alloc_before, in_use_before)) = before {
                        let alloc_delta = alloc_after as i64 - alloc_before as i64;
                        let in_use_delta = in_use_after as i64 - in_use_before as i64;

                        println!("\n--- Memory Delta ---");
                        println!(
                            "  Allocated delta: {} {}",
                            format_bytes(alloc_delta.unsigned_abs()),
                            if alloc_delta >= 0 { "(+)" } else { "(-)" }
                        );
                        println!(
                            "  In-use delta:    {} {}",
                            format_bytes(in_use_delta.unsigned_abs()),
                            if in_use_delta >= 0 { "(+)" } else { "(-)" }
                        );
                    }
                }

                // Get model config
                let config = model.config();
                let head_dim = config.hidden_size / config.num_attention_heads;
                println!("\n--- Model Architecture ---");
                println!("  Vocab size:   {}", config.vocab_size);
                println!("  Hidden size:  {}", config.hidden_size);
                println!("  Num layers:   {}", config.num_hidden_layers);
                println!("  Num heads:    {}", config.num_attention_heads);
                println!("  Head dim:     {}", head_dim);

                // Calculate theoretical memory (4-bit quantized)
                let params_approx = config.vocab_size as u64
                    * config.hidden_size as u64  // embeddings
                    + config.num_hidden_layers as u64
                        * (4 * config.hidden_size as u64 * config.hidden_size as u64  // attention
                           + 3 * config.hidden_size as u64 * config.hidden_size as u64); // FFN
                let theoretical_fp16 = params_approx * 2; // FP16
                let theoretical_4bit = params_approx / 2; // 4-bit

                println!("\n--- Theoretical Memory ---");
                println!(
                    "  ~Params:     {} ({:.1}B)",
                    format_bytes(params_approx),
                    params_approx as f64 / 1e9
                );
                println!("  FP16:        {}", format_bytes(theoretical_fp16));
                println!("  4-bit:       {}", format_bytes(theoretical_4bit));

                // Keep model alive for a bit to ensure measurements are stable
                println!("\n--- Keeping model loaded for 2 seconds ---");
                std::thread::sleep(std::time::Duration::from_secs(2));

                // Final measurement
                if let Some((alloc_final, in_use_final)) = get_metal_memory() {
                    println!("\n--- Final Memory State ---");
                    println!("  Metal allocated: {}", format_bytes(alloc_final));
                    println!("  Metal in-use:    {}", format_bytes(in_use_final));
                }

                println!("\n--- Dropping model ---");
                drop(model);
                std::thread::sleep(std::time::Duration::from_millis(500));

                // Check memory after drop
                if let Some((alloc_dropped, in_use_dropped)) = get_metal_memory() {
                    println!("  Metal allocated: {}", format_bytes(alloc_dropped));
                    println!("  Metal in-use:    {}", format_bytes(in_use_dropped));
                }
            }
            Err(e) => {
                println!("❌ Failed to load model: {}", e);
            }
        }
    }

    #[cfg(not(feature = "mlx"))]
    {
        println!("\n❌ MLX feature not enabled. Add to root Cargo.toml dev-dependencies:");
        println!("   adapteros-lora-mlx-ffi = {{ path = \"crates/adapteros-lora-mlx-ffi\" }}");
    }

    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║                      Test Complete                              ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
}

/// Compare memory usage: 1 worker vs 3 workers loading the same model
#[test]
#[ignore]
fn test_memory_comparison_1_vs_3_workers() {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║     Memory Comparison: 1 Model vs 3 Separate Loads              ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let model_path = match std::env::var("AOS_MODEL_PATH") {
        Ok(p) => p,
        Err(_) => {
            println!("❌ AOS_MODEL_PATH not set");
            return;
        }
    };

    println!("This test demonstrates the memory savings of the Model Server architecture.\n");
    println!("Scenario A: Model Server (1 shared model)");
    println!("Scenario B: Legacy (3 workers each loading model)\n");

    #[cfg(feature = "mlx")]
    {
        use adapteros_lora_mlx_ffi::MLXFFIModel;
        use std::path::Path;

        // Scenario A: Load model once
        println!("--- Scenario A: Single Shared Model ---");
        let before_a = get_metal_memory();

        let model_a = MLXFFIModel::load(Path::new(&model_path)).expect("Failed to load model");
        std::thread::sleep(std::time::Duration::from_millis(200));

        let after_a = get_metal_memory();

        if let (Some((_, in_use_before)), Some((_, in_use_after))) = (before_a, after_a) {
            let single_model_mem = in_use_after.saturating_sub(in_use_before);
            println!("  Single model memory: {}", format_bytes(single_model_mem));

            // Scenario B would be 3x this (we simulate, don't actually load 3 times)
            let three_models_mem = single_model_mem * 3;
            println!("\n--- Scenario B: Three Separate Loads (simulated) ---");
            println!("  3x model memory: {}", format_bytes(three_models_mem));

            // Calculate savings
            let savings = three_models_mem.saturating_sub(single_model_mem);
            let savings_pct = if three_models_mem > 0 {
                (savings as f64 / three_models_mem as f64) * 100.0
            } else {
                0.0
            };

            println!("\n╔════════════════════════════════════════════════════════════════╗");
            println!("║                      MEMORY SAVINGS                             ║");
            println!("╠════════════════════════════════════════════════════════════════╣");
            println!(
                "║  Model Server (1 model):   {:>12}                         ║",
                format_bytes(single_model_mem)
            );
            println!(
                "║  Legacy (3 workers):       {:>12}                         ║",
                format_bytes(three_models_mem)
            );
            println!(
                "║  Memory saved:             {:>12} ({:.1}%)                   ║",
                format_bytes(savings),
                savings_pct
            );
            println!("╚════════════════════════════════════════════════════════════════╝");
        }

        drop(model_a);
    }

    #[cfg(not(feature = "mlx"))]
    {
        println!("❌ MLX feature not enabled");
    }
}
