//! System certification scenarios for deterministic routing and adapter behavior.
//!
//! These tests exercise:
//! 1) Token-by-token router switching with large parallel fan-out.
//! 2) Hot-swap churn during long generation (added below).

#![allow(dead_code)]
#![allow(clippy::cloned_ref_to_slice_refs)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::clone_on_copy)]

use adapteros_core::{
    constants::BYTES_PER_MB,
    determinism::{DeterminismContext, DeterminismSource},
    B3Hash, Result, SeedMode,
};
use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};
use adapteros_lora_worker::{
    adapter_hotswap::AdapterTable, generation::Generator, kvcache::KvCache,
};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use blake3::Hasher;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use tokio::task::JoinSet;

const ROUTER_SEED: [u8; 32] = *b"router-switch-determinism-000000";
const BACKEND_ID: &str = "metal";
const KERNEL_VERSION_ID: &str = "v1.0.0";
const HOTSWAP_SEED: [u8; 32] = *b"hotswap-churn-cert-seed-00000000";
const HOTSWAP_TOKEN_COUNT: usize = 1000;
const MAX_TOKEN_LATENCY_MS: u128 = 200;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn token_by_token_router_switch_is_deterministic_across_parallel_runs() -> Result<()> {
    const RUNS: usize = 1000;
    const STEPS: usize = 16;
    let adapters = adapter_catalog();
    let adapter_ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);
    let determinism = DeterminismContext::new(
        ROUTER_SEED,
        None,
        SeedMode::Strict,
        RoutingDeterminismMode::Deterministic,
        DeterminismSource::RequestSeedHex,
    );

    let mut tasks = JoinSet::new();
    for _ in 0..RUNS {
        tasks.spawn(run_router_switch_sequence(
            adapters.clone(),
            policy_mask.clone(),
            determinism.clone(),
            STEPS,
        ));
    }

    let mut outputs = Vec::with_capacity(RUNS);
    while let Some(result) = tasks.join_next().await {
        outputs.push(result.expect("task join failed")?);
    }

    let (first_output, first_run_head) = outputs[0].clone();
    for (idx, (digest, run_head)) in outputs.into_iter().enumerate() {
        assert_eq!(digest, first_output, "output_digest drifted on run {}", idx);
        assert_eq!(run_head, first_run_head, "run_head drifted on run {}", idx);
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hot_swap_churn_stays_deterministic_and_fast() -> Result<()> {
    let main_adapters = ["adapter-main-a", "adapter-main-b"];
    let baseline = run_generation_with_optional_churn(&main_adapters, false).await?;
    let stressed = run_generation_with_optional_churn(&main_adapters, true).await?;

    assert_eq!(
        baseline.tokens, stressed.tokens,
        "token stream changed under hot-swap churn"
    );
    assert_eq!(
        baseline.output_digest, stressed.output_digest,
        "output digest drifted with background adapter churn"
    );
    assert!(
        stressed.max_latency.as_millis() <= MAX_TOKEN_LATENCY_MS,
        "max token latency {}ms exceeded {}ms budget",
        stressed.max_latency.as_millis(),
        MAX_TOKEN_LATENCY_MS
    );

    Ok(())
}

async fn run_router_switch_sequence(
    adapter_info: Vec<AdapterInfo>,
    policy_mask: PolicyMask,
    determinism: DeterminismContext,
    steps: usize,
) -> Result<(B3Hash, B3Hash)> {
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 1e-6);
    router.set_routing_determinism_mode(true);

    let mut generator = Generator::new_deterministic(&ROUTER_SEED, "system-cert-router-switch");
    let mut output_tokens = Vec::with_capacity(steps);
    let mut run_head = B3Hash::zero();
    let context_digest = context_digest_bytes();

    for step in 0..steps {
        let priors = if step % 2 == 0 {
            vec![0.9, 0.1]
        } else {
            vec![0.1, 0.9]
        };
        let features = vec![0.0f32; 4];
        let decision = router.route_with_adapter_info_with_ctx(
            &features,
            &priors,
            &adapter_info,
            &policy_mask,
            Some(&determinism),
        )?;

        assert_eq!(decision.indices.len(), 1, "decision must be 1-sparse");
        let expected_idx = if step % 2 == 0 { 0 } else { 1 };
        assert_eq!(
            decision.indices[0] as usize, expected_idx,
            "token {} should switch adapters",
            step
        );

        let adapter_ids_for_token = vec![adapter_info[decision.indices[0] as usize].id.clone()];
        generator.reseed_for_step(step);
        let logits = deterministic_logits(&adapter_ids_for_token[0], step);
        let token = generator.next_token(&logits)?;
        output_tokens.push(token);

        let decision_hash = hash_decision(
            &context_digest,
            step as u32,
            &adapter_ids_for_token,
            &decision.gates_q15,
            &policy_mask.digest,
            &policy_mask.allowed,
            BACKEND_ID,
            KERNEL_VERSION_ID,
        );
        run_head = update_run_head(&run_head, step as u32, &decision_hash);
    }

    let output_digest = compute_output_digest(&output_tokens);
    Ok((output_digest, run_head))
}

