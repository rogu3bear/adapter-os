use adapteros_api_types::{HealthResponse, ModelRuntimeHealth};
use adapteros_telemetry::MetricsCollector;

#[tokio::test]
async fn test_health_response_with_models() {
    // Test that HealthResponse can be created with model health info
    let response = HealthResponse {
        status: "healthy".to_string(),
        version: "1.0.0".to_string(),
        models: Some(ModelRuntimeHealth {
            total_models: 5,
            loaded_count: 3,
            healthy: true,
            inconsistencies_count: 0,
        }),
    };
    
    assert_eq!(response.status, "healthy");
    assert_eq!(response.version, "1.0.0");
    assert!(response.models.is_some());
    let models = response.models.unwrap();
    assert_eq!(models.total_models, 5);
    assert_eq!(models.loaded_count, 3);
    assert!(models.healthy);
    assert_eq!(models.inconsistencies_count, 0);
}

#[tokio::test]
async fn test_metrics_collector_prometheus_output() {
    // Test that MetricsCollector can render Prometheus metrics
    let collector = MetricsCollector::new().expect("Failed to create metrics collector");
    
    // Add some test metrics
    collector.record_inference_latency("tenant1", "adapter1", 0.1);
    collector.update_queue_depth("request", "tenant1", 5.0);
    
    // Render to Prometheus format
    let output = collector.render_prometheus().expect("Failed to render metrics");
    let output_str = String::from_utf8(output).expect("Invalid UTF-8");
    
    // Check that it contains expected metrics
    assert!(output_str.contains("adapteros_inference_latency_seconds"));
    assert!(output_str.contains("adapteros_queue_depth"));
    
    println!("Prometheus output sample:\n{}", &output_str[..500]);
}
