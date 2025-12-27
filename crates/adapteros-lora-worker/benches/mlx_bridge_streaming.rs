//! MLX Subprocess Bridge Streaming Benchmarks
//!
//! Measures real streaming performance of the MLX subprocess bridge:
//! - Time to first token (TTFT)
//! - Total generation time
//! - Tokens per second
//! - Streaming vs non-streaming comparison
//! - Dense vs MoE model comparison
//!
//! ## Available Models
//!
//! The benchmark auto-detects models in `./var/models/`:
//! - `Qwen2.5-7B-Instruct-4bit` - Dense 7B model
//! - `Qwen3-Coder-30B-A3B-Instruct-MLX-4bit` - MoE model (128 experts, 8 active)
//! - `Qwen2.5-VL-7B-Instruct-8bit` - Vision-Language model
//!
//! Override with `MLX_BENCHMARK_MODEL` environment variable.
//!
//! ## Running
//!
//! ```bash
//! # Default (uses first available model)
//! cargo bench --package adapteros-lora-worker --bench mlx_bridge_streaming
//!
//! # Specific model
//! MLX_BENCHMARK_MODEL=./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
//!   cargo bench --package adapteros-lora-worker --bench mlx_bridge_streaming
//! ```
//!
//! ## Baseline Results (2025-12-24, Apple Silicon)
//!
//! ### Qwen2.5-7B-Instruct-4bit (Dense)
//! | Mode | TTFT | Total (20 tokens) | Tokens/sec |
//! |------|------|-------------------|------------|
//! | Streaming | 182.74ms | 388.15ms | 51.53 |
//! | Non-streaming | N/A | 840.97ms | 23.78 |
//!
//! ### Qwen3-Coder-30B-A3B-Instruct-MLX-4bit (MoE, 128 experts)
//! | Mode | TTFT | Total (20 tokens) | Tokens/sec |
//! |------|------|-------------------|------------|
//! | Streaming | TBD | TBD | TBD |
//! | Non-streaming | TBD | TBD | TBD |
//!
//! These are REAL measurements, not simulated.
#![cfg(feature = "mlx-bridge")]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Model info for benchmark reporting
#[derive(Debug, Clone)]
struct ModelInfo {
    path: PathBuf,
    name: String,
    is_moe: bool,
}

/// Find an available MLX model for benchmarking
fn find_model_path() -> Option<ModelInfo> {
    // Check environment variable first
    if let Ok(path) = std::env::var("MLX_BENCHMARK_MODEL") {
        let p = PathBuf::from(&path);
        if p.exists() {
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let is_moe = name.contains("A3B") || name.contains("moe") || name.contains("MoE");
            return Some(ModelInfo {
                path: p,
                name,
                is_moe,
            });
        }
    }

    // Models to try, in order of preference (smaller/faster first for quicker benchmarks)
    let candidates = [
        // Dense models (faster)
        ("Qwen2.5-7B-Instruct-4bit", false),
        // MoE models (slower but important to benchmark)
        ("Qwen3-Coder-30B-A3B-Instruct-MLX-4bit", true),
        // Other models
        ("Qwen2.5-VL-7B-Instruct-8bit", false),
    ];

    // Try different base paths
    let base_paths = ["./var/models", "../var/models", "../../var/models"];

    for base in base_paths {
        for (model_name, is_moe) in &candidates {
            let p = PathBuf::from(base).join(model_name);
            if p.exists() {
                return Some(ModelInfo {
                    path: p,
                    name: model_name.to_string(),
                    is_moe: *is_moe,
                });
            }
        }
    }

    None
}

/// Find all available models for comprehensive benchmarking
fn find_all_models() -> Vec<ModelInfo> {
    let mut models = Vec::new();

    let candidates = [
        ("Qwen2.5-7B-Instruct-4bit", false),
        ("Qwen3-Coder-30B-A3B-Instruct-MLX-4bit", true),
    ];

    let base_paths = ["./var/models", "../var/models", "../../var/models"];

    for base in base_paths {
        for (model_name, is_moe) in &candidates {
            let p = PathBuf::from(base).join(model_name);
            if p.exists() {
                // Avoid duplicates
                if !models.iter().any(|m: &ModelInfo| m.name == *model_name) {
                    models.push(ModelInfo {
                        path: p,
                        name: model_name.to_string(),
                        is_moe: *is_moe,
                    });
                }
            }
        }
    }

    models
}

