//! Unit tests for health check functionality

use adapteros_cli::commands::doctor::{ComponentHealth, ComponentStatus, SystemHealthResponse};

#[test]
fn test_component_status_deserialization() {
    // Test that ComponentHealth can be deserialized from JSON
    let json = r#"{
        "component": "database",
        "status": "healthy",
        "message": "All systems operational",
        "timestamp": 1234567890
    }"#;

    let health: ComponentHealth = serde_json::from_str(json).unwrap();
    assert_eq!(health.component, "database");
    assert_eq!(health.status, ComponentStatus::Healthy);
    assert_eq!(health.message, "All systems operational");
    assert_eq!(health.timestamp, 1234567890);
}

#[test]
fn test_system_health_response_parsing() {
    // Test that SystemHealthResponse can be parsed
    let json = r#"{
        "overall_status": "healthy",
        "components": [
            {
                "component": "database",
                "status": "healthy",
                "message": "Connected",
                "timestamp": 1234567890
            },
            {
                "component": "api",
                "status": "degraded",
                "message": "High latency",
                "details": {"latency_ms": 150},
                "timestamp": 1234567891
            }
        ],
        "timestamp": 1234567890
    }"#;

    let response: SystemHealthResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.overall_status, ComponentStatus::Healthy);
    assert_eq!(response.components.len(), 2);
    assert_eq!(response.components[0].component, "database");
    assert_eq!(response.components[1].status, ComponentStatus::Degraded);
}

#[test]
fn test_component_status_ordering() {
    // Test that status ordering makes sense (Healthy > Degraded > Unhealthy)
    assert!(ComponentStatus::Healthy > ComponentStatus::Degraded);
    assert!(ComponentStatus::Degraded > ComponentStatus::Unhealthy);
    assert_eq!(ComponentStatus::Healthy, ComponentStatus::Healthy);
}
