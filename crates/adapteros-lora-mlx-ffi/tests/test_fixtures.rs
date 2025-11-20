//! Test fixtures and utilities for MLX backend testing
//!
//! Provides reusable test fixtures, helpers, and baseline data for comprehensive testing.

pub mod fixtures {
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::{LoRAAdapter, ModelConfig};

    /// Standard test model configurations
    pub struct StandardConfigs;

    impl StandardConfigs {
        /// Small model for fast testing
        pub fn small() -> ModelConfig {
            ModelConfig {
                hidden_size: 512,
                num_hidden_layers: 8,
                num_attention_heads: 8,
                num_key_value_heads: 4,
                intermediate_size: 2048,
                vocab_size: 8000,
                max_position_embeddings: 2048,
                rope_theta: 10000.0,
            }
        }

        /// Medium model (default mock)
        pub fn medium() -> ModelConfig {
            create_mock_config()
        }

        /// Large model for stress testing
        pub fn large() -> ModelConfig {
            ModelConfig {
                hidden_size: 8192,
                num_hidden_layers: 64,
                num_attention_heads: 64,
                num_key_value_heads: 16,
                intermediate_size: 22016,
                vocab_size: 64000,
                max_position_embeddings: 65536,
                rope_theta: 10000.0,
            }
        }
    }

    /// Standard adapter configurations
    pub struct StandardAdapters;

    impl StandardAdapters {
        /// Small rank adapter (fast, low memory)
        pub fn small(id: &str) -> LoRAAdapter {
            create_mock_adapter(id, 4)
        }

        /// Medium rank adapter (balanced)
        pub fn medium(id: &str) -> LoRAAdapter {
            create_mock_adapter(id, 8)
        }

        /// Large rank adapter (high capacity)
        pub fn large(id: &str) -> LoRAAdapter {
            create_mock_adapter(id, 16)
        }

        /// Extra large rank adapter (stress testing)
        pub fn extra_large(id: &str) -> LoRAAdapter {
            create_mock_adapter(id, 64)
        }
    }

    /// Test prompts for inference testing
    pub struct TestPrompts;

    impl TestPrompts {
        /// Short prompt
        pub fn short() -> Vec<u32> {
            vec![1, 2, 3]
        }

        /// Medium prompt
        pub fn medium() -> Vec<u32> {
            (1..=32).collect()
        }

        /// Long prompt
        pub fn long() -> Vec<u32> {
            (1..=256).collect()
        }

        /// Very long prompt (stress testing)
        pub fn very_long() -> Vec<u32> {
            (1..=2048).collect()
        }

        /// Random-like prompt pattern
        pub fn random_pattern(seed: u32, length: usize) -> Vec<u32> {
            (0..length)
                .map(|i| ((seed.wrapping_mul(31).wrapping_add(i as u32)) % 32000) + 1)
                .collect()
        }
    }

    /// Performance baselines for regression testing
    pub struct PerformanceBaselines;

    impl PerformanceBaselines {
        /// Maximum acceptable adapter registration time (ms)
        pub const ADAPTER_REGISTRATION_MS: u128 = 100;

        /// Maximum acceptable adapter unload time (ms)
        pub const ADAPTER_UNLOAD_MS: u128 = 50;

        /// Maximum acceptable hot-swap time (ms)
        pub const HOT_SWAP_MS: u128 = 100;

        /// Maximum acceptable memory query time (ms)
        pub const MEMORY_QUERY_MS: u128 = 10;

        /// Maximum acceptable GC time (ms)
        pub const GC_TIME_MS: u128 = 10;

        /// Maximum acceptable memory growth over 1000 cycles (MB)
        pub const MAX_MEMORY_GROWTH_MB: f32 = 100.0;

        /// Maximum acceptable memory retention after cleanup (MB)
        pub const MAX_MEMORY_RETENTION_MB: f32 = 10.0;
    }

    /// Memory thresholds for testing
    pub struct MemoryThresholds;

    impl MemoryThresholds {
        /// Warning threshold (MB)
        pub const WARNING_MB: f32 = 2048.0;

        /// Critical threshold (MB)
        pub const CRITICAL_MB: f32 = 4096.0;

        /// Headroom threshold (MB)
        pub const HEADROOM_MB: f32 = 512.0;
    }

    /// Expected output patterns for validation
    pub struct ExpectedOutputs;

    impl ExpectedOutputs {
        /// Validate logits shape
        pub fn validate_logits_shape(logits: &[f32], vocab_size: usize) -> bool {
            logits.len() == vocab_size
        }

        /// Validate logits have no NaN/Inf
        pub fn validate_logits_finite(logits: &[f32]) -> bool {
            logits.iter().all(|&x| x.is_finite())
        }

        /// Validate logits range (reasonable values)
        pub fn validate_logits_range(logits: &[f32]) -> bool {
            logits.iter().all(|&x| x.abs() < 1e6)
        }

