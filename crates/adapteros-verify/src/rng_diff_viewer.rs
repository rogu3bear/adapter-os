//! RNG state diff viewer for determinism auditing

use serde::{Deserialize, Serialize};

/// Comparison result for two RNG states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RngStateDiff {
    /// True if states are identical
    pub identical: bool,
    /// Seed comparison
    pub seed_match: bool,
    /// Label comparison
    pub label_match: bool,
    /// Step count difference
    pub step_count_diff: i64,
    /// Nonce difference
    pub nonce_diff: i64,
    /// Detailed description
    pub description: String,
}

/// RNG state for comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RngState {
    pub seed: [u8; 32],
    pub label: String,
    pub step_count: u64,
    pub nonce: u64,
}

/// Compare two RNG states and generate detailed diff
pub fn compare_rng_states(a: &RngState, b: &RngState) -> RngStateDiff {
    let seed_match = a.seed == b.seed;
    let label_match = a.label == b.label;
    let step_count_diff = (b.step_count as i64) - (a.step_count as i64);
    let nonce_diff = (b.nonce as i64) - (a.nonce as i64);

    let identical = seed_match && label_match && step_count_diff == 0 && nonce_diff == 0;

    let mut desc = String::new();
    if !seed_match {
        desc.push_str(&format!(
            "Seed mismatch: {} vs {}\n",
            hex::encode(&a.seed[..8]),
            hex::encode(&b.seed[..8])
        ));
    }
    if !label_match {
        desc.push_str(&format!("Label mismatch: {} vs {}\n", a.label, b.label));
    }
    if step_count_diff != 0 {
        desc.push_str(&format!("Step count diff: {}\n", step_count_diff));
    }
    if nonce_diff != 0 {
        desc.push_str(&format!("Nonce diff: {}\n", nonce_diff));
    }

    if identical {
        desc = "States are identical".to_string();
    }

    RngStateDiff {
        identical,
        seed_match,
        label_match,
        step_count_diff,
        nonce_diff,
        description: desc,
    }
}

/// Format diff for terminal output
pub fn format_diff(diff: &RngStateDiff) -> String {
    let mut output = String::new();

    if diff.identical {
        output.push_str("✅ RNG states are identical\n");
    } else {
        output.push_str("❌ RNG states differ:\n");
        output.push_str(&format!(
            "  Seed match: {}\n",
            if diff.seed_match { "✅" } else { "❌" }
        ));
        output.push_str(&format!(
            "  Label match: {}\n",
            if diff.label_match { "✅" } else { "❌" }
        ));

        if diff.step_count_diff != 0 {
            output.push_str(&format!("  Step count diff: {}\n", diff.step_count_diff));
        }
        if diff.nonce_diff != 0 {
            output.push_str(&format!("  Nonce diff: {}\n", diff.nonce_diff));
        }

        output.push('\n');
        output.push_str(&diff.description);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_states() {
        let state_a = RngState {
            seed: [42u8; 32],
            label: "router".to_string(),
            step_count: 100,
            nonce: 5,
        };
        let state_b = state_a.clone();

        let diff = compare_rng_states(&state_a, &state_b);
        assert!(diff.identical);
        assert!(diff.seed_match);
        assert!(diff.label_match);
        assert_eq!(diff.step_count_diff, 0);
        assert_eq!(diff.nonce_diff, 0);
    }

    #[test]
    fn test_different_seeds() {
        let state_a = RngState {
            seed: [42u8; 32],
            label: "router".to_string(),
            step_count: 100,
            nonce: 5,
        };
        let state_b = RngState {
            seed: [43u8; 32],
            label: "router".to_string(),
            step_count: 100,
            nonce: 5,
        };

        let diff = compare_rng_states(&state_a, &state_b);
        assert!(!diff.identical);
        assert!(!diff.seed_match);
        assert!(diff.label_match);
    }

    #[test]
    fn test_step_count_diff() {
        let state_a = RngState {
            seed: [42u8; 32],
            label: "router".to_string(),
            step_count: 100,
            nonce: 5,
        };
        let state_b = RngState {
            seed: [42u8; 32],
            label: "router".to_string(),
            step_count: 150,
            nonce: 5,
        };

        let diff = compare_rng_states(&state_a, &state_b);
        assert!(!diff.identical);
        assert_eq!(diff.step_count_diff, 50);
    }
}
