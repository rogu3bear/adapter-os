use adapteros_config::{DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT};
use adapteros_core::Result;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_kernel_coreml::{init_coreml, ComputeUnits, CoreMLBackend, CoreMLModelParams};
use serde_json::Value;
use std::cmp::Ordering;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("CoreML perf example requires macOS");
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    init_coreml()?;

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let default_model = repo_root
        .join(DEFAULT_MODEL_CACHE_ROOT)
        .join(DEFAULT_BASE_MODEL_ID)
        .join("model.mlpackage");
    let model_path = env::var("AOS_COREML_MLMODEL")
        .map(PathBuf::from)
        .unwrap_or(default_model);
    let model_path = model_path
        .canonicalize()
        .unwrap_or_else(|_| model_path.clone());

    let default_config = repo_root
        .join(DEFAULT_MODEL_CACHE_ROOT)
        .join(DEFAULT_BASE_MODEL_ID)
        .join("config.json");
    let config_path = env::var("AOS_COREML_CONFIG")
        .ok()
        .map(PathBuf::from)
        .or(Some(default_config))
        .and_then(|p| p.canonicalize().ok());

    let steps: usize = env::var("COREML_BENCH_STEPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(128);
    let warmup: usize = env::var("COREML_BENCH_WARMUP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    let seq_len: usize = env::var("COREML_BENCH_SEQ_LEN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(512);

    let (vocab_size, params) = load_params(config_path);

    let mut backend = CoreMLBackend::new(ComputeUnits::CpuAndNeuralEngine, true)?;
    backend.set_model_params(params);
    backend.load_model(&model_path)?;

    let ring = RouterRing::new(0);
    let mut io = IoBuffers::new(vocab_size);
    io.input_ids = (0..seq_len)
        .map(|i| (i % vocab_size).try_into().unwrap_or(0u32))
        .collect();

    for _ in 0..warmup {
        run_step_once(&mut backend, &ring, &mut io)?;
    }

    // Sanity check outputs after warmup
    if io.output_logits.iter().any(|v| !v.is_finite()) {
        return Err(adapteros_core::AosError::Kernel(
            "Non-finite logits after warmup; aborting perf run".to_string(),
        ));
    }

    let mut latencies = Vec::with_capacity(steps);
    for _ in 0..steps {
        let elapsed = run_step_once(&mut backend, &ring, &mut io)?;
        latencies.push(elapsed);
    }

    let total: Duration = latencies.iter().fold(Duration::ZERO, |acc, d| acc + *d);
    let tokens_processed = steps as f64 * seq_len as f64;
    let tokens_per_sec = tokens_processed / total.as_secs_f64();
    let p95_ms = percentile_ms(&latencies, 0.95);
    let p95_ms_per_token = p95_ms / seq_len as f64;
    let top_token = io
        .output_logits
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(Ordering::Equal));

    // Optional softmax for monitoring
    let softmax_prob = if env::var("COREML_BENCH_SOFTMAX")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        let max_logit = io
            .output_logits
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        let mut sum_exp = 0.0f32;
        let mut probs = Vec::with_capacity(io.output_logits.len());
        for &l in &io.output_logits {
            let e = (l - max_logit).exp();
            sum_exp += e;
            probs.push(e);
        }
        if sum_exp.is_finite() && sum_exp > 0.0 {
            Some(
                probs
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (i, e / sum_exp))
                    .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal)),
            )
        } else {
            None
        }
    } else {
        None
    };

    println!(
        "coreml_perf_summary model={} seq_len={} steps={} tokens_per_sec={:.2} p95_ms={:.2} p95_ms_per_token={:.4}",
        model_path.display(),
        seq_len,
        steps,
        tokens_per_sec,
        p95_ms,
        p95_ms_per_token
    );
    if let Some((idx, val)) = top_token {
        println!("coreml_perf_top1 token_id={} logit={:.4}", idx, val);
        if let Some(Some((prob_idx, prob_val))) = softmax_prob {
            println!(
                "coreml_perf_top1_prob token_id={} prob={:.6}",
                prob_idx, prob_val
            );
        }
        println!(
            "{{\"tokens_per_sec\":{:.2},\"p95_ms\":{:.2},\"p95_ms_per_token\":{:.4},\"steps\":{},\"seq_len\":{},\"model\":\"{}\",\"top_token\":{},\"top_logit\":{:.4},\"top_prob\":{}}}",
            tokens_per_sec,
            p95_ms,
            p95_ms_per_token,
            steps,
            seq_len,
            model_path.display(),
            idx,
            val,
            softmax_prob
                .and_then(|p| p.map(|(_, v)| v))
                .unwrap_or(f32::NAN)
        );
    } else {
        println!(
            "{{\"tokens_per_sec\":{:.2},\"p95_ms\":{:.2},\"p95_ms_per_token\":{:.4},\"steps\":{},\"seq_len\":{},\"model\":\"{}\"}}",
            tokens_per_sec,
            p95_ms,
            p95_ms_per_token,
            steps,
            seq_len,
            model_path.display()
        );
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn run_step_once(
    backend: &mut CoreMLBackend,
    ring: &RouterRing,
    io: &mut IoBuffers,
) -> Result<Duration> {
    io.position = 0;
    io.output_logits.iter_mut().for_each(|v| *v = 0.0);

    let start = Instant::now();
    backend.run_step(ring, io)?;
    Ok(start.elapsed())
}

fn percentile_ms(samples: &[Duration], percentile: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let idx = ((sorted.len() as f64 - 1.0) * percentile).round() as usize;
    sorted[idx].as_secs_f64() * 1000.0
}

fn load_params(config_path: Option<PathBuf>) -> (usize, CoreMLModelParams) {
    let mut vocab_size = 152_064usize;
    let mut params = CoreMLModelParams::default();

    if let Some(path) = config_path {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(json) = serde_json::from_str::<Value>(&contents) {
                vocab_size = json
                    .get("vocab_size")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(vocab_size);

                params = CoreMLModelParams::new(
                    json.get("hidden_size")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize)
                        .unwrap_or(params.hidden_size),
                    json.get("num_attention_heads")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize)
                        .unwrap_or(params.num_attention_heads),
                    json.get("num_key_value_heads")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize)
                        .unwrap_or(params.num_key_value_heads),
                    json.get("intermediate_size")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize)
                        .unwrap_or(params.intermediate_size),
                    json.get("rope_theta")
                        .and_then(|v| v.as_f64())
                        .map(|v| v as f32)
                        .unwrap_or(params.rope_theta),
                    json.get("max_position_embeddings")
                        .or_else(|| json.get("max_seq_len"))
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize)
                        .unwrap_or(params.max_seq_len),
                );
            }
        }
    }

    (vocab_size, params)
}