fn adapter_catalog() -> Vec<AdapterInfo> {
    vec![
        AdapterInfo {
            id: "adapter-alpha".to_string(),
            framework: Some("metal".to_string()),
            languages: vec![0],
            tier: "persistent".to_string(),
            ..Default::default()
        },
        AdapterInfo {
            id: "adapter-beta".to_string(),
            framework: Some("metal".to_string()),
            languages: vec![0],
            tier: "persistent".to_string(),
            ..Default::default()
        },
    ]
}

fn deterministic_logits(adapter_id: &str, step: usize) -> Vec<f32> {
    let mut hasher = Hasher::new();
    hasher.update(adapter_id.as_bytes());
    hasher.update(&(step as u32).to_le_bytes());
    let digest = hasher.finalize();

    digest
        .as_bytes()
        .chunks(4)
        .take(8)
        .map(|chunk| {
            let raw = u32::from_le_bytes(chunk.try_into().expect("4-byte chunk"));
            // Keep logits stable but adapter-specific.
            0.1 + (raw % 9000) as f32 / 10000.0
        })
        .collect()
}

fn encode_adapter_ids(ids: &[String]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + ids.iter().map(|s| s.len() + 4).sum::<usize>());
    out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
    for id in ids {
        let bytes = id.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

fn encode_gates_q15(gates: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + gates.len() * 2);
    out.extend_from_slice(&(gates.len() as u32).to_le_bytes());
    for g in gates {
        out.extend_from_slice(&g.to_le_bytes());
    }
    out
}

fn encode_allowed_mask(mask: &[bool]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + mask.len());
    out.extend_from_slice(&(mask.len() as u32).to_le_bytes());
    out.extend(mask.iter().map(|b| if *b { 1u8 } else { 0u8 }));
    out
}

fn hash_decision(
    context_digest: &[u8; 32],
    token_index: u32,
    adapter_ids: &[String],
    gates_q15: &[i16],
    policy_mask_digest: &B3Hash,
    allowed_mask: &[bool],
    backend_id: &str,
    kernel_version_id: &str,
) -> B3Hash {
    let adapter_blob = encode_adapter_ids(adapter_ids);
    let gates_blob = encode_gates_q15(gates_q15);
    let policy_bytes = policy_mask_digest.as_bytes().to_vec();
    let allowed_bytes = encode_allowed_mask(allowed_mask);
    let backend_bytes = backend_id.as_bytes().to_vec();
    let kernel_bytes = kernel_version_id.as_bytes().to_vec();

    B3Hash::hash_multi(&[
        context_digest,
        &token_index.to_le_bytes(),
        &(adapter_blob.len() as u32).to_le_bytes(),
        &adapter_blob,
        &(gates_blob.len() as u32).to_le_bytes(),
        &gates_blob,
        &(policy_bytes.len() as u32).to_le_bytes(),
        &policy_bytes,
        &(allowed_bytes.len() as u32).to_le_bytes(),
        &allowed_bytes,
        &0u32.to_le_bytes(), // policy_overrides length
        &[],
        &(backend_bytes.len() as u32).to_le_bytes(),
        &backend_bytes,
        &(kernel_bytes.len() as u32).to_le_bytes(),
        &kernel_bytes,
    ])
}

fn update_run_head(prev: &B3Hash, token_index: u32, decision_hash: &B3Hash) -> B3Hash {
    B3Hash::hash_multi(&[
        prev.as_bytes(),
        decision_hash.as_bytes(),
        &token_index.to_le_bytes(),
    ])
}

