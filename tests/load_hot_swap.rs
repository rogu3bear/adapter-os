//! Load and churn scenarios for adapter hot-swaps and training overlap.
//! - Thundering Herd: adapter churn at 10Hz while streaming inference keeps p99 < 200ms.
//! - Training overlap: concurrent training-style load degrades latency linearly and preserves determinism.
//! - Memory leak guard: optional 1h soak (ignored by default) to watch RSS under churn.

#![allow(clippy::cloned_ref_to_slice_refs)]

use adapteros_core::{constants::BYTES_PER_MB, B3Hash, Result};
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
use adapteros_lora_worker::generation::Generator;
use adapteros_lora_worker::kvcache::KvCache;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use sysinfo::System;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

const STREAM_STEPS: usize = 100;
const STREAM_DELAY_MS: u64 = 10;
const STREAM_SEED: &[u8] = b"herd-stream-seed-for-tests-00000";
const STREAM_CONTEXT: &str = "herd-stream-context";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn thundering_herd_p99_under_200ms_and_deterministic() -> Result<()> {
    let table = setup_table("herd-base").await?;
    let kv_cache = Arc::new(Mutex::new(KvCache::new(BYTES_PER_MB)));

    let (baseline_tokens, _) =
        run_inference_stream(table.clone(), kv_cache.clone(), STREAM_CONTEXT).await?;

    let churn_handle = tokio::spawn(churn_adapters(table.clone(), 120));
    let (herd_tokens, herd_latencies) =
        run_inference_stream(table.clone(), kv_cache.clone(), STREAM_CONTEXT).await?;
    churn_handle.await.expect("join churn task")?;

    assert_eq!(
        baseline_tokens, herd_tokens,
        "determinism must hold with continuous load/unload churn"
    );

    let p99 = percentile_ms(&herd_latencies, 99);
    assert!(
        p99 <= 200.0,
        "p99 latency {:.1}ms exceeded 200ms limit under herd load",
        p99
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_training_load_degrades_linearly() -> Result<()> {
    let table = setup_table("herd-training").await?;
    let kv_cache = Arc::new(Mutex::new(KvCache::new(BYTES_PER_MB)));

    let (baseline_tokens, baseline_latencies) =
        run_inference_stream(table.clone(), kv_cache.clone(), STREAM_CONTEXT).await?;
    let baseline_p99 = percentile_ms(&baseline_latencies, 99);

    let stop = Arc::new(AtomicBool::new(false));
    let training_handle = spawn_training_load(stop.clone());
    let (tokens_with_training, latencies_with_training) =
        run_inference_stream(table.clone(), kv_cache.clone(), STREAM_CONTEXT).await?;
    stop.store(true, Ordering::Relaxed);
    training_handle.await.expect("join training load");

    assert_eq!(
        baseline_tokens, tokens_with_training,
        "token stream must stay deterministic under training overlap"
    );

    let training_p99 = percentile_ms(&latencies_with_training, 99);
    assert!(
        training_p99 <= baseline_p99 * 3.0,
        "latency degraded superlinearly: baseline p99 {:.1}ms vs {:.1}ms with training",
        baseline_p99,
        training_p99
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "1h soak; run manually under valgrind or profiler"]
async fn thundering_herd_memory_leak_sentinel() -> Result<()> {
    let duration_secs = std::env::var("HERD_LEAK_DURATION_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3600);
    let sample_secs = std::env::var("HERD_LEAK_SAMPLE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let table = setup_table("herd-leak").await?;
    let kv_cache = Arc::new(Mutex::new(KvCache::new(BYTES_PER_MB)));

    let start_mem = current_rss_mb();
    let start = Instant::now();
    let mut max_mem = start_mem;

    while start.elapsed().as_secs() < duration_secs as u64 {
        let churn_handle = tokio::spawn(churn_adapters(table.clone(), 50));
        let _ = run_inference_stream(table.clone(), kv_cache.clone(), STREAM_CONTEXT).await?;
        churn_handle.await.expect("join churn task")?;

        sleep(Duration::from_secs(sample_secs as u64)).await;
        max_mem = max_mem.max(current_rss_mb());
    }

    let growth = max_mem.saturating_sub(start_mem);
    assert!(
        growth < 128,
        "RSS grew by {} MB during herd soak (start {} MB)",
        growth,
        start_mem
    );

    Ok(())
}

async fn setup_table(base_id: &str) -> Result<Arc<AdapterTable>> {
    let table = Arc::new(AdapterTable::new());
    let hash = B3Hash::hash(base_id.as_bytes());
    table
        .preload(base_id.to_string(), hash, 16)
        .await
        .expect("preload base adapter");
    table
        .swap(&[base_id.to_string()], &[])
        .await
        .expect("activate base adapter");
    Ok(table)
}

async fn run_inference_stream(
    table: Arc<AdapterTable>,
    kv_cache: Arc<Mutex<KvCache>>,
    generator_tag: &str,
) -> Result<(Vec<u32>, Vec<Duration>)> {
    let handle = table.get_current_stack_handle();

    {
        let mut kv_guard = kv_cache.lock().expect("kv cache poisoned");
        let _ = kv_guard.ensure_cache_coherence(handle.generation)?;
    }

    for name in handle.active.keys() {
        table.inc_ref(name).await;
    }

    let mut generator = Generator::new_deterministic(STREAM_SEED, generator_tag);
    let logits = vec![0.3f32; 6];
    let mut tokens = Vec::with_capacity(STREAM_STEPS);
    let mut latencies = Vec::with_capacity(STREAM_STEPS);

    for step in 0..STREAM_STEPS {
        let start = Instant::now();
        generator.reseed_for_step(step);
        tokens.push(generator.next_token(&logits)?);
        sleep(Duration::from_millis(STREAM_DELAY_MS)).await;
        latencies.push(start.elapsed());
    }

    for name in handle.active.keys() {
        table.dec_ref(name).await;
    }

    Ok((tokens, latencies))
}

async fn churn_adapters(table: Arc<AdapterTable>, iterations: usize) -> Result<()> {
    let mut last: Option<String> = None;
    for i in 0..iterations {
        let id = format!("noise-{i}");
        let hash = B3Hash::hash(id.as_bytes());
        table.preload(id.clone(), hash, 8).await?;
        let remove = last.iter().cloned().collect::<Vec<_>>();
        table.swap(&[id.clone()], &remove).await?;
        last = Some(id);
        sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

fn percentile_ms(latencies: &[Duration], pct: usize) -> f64 {
    if latencies.is_empty() {
        return 0.0;
    }
    let mut samples: Vec<f64> = latencies.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = (((pct as f64 / 100.0) * (samples.len() as f64 - 1.0)).round() as usize)
        .min(samples.len() - 1);
    samples[idx]
}

fn spawn_training_load(stop: Arc<AtomicBool>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut rng = StdRng::seed_from_u64(4242);
        while !stop.load(Ordering::Relaxed) {
            let payload: Vec<u8> = (0..256).map(|_| rng.gen()).collect();
            tokio::task::spawn_blocking(move || {
                let _digest = blake3::hash(&payload);
            });
            sleep(Duration::from_millis(5)).await;
        }
    })
}

fn current_rss_mb() -> u64 {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.used_memory() / 1024
}
