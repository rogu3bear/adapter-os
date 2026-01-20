//! Stress testing infrastructure for adapterOS
//!
//! Provides tools for testing concurrent operations, memory pressure scenarios,
//! and system resilience under load.

use crate::retry::{retry_async, RetryConfig};
use crate::state::AppState;
use adapteros_system_metrics::SystemMetricsCollector;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

/// Configuration for stress testing
#[derive(Debug, Clone)]
pub struct StressTestConfig {
    /// Number of concurrent operations
    pub concurrency: usize,
    /// Total number of operations to perform
    pub total_operations: usize,
    /// Memory pressure level (0.0 = none, 1.0 = maximum)
    pub memory_pressure: f64,
    /// Test duration limit
    pub duration_limit: Duration,
    /// Whether to simulate failures
    pub simulate_failures: bool,
    /// Failure rate (0.0 to 1.0)
    pub failure_rate: f64,
}

impl Default for StressTestConfig {
    fn default() -> Self {
        Self {
            concurrency: 10,
            total_operations: 1000,
            memory_pressure: 0.0,
            duration_limit: Duration::from_secs(300), // 5 minutes
            simulate_failures: false,
            failure_rate: 0.0,
        }
    }
}

/// Results of a stress test
#[derive(Debug)]
pub struct StressTestResults {
    pub total_operations: usize,
    pub successful_operations: usize,
    pub failed_operations: usize,
    pub average_latency: Duration,
    pub p50_latency: Duration,
    pub p95_latency: Duration,
    pub p99_latency: Duration,
    pub total_duration: Duration,
    pub operations_per_second: f64,
    pub memory_peak_usage: Option<u64>,
    pub error_breakdown: std::collections::HashMap<String, usize>,
}

/// Stress testing coordinator
pub struct StressTester {
    config: StressTestConfig,
}

impl StressTester {
    pub fn new(config: StressTestConfig) -> Self {
        Self { config }
    }