        /// Validate hidden states structure
        pub fn validate_hidden_states(
            hidden_states: &std::collections::HashMap<String, Vec<f32>>,
            expected_modules: &[&str],
        ) -> bool {
            if hidden_states.len() != expected_modules.len() {
                return false;
            }

            for module_name in expected_modules {
                if !hidden_states.contains_key(*module_name) {
                    return false;
                }
            }

            true
        }
    }
}

pub mod helpers {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::memory;
    use adapteros_lora_mlx_ffi::MLXFFIModel;
    use std::time::Instant;

    /// Memory snapshot for tracking
    #[derive(Debug, Clone, Copy)]
    pub struct MemorySnapshot {
        pub total_bytes: usize,
        pub allocation_count: usize,
        pub timestamp: std::time::Instant,
    }

    impl MemorySnapshot {
        /// Capture current memory state
        pub fn capture() -> Self {
            let stats = memory::stats();
            Self {
                total_bytes: stats.total_bytes,
                allocation_count: stats.allocation_count,
                timestamp: Instant::now(),
            }
        }

        /// Calculate memory growth since snapshot
        pub fn growth_mb(&self) -> f32 {
            let current = memory::stats();
            let growth_bytes = current.total_bytes.saturating_sub(self.total_bytes);
            memory::bytes_to_mb(growth_bytes)
        }

        /// Calculate allocation delta since snapshot
        pub fn allocation_delta(&self) -> i64 {
            let current = memory::stats();
            current.allocation_count as i64 - self.allocation_count as i64
        }

        /// Time elapsed since snapshot
        pub fn elapsed_ms(&self) -> u128 {
            self.timestamp.elapsed().as_millis()
        }
    }

    /// Performance metrics tracker
    #[derive(Debug, Default)]
    pub struct PerformanceMetrics {
        pub operation_count: usize,
        pub total_time_ms: u128,
        pub min_time_ms: u128,
        pub max_time_ms: u128,
    }

    impl PerformanceMetrics {
        pub fn new() -> Self {
            Self {
                operation_count: 0,
                total_time_ms: 0,
                min_time_ms: u128::MAX,
                max_time_ms: 0,
            }
        }

        pub fn record(&mut self, time_ms: u128) {
            self.operation_count += 1;
            self.total_time_ms += time_ms;
            self.min_time_ms = self.min_time_ms.min(time_ms);
            self.max_time_ms = self.max_time_ms.max(time_ms);
        }

        pub fn average_ms(&self) -> f64 {
            if self.operation_count == 0 {
                0.0
            } else {
                self.total_time_ms as f64 / self.operation_count as f64
            }
        }

        pub fn report(&self) -> String {
            format!(
                "Operations: {}, Avg: {:.2}ms, Min: {}ms, Max: {}ms",
                self.operation_count,
                self.average_ms(),
                self.min_time_ms,
                self.max_time_ms
            )
        }
    }

    /// Adapter lifecycle tracker
    pub struct AdapterLifecycleTracker {
        pub loaded: Vec<u16>,
        pub load_times: std::collections::HashMap<u16, Instant>,
    }

    impl AdapterLifecycleTracker {
        pub fn new() -> Self {
            Self {
                loaded: Vec::new(),
                load_times: std::collections::HashMap::new(),
            }
        }

        pub fn track_load(&mut self, adapter_id: u16) {
            if !self.loaded.contains(&adapter_id) {
                self.loaded.push(adapter_id);
            }
            self.load_times.insert(adapter_id, Instant::now());
        }

        pub fn track_unload(&mut self, adapter_id: u16) {
            self.loaded.retain(|&id| id != adapter_id);
            self.load_times.remove(&adapter_id);
        }

        pub fn loaded_count(&self) -> usize {
            self.loaded.len()
        }

        pub fn time_loaded_ms(&self, adapter_id: u16) -> Option<u128> {
            self.load_times
                .get(&adapter_id)
                .map(|t| t.elapsed().as_millis())
        }
    }

    /// Test backend builder with fluent API
    pub struct TestBackendBuilder {
        model_hash_seed: Vec<u8>,
        with_adapters: Vec<(u16, String, usize)>,
    }

    impl TestBackendBuilder {
        pub fn new() -> Self {
            Self {
                model_hash_seed: b"test-backend".to_vec(),
                with_adapters: Vec::new(),
            }
        }

        pub fn with_hash_seed(mut self, seed: &[u8]) -> Self {
            self.model_hash_seed = seed.to_vec();
            self
        }

        pub fn with_adapter(mut self, id: u16, name: &str, rank: usize) -> Self {
            self.with_adapters.push((id, name.to_string(), rank));
            self
        }

        pub fn build(self) -> MLXFFIBackend {
            use super::fixtures::StandardConfigs;

            let config = StandardConfigs::medium();
            let model = MLXFFIModel {
                model: std::ptr::null_mut(),
                config,
                model_hash: adapteros_core::B3Hash::hash(&self.model_hash_seed),
            };

            let backend = MLXFFIBackend::new(model);

            // Load adapters if specified
            for (id, name, rank) in self.with_adapters {
                use adapteros_lora_mlx_ffi::mock::create_mock_adapter;
                let adapter = create_mock_adapter(&name, rank);
                backend.register_adapter(id, adapter).unwrap();
            }

            backend
        }
    }

