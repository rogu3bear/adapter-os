#![cfg(feature = "prod-gate")]

use adapteros_core::{compute_stack_hash, derive_seed, AosError, B3Hash};
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_lora_kernel_api::MockKernels;
use adapteros_lora_router::ROUTER_GATE_Q15_DENOM;
use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
use adapteros_lora_worker::generation::Generator;
use adapteros_lora_worker::model_handle_cache::{ModelHandle, ModelHandleCache};
use adapteros_lora_worker::model_key::{ModelCacheIdentity, ModelKey};
use adapteros_server_api::request_tracker::RequestTracker;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct GateCheck {
    name: String,
    status: String,
    duration_ms: u128,
    details: Value,
    error: Option<String>,
}

#[derive(Serialize)]
struct GateSummary {
    total: usize,
    passed: usize,
    failed: usize,
}

#[derive(Serialize)]
struct GateReport {
    gate: String,
    status: String,
    timestamp_unix: u64,
    checks: Vec<GateCheck>,
    summary: GateSummary,
}

#[test]
fn prod_gate() {
    let checks = vec![
        run_check("routing_correctness", check_routing_correctness),
        run_check("base_model_residency", check_base_model_residency),
        run_check("adapter_integrity", check_adapter_integrity),
        run_check("generation_parity", check_generation_parity),
        run_check("determinism_envelope", check_determinism_envelope),
        run_check("cancellation", check_cancellation),
        run_check("latency_sanity", check_latency_sanity),
    ];

    let passed = checks.iter().filter(|c| c.status == "pass").count();
    let failed = checks.len() - passed;
    let status = if failed == 0 { "pass" } else { "fail" };

    let report = GateReport {
        gate: "prod-gate".to_string(),
        status: status.to_string(),
        timestamp_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs(),
        checks,
        summary: GateSummary {
            total: passed + failed,
            passed,
            failed,
        },
    };

    let report_path = write_report(&report).unwrap_or_else(|err| {
        panic!("prod gate: failed to write report: {}", err);
    });

    if report.status != "pass" {
        let failed_checks: Vec<&GateCheck> = report
            .checks
            .iter()
            .filter(|c| c.status != "pass")
            .collect();
        panic!(
            "prod gate failed: {} failed checks (report: {})\n{:?}",
            failed_checks.len(),
            report_path.display(),
            failed_checks
                .iter()
                .map(|c| format!("{}: {}", c.name, c.error.clone().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
}

fn run_check<F>(name: &str, f: F) -> GateCheck
where
    F: FnOnce() -> Result<Value, String>,
{
    let start = Instant::now();
    match f() {
        Ok(details) => GateCheck {
            name: name.to_string(),
            status: "pass".to_string(),
            duration_ms: start.elapsed().as_millis(),
            details,
            error: None,
        },
        Err(error) => GateCheck {
            name: name.to_string(),
            status: "fail".to_string(),
            duration_ms: start.elapsed().as_millis(),
            details: json!({}),
            error: Some(error),
        },
    }
}

fn write_report(report: &GateReport) -> Result<PathBuf, String> {
    let path = report_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let payload = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
    std::fs::write(&path, payload).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Get the path for the gate report.
///
/// Reports are intentionally written to `target/` (gitignored) for post-test
/// inspection. Override with `AOS_PROD_GATE_REPORT` environment variable.
fn report_path() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("AOS_PROD_GATE_REPORT") {
        return Ok(PathBuf::from(value));
    }
    Ok(PathBuf::from("target/prod-gate/report.json"))
}

fn build_router_inputs() -> (Vec<f32>, Vec<f32>, Vec<AdapterInfo>, PolicyMask) {
    let features = vec![0.0f32; 22];
    let priors = vec![1.0f32, 1.0f32, 1.0f32];
    let adapter_info = vec![
        AdapterInfo {
            id: "adapter_a".to_string(),
            ..Default::default()
        },
        AdapterInfo {
            id: "adapter_b".to_string(),
            ..Default::default()
        },
        AdapterInfo {
            id: "adapter_c".to_string(),
            ..Default::default()
        },
    ];
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);
    (features, priors, adapter_info, policy_mask)
}

fn check_routing_correctness() -> Result<Value, String> {
    let mut router = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.0);
    let (features, priors, adapter_info, policy_mask) = build_router_inputs();
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
        .map_err(|e| e.to_string())?;
    let indices: Vec<u16> = decision.indices.iter().copied().collect();
    if indices != vec![0, 1] {
        return Err(format!("expected indices [0, 1], got {:?}", indices));
    }
    Ok(json!({
        "indices": indices,
        "gates_q15": decision.gates_q15,
    }))
}

fn check_base_model_residency() -> Result<Value, String> {
    let cache = ModelHandleCache::new(100);
    let backend = BackendType::Metal;
    let identity = ModelCacheIdentity::for_backend(backend);

    let base_key = ModelKey::new(backend, B3Hash::hash(b"base"), identity.clone());
    let adapter_key_1 = ModelKey::new(backend, B3Hash::hash(b"adapter_1"), identity.clone());
    let adapter_key_2 = ModelKey::new(backend, B3Hash::hash(b"adapter_2"), identity);

    cache
        .get_or_load_base_model(&base_key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0u8; 60])), 60))
        })
        .map_err(|e| e.to_string())?;

    if !cache.is_pinned(&base_key) {
        return Err("base model not pinned".to_string());
    }

    cache
        .get_or_load(&adapter_key_1, || {
            Ok((ModelHandle::Metal(Arc::new(vec![1u8; 30])), 30))
        })
        .map_err(|e| e.to_string())?;

    cache
        .get_or_load(&adapter_key_2, || {
            Ok((ModelHandle::Metal(Arc::new(vec![2u8; 30])), 30))
        })
        .map_err(|e| e.to_string())?;

    cache
        .get_or_load(&base_key, || {
            Err(AosError::Internal("base model evicted".to_string()))
        })
        .map_err(|e| e.to_string())?;

    Ok(json!({
        "base_pinned": cache.is_pinned(&base_key),
        "cache_len": cache.len(),
        "memory_bytes": cache.memory_usage(),
    }))
}