/// Find the bridge script
fn find_bridge_script() -> Option<PathBuf> {
    let candidates = [
        "./scripts/mlx_bridge_server.py",
        "../scripts/mlx_bridge_server.py",
        "../../scripts/mlx_bridge_server.py",
        "../../../scripts/mlx_bridge_server.py",
    ];

    for path in candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    None
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BridgeResponse {
    Ready {
        #[allow(dead_code)]
        model_path: String,
        #[allow(dead_code)]
        model_type: String,
    },
    GenerateResponse {
        #[allow(dead_code)]
        text: String,
        tokens: usize,
        #[allow(dead_code)]
        finish_reason: String,
        timing: Option<TimingStats>,
    },
    StreamToken {
        #[allow(dead_code)]
        token: String,
        index: usize,
    },
    StreamEnd {
        tokens: usize,
        #[allow(dead_code)]
        finish_reason: String,
        timing: Option<TimingStats>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize, Clone)]
struct TimingStats {
    ttft_ms: Option<f64>,
    total_ms: f64,
    tokens_per_second: f64,
}

#[derive(Debug, Serialize)]
struct GenerateRequest {
    #[serde(rename = "type")]
    request_type: String,
    prompt: String,
    max_tokens: usize,
    temperature: f32,
    top_p: f32,
    stream: bool,
    protocol_version: u32,
}

/// Benchmark results for a single run
#[derive(Debug, Clone)]
struct BenchResult {
    ttft_ms: Option<f64>,
    total_ms: f64,
    tokens: usize,
    tokens_per_second: f64,
}

/// Run a single generation and return timing results
fn run_generation(
    stdin: &mut std::process::ChildStdin,
    stdout: &mut BufReader<std::process::ChildStdout>,
    prompt: &str,
    max_tokens: usize,
    stream: bool,
) -> Option<BenchResult> {
    let request = GenerateRequest {
        request_type: "generate".to_string(),
        prompt: prompt.to_string(),
        max_tokens,
        temperature: 0.7,
        top_p: 0.9,
        stream,
        protocol_version: 2,
    };

    let json = serde_json::to_string(&request).ok()?;
    writeln!(stdin, "{}", json).ok()?;
    stdin.flush().ok()?;

    let start = Instant::now();
    let mut first_token_time: Option<Instant> = None;
    let mut tokens = 0;

    loop {
        let mut line = String::new();
        if stdout.read_line(&mut line).ok()? == 0 {
            return None;
        }

        let response: BridgeResponse = serde_json::from_str(&line).ok()?;

        match response {
            BridgeResponse::StreamToken { index, .. } => {
                if first_token_time.is_none() {
                    first_token_time = Some(Instant::now());
                }
                tokens = index + 1;
            }
            BridgeResponse::StreamEnd {
                tokens: t, timing, ..
            } => {
                let total = start.elapsed();
                let ttft = first_token_time.map(|ft| ft.duration_since(start));

                // Prefer timing from response if available
                if let Some(stats) = timing {
                    return Some(BenchResult {
                        ttft_ms: stats.ttft_ms,
                        total_ms: stats.total_ms,
                        tokens: t,
                        tokens_per_second: stats.tokens_per_second,
                    });
                }

                return Some(BenchResult {
                    ttft_ms: ttft.map(|d| d.as_secs_f64() * 1000.0),
                    total_ms: total.as_secs_f64() * 1000.0,
                    tokens: t,
                    tokens_per_second: t as f64 / total.as_secs_f64(),
                });
            }
            BridgeResponse::GenerateResponse {
                tokens: t, timing, ..
            } => {
                let total = start.elapsed();

                if let Some(stats) = timing {
                    return Some(BenchResult {
                        ttft_ms: None,
                        total_ms: stats.total_ms,
                        tokens: t,
                        tokens_per_second: stats.tokens_per_second,
                    });
                }

                return Some(BenchResult {
                    ttft_ms: None,
                    total_ms: total.as_secs_f64() * 1000.0,
                    tokens: t,
                    tokens_per_second: t as f64 / total.as_secs_f64(),
                });
            }
            _ => continue,
        }
    }
}

fn bench_mlx_bridge_streaming(c: &mut Criterion) {
    let model_info = match find_model_path() {
        Some(m) => m,
        None => {
            eprintln!("⚠️  Skipping MLX bridge benchmarks: no model found");
            eprintln!("   Set MLX_BENCHMARK_MODEL or place model at ./var/models/Qwen2.5-7B-Instruct-4bit");
            return;
        }
    };

    let script_path = match find_bridge_script() {
        Some(p) => p,
        None => {
            eprintln!("⚠️  Skipping MLX bridge benchmarks: bridge script not found");
            return;
        }
    };

    let model_type = if model_info.is_moe { "MoE" } else { "Dense" };
    eprintln!("📦 Using model: {} ({})", model_info.name, model_type);
    eprintln!("🐍 Using script: {}", script_path.display());

    // Start the bridge process
    let mut child = match Command::new("python3")
        .arg(&script_path)
        .env("MLX_MODEL_PATH", &model_info.path)
        .env("PYTHONUNBUFFERED", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("⚠️  Failed to start bridge: {}", e);
            return;
        }
    };

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Wait for ready
    let mut line = String::new();
    if stdout.read_line(&mut line).is_err() {
        eprintln!("⚠️  Bridge failed to start");
        let _ = child.kill();
        return;
    }

    if !line.contains("ready") {
        eprintln!("⚠️  Unexpected bridge response: {}", line.trim());
        let _ = child.kill();
        return;
    }

    eprintln!("✅ Bridge ready, warming up...");

    // Warm up with 3 generations
    for _ in 0..3 {
        let _ = run_generation(&mut stdin, &mut stdout, "def hello():", 10, false);
    }

    eprintln!("🔥 Running benchmarks...");

    // Include model name in benchmark group for clarity
    let group_name = format!("mlx_bridge/{}", model_info.name);
    let mut group = c.benchmark_group(&group_name);
    group.sample_size(20); // Reduce samples since each is slow
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(30));

    // Benchmark streaming mode
    for max_tokens in [10, 20, 50] {
        group.bench_with_input(
            BenchmarkId::new("streaming", max_tokens),
            &max_tokens,
            |b, &tokens| {
                b.iter(|| {
                    black_box(run_generation(
                        &mut stdin,
                        &mut stdout,
                        "def hello():",
                        tokens,
                        true,
                    ))
                })
            },
        );
    }

    // Benchmark non-streaming mode
    for max_tokens in [10, 20, 50] {
        group.bench_with_input(
            BenchmarkId::new("non_streaming", max_tokens),
            &max_tokens,
            |b, &tokens| {
                b.iter(|| {
                    black_box(run_generation(
                        &mut stdin,
                        &mut stdout,
                        "def hello():",
                        tokens,
                        false,
                    ))
                })
            },
        );
    }

    group.finish();

    // Cleanup
    let _ = writeln!(stdin, r#"{{"type":"shutdown"}}"#);
    let _ = stdin.flush();
    let _ = child.wait();

    eprintln!("✅ Benchmarks complete");
}

