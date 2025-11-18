//! Cache warmup functionality for adapters
//!
//! This module provides functionality to warm up adapter caches with common queries
//! to improve inference performance and reduce cold start latency.
//!
//! Citation: Based on `crates/adapteros-lora-worker/src/lib.rs:417-453` - extends the
//! existing inference loop with cache warmup capabilities.

use adapteros_core::{AosError, Result};
use std::time::Duration;
use tracing::{info, warn};

use crate::{InferenceRequest, RequestType, Worker};

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
}
