//! Router decision comparison for golden run verification

use adapteros_telemetry::events::RouterDecisionEvent;
use serde::{Deserialize, Serialize};

use crate::ComparisonConfig;

/// Divergence in routing decisions between golden and current runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDivergence {
    /// Step number where divergence occurred
    pub step: usize,
    /// Adapter indices selected in golden run
    pub golden_adapters: Vec<u16>,
    /// Adapter indices selected in current run
    pub current_adapters: Vec<u16>,
    /// Entropy in golden run
    pub golden_entropy: f32,
    /// Entropy in current run
    pub current_entropy: f32,
    /// Contextual information about the divergence
    pub context: String,
}

impl RoutingDivergence {
    /// Create a new routing divergence
    pub fn new(
        step: usize,
        golden: &RouterDecisionEvent,
        current: &RouterDecisionEvent,
        reason: &str,
    ) -> Self {
        let golden_adapters: Vec<u16> = golden
            .candidate_adapters
            .iter()
            .filter(|c| c.gate_q15 > 0)
            .map(|c| c.adapter_idx)
            .collect();

        let current_adapters: Vec<u16> = current
            .candidate_adapters
            .iter()
            .filter(|c| c.gate_q15 > 0)
            .map(|c| c.adapter_idx)
            .collect();

        Self {
            step,
            golden_adapters,
            current_adapters,
            golden_entropy: golden.entropy,
            current_entropy: current.entropy,
            context: reason.to_string(),
        }
    }

    /// Format divergence for display
    pub fn format(&self) -> String {
        format!(
            "Step {}: adapters [{:?}] vs [{:?}], entropy {:.4} vs {:.4} ({})",
            self.step,
            self.golden_adapters,
            self.current_adapters,
            self.golden_entropy,
            self.current_entropy,
            self.context
        )
    }
}

/// Compare routing decisions between golden and current runs
///
/// Returns (match_result, divergences)
pub fn compare_routing_decisions(
    golden: &[RouterDecisionEvent],
    current: &[RouterDecisionEvent],
    config: &ComparisonConfig,
) -> (bool, Vec<RoutingDivergence>) {
    let mut divergences = Vec::new();

    // Check length match
    if golden.len() != current.len() {
        // Length mismatch is always a divergence
        return (
            false,
            vec![RoutingDivergence {
                step: golden.len().min(current.len()),
                golden_adapters: Vec::new(),
                current_adapters: Vec::new(),
                golden_entropy: 0.0,
                current_entropy: 0.0,
                context: format!(
                    "Step count mismatch: golden={}, current={}",
                    golden.len(),
                    current.len()
                ),
            }],
        );
    }

    // Compare step-by-step
    for (i, (g, c)) in golden.iter().zip(current.iter()).enumerate() {
        // Check step numbers match
        if g.step != c.step {
            divergences.push(RoutingDivergence::new(
                i,
                g,
                c,
                &format!("Step number mismatch: {} vs {}", g.step, c.step),
            ));
            continue;
        }

        // Extract selected adapter indices (gate_q15 > 0)
        let golden_selected: Vec<u16> = g
            .candidate_adapters
            .iter()
            .filter(|c| c.gate_q15 > 0)
            .map(|c| c.adapter_idx)
            .collect();

        let current_selected: Vec<u16> = c
            .candidate_adapters
            .iter()
            .filter(|c| c.gate_q15 > 0)
            .map(|c| c.adapter_idx)
            .collect();

        // Check adapter set matches (order matters for determinism)
        if golden_selected != current_selected {
            divergences.push(RoutingDivergence::new(
                i,
                g,
                c,
                "Adapter selection mismatch",
            ));
            continue;
        }

        // Check entropy within tolerance (for statistical mode)
        let entropy_tolerance = config.strictness.epsilon_threshold() as f32;
        let entropy_diff = (g.entropy - c.entropy).abs();

        if entropy_diff > entropy_tolerance {
            divergences.push(RoutingDivergence::new(
                i,
                g,
                c,
                &format!("Entropy divergence: Δ={:.6}", entropy_diff),
            ));
            continue;
        }

        // For bitwise strictness, check gate values exactly
        if config.strictness.epsilon_threshold() == 0.0 {
            // Bitwise comparison: Q15 gate values must match exactly
            for (gc, cc) in g.candidate_adapters.iter().zip(c.candidate_adapters.iter()) {
                if gc.adapter_idx != cc.adapter_idx || gc.gate_q15 != cc.gate_q15 {
                    divergences.push(RoutingDivergence::new(
                        i,
                        g,
                        c,
                        &format!(
                            "Gate value mismatch: adapter {} Q15 {} vs {}",
                            gc.adapter_idx, gc.gate_q15, cc.gate_q15
                        ),
                    ));
                    break;
                }
            }
        }
    }

    let passed = divergences.is_empty();
    (passed, divergences)
}

