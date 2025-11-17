use crate::unified_memory::{BackendStats, MemoryStats};
use std::collections::HashMap;

/// Report describing current memory pressure across backends.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryPressureReport {
    /// Total allocated bytes across the system.
    pub total_allocated: usize,
    /// Global memory limit.
    pub memory_limit: usize,
    /// Global pressure percentage (0-100).
    pub pressure_pct: f32,
    /// Per backend pressure percentages.
    pub backend_pressure: HashMap<String, f32>,
    /// Fragmentation score (0-1) computed from backend variance.
    pub fragmentation: f32,
}

/// Action the optimizer recommends for improving memory health.
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryOptimizationAction {
    /// Rebalance memory from one backend to another.
    Rebalance {
        from: String,
        to: String,
        bytes: usize,
    },
    /// Release cold buffers from a backend.
    EvictCold { backend: String, bytes: usize },
    /// Trigger in place compaction for a backend.
    Compact { backend: String },
    /// Expand a backend pool when pressure is low but fragmentation is high.
    ExpandPool {
        backend: String,
        additional_bytes: usize,
    },
}

/// Optimization plan produced by [`MemoryOptimizer`].
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryOptimizationPlan {
    /// Ordered list of optimization actions.
    pub actions: Vec<MemoryOptimizationAction>,
    /// Target fragmentation score after executing the plan.
    pub target_fragmentation: f32,
    /// Target pressure percentage after executing the plan.
    pub target_pressure_pct: f32,
}

/// Production memory optimizer that balances fragmentation, pressure, and determinism.
#[derive(Debug, Clone)]
pub struct MemoryOptimizer {
    pressure_threshold_pct: f32,
    fragmentation_threshold: f32,
    rebalance_granularity: usize,
}

impl MemoryOptimizer {
    /// Create a new optimizer.
    pub fn new(
        pressure_threshold_pct: f32,
        fragmentation_threshold: f32,
        rebalance_granularity: usize,
    ) -> Self {
        Self {
            pressure_threshold_pct: pressure_threshold_pct.clamp(10.0, 100.0),
            fragmentation_threshold: fragmentation_threshold.clamp(0.0, 1.0),
            rebalance_granularity: rebalance_granularity.max(1 << 20), // minimum 1 MiB granularity
        }
    }

    /// Analyse raw memory stats and produce a pressure report.
    pub fn analyse(&self, stats: &MemoryStats) -> MemoryPressureReport {
        let pressure_pct = if stats.memory_limit == 0 {
            0.0
        } else {
            (stats.total_allocated as f32 / stats.memory_limit as f32 * 100.0).min(100.0)
        };

        let backend_pressure = stats
            .backend_stats
            .iter()
            .map(|(backend, backend_stats)| {
                let pct = Self::backend_pressure_pct(backend_stats);
                (backend.clone(), pct)
            })
            .collect::<HashMap<_, _>>();

        let fragmentation = Self::compute_fragmentation(&backend_pressure);

        MemoryPressureReport {
            total_allocated: stats.total_allocated,
            memory_limit: stats.memory_limit,
            pressure_pct,
            backend_pressure,
            fragmentation,
        }
    }

    /// Generate an optimisation plan using the provided stats.
    pub fn plan(&self, stats: &MemoryStats) -> MemoryOptimizationPlan {
        let report = self.analyse(stats);
        let mut actions = Vec::new();

        // Evict cold buffers when global pressure breaches the threshold.
        if report.pressure_pct >= self.pressure_threshold_pct {
            if let Some((backend, _pressure)) = report
                .backend_pressure
                .iter()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            {
                let evict_bytes = (stats
                    .backend_stats
                    .get(backend)
                    .map(|s| s.allocated)
                    .unwrap_or(0)
                    / 4)
                .max(self.rebalance_granularity);
                actions.push(MemoryOptimizationAction::EvictCold {
                    backend: backend.clone(),
                    bytes: evict_bytes,
                });

                // Attempt to rebalance towards the least utilised backend.
                if let Some((target_backend, _)) = report
                    .backend_pressure
                    .iter()
                    .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                {
                    if target_backend != backend {
                        actions.push(MemoryOptimizationAction::Rebalance {
                            from: backend.clone(),
                            to: target_backend.clone(),
                            bytes: self.rebalance_granularity,
                        });
                    }
                }
            }
        }

        // If fragmentation is high, trigger compaction or pool expansion.
        if report.fragmentation > self.fragmentation_threshold {
            if let Some((backend, _)) = report
                .backend_pressure
                .iter()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            {
                actions.push(MemoryOptimizationAction::Compact {
                    backend: backend.clone(),
                });
            }

            if let Some((backend, stats)) = stats
                .backend_stats
                .iter()
                .min_by(|(_, a), (_, b)| a.available.cmp(&b.available))
            {
                if stats.available < self.rebalance_granularity {
                    actions.push(MemoryOptimizationAction::ExpandPool {
                        backend: backend.clone(),
                        additional_bytes: self.rebalance_granularity,
                    });
                }
            }
        }

        MemoryOptimizationPlan {
            actions,
            target_fragmentation: self.fragmentation_threshold,
            target_pressure_pct: self.pressure_threshold_pct - 5.0,
        }
    }