/// Measure TTFT specifically
fn bench_time_to_first_token(c: &mut Criterion) {
    let model_info = match find_model_path() {
        Some(m) => m,
        None => return,
    };

    let script_path = match find_bridge_script() {
        Some(p) => p,
        None => return,
    };

    let mut child = match Command::new("python3")
        .arg(&script_path)
        .env("MLX_MODEL_PATH", &model_info.path)
        .env("PYTHONUNBUFFERED", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Wait for ready
    let mut line = String::new();
    if stdout.read_line(&mut line).is_err() || !line.contains("ready") {
        let _ = child.kill();
        return;
    }

    // Warm up
    for _ in 0..3 {
        let _ = run_generation(&mut stdin, &mut stdout, "Hello", 5, true);
    }

    let mut group = c.benchmark_group("mlx_bridge_ttft");
    group.sample_size(30);

    // Different prompt lengths to see TTFT impact
    let prompts = [
        ("short", "Hi"),
        ("medium", "def hello():"),
        (
            "long",
            "Write a Python function that calculates the factorial of a number",
        ),
    ];

    for (name, prompt) in prompts {
        group.bench_function(name, |b| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    if let Some(result) = run_generation(&mut stdin, &mut stdout, prompt, 1, true) {
                        if let Some(ttft) = result.ttft_ms {
                            total += Duration::from_secs_f64(ttft / 1000.0);
                        }
                    }
                }
                total
            })
        });
    }

    group.finish();

    let _ = writeln!(stdin, r#"{{"type":"shutdown"}}"#);
    let _ = stdin.flush();
    let _ = child.wait();
}

criterion_group!(
    benches,
    bench_mlx_bridge_streaming,
    bench_time_to_first_token,
);

criterion_main!(benches);
