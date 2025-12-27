//! Certification stress cases for deterministic router → worker hot-swaps.
//! - Kaleidoscope: round-robin adapter switching per token must be bit-for-bit stable.
//! - Chaos Mode: injected load jitter cannot perturb output digests.
//! - Router variance: temperature=0.0 in deterministic mode yields hard 1/0 gates.

use adapteros_core::{
    determinism::{DeterminismContext, DeterminismSource},
    B3Hash, Result, SeedMode,
};
use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};
use adapteros_lora_worker::{chaos_mode, generation::Generator};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use sysinfo::{CpuExt, System, SystemExt};
use tokio::task::JoinSet;

const ROUTER_SEED: [u8; 32] = *b"kaleidoscope-router-seed-0000000";
const REPORT_PATH: &str = "var/certification/certification_report.json";
const TOKEN_STEPS: usize = 100;
const RUNS: usize = 10;

#[derive(Debug, Default, Serialize, Deserialize)]
struct CertificationReport {
    seeds: HashMap<String, String>,
    hardware: HardwareSnapshot,
    outputs: HashMap<String, String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HardwareSnapshot {
    cpu: String,
    logical_cores: usize,
    memory_mb: u64,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn kaleidoscope_router_switch_is_deterministic() -> Result<()> {
    let adapters = adapter_catalog();
    let determinism = determinism_ctx();
    let policy_mask = PolicyMask::allow_all(&adapter_ids(&adapters), None);

    let mut tasks = JoinSet::new();
    for _ in 0..RUNS {
        let adapters = adapters.clone();
        let policy = policy_mask.clone();
        let determinism = determinism.clone();
        tasks.spawn(run_kaleidoscope(adapters, policy, determinism, false));
    }

    let mut digests = Vec::with_capacity(RUNS);
    while let Some(result) = tasks.join_next().await {
        digests.push(result.expect("task join failed")?);
    }

    let (baseline_digest, baseline_tokens) = &digests[0];
    for (idx, (digest, tokens)) in digests.iter().enumerate() {
        assert_eq!(
            digest, baseline_digest,
            "output_digest drifted on parallel run {idx}"
        );
        assert_eq!(
            tokens, baseline_tokens,
            "token stream drifted on parallel run {idx}"
        );
    }

    let sha = sha256_tokens(baseline_tokens);
    write_report(&sha)?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chaos_mode_jitter_preserves_output_digest() -> Result<()> {
    let adapters = adapter_catalog();
    let determinism = determinism_ctx();
    let policy_mask = PolicyMask::allow_all(&adapter_ids(&adapters), None);

    // Baseline without chaos flag
    let (baseline_digest, baseline_tokens) = run_kaleidoscope(
        adapters.clone(),
        policy_mask.clone(),
        determinism.clone(),
        false,
    )
    .await?;

    // Enable chaos mode and re-run with injected layer jitter
    std::env::set_var("AOS_WORKER_CHAOS_MODE", "1");
    std::env::set_var("AOS_CHAOS_SEED", "424242");
    let (chaos_digest, chaos_tokens) =
        run_kaleidoscope(adapters, policy_mask, determinism, true).await?;
    std::env::remove_var("AOS_WORKER_CHAOS_MODE");
    std::env::remove_var("AOS_CHAOS_SEED");

    assert_eq!(
        baseline_digest, chaos_digest,
        "Chaos Mode must not perturb output_digest"
    );
    assert_eq!(
        baseline_tokens, chaos_tokens,
        "Chaos Mode must not perturb token stream"
    );

    Ok(())
}

#[test]
fn deterministic_router_zero_temperature_has_zero_entropy() -> Result<()> {
    let adapters = adapter_catalog();
    let determinism = determinism_ctx();
    let policy_mask = PolicyMask::allow_all(&adapter_ids(&adapters), None);

    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 0.0, 0.0);
    router.set_routing_determinism_mode(true);

    let priors = vec![0.9, 0.05, 0.03, 0.02];
    let features = vec![0.0f32; 4];
    let decision = router.route_with_adapter_info_with_ctx(
        &features,
        &priors,
        &adapters,
        &policy_mask,
        Some(&determinism),
    )?;

    assert_eq!(decision.entropy, 0.0, "entropy must be zero when tau=0");
    let positive_gates: Vec<_> = decision
        .gates_q15
        .iter()
        .copied()
        .filter(|g| *g > 0)
        .collect();
    assert_eq!(
        positive_gates.len(),
        1,
        "zero temperature should yield a single active gate"
    );

    Ok(())
}

async fn run_kaleidoscope(
    adapters: Vec<AdapterInfo>,
    policy_mask: PolicyMask,
    determinism: DeterminismContext,
    chaos: bool,
) -> Result<(B3Hash, Vec<u32>)> {
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 1e-6);
    router.set_routing_determinism_mode(true);

    let mut generator = Generator::new_deterministic(&ROUTER_SEED, "kaleidoscope-router");
    let mut tokens = Vec::with_capacity(TOKEN_STEPS);

    for step in 0..TOKEN_STEPS {
        if chaos {
            chaos_mode::maybe_delay_layer(step);
        }

        let target = step % adapters.len();
        let priors: Vec<f32> = (0..adapters.len())
            .map(|idx| if idx == target { 0.95 } else { 0.05 })
            .collect();
        let features = vec![0.0f32; 4];
        let decision = router.route_with_adapter_info_with_ctx(
            &features,
            &priors,
            &adapters,
            &policy_mask,
            Some(&determinism),
        )?;
        assert_eq!(decision.indices.len(), 1);
        assert_eq!(
            decision.indices[0] as usize, target,
            "token {} forced adapter {} but router returned {}",
            step, target, decision.indices[0]
        );

        generator.reseed_for_step(step);
        let logits = deterministic_logits(&adapters[target].id, step);
        let token = generator.next_token(&logits)?;
        tokens.push(token);
    }

    Ok((compute_output_digest(&tokens), tokens))
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
            0.05 + (raw % 9500) as f32 / 10000.0
        })
        .collect()
}

