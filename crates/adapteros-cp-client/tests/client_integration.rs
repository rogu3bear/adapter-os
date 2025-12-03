//! Integration tests for ControlPlaneClient using wiremock
//!
//! These tests verify actual HTTP behavior by mocking the control plane server.

use std::time::Duration;

use adapteros_api_types::workers::{
    WorkerFatalRequest, WorkerFatalResponse, WorkerHeartbeatRequest, WorkerHeartbeatResponse,
    WorkerRegistrationRequest, WorkerRegistrationResponse, WorkerStatusNotification,
    WorkerStatusResponse,
};
use adapteros_cp_client::{ClientConfig, ControlPlaneClient};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// =============================================================================
// Registration Tests
// =============================================================================

#[tokio::test]
async fn test_register_success() {
    let mock_server = MockServer::start().await;

    let response = WorkerRegistrationResponse {
        accepted: true,
        worker_id: "worker-123".to_string(),
        rejection_reason: None,
        heartbeat_interval_secs: 30,
    };

    Mock::given(method("POST"))
        .and(path("/v1/workers/register"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = ClientConfig::builder()
        .base_url(&mock_server.uri())
        .max_retries(1)
        .build()
        .unwrap();

    let client = ControlPlaneClient::new(config).unwrap();

    let req = WorkerRegistrationRequest {
        worker_id: "worker-123".to_string(),
        tenant_id: "tenant-abc".to_string(),
        plan_id: "plan-xyz".to_string(),
        manifest_hash: "abc123".to_string(),
        schema_version: "1.0".to_string(),
        api_version: "1.0".to_string(),
        pid: 12345,
        uds_path: "/var/run/aos/worker.sock".to_string(),
        capabilities: vec!["metal".to_string()],
    };

    let result = client.register(req).await;
    assert!(result.is_ok());

    let resp = result.unwrap();
    assert!(resp.accepted);
    assert_eq!(resp.worker_id, "worker-123");
    assert_eq!(resp.heartbeat_interval_secs, 30);
}

#[tokio::test]
async fn test_register_rejected() {
    let mock_server = MockServer::start().await;

    let response = WorkerRegistrationResponse {
        accepted: false,
        worker_id: "worker-123".to_string(),
        rejection_reason: Some("Manifest hash mismatch".to_string()),
        heartbeat_interval_secs: 30,
    };

    Mock::given(method("POST"))
        .and(path("/v1/workers/register"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = ClientConfig::builder()
        .base_url(&mock_server.uri())
        .max_retries(1)
        .build()
        .unwrap();

    let client = ControlPlaneClient::new(config).unwrap();

    let req = WorkerRegistrationRequest {
        worker_id: "worker-123".to_string(),
        tenant_id: "tenant-abc".to_string(),
        plan_id: "plan-xyz".to_string(),
        manifest_hash: "wrong-hash".to_string(),
        schema_version: "1.0".to_string(),
        api_version: "1.0".to_string(),
        pid: 12345,
        uds_path: "/var/run/aos/worker.sock".to_string(),
        capabilities: vec![],
    };

    let result = client.register(req).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    // Error format is "Registration rejected: {reason}" (lowercase 'r')
    assert!(
        err.to_string().contains("rejected") || err.to_string().contains("Rejected"),
        "Expected rejection error, got: {}",
        err
    );
}

#[tokio::test]
async fn test_register_server_error_is_retryable() {
    // Test that 503 errors are marked as retryable
    // (Actual retry behavior is tested in retry module unit tests)
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/workers/register"))
        .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::builder()
        .base_url(&mock_server.uri())
        .max_retries(0) // No retries - just test single request
        .build()
        .unwrap();

    let client = ControlPlaneClient::new(config).unwrap();

    let req = WorkerRegistrationRequest {
        worker_id: "worker-123".to_string(),
        tenant_id: "tenant-abc".to_string(),
        plan_id: "plan-xyz".to_string(),
        manifest_hash: "abc123".to_string(),
        schema_version: "1.0".to_string(),
        api_version: "1.0".to_string(),
        pid: 12345,
        uds_path: "/var/run/aos/worker.sock".to_string(),
        capabilities: vec![],
    };

    let result = client.register(req).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    // 503 should be a retryable server error
    assert!(
        err.is_retryable(),
        "503 error should be retryable, got: {}",
        err
    );
}

// =============================================================================
// Status Notification Tests
// =============================================================================

#[tokio::test]
async fn test_notify_status_success() {
    let mock_server = MockServer::start().await;

    let response = WorkerStatusResponse {
        success: true,
        worker_id: "worker-123".to_string(),
        status: "serving".to_string(),
    };

    Mock::given(method("POST"))
        .and(path("/v1/workers/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = ClientConfig::builder()
        .base_url(&mock_server.uri())
        .build()
        .unwrap();

    let client = ControlPlaneClient::new(config).unwrap();

    let req = WorkerStatusNotification {
        worker_id: "worker-123".to_string(),
        status: "serving".to_string(),
        reason: "Model loaded successfully".to_string(),
    };

    let result = client.notify_status(req).await;
    assert!(result.is_ok());

    let resp = result.unwrap();
    assert!(resp.success);
    assert_eq!(resp.status, "serving");
}

// =============================================================================
// Heartbeat Tests
// =============================================================================

#[tokio::test]
async fn test_send_heartbeat_success() {
    let mock_server = MockServer::start().await;

    let response = WorkerHeartbeatResponse {
        acknowledged: true,
        next_heartbeat_secs: 60, // CP requests longer interval
    };

    Mock::given(method("POST"))
        .and(path("/v1/workers/heartbeat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = ClientConfig::builder()
        .base_url(&mock_server.uri())
        .build()
        .unwrap();

    let client = ControlPlaneClient::new(config).unwrap();

    let req = WorkerHeartbeatRequest {
        worker_id: "worker-123".to_string(),
        status: "serving".to_string(),
        memory_usage_pct: Some(45.5),
        adapters_loaded: Some(3),
        timestamp: "2024-01-15T10:30:00Z".to_string(),
    };

    let result = client.send_heartbeat(req).await;
    assert!(result.is_ok());

    let resp = result.unwrap();
    assert!(resp.acknowledged);
    assert_eq!(resp.next_heartbeat_secs, 60);
}

#[tokio::test]
async fn test_heartbeat_worker_not_found() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/workers/heartbeat"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(&serde_json::json!({
                "error": "Worker not found",
                "code": "NOT_FOUND"
            })),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = ClientConfig::builder()
        .base_url(&mock_server.uri())
        .build()
        .unwrap();

    let client = ControlPlaneClient::new(config).unwrap();

    let req = WorkerHeartbeatRequest {
        worker_id: "unknown-worker".to_string(),
        status: "serving".to_string(),
        memory_usage_pct: None,
        adapters_loaded: None,
        timestamp: "2024-01-15T10:30:00Z".to_string(),
    };

    let result = client.send_heartbeat(req).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    // Should be a client error (4xx)
    assert!(err.to_string().contains("404") || err.to_string().contains("Client"));
}

// =============================================================================
// Fatal Error Tests
// =============================================================================

#[tokio::test]
async fn test_report_fatal_success() {
    let mock_server = MockServer::start().await;

    let response = WorkerFatalResponse {
        recorded: true,
        incident_id: "incident-abc123".to_string(),
    };

    Mock::given(method("POST"))
        .and(path("/v1/workers/fatal"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = ClientConfig::builder()
        .base_url(&mock_server.uri())
        .build()
        .unwrap();

    let client = ControlPlaneClient::new(config).unwrap();

    let req = WorkerFatalRequest {
        worker_id: "worker-123".to_string(),
        reason: "PANIC: index out of bounds".to_string(),
        backtrace_snippet: Some("at src/main.rs:42".to_string()),
        timestamp: "2024-01-15T10:30:00Z".to_string(),
    };

    let result = client.report_fatal(req).await;
    assert!(result.is_ok());

    let resp = result.unwrap();
    assert!(resp.recorded);
    assert_eq!(resp.incident_id, "incident-abc123");
}

// =============================================================================
// Auth Token Tests
// =============================================================================

#[tokio::test]
async fn test_auth_token_sent_in_header() {
    let mock_server = MockServer::start().await;

    let response = WorkerHeartbeatResponse {
        acknowledged: true,
        next_heartbeat_secs: 30,
    };

    Mock::given(method("POST"))
        .and(path("/v1/workers/heartbeat"))
        .and(header("Authorization", "Bearer secret-token-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = ClientConfig::builder()
        .base_url(&mock_server.uri())
        .auth_token("secret-token-123")
        .build()
        .unwrap();

    let client = ControlPlaneClient::new(config).unwrap();

    let req = WorkerHeartbeatRequest {
        worker_id: "worker-123".to_string(),
        status: "serving".to_string(),
        memory_usage_pct: None,
        adapters_loaded: None,
        timestamp: "2024-01-15T10:30:00Z".to_string(),
    };

    let result = client.send_heartbeat(req).await;
    assert!(result.is_ok());
}

// =============================================================================
// Timeout Tests
// =============================================================================

#[tokio::test]
async fn test_request_timeout() {
    let mock_server = MockServer::start().await;

    // Respond with a 2 second delay
    Mock::given(method("POST"))
        .and(path("/v1/workers/heartbeat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(2))
                .set_body_json(&WorkerHeartbeatResponse {
                    acknowledged: true,
                    next_heartbeat_secs: 30,
                }),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = ClientConfig::builder()
        .base_url(&mock_server.uri())
        .heartbeat_timeout(Duration::from_millis(100)) // 100ms timeout
        .build()
        .unwrap();

    let client = ControlPlaneClient::new(config).unwrap();

    let req = WorkerHeartbeatRequest {
        worker_id: "worker-123".to_string(),
        status: "serving".to_string(),
        memory_usage_pct: None,
        adapters_loaded: None,
        timestamp: "2024-01-15T10:30:00Z".to_string(),
    };

    let result = client.send_heartbeat(req).await;
    assert!(result.is_err());

    // Should be a timeout or network error
    // Error format is "Request timed out after {duration_ms}ms" or "Network error: ..."
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("timed out")
            || err.to_string().contains("timeout")
            || err.to_string().contains("Network"),
        "Expected timeout or network error, got: {}",
        err
    );
}
