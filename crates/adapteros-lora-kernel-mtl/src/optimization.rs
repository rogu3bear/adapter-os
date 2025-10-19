use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::MploraConfig;

/// Runtime performance telemetry collected from kernels.
#[derive(Debug, Clone, Default)]
pub struct KernelPerformanceMetrics {
    /// Observed median latency per token (microseconds).
    pub latency_us: f32,
    /// Observed peak GPU memory usage (bytes).
    pub memory_bytes: usize,
    /// Average GPU utilisation percentage.
    pub gpu_utilization_pct: f32,
    /// Observed throughput in tokens/second.
    pub throughput_tokens: f32,
    /// Batch size used when collecting metrics.
    pub batch_size: usize,
}

/// Optimization plan returned by the optimizer.
#[derive(Debug, Clone)]
pub struct KernelOptimizationPlan {
    /// Tuned MPLoRA configuration for subsequent runs.
    pub updated_config: MploraConfig,
    /// Recommended batch size that meets deterministic latency bounds.
    pub recommended_batch_size: usize,
    /// Expected latency after applying the plan.
    pub expected_latency_us: f32,
    /// Expected GPU utilisation after applying the plan.
    pub expected_utilization_pct: f32,
}

/// Kernel optimizer that keeps execution deterministic while tuning performance knobs.
#[derive(Debug, Clone)]
pub struct KernelOptimizer {
    latency_target_us: f32,
    utilization_target_pct: f32,
    memory_budget_bytes: usize,
}

impl KernelOptimizer {
    /// Create a new optimizer with production targets.
    pub fn new(
        latency_target_us: f32,
        utilization_target_pct: f32,
        memory_budget_bytes: usize,
    ) -> Self {
        Self {
            latency_target_us,
            utilization_target_pct: utilization_target_pct.clamp(10.0, 100.0),
            memory_budget_bytes,
        }
    }

    /// Optimize the given MPLoRA configuration using live telemetry.
    pub fn optimize(
        &self,
        config: &MploraConfig,
        metrics: &KernelPerformanceMetrics,
    ) -> Result<KernelOptimizationPlan> {
        let updated_config = self.tune_config(config, metrics);
        self.validate_config(&updated_config)?;

        let recommended_batch_size = self.recommend_batch_size(metrics);
        let expected_latency = self.estimate_latency(metrics, &updated_config);
        let expected_utilization = self.estimate_utilization(metrics, &updated_config);

        Ok(KernelOptimizationPlan {
            updated_config,
            recommended_batch_size,
            expected_latency_us: expected_latency,
            expected_utilization_pct: expected_utilization,
        })
    }

    /// Adjust MPLoRA configuration according to heuristics.
    fn tune_config(
        &self,
        config: &MploraConfig,
        metrics: &KernelPerformanceMetrics,
    ) -> MploraConfig {
        let mut updated = config.clone();

        // Adjust compression ratio if we are memory constrained or underutilising the GPU.
        if metrics.memory_bytes > self.memory_budget_bytes {
            updated.compression_ratio = (updated.compression_ratio - 0.05).max(0.5);
        } else if metrics.gpu_utilization_pct < self.utilization_target_pct * 0.75 {
            updated.compression_ratio = (updated.compression_ratio + 0.05).min(0.95);
        }

        // Tighten similarity threshold when latency drifts above the target to prune low value adapters.
        if metrics.latency_us > self.latency_target_us {
            updated.similarity_threshold = (updated.similarity_threshold + 0.02).min(0.95);
            updated.penalty_weight = (updated.penalty_weight + 0.05).min(1.0);
        } else {
            updated.similarity_threshold = (updated.similarity_threshold - 0.01).max(0.5);
            updated.penalty_weight = (updated.penalty_weight * 0.95).max(0.05);
        }

        // Enable shared downsample when throughput is below expectations to better reuse compute.
        if metrics.throughput_tokens < 0.95 * self.utilization_target_pct {
            updated.shared_downsample = true;
        }

        // Expand orthogonal history when GPU has headroom to improve accuracy without impacting determinism.
        if metrics.gpu_utilization_pct < self.utilization_target_pct * 0.6 {
            updated.history_window = (updated.history_window + 2).min(64);
        }

        updated
    }