fn compute_output_digest(output_tokens: &[u32]) -> B3Hash {
    let mut buf = Vec::with_capacity(4 + output_tokens.len() * 4);
    buf.extend_from_slice(&(output_tokens.len() as u32).to_le_bytes());
    for t in output_tokens {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    B3Hash::hash(&buf)
}

fn context_digest_bytes() -> [u8; 32] {
    B3Hash::hash(b"router-switch-context").to_bytes()
}

struct GenerationOutcome {
    tokens: Vec<u32>,
    output_digest: B3Hash,
    max_latency: Duration,
}

async fn run_generation_with_optional_churn(
    main_ids: &[&str],
    enable_churn: bool,
) -> Result<GenerationOutcome> {
    let (table, kv_cache) = setup_main_table(main_ids).await?;
    let mut churn_handle = None;
    let mut stop_tx = None;

    if enable_churn {
        let (tx, rx) = oneshot::channel();
        stop_tx = Some(tx);
        churn_handle = Some(tokio::spawn(churn_adapters(table.clone(), rx)));
    }

    let generation =
        run_long_generation(table.clone(), kv_cache.clone(), HOTSWAP_TOKEN_COUNT).await?;

    if let Some(tx) = stop_tx {
        let _ = tx.send(());
    }
    if let Some(handle) = churn_handle {
        handle.await.expect("churn task join")?;
    }

    Ok(generation)
}

async fn setup_main_table(main_ids: &[&str]) -> Result<(Arc<AdapterTable>, Arc<Mutex<KvCache>>)> {
    let table = Arc::new(AdapterTable::new());
    for id in main_ids {
        let hash = B3Hash::hash(id.as_bytes());
        table.preload((*id).to_string(), hash, 256).await?;
    }
    let add_ids: Vec<String> = main_ids.iter().map(|s| (*s).to_string()).collect();
    let _ = table.swap(&add_ids, &[]).await?;

    let kv_cache = Arc::new(Mutex::new(KvCache::new(8 * BYTES_PER_MB)));
    Ok((table, kv_cache))
}

async fn run_long_generation(
    table: Arc<AdapterTable>,
    kv_cache: Arc<Mutex<KvCache>>,
    steps: usize,
) -> Result<GenerationOutcome> {
    let handle = table.get_current_stack_handle();
    {
        let mut kv_guard = kv_cache.lock().unwrap();
        kv_guard.ensure_cache_coherence(handle.generation)?;
    }

    let mut active_ids: Vec<String> = handle.active.keys().cloned().collect();
    active_ids.sort();
    assert!(
        !active_ids.is_empty(),
        "active adapter set should not be empty during generation"
    );
    for id in &active_ids {
        table.inc_ref(id).await;
    }

    let mut generator = Generator::new_deterministic(&HOTSWAP_SEED, "hotswap-churn");
    let mut tokens = Vec::with_capacity(steps);
    let mut max_latency = Duration::from_millis(0);

    for (step, adapter_id) in active_ids.iter().cycle().take(steps).enumerate() {
        generator.reseed_for_step(step);
        let start = Instant::now();
        let logits = deterministic_logits(adapter_id, step);
        let token = generator.next_token(&logits)?;
        let elapsed = start.elapsed();
        if elapsed > max_latency {
            max_latency = elapsed;
        }
        tokens.push(token);
    }

    for id in &active_ids {
        table.dec_ref(id).await;
    }

    let output_digest = compute_output_digest(&tokens);
    Ok(GenerationOutcome {
        tokens,
        output_digest,
        max_latency,
    })
}

async fn churn_adapters(table: Arc<AdapterTable>, mut stop: oneshot::Receiver<()>) -> Result<()> {
    let mut idx: u32 = 0;
    loop {
        tokio::select! {
            _ = &mut stop => break,
            res = churn_once(table.clone(), idx) => {
                res?;
                idx = idx.wrapping_add(1);
            }
        }
    }
    Ok(())
}

async fn churn_once(table: Arc<AdapterTable>, idx: u32) -> Result<()> {
    let adapter_id = format!("churn-temp-{}", idx % 8);
    let hash = B3Hash::hash(adapter_id.as_bytes());
    let vram_mb = 64 + (idx % 32) as u64;

    table.preload(adapter_id.clone(), hash, vram_mb).await?;
    let _ = table.swap(&[adapter_id.clone()], &[]).await?;
    let _ = table.swap(&[], &[adapter_id]).await?;
    tokio::time::sleep(Duration::from_millis(2)).await;
    Ok(())
}
