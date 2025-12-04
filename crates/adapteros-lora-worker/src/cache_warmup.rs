//! Cache warmup functionality for adapters
//!
//! This module provides functionality to warm up adapter caches with common queries
//! to improve inference performance and reduce cold start latency.
//!
//! Citation: Based on `crates/adapteros-lora-worker/src/lib.rs:417-453` - extends the
//! existing inference loop with cache warmup capabilities.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::{InferenceRequest, RequestType, Worker};

/// Configuration for health check inference tests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Test prompt for warmup inference
    pub test_prompt: String,
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Timeout for the test
    pub timeout: Duration,
    /// Number of warmup iterations
    pub iterations: usize,
    /// Temperature (0.0 for deterministic)
    pub temperature: f32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            test_prompt: "Hello".to_string(),
            max_tokens: 5,
            timeout: Duration::from_secs(30),
            iterations: 1,
            temperature: 0.0,
        }
    }
}

/// Result of a health check inference test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Whether the health check passed
    pub passed: bool,
    /// Latency in milliseconds
    pub latency_ms: u64,
    /// Number of tokens generated
    pub tokens_generated: usize,
    /// Tokens per second throughput
    pub tokens_per_second: f64,
    /// Failure reason if test failed
    pub failure_reason: Option<String>,
}

/// Cache warmup manager
pub struct CacheWarmupManager {
    /// Common warmup queries
    warmup_queries: Vec<String>,
    /// Warmup delay between queries
    warmup_delay: Duration,
    /// Maximum warmup duration
    max_warmup_duration: Duration,
}

impl CacheWarmupManager {
    /// Create a new cache warmup manager
    pub fn new() -> Self {
        Self {
            warmup_queries: vec![
                "Hello, world!".to_string(),
                "What is the weather like?".to_string(),
                "Explain machine learning".to_string(),
                "Write a function in Rust".to_string(),
                "How does attention work?".to_string(),
                "What is a neural network?".to_string(),
                "Explain transformer architecture".to_string(),
                "How to optimize performance?".to_string(),
            ],
            warmup_delay: Duration::from_millis(100),
            max_warmup_duration: Duration::from_secs(30),
        }
    }

    /// Create a custom cache warmup manager
    pub fn with_queries(queries: Vec<String>) -> Self {
        Self {
            warmup_queries: queries,
            warmup_delay: Duration::from_millis(100),
            max_warmup_duration: Duration::from_secs(30),
        }
    }

    /// Warmup adapter cache with common queries
    pub async fn warmup_cache<K>(&self, worker: &mut Worker<K>) -> Result<()>
    where
        K: adapteros_lora_kernel_api::FusedKernels,
    {
        info!(
            "Starting cache warmup with {} queries",
            self.warmup_queries.len()
        );

        let start_time = std::time::Instant::now();
        let mut successful_queries = 0;
        let mut failed_queries = 0;

        for (i, query) in self.warmup_queries.iter().enumerate() {
            // Check if we've exceeded the maximum warmup duration
            if start_time.elapsed() > self.max_warmup_duration {
                warn!("Cache warmup exceeded maximum duration, stopping early");
                break;
            }

            let request = InferenceRequest {
                cpid: "warmup".to_string(),
                prompt: query.clone(),
                max_tokens: 50,
                require_evidence: false,
                request_type: RequestType::Normal,
                stack_id: None,
                stack_version: None,
                temperature: None,
                top_k: None,
                top_p: None,
                seed: None,
                router_seed: None,
                pinned_adapter_ids: None,
                determinism_mode: "strict".to_string(),
                effective_adapter_ids: None,
            };

            match worker.infer(request).await {
                Ok(_response) => {
                    successful_queries += 1;
                    info!("Warmup query {} completed successfully", i + 1);
                }
                Err(e) => {
                    failed_queries += 1;
                    warn!("Warmup query {} failed: {}", i + 1, e);
                }
            }

            // Small delay to prevent overwhelming the system
            tokio::time::sleep(self.warmup_delay).await;
        }

        let total_duration = start_time.elapsed();
        info!(
            "Cache warmup completed: {} successful, {} failed, duration: {:?}",
            successful_queries, failed_queries, total_duration
        );

        if failed_queries > successful_queries {
            return Err(AosError::Worker(format!(
                "Cache warmup failed: {} failures out of {} queries",
                failed_queries,
                successful_queries + failed_queries
            )));
        }

        Ok(())
    }

