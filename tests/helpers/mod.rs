//! Test Helper Modules
//!
//! Common utilities for integration tests

pub mod test_adapter_factory_simple;
pub mod tracing_analyzer;

// Re-export commonly used functions
pub use test_adapter_factory_simple::{
    compute_l2_distance, create_adapter_with_constant_weights, create_minimal_test_adapter,
    create_synthetic_adapter, WeightPattern,
};

pub use tracing_analyzer::{
    ImprovementReport, SpanRecord, TimingMetrics, TracingCapture,
};
