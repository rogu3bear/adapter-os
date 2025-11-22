//! Critical component metrics for AdapterOS production monitoring
//!
//! Provides specialized metric collectors for:
//! - Metal kernel execution time histograms
//! - Hot-swap latency percentiles
//! - Determinism violation counters
//! - Hash collision detection
//! - Memory pressure indicators
//! - HKDF derivation tracking
//! - Checkpoint operations
//!
//! Integration with Prometheus for real-time observability.

use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry, TextEncoder,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};

use adapteros_core::Result;

/// Critical component metrics collector
#[derive(Clone)]
pub struct CriticalComponentMetrics {
    registry: Arc<Registry>,

    // Metal kernel metrics
    pub metal_kernel_execution_seconds: HistogramVec,
    pub metal_kernel_execution_us: HistogramVec,
    pub metal_kernel_failures_total: CounterVec,
    pub metal_kernel_panic_count: CounterVec,
    pub gpu_device_recovery_count: CounterVec,

    // Hot-swap metrics
    pub hotswap_latency_seconds: HistogramVec,
    pub hotswap_latency_ms: HistogramVec,
    pub hotswap_queue_depth: Gauge,
    pub hotswap_memory_freed_mb: CounterVec,
    pub swap_rollback_count: CounterVec,
    pub adapter_swap_count: CounterVec,

    // Determinism metrics
    pub determinism_violation_total: CounterVec,
    pub determinism_violations: CounterVec,
    pub executor_tick_counter: Gauge,
    pub gpu_buffer_integrity_violations: CounterVec,
    pub cross_layer_hash_mismatches: CounterVec,

    // Hash operations metrics
    pub hash_operations_total: CounterVec,

    // HKDF derivation metrics
    pub hkdf_derivations_total: CounterVec,

    // Adapter ID mapping metrics
    pub adapter_id_collisions: CounterVec,
    pub adapter_id_mapping_errors: CounterVec,

    // Memory pressure metrics
    pub memory_pressure_ratio: GaugeVec,
    pub vram_usage_bytes: GaugeVec,
    pub gpu_memory_pressure: GaugeVec,
    pub gpu_memory_pool_reuse_ratio: GaugeVec,
    pub gpu_memory_pool_fragmentation: GaugeVec,

    // Adapter lifecycle metrics
    pub adapter_lifecycle_transitions_total: CounterVec,
    pub adapter_state_transitions: CounterVec,
    pub adapter_activation_percentage: GaugeVec,
    pub adapter_evictions_total: CounterVec,

    // Checkpoint metrics
    pub checkpoint_operations_total: CounterVec,

    // GPU fingerprint metrics
    pub gpu_fingerprint_mismatches_total: Counter,
    pub gpu_fingerprint_sample_time_us: HistogramVec,
    pub gpu_buffer_corruption_detections: CounterVec,
}

