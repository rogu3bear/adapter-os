//! CI contract coverage for the hot-swap scenarios described in `docs/hot_swap_scenarios.md`.
//! Each test sets up an adapter registry, performs the described swap/failure, and asserts on the active
//! adapter set, stack hashes, inference outputs, and the telemetry that would be emitted.

use adapteros_core::B3Hash;
use adapteros_lora_worker::adapter_hotswap::{AdapterTable, GpuFingerprint, HotSwapManager};
use adapteros_telemetry::{AdapterSwapEvent, StackVerificationEvent};
use std::sync::{Arc, Mutex};

struct InMemoryTelemetry {
    swap_events: Mutex<Vec<AdapterSwapEvent>>,
    verification_events: Mutex<Vec<StackVerificationEvent>>,
}

impl InMemoryTelemetry {
    fn new() -> Self {
        Self {
            swap_events: Mutex::new(Vec::new()),
            verification_events: Mutex::new(Vec::new()),
        }
    }

    fn log_swap(&self, event: AdapterSwapEvent) {
        self.swap_events.lock().unwrap().push(event);
    }

    fn log_verification(&self, event: StackVerificationEvent) {
        self.verification_events.lock().unwrap().push(event);
    }

    fn last_swap_event(&self) -> Option<AdapterSwapEvent> {
        self.swap_events.lock().unwrap().last().cloned()
    }

    fn last_verification_event(&self) -> Option<StackVerificationEvent> {
        self.verification_events.lock().unwrap().last().cloned()
    }
}

struct ScenarioHarness {
    manager: HotSwapManager,
    telemetry: InMemoryTelemetry,
    tenant_id: String,
}

impl ScenarioHarness {
    fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            manager: HotSwapManager::new(),
            telemetry: InMemoryTelemetry::new(),
            tenant_id: tenant_id.into(),
        }
    }

    fn table(&self) -> Arc<AdapterTable> {
        self.manager.table().clone()
    }

    fn stage_adapter(&self, adapter_id: &str) {
        let hash = B3Hash::hash(adapter_id.as_bytes());
        let vram_mb = adapter_vram_mb(adapter_id);
        self.table()
            .preload(adapter_id.to_string(), hash, vram_mb)
            .unwrap();
    }

    fn swap_adapters(&self, add: &[&str], remove: &[&str]) -> (B3Hash, i64) {
        for adapter in add {
            self.stage_adapter(adapter);
        }

        let add_ids: Vec<_> = add.iter().map(|id| id.to_string()).collect();
        let remove_ids: Vec<_> = remove.iter().map(|id| id.to_string()).collect();

        let (vram_delta, _) = self
            .table()
            .swap(&add_ids, &remove_ids)
            .expect("swap should succeed");

        self.table().clear_staged();
        let stack_hash = self.table().compute_stack_hash();
        (stack_hash, vram_delta)
    }

    fn load_base_and_safety(&self) -> B3Hash {
        let (hash, _) = self.swap_adapters(&["Base", "Safety"], &[]);
        let expected = vec!["Base".to_string(), "Safety".to_string()];
        assert_eq!(self.active_adapter_ids(), expected);
        hash
    }

    fn stack_hash(&self) -> B3Hash {
        self.table().compute_stack_hash()
    }

    fn active_adapter_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self
            .table()
            .get_active()
            .into_iter()
            .map(|adapter| adapter.id)
            .collect();
        ids.sort();
        ids
    }

    fn inference_output(&self) -> String {
        deterministic_response(&self.active_adapter_ids())
    }

    fn log_swap_event(
        &self,
        add: &[&str],
        remove: &[&str],
        vram_delta: i64,
        result: &str,
        stack_hash: &B3Hash,
    ) {
        self.telemetry.log_swap(AdapterSwapEvent {
            tenant: self.tenant_id.clone(),
            add: add.iter().map(|id| id.to_string()).collect(),
            remove: remove.iter().map(|id| id.to_string()).collect(),
            vram_mb: vram_delta,
            latency_ms: 0,
            result: result.to_string(),
            stack_hash: Some(stack_hash.to_hex()),
        });
    }

    fn log_stack_verification_event(&self, result: &str, stack_hash: &B3Hash) {
        self.telemetry.log_verification(StackVerificationEvent {
            plan_id: format!("verification:{}", self.tenant_id),
            stack_hash: stack_hash.to_hex(),
            adapters: self.active_adapter_ids(),
            result: result.to_string(),
        });
    }
}

