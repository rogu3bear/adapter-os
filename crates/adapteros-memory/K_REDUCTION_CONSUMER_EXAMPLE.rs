// Example: Lifecycle Manager consuming K reduction requests
//
// This example demonstrates how the lifecycle manager (or any consumer)
// would integrate with the K reduction channel to process memory pressure
// requests from the memory manager.

#![allow(dead_code)]

use adapteros_memory::{
    KReductionChannelManager, KReductionRequestReceiver, KReductionRequest,
};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Simulated lifecycle manager context
pub struct LifecycleContext {
    /// Current K value (number of active adapters)
    current_k: usize,
    /// Minimum K value (never go below)
    min_k: usize,
    /// Adapter names for demonstration
    active_adapters: Vec<String>,
}

impl LifecycleContext {
    fn new(current_k: usize) -> Self {
        let mut active_adapters = Vec::new();
        for i in 0..current_k {
            active_adapters.push(format!("adapter-{}", i));
        }

        Self {
            current_k,
            min_k: 1,
            active_adapters,
        }
    }

    /// Evaluate if K reduction is feasible
    fn can_reduce_to(&self, target_k: usize) -> bool {
        target_k >= self.min_k && target_k < self.current_k
    }

    /// Perform actual K reduction
    async fn execute_k_reduction(&mut self, request: &KReductionRequest) -> Result<u64, String> {
        if !self.can_reduce_to(request.target_k) {
            return Err(format!(
                "Cannot reduce from {} to {} (min: {})",
                self.current_k, request.target_k, self.min_k
            ));
        }

        info!(
            current_k = self.current_k,
            target_k = request.target_k,
            "Starting K reduction"
        );

        // Simulate unloading adapters
        let mut freed_bytes = 0u64;
        let adapters_to_unload = self.current_k - request.target_k;

        for _ in 0..adapters_to_unload {
            if let Some(adapter) = self.active_adapters.pop() {
                // Simulate unloading and memory reclamation
                let memory_freed = 1024 * 1024; // 1MB per adapter
                freed_bytes += memory_freed;

                info!(
                    adapter_name = %adapter,
                    memory_freed_bytes = memory_freed,
                    "Unloaded adapter"
                );

                // Simulate async unload operation
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        }

        self.current_k = request.target_k;

        info!(
            new_k = self.current_k,
            total_freed = freed_bytes,
            "K reduction completed"
        );

        Ok(freed_bytes)
    }
}

/// Configuration for K reduction consumer
pub struct KReductionConsumerConfig {
    /// Enable automatic execution of approved reductions
    pub auto_execute: bool,
    /// Maximum time to wait for processing (ms)
    pub processing_timeout_ms: u64,
    /// Log level (0=error, 1=warn, 2=info, 3=debug)
    pub log_level: u32,
}

impl Default for KReductionConsumerConfig {
    fn default() -> Self {
        Self {
            auto_execute: true,
            processing_timeout_ms: 5000,
            log_level: 2,
        }
    }
}

/// K reduction consumer task
pub struct KReductionConsumer {
    config: KReductionConsumerConfig,
    context: Arc<tokio::sync::Mutex<LifecycleContext>>,
}

impl KReductionConsumer {
    pub fn new(config: KReductionConsumerConfig, initial_k: usize) -> Self {
        Self {
            config,
            context: Arc::new(tokio::sync::Mutex::new(LifecycleContext::new(initial_k))),
        }
    }

    /// Start the consumer task
    pub fn start(
        self,
        mut receiver: KReductionRequestReceiver,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            info!("K reduction consumer started");

            let mut request_count = 0u64;
            let mut successful_reductions = 0u64;
            let mut failed_reductions = 0u64;

            loop {
                // Wait for next K reduction request
                match receiver.recv().await {
                    Some(request) => {
                        request_count += 1;

                        debug!(
                            request_id = %request.request_id,
                            target_k = request.target_k,
                            pressure_level = request.pressure_level,
                            "Received K reduction request"
                        );

                        // Process the request
                        match self.process_request(&request).await {
                            Ok((approved, freed)) => {
                                receiver.record_decision_outcome(approved);

                                if approved {
                                    successful_reductions += 1;
                                    info!(
                                        request_id = %request.request_id,
                                        freed_bytes = freed,
                                        "K reduction completed successfully"
                                    );
                                } else {
                                    failed_reductions += 1;
                                    warn!(
                                        request_id = %request.request_id,
                                        "K reduction rejected"
                                    );
                                }
                            }
                            Err(e) => {
                                failed_reductions += 1;
                                receiver.record_decision_outcome(false);
                                error!(
                                    request_id = %request.request_id,
                                    error = %e,
                                    "K reduction failed"
                                );
                            }
                        }
                    }
                    None => {
                        info!(
                            total_requests = request_count,
                            successful = successful_reductions,
                            failed = failed_reductions,
                            "K reduction channel closed, consumer stopping"
                        );
                        break;
                    }
                }
            }
        })
    }

    /// Process a single K reduction request
    async fn process_request(
        &self,
        request: &KReductionRequest,
    ) -> Result<(bool, u64), String> {
        let mut ctx = self.context.lock().await;

        // Validate request
        if !request.is_valid() {
            return Err("Invalid K reduction request".to_string());
        }

        // Check if reduction is feasible
        if !ctx.can_reduce_to(request.target_k) {
            return Ok((false, 0));
        }

        // Only execute if auto_execute is enabled
        if !self.config.auto_execute {
            debug!(
                request_id = %request.request_id,
                "Auto-execute disabled, skipping"
            );
            return Ok((false, 0));
        }

        // Execute the K reduction
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(self.config.processing_timeout_ms),
            ctx.execute_k_reduction(request),
        )
        .await
        {
            Ok(Ok(freed)) => Ok((true, freed)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("K reduction timed out".to_string()),
        }
    }

    /// Get current K value
    pub async fn get_current_k(&self) -> usize {
        self.context.lock().await.current_k
    }

    /// Get active adapters
    pub async fn get_active_adapters(&self) -> Vec<String> {
        self.context.lock().await.active_adapters.clone()
    }
}

