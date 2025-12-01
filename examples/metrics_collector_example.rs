//! Example demonstrating AdapterOS telemetry metrics collector
//!
//! This example shows how to:
//! - Create a metrics collector
//! - Record various metrics using the increment API
//! - Query counter values
//!
//! Note: The MetricsCollector provides a simple counter-based interface.
//! For Prometheus-style metrics with histograms and gauges, see the
//! CriticalComponentMetrics in adapteros_telemetry::metrics::critical_components.

use adapteros_telemetry::MetricsCollector;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting AdapterOS metrics collector example");

    // Create metrics collector wrapped in Mutex for interior mutability
    // (increment requires &mut self)
    let collector = Arc::new(Mutex::new(MetricsCollector::new(
        adapteros_telemetry::metrics::MetricsConfig::default(),
    )));
    println!("Created metrics collector");

    // Simulate metrics collection
    let collector_clone = collector.clone();
    let metrics_task = tokio::spawn(async move {
        let mut counter = 0u64;
        loop {
            counter += 1;

            // Lock collector for updates
            {
                let mut coll = collector_clone.lock().unwrap();

                // Record inference count
                coll.increment("inference_count", 1);

                // Record tokens generated
                let tokens = 50 + (counter % 100);
                coll.increment("tokens_generated_total", tokens);

                // Record latency samples (as microseconds for integer storage)
                let latency_us = (25.0 + (counter % 50) as f64) * 1000.0;
                coll.increment("inference_latency_us_total", latency_us as u64);

                // Record queue depth samples
                let queue_depth = counter % 20;
                coll.increment("queue_depth_samples", queue_depth);

                // Record policy violations
                if counter % 100 == 0 {
                    coll.increment("policy_violations", 1);
                }

                // Record adapter activations
                if counter % 200 == 0 {
                    coll.increment("adapter_activations", 1);
                }

                // Record adapter evictions
                if counter % 300 == 0 {
                    coll.increment("adapter_evictions", 1);
                }
            }

            if counter % 100 == 0 {
                let coll = collector_clone.lock().unwrap();
                println!(
                    "Recorded {} iterations - inferences: {:?}, tokens: {:?}",
                    counter,
                    coll.get("inference_count"),
                    coll.get("tokens_generated_total")
                );
            }

            sleep(Duration::from_millis(100)).await;
        }
    });

    println!("Metrics collection started");
    println!("Collecting metrics for 10 seconds...");

    // Wait for metrics task
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(10)) => {
            println!("Collection period complete");
        }
        _ = metrics_task => {
            println!("Metrics collection stopped");
        }
    }

    // Print final metrics
    {
        let coll = collector.lock().unwrap();
        println!("\nFinal metrics:");
        println!("  inference_count: {:?}", coll.get("inference_count"));
        println!(
            "  tokens_generated_total: {:?}",
            coll.get("tokens_generated_total")
        );
        println!(
            "  inference_latency_us_total: {:?}",
            coll.get("inference_latency_us_total")
        );
        println!("  policy_violations: {:?}", coll.get("policy_violations"));
        println!(
            "  adapter_activations: {:?}",
            coll.get("adapter_activations")
        );
        println!("  adapter_evictions: {:?}", coll.get("adapter_evictions"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_example() {
        let mut collector =
            MetricsCollector::new(adapteros_telemetry::metrics::MetricsConfig::default());

        // Test basic metrics recording
        collector.increment("inference_count", 1);
        collector.increment("inference_count", 1);
        collector.increment("tokens_generated", 100);

        // Verify counter values
        assert_eq!(collector.get("inference_count"), Some(2));
        assert_eq!(collector.get("tokens_generated"), Some(100));
        assert_eq!(collector.get("nonexistent"), None);
    }
}