    /// Run a comprehensive stress test
    pub async fn run_comprehensive_test(
        &self,
        state: &Arc<AppState>,
    ) -> Result<StressTestResults, String> {
        info!(
            "Starting comprehensive stress test with config: {:?}",
            self.config
        );

        let start_time = Instant::now();
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));

        // Track peak memory usage
        let peak_memory = Arc::new(AtomicU64::new(0));
        let mut collector = SystemMetricsCollector::new();
        peak_memory.store(collector.used_memory(), Ordering::Relaxed);

        let mut join_set = JoinSet::new();
        let mut latencies = Vec::new();
        let mut error_counts = std::collections::HashMap::new();

        // Spawn worker tasks
        for i in 0..self.config.total_operations {
            let semaphore = semaphore.clone();
            let state = state.clone();
            let config = self.config.clone();

            join_set.spawn(async move {
                let _permit = semaphore
                    .acquire()
                    .await
                    .expect("Failed to acquire semaphore permit");

                let operation_start = Instant::now();
                let result = Self::execute_test_operation(&state, i, &config).await;
                let latency = operation_start.elapsed();

                (result, latency)
            });
        }

        // Collect results
        let mut successful = 0;
        let mut failed = 0;

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((operation_result, latency)) => {
                    latencies.push(latency);

                    match operation_result {
                        Ok(_) => successful += 1,
                        Err(error_type) => {
                            failed += 1;
                            *error_counts.entry(error_type).or_insert(0) += 1;
                        }
                    }
                }
                Err(e) => {
                    error!("Task join error: {:?}", e);
                    failed += 1;
                }
            }

            // Sample memory periodically (every ~10 results)
            if (successful + failed) % 10 == 0 {
                collector.collect_metrics();
                let current = collector.used_memory();
                peak_memory.fetch_max(current, Ordering::Relaxed);
            }
        }

        let total_duration = start_time.elapsed();

        // Calculate latency percentiles
        latencies.sort();
        let p50 = Self::calculate_percentile(&latencies, 50.0);
        let p95 = Self::calculate_percentile(&latencies, 95.0);
        let p99 = Self::calculate_percentile(&latencies, 99.0);
        let avg = latencies.iter().sum::<Duration>() / latencies.len() as u32;

        let ops_per_sec = self.config.total_operations as f64 / total_duration.as_secs_f64();

        let results = StressTestResults {
            total_operations: self.config.total_operations,
            successful_operations: successful,
            failed_operations: failed,
            average_latency: avg,
            p50_latency: p50,
            p95_latency: p95,
            p99_latency: p99,
            total_duration,
            operations_per_second: ops_per_sec,
            memory_peak_usage: Some(peak_memory.load(Ordering::Relaxed)),
            error_breakdown: error_counts,
        };

        info!("Stress test completed: {:?}", results);
        Ok(results)
    }

    /// Execute a single test operation (simulates real workload)
    async fn execute_test_operation(
        state: &Arc<AppState>,
        operation_id: usize,
        config: &StressTestConfig,
    ) -> Result<(), String> {
        // Simulate different types of operations
        match operation_id % 4 {
            0 => Self::test_model_import_operation(state, operation_id, config).await,
            1 => Self::test_model_load_operation(state, operation_id, config).await,
            2 => Self::test_adapter_load_operation(state, operation_id, config).await,
            3 => Self::test_concurrent_inference(state, operation_id, config).await,
            _ => unreachable!(),
        }
    }

    /// Test model import operations under load
    async fn test_model_import_operation(
        _state: &Arc<AppState>,
        operation_id: usize,
        config: &StressTestConfig,
    ) -> Result<(), String> {
        // Simulate file validation and import processing time
        Self::simulate_processing_time(config.memory_pressure).await;

        // Simulate occasional failures
        if config.simulate_failures && rand::random::<f64>() < config.failure_rate {
            return Err("simulated_import_failure".to_string());
        }

        // Simulate database operation with retry
        let retry_config = RetryConfig::database();
        let db_result = retry_async(&retry_config, || async {
            // Simulate database write
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok::<(), String>(())
        })
        .await;

        match db_result {
            crate::retry::RetryResult::Success(_) => Ok(()),
            crate::retry::RetryResult::Failed(_) => Err("database_import_failure".to_string()),
        }
    }

    /// Test model loading operations
    async fn test_model_load_operation(
        state: &Arc<AppState>,
        operation_id: usize,
        config: &StressTestConfig,
    ) -> Result<(), String> {
        Self::simulate_processing_time(config.memory_pressure).await;

        if let Some(ref model_runtime) = state.model_runtime {
            // Test lazy loading under concurrent access
            let tenant_id = format!("tenant_{}", operation_id % 10);
            let model_id = format!("model_{}", operation_id % 5);

            // This would trigger lazy loading if enabled
            let _ = model_runtime
                .lock()
                .await
                .ensure_model_loaded(&tenant_id, &model_id)
                .await;
        }

        if config.simulate_failures && rand::random::<f64>() < config.failure_rate {
            return Err("simulated_load_failure".to_string());
        }

        Ok(())
    }

    /// Test adapter loading operations
    async fn test_adapter_load_operation(
        _state: &Arc<AppState>,
        operation_id: usize,
        config: &StressTestConfig,
    ) -> Result<(), String> {
        Self::simulate_processing_time(config.memory_pressure).await;

        // Simulate adapter loading with potential memory pressure
        let memory_factor = 1.0 + config.memory_pressure;
        let load_time = Duration::from_millis((50.0 * memory_factor) as u64);
        tokio::time::sleep(load_time).await;

        if config.simulate_failures && rand::random::<f64>() < config.failure_rate {
            return Err("simulated_adapter_failure".to_string());
        }

        Ok(())
    }

    /// Test concurrent inference operations
    async fn test_concurrent_inference(
        _state: &Arc<AppState>,
        operation_id: usize,
        config: &StressTestConfig,
    ) -> Result<(), String> {
        Self::simulate_processing_time(config.memory_pressure).await;

        // Simulate inference time with memory pressure effects
        let memory_factor = 1.0 + config.memory_pressure;
        let inference_time = Duration::from_millis((20.0 * memory_factor) as u64);
        tokio::time::sleep(inference_time).await;

        if config.simulate_failures && rand::random::<f64>() < config.failure_rate {
            return Err("simulated_inference_failure".to_string());
        }

        Ok(())
    }

    /// Simulate processing time affected by memory pressure
    async fn simulate_processing_time(memory_pressure: f64) {
        // Under memory pressure, operations take longer
        let base_delay = Duration::from_millis(10);
        let pressure_factor = 1.0 + memory_pressure * 2.0; // Up to 3x slower
        let delay = Duration::from_millis((base_delay.as_millis() as f64 * pressure_factor) as u64);

        tokio::time::sleep(delay.min(Duration::from_millis(100))).await;
    }

    /// Calculate latency percentile
    fn calculate_percentile(latencies: &[Duration], percentile: f64) -> Duration {
        if latencies.is_empty() {
            return Duration::ZERO;
        }

        let index = ((latencies.len() - 1) as f64 * percentile / 100.0) as usize;
        latencies[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stress_test_config_defaults() {
        let config = StressTestConfig::default();
        assert_eq!(config.concurrency, 10);
        assert_eq!(config.total_operations, 1000);
        assert_eq!(config.memory_pressure, 0.0);
    }

    #[test]
    fn test_calculate_percentile() {
        let latencies = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(30),
            Duration::from_millis(40),
            Duration::from_millis(50),
        ];

        assert_eq!(
            StressTester::calculate_percentile(&latencies, 50.0),
            Duration::from_millis(30)
        );
        assert_eq!(
            StressTester::calculate_percentile(&latencies, 95.0),
            Duration::from_millis(50)
        );
    }
}
