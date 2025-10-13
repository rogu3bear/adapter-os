//! Adapter quality scoring logic

use crate::metrics::AdapterMetrics;

/// Score an adapter for promotion/demotion decisions
pub struct AdapterScorer {
    /// Weight for activation frequency
    pub activation_weight: f32,
    /// Weight for latency (lower is better)
    pub latency_weight: f32,
    /// Weight for quality delta
    pub quality_weight: f32,
}

impl Default for AdapterScorer {
    fn default() -> Self {
        Self {
            activation_weight: 0.5,
            latency_weight: 0.2,
            quality_weight: 0.3,
        }
    }
}

impl AdapterScorer {
    /// Compute composite score for adapter
    /// Higher score = more valuable adapter
    pub fn score(&self, metrics: &AdapterMetrics) -> f32 {
        // Activation component (higher is better)
        let activation_score = metrics.activation_pct / 100.0;

        // Latency component (lower is better, normalize to 0-1)
        // Assume reasonable latency range is 0-1000 microseconds
        let latency_score = if metrics.avg_latency_us > 0.0 {
            1.0 - (metrics.avg_latency_us / 1000.0).min(1.0)
        } else {
            1.0 // No latency data, assume good
        };

        // Quality component (higher is better, already normalized)
        let quality_score = metrics.quality_delta.max(0.0).min(1.0);

        // Weighted sum
        activation_score * self.activation_weight
            + latency_score * self.latency_weight
            + quality_score * self.quality_weight
    }

    /// Check if adapter should be promoted
    pub fn should_promote(&self, metrics: &AdapterMetrics, threshold: f32) -> bool {
        self.score(metrics) >= threshold
    }

    /// Check if adapter should be demoted
    pub fn should_demote(&self, metrics: &AdapterMetrics, threshold: f32) -> bool {
        self.score(metrics) < threshold
    }
}

/// Rank adapters by score
pub fn rank_adapters(metrics: &[AdapterMetrics], scorer: &AdapterScorer) -> Vec<(usize, f32)> {
    let mut ranked: Vec<(usize, f32)> = metrics
        .iter()
        .enumerate()
        .map(|(idx, m)| (idx, scorer.score(m)))
        .collect();

    // Sort by score descending
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scoring() {
        let scorer = AdapterScorer::default();

        let high_activation = AdapterMetrics {
            adapter_id: "test".to_string(),
            activation_count: 80,
            total_tokens: 100,
            activation_pct: 80.0,
            avg_latency_us: 100.0,
            memory_bytes: 1000,
            quality_delta: 0.8,
        };

        let low_activation = AdapterMetrics {
            adapter_id: "test".to_string(),
            activation_count: 5,
            total_tokens: 100,
            activation_pct: 5.0,
            avg_latency_us: 200.0,
            memory_bytes: 1000,
            quality_delta: 0.3,
        };

        let high_score = scorer.score(&high_activation);
        let low_score = scorer.score(&low_activation);

        assert!(high_score > low_score);
        assert!(scorer.should_promote(&high_activation, 0.5));
        assert!(scorer.should_demote(&low_activation, 0.3));
    }

    #[test]
    fn test_ranking() {
        let scorer = AdapterScorer::default();

        let metrics = vec![
            AdapterMetrics {
                adapter_id: "a".to_string(),
                activation_count: 50,
                total_tokens: 100,
                activation_pct: 50.0,
                avg_latency_us: 100.0,
                memory_bytes: 1000,
                quality_delta: 0.5,
            },
            AdapterMetrics {
                adapter_id: "b".to_string(),
                activation_count: 80,
                total_tokens: 100,
                activation_pct: 80.0,
                avg_latency_us: 100.0,
                memory_bytes: 1000,
                quality_delta: 0.8,
            },
            AdapterMetrics {
                adapter_id: "c".to_string(),
                activation_count: 10,
                total_tokens: 100,
                activation_pct: 10.0,
                avg_latency_us: 200.0,
                memory_bytes: 1000,
                quality_delta: 0.2,
            },
        ];

        let ranked = rank_adapters(&metrics, &scorer);

        // Should be ranked: b > a > c
        assert_eq!(ranked[0].0, 1); // b
        assert_eq!(ranked[1].0, 0); // a
        assert_eq!(ranked[2].0, 2); // c
    }
}
