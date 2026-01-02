//! ANE Accelerator Configuration
//!
//! Configuration for when and how to use Neural Engine acceleration.

use std::collections::HashSet;

/// Operation kinds that can be accelerated on ANE
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AneOpKind {
    /// Layer normalization
    LayerNorm,
    /// RMS normalization
    RMSNorm,
    /// Softmax
    Softmax,
    /// Gate routing (for MoE models)
    GateRouting,
}

/// ANE Accelerator Configuration
#[derive(Debug, Clone)]
pub struct AneConfig {
    /// Minimum batch size to use ANE (default: 32)
    ///
    /// Below this threshold, the GPU→ANE transfer overhead exceeds the benefit.
    pub batch_threshold: usize,

    /// Operations to accelerate on ANE
    ///
    /// By default: LayerNorm, RMSNorm, Softmax
    pub enabled_ops: HashSet<AneOpKind>,

    /// Require deterministic execution (default: true)
    ///
    /// When true, ANE will ONLY use `CpuAndNeuralEngine` compute units,
    /// never GPU fallback which would break determinism.
    pub require_determinism: bool,

    /// Enable ANE acceleration (default: true)
    ///
    /// Set to false to disable ANE and use MLX GPU for all operations.
    pub enabled: bool,

    /// Production mode flag (default: false)
    ///
    /// ANE is typically only used in production where power efficiency matters.
    /// In development/testing, MLX GPU is preferred for faster iteration.
    pub production_mode: bool,
}

impl Default for AneConfig {
    fn default() -> Self {
        let mut enabled_ops = HashSet::new();
        enabled_ops.insert(AneOpKind::LayerNorm);
        enabled_ops.insert(AneOpKind::RMSNorm);
        enabled_ops.insert(AneOpKind::Softmax);

        Self {
            batch_threshold: 32,
            enabled_ops,
            require_determinism: true,
            enabled: true,
            production_mode: false,
        }
    }
}

impl AneConfig {
    /// Create a production configuration
    ///
    /// Enables ANE with production mode, which is the recommended
    /// configuration for deployed systems.
    pub fn production() -> Self {
        Self {
            production_mode: true,
            ..Default::default()
        }
    }

    /// Create a development configuration (ANE disabled)
    ///
    /// Uses MLX GPU for all operations, which is faster for development
    /// and debugging.
    pub fn development() -> Self {
        Self {
            enabled: false,
            production_mode: false,
            ..Default::default()
        }
    }

    /// Check if a specific operation should use ANE
    pub fn should_accelerate(&self, op: AneOpKind, batch_size: usize) -> bool {
        self.enabled
            && self.enabled_ops.contains(&op)
            && batch_size >= self.batch_threshold
            && (self.production_mode || !self.require_determinism)
    }

    /// Builder: set batch threshold
    pub fn with_batch_threshold(mut self, threshold: usize) -> Self {
        self.batch_threshold = threshold;
        self
    }

    /// Builder: enable/disable specific operation
    pub fn with_op(mut self, op: AneOpKind, enabled: bool) -> Self {
        if enabled {
            self.enabled_ops.insert(op);
        } else {
            self.enabled_ops.remove(&op);
        }
        self
    }

    /// Builder: set production mode
    pub fn with_production_mode(mut self, enabled: bool) -> Self {
        self.production_mode = enabled;
        self
    }

    /// Builder: enable/disable ANE entirely
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AneConfig::default();
        assert_eq!(config.batch_threshold, 32);
        assert!(config.enabled);
        assert!(config.require_determinism);
        assert!(!config.production_mode);
        assert!(config.enabled_ops.contains(&AneOpKind::LayerNorm));
        assert!(config.enabled_ops.contains(&AneOpKind::Softmax));
    }

    #[test]
    fn test_production_config() {
        let config = AneConfig::production();
        assert!(config.production_mode);
        assert!(config.enabled);
    }

    #[test]
    fn test_development_config() {
        let config = AneConfig::development();
        assert!(!config.enabled);
        assert!(!config.production_mode);
    }

    #[test]
    fn test_should_accelerate() {
        let config = AneConfig::production();

        // Large enough batch, enabled op
        assert!(config.should_accelerate(AneOpKind::LayerNorm, 64));

        // Too small batch
        assert!(!config.should_accelerate(AneOpKind::LayerNorm, 16));

        // Disabled op (GateRouting not in default set)
        assert!(!config.should_accelerate(AneOpKind::GateRouting, 64));
    }

    #[test]
    fn test_builder_pattern() {
        let config = AneConfig::default()
            .with_batch_threshold(64)
            .with_op(AneOpKind::GateRouting, true)
            .with_production_mode(true);

        assert_eq!(config.batch_threshold, 64);
        assert!(config.enabled_ops.contains(&AneOpKind::GateRouting));
        assert!(config.production_mode);
    }
}