fn compute_output_digest(output_tokens: &[u32]) -> B3Hash {
    let mut buf = Vec::with_capacity(4 + output_tokens.len() * 4);
    buf.extend_from_slice(&(output_tokens.len() as u32).to_le_bytes());
    for t in output_tokens {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    B3Hash::hash(&buf)
}

fn sha256_tokens(tokens: &[u32]) -> String {
    let mut hasher = Sha256::new();
    for token in tokens {
        hasher.update(token.to_le_bytes());
    }
    let digest = hasher.finalize();
    format!("{:x}", digest)
}

fn write_report(output_sha: &str) -> Result<()> {
    let mut sys = System::new_all();
    sys.refresh_cpu();
    sys.refresh_memory();

    let hardware = HardwareSnapshot {
        cpu: sys
            .cpus()
            .get(0)
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        logical_cores: sys.cpus().len(),
        memory_mb: sys.total_memory() / 1024,
    };

    let mut seeds = HashMap::new();
    seeds.insert("router_seed".to_string(), hex::encode(ROUTER_SEED));

    let mut outputs = HashMap::new();
    outputs.insert(
        "kaleidoscope_output_sha256".to_string(),
        output_sha.to_string(),
    );

    let report = CertificationReport {
        seeds,
        hardware,
        outputs,
    };

    let path = PathBuf::from(REPORT_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(path, json)?;
    Ok(())
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
        AdapterInfo {
            id: "adapter-gamma".to_string(),
            framework: Some("metal".to_string()),
            languages: vec![0],
            tier: "persistent".to_string(),
            ..Default::default()
        },
        AdapterInfo {
            id: "adapter-delta".to_string(),
            framework: Some("metal".to_string()),
            languages: vec![0],
            tier: "persistent".to_string(),
            ..Default::default()
        },
    ]
}

fn adapter_ids(adapters: &[AdapterInfo]) -> Vec<String> {
    adapters.iter().map(|a| a.id.clone()).collect()
}

fn determinism_ctx() -> DeterminismContext {
    DeterminismContext::new(
        ROUTER_SEED,
        None,
        SeedMode::Strict,
        RoutingDeterminismMode::Deterministic,
        DeterminismSource::RequestSeedHex,
    )
}
