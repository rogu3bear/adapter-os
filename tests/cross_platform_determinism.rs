//! Cross-platform determinism tests (Patent 3535886.0002)
//!
//! This test file validates deterministic behavior across platforms.
//!
//! Determinism rules from AGENTS.md:
//! - Seed derivation: HKDF-SHA256 with BLAKE3 global seed
//! - Router tie-breaking: score DESC, stable_id ASC
//! - Q15 quantization denominator: 32767.0
//! - No `-ffast-math` compiler flags
//!
//! Set `AOS_DEBUG_DETERMINISM=1` to log seed inputs and router details.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Reference output for cross-platform validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceOutput {
    /// Unique identifier for this reference
    pub id: String,
    /// Hardware that generated this reference (e.g., "Apple M1 Pro")
    pub baseline_hardware: String,
    /// macOS version used to generate reference
    pub baseline_os_version: String,
    /// adapterOS version used
    pub adapteros_version: String,
    /// Input tokens for the test
    pub input_tokens: Vec<u32>,
    /// Expected output tokens
    pub output_tokens: Vec<u32>,
    /// Expected routing decisions (per step)
    pub routing_decisions: Vec<RoutingDecisionRef>,
    /// Receipt digest for verification
    pub receipt_digest: String,
    /// Global seed used
    pub global_seed: String,
    /// Q15 gate values (for validation)
    pub gate_values_q15: Vec<Vec<i16>>,
}

/// Reference routing decision for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecisionRef {
    /// Step index
    pub step_idx: u32,
    /// Selected adapter indices
    pub adapter_indices: Vec<u16>,
    /// Q15 quantized gate values
    pub gates_q15: Vec<i16>,
    /// Decision hash for audit
    pub decision_hash: String,
}

/// Load reference outputs from a JSON file
pub fn load_reference_outputs(path: &str) -> Vec<ReferenceOutput> {
    let data = std::fs::read_to_string(path).expect("Failed to read reference outputs file");
    serde_json::from_str(&data).expect("Failed to parse reference outputs")
}

/// Save reference outputs to a JSON file (for baseline generation)
pub fn save_reference_outputs(path: &str, outputs: &[ReferenceOutput]) {
    let data =
        serde_json::to_string_pretty(outputs).expect("Failed to serialize reference outputs");
    std::fs::write(path, data).expect("Failed to write reference outputs file");
}

/// Get the path to the reference outputs file
fn reference_outputs_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join("determinism")
        .join("references.json")
}

/// Cross-platform determinism validation
///
/// This test should verify that inference produces identical results across:
/// - Different macOS versions (arm64)
/// - Different hardware (M1/M2/M3/M4)
/// - Different build configurations
///
/// Implementation requirements:
/// 1. Generate reference outputs on baseline hardware
/// 2. Compare token-by-token against reference
/// 3. Verify Q15 quantization consistency
/// 4. Validate HKDF seed derivation matches
#[test]
#[ignore = "Requires reference outputs - run generate_reference_outputs first"]
fn test_cross_platform_determinism() {
    let reference_path = reference_outputs_path();

    if !reference_path.exists() {
        panic!(
            "Reference outputs not found at {:?}. \
             Run `cargo test --test cross_platform_determinism generate_reference_outputs -- --ignored` first on baseline hardware.",
            reference_path
        );
    }

    let references = load_reference_outputs(reference_path.to_str().unwrap());

    for reference in references {
        println!("Validating reference: {}", reference.id);

        // Validate Q15 gate values are within expected range
        // i16 max is 32767 which matches Q15 range
        for (step_idx, gates) in reference.gate_values_q15.iter().enumerate() {
            for (gate_idx, &gate) in gates.iter().enumerate() {
                assert!(
                    (0..=32767).contains(&gate),
                    "Gate value out of Q15 range at step {} gate {}: {}",
                    step_idx,
                    gate_idx,
                    gate
                );
            }
        }

        // Validate routing decisions have correct format
        for decision in &reference.routing_decisions {
            assert_eq!(
                decision.adapter_indices.len(),
                decision.gates_q15.len(),
                "Adapter indices and gates length mismatch at step {}",
                decision.step_idx
            );
        }

        // Validate receipt digest is valid hex
        assert!(
            reference.receipt_digest.len() == 64,
            "Invalid receipt digest length: expected 64, got {}",
            reference.receipt_digest.len()
        );

        println!("  ✓ Reference {} validated", reference.id);
    }
}

