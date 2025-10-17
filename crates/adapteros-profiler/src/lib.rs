//! Adapter profiler for MPLoRA lifecycle management
//!
//! See [`adapter_profiler::AdapterProfiler`] for the primary entry point.

pub mod adapter_profiler;
pub mod metrics;
pub mod scoring;

pub use adapter_profiler::{
    AdapterPerformanceEntry, AdapterProfiler, PerformanceReport, ProblemAdapter, ProfilingSnapshot,
};
pub use metrics::{AdapterMetrics, MetricsAggregator};
pub use scoring::{rank_adapters, AdapterScorer};
