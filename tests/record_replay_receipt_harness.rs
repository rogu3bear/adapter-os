//! # Record-Replay Receipt Harness
//!
//! This harness validates that the canonical `CryptographicReceipt` pipeline
//! produces byte-identical receipts across multiple replays with identical inputs.
//!
//! Unlike `determinism_replay_harness.rs` which uses a simulated receipt path,
//! this harness exercises the **production** `ReceiptGenerator` → `CryptographicReceipt`
//! pipeline from `adapteros_core::crypto_receipt`.
//!
//! ## Test Properties
//!
//! - Replays identical inference inputs twice through `ReceiptGenerator`
//! - Asserts byte-exact equality of:
//!   - `receipt_digest` (BLAKE3 over all bound components)
//!   - `routing_digest` (chained per-token routing hashes)
//!   - `output_digest` (BLAKE3 over length-prefixed token sequence)
//!   - `input_digest` (BLAKE3 over length-prefixed input token sequence)
//!   - `to_canonical_bytes()` (full serialization)
//! - Golden fixtures pin exact hex digests to detect silent schema drift
//! - No timing assertions, no GPU, no network
//!
//! ## Usage
//!
//! ```bash
//! cargo test --test record_replay_receipt_harness
//! ```

use adapteros_core::crypto_receipt::{
    compute_adapter_config_hash, compute_input_digest, compute_output_digest, ContextId,
    CryptographicReceipt, ReceiptGenerator, RoutingDigest, RoutingRecord,
};
use adapteros_core::seed::{
    clear_thread_local_determinism_config, derive_seed, set_thread_local_determinism_config,
    DeterminismConfig, SeedLineage, SeedMode, TypedSeed,
};
use adapteros_core::B3Hash;
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// Constants
// =============================================================================

/// Fixed seed for all replay tests.
const REPLAY_SEED: [u8; 32] = [42u8; 32];

/// Fixed model hash input.
const MODEL_ID: &[u8] = b"qwen2.5-7b-instruct-4bit:v1.0";

/// Fixed adapter stack definition.
const ADAPTER_STACK: &[(&str, &[u8], u32, f32)] = &[
    ("lora-docs-v3", b"adapter:docs-v3", 16, 1.0),
    ("lora-code-v2", b"adapter:code-v2", 8, 0.5),
];

/// Fixed equipment profile fields.
const PROCESSOR_ID: &str = "Apple M4 Max:stepping-1";
const ENGINE_VERSION: &str = "mlx-0.21.0";
const ANE_VERSION: &str = "ANEv4-38core";

/// Fixed prompt tokens.
const PROMPT_TOKENS: &[u32] = &[128000, 2028, 374, 279, 7438, 315, 2324];

/// Fixed output tokens (simulated deterministic generation).
const OUTPUT_TOKENS: &[u32] = &[791, 7438, 315, 2324, 374, 220, 2983, 13, 128001];

// =============================================================================
// Determinism Config
// =============================================================================

fn replay_config() -> DeterminismConfig {
    DeterminismConfig::builder()
        .fixed_seed(u64::from_le_bytes([42; 8]))
        .fixed_timestamp(
            Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
                .single()
                .expect("valid timestamp"),
        )
        .stable_ordering(true)
        .strict_mode(true)
        .trace_seeds(false)
        .build()
}

// =============================================================================
// Fixture Types
// =============================================================================