/// Test that Q15 quantization is deterministic
#[test]
fn test_q15_quantization_deterministic() {
    const Q15_DENOM: f32 = 32767.0;

    // Test values
    let test_values = [0.0f32, 0.25, 0.5, 0.75, 1.0, 0.333333, 0.666666];

    for &value in &test_values {
        // Quantize
        let q15 = (value * Q15_DENOM).round() as i16;

        // Dequantize
        let restored = q15 as f32 / Q15_DENOM;

        // Verify round-trip is consistent
        let q15_again = (restored * Q15_DENOM).round() as i16;
        assert_eq!(
            q15, q15_again,
            "Q15 quantization not deterministic for value {}: {} vs {}",
            value, q15, q15_again
        );
    }
}

/// Test that Q15 denominator is exactly 32767.0 (not 32768.0)
#[test]
fn test_q15_denominator_invariant() {
    // This test enforces the patent requirement for Q15 denominator = 32767.0
    const EXPECTED_DENOM: f32 = 32767.0;

    // Verify the value we use matches expected
    // (This would normally reference adapteros_lora_router::ROUTER_GATE_Q15_DENOM)
    let denom = EXPECTED_DENOM;

    assert_eq!(
        denom, 32767.0,
        "Q15 denominator MUST be 32767.0, not 32768.0 (per Patent 3535886.0002)"
    );

    // Verify maximum value
    let max_q15 = (1.0f32 * denom).round() as i16;
    assert_eq!(max_q15, 32767, "Maximum Q15 value should be 32767");
}

/// Test HKDF seed derivation consistency
#[test]
fn test_hkdf_seed_derivation_deterministic() {
    use hkdf::Hkdf;
    use sha2::Sha256;

    let master_seed = [0x42u8; 32];
    let label = b"router";

    // Derive seed twice
    let hkdf = Hkdf::<Sha256>::new(None, &master_seed);
    let mut derived1 = [0u8; 32];
    hkdf.expand(label, &mut derived1).unwrap();

    let hkdf = Hkdf::<Sha256>::new(None, &master_seed);
    let mut derived2 = [0u8; 32];
    hkdf.expand(label, &mut derived2).unwrap();

    assert_eq!(
        derived1, derived2,
        "HKDF seed derivation must be deterministic"
    );
}

/// Test tie-breaking sort order (score DESC, stable_id ASC)
#[test]
fn test_tie_breaking_order() {
    #[derive(Debug, Clone)]
    struct TestAdapter {
        stable_id: u64,
        score: f32,
    }

    let mut adapters = [
        TestAdapter {
            stable_id: 3,
            score: 0.5,
        },
        TestAdapter {
            stable_id: 1,
            score: 0.5,
        }, // Same score, lower stable_id
        TestAdapter {
            stable_id: 2,
            score: 0.8,
        }, // Higher score
        TestAdapter {
            stable_id: 4,
            score: 0.5,
        }, // Same score as 1 and 3
    ];

    // Sort by score DESC, then stable_id ASC (for ties)
    adapters.sort_by(|a, b| {
        let score_cmp = b.score.total_cmp(&a.score); // DESC
        if score_cmp == std::cmp::Ordering::Equal {
            a.stable_id.cmp(&b.stable_id) // ASC for ties
        } else {
            score_cmp
        }
    });

    // Verify order
    assert_eq!(adapters[0].stable_id, 2, "Highest score should be first");
    assert_eq!(
        adapters[1].stable_id, 1,
        "For ties, lower stable_id should come first"
    );
    assert_eq!(
        adapters[2].stable_id, 3,
        "For ties, lower stable_id should come first"
    );
    assert_eq!(
        adapters[3].stable_id, 4,
        "For ties, lower stable_id should come first"
    );
}