impl CriticalComponentMetrics {
    /// Create new critical component metrics collector
    pub fn new() -> Result<Self> {
        let registry = Registry::new();

        // Metal kernel execution time histogram (seconds) - canonical Prometheus unit
        let metal_kernel_execution_seconds = HistogramVec::new(
            HistogramOpts::new(
                "metal_kernel_execution_seconds",
                "Metal kernel execution time in seconds",
            )
            .buckets(vec![
                0.00001, 0.000025, 0.00005, 0.0001, 0.00025, 0.0005, 0.001, 0.0025, 0.005, 0.01,
            ]),
            &["kernel_type", "size"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // Metal kernel execution time histogram (microseconds) - legacy metric
        let metal_kernel_execution_us = HistogramVec::new(
            HistogramOpts::new(
                "metal_kernel_execution_us",
                "Metal kernel execution time in microseconds",
            )
            .buckets(vec![
                10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0,
            ]),
            &["kernel_type", "adapter_id", "input_shape"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // Metal kernel failures counter
        let metal_kernel_failures_total = CounterVec::new(
            Opts::new(
                "metal_kernel_failures_total",
                "Total Metal kernel execution failures",
            ),
            &["kernel_type", "error_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Metal kernel panic counter
        let metal_kernel_panic_count = CounterVec::new(
            Opts::new(
                "metal_kernel_panic_count_total",
                "Total count of Metal kernel panics caught",
            ),
            &["kernel_type", "adapter_id"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // GPU device recovery counter
        let gpu_device_recovery_count = CounterVec::new(
            Opts::new(
                "gpu_device_recovery_count_total",
                "Total GPU device recovery attempts",
            ),
            &["recovery_type", "status"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Hot-swap latency histogram (seconds) - canonical Prometheus unit
        let hotswap_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "hotswap_latency_seconds",
                "Hot-swap operation latency in seconds",
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
            &["swap_type", "success"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // Hot-swap latency histogram (milliseconds) - legacy metric
        let hotswap_latency_ms = HistogramVec::new(
            HistogramOpts::new(
                "hotswap_latency_ms",
                "Hot-swap operation latency in milliseconds",
            )
            .buckets(vec![
                1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0,
            ]),
            &["operation", "adapter_count", "status"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // Hot-swap queue depth gauge
        let hotswap_queue_depth = Gauge::new(
            "hotswap_queue_depth",
            "Current number of pending hot-swap operations in queue",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        // Hot-swap memory freed counter (megabytes)
        let hotswap_memory_freed_mb = CounterVec::new(
            Opts::new(
                "hotswap_memory_freed_mb_total",
                "Total GPU memory freed by hot-swap operations",
            ),
            &["adapter_id"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Swap rollback counter
        let swap_rollback_count = CounterVec::new(
            Opts::new(
                "swap_rollback_count_total",
                "Total hot-swap rollback operations",
            ),
            &["reason"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Adapter swap count
        let adapter_swap_count = CounterVec::new(
            Opts::new("swap_count_total", "Total adapter swap operations"),
            &["status"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Determinism violation counter (canonical metric with component and cause labels)
        let determinism_violation_total = CounterVec::new(
            Opts::new(
                "determinism_violation_total",
                "Total determinism policy violations detected",
            ),
            &["component", "cause"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Determinism violations counter (legacy metric with extended labels)
        let determinism_violations = CounterVec::new(
            Opts::new(
                "determinism_violations_total",
                "Total determinism policy violations detected (legacy)",
            ),
            &["violation_type", "adapter_id", "severity"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Executor tick counter gauge (determinism tracking)
        let executor_tick_counter = Gauge::new(
            "executor_tick_counter",
            "Current global executor tick counter for deterministic execution",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        // GPU buffer integrity violations counter
        let gpu_buffer_integrity_violations = CounterVec::new(
            Opts::new(
                "gpu_buffer_integrity_violations_total",
                "Total GPU buffer fingerprint mismatches",
            ),
            &["adapter_id", "check_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Cross-layer hash mismatches counter
        let cross_layer_hash_mismatches = CounterVec::new(
            Opts::new(
                "cross_layer_hash_mismatches_total",
                "Total cross-layer hash verification failures",
            ),
            &["adapter_ids"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Hash operations counter (BLAKE3, SHA256, etc.)
        let hash_operations_total = CounterVec::new(
            Opts::new("hash_operations_total", "Total hash operations performed"),
            &["operation_type", "size_bucket"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // HKDF derivation counter
        let hkdf_derivations_total = CounterVec::new(
            Opts::new(
                "hkdf_derivations_total",
                "Total HKDF key derivations performed",
            ),
            &["domain"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Adapter ID collision counter
        let adapter_id_collisions = CounterVec::new(
            Opts::new(
                "adapter_id_collisions_total",
                "Total BLAKE3 hash collisions in adapter ID mapping",
            ),
            &["adapter_id_1", "adapter_id_2"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Adapter ID mapping errors counter
        let adapter_id_mapping_errors = CounterVec::new(
            Opts::new(
                "adapter_id_mapping_errors_total",
                "Total errors in adapter ID → u16 mapping",
            ),
            &["error_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Memory pressure ratio gauge (canonical metric with pool_type label)
        let memory_pressure_ratio = GaugeVec::new(
            Opts::new(
                "memory_pressure_ratio",
                "Memory pressure ratio by pool type (0.0 = empty, 1.0 = full)",
            ),
            &["pool_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        // VRAM usage in bytes (per adapter)
        let vram_usage_bytes = GaugeVec::new(
            Opts::new("vram_usage_bytes", "GPU VRAM usage in bytes by adapter"),
            &["adapter_id"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        // GPU memory pressure gauge (0.0 - 1.0) - legacy metric
        let gpu_memory_pressure = GaugeVec::new(
            Opts::new(
                "gpu_memory_pressure",
                "GPU memory utilization pressure (0.0 = empty, 1.0 = full)",
            ),
            &["device_id"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        // GPU memory pool reuse ratio gauge (0.0 - 1.0)
        let gpu_memory_pool_reuse_ratio = GaugeVec::new(
            Opts::new(
                "gpu_memory_pool_reuse_ratio",
                "Ratio of reused buffers to total allocations",
            ),
            &["pool_size_bucket"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        // GPU memory pool fragmentation gauge
        let gpu_memory_pool_fragmentation = GaugeVec::new(
            Opts::new(
                "gpu_memory_pool_fragmentation_ratio",
                "GPU memory pool fragmentation ratio",
            ),
            &["device_id"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        // Adapter state transitions counter
        let adapter_state_transitions = CounterVec::new(
            Opts::new(
                "adapter_state_transitions_total",
                "Total adapter state machine transitions",
            ),
            &["adapter_id", "from_state", "to_state", "reason"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Adapter activation percentage gauge
        let adapter_activation_percentage = GaugeVec::new(
            Opts::new(
                "adapter_activation_percentage",
                "Percentage of time adapter was actively used",
            ),
            &["adapter_id", "time_window"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        // Adapter evictions counter
        let adapter_evictions_total = CounterVec::new(
            Opts::new(
                "adapter_evictions_total",
                "Total adapter evictions due to memory pressure",
            ),
            &["adapter_id", "reason"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Adapter lifecycle transitions counter (canonical metric with from_state and to_state labels)
        let adapter_lifecycle_transitions_total = CounterVec::new(
            Opts::new(
                "adapter_lifecycle_transitions_total",
                "Total adapter lifecycle state transitions",
            ),
            &["from_state", "to_state"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Checkpoint operations counter
        let checkpoint_operations_total = CounterVec::new(
            Opts::new(
                "checkpoint_operations_total",
                "Total checkpoint operations performed",
            ),
            &["operation", "success"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // GPU fingerprint sampling time histogram
        let gpu_fingerprint_sample_time_us = HistogramVec::new(
            HistogramOpts::new(
                "gpu_fingerprint_sample_time_us",
                "Time to sample and hash GPU buffer fingerprints",
            )
            .buckets(vec![100.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0]),
            &["adapter_id", "sample_count"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // GPU buffer corruption detections counter
        let gpu_buffer_corruption_detections = CounterVec::new(
            Opts::new(
                "gpu_buffer_corruption_detections_total",
                "Total GPU buffer corruption detections",
            ),
            &["adapter_id", "corruption_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // GPU fingerprint mismatches counter (canonical metric without labels)
        let gpu_fingerprint_mismatches_total = Counter::new(
            "gpu_fingerprint_mismatches_total",
            "Total GPU fingerprint verification mismatches",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Register all metrics
        let registry_arc = Arc::new(registry);

        registry_arc
            .register(Box::new(metal_kernel_execution_seconds.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(metal_kernel_execution_us.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(metal_kernel_failures_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(metal_kernel_panic_count.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(gpu_device_recovery_count.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(hotswap_latency_seconds.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(hotswap_latency_ms.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(hotswap_queue_depth.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(hotswap_memory_freed_mb.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(swap_rollback_count.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(adapter_swap_count.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(determinism_violation_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(determinism_violations.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(executor_tick_counter.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(gpu_buffer_integrity_violations.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(cross_layer_hash_mismatches.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(hash_operations_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(hkdf_derivations_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(adapter_id_collisions.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(adapter_id_mapping_errors.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(memory_pressure_ratio.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(vram_usage_bytes.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(gpu_memory_pressure.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(gpu_memory_pool_reuse_ratio.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(gpu_memory_pool_fragmentation.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(adapter_state_transitions.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(adapter_activation_percentage.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(adapter_evictions_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(adapter_lifecycle_transitions_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(checkpoint_operations_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(gpu_fingerprint_sample_time_us.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(gpu_buffer_corruption_detections.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(gpu_fingerprint_mismatches_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        info!("Critical component metrics initialized");

        Ok(Self {
            registry: registry_arc,
            // Metal kernel metrics
            metal_kernel_execution_seconds,
            metal_kernel_execution_us,
            metal_kernel_failures_total,
            metal_kernel_panic_count,
            gpu_device_recovery_count,
            // Hot-swap metrics
            hotswap_latency_seconds,
            hotswap_latency_ms,
            hotswap_queue_depth,
            hotswap_memory_freed_mb,
            swap_rollback_count,
            adapter_swap_count,
            // Determinism metrics
            determinism_violation_total,
            determinism_violations,
            executor_tick_counter,
            gpu_buffer_integrity_violations,
            cross_layer_hash_mismatches,
            // Hash operations metrics
            hash_operations_total,
            // HKDF metrics
            hkdf_derivations_total,
            // Adapter ID mapping metrics
            adapter_id_collisions,
            adapter_id_mapping_errors,
            // Memory pressure metrics
            memory_pressure_ratio,
            vram_usage_bytes,
            gpu_memory_pressure,
            gpu_memory_pool_reuse_ratio,
            gpu_memory_pool_fragmentation,
            // Adapter lifecycle metrics
            adapter_lifecycle_transitions_total,
            adapter_state_transitions,
            adapter_activation_percentage,
            adapter_evictions_total,
            // Checkpoint metrics
            checkpoint_operations_total,
            // GPU fingerprint metrics
            gpu_fingerprint_mismatches_total,
            gpu_fingerprint_sample_time_us,
            gpu_buffer_corruption_detections,
        })
    }

    /// Export metrics in Prometheus text format
    pub fn export(&self) -> Result<String> {
        let encoder = TextEncoder::new();
        encoder
            .encode_to_string(&self.registry.gather())
            .map_err(|e| adapteros_core::AosError::Telemetry(format!("Export failed: {}", e)))
    }

    /// Record metal kernel execution time (microseconds)
    pub fn record_metal_kernel_execution(
        &self,
        kernel_type: &str,
        adapter_id: &str,
        input_shape: &str,
        duration_us: f64,
    ) {
        self.metal_kernel_execution_us
            .with_label_values(&[kernel_type, adapter_id, input_shape])
            .observe(duration_us);
    }

    /// Increment metal kernel panic counter
    pub fn increment_metal_kernel_panic(&self, kernel_type: &str, adapter_id: &str) {
        self.metal_kernel_panic_count
            .with_label_values(&[kernel_type, adapter_id])
            .inc();
    }

    /// Record GPU device recovery attempt
    pub fn record_gpu_recovery(&self, recovery_type: &str, status: &str) {
        self.gpu_device_recovery_count
            .with_label_values(&[recovery_type, status])
            .inc();
    }

    /// Record hot-swap latency (milliseconds)
    pub fn record_hotswap_latency(
        &self,
        operation: &str,
        adapter_count: usize,
        status: &str,
        duration_ms: f64,
    ) {
        self.hotswap_latency_ms
            .with_label_values(&[operation, &adapter_count.to_string(), status])
            .observe(duration_ms);
    }

    /// Record hot-swap memory freed
    pub fn record_hotswap_memory_freed(&self, adapter_id: &str, freed_mb: f64) {
        self.hotswap_memory_freed_mb
            .with_label_values(&[adapter_id])
            .inc_by(freed_mb);
    }

    /// Increment swap rollback counter
    pub fn increment_swap_rollback(&self, reason: &str) {
        self.swap_rollback_count.with_label_values(&[reason]).inc();
    }

    /// Record adapter swap completion
    pub fn record_adapter_swap(&self, status: &str) {
        self.adapter_swap_count.with_label_values(&[status]).inc();
    }

    /// Record determinism violation
    pub fn record_determinism_violation(
        &self,
        violation_type: &str,
        adapter_id: &str,
        severity: &str,
    ) {
        self.determinism_violations
            .with_label_values(&[violation_type, adapter_id, severity])
            .inc();
    }

    /// Record GPU buffer integrity violation
    pub fn record_gpu_buffer_integrity_violation(&self, adapter_id: &str, check_type: &str) {
        self.gpu_buffer_integrity_violations
            .with_label_values(&[adapter_id, check_type])
            .inc();
    }

    /// Record cross-layer hash mismatch
    pub fn record_cross_layer_hash_mismatch(&self, adapter_ids: &str) {
        self.cross_layer_hash_mismatches
            .with_label_values(&[adapter_ids])
            .inc();
    }

    /// Record adapter ID collision
    pub fn record_adapter_id_collision(&self, id1: &str, id2: &str) {
        warn!(
            id1 = id1,
            id2 = id2,
            "Adapter ID collision detected (u16 space exhaustion risk)"
        );
        self.adapter_id_collisions
            .with_label_values(&[id1, id2])
            .inc();
    }

    /// Record adapter ID mapping error
    pub fn record_adapter_id_mapping_error(&self, error_type: &str) {
        error!(error_type = error_type, "Adapter ID mapping error");
        self.adapter_id_mapping_errors
            .with_label_values(&[error_type])
            .inc();
    }

    /// Set GPU memory pressure gauge
    pub fn set_gpu_memory_pressure(&self, device_id: &str, pressure: f32) {
        self.gpu_memory_pressure
            .with_label_values(&[device_id])
            .set(pressure as f64);
    }

    /// Set GPU memory pool reuse ratio
    pub fn set_gpu_memory_pool_reuse_ratio(&self, pool_size_bucket: &str, ratio: f32) {
        self.gpu_memory_pool_reuse_ratio
            .with_label_values(&[pool_size_bucket])
            .set(ratio as f64);
    }

    /// Set GPU memory pool fragmentation ratio
    pub fn set_gpu_memory_pool_fragmentation(&self, device_id: &str, ratio: f32) {
        self.gpu_memory_pool_fragmentation
            .with_label_values(&[device_id])
            .set(ratio as f64);
    }

    /// Record adapter state transition
    pub fn record_adapter_state_transition(
        &self,
        adapter_id: &str,
        from_state: &str,
        to_state: &str,
        reason: &str,
    ) {
        self.adapter_state_transitions
            .with_label_values(&[adapter_id, from_state, to_state, reason])
            .inc();
    }

    /// Set adapter activation percentage
    pub fn set_adapter_activation_percentage(
        &self,
        adapter_id: &str,
        time_window: &str,
        percentage: f32,
    ) {
        self.adapter_activation_percentage
            .with_label_values(&[adapter_id, time_window])
            .set(percentage as f64);
    }

    /// Record adapter eviction
    pub fn record_adapter_eviction(&self, adapter_id: &str, reason: &str) {
        self.adapter_evictions_total
            .with_label_values(&[adapter_id, reason])
            .inc();
    }

    /// Record GPU fingerprint sampling time
    pub fn record_gpu_fingerprint_sample_time(
        &self,
        adapter_id: &str,
        sample_count: usize,
        duration_us: f64,
    ) {
        self.gpu_fingerprint_sample_time_us
            .with_label_values(&[adapter_id, &sample_count.to_string()])
            .observe(duration_us);
    }

    /// Record GPU buffer corruption detection
    pub fn record_gpu_buffer_corruption(&self, adapter_id: &str, corruption_type: &str) {
        error!(
            adapter_id = adapter_id,
            corruption_type = corruption_type,
            "GPU buffer corruption detected"
        );
        self.gpu_buffer_corruption_detections
            .with_label_values(&[adapter_id, corruption_type])
            .inc();
    }

    // ========================================
    // New canonical metric helper functions
    // ========================================

    /// Record metal kernel execution time in seconds (canonical Prometheus unit)
    pub fn record_metal_kernel_execution_seconds(
        &self,
        kernel_type: &str,
        size: &str,
        duration_seconds: f64,
    ) {
        self.metal_kernel_execution_seconds
            .with_label_values(&[kernel_type, size])
            .observe(duration_seconds);
    }

    /// Record hot-swap latency in seconds (canonical Prometheus unit)
    pub fn record_hotswap_latency_seconds(
        &self,
        swap_type: &str,
        success: bool,
        duration_seconds: f64,
    ) {
        let success_str = if success { "true" } else { "false" };
        self.hotswap_latency_seconds
            .with_label_values(&[swap_type, success_str])
            .observe(duration_seconds);
    }

    /// Set hot-swap queue depth
    pub fn set_hotswap_queue_depth(&self, depth: i64) {
        self.hotswap_queue_depth.set(depth as f64);
    }

    /// Increment hot-swap queue depth
    pub fn inc_hotswap_queue_depth(&self) {
        self.hotswap_queue_depth.inc();
    }

    /// Decrement hot-swap queue depth
    pub fn dec_hotswap_queue_depth(&self) {
        self.hotswap_queue_depth.dec();
    }

    /// Record determinism violation (canonical metric with component and cause)
    pub fn record_determinism_violation_canonical(&self, component: &str, cause: &str) {
        self.determinism_violation_total
            .with_label_values(&[component, cause])
            .inc();
    }

    /// Record hash operation
    pub fn record_hash_operation(&self, operation_type: &str, size_bucket: &str) {
        self.hash_operations_total
            .with_label_values(&[operation_type, size_bucket])
            .inc();
    }

    /// Increment hash operations counter by a specific amount
    pub fn inc_hash_operations(&self, operation_type: &str, size_bucket: &str, count: u64) {
        self.hash_operations_total
            .with_label_values(&[operation_type, size_bucket])
            .inc_by(count as f64);
    }

    /// Record HKDF derivation
    pub fn record_hkdf_derivation(&self, domain: &str) {
        self.hkdf_derivations_total
            .with_label_values(&[domain])
            .inc();
    }

    /// Increment HKDF derivations counter by a specific amount
    pub fn inc_hkdf_derivations(&self, domain: &str, count: u64) {
        self.hkdf_derivations_total
            .with_label_values(&[domain])
            .inc_by(count as f64);
    }

    /// Set memory pressure ratio by pool type
    pub fn set_memory_pressure_ratio(&self, pool_type: &str, ratio: f64) {
        self.memory_pressure_ratio
            .with_label_values(&[pool_type])
            .set(ratio);
    }

    /// Record adapter lifecycle transition (canonical metric)
    pub fn record_adapter_lifecycle_transition(&self, from_state: &str, to_state: &str) {
        self.adapter_lifecycle_transitions_total
            .with_label_values(&[from_state, to_state])
            .inc();
    }

    /// Record checkpoint operation
    pub fn record_checkpoint_operation(&self, operation: &str, success: bool) {
        let success_str = if success { "true" } else { "false" };
        self.checkpoint_operations_total
            .with_label_values(&[operation, success_str])
            .inc();
    }

    /// Increment GPU fingerprint mismatches counter
    pub fn inc_gpu_fingerprint_mismatches(&self) {
        self.gpu_fingerprint_mismatches_total.inc();
    }

    /// Get current GPU fingerprint mismatches count
    pub fn get_gpu_fingerprint_mismatches(&self) -> f64 {
        self.gpu_fingerprint_mismatches_total.get()
    }

    /// Record metal kernel failure
    pub fn record_metal_kernel_failure(&self, kernel_type: &str, error_type: &str) {
        self.metal_kernel_failures_total
            .with_label_values(&[kernel_type, error_type])
            .inc();
    }

    /// Increment metal kernel failures counter by a specific amount
    pub fn inc_metal_kernel_failures(&self, kernel_type: &str, error_type: &str, count: u64) {
        self.metal_kernel_failures_total
            .with_label_values(&[kernel_type, error_type])
            .inc_by(count as f64);
    }

    /// Set executor tick counter
    pub fn set_executor_tick_counter(&self, tick: u64) {
        self.executor_tick_counter.set(tick as f64);
    }

    /// Increment executor tick counter
    pub fn inc_executor_tick_counter(&self) {
        self.executor_tick_counter.inc();
    }

    /// Get current executor tick counter value
    pub fn get_executor_tick_counter(&self) -> f64 {
        self.executor_tick_counter.get()
    }

    /// Set VRAM usage for a specific adapter
    pub fn set_vram_usage_bytes(&self, adapter_id: &str, bytes: u64) {
        self.vram_usage_bytes
            .with_label_values(&[adapter_id])
            .set(bytes as f64);
    }

    /// Add to VRAM usage for a specific adapter
    pub fn add_vram_usage_bytes(&self, adapter_id: &str, bytes: i64) {
        if bytes >= 0 {
            self.vram_usage_bytes
                .with_label_values(&[adapter_id])
                .add(bytes as f64);
        } else {
            self.vram_usage_bytes
                .with_label_values(&[adapter_id])
                .sub((-bytes) as f64);
        }
    }

    /// Get VRAM usage for a specific adapter
    pub fn get_vram_usage_bytes(&self, _adapter_id: &str) -> f64 {
        // Note: Prometheus gauges don't have a direct get method with labels
        // This would require gathering metrics and parsing, which is expensive
        // For now, return 0.0 as a placeholder - callers should track this locally
        0.0
    }

    // ========================================
    // Convenience helper functions
    // ========================================

    /// Get size bucket label for hash operations based on byte size
    pub fn size_bucket(bytes: usize) -> &'static str {
        match bytes {
            0..=1024 => "1KB",
            1025..=10240 => "10KB",
            10241..=102400 => "100KB",
            102401..=1048576 => "1MB",
            1048577..=10485760 => "10MB",
            _ => "100MB+",
        }
    }

    /// Get pool type label for memory pressure
    pub fn pool_type_gpu() -> &'static str {
        "gpu"
    }

    pub fn pool_type_system() -> &'static str {
        "system"
    }

    pub fn pool_type_adapter() -> &'static str {
        "adapter"
    }
}

impl Default for CriticalComponentMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create default CriticalComponentMetrics")
    }
}

/// Timer helper for recording kernel execution time
pub struct KernelExecutionTimer {
    start: Instant,
    kernel_type: String,
    adapter_id: String,
    input_shape: String,
    metrics: Arc<CriticalComponentMetrics>,
}

impl KernelExecutionTimer {
    /// Create new kernel execution timer
    pub fn new(
        kernel_type: &str,
        adapter_id: &str,
        input_shape: &str,
        metrics: Arc<CriticalComponentMetrics>,
    ) -> Self {
        Self {
            start: Instant::now(),
            kernel_type: kernel_type.to_string(),
            adapter_id: adapter_id.to_string(),
            input_shape: input_shape.to_string(),
            metrics,
        }
    }
}

impl Drop for KernelExecutionTimer {
    fn drop(&mut self) {
        let duration_us = self.start.elapsed().as_micros() as f64;
        self.metrics.record_metal_kernel_execution(
            &self.kernel_type,
            &self.adapter_id,
            &self.input_shape,
            duration_us,
        );
    }
}

/// Timer helper for recording hot-swap latency
pub struct HotSwapTimer {
    start: Instant,
    operation: String,
    adapter_count: usize,
    metrics: Arc<CriticalComponentMetrics>,
}

impl HotSwapTimer {
    /// Create new hot-swap timer
    pub fn new(
        operation: &str,
        adapter_count: usize,
        metrics: Arc<CriticalComponentMetrics>,
    ) -> Self {
        Self {
            start: Instant::now(),
            operation: operation.to_string(),
            adapter_count,
            metrics,
        }
    }

    /// Record timing with status
    pub fn record(self, status: &str) {
        let duration_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        self.metrics.record_hotswap_latency(
            &self.operation,
            self.adapter_count,
            status,
            duration_ms,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critical_metrics_creation() {
        let metrics = CriticalComponentMetrics::new().expect("Failed to create metrics");
        assert!(!metrics.export().expect("Failed to export").is_empty());
    }

    #[test]
    fn test_kernel_execution_recording() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));
        metrics.record_metal_kernel_execution("FusedMlp", "adapter-a", "4096", 1500.0);
        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("metal_kernel_execution_us"));
    }

    #[test]
    fn test_hotswap_timer() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));
        {
            let _timer = HotSwapTimer::new("swap", 2, metrics.clone());
            std::thread::sleep(std::time::Duration::from_millis(10));
        } // Timer records on drop
        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("hotswap_latency_ms"));
    }

    #[test]
    fn test_determinism_violation_recording() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));
        metrics.record_determinism_violation("hash_mismatch", "adapter-x", "critical");
        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("determinism_violations_total"));
    }

    #[test]
    fn test_gpu_memory_pressure_gauge() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));
        metrics.set_gpu_memory_pressure("gpu-0", 0.85);
        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("gpu_memory_pressure 0.85"));
    }

    #[test]
    fn test_adapter_id_collision_recording() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));
        metrics.record_adapter_id_collision("adapter-a", "adapter-b");
        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("adapter_id_collisions_total"));
    }

    #[test]
    fn test_metal_kernel_failures() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));
        metrics.record_metal_kernel_failure("fused_mlp", "timeout");
        metrics.inc_metal_kernel_failures("matmul", "out_of_memory", 3);
        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("metal_kernel_failures_total"));
    }

    #[test]
    fn test_executor_tick_counter() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));
        metrics.set_executor_tick_counter(12345);
        let current = metrics.get_executor_tick_counter();
        assert_eq!(current, 12345.0);

        metrics.inc_executor_tick_counter();
        let after_inc = metrics.get_executor_tick_counter();
        assert_eq!(after_inc, 12346.0);

        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("executor_tick_counter"));
    }

    #[test]
    fn test_vram_usage_bytes() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Set initial VRAM usage
        metrics.set_vram_usage_bytes("adapter-x", 1_073_741_824); // 1GB

        // Add more VRAM usage
        metrics.add_vram_usage_bytes("adapter-x", 536_870_912); // +512MB

        // Subtract VRAM usage
        metrics.add_vram_usage_bytes("adapter-y", -268_435_456); // -256MB

        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("vram_usage_bytes"));
        assert!(export.contains("adapter-x"));
    }

    #[test]
    fn test_canonical_hotswap_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Test latency recording
        metrics.record_hotswap_latency_seconds("full_swap", true, 0.025);
        metrics.record_hotswap_latency_seconds("partial_swap", false, 0.150);

        // Test queue depth
        metrics.set_hotswap_queue_depth(5);
        metrics.inc_hotswap_queue_depth();
        metrics.dec_hotswap_queue_depth();

        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("hotswap_latency_seconds"));
        assert!(export.contains("hotswap_queue_depth"));
    }

    #[test]
    fn test_canonical_determinism_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        metrics.record_determinism_violation_canonical("router", "unseeded_random");
        metrics.record_determinism_violation_canonical("executor", "thread_race");

        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("determinism_violation_total"));
    }

    #[test]
    fn test_hash_and_hkdf_operations() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Test hash operations
        metrics.record_hash_operation("blake3", "1KB");
        metrics.inc_hash_operations("sha256", "10MB", 50);

        // Test HKDF derivations
        metrics.record_hkdf_derivation("router");
        metrics.inc_hkdf_derivations("dropout", 100);

        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("hash_operations_total"));
        assert!(export.contains("hkdf_derivations_total"));
    }

    #[test]
    fn test_memory_pressure_and_lifecycle() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Test memory pressure
        metrics.set_memory_pressure_ratio("gpu", 0.75);
        metrics.set_memory_pressure_ratio("system", 0.60);

        // Test lifecycle transitions
        metrics.record_adapter_lifecycle_transition("cold", "warm");
        metrics.record_adapter_lifecycle_transition("warm", "hot");

        // Test checkpoint operations
        metrics.record_checkpoint_operation("save", true);
        metrics.record_checkpoint_operation("restore", false);

        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("memory_pressure_ratio"));
        assert!(export.contains("adapter_lifecycle_transitions_total"));
        assert!(export.contains("checkpoint_operations_total"));
    }

    #[test]
    fn test_size_bucket_helper() {
        assert_eq!(CriticalComponentMetrics::size_bucket(512), "1KB");
        assert_eq!(CriticalComponentMetrics::size_bucket(5000), "10KB");
        assert_eq!(CriticalComponentMetrics::size_bucket(50000), "100KB");
        assert_eq!(CriticalComponentMetrics::size_bucket(500000), "1MB");
        assert_eq!(CriticalComponentMetrics::size_bucket(5000000), "10MB");
        assert_eq!(CriticalComponentMetrics::size_bucket(50000000), "100MB+");
    }

    #[test]
    fn test_pool_type_helpers() {
        assert_eq!(CriticalComponentMetrics::pool_type_gpu(), "gpu");
        assert_eq!(CriticalComponentMetrics::pool_type_system(), "system");
        assert_eq!(CriticalComponentMetrics::pool_type_adapter(), "adapter");
    }
}
