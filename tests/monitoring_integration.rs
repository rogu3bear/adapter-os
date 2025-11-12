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
    let output = collector
        .render_prometheus()
        .expect("Failed to render metrics");
    let output_str = String::from_utf8(output).expect("Invalid UTF-8");

    // Check that it contains expected metrics
    assert!(output_str.contains("adapteros_inference_latency_seconds"));
    assert!(output_str.contains("adapteros_queue_depth"));

    println!("Prometheus output sample:\n{}", &output_str[..500]);
}

#[tokio::test]
async fn test_health_cache_functionality() {
    // Test that the health cache works properly

    // Create a minimal mock state (this would be complex in real integration test)
    // For now, just test that the function signature works and cache is accessible

    // The cache should be accessible via the lazy static
    // In a real test, we'd need to set up a full AppState with database
    println!("Health cache functionality test placeholder - requires full AppState setup");
}

#[cfg(feature = "integration_tests")]
mod integration_tests {
    use super::*;
    use adapteros_server_api::routes;
    use adapteros_server_api::state::{AppState, ApiConfig, MetricsConfig};
    use axum_test::TestServer;
    use sqlx::SqlitePool;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    async fn setup_test_server() -> TestServer {
        // Create test database
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

        // Run migrations
        sqlx::migrate!("../migrations").run(&pool).await.unwrap();

        // Create minimal config
        let config = Arc::new(RwLock::new(ApiConfig {
            metrics: MetricsConfig {
                enabled: true,
                bearer_token: "test_token_12345".to_string(),
                system_metrics_interval_secs: 30,
                telemetry_buffer_capacity: 1024,
                telemetry_channel_capacity: 256,
                trace_buffer_capacity: 512,
                server_port: 9090,
                server_enabled: true,
            },
            golden_gate: None,
            bundles_root: "/tmp".to_string(),
            rate_limits: None,
            path_policy: Default::default(),
            production_mode: false,
            model_load_timeout_secs: 300,
            model_unload_timeout_secs: 30,
            operation_retry: Default::default(),
        }));

        // Create minimal AppState
        let state = AppState::new(
            adapteros_db::Database::Sqlite(pool),
            b"test_jwt_secret_32_chars_long_______".to_vec(),
            config,
            Arc::new(
                adapteros_metrics_exporter::MetricsExporter::new(vec![0.1, 0.5, 1.0]).unwrap(),
            ),
            Arc::new(MetricsCollector::new().unwrap()),
            Arc::new(adapteros_telemetry::MetricsRegistry::new(Arc::new(
                MetricsCollector::new().unwrap(),
            ))),
            Arc::new(adapteros_orchestrator::TrainingService::new()),
            None,
        );

        // Create test server
        let app = routes::build(state);
        TestServer::new(app).unwrap()
    }

    #[tokio::test]
    async fn test_health_endpoint_returns_model_info() {
        let server = setup_test_server().await;

        let response = server.get("/healthz").await;

        assert_eq!(response.status_code(), 200);

        let health: HealthResponse = response.json();
        assert_eq!(health.status, "healthy");
        assert!(health.version.len() > 0);

        // Model health should be present (may be None if no runtime)
        // This tests the basic structure
    }

    #[tokio::test]
    async fn test_metrics_endpoint_requires_auth() {
        let server = setup_test_server().await;

        // Test without auth - should fail
        let response = server.get("/metrics").await;
        assert_eq!(response.status_code(), 401);

        // Test with wrong auth - should fail
        let response = server
            .get("/metrics")
            .authorization_bearer("wrong_token")
            .await;
        assert_eq!(response.status_code(), 401);

        // Test with correct auth - should succeed
        let response = server
            .get("/metrics")
            .authorization_bearer("test_token_12345")
            .await;
        assert_eq!(response.status_code(), 200);

        // Check content type
        let content_type = response.headers().get("content-type").unwrap();
        assert!(content_type.to_str().unwrap().contains("text/plain"));
    }

    #[tokio::test]
    async fn test_metrics_endpoint_returns_prometheus_format() {
        let server = setup_test_server().await;

        let response = server
            .get("/metrics")
            .authorization_bearer("test_token_12345")
            .await;

        let body = response.text();
        assert!(body.contains("# TYPE")); // Prometheus format indicator
        assert!(body.contains("adapteros_")); // Our metric prefix
    }
}