/// Create a test routing decision (public for use in other test modules)
#[cfg(test)]
pub fn create_test_decision(step: usize, adapters: Vec<(u16, i16)>, entropy: f32) -> RouterDecisionEvent {
    use adapteros_telemetry::events::RouterCandidate;
    
    RouterDecisionEvent {
        step,
        input_token_id: Some(42),
        candidate_adapters: adapters
            .into_iter()
            .map(|(idx, gate)| RouterCandidate {
                adapter_idx: idx,
                raw_score: gate as f32 / 32767.0,
                gate_q15: gate,
            })
            .collect(),
        entropy,
        tau: 0.1,
        entropy_floor: 0.01,
        stack_hash: None,
        stack_id: None,
        stack_version: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StrictnessLevel;

    #[test]
    fn test_routing_comparison_exact_match() {
        let golden = vec![
            create_test_decision(0, vec![(0, 16384), (1, 16383)], 0.5),
            create_test_decision(1, vec![(2, 32767), (3, 0)], 0.3),
        ];

        let current = golden.clone();

        let config = ComparisonConfig {
            strictness: StrictnessLevel::Bitwise,
            verify_toolchain: false,
            verify_adapters: false,
            verify_device: false,
            verify_signature: false,
        };

        let (passed, divs) = compare_routing_decisions(&golden, &current, &config);
        assert!(passed);
        assert_eq!(divs.len(), 0);
    }

    #[test]
    fn test_routing_comparison_adapter_mismatch() {
        let golden = vec![create_test_decision(0, vec![(0, 16384), (1, 16383)], 0.5)];

        let current = vec![create_test_decision(0, vec![(0, 16384), (2, 16383)], 0.5)];

        let config = ComparisonConfig {
            strictness: StrictnessLevel::EpsilonTolerant,
            verify_toolchain: false,
            verify_adapters: false,
            verify_device: false,
            verify_signature: false,
        };

        let (passed, divs) = compare_routing_decisions(&golden, &current, &config);
        assert!(!passed);
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].step, 0);
        assert!(divs[0].context.contains("Adapter selection mismatch"));
    }

    #[test]
    fn test_routing_comparison_length_mismatch() {
        let golden = vec![
            create_test_decision(0, vec![(0, 16384)], 0.5),
            create_test_decision(1, vec![(1, 16384)], 0.5),
        ];

        let current = vec![create_test_decision(0, vec![(0, 16384)], 0.5)];

        let config = ComparisonConfig {
            strictness: StrictnessLevel::EpsilonTolerant,
            verify_toolchain: false,
            verify_adapters: false,
            verify_device: false,
            verify_signature: false,
        };

        let (passed, divs) = compare_routing_decisions(&golden, &current, &config);
        assert!(!passed);
        assert_eq!(divs.len(), 1);
        assert!(divs[0].context.contains("Step count mismatch"));
    }

    #[test]
    fn test_routing_comparison_entropy_tolerance() {
        let golden = vec![create_test_decision(0, vec![(0, 16384), (1, 16383)], 0.5)];

        // Use entropy diff of 5e-7 which is less than 1e-6 threshold
        let current = vec![create_test_decision(0, vec![(0, 16384), (1, 16383)], 0.5000005)];

        // EpsilonTolerant should pass (entropy diff < 1e-6)
        let config_tolerant = ComparisonConfig {
            strictness: StrictnessLevel::EpsilonTolerant,
            verify_toolchain: false,
            verify_adapters: false,
            verify_device: false,
            verify_signature: false,
        };

        let (passed, divs) = compare_routing_decisions(&golden, &current, &config_tolerant);
        assert!(passed, "Should pass with entropy diff < 1e-6, got divs: {:?}", divs);
        assert_eq!(divs.len(), 0);

        // Bitwise should fail (entropy must match exactly - but entropy diff of 1e-6 is within tolerance)
        // Actually, let's make the difference larger to guarantee failure
        let current_large_diff = vec![create_test_decision(0, vec![(0, 16384), (1, 16383)], 0.501)];
        
        let config_bitwise = ComparisonConfig {
            strictness: StrictnessLevel::Bitwise,
            verify_toolchain: false,
            verify_adapters: false,
            verify_device: false,
            verify_signature: false,
        };

        let (passed, divs) = compare_routing_decisions(&golden, &current_large_diff, &config_bitwise);
        assert!(!passed, "Should fail with bitwise and entropy diff of 0.001");
        assert!(divs.len() > 0);
    }
}

