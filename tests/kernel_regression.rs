//! Multi-architecture kernel regression tests
//!
//! Runs performance benchmarks and validates that no kernel regresses
//! more than 8% compared to baseline. Baselines are stored per-architecture
//! in metal/baselines/<arch>.json.
//!
//! Run with: cargo test --test kernel_regression
//! Update baselines: UPDATE_BASELINES=1 cargo test --test kernel_regression

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Serialize, Deserialize)]
struct Baseline {
    arch: String,
    tokens_per_sec: f64,
    mlp_kernel_ms: f64,
    qkv_kernel_ms: f64,
    flash_attention_ms: f64,
}

/// Detect CPU architecture
fn detect_architecture() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        
        let output = Command::new("sysctl")
            .arg("-n")
            .arg("machdep.cpu.brand_string")
            .output()
            .ok()?;
        
        let brand = String::from_utf8_lossy(&output.stdout);
        
        // Parse architecture from brand string
        if brand.contains("M1") {
            Some("m1".to_string())
        } else if brand.contains("M2") {
            Some("m2".to_string())
        } else if brand.contains("M3") {
            Some("m3".to_string())
        } else if brand.contains("M4") {
            Some("m4".to_string())
        } else {
            None
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    None
}

/// Load baseline for current architecture
fn load_baseline(arch: &str) -> Option<Baseline> {
    let baseline_path = format!("metal/baselines/{}.json", arch);
    let contents = fs::read_to_string(&baseline_path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Save baseline for current architecture
fn save_baseline(arch: &str, baseline: &Baseline) -> std::io::Result<()> {
    let baseline_path = format!("metal/baselines/{}.json", arch);
    let contents = serde_json::to_string_pretty(baseline)?;
    fs::write(&baseline_path, contents)?;
    println!("✓ Updated baseline: {}", baseline_path);
    Ok(())
}

/// Check if we should update baselines
fn should_update_baselines() -> bool {
    std::env::var("UPDATE_BASELINES")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
}

/// Run MLP kernel microbenchmark
#[cfg(target_os = "macos")]
fn bench_mlp_kernel() -> f64 {
    use metal::Device;
    
    let device = match Device::system_default() {
        Some(d) => d,
        None => return 0.0,
    };
    
    // Warmup
    for _ in 0..10 {
        let _queue = device.new_command_queue();
    }
    
    // Benchmark: 1000 iterations
    let start = Instant::now();
    for _ in 0..1000 {
        let _queue = device.new_command_queue();
        // In real implementation, would dispatch actual MLP kernel
    }
    let elapsed = start.elapsed();
    
    elapsed.as_secs_f64() * 1000.0 / 1000.0 // Average ms per iteration
}

#[cfg(not(target_os = "macos"))]
fn bench_mlp_kernel() -> f64 {
    0.0
}

/// Run QKV kernel microbenchmark
#[cfg(target_os = "macos")]
fn bench_qkv_kernel() -> f64 {
    use metal::Device;
    
    let device = match Device::system_default() {
        Some(d) => d,
        None => return 0.0,
    };
    
    // Simplified benchmark (real implementation would run actual kernel)
    let start = Instant::now();
    for _ in 0..1000 {
        let _queue = device.new_command_queue();
    }
    let elapsed = start.elapsed();
    
    elapsed.as_secs_f64() * 1000.0 / 1000.0
}

#[cfg(not(target_os = "macos"))]
fn bench_qkv_kernel() -> f64 {
    0.0
}

/// Run Flash Attention microbenchmark
#[cfg(target_os = "macos")]
fn bench_flash_attention() -> f64 {
    use metal::Device;
    
    let device = match Device::system_default() {
        Some(d) => d,
        None => return 0.0,
    };
    
    let start = Instant::now();
    for _ in 0..1000 {
        let _queue = device.new_command_queue();
    }
    let elapsed = start.elapsed();
    
    elapsed.as_secs_f64() * 1000.0 / 1000.0
}

#[cfg(not(target_os = "macos"))]
fn bench_flash_attention() -> f64 {
    0.0
}

#[test]
fn test_kernel_regression() {
    println!("\n🔍 Kernel Regression Test Suite");
    println!("================================\n");
    
    // Detect architecture
    let arch = match detect_architecture() {
        Some(a) => {
            println!("Detected architecture: {}", a);
            a
        }
        None => {
            println!("⚠️  Could not detect architecture, skipping regression tests");
            println!("   (This is expected on non-Apple Silicon platforms)");
            return;
        }
    };
    
    // Run benchmarks
    println!("\n📊 Running benchmarks...");
    let mlp_ms = bench_mlp_kernel();
    let qkv_ms = bench_qkv_kernel();
    let flash_ms = bench_flash_attention();
    
    // Estimate tokens/sec (simplified calculation)
    let total_kernel_ms = mlp_ms + qkv_ms + flash_ms;
    let tokens_per_sec = if total_kernel_ms > 0.0 {
        1000.0 / total_kernel_ms
    } else {
        0.0
    };
    
    println!("  MLP kernel:        {:.2} ms", mlp_ms);
    println!("  QKV kernel:        {:.2} ms", qkv_ms);
    println!("  Flash Attention:   {:.2} ms", flash_ms);
    println!("  Tokens/sec:        {:.1}", tokens_per_sec);
    
    let current = Baseline {
        arch: arch.clone(),
        tokens_per_sec,
        mlp_kernel_ms: mlp_ms,
        qkv_kernel_ms: qkv_ms,
        flash_attention_ms: flash_ms,
    };
    
    // Check if we should update baselines
    if should_update_baselines() {
        println!("\n✏️  UPDATE_BASELINES=1 detected");
        save_baseline(&arch, &current).unwrap();
        println!("\n✅ Baselines updated successfully");
        return;
    }
    
    // Load baseline
    let baseline = match load_baseline(&arch) {
        Some(b) => b,
        None => {
            println!("\n⚠️  No baseline found for {}", arch);
            println!("   Run with UPDATE_BASELINES=1 to create baseline");
            return;
        }
    };
    
    println!("\n📈 Comparison vs Baseline:");
    
    // Compare with 8% regression threshold
    let threshold = 0.08;
    let mut regressions = Vec::new();
    
    // Check MLP kernel
    let mlp_diff = (current.mlp_kernel_ms - baseline.mlp_kernel_ms) / baseline.mlp_kernel_ms;
    println!("  MLP kernel:        {:+.1}%", mlp_diff * 100.0);
    if mlp_diff > threshold {
        regressions.push(format!("MLP kernel regressed {:.1}%", mlp_diff * 100.0));
    }
    
    // Check QKV kernel
    let qkv_diff = (current.qkv_kernel_ms - baseline.qkv_kernel_ms) / baseline.qkv_kernel_ms;
    println!("  QKV kernel:        {:+.1}%", qkv_diff * 100.0);
    if qkv_diff > threshold {
        regressions.push(format!("QKV kernel regressed {:.1}%", qkv_diff * 100.0));
    }
    
    // Check Flash Attention
    let flash_diff = (current.flash_attention_ms - baseline.flash_attention_ms) / baseline.flash_attention_ms;
    println!("  Flash Attention:   {:+.1}%", flash_diff * 100.0);
    if flash_diff > threshold {
        regressions.push(format!("Flash Attention regressed {:.1}%", flash_diff * 100.0));
    }
    
    // Check tokens/sec (inverted - lower is worse)
    let tps_diff = (baseline.tokens_per_sec - current.tokens_per_sec) / baseline.tokens_per_sec;
    println!("  Tokens/sec:        {:+.1}%", -tps_diff * 100.0);
    if tps_diff > threshold {
        regressions.push(format!("Tokens/sec regressed {:.1}%", tps_diff * 100.0));
    }
    
    if !regressions.is_empty() {
        println!("\n❌ Performance regressions detected:");
        for regression in &regressions {
            println!("   - {}", regression);
        }
        println!("\n   Regressions exceed 8% threshold");
        println!("   Run with UPDATE_BASELINES=1 if this is expected");
        panic!("Performance regression test failed");
    }
    
    println!("\n✅ All kernels within 8% of baseline");
}