// Example usage in a test
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn example_k_reduction_consumer() {
        // Setup tracing for this example
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        // Create channel
        let channel_manager = KReductionChannelManager::new();
        let (tx, rx) = channel_manager.create_channel();

        // Create consumer with initial K=8
        let consumer = KReductionConsumer::new(
            KReductionConsumerConfig {
                auto_execute: true,
                ..Default::default()
            },
            8,
        );

        // Start consumer task
        let consumer_task = consumer.start(rx);

        // Simulate sending K reduction requests
        for i in 0..3 {
            let request = KReductionRequest::new(
                6, // target K
                8, // current K
                0.85,
                1024 * 1024 * 2,
                10.0,
                format!("Memory pressure spike {}", i),
            );

            tx.send(request).await.expect("Failed to send request");

            // Give consumer time to process
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // Check final state
        let final_k = consumer.get_current_k().await;
        println!("Final K value: {}", final_k);
        assert_eq!(final_k, 6);

        // Close channel
        drop(tx);

        // Wait for consumer to finish
        consumer_task.await.expect("Consumer task failed");

        println!("K reduction consumer example completed");
    }

    #[tokio::test]
    async fn example_with_rejection() {
        // Create channel
        let channel_manager = KReductionChannelManager::new();
        let (tx, rx) = channel_manager.create_channel();

        // Create consumer that rejects reductions
        let consumer = KReductionConsumer::new(
            KReductionConsumerConfig {
                auto_execute: false, // Reject all
                ..Default::default()
            },
            8,
        );

        // Start consumer task
        let consumer_task = consumer.start(rx);

        // Send request
        let request = KReductionRequest::new(6, 8, 0.85, 1024 * 1024 * 2, 10.0, "Test".to_string());
        tx.send(request).await.expect("Failed to send request");

        // Give consumer time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // K should remain unchanged
        let final_k = consumer.get_current_k().await;
        assert_eq!(final_k, 8);

        // Close channel
        drop(tx);
        consumer_task.await.expect("Consumer task failed");

        println!("Rejection example completed");
    }
}

/// Example of using the consumer in a real scenario
pub async fn example_full_integration() {
    // Create channel
    let channel_manager = KReductionChannelManager::new();
    let (tx, rx) = channel_manager.create_channel();

    // Create consumer
    let consumer = KReductionConsumer::new(KReductionConsumerConfig::default(), 8);

    // Start consumer in background
    let consumer_handle = consumer.start(rx);

    // Simulate memory manager sending requests
    for i in 0..5 {
        let request = KReductionRequest::new(
            8 - (i as usize),  // Gradually reduce K
            8,
            0.25 + (i as f32 * 0.15), // Increasing pressure
            1024 * 1024 * (i as u64 + 1),
            (85.0 - (i as f32 * 10.0)).max(15.0),
            format!("Pressure spike {}", i),
        );

        println!("Sending request: K {} -> {}", 8, request.target_k);

        if let Err(e) = tx.send(request).await {
            eprintln!("Failed to send request: {:?}", e);
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Check stats
    let stats = channel_manager.get_stats();
    println!("Final stats:");
    println!("  Requests sent: {}", stats.total_requests_sent);
    println!("  Requests received: {}", stats.total_requests_received);
    println!("  Approved: {}", stats.total_approved);
    println!("  Rejected: {}", stats.total_rejected);

    // Close channel
    drop(tx);

    // Wait for consumer to finish
    consumer_handle.await.expect("Consumer failed");

    println!("Full integration example completed");
}