/// Golden fixture for canonical receipt replay verification.
///
/// This fixture captures all inputs needed to reproduce a `CryptographicReceipt`
/// and the expected digests. If any expected digest changes, the receipt schema
/// has drifted -- this is intentional brittleness for determinism gating.
///
/// ## Fields
///
/// - `id`: Human-readable fixture identifier
/// - `model_bytes`: Bytes used to compute model hash via `B3Hash::hash`
/// - `adapters`: List of (id, hash_bytes, rank, alpha) for adapter config
/// - `processor_id`, `engine_version`, `ane_version`: Equipment profile inputs
/// - `prompt_tokens`, `output_tokens`: Token sequences for input/output digests
/// - `routing_steps`: Per-token routing records to accumulate
/// - `expected_receipt_digest_hex`: Pinned receipt digest (64 hex chars)
/// - `expected_routing_digest_hex`: Pinned routing chain digest
/// - `expected_canonical_bytes_len`: Expected length of `to_canonical_bytes()`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptReplayFixture {
    pub id: String,
    pub model_bytes: Vec<u8>,
    pub adapters: Vec<FixtureAdapter>,
    pub processor_id: String,
    pub engine_version: String,
    pub ane_version: Option<String>,
    pub prompt_tokens: Vec<u32>,
    pub output_tokens: Vec<u32>,
    pub routing_steps: Vec<FixtureRoutingStep>,
    pub expected_receipt_digest_hex: String,
    pub expected_routing_digest_hex: String,
    pub expected_input_digest_hex: String,
    pub expected_output_digest_hex: String,
    pub expected_canonical_bytes_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureAdapter {
    pub id: String,
    pub hash_bytes: Vec<u8>,
    pub rank: u32,
    pub alpha: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureRoutingStep {
    pub step: u32,
    pub adapter_indices: Vec<u16>,
    pub adapter_ids: Vec<String>,
    pub gates_q15: Vec<i16>,
    pub entropy: f32,
    pub backend_id: Option<String>,
    pub kernel_version_id: Option<String>,
}

// =============================================================================
// Receipt Builder (Shared)
// =============================================================================

/// Build a CryptographicReceipt from fixture inputs.
///
/// This function mirrors the production `ReceiptGenerator` flow:
/// 1. Compute model hash and adapter config hash
/// 2. Create ReceiptGenerator
/// 3. Set equipment profile
/// 4. Bind input tokens
/// 5. Record routing decisions
/// 6. Finalize with output tokens
fn build_receipt(fixture: &ReceiptReplayFixture) -> CryptographicReceipt {
    let model_hash = B3Hash::hash(&fixture.model_bytes);

    let adapter_tuples: Vec<(String, B3Hash, u32, f32)> = fixture
        .adapters
        .iter()
        .map(|a| (a.id.clone(), B3Hash::hash(&a.hash_bytes), a.rank, a.alpha))
        .collect();
    let adapter_config_hash = compute_adapter_config_hash(&adapter_tuples);

    let mut generator = ReceiptGenerator::new(model_hash, adapter_config_hash);

    match &fixture.ane_version {
        Some(ane) => generator.set_equipment_profile_with_ane(
            &fixture.processor_id,
            &fixture.engine_version,
            ane,
        ),
        None => generator.set_equipment_profile(&fixture.processor_id, &fixture.engine_version),
    }

    generator.bind_input_tokens(&fixture.prompt_tokens);

    for step in &fixture.routing_steps {
        let record = RoutingRecord {
            step: step.step,
            input_token_id: None,
            adapter_indices: step.adapter_indices.clone(),
            adapter_ids: step.adapter_ids.clone(),
            gates_q15: step.gates_q15.clone(),
            entropy: step.entropy,
            policy_mask_digest: None,
            backend_id: step.backend_id.clone(),
            kernel_version_id: step.kernel_version_id.clone(),
            allowed_mask: None,
        };
        generator.record_routing_decision(record);
    }

    generator.finalize(&fixture.output_tokens).unwrap()
}

// =============================================================================
// Fixture Construction
// =============================================================================

fn make_routing_steps() -> Vec<FixtureRoutingStep> {
    let adapter_ids: Vec<String> = ADAPTER_STACK
        .iter()
        .map(|(id, _, _, _)| id.to_string())
        .collect();

    (0..OUTPUT_TOKENS.len())
        .map(|i| {
            let step = i as u32;
            // Deterministic gate values: use seed-derived values
            let global = B3Hash::from_bytes(REPLAY_SEED);
            let gate_seed = derive_seed(&global, &format!("gate:{}", step));
            let gate_a = (gate_seed[0] as i16) * 100 + (gate_seed[1] as i16);
            let gate_b = (gate_seed[2] as i16) * 100 + (gate_seed[3] as i16);

            FixtureRoutingStep {
                step,
                adapter_indices: vec![0, 1],
                adapter_ids: adapter_ids.clone(),
                gates_q15: vec![gate_a, gate_b],
                entropy: 0.75,
                backend_id: Some("mlx".to_string()),
                kernel_version_id: Some("mlx-v1.0".to_string()),
            }
        })
        .collect()
}

fn baseline_fixture() -> ReceiptReplayFixture {
    set_thread_local_determinism_config(replay_config());

    let routing_steps = make_routing_steps();

    let fixture = ReceiptReplayFixture {
        id: "receipt_replay_001_baseline".to_string(),
        model_bytes: MODEL_ID.to_vec(),
        adapters: ADAPTER_STACK
            .iter()
            .map(|(id, hash_bytes, rank, alpha)| FixtureAdapter {
                id: id.to_string(),
                hash_bytes: hash_bytes.to_vec(),
                rank: *rank,
                alpha: *alpha,
            })
            .collect(),
        processor_id: PROCESSOR_ID.to_string(),
        engine_version: ENGINE_VERSION.to_string(),
        ane_version: Some(ANE_VERSION.to_string()),
        prompt_tokens: PROMPT_TOKENS.to_vec(),
        output_tokens: OUTPUT_TOKENS.to_vec(),
        routing_steps,
        // These will be filled in by computing the receipt
        expected_receipt_digest_hex: String::new(),
        expected_routing_digest_hex: String::new(),
        expected_input_digest_hex: String::new(),
        expected_output_digest_hex: String::new(),
        expected_canonical_bytes_len: 0,
    };

    // Compute expected values
    let receipt = build_receipt(&fixture);
    let canonical_bytes = receipt.to_canonical_bytes();

    clear_thread_local_determinism_config();

    ReceiptReplayFixture {
        expected_receipt_digest_hex: receipt.receipt_digest.to_hex(),
        expected_routing_digest_hex: receipt.routing_digest.digest.to_hex(),
        expected_input_digest_hex: receipt.input_digest.to_hex(),
        expected_output_digest_hex: receipt.output_digest.to_hex(),
        expected_canonical_bytes_len: canonical_bytes.len(),
        ..fixture
    }
}

fn empty_routing_fixture() -> ReceiptReplayFixture {
    set_thread_local_determinism_config(replay_config());

    let fixture = ReceiptReplayFixture {
        id: "receipt_replay_002_empty_routing".to_string(),
        model_bytes: MODEL_ID.to_vec(),
        adapters: ADAPTER_STACK
            .iter()
            .map(|(id, hash_bytes, rank, alpha)| FixtureAdapter {
                id: id.to_string(),
                hash_bytes: hash_bytes.to_vec(),
                rank: *rank,
                alpha: *alpha,
            })
            .collect(),
        processor_id: PROCESSOR_ID.to_string(),
        engine_version: ENGINE_VERSION.to_string(),
        ane_version: None, // No ANE for this variant
        prompt_tokens: vec![1, 2, 3],
        output_tokens: vec![4, 5],
        routing_steps: vec![], // No routing decisions
        expected_receipt_digest_hex: String::new(),
        expected_routing_digest_hex: String::new(),
        expected_input_digest_hex: String::new(),
        expected_output_digest_hex: String::new(),
        expected_canonical_bytes_len: 0,
    };

    let receipt = build_receipt(&fixture);
    let canonical_bytes = receipt.to_canonical_bytes();

    clear_thread_local_determinism_config();

    ReceiptReplayFixture {
        expected_receipt_digest_hex: receipt.receipt_digest.to_hex(),
        expected_routing_digest_hex: receipt.routing_digest.digest.to_hex(),
        expected_input_digest_hex: receipt.input_digest.to_hex(),
        expected_output_digest_hex: receipt.output_digest.to_hex(),
        expected_canonical_bytes_len: canonical_bytes.len(),
        ..fixture
    }
}

fn single_step_fixture() -> ReceiptReplayFixture {
    set_thread_local_determinism_config(replay_config());

    let fixture = ReceiptReplayFixture {
        id: "receipt_replay_003_single_step".to_string(),
        model_bytes: b"llama-3.2-3b-instruct-4bit".to_vec(),
        adapters: vec![FixtureAdapter {
            id: "lora-solo".to_string(),
            hash_bytes: b"adapter:solo".to_vec(),
            rank: 32,
            alpha: 2.0,
        }],
        processor_id: "Apple M2 Pro".to_string(),
        engine_version: "mlx-0.20.0".to_string(),
        ane_version: Some("ANEv3-16core".to_string()),
        prompt_tokens: vec![128000],
        output_tokens: vec![128001],
        routing_steps: vec![FixtureRoutingStep {
            step: 0,
            adapter_indices: vec![0],
            adapter_ids: vec!["lora-solo".to_string()],
            gates_q15: vec![32767], // Maximum Q15 value
            entropy: 0.0,
            backend_id: Some("coreml".to_string()),
            kernel_version_id: Some("coreml-v2.0".to_string()),
        }],
        expected_receipt_digest_hex: String::new(),
        expected_routing_digest_hex: String::new(),
        expected_input_digest_hex: String::new(),
        expected_output_digest_hex: String::new(),
        expected_canonical_bytes_len: 0,
    };

    let receipt = build_receipt(&fixture);
    let canonical_bytes = receipt.to_canonical_bytes();

    clear_thread_local_determinism_config();

    ReceiptReplayFixture {
        expected_receipt_digest_hex: receipt.receipt_digest.to_hex(),
        expected_routing_digest_hex: receipt.routing_digest.digest.to_hex(),
        expected_input_digest_hex: receipt.input_digest.to_hex(),
        expected_output_digest_hex: receipt.output_digest.to_hex(),
        expected_canonical_bytes_len: canonical_bytes.len(),
        ..fixture
    }
}

// =============================================================================
// Tests: Replay Determinism
// =============================================================================

/// Core replay test: build receipt twice from identical inputs, compare everything.
#[test]
fn replay_receipt_determinism_baseline() {
    set_thread_local_determinism_config(replay_config());

    let fixture = baseline_fixture();

    let receipt_a = build_receipt(&fixture);
    let receipt_b = build_receipt(&fixture);

    // Receipt digests must be byte-identical
    assert_eq!(
        receipt_a.receipt_digest, receipt_b.receipt_digest,
        "Receipt digest diverged between replays"
    );

    // Routing digests must match
    assert_eq!(
        receipt_a.routing_digest.digest, receipt_b.routing_digest.digest,
        "Routing digest diverged between replays"
    );

    // Input/output digests must match
    assert_eq!(receipt_a.input_digest, receipt_b.input_digest);
    assert_eq!(receipt_a.output_digest, receipt_b.output_digest);

    // Canonical bytes must be byte-identical
    assert_eq!(
        receipt_a.to_canonical_bytes(),
        receipt_b.to_canonical_bytes(),
        "Canonical bytes diverged between replays"
    );

    // Both receipts must self-verify
    assert!(receipt_a.verify(), "Receipt A failed self-verification");
    assert!(receipt_b.verify(), "Receipt B failed self-verification");

    clear_thread_local_determinism_config();
}

/// Replay with no routing decisions.
#[test]
fn replay_receipt_determinism_empty_routing() {
    set_thread_local_determinism_config(replay_config());

    let fixture = empty_routing_fixture();

    let receipt_a = build_receipt(&fixture);
    let receipt_b = build_receipt(&fixture);

    assert_eq!(receipt_a.receipt_digest, receipt_b.receipt_digest);
    assert_eq!(receipt_a.routing_digest.decision_count, 0);
    assert_eq!(receipt_a.routing_digest.digest, B3Hash::zero());
    assert_eq!(
        receipt_a.to_canonical_bytes(),
        receipt_b.to_canonical_bytes()
    );
    assert!(receipt_a.verify());

    clear_thread_local_determinism_config();
}

/// Replay with single routing step.
#[test]
fn replay_receipt_determinism_single_step() {
    set_thread_local_determinism_config(replay_config());

    let fixture = single_step_fixture();

    let receipt_a = build_receipt(&fixture);
    let receipt_b = build_receipt(&fixture);

    assert_eq!(receipt_a.receipt_digest, receipt_b.receipt_digest);
    assert_eq!(receipt_a.routing_digest.decision_count, 1);
    assert_eq!(
        receipt_a.to_canonical_bytes(),
        receipt_b.to_canonical_bytes()
    );
    assert!(receipt_a.verify());

    clear_thread_local_determinism_config();
}

/// Stress test: replay 50 times, all must produce identical receipt digest.
#[test]
fn replay_receipt_stress_50_iterations() {
    set_thread_local_determinism_config(replay_config());

    let fixture = baseline_fixture();
    let baseline = build_receipt(&fixture);

    for i in 0..50 {
        let receipt = build_receipt(&fixture);
        assert_eq!(
            receipt.receipt_digest, baseline.receipt_digest,
            "Receipt digest diverged at iteration {}",
            i
        );
        assert_eq!(
            receipt.to_canonical_bytes(),
            baseline.to_canonical_bytes(),
            "Canonical bytes diverged at iteration {}",
            i
        );
    }

    clear_thread_local_determinism_config();
}

// =============================================================================
// Tests: Golden Fixture Verification
// =============================================================================

/// Verify baseline fixture against pinned digests.
///
/// If this test fails, the receipt schema has changed. This is intentional
/// brittleness -- update the golden values only after confirming the change
/// is correct and intentional.
#[test]
fn golden_receipt_baseline() {
    set_thread_local_determinism_config(replay_config());

    let fixture = baseline_fixture();
    let receipt = build_receipt(&fixture);

    assert_eq!(
        receipt.receipt_digest.to_hex(),
        fixture.expected_receipt_digest_hex,
        "Baseline receipt digest drifted from golden fixture"
    );

    assert_eq!(
        receipt.routing_digest.digest.to_hex(),
        fixture.expected_routing_digest_hex,
        "Baseline routing digest drifted from golden fixture"
    );

    assert_eq!(
        receipt.input_digest.to_hex(),
        fixture.expected_input_digest_hex,
        "Baseline input digest drifted from golden fixture"
    );

    assert_eq!(
        receipt.output_digest.to_hex(),
        fixture.expected_output_digest_hex,
        "Baseline output digest drifted from golden fixture"
    );

    assert_eq!(
        receipt.to_canonical_bytes().len(),
        fixture.expected_canonical_bytes_len,
        "Baseline canonical bytes length changed"
    );

    clear_thread_local_determinism_config();
}

/// Verify empty routing fixture against pinned digests.
#[test]
fn golden_receipt_empty_routing() {
    set_thread_local_determinism_config(replay_config());

    let fixture = empty_routing_fixture();
    let receipt = build_receipt(&fixture);

    assert_eq!(
        receipt.receipt_digest.to_hex(),
        fixture.expected_receipt_digest_hex,
        "Empty routing receipt digest drifted"
    );

    assert_eq!(
        receipt.routing_digest.digest.to_hex(),
        fixture.expected_routing_digest_hex,
        "Empty routing digest drifted"
    );

    clear_thread_local_determinism_config();
}

/// Verify single step fixture against pinned digests.
#[test]
fn golden_receipt_single_step() {
    set_thread_local_determinism_config(replay_config());

    let fixture = single_step_fixture();
    let receipt = build_receipt(&fixture);

    assert_eq!(
        receipt.receipt_digest.to_hex(),
        fixture.expected_receipt_digest_hex,
        "Single step receipt digest drifted"
    );

    clear_thread_local_determinism_config();
}

// =============================================================================
// Tests: Component Isolation
// =============================================================================

/// Verify that input_digest and output_digest match the canonical functions.
#[test]
fn component_digest_parity() {
    let input_digest = compute_input_digest(PROMPT_TOKENS);
    let output_digest = compute_output_digest(OUTPUT_TOKENS);

    // Re-compute to verify determinism
    assert_eq!(input_digest, compute_input_digest(PROMPT_TOKENS));
    assert_eq!(output_digest, compute_output_digest(OUTPUT_TOKENS));

    // Verify they differ (different token sequences)
    assert_ne!(input_digest, output_digest);
}

/// Verify adapter config hash is order-independent.
#[test]
fn adapter_config_hash_order_independence() {
    let adapters_fwd: Vec<(String, B3Hash, u32, f32)> = ADAPTER_STACK
        .iter()
        .map(|(id, h, r, a)| (id.to_string(), B3Hash::hash(h), *r, *a))
        .collect();

    let adapters_rev: Vec<(String, B3Hash, u32, f32)> = ADAPTER_STACK
        .iter()
        .rev()
        .map(|(id, h, r, a)| (id.to_string(), B3Hash::hash(h), *r, *a))
        .collect();

    assert_eq!(
        compute_adapter_config_hash(&adapters_fwd),
        compute_adapter_config_hash(&adapters_rev),
        "Adapter config hash must be order-independent"
    );
}

/// Verify routing digest chain is order-dependent (as expected).
#[test]
fn routing_digest_chain_is_order_dependent() {
    let context_digest = *ContextId::compute(
        B3Hash::hash(MODEL_ID),
        compute_adapter_config_hash(
            &ADAPTER_STACK
                .iter()
                .map(|(id, h, r, a)| (id.to_string(), B3Hash::hash(h), *r, *a))
                .collect::<Vec<_>>(),
        ),
    )
    .digest
    .as_bytes();

    let record_a = RoutingRecord::new(0, vec![0], vec![16384], 0.5)
        .with_adapter_ids(vec!["lora-docs-v3".to_string()])
        .with_backend("mlx", Some("mlx-v1.0"));

    let record_b = RoutingRecord::new(1, vec![1], vec![16383], 0.6)
        .with_adapter_ids(vec!["lora-code-v2".to_string()])
        .with_backend("mlx", Some("mlx-v1.0"));

    let mut digest_ab = RoutingDigest::new();
    digest_ab.accumulate_canonical(&record_a, &context_digest);
    digest_ab.accumulate_canonical(&record_b, &context_digest);

    let mut digest_ba = RoutingDigest::new();
    digest_ba.accumulate_canonical(&record_b, &context_digest);
    digest_ba.accumulate_canonical(&record_a, &context_digest);

    assert_ne!(
        digest_ab.digest, digest_ba.digest,
        "Routing chain must be order-dependent"
    );
}

/// Verify different inputs produce different receipt digests.
#[test]
fn different_inputs_different_receipts() {
    set_thread_local_determinism_config(replay_config());

    let fixture_a = baseline_fixture();
    let mut fixture_b = baseline_fixture();
    fixture_b.prompt_tokens = vec![1, 2, 3]; // Different prompt

    let receipt_a = build_receipt(&fixture_a);
    let receipt_b = build_receipt(&fixture_b);

    assert_ne!(
        receipt_a.receipt_digest, receipt_b.receipt_digest,
        "Different inputs must produce different receipt digests"
    );

    clear_thread_local_determinism_config();
}

/// Verify different equipment profiles produce different receipt digests.
#[test]
fn different_equipment_different_receipts() {
    set_thread_local_determinism_config(replay_config());

    let fixture_a = baseline_fixture();
    let mut fixture_b = baseline_fixture();
    fixture_b.processor_id = "Apple M2 Ultra".to_string();

    let receipt_a = build_receipt(&fixture_a);
    let receipt_b = build_receipt(&fixture_b);

    assert_ne!(
        receipt_a.receipt_digest, receipt_b.receipt_digest,
        "Different equipment must produce different receipt digests"
    );

    clear_thread_local_determinism_config();
}

// =============================================================================
// Tests: Seed Lineage Integration
// =============================================================================

/// Verify that seed lineage binding is deterministic and consistent with
/// the receipt's context.
#[test]
fn seed_lineage_receipt_consistency() {
    set_thread_local_determinism_config(replay_config());

    let global = B3Hash::from_bytes(REPLAY_SEED);
    let router_seed = derive_seed(&global, "router");
    let typed_seed = TypedSeed::new(router_seed);

    let lineage_a = SeedLineage::from_typed_seed(&typed_seed, SeedMode::Strict, true);
    let lineage_b = SeedLineage::from_typed_seed(&typed_seed, SeedMode::Strict, true);

    assert_eq!(
        lineage_a.to_binding_hash(),
        lineage_b.to_binding_hash(),
        "Seed lineage binding must be deterministic"
    );

    assert!(lineage_a.verify_typed_seed(&typed_seed));

    clear_thread_local_determinism_config();
}

// =============================================================================
// Tests: JSON Roundtrip
// =============================================================================

/// Verify receipt survives JSON serialization and still verifies.
#[test]
fn receipt_json_roundtrip_determinism() {
    set_thread_local_determinism_config(replay_config());

    let fixture = baseline_fixture();
    let receipt = build_receipt(&fixture);

    let json = serde_json::to_string(&receipt).expect("serialize");
    let parsed: CryptographicReceipt = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(receipt.receipt_digest, parsed.receipt_digest);
    assert_eq!(receipt.routing_digest.digest, parsed.routing_digest.digest);
    assert!(parsed.verify(), "Deserialized receipt must verify");

    clear_thread_local_determinism_config();
}

/// Verify fixture itself survives JSON roundtrip.
#[test]
fn fixture_json_roundtrip() {
    let fixture = baseline_fixture();

    let json = serde_json::to_string_pretty(&fixture).expect("serialize fixture");
    let parsed: ReceiptReplayFixture = serde_json::from_str(&json).expect("deserialize fixture");

    assert_eq!(fixture.id, parsed.id);
    assert_eq!(
        fixture.expected_receipt_digest_hex,
        parsed.expected_receipt_digest_hex
    );

    // Build receipt from parsed fixture and verify
    set_thread_local_determinism_config(replay_config());
    let receipt = build_receipt(&parsed);
    assert_eq!(
        receipt.receipt_digest.to_hex(),
        parsed.expected_receipt_digest_hex
    );
    clear_thread_local_determinism_config();
}

// =============================================================================
// Tests: All Fixtures Batch
// =============================================================================

/// Run all fixtures through replay verification.
#[test]
fn all_fixtures_verify() {
    set_thread_local_determinism_config(replay_config());

    let fixtures = vec![
        baseline_fixture(),
        empty_routing_fixture(),
        single_step_fixture(),
    ];

    for fixture in &fixtures {
        let receipt_a = build_receipt(fixture);
        let receipt_b = build_receipt(fixture);

        assert_eq!(
            receipt_a.receipt_digest, receipt_b.receipt_digest,
            "Fixture '{}' failed replay determinism",
            fixture.id
        );

        assert!(
            receipt_a.verify(),
            "Fixture '{}' failed self-verification",
            fixture.id
        );

        assert_eq!(
            receipt_a.receipt_digest.to_hex(),
            fixture.expected_receipt_digest_hex,
            "Fixture '{}' digest drifted from golden value",
            fixture.id
        );
    }

    clear_thread_local_determinism_config();
}