    /// Assertion helpers
    pub fn assert_memory_stable(before: &MemorySnapshot, after: &MemorySnapshot, max_growth_mb: f32) {
        let growth_bytes = after.total_bytes.saturating_sub(before.total_bytes);
        let growth_mb = memory::bytes_to_mb(growth_bytes);

        assert!(
            growth_mb <= max_growth_mb,
            "Memory grew by {:.2} MB (max: {} MB)",
            growth_mb,
            max_growth_mb
        );
    }

    pub fn assert_performance_acceptable(actual_ms: u128, max_ms: u128, operation: &str) {
        assert!(
            actual_ms <= max_ms,
            "{} took {} ms (max: {} ms)",
            operation,
            actual_ms,
            max_ms
        );
    }

    pub fn assert_no_allocations_leaked(before_count: usize, after_count: usize) {
        assert!(
            after_count <= before_count,
            "Allocations leaked: before={}, after={}",
            before_count,
            after_count
        );
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::fixtures::*;
    use super::helpers::*;

    #[test]
    fn test_standard_configs() {
        let small = StandardConfigs::small();
        assert_eq!(small.hidden_size, 512);
        assert_eq!(small.vocab_size, 8000);

        let medium = StandardConfigs::medium();
        assert_eq!(medium.hidden_size, 4096);
        assert_eq!(medium.vocab_size, 32000);

        let large = StandardConfigs::large();
        assert_eq!(large.hidden_size, 8192);
        assert_eq!(large.vocab_size, 64000);
    }

    #[test]
    fn test_standard_adapters() {
        let small = StandardAdapters::small("small-test");
        assert_eq!(small.config().rank, 4);

        let medium = StandardAdapters::medium("medium-test");
        assert_eq!(medium.config().rank, 8);

        let large = StandardAdapters::large("large-test");
        assert_eq!(large.config().rank, 16);

        let xl = StandardAdapters::extra_large("xl-test");
        assert_eq!(xl.config().rank, 64);
    }

    #[test]
    fn test_test_prompts() {
        assert_eq!(TestPrompts::short().len(), 3);
        assert_eq!(TestPrompts::medium().len(), 32);
        assert_eq!(TestPrompts::long().len(), 256);
        assert_eq!(TestPrompts::very_long().len(), 2048);

        let random = TestPrompts::random_pattern(42, 100);
        assert_eq!(random.len(), 100);

        // Should be deterministic
        let random2 = TestPrompts::random_pattern(42, 100);
        assert_eq!(random, random2);
    }

    #[test]
    fn test_expected_outputs_validation() {
        let logits = vec![0.0; 32000];
        assert!(ExpectedOutputs::validate_logits_shape(&logits, 32000));
        assert!(ExpectedOutputs::validate_logits_finite(&logits));
        assert!(ExpectedOutputs::validate_logits_range(&logits));

        // Test with invalid logits
        let invalid = vec![f32::NAN, f32::INFINITY, 1e10];
        assert!(!ExpectedOutputs::validate_logits_finite(&invalid));
    }

    #[test]
    fn test_memory_snapshot() {
        use adapteros_lora_mlx_ffi::memory;

        memory::reset();

        let snapshot = MemorySnapshot::capture();
        assert_eq!(snapshot.total_bytes, 0);
        assert_eq!(snapshot.allocation_count, 0);

        let growth = snapshot.growth_mb();
        assert!((growth - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_performance_metrics() {
        let mut metrics = PerformanceMetrics::new();

        metrics.record(10);
        metrics.record(20);
        metrics.record(30);

        assert_eq!(metrics.operation_count, 3);
        assert_eq!(metrics.min_time_ms, 10);
        assert_eq!(metrics.max_time_ms, 30);
        assert!((metrics.average_ms() - 20.0).abs() < 0.01);

        let report = metrics.report();
        assert!(report.contains("Operations: 3"));
    }

    #[test]
    fn test_adapter_lifecycle_tracker() {
        let mut tracker = AdapterLifecycleTracker::new();

        tracker.track_load(1);
        tracker.track_load(2);
        assert_eq!(tracker.loaded_count(), 2);

        tracker.track_unload(1);
        assert_eq!(tracker.loaded_count(), 1);

        assert!(tracker.time_loaded_ms(2).is_some());
        assert!(tracker.time_loaded_ms(1).is_none());
    }

    #[test]
    fn test_backend_builder() {
        let backend = TestBackendBuilder::new()
            .with_hash_seed(b"test-seed")
            .with_adapter(1, "adapter1", 8)
            .with_adapter(2, "adapter2", 4)
            .build();

        assert_eq!(backend.adapter_count(), 2);
    }

    #[test]
    fn test_assertion_helpers() {
        use adapteros_lora_mlx_ffi::memory;

        memory::reset();

        let before = MemorySnapshot::capture();
        let after = MemorySnapshot::capture();

        assert_memory_stable(&before, &after, 10.0);
        assert_performance_acceptable(50, 100, "test operation");
        assert_no_allocations_leaked(10, 5);
    }
}