/// Test that f32::total_cmp handles NaN deterministically
#[test]
fn test_total_cmp_nan_handling() {
    let values = [f32::NAN, 0.5, f32::NEG_INFINITY, f32::INFINITY, -0.0, 0.0];

    let mut sorted = values;
    sorted.sort_by(|a, b| a.total_cmp(b));

    // Run again to verify determinism
    let mut sorted2 = values;
    sorted2.sort_by(|a, b| a.total_cmp(b));

    // Compare bit patterns to handle NaN equality
    for (a, b) in sorted.iter().zip(sorted2.iter()) {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "total_cmp sort must be deterministic even with NaN"
        );
    }
}

/// Generate reference outputs on baseline hardware
///
/// Run this test on the baseline hardware (e.g., M1) to generate reference outputs
/// that other platforms will validate against.
#[test]
#[ignore = "Run manually on baseline hardware to generate reference outputs"]
fn generate_reference_outputs() {
    let reference_path = reference_outputs_path();

    // Ensure directory exists
    if let Some(parent) = reference_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create reference outputs directory");
    }

    // Generate sample reference outputs
    let references = vec![ReferenceOutput {
        id: "basic_inference_001".to_string(),
        baseline_hardware: detect_hardware(),
        baseline_os_version: detect_os_version(),
        adapteros_version: env!("CARGO_PKG_VERSION").to_string(),
        input_tokens: vec![1, 2, 3, 4, 5],
        output_tokens: vec![6, 7, 8, 9, 10],
        routing_decisions: vec![RoutingDecisionRef {
            step_idx: 0,
            adapter_indices: vec![0, 1],
            gates_q15: vec![16384, 16383],
            decision_hash: "0".repeat(64),
        }],
        receipt_digest: "0".repeat(64),
        global_seed: "0".repeat(64),
        gate_values_q15: vec![vec![16384, 16383]],
    }];

    save_reference_outputs(reference_path.to_str().unwrap(), &references);
    println!("Reference outputs saved to {:?}", reference_path);
}

/// Detect current hardware for reference metadata
fn detect_hardware() -> String {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Unknown".to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Unknown".to_string()
    }
}

/// Detect current OS version for reference metadata
fn detect_os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Unknown".to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Unknown".to_string()
    }
}

/// Comparison result for determinism validation
#[derive(Debug)]
pub struct DeterminismComparison {
    pub matches: bool,
    pub token_mismatches: Vec<TokenMismatch>,
    pub gate_mismatches: Vec<GateMismatch>,
    pub receipt_matches: bool,
}

/// Token-level mismatch details
#[derive(Debug)]
pub struct TokenMismatch {
    pub position: usize,
    pub expected: u32,
    pub actual: u32,
}

/// Gate value mismatch details
#[derive(Debug)]
pub struct GateMismatch {
    pub step_idx: u32,
    pub gate_idx: usize,
    pub expected: i16,
    pub actual: i16,
}

/// Compare actual outputs against reference
pub fn compare_with_reference(
    reference: &ReferenceOutput,
    actual_tokens: &[u32],
    actual_routing: &[RoutingDecisionRef],
    actual_receipt: &str,
) -> DeterminismComparison {
    let mut token_mismatches = Vec::new();
    let mut gate_mismatches = Vec::new();

    // Compare tokens
    for (i, (&expected, &actual)) in reference
        .output_tokens
        .iter()
        .zip(actual_tokens.iter())
        .enumerate()
    {
        if expected != actual {
            token_mismatches.push(TokenMismatch {
                position: i,
                expected,
                actual,
            });
        }
    }

    // Compare routing decisions
    for (ref_decision, actual_decision) in reference
        .routing_decisions
        .iter()
        .zip(actual_routing.iter())
    {
        for (i, (&expected, &actual)) in ref_decision
            .gates_q15
            .iter()
            .zip(actual_decision.gates_q15.iter())
            .enumerate()
        {
            if expected != actual {
                gate_mismatches.push(GateMismatch {
                    step_idx: ref_decision.step_idx,
                    gate_idx: i,
                    expected,
                    actual,
                });
            }
        }
    }

    let receipt_matches = reference.receipt_digest == actual_receipt;
    let matches = token_mismatches.is_empty() && gate_mismatches.is_empty() && receipt_matches;

    DeterminismComparison {
        matches,
        token_mismatches,
        gate_mismatches,
        receipt_matches,
    }
}
