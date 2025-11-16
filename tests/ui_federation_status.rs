<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

=======
>>>>>>> integration-branch
//! UI Federation Status Integration Tests
//!
//! Tests the federation status REST API endpoints and response formats.

use adapteros_core::Result;
use adapteros_crypto::Keypair;
<<<<<<< HEAD
use adapteros_db::{Database, Db};
=======
use adapteros_db::Db;
>>>>>>> integration-branch
use adapteros_federation::FederationManager;
use adapteros_orchestrator::{FederationDaemon, FederationDaemonConfig};
use adapteros_policy::PolicyHashWatcher;
use adapteros_server_api::handlers::federation::{
    FederationApiState, FederationStatusResponse, QuarantineStatusResponse,
};
use adapteros_telemetry::TelemetryWriter;
use std::sync::Arc;
use tempfile::TempDir;

/// Setup test API state
async fn setup_api_state() -> (Arc<FederationApiState>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db_url = format!("sqlite://{}", db_path.display());

<<<<<<< HEAD
    // Use Database wrapper for consistency
    let db = Database::connect(&db_url).await.unwrap();
    db.migrate().await.unwrap();

    // FederationManager needs Db, so extract from Database wrapper
    // This is safe because we're using SQLite in tests
    let db_inner = db.inner().clone();
    let keypair = Keypair::generate();
    let federation = FederationManager::new(db_inner, keypair).unwrap();
=======
    let db = Db::connect(&db_url).await.unwrap();
    db.migrate().await.unwrap();

    let keypair = Keypair::generate();
    let federation = FederationManager::new(db.clone(), keypair).unwrap();
>>>>>>> integration-branch

    let telemetry_dir = temp_dir.path().join("telemetry");
    std::fs::create_dir_all(&telemetry_dir).unwrap();
    let telemetry = TelemetryWriter::new(&telemetry_dir, 1000, 1024 * 1024).unwrap();

<<<<<<< HEAD
    // PolicyHashWatcher expects Arc<Database>
=======
>>>>>>> integration-branch
    let policy_watcher = PolicyHashWatcher::new(
        Arc::new(db.clone()),
        Arc::new(telemetry.clone()),
        Some("test-cp".to_string()),
    );

    let config = FederationDaemonConfig {
        interval_secs: 300,
        max_hosts_per_sweep: 10,
        enable_quarantine: true,
    };

<<<<<<< HEAD
    // FederationDaemon now expects Arc<Database>
=======
>>>>>>> integration-branch
    let daemon = FederationDaemon::new(
        Arc::new(federation),
        Arc::new(policy_watcher),
        Arc::new(telemetry),
        Arc::new(db.clone()),
        config,
    );

<<<<<<< HEAD
    // FederationApiState needs Arc<Db>, so extract from Database wrapper
    let db_for_state = db.into_inner();
    let state = FederationApiState {
        daemon: Arc::new(daemon),
        db: Arc::new(db_for_state),
=======
    let state = FederationApiState {
        daemon: Arc::new(daemon),
        db: Arc::new(db),
>>>>>>> integration-branch
    };

    (Arc::new(state), temp_dir)
}

#[tokio::test]
async fn test_federation_status_response_format() -> Result<()> {
    let (state, _temp) = setup_api_state().await;

    // Test serialization/deserialization
    use adapteros_orchestrator::FederationVerificationReport;
    use chrono::Utc;

    let response = FederationStatusResponse {
        operational: true,
        quarantined: false,
        quarantine_reason: None,
        latest_verification: Some(FederationVerificationReport {
            ok: true,
            hosts_verified: 5,
            errors: vec![],
            verified_at: Utc::now().to_rfc3339(),
        }),
        total_hosts: 5,
        timestamp: Utc::now().to_rfc3339(),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("operational"));
    assert!(json.contains("quarantined"));
    assert!(json.contains("latest_verification"));

    // Deserialize back
    let deserialized: FederationStatusResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.operational, response.operational);
    assert_eq!(deserialized.total_hosts, response.total_hosts);

    Ok(())
}

#[tokio::test]
async fn test_quarantine_status_response_format() -> Result<()> {
    let (state, _temp) = setup_api_state().await;

    use adapteros_server_api::handlers::federation::QuarantineDetails;
    use chrono::Utc;

    let response = QuarantineStatusResponse {
        quarantined: true,
        details: Some(QuarantineDetails {
            reason: "Federation chain break detected".to_string(),
            triggered_at: Utc::now().to_rfc3339(),
            violation_type: "federation_verification_failed".to_string(),
            cpid: Some("cpid-001".to_string()),
        }),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("quarantined"));
    assert!(json.contains("details"));
    assert!(json.contains("Federation chain break"));

    // Deserialize back
    let deserialized: QuarantineStatusResponse = serde_json::from_str(&json).unwrap();
    assert!(deserialized.quarantined);
    assert!(deserialized.details.is_some());

    Ok(())
}

#[tokio::test]
async fn test_api_state_daemon_access() -> Result<()> {
    let (state, _temp) = setup_api_state().await;

    // Test daemon is accessible
    assert!(!state.daemon.is_quarantined());

    // Get status should work
    let report = state.daemon.get_latest_report().await?;
    assert!(report.ok);

    Ok(())
}

#[tokio::test]
async fn test_federation_status_operational() -> Result<()> {
    let (state, _temp) = setup_api_state().await;

    // Get status
    let report = state.daemon.get_latest_report().await?;

    // Should be operational with no hosts
    assert!(report.ok);
    assert_eq!(report.hosts_verified, 0);
    assert!(report.errors.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_federation_status_with_errors() -> Result<()> {
    let (state, _temp) = setup_api_state().await;

    use adapteros_orchestrator::FederationVerificationReport;
    use chrono::Utc;

    // Simulate errors
    let failed_report = FederationVerificationReport {
        ok: false,
        hosts_verified: 2,
        errors: vec![
            "host-1: Signature mismatch".to_string(),
            "host-2: Chain break".to_string(),
        ],
        verified_at: Utc::now().to_rfc3339(),
    };

    // Handle report
    state.daemon.handle_verification_report(failed_report).await;

    // Should be quarantined
    assert!(state.daemon.is_quarantined());

    Ok(())
}

#[tokio::test]
async fn test_json_response_structure() -> Result<()> {
    let (state, _temp) = setup_api_state().await;

    use chrono::Utc;

    // Create a typical response
    let response = FederationStatusResponse {
        operational: false,
        quarantined: true,
        quarantine_reason: Some("Test quarantine".to_string()),
        latest_verification: None,
        total_hosts: 0,
        timestamp: Utc::now().to_rfc3339(),
    };

    let json = serde_json::to_value(&response).unwrap();

    // Verify structure
    assert!(json.get("operational").is_some());
    assert!(json.get("quarantined").is_some());
    assert!(json.get("quarantine_reason").is_some());
    assert!(json.get("total_hosts").is_some());
    assert!(json.get("timestamp").is_some());

    Ok(())
}
