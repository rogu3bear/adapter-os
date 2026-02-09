//! Adapter profiler for DIR lifecycle management
//!
//! Compatibility wrapper around `adapteros-telemetry`'s `profiler` module.

pub use adapteros_telemetry::profiler::*;

// Preserve legacy export name.
pub use adapteros_telemetry::profiler::metrics::AdapterMetrics;
