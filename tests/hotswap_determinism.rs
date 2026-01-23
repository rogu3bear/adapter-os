//! Hot-swap determinism scenarios for in-flight inference.
//!
//! This test simulates streaming inference pinned to a stack generation while a
//! concurrent swap attempt arrives. Expected outcomes:
//! - Swap while refs are held returns deterministic error (maps to 409 ADAPTER_IN_USE).
//! - In-flight tokens remain identical to a baseline run without swap pressure.
//! - A generation bump with identical adapter content resets KV cache coherence
//!   and preserves determinism for subsequent requests.

use adapteros_core::constants::BYTES_PER_MB;
use adapteros_core::B3Hash;
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
use adapteros_lora_worker::generation::Generator;
use adapteros_lora_worker::kvcache::KvCache;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn streaming_swap_attempt_is_deterministic() {
    const ADAPTER_ID: &str = "adapter-hotswap-determinism";
    let seed = b"hotswap-determinism-seed-32bytes!!";
    let steps = 6;
    let hash = B3Hash::hash(ADAPTER_ID.as_bytes());

    // Baseline run with no swap pressure (shared table/kv for determinism)
    let table = Arc::new(setup_table(ADAPTER_ID, hash).await);
    let kv_cache = Arc::new(Mutex::new(KvCache::new(BYTES_PER_MB)));
    let (baseline_tokens, baseline_reset) = run_stream(
        table.clone(),
        kv_cache.clone(),
        ADAPTER_ID,
        None,
        seed,
        steps,
    )
    .await;
    assert!(
        baseline_reset,
        "initial coherence check should reset KV cache on first run"
    );

    // Scenario: in-flight inference while swap attempt happens
    let infer_table = table.clone();
    let infer_kv = kv_cache.clone();
    let (ready_tx, ready_rx) = oneshot::channel();
    let infer_handle = tokio::spawn(run_stream(
        infer_table,
        infer_kv,
        ADAPTER_ID,
        Some(ready_tx),
        seed,
        steps,
    ));

    let swap_table = table.clone();
    let swap_handle = tokio::spawn(async move {
        ready_rx
            .await
            .expect("inference should hold refs before swap");
        sleep(Duration::from_millis(5)).await;
        swap_table
            .wait_for_zero_refs(&[ADAPTER_ID.to_string()], Duration::from_millis(25))
            .await
    });

    let (tokens_with_swap, reset_flag) = infer_handle.await.expect("join inference task");
    let swap_result = swap_handle.await.expect("join swap task");

    assert!(
        swap_result.is_err(),
        "swap should be rejected while in-flight refcount is held"
    );
    assert_eq!(
        tokens_with_swap, baseline_tokens,
        "in-flight stream must stay deterministic when swap is deferred"
    );
    assert!(
        !reset_flag,
        "KV cache should stay coherent when generation is unchanged"
    );

    // After in-flight completes, bump generation with identical content and ensure determinism
    table
        .preload(ADAPTER_ID.to_string(), hash, 10)
        .await
        .expect("restage same adapter");
    table
        .swap(&[ADAPTER_ID.to_string()], &[ADAPTER_ID.to_string()])
        .await
        .expect("self-swap to bump generation should succeed");
    let (post_tokens, post_reset) = run_stream(
        table.clone(),
        kv_cache.clone(),
        ADAPTER_ID,
        None,
        seed,
        steps,
    )
    .await;
    assert!(post_reset, "KV cache must reset on generation change");
    assert_eq!(
        post_tokens, baseline_tokens,
        "same adapter content + seed should remain deterministic across generation bump"
    );
}

async fn setup_table(adapter_id: &str, hash: B3Hash) -> AdapterTable {
    let table = AdapterTable::new();
    table
        .preload(adapter_id.to_string(), hash, 10)
        .await
        .expect("preload should work");
    table
        .swap(&[adapter_id.to_string()], &[])
        .await
        .expect("initial swap should work");
    table
}

async fn run_stream(
    table: Arc<AdapterTable>,
    kv_cache: Arc<Mutex<KvCache>>,
    adapter_id: &str,
    notify_refs_held: Option<oneshot::Sender<()>>,
    seed: &[u8],
    steps: usize,
) -> (Vec<u32>, bool) {
    let handle = table.get_current_stack_handle();

    // Align KV cache with the captured generation (mirrors infer_internal)
    let reset = {
        let mut kv_guard = kv_cache.lock().unwrap();
        kv_guard
            .ensure_cache_coherence(handle.generation)
            .expect("coherence check should succeed")
    };

    {
        let refcounts = table.refcounts().lock().await;
        assert!(
            refcounts.contains_key(adapter_id),
            "refcount entry must exist before holding references"
        );
    }

    // Hold refcounts for active adapters
    for name in handle.active.keys() {
        table.inc_ref(name).await;
    }
    if let Some(tx) = notify_refs_held {
        let _ = tx.send(());
    }

    let mut generator = Generator::new_deterministic(seed, "hotswap-determinism")
        .expect("generator creation should succeed");
    let logits = vec![0.25_f32; 4];
    let mut tokens = Vec::new();
    for step in 0..steps {
        generator
            .reseed_for_step(step)
            .expect("reseed should succeed");
        tokens.push(
            generator
                .next_token(&logits)
                .expect("mock sampling should succeed"),
        );
        sleep(Duration::from_millis(50)).await;
    }

    // Release refcounts
    for name in handle.active.keys() {
        table.dec_ref(name).await;
    }

    (tokens, reset)
}