fn check_adapter_integrity() -> Result<Value, String> {
    let table = AdapterTable::new();
    let id_a = "adapter_a".to_string();
    let id_b = "adapter_b".to_string();
    let hash_a = B3Hash::hash(b"hash_a");
    let hash_b = B3Hash::hash(b"hash_b");

    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime
        .block_on(async {
            table.preload(id_a.clone(), hash_a, 1).await?;
            table.preload(id_b.clone(), hash_b, 1).await?;
            table.swap(&[id_a.clone(), id_b.clone()], &[]).await?;
            Ok::<(), AosError>(())
        })
        .map_err(|e| e.to_string())?;

    let computed = table.compute_stack_hash();
    let expected = compute_stack_hash(vec![(id_b, hash_b), (id_a, hash_a)]);

    if computed != expected {
        return Err(format!(
            "stack hash mismatch: expected {}, got {}",
            expected.to_hex(),
            computed.to_hex()
        ));
    }

    Ok(json!({
        "stack_hash": computed.to_hex(),
    }))
}

fn generate_tokens_once(seed: &[u8]) -> Result<Vec<u32>, String> {
    let mut generator =
        Generator::new_deterministic(seed, "prod-gate").map_err(|e| e.to_string())?;
    let mut router = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.0);
    let mut kernels = MockKernels::new();
    let initial_tokens = vec![1u32];
    let vocab_size = 16;
    let eos_token = 999;

    // Create adapter info for the 2 adapters configured in the router
    let adapter_info: Vec<AdapterInfo> = (0..2)
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            stable_id: i as u64,
            tier: "persistent".to_string(),
            ..Default::default()
        })
        .collect();

    generator
        .generate_tokens(
            &mut kernels,
            &mut router,
            &adapter_info,
            &vec![0.0; 22],
            initial_tokens,
            6,
            vocab_size,
            eos_token,
        )
        .map_err(|e| e.to_string())
}

fn check_generation_parity() -> Result<Value, String> {
    let seed = b"prod-gate-seed";
    let run_a = generate_tokens_once(seed)?;
    let run_b = generate_tokens_once(seed)?;

    if run_a != run_b {
        return Err(format!("generation mismatch: {:?} vs {:?}", run_a, run_b));
    }

    Ok(json!({
        "token_count": run_a.len(),
        "tokens": run_a,
    }))
}

fn check_determinism_envelope() -> Result<Value, String> {
    let global = B3Hash::hash(b"global-seed");
    let seed_a = derive_seed(&global, "context");
    let seed_b = derive_seed(&global, "context");
    let seed_c = derive_seed(&global, "context-alt");

    if seed_a != seed_b {
        return Err("seed derivation not deterministic".to_string());
    }
    if seed_a == seed_c {
        return Err("seed derivation not context-separated".to_string());
    }
    if (ROUTER_GATE_Q15_DENOM - 32767.0).abs() > f32::EPSILON {
        return Err(format!(
            "unexpected Q15 denominator: {}",
            ROUTER_GATE_Q15_DENOM
        ));
    }

    Ok(json!({
        "q15_denominator": ROUTER_GATE_Q15_DENOM,
        "seed_deterministic": true,
    }))
}

fn check_cancellation() -> Result<Value, String> {
    let tracker = RequestTracker::new();
    let request_id = "req-prod-gate".to_string();
    let token = tracker.register(request_id.clone());

    if token.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("token should start unset".to_string());
    }

    if !tracker.cancel(&request_id) {
        return Err("cancel returned false".to_string());
    }

    if !token.load(std::sync::atomic::Ordering::Acquire) {
        return Err("token not marked cancelled".to_string());
    }

    if !tracker.is_cancelled(&request_id) {
        return Err("tracker did not report cancelled".to_string());
    }

    if !tracker.complete(&request_id) {
        return Err("complete returned false".to_string());
    }

    Ok(json!({
        "cancelled": true,
        "completed": true,
    }))
}

fn check_latency_sanity() -> Result<Value, String> {
    let mut router = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.0);
    let (features, priors, adapter_info, policy_mask) = build_router_inputs();
    let iterations = 200;
    let threshold_ms = 1000;

    let start = Instant::now();
    for _ in 0..iterations {
        router
            .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
            .map_err(|e| e.to_string())?;
    }
    let elapsed = start.elapsed();

    if elapsed > Duration::from_millis(threshold_ms) {
        return Err(format!(
            "latency sanity exceeded: {:?} for {} iterations",
            elapsed, iterations
        ));
    }

    Ok(json!({
        "iterations": iterations,
        "elapsed_ms": elapsed.as_millis(),
        "threshold_ms": threshold_ms,
    }))
}
