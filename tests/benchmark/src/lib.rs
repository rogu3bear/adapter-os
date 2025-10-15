//! # AdapterOS Performance Benchmarking Suite
//!
//! This crate provides comprehensive performance benchmarks for AdapterOS components,
//! focusing on the unique performance characteristics of a deterministic inference
//! runtime with Metal kernels, multi-tenant isolation, and evidence-grounded responses.
//!
//! ## Benchmark Categories
//!
//! - **Kernel Performance**: Metal kernel operations, matrix multiplications, attention mechanisms
//! - **Memory Management**: Allocation patterns, memory pressure, garbage collection
//! - **Throughput**: Inference throughput, request processing, concurrent operations
//! - **System Metrics**: Determinism overhead, isolation boundaries, evidence processing
//! - **Multi-tenant Isolation**: Resource isolation, security boundaries, performance isolation
//! - **Evidence Processing**: Response grounding, evidence validation, latency characteristics

pub mod kernel_benchmarks;
pub mod memory_benchmarks;
pub mod throughput_benchmarks;
pub mod system_benchmarks;
pub mod isolation_benchmarks;
pub mod evidence_benchmarks;
pub mod utils;

/// Common benchmark configuration and utilities
pub mod config {
    use std::time::Duration;

    /// Default benchmark configuration
    pub struct BenchmarkConfig {
        pub sample_size: usize,
        pub measurement_time: Duration,
        pub warmup_time: Duration,
        pub noise_threshold: f64,
    }

    impl Default for BenchmarkConfig {
        fn default() -> Self {
            Self {
                sample_size: 100,
                measurement_time: Duration::from_secs(5),
                warmup_time: Duration::from_secs(1),
                noise_threshold: 0.05,
            }
        }
    }

    /// Metal-specific benchmark configuration
    #[cfg(feature = "metal")]
    pub struct MetalBenchmarkConfig {
        pub base_config: BenchmarkConfig,
        pub device_index: usize,
        pub enable_profiling: bool,
        pub memory_pool_size: usize,
    }

    #[cfg(feature = "metal")]
    impl Default for MetalBenchmarkConfig {
        fn default() -> Self {
            Self {
                base_config: BenchmarkConfig::default(),
                device_index: 0,
                enable_profiling: true,
                memory_pool_size: 1024 * 1024 * 1024, // 1GB
            }
        }
    }
}

/// Benchmark result aggregation and reporting
pub mod reporting {
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BenchmarkResult {
        pub name: String,
        pub category: String,
        pub mean_time_ns: f64,
        pub std_dev_ns: f64,
        pub throughput: Option<f64>,
        pub memory_usage_mb: Option<f64>,
        pub metadata: HashMap<String, String>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct BenchmarkReport {
        pub timestamp: chrono::DateTime<chrono::Utc>,
        pub system_info: SystemInfo,
        pub results: Vec<BenchmarkResult>,
        pub summary: BenchmarkSummary,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct SystemInfo {
        pub os: String,
        pub cpu: String,
        pub memory_gb: f64,
        pub gpu: Option<String>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct BenchmarkSummary {
        pub total_benchmarks: usize,
        pub total_time_seconds: f64,
        pub regressions: Vec<String>,
        pub improvements: Vec<String>,
    }
}