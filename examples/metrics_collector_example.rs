//! Example demonstrating AdapterOS telemetry metrics collector
//!
//! This example shows how to:
//! - Create a metrics collector
//! - Record various metrics (latency, queue depth, tokens/sec)
//! - Export metrics via Prometheus and JSON endpoints
//! - Start a metrics server

use adapteros_telemetry::{MetricsCollector, MetricsServer};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting AdapterOS metrics collector example");

    // Create metrics collector
    let collector = Arc::new(MetricsCollector::new()?);
    println!("Created metrics collector");

    // Start metrics server in background
    let server_port = 9090;
    let server = MetricsServer::new(collector.clone(), server_port);
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            println!("Metrics server error: {}", e);
        }
    });

    println!("Started metrics server on port {}", server_port);

    // Simulate metrics collection
    let collector_clone = collector.clone();
    let metrics_task = tokio::spawn(async move {
        let mut counter = 0;
        loop {
            counter += 1;

            // Simulate inference latency
            let latency_ms = 25.0 + (counter % 50) as f64; // 25-75ms
            collector_clone.record_inference_latency("tenant1", "adapter1", latency_ms / 1000.0);

            // Simulate router latency
            let router_latency_ms = 5.0 + (counter % 10) as f64; // 5-15ms
            collector_clone.record_router_latency("tenant1", router_latency_ms / 1000.0);

            // Simulate kernel latency
            let kernel_latency_ms = 10.0 + (counter % 20) as f64; // 10-30ms
            collector_clone.record_kernel_latency("attention", "tenant1", kernel_latency_ms / 1000.0);

            // Simulate queue depth
            let queue_depth = (counter % 20) as f64;
            collector_clone.update_queue_depth("request", "tenant1", queue_depth);
            collector_clone.update_adapter_queue_depth("adapter1", "tenant1", queue_depth / 2.0);

            // Simulate token generation
            let tokens = 50 + (counter % 100) as u64; // 50-150 tokens
            collector_clone.record_tokens_generated("tenant1", "adapter1", tokens);

            // Simulate tokens per second
            let tps = 40.0 + (counter % 20) as f64; // 40-60 tps
            collector_clone.update_tokens_per_second("tenant1", tps);

            // Simulate active sessions
            let sessions = 5.0 + (counter % 10) as f64; // 5-15 sessions
            collector_clone.update_active_sessions(sessions);

            // Simulate memory usage
            let memory_mb = 1024.0 + (counter % 512) as f64; // 1-1.5GB
            collector_clone.update_memory_usage("worker", "tenant1", memory_mb * 1_048_576.0);

            // Simulate policy violations
            if counter % 100 == 0 {
                collector_clone.record_policy_violation("egress", "attempt");
            }

            // Simulate abstain events
            if counter % 50 == 0 {
                collector_clone.record_abstain_event("low_confidence", "tenant1");
            }

            // Simulate adapter activations/evictions
            if counter % 200 == 0 {
                collector_clone.record_adapter_activation("adapter2", "tenant1");
            }
            if counter % 300 == 0 {
                collector_clone.record_adapter_eviction("adapter1", "tenant1", "memory");
            }

            // Update metrics cache
            if let Err(e) = collector_clone.update_cache().await {
                println!("Failed to update metrics cache: {}", e);
            }

            if counter % 100 == 0 {
                println!("Recorded {} metric samples", counter);
            }

            sleep(Duration::from_millis(100)).await;
        }
    });

    println!("Metrics collection started");
    println!("Prometheus endpoint: http://localhost:{}/metrics", server_port);
    println!("JSON endpoint: http://localhost:{}/metrics/json", server_port);
    println!("Health endpoint: http://localhost:{}/health", server_port);

    // Wait for either task to complete
    tokio::select! {
        _ = server_handle => {
            println!("Metrics server stopped");
        }
        _ = metrics_task => {
            println!("Metrics collection stopped");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector_example() {
        let collector = MetricsCollector::new().expect("Should create metrics collector");
        
        // Test basic metrics recording
        collector.record_inference_latency("test_tenant", "test_adapter", 0.025);
        collector.update_queue_depth("request", "test_tenant", 5.0);
        collector.record_tokens_generated("test_tenant", "test_adapter", 100);
        
        // Test snapshot generation
        let snapshot = collector.get_metrics_snapshot().await;
        assert!(snapshot.timestamp > 0);
        
        // Test Prometheus rendering
        let prometheus_output = collector.render_prometheus().expect("Should render Prometheus metrics");
        let output_str = String::from_utf8(prometheus_output).expect("Should be valid UTF-8");
        assert!(output_str.contains("adapteros_inference_latency_seconds"));
        assert!(output_str.contains("adapteros_queue_depth"));
        assert!(output_str.contains("adapteros_tokens_generated_total"));
    }
}