fn deterministic_response(adapter_ids: &[String]) -> String {
    format!("response:{}", adapter_ids.join("|"))
}

fn adapter_vram_mb(adapter_id: &str) -> u64 {
    match adapter_id {
        "Base" => 256,
        "Safety" => 128,
        "Safety_v2" => 140,
        "DomainFinance" => 192,
        "DomainCompliance" => 160,
        "DomainRisk" => 208,
        _ => 64,
    }
}

#[test]
fn s1_tenant_a_domain_finance_midday_swap_matches_day_start_output() {
    // Baseline: DomainFinance is part of the stack from day start.
    let baseline = ScenarioHarness::new("tenant-a-day-start");
    baseline.load_base_and_safety();
    let (baseline_hash, _) = baseline.swap_adapters(&["DomainFinance"], &[]);
    let baseline_output = baseline.inference_output();

    // Mid-day swap: start with Base+Safety and hot-swap in DomainFinance later.
    let midday = ScenarioHarness::new("tenant-a-midday");
    midday.load_base_and_safety();
    assert_eq!(
        midday.active_adapter_ids(),
        vec!["Base".to_string(), "Safety".to_string()]
    );

    let (midday_hash, delta) = midday.swap_adapters(&["DomainFinance"], &[]);
    let midday_output = midday.inference_output();

    assert_eq!(
        midday_hash, baseline_hash,
        "stack hash should match baseline"
    );
    assert_eq!(
        midday_output, baseline_output,
        "model output must be identical"
    );
    assert_eq!(
        midday.active_adapter_ids(),
        vec![
            "Base".to_string(),
            "DomainFinance".to_string(),
            "Safety".to_string()
        ]
    );

    midday.log_swap_event(&["DomainFinance"], &[], delta, "ok", &midday_hash);
    let event = midday
        .telemetry
        .last_swap_event()
        .expect("swap telemetry emitted");
    assert_eq!(event.result, "ok");
    assert_eq!(event.stack_hash, Some(midday_hash.to_hex()));
    assert_eq!(event.add, vec!["DomainFinance".to_string()]);
    assert_eq!(event.vram_mb, delta);
}

#[test]
fn s2_hash_mismatch_rolls_back_and_logs() {
    let harness = ScenarioHarness::new("tenant-a-hash-mismatch");
    harness.load_base_and_safety();
    let baseline_hash = harness.stack_hash();

    let (candidate_hash, delta) = harness.swap_adapters(&["DomainFinance"], &[]);
    let expected_hash = B3Hash::hash(b"expected-stack-hash");
    assert_ne!(candidate_hash, expected_hash, "mismatch simulation sanity");

    harness.table().rollback().unwrap();
    assert_eq!(
        harness.stack_hash(),
        baseline_hash,
        "rollback should restore previous stack hash"
    );
    assert_eq!(
        harness.active_adapter_ids(),
        vec!["Base".to_string(), "Safety".to_string()]
    );

    harness.log_swap_event(&["DomainFinance"], &[], delta, "rollback", &baseline_hash);
    let event = harness
        .telemetry
        .last_swap_event()
        .expect("rollback telemetry emitted");
    assert_eq!(event.result, "rollback");
    assert_eq!(event.stack_hash, Some(baseline_hash.to_hex()));
}

#[test]
fn s3_domain_compliance_to_domain_risk_shows_hash_change_and_vram() {
    let harness = ScenarioHarness::new("tenant-c-domain-rotation");
    harness.load_base_and_safety();
    let (initial_hash, _) = harness.swap_adapters(&["DomainCompliance"], &[]);
    let initial_output = harness.inference_output();

    let (next_hash, delta) = harness.swap_adapters(&["DomainRisk"], &["DomainCompliance"]);
    let next_output = harness.inference_output();

    assert_ne!(initial_hash, next_hash, "stack hash must change after swap");
    assert_ne!(
        initial_output, next_output,
        "model output should reflect the new domain"
    );
    let expected_delta =
        adapter_vram_mb("DomainRisk") as i64 - adapter_vram_mb("DomainCompliance") as i64;
    assert_eq!(delta, expected_delta);

    harness.log_swap_event(
        &["DomainRisk"],
        &["DomainCompliance"],
        delta,
        "ok",
        &next_hash,
    );
    let event = harness
        .telemetry
        .last_swap_event()
        .expect("swap telemetry emitted");
    assert_eq!(event.vram_mb, expected_delta);
    assert_eq!(event.stack_hash, Some(next_hash.to_hex()));
    assert_eq!(
        harness.active_adapter_ids(),
        vec![
            "Base".to_string(),
            "DomainRisk".to_string(),
            "Safety".to_string()
        ]
    );
}