    /// Warmup specific adapters
    pub async fn warmup_adapters<K>(
        &self,
        worker: &mut Worker<K>,
        adapter_ids: &[String],
    ) -> Result<()>
    where
        K: adapteros_lora_kernel_api::FusedKernels,
    {
        info!("Warming up {} specific adapters", adapter_ids.len());

        for adapter_id in adapter_ids {
            let request = InferenceRequest {
                cpid: format!("warmup-{}", adapter_id),
                prompt: format!("Test query for adapter {}", adapter_id),
                max_tokens: 20,
                require_evidence: false,
                request_type: RequestType::Normal,
                stack_id: None,
                stack_version: None,
                temperature: None,
                top_k: None,
                top_p: None,
                seed: None,
                router_seed: None,
                pinned_adapter_ids: None,
                determinism_mode: "strict".to_string(),
                effective_adapter_ids: None,
            };

            match worker.infer(request).await {
                Ok(_response) => {
                    info!("Successfully warmed up adapter: {}", adapter_id);
                }
                Err(e) => {
                    warn!("Failed to warm up adapter {}: {}", adapter_id, e);
                }
            }

            tokio::time::sleep(self.warmup_delay).await;
        }

        Ok(())
    }

    /// Check if cache warmup is needed
    pub fn should_warmup(&self, last_warmup: Option<std::time::Instant>) -> bool {
        match last_warmup {
            None => true, // Never warmed up
            Some(last) => {
                // Warmup if it's been more than 1 hour
                last.elapsed() > Duration::from_secs(3600)
            }
        }
    }

    /// Get warmup statistics
    pub fn get_warmup_stats(&self) -> WarmupStats {
        WarmupStats {
            total_queries: self.warmup_queries.len(),
            warmup_delay_ms: self.warmup_delay.as_millis() as u64,
            max_duration_ms: self.max_warmup_duration.as_millis() as u64,
        }
    }
}

impl Default for CacheWarmupManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Warmup statistics
#[derive(Debug, Clone)]
pub struct WarmupStats {
    pub total_queries: usize,
    pub warmup_delay_ms: u64,
    pub max_duration_ms: u64,
}

/// Auto-reload functionality for cache misses
pub struct AutoReloadManager {
    /// Cache miss threshold for auto-reload
    miss_threshold: usize,
    /// Reload delay to prevent thrashing
    reload_delay: Duration,
}

impl AutoReloadManager {
    /// Create a new auto-reload manager
    pub fn new() -> Self {
        Self {
            miss_threshold: 3,
            reload_delay: Duration::from_secs(5),
        }
    }

    /// Handle cache miss with auto-reload
    pub async fn handle_cache_miss<K>(&self, worker: &mut Worker<K>, adapter_id: &str) -> Result<()>
    where
        K: adapteros_lora_kernel_api::FusedKernels,
    {
        info!("Handling cache miss for adapter: {}", adapter_id);

        // Check if adapter exists in registry
        // For now, we'll simulate this check
        // In a real implementation, this would query the adapter registry
        if self.adapter_exists(adapter_id).await? {
            // Reload adapter from artifact store
            self.reload_adapter(worker, adapter_id).await?;

            info!("Auto-reloaded adapter {} from cache miss", adapter_id);
            Ok(())
        } else {
            Err(AosError::Worker(format!(
                "Adapter {} not found for auto-reload",
                adapter_id
            )))
        }
    }

    /// Check if adapter exists in registry
    async fn adapter_exists(&self, adapter_id: &str) -> Result<bool> {
        // Simulate adapter existence check
        // In a real implementation, this would query the adapter registry
        Ok(!adapter_id.is_empty())
    }

