//! Critical component metrics for adapterOS production monitoring
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

use adapteros_core::singleflight::SingleFlightMetrics;
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
    pub adapter_cache_bytes: Gauge,
    pub adapter_cache_budget_exceeded_total: Counter,

    // Model cache metrics
    pub model_cache_hits_total: Counter,
    pub model_cache_misses_total: Counter,
    pub model_cache_eviction_blocked_pinned_total: Counter,
    pub model_cache_pinned_entries: Gauge,
    pub model_cache_pin_limit_rejections_total: Counter,
    pub model_cache_pinned_memory_bytes: Gauge,
    pub model_cache_pin_limit: Gauge,

    // Residency probe metrics
    pub residency_probe_ok: Gauge,
    pub residency_probe_runs_total: Counter,

    // Checkpoint metrics
    pub checkpoint_operations_total: CounterVec,

    // GPU fingerprint metrics
    pub gpu_fingerprint_mismatches_total: Counter,
    pub gpu_fingerprint_sample_time_us: HistogramVec,
    pub gpu_buffer_corruption_detections: CounterVec,

    // KV cache residency metrics (Phase 8)
    pub kv_hot_entries: Gauge,
    pub kv_cold_entries: Gauge,
    pub kv_hot_bytes: Gauge,
    pub kv_cold_bytes: Gauge,
    pub kv_evictions_by_residency_total: CounterVec,
    pub kv_quota_exceeded_total: Counter,
    pub kv_purgeable_failures_total: Counter,

    // SingleFlight deduplication metrics
    pub singleflight_leader_count: CounterVec,
    pub singleflight_waiter_count: GaugeVec,
    pub singleflight_error_count: CounterVec,
    // Tenant isolation metrics
    pub tenant_isolation_violation_total: CounterVec,
    pub tenant_isolation_access_attempts_total: CounterVec,
    // Database performance metrics
    pub db_query_duration_seconds: HistogramVec,
    pub db_index_scan_total: CounterVec,
    pub db_composite_index_hit_ratio: GaugeVec,
    // Tenant performance metrics
    pub db_tenant_query_duration_seconds: HistogramVec,
    pub db_tenant_query_errors_total: CounterVec,
    // Evidence validation metrics
    pub evidence_validation_success_total: CounterVec,
    pub evidence_validation_failure_total: CounterVec,

    // UDS phase breakdown metrics
    pub uds_connect_latency_seconds: HistogramVec,
    pub uds_write_latency_seconds: HistogramVec,
    pub uds_read_latency_seconds: HistogramVec,

    // Worker inference timing metrics
    /// Time spent waiting in queue before inference starts (seconds)
    pub worker_queue_wait_seconds: HistogramVec,
    /// Time spent in actual token generation (seconds)
    pub worker_generation_seconds: HistogramVec,

    // Server handler latency metrics
    /// Pre-UDS latency: time from request receipt to UDS call start (seconds)
    pub server_handler_latency_seconds: HistogramVec,
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

        // Tenant isolation violation counter
        let tenant_isolation_violation_total = CounterVec::new(
            Opts::new(
                "tenant_isolation_violation_total",
                "Total tenant isolation violations detected",
            ),
            &["violation_type", "resource_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Tenant isolation access attempts counter
        let tenant_isolation_access_attempts_total = CounterVec::new(
            Opts::new(
                "tenant_isolation_access_attempts_total",
                "Total tenant isolation access attempts",
            ),
            &["access_type", "granted"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Database query duration histogram
        let db_query_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "db_query_duration_seconds",
                "Database query execution time in seconds",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
            ]),
            &["tenant_id", "query_type", "table_name"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // Database index scan counter
        let db_index_scan_total = CounterVec::new(
            Opts::new(
                "db_index_scan_total",
                "Total database index scans performed",
            ),
            &["tenant_id", "table_name", "index_name", "scan_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Database composite index hit ratio gauge
        let db_composite_index_hit_ratio = GaugeVec::new(
            Opts::new(
                "db_composite_index_hit_ratio",
                "Database composite index hit ratio (0.0 to 1.0)",
            ),
            &["tenant_id", "table_name", "index_name"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        // Tenant-specific database query duration histogram
        let db_tenant_query_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "db_tenant_query_duration_seconds",
                "Tenant-specific database query execution time in seconds",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
            ]),
            &["tenant_id", "query_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // Tenant-specific database query errors counter
        let db_tenant_query_errors_total = CounterVec::new(
            Opts::new(
                "db_tenant_query_errors_total",
                "Total tenant-specific database query errors",
            ),
            &["tenant_id", "query_type", "error_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Evidence validation success counter
        let evidence_validation_success_total = CounterVec::new(
            Opts::new(
                "evidence_validation_success_total",
                "Total successful evidence validations",
            ),
            &["evidence_type", "validation_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Evidence validation failure counter
        let evidence_validation_failure_total = CounterVec::new(
            Opts::new(
                "evidence_validation_failure_total",
                "Total failed evidence validations",
            ),
            &["evidence_type", "failure_reason"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // UDS phase breakdown histograms
        // Connect/Write: fast operations, sub-millisecond to sub-second buckets
        let uds_connect_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "uds_connect_latency_seconds",
                "UDS socket connect phase latency in seconds",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 0.25, 0.5,
            ]),
            &["worker_id"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        let uds_write_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "uds_write_latency_seconds",
                "UDS socket write phase latency in seconds",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 0.25, 0.5,
            ]),
            &["worker_id"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // Read: longer operations including inference time, extended buckets
        let uds_read_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "uds_read_latency_seconds",
                "UDS socket read phase latency in seconds",
            )
            .buckets(vec![0.001, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
            &["worker_id"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // Worker queue wait time histogram (seconds)
        // Measures time from request arrival to inference start
        // Buckets optimized for queue wait times (sub-millisecond to 1 second)
        let worker_queue_wait_seconds = HistogramVec::new(
            HistogramOpts::new(
                "worker_queue_wait_seconds",
                "Time spent waiting in queue before inference starts (seconds)",
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
            &["worker_id"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // Worker generation time histogram (seconds)
        // Measures actual token generation time (excludes queue wait)
        // Buckets optimized for generation times (10ms to 10+ seconds)
        let worker_generation_seconds = HistogramVec::new(
            HistogramOpts::new(
                "worker_generation_seconds",
                "Time spent in actual token generation (seconds)",
            )
            .buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
            &["worker_id", "model_id"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
        })?;

        // Server handler latency histogram (seconds)
        // Measures pre-UDS latency from request receipt to UDS call start
        let server_handler_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "server_handler_latency_seconds",
                "Pre-UDS latency: time from request receipt to UDS call start (seconds)",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
            ]),
            &["endpoint", "status"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Histogram creation failed: {}", e))
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

        let adapter_cache_bytes =
            Gauge::new("adapter_cache_bytes", "Total bytes used by cached adapters").map_err(
                |e| adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e)),
            )?;

        let adapter_cache_budget_exceeded_total = Counter::new(
            "adapter_cache_budget_exceeded_total",
            "Total adapter cache load failures due to budget limits",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // Model cache metrics
        let model_cache_hits_total =
            Counter::new("model_cache_hits_total", "Total model cache hits").map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
            })?;

        let model_cache_misses_total =
            Counter::new("model_cache_misses_total", "Total model cache misses").map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
            })?;

        let model_cache_eviction_blocked_pinned_total = Counter::new(
            "model_cache_eviction_blocked_pinned_total",
            "Total eviction attempts blocked due to pinned entries",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        let model_cache_pinned_entries = Gauge::new(
            "model_cache_pinned_entries",
            "Current number of pinned cache entries",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        let model_cache_pin_limit_rejections_total = Counter::new(
            "model_cache_pin_limit_rejections_total",
            "Total pin attempts rejected due to pin limit exceeded",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        let model_cache_pinned_memory_bytes = Gauge::new(
            "model_cache_pinned_memory_bytes",
            "Total memory in bytes used by pinned cache entries",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        let model_cache_pin_limit = Gauge::new(
            "model_cache_pin_limit",
            "Configured maximum number of pinned entries allowed",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        // Residency probe metrics
        let residency_probe_ok = Gauge::new(
            "residency_probe_ok",
            "1 if last residency probe passed, 0 otherwise",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        let residency_probe_runs_total =
            Counter::new("residency_probe_runs_total", "Total residency probe runs").map_err(
                |e| adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e)),
            )?;

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

        // KV cache residency metrics (Phase 8)
        let kv_hot_entries = Gauge::new("kv_hot_entries", "Current count of HOT KV cache entries")
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
            })?;

        let kv_cold_entries =
            Gauge::new("kv_cold_entries", "Current count of COLD KV cache entries").map_err(
                |e| adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e)),
            )?;

        let kv_hot_bytes = Gauge::new("kv_hot_bytes", "Total bytes in HOT KV cache entries")
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
            })?;

        let kv_cold_bytes = Gauge::new("kv_cold_bytes", "Total bytes in COLD KV cache entries")
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
            })?;

        let kv_evictions_by_residency_total = CounterVec::new(
            Opts::new(
                "kv_evictions_by_residency_total",
                "Total KV cache evictions by residency state (hot/cold)",
            ),
            &["residency_type"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        let kv_quota_exceeded_total = Counter::new(
            "kv_quota_exceeded_total",
            "Total KV cache quota exceeded events",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        let kv_purgeable_failures_total = Counter::new(
            "kv_purgeable_failures_total",
            "Total KV cache purgeable state failures",
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        // SingleFlight deduplication metrics
        let singleflight_leader_count = CounterVec::new(
            Opts::new(
                "singleflight_leader_count_total",
                "Total SingleFlight leaders (requests that triggered loads)",
            ),
            &["operation"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Counter creation failed: {}", e))
        })?;

        let singleflight_waiter_count = GaugeVec::new(
            Opts::new(
                "singleflight_waiter_count",
                "Current number of SingleFlight waiters by operation",
            ),
            &["operation"],
        )
        .map_err(|e| {
            adapteros_core::AosError::Telemetry(format!("Gauge creation failed: {}", e))
        })?;

        let singleflight_error_count = CounterVec::new(
            Opts::new(
                "singleflight_error_count_total",
                "Total SingleFlight errors by operation and error type",
            ),
            &["operation", "error_type"],
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
            .register(Box::new(adapter_cache_bytes.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;
        registry_arc
            .register(Box::new(adapter_cache_budget_exceeded_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(model_cache_hits_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(model_cache_misses_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(model_cache_eviction_blocked_pinned_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(model_cache_pinned_entries.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(model_cache_pin_limit_rejections_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(model_cache_pinned_memory_bytes.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(model_cache_pin_limit.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(residency_probe_ok.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(residency_probe_runs_total.clone()))
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

        // Register KV cache residency metrics
        registry_arc
            .register(Box::new(kv_hot_entries.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(kv_cold_entries.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(kv_hot_bytes.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(kv_cold_bytes.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(kv_evictions_by_residency_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(kv_quota_exceeded_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(kv_purgeable_failures_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        // Register SingleFlight metrics
        registry_arc
            .register(Box::new(singleflight_leader_count.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(singleflight_waiter_count.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(singleflight_error_count.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        // Register tenant isolation metrics
        registry_arc
            .register(Box::new(tenant_isolation_violation_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(tenant_isolation_access_attempts_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        // Register database performance metrics
        registry_arc
            .register(Box::new(db_query_duration_seconds.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(db_index_scan_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(db_composite_index_hit_ratio.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(db_tenant_query_duration_seconds.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(db_tenant_query_errors_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        // Register evidence validation metrics
        registry_arc
            .register(Box::new(evidence_validation_success_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(evidence_validation_failure_total.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        // Register UDS phase breakdown metrics
        registry_arc
            .register(Box::new(uds_connect_latency_seconds.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(uds_write_latency_seconds.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(uds_read_latency_seconds.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        // Register worker inference timing metrics
        registry_arc
            .register(Box::new(worker_queue_wait_seconds.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(worker_generation_seconds.clone()))
            .map_err(|e| {
                adapteros_core::AosError::Telemetry(format!("Registration failed: {}", e))
            })?;

        registry_arc
            .register(Box::new(server_handler_latency_seconds.clone()))
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
            adapter_cache_bytes,
            adapter_cache_budget_exceeded_total,
            // Checkpoint metrics
            checkpoint_operations_total,
            // GPU fingerprint metrics
            gpu_fingerprint_mismatches_total,
            gpu_fingerprint_sample_time_us,
            gpu_buffer_corruption_detections,
            // Model cache metrics
            model_cache_hits_total,
            model_cache_misses_total,
            model_cache_eviction_blocked_pinned_total,
            model_cache_pinned_entries,
            model_cache_pin_limit_rejections_total,
            model_cache_pinned_memory_bytes,
            model_cache_pin_limit,
            // Residency probe metrics
            residency_probe_ok,
            residency_probe_runs_total,
            // KV cache residency metrics
            kv_hot_entries,
            kv_cold_entries,
            kv_hot_bytes,
            kv_cold_bytes,
            kv_evictions_by_residency_total,
            kv_quota_exceeded_total,
            kv_purgeable_failures_total,
            // SingleFlight metrics
            singleflight_leader_count,
            singleflight_waiter_count,
            singleflight_error_count,
            // Tenant isolation metrics
            tenant_isolation_violation_total,
            tenant_isolation_access_attempts_total,
            // Database performance metrics
            db_query_duration_seconds,
            db_index_scan_total,
            db_composite_index_hit_ratio,
            // Tenant performance metrics
            db_tenant_query_duration_seconds,
            db_tenant_query_errors_total,
            // Evidence validation metrics
            evidence_validation_success_total,
            evidence_validation_failure_total,
            // UDS phase breakdown metrics
            uds_connect_latency_seconds,
            uds_write_latency_seconds,
            uds_read_latency_seconds,
            // Worker inference timing metrics
            worker_queue_wait_seconds,
            worker_generation_seconds,
            // Server handler latency metrics
            server_handler_latency_seconds,
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

    /// Set total adapter cache bytes used
    pub fn set_adapter_cache_bytes(&self, bytes: u64) {
        self.adapter_cache_bytes.set(bytes as f64);
    }

    /// Record adapter cache budget exceeded event
    pub fn record_adapter_cache_budget_exceeded(&self) {
        self.adapter_cache_budget_exceeded_total.inc();
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

    // ========================================
    // Model cache metrics helper functions
    // ========================================

    /// Record a model cache hit
    pub fn record_model_cache_hit(&self) {
        self.model_cache_hits_total.inc();
    }

    /// Record a model cache miss
    pub fn record_model_cache_miss(&self) {
        self.model_cache_misses_total.inc();
    }

    /// Record an eviction blocked due to pinned entry
    pub fn record_eviction_blocked_pinned(&self) {
        self.model_cache_eviction_blocked_pinned_total.inc();
    }

    /// Get current model cache hits count
    pub fn get_model_cache_hits(&self) -> f64 {
        self.model_cache_hits_total.get()
    }

    /// Get current model cache misses count
    pub fn get_model_cache_misses(&self) -> f64 {
        self.model_cache_misses_total.get()
    }

    /// Set the current number of pinned cache entries
    pub fn set_pinned_entries_count(&self, count: usize) {
        self.model_cache_pinned_entries.set(count as f64);
    }

    /// Get current number of pinned cache entries
    pub fn get_pinned_entries_count(&self) -> usize {
        self.model_cache_pinned_entries.get() as usize
    }

    /// Record a pin limit rejection
    pub fn record_pin_limit_rejection(&self) {
        self.model_cache_pin_limit_rejections_total.inc();
    }

    /// Get total pin limit rejections
    pub fn get_pin_limit_rejections(&self) -> f64 {
        self.model_cache_pin_limit_rejections_total.get()
    }

    /// Set the current pinned memory in bytes
    pub fn set_pinned_memory_bytes(&self, bytes: u64) {
        self.model_cache_pinned_memory_bytes.set(bytes as f64);
    }

    /// Get current pinned memory in bytes
    pub fn get_pinned_memory_bytes(&self) -> u64 {
        self.model_cache_pinned_memory_bytes.get() as u64
    }

    /// Set the configured pin limit
    pub fn set_pin_limit(&self, limit: usize) {
        self.model_cache_pin_limit.set(limit as f64);
    }

    /// Get configured pin limit
    pub fn get_pin_limit(&self) -> usize {
        self.model_cache_pin_limit.get() as usize
    }

    // ========================================
    // Residency probe metrics helper functions
    // ========================================

    /// Record residency probe result
    pub fn record_residency_probe(&self, ok: bool) {
        self.residency_probe_runs_total.inc();
        self.residency_probe_ok.set(if ok { 1.0 } else { 0.0 });
    }

    /// Set residency probe ok status directly
    pub fn set_residency_probe_ok(&self, ok: bool) {
        self.residency_probe_ok.set(if ok { 1.0 } else { 0.0 });
    }

    /// Get current residency probe ok status
    pub fn get_residency_probe_ok(&self) -> bool {
        self.residency_probe_ok.get() == 1.0
    }

    /// Get total residency probe runs
    pub fn get_residency_probe_runs(&self) -> f64 {
        self.residency_probe_runs_total.get()
    }

    // ========================================
    // KV cache residency metrics helper functions (Phase 8)
    // ========================================

    /// Set KV cache HOT entries count
    pub fn set_kv_hot_entries(&self, count: usize) {
        self.kv_hot_entries.set(count as f64);
    }

    /// Set KV cache COLD entries count
    pub fn set_kv_cold_entries(&self, count: usize) {
        self.kv_cold_entries.set(count as f64);
    }

    /// Set KV cache HOT bytes
    pub fn set_kv_hot_bytes(&self, bytes: u64) {
        self.kv_hot_bytes.set(bytes as f64);
    }

    /// Set KV cache COLD bytes
    pub fn set_kv_cold_bytes(&self, bytes: u64) {
        self.kv_cold_bytes.set(bytes as f64);
    }

    /// Record KV cache eviction by residency type
    pub fn record_kv_eviction(&self, residency_type: &str) {
        self.kv_evictions_by_residency_total
            .with_label_values(&[residency_type])
            .inc();
    }

    /// Record KV cache evictions by residency type with count
    pub fn record_kv_evictions(&self, residency_type: &str, count: u64) {
        self.kv_evictions_by_residency_total
            .with_label_values(&[residency_type])
            .inc_by(count as f64);
    }

    /// Record KV cache quota exceeded event
    pub fn record_kv_quota_exceeded(&self) {
        self.kv_quota_exceeded_total.inc();
    }

    /// Record KV cache purgeable state failure
    pub fn record_kv_purgeable_failure(&self) {
        self.kv_purgeable_failures_total.inc();
    }

    /// Get current KV HOT entries count
    pub fn get_kv_hot_entries(&self) -> usize {
        self.kv_hot_entries.get() as usize
    }

    /// Get current KV COLD entries count
    pub fn get_kv_cold_entries(&self) -> usize {
        self.kv_cold_entries.get() as usize
    }

    /// Get current KV HOT bytes
    pub fn get_kv_hot_bytes(&self) -> u64 {
        self.kv_hot_bytes.get() as u64
    }

    /// Get current KV COLD bytes
    pub fn get_kv_cold_bytes(&self) -> u64 {
        self.kv_cold_bytes.get() as u64
    }

    /// Get total KV quota exceeded events
    pub fn get_kv_quota_exceeded_total(&self) -> u64 {
        self.kv_quota_exceeded_total.get() as u64
    }

    /// Get total KV purgeable failures
    pub fn get_kv_purgeable_failures_total(&self) -> u64 {
        self.kv_purgeable_failures_total.get() as u64
    }

    /// Tenant label used for non-tenant or system-wide database operations.
    pub fn tenant_label_system() -> &'static str {
        "system"
    }

    /// Residency type label for HOT KV cache entries
    pub fn kv_residency_hot() -> &'static str {
        "hot"
    }

    /// Residency type label for COLD KV cache entries
    pub fn kv_residency_cold() -> &'static str {
        "cold"
    }

    // ========================================
    // SingleFlight deduplication metrics helper functions
    // ========================================

    /// Record a SingleFlight leader (request that triggered the load)
    pub fn record_singleflight_leader(&self, operation: &str) {
        self.singleflight_leader_count
            .with_label_values(&[operation])
            .inc();
    }

    /// Record a SingleFlight waiter (request waiting for in-progress load)
    pub fn record_singleflight_waiter(&self, operation: &str) {
        self.singleflight_waiter_count
            .with_label_values(&[operation])
            .inc();
    }

    /// Set the current number of SingleFlight waiters for an operation
    pub fn set_singleflight_waiters(&self, operation: &str, count: usize) {
        self.singleflight_waiter_count
            .with_label_values(&[operation])
            .set(count as f64);
    }

    /// Record a SingleFlight error
    pub fn record_singleflight_error(&self, operation: &str, error_type: &str) {
        self.singleflight_error_count
            .with_label_values(&[operation, error_type])
            .inc();
    }

    /// Operation label for model load deduplication
    pub fn singleflight_op_model_load() -> &'static str {
        "model_load"
    }

    /// Operation label for adapter load deduplication
    pub fn singleflight_op_adapter_load() -> &'static str {
        "adapter_load"
    }

    /// Operation label for prefix KV build deduplication
    pub fn singleflight_op_prefix_kv() -> &'static str {
        "prefix_kv_build"
    }

    // ========================================
    // Tenant performance metrics helper functions
    // ========================================

    /// Record database query duration, keeping the tenant label first to support high-cardinality slicing.
    pub fn record_db_query_duration(
        &self,
        query_type: &str,
        table_name: &str,
        tenant_id: &str,
        duration_seconds: f64,
    ) {
        self.db_query_duration_seconds
            .with_label_values(&[tenant_id, query_type, table_name])
            .observe(duration_seconds);
    }

    /// Record database index scan
    pub fn record_db_index_scan(
        &self,
        table_name: &str,
        index_name: &str,
        scan_type: &str,
        tenant_id: &str,
    ) {
        self.db_index_scan_total
            .with_label_values(&[tenant_id, table_name, index_name, scan_type])
            .inc();
    }

    /// Set database composite index hit ratio
    pub fn set_db_composite_index_hit_ratio(
        &self,
        table_name: &str,
        index_name: &str,
        tenant_id: &str,
        ratio: f64,
    ) {
        self.db_composite_index_hit_ratio
            .with_label_values(&[tenant_id, table_name, index_name])
            .set(ratio);
    }

    /// Record tenant-specific query duration
    pub fn record_tenant_query_duration(
        &self,
        tenant_id: &str,
        query_type: &str,
        duration_seconds: f64,
    ) {
        self.db_tenant_query_duration_seconds
            .with_label_values(&[tenant_id, query_type])
            .observe(duration_seconds);
    }

    /// Record tenant-specific query error
    pub fn record_tenant_query_error(&self, tenant_id: &str, query_type: &str, error_type: &str) {
        self.db_tenant_query_errors_total
            .with_label_values(&[tenant_id, query_type, error_type])
            .inc();
    }

    // ========================================
    // UDS phase breakdown metrics helper functions
    // ========================================

    /// Record UDS connect phase latency in seconds
    pub fn record_uds_connect_latency(&self, worker_id: &str, duration_seconds: f64) {
        self.uds_connect_latency_seconds
            .with_label_values(&[worker_id])
            .observe(duration_seconds);
    }

    /// Record UDS write phase latency in seconds
    pub fn record_uds_write_latency(&self, worker_id: &str, duration_seconds: f64) {
        self.uds_write_latency_seconds
            .with_label_values(&[worker_id])
            .observe(duration_seconds);
    }

    /// Record UDS read phase latency in seconds
    pub fn record_uds_read_latency(&self, worker_id: &str, duration_seconds: f64) {
        self.uds_read_latency_seconds
            .with_label_values(&[worker_id])
            .observe(duration_seconds);
    }

    /// Record all UDS phase timings at once
    pub fn record_uds_phase_timings(
        &self,
        worker_id: &str,
        connect_secs: f64,
        write_secs: f64,
        read_secs: f64,
    ) {
        self.record_uds_connect_latency(worker_id, connect_secs);
        self.record_uds_write_latency(worker_id, write_secs);
        self.record_uds_read_latency(worker_id, read_secs);
    }

    // ========================================
    // Worker inference timing metrics helper functions
    // ========================================

    /// Record worker queue wait time in seconds
    ///
    /// This measures the time from when a request arrives at the worker
    /// until inference actually begins (i.e., time spent waiting for
    /// resources, locks, or other queued requests).
    pub fn record_worker_queue_wait(&self, worker_id: &str, duration_seconds: f64) {
        self.worker_queue_wait_seconds
            .with_label_values(&[worker_id])
            .observe(duration_seconds);
    }

    /// Record worker generation time in seconds
    ///
    /// This measures the actual time spent generating tokens, excluding
    /// any queue wait time. Useful for understanding pure inference
    /// performance vs. queueing overhead.
    pub fn record_worker_generation_time(
        &self,
        worker_id: &str,
        model_id: &str,
        duration_seconds: f64,
    ) {
        self.worker_generation_seconds
            .with_label_values(&[worker_id, model_id])
            .observe(duration_seconds);
    }

    /// Record both queue wait and generation time at once
    ///
    /// Convenience method for recording both timing metrics together.
    pub fn record_worker_inference_timing(
        &self,
        worker_id: &str,
        model_id: &str,
        queue_wait_seconds: f64,
        generation_seconds: f64,
    ) {
        self.record_worker_queue_wait(worker_id, queue_wait_seconds);
        self.record_worker_generation_time(worker_id, model_id, generation_seconds);
    }

    // ========================================
    // Server handler latency metrics helper functions
    // ========================================

    /// Record server handler latency in seconds
    ///
    /// This measures pre-UDS latency: the time from request receipt to UDS call start.
    /// Useful for understanding control plane overhead before inference dispatch.
    ///
    /// # Arguments
    /// * `endpoint` - The API endpoint being called (e.g., "infer", "stream")
    /// * `status` - The request status (e.g., "success", "error")
    /// * `duration_seconds` - The latency in seconds
    pub fn record_server_handler_latency(
        &self,
        endpoint: &str,
        status: &str,
        duration_seconds: f64,
    ) {
        self.server_handler_latency_seconds
            .with_label_values(&[endpoint, status])
            .observe(duration_seconds);
    }
}

impl Default for CriticalComponentMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create default CriticalComponentMetrics")
    }
}

/// Implement SingleFlightMetrics trait for CriticalComponentMetrics
/// This allows CriticalComponentMetrics to be used as a metrics backend for SingleFlight
impl SingleFlightMetrics for CriticalComponentMetrics {
    fn record_leader(&self, operation: &str) {
        self.record_singleflight_leader(operation);
    }

    fn record_waiter(&self, operation: &str) {
        self.record_singleflight_waiter(operation);
    }

    fn set_waiter_gauge(&self, operation: &str, count: usize) {
        self.set_singleflight_waiters(operation, count);
    }

    fn record_error(&self, operation: &str, error_type: &str) {
        self.record_singleflight_error(operation, error_type);
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

    #[test]
    fn test_model_cache_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record cache hits and misses
        metrics.record_model_cache_hit();
        metrics.record_model_cache_hit();
        metrics.record_model_cache_miss();
        metrics.record_eviction_blocked_pinned();

        // Verify counts
        assert_eq!(metrics.get_model_cache_hits(), 2.0);
        assert_eq!(metrics.get_model_cache_misses(), 1.0);

        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("model_cache_hits_total"));
        assert!(export.contains("model_cache_misses_total"));
        assert!(export.contains("model_cache_eviction_blocked_pinned_total"));
    }

    #[test]
    fn test_residency_probe_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Initial state - no probes run
        assert_eq!(metrics.get_residency_probe_runs(), 0.0);

        // Record successful probe
        metrics.record_residency_probe(true);
        assert!(metrics.get_residency_probe_ok());
        assert_eq!(metrics.get_residency_probe_runs(), 1.0);

        // Record failed probe
        metrics.record_residency_probe(false);
        assert!(!metrics.get_residency_probe_ok());
        assert_eq!(metrics.get_residency_probe_runs(), 2.0);

        // Set ok status directly
        metrics.set_residency_probe_ok(true);
        assert!(metrics.get_residency_probe_ok());

        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("residency_probe_ok"));
        assert!(export.contains("residency_probe_runs_total"));
    }

    #[test]
    fn test_kv_residency_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Set HOT and COLD entry counts
        metrics.set_kv_hot_entries(42);
        metrics.set_kv_cold_entries(128);
        assert_eq!(metrics.get_kv_hot_entries(), 42);
        assert_eq!(metrics.get_kv_cold_entries(), 128);

        // Set HOT and COLD byte counts
        metrics.set_kv_hot_bytes(1024 * 1024 * 100); // 100 MB
        metrics.set_kv_cold_bytes(1024 * 1024 * 500); // 500 MB
        assert_eq!(metrics.get_kv_hot_bytes(), 1024 * 1024 * 100);
        assert_eq!(metrics.get_kv_cold_bytes(), 1024 * 1024 * 500);

        // Record evictions
        metrics.record_kv_eviction(CriticalComponentMetrics::kv_residency_hot());
        metrics.record_kv_eviction(CriticalComponentMetrics::kv_residency_cold());
        metrics.record_kv_evictions(CriticalComponentMetrics::kv_residency_cold(), 5);

        // Record quota and purgeable failures
        metrics.record_kv_quota_exceeded();
        metrics.record_kv_quota_exceeded();
        metrics.record_kv_purgeable_failure();

        assert_eq!(metrics.get_kv_quota_exceeded_total(), 2);
        assert_eq!(metrics.get_kv_purgeable_failures_total(), 1);

        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("kv_hot_entries"));
        assert!(export.contains("kv_cold_entries"));
        assert!(export.contains("kv_hot_bytes"));
        assert!(export.contains("kv_cold_bytes"));
        assert!(export.contains("kv_evictions_by_residency_total"));
        assert!(export.contains("kv_quota_exceeded_total"));
        assert!(export.contains("kv_purgeable_failures_total"));
    }

    #[test]
    fn test_kv_residency_type_labels() {
        assert_eq!(CriticalComponentMetrics::kv_residency_hot(), "hot");
        assert_eq!(CriticalComponentMetrics::kv_residency_cold(), "cold");
    }

    #[test]
    fn test_singleflight_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record leaders
        metrics.record_singleflight_leader(CriticalComponentMetrics::singleflight_op_model_load());
        metrics
            .record_singleflight_leader(CriticalComponentMetrics::singleflight_op_adapter_load());
        metrics.record_singleflight_leader(CriticalComponentMetrics::singleflight_op_prefix_kv());

        // Record waiters
        metrics.record_singleflight_waiter(CriticalComponentMetrics::singleflight_op_model_load());
        metrics.set_singleflight_waiters(CriticalComponentMetrics::singleflight_op_model_load(), 5);

        // Record errors
        metrics.record_singleflight_error(
            CriticalComponentMetrics::singleflight_op_model_load(),
            "timeout",
        );
        metrics.record_singleflight_error(
            CriticalComponentMetrics::singleflight_op_adapter_load(),
            "load_error",
        );

        let export = metrics.export().expect("Failed to export");
        assert!(export.contains("singleflight_leader_count_total"));
        assert!(export.contains("singleflight_waiter_count"));
        assert!(export.contains("singleflight_error_count_total"));
        assert!(export.contains("model_load"));
        assert!(export.contains("adapter_load"));
        assert!(export.contains("prefix_kv_build"));
    }

    #[test]
    fn test_singleflight_operation_labels() {
        assert_eq!(
            CriticalComponentMetrics::singleflight_op_model_load(),
            "model_load"
        );
        assert_eq!(
            CriticalComponentMetrics::singleflight_op_adapter_load(),
            "adapter_load"
        );
        assert_eq!(
            CriticalComponentMetrics::singleflight_op_prefix_kv(),
            "prefix_kv_build"
        );
    }

    #[test]
    fn test_worker_inference_timing_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record queue wait time
        metrics.record_worker_queue_wait("worker-1", 0.015); // 15ms queue wait

        // Record generation time
        metrics.record_worker_generation_time("worker-1", "qwen-7b", 0.250); // 250ms generation

        // Record combined timing
        metrics.record_worker_inference_timing("worker-2", "qwen-32b", 0.005, 1.5);

        let export = metrics.export().expect("Failed to export");
        assert!(
            export.contains("worker_queue_wait_seconds"),
            "Export should contain worker_queue_wait_seconds"
        );
        assert!(
            export.contains("worker_generation_seconds"),
            "Export should contain worker_generation_seconds"
        );
        assert!(
            export.contains("worker-1"),
            "Export should contain worker_id label"
        );
        assert!(
            export.contains("qwen-7b"),
            "Export should contain model_id label"
        );
    }
}