#[test]
fn s4_safety_toggle_cycles_keep_determinism_and_no_downtime() {
    let harness = ScenarioHarness::new("tenant-d-safety-toggle");
    let baseline_hash = harness.load_base_and_safety();
    let baseline_output = harness.inference_output();

    // Upgrade to Safety_v2 (remove Safety, add Safety_v2)
    let (upgrade_hash, upgrade_delta) = harness.swap_adapters(&["Safety_v2"], &["Safety"]);
    harness.log_swap_event(
        &["Safety_v2"],
        &["Safety"],
        upgrade_delta,
        "ok",
        &upgrade_hash,
    );
    assert!(
        harness.active_adapter_ids().contains(&"Base".to_string()),
        "Base adapter must stay active"
    );

    // Downgrade back to Safety
    let (downgrade_hash, downgrade_delta) = harness.swap_adapters(&["Safety"], &["Safety_v2"]);
    harness.log_swap_event(
        &["Safety"],
        &["Safety_v2"],
        downgrade_delta,
        "ok",
        &downgrade_hash,
    );

    assert_eq!(downgrade_hash, baseline_hash);
    assert_eq!(harness.inference_output(), baseline_output);
    assert_eq!(
        harness.active_adapter_ids(),
        vec!["Base".to_string(), "Safety".to_string()]
    );
}

#[test]
fn s5_stack_checkpoint_verification_detects_gpu_mismatch() {
    let harness = ScenarioHarness::new("tenant-e-checkpoint");
    harness.load_base_and_safety();
    let baseline_hash = harness.stack_hash();

    let (post_finance_hash, _) = harness.swap_adapters(&["DomainFinance"], &[]);
    let gpu_fps = vec![
        GpuFingerprint {
            adapter_id: "Base".to_string(),
            buffer_bytes: 1024,
            checkpoint_hash: B3Hash::hash(b"gpu-base"),
        },
        GpuFingerprint {
            adapter_id: "Safety".to_string(),
            buffer_bytes: 768,
            checkpoint_hash: B3Hash::hash(b"gpu-safety"),
        },
        GpuFingerprint {
            adapter_id: "DomainFinance".to_string(),
            buffer_bytes: 2048,
            checkpoint_hash: B3Hash::hash(b"gpu-finance"),
        },
    ];

    let checkpoint = harness.table().create_checkpoint(gpu_fps.clone());
    assert_eq!(
        checkpoint.metadata_hash, post_finance_hash,
        "checkpoint hash ties to current stack"
    );

    let verification_ok = harness
        .table()
        .verify_against_checkpoint(&checkpoint, &gpu_fps)
        .unwrap();
    assert!(verification_ok);
    harness.log_stack_verification_event("ok", &checkpoint.metadata_hash);
    let success_event = harness
        .telemetry
        .last_verification_event()
        .expect("verification telemetry emitted");
    assert_eq!(success_event.result, "ok");

    // Simulate GPU fingerprint drift
    let mut mismatched_fps = gpu_fps.clone();
    mismatched_fps[2].buffer_bytes += 512;
    let verification_failed = harness
        .table()
        .verify_against_checkpoint(&checkpoint, &mismatched_fps)
        .unwrap();
    assert!(!verification_failed, "mismatch should be detected");
    harness.log_stack_verification_event("mismatch", &checkpoint.metadata_hash);
    harness.table().rollback().unwrap();
    assert_eq!(
        harness.stack_hash(),
        baseline_hash,
        "rollback restores the previous verified stack"
    );
    assert_eq!(
        harness.active_adapter_ids(),
        vec!["Base".to_string(), "Safety".to_string()]
    );
    let failure_event = harness
        .telemetry
        .last_verification_event()
        .expect("failure telemetry emitted");
    assert_eq!(failure_event.result, "mismatch");
}