    fn backend_pressure_pct(stats: &BackendStats) -> f32 {
        if stats.total == 0 {
            return 0.0;
        }
        (stats.allocated as f32 / stats.total as f32 * 100.0).min(100.0)
    }

    fn compute_fragmentation(backend_pressure: &HashMap<String, f32>) -> f32 {
        if backend_pressure.len() <= 1 {
            return 0.0;
        }
        let mean = backend_pressure.values().sum::<f32>() / backend_pressure.len() as f32;
        if mean <= f32::EPSILON {
            return 0.0;
        }
        let variance = backend_pressure
            .values()
            .map(|pressure| {
                let diff = *pressure - mean;
                diff * diff
            })
            .sum::<f32>()
            / backend_pressure.len() as f32;
        (variance.sqrt() / mean).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(
        total_allocated: usize,
        memory_limit: usize,
        backends: &[(&str, BackendStats)],
    ) -> MemoryStats {
        let mut backend_stats = HashMap::new();
        for (name, stats) in backends {
            backend_stats.insert((*name).to_string(), stats.clone());
        }
        MemoryStats {
            total_allocated,
            memory_limit,
            backend_stats,
        }
    }

    #[test]
    fn computes_pressure_and_fragmentation() {
        let stats = stats(
            800,
            1_000,
            &[
                (
                    "gpu",
                    BackendStats {
                        allocated: 600,
                        available: 400,
                        total: 1_000,
                        block_count: 8,
                    },
                ),
                (
                    "cpu",
                    BackendStats {
                        allocated: 200,
                        available: 800,
                        total: 1_000,
                        block_count: 4,
                    },
                ),
            ],
        );
        let optimizer = MemoryOptimizer::new(80.0, 0.2, 1 << 20);
        let report = optimizer.analyse(&stats);
        assert!(report.pressure_pct > 70.0);
        assert!(report.fragmentation >= 0.0);
    }

    #[test]
    fn generates_actions_under_pressure() {
        let stats = stats(
            2_000_000,
            2_500_000,
            &[(
                "gpu",
                BackendStats {
                    allocated: 1_800_000,
                    available: 200_000,
                    total: 2_000_000,
                    block_count: 32,
                },
            )],
        );
        let optimizer = MemoryOptimizer::new(70.0, 0.1, 512 * 1024);
        let plan = optimizer.plan(&stats);
        assert!(!plan.actions.is_empty());
    }

    #[test]
    fn expansion_triggered_when_fragmented() {
        let stats = stats(
            1_200_000,
            2_000_000,
            &[
                (
                    "metal",
                    BackendStats {
                        allocated: 900_000,
                        available: 50_000,
                        total: 950_000,
                        block_count: 48,
                    },
                ),
                (
                    "mlx",
                    BackendStats {
                        allocated: 300_000,
                        available: 300_000,
                        total: 600_000,
                        block_count: 16,
                    },
                ),
            ],
        );
        let optimizer = MemoryOptimizer::new(85.0, 0.05, 256 * 1024);
        let plan = optimizer.plan(&stats);
        assert!(plan
            .actions
            .iter()
            .any(|action| matches!(action, MemoryOptimizationAction::ExpandPool { .. })));
    }
}
