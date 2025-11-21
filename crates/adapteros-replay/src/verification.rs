//! Hash verification and trace comparison

use anyhow::Result;
use std::path::Path;
use thiserror::Error;
use tracing::info;

use adapteros_core::B3Hash;
use adapteros_trace::reader::read_trace_bundle;

#[derive(Error, Debug)]
pub enum VerificationError {
    #[error("Trace error: {0}")]
    TraceError(String),
    #[error("Comparison error: {0}")]
    ComparisonError(String),
    #[error("AosError: {0}")]
    AosError(#[from] adapteros_core::AosError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationMode {
    /// Strict byte-for-byte hash comparison
    Strict,
    /// Allows for minor numerical differences (e.g., floating point epsilon)
    Permissive,
    /// Only verifies that hashes can be computed, no comparison
    HashOnly,
}

#[derive(Debug, Clone)]
pub enum ComparisonResult {
    Identical,
    Divergent { reason: String, step: usize },
}

/// Compares two trace files event by event
pub async fn compare_traces(
    trace_a_path: &Path,
    trace_b_path: &Path,
) -> Result<ComparisonResult, VerificationError> {
    info!(
        "Comparing traces: {} vs {}",
        trace_a_path.display(),
        trace_b_path.display()
    );

    let bundle_a = read_trace_bundle(trace_a_path)?;
    let bundle_b = read_trace_bundle(trace_b_path)?;

    if bundle_a.events.len() != bundle_b.events.len() {
        return Ok(ComparisonResult::Divergent {
            reason: format!(
                "Trace length mismatch: A has {} events, B has {} events",
                bundle_a.events.len(),
                bundle_b.events.len()
            ),
            step: 0,
        });
    }

    for (i, (event_a, event_b)) in bundle_a
        .events
        .iter()
        .zip(bundle_b.events.iter())
        .enumerate()
    {
        if event_a.blake3_hash != event_b.blake3_hash {
            return Ok(ComparisonResult::Divergent {
                reason: format!(
                    "Hash mismatch at event {}. Expected: {}, Actual: {}",
                    i, event_a.blake3_hash, event_b.blake3_hash
                ),
                step: i,
            });
        }
        // Optionally, compare other fields if hash comparison is not enough
        if event_a.event_type != event_b.event_type {
            return Ok(ComparisonResult::Divergent {
                reason: format!(
                    "Event type mismatch at step {}. Expected: {}, Actual: {}",
                    i, event_a.event_type, event_b.event_type
                ),
                step: i,
            });
        }
        if event_a.tick_id != event_b.tick_id {
            return Ok(ComparisonResult::Divergent {
                reason: format!(
                    "Tick ID mismatch at step {}. Expected: {}, Actual: {}",
                    i, event_a.tick_id, event_b.tick_id
                ),
                step: i,
            });
        }
        // Deep comparison of inputs/outputs might be needed for Permissive mode
        if event_a.inputs != event_b.inputs || event_a.outputs != event_b.outputs {
            return Ok(ComparisonResult::Divergent {
                reason: format!(
                    "Input/output mismatch at step {}. This might indicate a subtle difference not caught by hash.",
                    i
                ),
                step: i,
            });
        }
    }

    Ok(ComparisonResult::Identical)
}

// Placeholder for more advanced verifiers if needed
pub struct HashVerifier;
impl HashVerifier {
    pub fn verify_strict(expected: &B3Hash, actual: &B3Hash) -> bool {
        expected == actual
    }
}

pub struct TolerantVerifier;
impl TolerantVerifier {
    /// Compare hashes with tolerance for floating point differences
    ///
    /// When hashes don't match exactly, this attempts to deserialize the payloads
    /// and compare numerical values within an epsilon tolerance.
    pub fn verify_permissive(expected: &B3Hash, actual: &B3Hash) -> bool {
        // First try strict comparison
        if HashVerifier::verify_strict(expected, actual) {
            return true;
        }
        // Hashes differ - in permissive mode we still fail since we can't
        // reconstruct the original data from hashes. The caller should use
        // compare_floating_point_outputs for value-level comparison.
        false
    }

    /// Compare floating point values with epsilon tolerance
    ///
    /// Uses both absolute and relative epsilon for robust comparison.
    pub fn compare_floating_point(expected: f64, actual: f64) -> bool {
        const ABSOLUTE_EPSILON: f64 = 1e-9;
        const RELATIVE_EPSILON: f64 = 1e-6;

        // Handle exact equality (including infinities)
        if expected == actual {
            return true;
        }

        // Handle NaN cases
        if expected.is_nan() || actual.is_nan() {
            return expected.is_nan() && actual.is_nan();
        }

        let diff = (expected - actual).abs();

        // Absolute tolerance for values near zero
        if diff <= ABSOLUTE_EPSILON {
            return true;
        }

        // Relative tolerance for larger values
        let max_val = expected.abs().max(actual.abs());
        diff <= max_val * RELATIVE_EPSILON
    }

    /// Compare f32 values with epsilon tolerance
    pub fn compare_f32(expected: f32, actual: f32) -> bool {
        Self::compare_floating_point(expected as f64, actual as f64)
    }

    /// Compare arrays of floating point values
    pub fn compare_float_arrays(expected: &[f64], actual: &[f64]) -> bool {
        if expected.len() != actual.len() {
            return false;
        }
        expected
            .iter()
            .zip(actual.iter())
            .all(|(e, a)| Self::compare_floating_point(*e, *a))
    }

    /// Compare JSON values with floating point tolerance
    pub fn compare_json_values(
        expected: &serde_json::Value,
        actual: &serde_json::Value,
    ) -> bool {
        match (expected, actual) {
            (serde_json::Value::Number(e), serde_json::Value::Number(a)) => {
                match (e.as_f64(), a.as_f64()) {
                    (Some(ef), Some(af)) => Self::compare_floating_point(ef, af),
                    _ => e == a,
                }
            }
            (serde_json::Value::Array(e), serde_json::Value::Array(a)) => {
                if e.len() != a.len() {
                    return false;
                }
                e.iter()
                    .zip(a.iter())
                    .all(|(ev, av)| Self::compare_json_values(ev, av))
            }
            (serde_json::Value::Object(e), serde_json::Value::Object(a)) => {
                if e.len() != a.len() {
                    return false;
                }
                e.iter().all(|(k, v)| {
                    a.get(k)
                        .map(|av| Self::compare_json_values(v, av))
                        .unwrap_or(false)
                })
            }
            _ => expected == actual,
        }
    }
}

/// Compare two trace events with floating point tolerance
pub fn compare_events_permissive(
    event_a: &adapteros_trace::schema::Event,
    event_b: &adapteros_trace::schema::Event,
) -> bool {
    // Check basic fields
    if event_a.event_type != event_b.event_type || event_a.tick_id != event_b.tick_id {
        return false;
    }

    // Compare inputs with tolerance
    if event_a.inputs.len() != event_b.inputs.len() {
        return false;
    }
    for (key, val_a) in &event_a.inputs {
        match event_b.inputs.get(key) {
            Some(val_b) => {
                if !TolerantVerifier::compare_json_values(val_a, val_b) {
                    return false;
                }
            }
            None => return false,
        }
    }

    // Compare outputs with tolerance
    if event_a.outputs.len() != event_b.outputs.len() {
        return false;
    }
    for (key, val_a) in &event_a.outputs {
        match event_b.outputs.get(key) {
            Some(val_b) => {
                if !TolerantVerifier::compare_json_values(val_a, val_b) {
                    return false;
                }
            }
            None => return false,
        }
    }

    true
}
