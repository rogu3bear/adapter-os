//! Replay system for determinism verification

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayBundle {
    pub cpid: String,
    pub plan_id: String,
    pub seed_global: B3Hash,
    pub events: Vec<ReplayEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEvent {
    pub event_type: String,
    pub timestamp: u128,
    pub event_hash: B3Hash,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ReplayDivergence {
    pub token_idx: usize,
    pub expected_hash: B3Hash,
    pub actual_hash: B3Hash,
    pub context: String,
}

/// Load a replay bundle from NDJSON file
pub fn load_replay_bundle<P: AsRef<Path>>(path: P) -> Result<ReplayBundle> {
    let file = File::open(path.as_ref())
        .map_err(|e| AosError::Telemetry(format!("Failed to open replay bundle: {}", e)))?;

    let reader = BufReader::new(file);
    let mut events = Vec::new();

    // Parse NDJSON (one JSON object per line)
    for (line_no, line) in reader.lines().enumerate() {
        let line = line
            .map_err(|e| AosError::Telemetry(format!("Failed to read line {}: {}", line_no, e)))?;

        if line.trim().is_empty() {
            continue;
        }

        let event: ReplayEvent = serde_json::from_str(&line).map_err(|e| {
            AosError::Telemetry(format!("Failed to parse event at line {}: {}", line_no, e))
        })?;

        events.push(event);
    }

    if events.is_empty() {
        return Err(AosError::Telemetry("No events found in bundle".to_string()));
    }

    // Extract metadata from first event (assumed to be bundle metadata)
    let metadata = &events[0];
    let cpid = metadata
        .payload
        .get("cpid")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let plan_id = metadata
        .payload
        .get("plan_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let seed_global = metadata
        .payload
        .get("seed_global")
        .and_then(|v| v.as_str())
        .and_then(|s| B3Hash::from_hex(s).ok())
        .unwrap_or_else(|| B3Hash::hash(b"default"));

    Ok(ReplayBundle {
        cpid,
        plan_id,
        seed_global,
        events,
    })
}

/// Compare two event sequences and find first divergence
pub fn find_divergence(
    expected: &[ReplayEvent],
    actual: &[ReplayEvent],
) -> Option<ReplayDivergence> {
    let min_len = expected.len().min(actual.len());

    for i in 0..min_len {
        if expected[i].event_hash != actual[i].event_hash {
            return Some(ReplayDivergence {
                token_idx: i,
                expected_hash: expected[i].event_hash.clone(),
                actual_hash: actual[i].event_hash.clone(),
                context: format!(
                    "Expected: {:?}, Actual: {:?}",
                    expected[i].event_type, actual[i].event_type
                ),
            });
        }
    }

    // Check for length mismatch
    if expected.len() != actual.len() {
        return Some(ReplayDivergence {
            token_idx: min_len,
            expected_hash: B3Hash::hash(b"end"),
            actual_hash: B3Hash::hash(b"mismatch"),
            context: format!(
                "Length mismatch: expected {} events, got {}",
                expected.len(),
                actual.len()
            ),
        });
    }

    None
}

/// Format divergence for display
pub fn format_divergence(div: &ReplayDivergence, verbose: bool) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "\n🔴 Divergence detected at token {}\n",
        div.token_idx
    ));
    output.push_str(&format!("   Expected: {}\n", div.expected_hash));
    output.push_str(&format!("   Actual:   {}\n", div.actual_hash));

    if verbose {
        output.push_str(&format!("\n   Context: {}\n", div.context));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_divergence() {
        let event1 = ReplayEvent {
            event_type: "token".to_string(),
            timestamp: 100,
            event_hash: B3Hash::hash(b"a"),
            payload: serde_json::json!({}),
        };

        let event2 = ReplayEvent {
            event_type: "token".to_string(),
            timestamp: 200,
            event_hash: B3Hash::hash(b"b"),
            payload: serde_json::json!({}),
        };

        let event2_diff = ReplayEvent {
            event_type: "token".to_string(),
            timestamp: 200,
            event_hash: B3Hash::hash(b"c"), // Different hash
            payload: serde_json::json!({}),
        };

        // No divergence
        let expected = vec![event1.clone(), event2.clone()];
        let actual = vec![event1.clone(), event2.clone()];
        assert!(find_divergence(&expected, &actual).is_none());

        // Divergence at index 1
        let expected = vec![event1.clone(), event2.clone()];
        let actual = vec![event1.clone(), event2_diff.clone()];
        let div = find_divergence(&expected, &actual);
        assert!(div.is_some());
        assert_eq!(div.expect("Should find divergence").token_idx, 1);
    }
}
