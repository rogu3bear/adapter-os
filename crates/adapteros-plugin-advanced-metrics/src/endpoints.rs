//! Custom Prometheus Endpoint
//!
//! Provides a custom HTTP endpoint for exposing advanced metrics
//! in Prometheus text format.

use prometheus::{Encoder, TextEncoder};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error};

use crate::MetricsCollector;

/// Metrics endpoint response
pub struct MetricsResponse {
    pub content_type: String,
    pub body: String,
}

/// Generate Prometheus metrics endpoint response
///
/// Returns metrics in Prometheus text format with proper content type.
///
/// # Arguments
///
/// * `collector` - Reference to the metrics collector
///
/// # Returns
///
/// * `Ok(MetricsResponse)` - Metrics in Prometheus format
/// * `Err(String)` - Error message if metrics encoding fails
pub async fn metrics_endpoint(
    collector: Arc<RwLock<MetricsCollector>>,
) -> Result<MetricsResponse, String> {
    debug!("Generating Prometheus metrics endpoint response");

    // Lock collector for reading (prevents concurrent modification during encoding)
    let _collector = collector.read().await;

    // Encode all registered Prometheus metrics
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();

    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).map_err(|e| {
        error!(error = %e, "Failed to encode Prometheus metrics");
        format!("Failed to encode metrics: {}", e)
    })?;

    let body = String::from_utf8(buffer).map_err(|e| {
        error!(error = %e, "Failed to convert metrics to UTF-8");
        format!("Failed to convert metrics to UTF-8: {}", e)
    })?;

    debug!(
        metrics_size = body.len(),
        "Generated Prometheus metrics response"
    );

    Ok(MetricsResponse {
        content_type: encoder.format_type().to_string(),
        body,
    })
}

/// Helper function to extract metrics for JSON response
///
/// This can be used for non-Prometheus consumers that prefer JSON.
pub async fn metrics_json(
    collector: Arc<RwLock<MetricsCollector>>,
) -> Result<serde_json::Value, String> {
    let collector = collector.read().await;
    let stats = collector.get_stats();
    let adapter_metrics = collector.get_adapter_metrics();

    let mut adapters = Vec::new();
    for (adapter_id, activation_count, avg_latency) in adapter_metrics {
        adapters.push(serde_json::json!({
            "adapter_id": adapter_id,
            "activation_count": activation_count,
            "avg_latency_ms": avg_latency,
        }));
    }

    Ok(serde_json::json!({
        "stats": {
            "tracked_adapters": stats.tracked_adapters,
            "inference_events": stats.inference_events,
            "training_events": stats.training_events,
            "metrics_ticks": stats.metrics_ticks,
        },
        "adapters": adapters,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetricsCollector;
    use adapteros_core::plugin_events::InferenceEvent;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let mut collector = MetricsCollector::new();
        collector.initialize().unwrap();

        // Add some test data
        let event = InferenceEvent {
            request_id: "req-1".to_string(),
            adapter_ids: vec!["test-adapter".to_string()],
            stack_id: None,
            prompt: None,
            output: None,
            latency_ms: 123.45,
            tokens_generated: Some(50),
            tokens_per_sec: Some(100.0),
            tenant_id: Some("test-tenant".to_string()),
            model: None,
            streaming: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };

        collector.record_inference_complete(&event).unwrap();

        let collector_arc = Arc::new(RwLock::new(collector));
        let response = metrics_endpoint(collector_arc).await.unwrap();

        assert_eq!(response.content_type, "text/plain; version=0.0.4");
        assert!(!response.body.is_empty());
        assert!(response.body.contains("adapteros_inference_latency_ms"));
    }

    #[tokio::test]
    async fn test_metrics_json() {
        let mut collector = MetricsCollector::new();
        collector.initialize().unwrap();

        // Add some test data
        let event = InferenceEvent {
            request_id: "req-1".to_string(),
            adapter_ids: vec!["test-adapter".to_string()],
            stack_id: None,
            prompt: None,
            output: None,
            latency_ms: 100.0,
            tokens_generated: Some(50),
            tokens_per_sec: Some(100.0),
            tenant_id: Some("test-tenant".to_string()),
            model: None,
            streaming: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };

        collector.record_inference_complete(&event).unwrap();

        let collector_arc = Arc::new(RwLock::new(collector));
        let json = metrics_json(collector_arc).await.unwrap();

        assert!(json.get("stats").is_some());
        assert!(json.get("adapters").is_some());

        let stats = json.get("stats").unwrap();
        assert_eq!(stats.get("inference_events").unwrap().as_u64(), Some(1));
        assert_eq!(stats.get("tracked_adapters").unwrap().as_u64(), Some(1));

        let adapters = json.get("adapters").unwrap().as_array().unwrap();
        assert_eq!(adapters.len(), 1);
        assert_eq!(
            adapters[0].get("adapter_id").unwrap().as_str(),
            Some("test-adapter")
        );
        assert_eq!(
            adapters[0].get("activation_count").unwrap().as_u64(),
            Some(1)
        );
    }

    #[tokio::test]
    async fn test_empty_metrics_endpoint() {
        let mut collector = MetricsCollector::new();
        collector.initialize().unwrap();

        let collector_arc = Arc::new(RwLock::new(collector));
        let response = metrics_endpoint(collector_arc).await.unwrap();

        assert_eq!(response.content_type, "text/plain; version=0.0.4");
        assert!(!response.body.is_empty());
    }
}
