//! Promotion and demotion policies

use adapteros_model_hub::manifest::Policies;
use adapteros_telemetry::profiler::metrics::AdapterMetrics;

/// Policy for adapter lifecycle transitions
pub struct LifecyclePolicy {
    /// Minimum activation percentage to stay loaded
    pub min_activation_pct: f32,
    /// Activation threshold for promotion
    pub promotion_threshold: f32,
    /// Activation threshold for demotion
    pub demotion_threshold: f32,
    /// Minimum quality delta to stay loaded
    pub min_quality_delta: f32,
    /// Minimum memory headroom percentage
    pub min_memory_headroom_pct: u8,
}

impl LifecyclePolicy {
    /// Create policy from manifest
    pub fn from_manifest(policies: &Policies) -> Self {
        Self {
            min_activation_pct: 2.0,  // Default value since adapters field doesn't exist
            promotion_threshold: 4.0, // 2x for promotion
            demotion_threshold: 1.0,  // 0.5x for demotion
            min_quality_delta: 0.5,   // Default value
            min_memory_headroom_pct: policies.memory.min_headroom_pct,
        }
    }

    /// Check if adapter should be promoted
    pub fn should_promote(&self, metrics: &AdapterMetrics) -> bool {
        // Must meet activation threshold
        if metrics.activation_pct < self.promotion_threshold {
            return false;
        }

        // Must meet minimum quality delta
        if metrics.quality_delta < self.min_quality_delta {
            return false;
        }

        true
    }

    /// Check if adapter should be demoted
    pub fn should_demote(&self, metrics: &AdapterMetrics) -> bool {
        // Below minimum activation threshold
        if metrics.activation_pct < self.demotion_threshold {
            return true;
        }

        // Below minimum quality delta
        if metrics.quality_delta < self.min_quality_delta * 0.5 {
            return true;
        }

        false
    }

    /// Check if adapter should be evicted due to low usage
    pub fn should_evict(&self, metrics: &AdapterMetrics) -> bool {
        // Very low activation
        if metrics.activation_pct < self.min_activation_pct {
            return true;
        }

        false
    }
}

/// Eviction order policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionOrder {
    /// Evict ephemeral adapters first
    EphemeralFirst,
    /// Evict cold adapters (LRU)
    ColdLru,
    /// Evict warm adapters (LRU)
    WarmLru,
}

impl EvictionOrder {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ephemeral_ttl" => Some(Self::EphemeralFirst),
            "cold_lru" => Some(Self::ColdLru),
            "warm_lru" => Some(Self::WarmLru),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_promotion_policy() {
        let policy = LifecyclePolicy {
            min_activation_pct: 2.0,
            promotion_threshold: 4.0,
            demotion_threshold: 1.0,
            min_quality_delta: 0.5,
            min_memory_headroom_pct: 15,
        };

        let high_metrics = AdapterMetrics {
            adapter_id: "test".to_string(),
            activation_count: 50,
            total_tokens: 100,
            activation_pct: 50.0,
            avg_latency_us: 100.0,
            latency_p95_us: 0.0,
            latency_p99_us: 0.0,
            memory_bytes: 1000,
            peak_memory_bytes: 1000,
            memory_fragmentation: 0.0,
            gpu_utilization_pct: 0.0,
            gpu_memory_bytes: 0,
            quality_delta: 0.8,
        };

        let low_metrics = AdapterMetrics {
            adapter_id: "test".to_string(),
            activation_count: 1,
            total_tokens: 100,
            activation_pct: 1.0,
            avg_latency_us: 100.0,
            latency_p95_us: 0.0,
            latency_p99_us: 0.0,
            memory_bytes: 1000,
            peak_memory_bytes: 1000,
            memory_fragmentation: 0.0,
            gpu_utilization_pct: 0.0,
            gpu_memory_bytes: 0,
            quality_delta: 0.2,
        };

        assert!(policy.should_promote(&high_metrics));
        assert!(!policy.should_promote(&low_metrics));

        assert!(!policy.should_demote(&high_metrics));
        assert!(policy.should_demote(&low_metrics));

        assert!(policy.should_evict(&low_metrics));
    }
}