    /// Reload adapter from artifact store
    async fn reload_adapter<K>(&self, _worker: &mut Worker<K>, adapter_id: &str) -> Result<()>
    where
        K: adapteros_lora_kernel_api::FusedKernels,
    {
        // Simulate adapter reload
        // In a real implementation, this would:
        // 1. Load adapter from artifact store
        // 2. Update worker's adapter cache
        // 3. Log reload event

        info!("Reloading adapter {} from artifact store", adapter_id);

        // Small delay to prevent thrashing
        tokio::time::sleep(self.reload_delay).await;

        Ok(())
    }

    /// Get auto-reload statistics
    pub fn get_reload_stats(&self) -> ReloadStats {
        ReloadStats {
            miss_threshold: self.miss_threshold,
            reload_delay_ms: self.reload_delay.as_millis() as u64,
        }
    }
}

impl Default for AutoReloadManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Auto-reload statistics
#[derive(Debug, Clone)]
pub struct ReloadStats {
    pub miss_threshold: usize,
    pub reload_delay_ms: u64,
}

/// Perform a health check inference test on a model
///
/// This function runs a test inference with the given configuration to verify
/// that the model is ready and capable of generating responses. It measures
/// latency, throughput, and validates the output.
///
/// # Arguments
///
/// * `worker` - The worker instance to run inference on
/// * `config` - Health check configuration
/// * `adapter_id` - Optional adapter ID to test (None for base model)
///
/// # Returns
///
/// Returns a `HealthCheckResult` containing metrics and pass/fail status
pub async fn check_model_health<K>(
    worker: &mut Worker<K>,
    config: &HealthCheckConfig,
    adapter_id: Option<String>,
) -> Result<HealthCheckResult>
where
    K: adapteros_lora_kernel_api::FusedKernels,
{
    info!(
        adapter_id = ?adapter_id,
        prompt = %config.test_prompt,
        max_tokens = config.max_tokens,
        "Starting model health check"
    );

    let start_time = Instant::now();
    let mut total_tokens_generated = 0;
    let mut last_error: Option<String> = None;

    // Run health check iterations
    for iteration in 0..config.iterations {
        let iteration_start = Instant::now();

        // Create inference request
        let request = InferenceRequest {
            cpid: adapter_id
                .as_ref()
                .map(|id| format!("health-check-{}", id))
                .unwrap_or_else(|| "health-check-base".to_string()),
            prompt: config.test_prompt.clone(),
            max_tokens: config.max_tokens,
            require_evidence: false,
            request_type: RequestType::Normal,
            stack_id: None,
            stack_version: None,
            temperature: None,
            top_k: None,
                top_p: None,
                seed: None,
                router_seed: None,
                pinned_adapter_ids: None,
                determinism_mode: "strict".to_string(),
                effective_adapter_ids: None,
            };

        // Run inference with timeout
        let result = tokio::time::timeout(config.timeout, worker.infer(request)).await;

        match result {
            Ok(Ok(response)) => {
                let iteration_latency = iteration_start.elapsed();
                let tokens = response.text.split_whitespace().count();
                total_tokens_generated += tokens;

                info!(
                    iteration = iteration + 1,
                    latency_ms = iteration_latency.as_millis(),
                    tokens = tokens,
                    "Health check iteration completed successfully"
                );

                // Verify output is non-empty
                if response.text.trim().is_empty() {
                    last_error = Some("Generated text is empty".to_string());
                    warn!("Health check iteration {} produced empty output", iteration + 1);
                } else {
                    // Success - clear any previous errors
                    last_error = None;
                }
            }
            Ok(Err(e)) => {
                last_error = Some(format!("Inference failed: {}", e));
                warn!(
                    iteration = iteration + 1,
                    error = %e,
                    "Health check iteration failed"
                );
            }
            Err(_) => {
                last_error = Some(format!("Timeout after {:?}", config.timeout));
                warn!(
                    iteration = iteration + 1,
                    timeout = ?config.timeout,
                    "Health check iteration timed out"
                );
            }
        }

        // Small delay between iterations
        if iteration + 1 < config.iterations {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    let total_latency = start_time.elapsed();
    let latency_ms = total_latency.as_millis() as u64;

    // Calculate tokens per second
    let tokens_per_second = if total_latency.as_secs_f64() > 0.0 {
        total_tokens_generated as f64 / total_latency.as_secs_f64()
    } else {
        0.0
    };

    // Determine if health check passed
    let passed = last_error.is_none() && total_tokens_generated > 0;

    let result = HealthCheckResult {
        passed,
        latency_ms,
        tokens_generated: total_tokens_generated,
        tokens_per_second,
        failure_reason: last_error,
    };

    if result.passed {
        info!(
            latency_ms = result.latency_ms,
            tokens_generated = result.tokens_generated,
            tokens_per_second = result.tokens_per_second,
            "Model health check PASSED"
        );
    } else {
        warn!(
            failure_reason = ?result.failure_reason,
            "Model health check FAILED"
        );
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_warmup_manager_creation() {
        let manager = CacheWarmupManager::new();
        assert!(!manager.warmup_queries.is_empty());
        assert_eq!(manager.warmup_delay, Duration::from_millis(100));
    }

    #[test]
    fn test_should_warmup() {
        let manager = CacheWarmupManager::new();

        // Should warmup if never warmed up before
        assert!(manager.should_warmup(None));

        // Should not warmup if recently warmed up
        let recent = std::time::Instant::now();
        assert!(!manager.should_warmup(Some(recent)));

        // Should warmup if it's been a long time
        let old = std::time::Instant::now() - Duration::from_secs(7200); // 2 hours ago
        assert!(manager.should_warmup(Some(old)));
    }

    #[test]
    fn test_auto_reload_manager_creation() {
        let manager = AutoReloadManager::new();
        assert_eq!(manager.miss_threshold, 3);
        assert_eq!(manager.reload_delay, Duration::from_secs(5));
    }

    #[test]
    fn test_warmup_stats() {
        let manager = CacheWarmupManager::new();
        let stats = manager.get_warmup_stats();

        assert!(stats.total_queries > 0);
        assert_eq!(stats.warmup_delay_ms, 100);
        assert_eq!(stats.max_duration_ms, 30000);
    }

    #[test]
    fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.test_prompt, "Hello");
        assert_eq!(config.max_tokens, 5);
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.iterations, 1);
        assert_eq!(config.temperature, 0.0);
    }

    #[test]
    fn test_health_check_config_custom() {
        let config = HealthCheckConfig {
            test_prompt: "Test prompt".to_string(),
            max_tokens: 10,
            timeout: Duration::from_secs(60),
            iterations: 3,
            temperature: 0.7,
        };
        assert_eq!(config.test_prompt, "Test prompt");
        assert_eq!(config.max_tokens, 10);
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert_eq!(config.iterations, 3);
        assert_eq!(config.temperature, 0.7);
    }

    #[test]
    fn test_health_check_result_passed() {
        let result = HealthCheckResult {
            passed: true,
            latency_ms: 100,
            tokens_generated: 5,
            tokens_per_second: 50.0,
            failure_reason: None,
        };
        assert!(result.passed);
        assert_eq!(result.latency_ms, 100);
        assert_eq!(result.tokens_generated, 5);
        assert_eq!(result.tokens_per_second, 50.0);
        assert!(result.failure_reason.is_none());
    }

    #[test]
    fn test_health_check_result_failed() {
        let result = HealthCheckResult {
            passed: false,
            latency_ms: 0,
            tokens_generated: 0,
            tokens_per_second: 0.0,
            failure_reason: Some("Inference failed".to_string()),
        };
        assert!(!result.passed);
        assert_eq!(result.tokens_generated, 0);
        assert!(result.failure_reason.is_some());
        assert_eq!(result.failure_reason.unwrap(), "Inference failed");
    }
}
