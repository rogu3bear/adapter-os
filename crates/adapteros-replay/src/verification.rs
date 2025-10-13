//! Hash verification and trace comparison

use anyhow::Result;
use thiserror::Error;
use std::path::Path;
use tracing::{info, debug};

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
pub async fn compare_traces(trace_a_path: &Path, trace_b_path: &Path) -> Result<ComparisonResult, VerificationError> {
    info!("Comparing traces: {} vs {}", trace_a_path.display(), trace_b_path.display());

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

    for (i, (event_a, event_b)) in bundle_a.events.iter().zip(bundle_b.events.iter()).enumerate() {
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
    pub fn verify_permissive(expected: &B3Hash, actual: &B3Hash) -> bool {
        // TODO: Implement actual tolerant comparison for floating point values
        // This would involve deserializing payloads and comparing numerical values within an epsilon
        HashVerifier::verify_strict(expected, actual)
    }
}