    /// Recommend a batch size that respects the latency target.
    fn recommend_batch_size(&self, metrics: &KernelPerformanceMetrics) -> usize {
        if metrics.latency_us <= 0.0 || metrics.batch_size == 0 {
            return metrics.batch_size.max(1);
        }

        if metrics.latency_us < self.latency_target_us * 0.7 {
            (metrics.batch_size as f32 * 1.25).round().max(1.0) as usize
        } else if metrics.latency_us > self.latency_target_us * 1.2 {
            (metrics.batch_size as f32 * 0.85).round().max(1.0) as usize
        } else {
            metrics.batch_size
        }
    }

    /// Estimate latency after adjustments using a simple proportional model.
    fn estimate_latency(&self, metrics: &KernelPerformanceMetrics, config: &MploraConfig) -> f32 {
        let mut latency = metrics.latency_us;

        // Compression ratio closer to 1.0 reduces latency slightly because fewer adapters are pruned early.
        let compression_bonus =
            (config.compression_ratio - metrics.compression_ratio_hint()) * 1000.0;
        latency = (latency - compression_bonus).max(50.0);

        // Penalty weight increases reduce latency by prioritising confident adapters.
        latency -= config.penalty_weight * 20.0;
        latency.max(50.0)
    }

    /// Estimate utilisation after adjustments.
    fn estimate_utilization(
        &self,
        metrics: &KernelPerformanceMetrics,
        config: &MploraConfig,
    ) -> f32 {
        let mut utilization = metrics.gpu_utilization_pct;
        if config.shared_downsample {
            utilization += 5.0;
        }
        utilization = utilization.clamp(10.0, 100.0);
        utilization
    }

    /// Ensure updated config remains within deterministic ranges.
    fn validate_config(&self, config: &MploraConfig) -> Result<()> {
        if !(0.4..=1.0).contains(&config.compression_ratio) {
            return Err(AosError::Kernel(
                "Compression ratio outside deterministic bounds".into(),
            ));
        }
        if !(0.0..=1.0).contains(&config.similarity_threshold) {
            return Err(AosError::Kernel(
                "Similarity threshold outside [0,1]".into(),
            ));
        }
        if !(0.0..=1.5).contains(&config.penalty_weight) {
            return Err(AosError::Kernel(
                "Penalty weight outside supported range".into(),
            ));
        }
        if config.history_window == 0 || config.history_window > 128 {
            return Err(AosError::Kernel(
                "History window outside supported range".into(),
            ));
        }
        Ok(())
    }
}

impl KernelPerformanceMetrics {
    /// Provide a deterministic hint for current compression ratio.
    fn compression_ratio_hint(&self) -> f32 {
        // Map throughput to an implied compression ratio so adjustments remain deterministic.
        if self.throughput_tokens <= 0.0 {
            0.8
        } else {
            (0.6 + (self.throughput_tokens / 10_000.0).min(0.4)).clamp(0.6, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimizer_adjusts_for_memory_pressure() {
        let optimizer = KernelOptimizer::new(250.0, 80.0, 1_000_000);
        let config = MploraConfig::default();
        let metrics = KernelPerformanceMetrics {
            latency_us: 220.0,
            memory_bytes: 1_500_000,
            gpu_utilization_pct: 70.0,
            throughput_tokens: 50_000.0,
            batch_size: 8,
        };

        let plan = optimizer.optimize(&config, &metrics).expect("plan");
        assert!(plan.updated_config.compression_ratio <= config.compression_ratio);
        assert!(plan.recommended_batch_size <= metrics.batch_size);
    }

    #[test]
    fn optimizer_enables_shared_downsample_when_throughput_low() {
        let optimizer = KernelOptimizer::new(300.0, 85.0, 2_000_000);
        let mut config = MploraConfig::default();
        config.shared_downsample = false;
        let metrics = KernelPerformanceMetrics {
            latency_us: 180.0,
            memory_bytes: 500_000,
            gpu_utilization_pct: 40.0,
            throughput_tokens: 40.0,
            batch_size: 4,
        };

        let plan = optimizer.optimize(&config, &metrics).expect("plan");
        assert!(plan.updated_config.shared_downsample);
        assert!(plan.expected_utilization_pct >= metrics.gpu_utilization_pct);
    }

    #[test]
    fn optimizer_rejects_invalid_configs() {
        let optimizer = KernelOptimizer::new(250.0, 80.0, 2_000_000);
        let mut config = MploraConfig::default();
        config.history_window = 0;
        let metrics = KernelPerformanceMetrics::default();

        let err = optimizer.optimize(&config, &metrics).unwrap_err();
        assert!(format!("{}", err).contains("History window"));
    }
}
