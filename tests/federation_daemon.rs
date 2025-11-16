<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

//! Federation Daemon Integration Tests
//!
//! Tests the federation daemon's continuous verification and quarantine functionality.
//!
//! **Status**: Requires experimental `federation` feature (crate not in workspace)

#![cfg(feature = "federation")]

use adapteros_core::Result;
use adapteros_crypto::Keypair;
use adapteros_db::{Database, Db};
=======
//! Federation Daemon Integration Tests
//!
//! Tests the federation daemon's continuous verification and quarantine functionality.

use adapteros_core::Result;
use adapteros_crypto::Keypair;
use adapteros_db::Db;
>>>>>>> integration-branch
use adapteros_federation::FederationManager;
use adapteros_orchestrator::{FederationDaemon, FederationDaemonConfig};
use adapteros_policy::{PolicyHashWatcher, QuarantineOperation};
use adapteros_telemetry::TelemetryWriter;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

/// Setup test environment with daemon
async fn setup_test_daemon() -> (FederationDaemon, TempDir) {
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
        interval_secs: 1, // Fast interval for testing
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
        Arc::new(db),
        config,
    );

    (daemon, temp_dir)
}

#[tokio::test]
async fn test_daemon_periodic_verification() -> Result<()> {
    let (daemon, _temp) = setup_test_daemon().await;

    // Get initial status (should be operational with no hosts)
    let report = daemon.get_latest_report().await?;
    assert!(report.ok);
    assert_eq!(report.hosts_verified, 0);
    assert_eq!(report.errors.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_quarantine_trigger_on_failure() -> Result<()> {
    let (daemon, _temp) = setup_test_daemon().await;

    // Initially not quarantined
    assert!(!daemon.is_quarantined());

    // Simulate a failed verification by manually creating a report
    use adapteros_orchestrator::FederationVerificationReport;
    use chrono::Utc;

    let failed_report = FederationVerificationReport {
        ok: false,
        hosts_verified: 0,
        errors: vec!["Test federation chain break".to_string()],
        verified_at: Utc::now().to_rfc3339(),
    };

    // Handle the report (should trigger quarantine)
    daemon.handle_verification_report(failed_report).await;

    // Now should be quarantined
    assert!(daemon.is_quarantined());

    // Operations should be denied
    assert!(daemon
        .check_operation(QuarantineOperation::Inference)
        .is_err());
    assert!(daemon
        .check_operation(QuarantineOperation::AdapterLoad)
        .is_err());

    // Audit operations should be allowed
    assert!(daemon.check_operation(QuarantineOperation::Audit).is_ok());
    assert!(daemon.check_operation(QuarantineOperation::Status).is_ok());

    Ok(())
}

#[tokio::test]
async fn test_quarantine_release_on_success() -> Result<()> {
    let (daemon, _temp) = setup_test_daemon().await;

    use adapteros_orchestrator::FederationVerificationReport;
    use chrono::Utc;

    // First trigger quarantine
    let failed_report = FederationVerificationReport {
        ok: false,
        hosts_verified: 0,
        errors: vec!["Test error".to_string()],
        verified_at: Utc::now().to_rfc3339(),
    };
    daemon.handle_verification_report(failed_report).await;
    assert!(daemon.is_quarantined());

    // Then send successful report
    let success_report = FederationVerificationReport {
        ok: true,
        hosts_verified: 3,
        errors: vec![],
        verified_at: Utc::now().to_rfc3339(),
    };
    daemon.handle_verification_report(success_report).await;

    // Should no longer be quarantined
    assert!(!daemon.is_quarantined());

    Ok(())
}

#[tokio::test]
async fn test_daemon_background_task() -> Result<()> {
    let (daemon, _temp) = setup_test_daemon().await;

    // Start daemon in background
    let daemon = Arc::new(daemon);
    let handle = daemon.clone().start();

    // Let it run for a few cycles
    sleep(Duration::from_secs(3)).await;

    // Daemon should still be running (handle won't be finished)
    assert!(!handle.is_finished());

    // Get status
    let report = daemon.get_latest_report().await?;
    assert!(report.ok); // Should be ok with no hosts

    // Abort the background task
    handle.abort();

    Ok(())
}

#[tokio::test]
async fn test_quarantine_status_message() -> Result<()> {
    let (daemon, _temp) = setup_test_daemon().await;

    // Initially operational
    let status = daemon.quarantine_status();
    assert!(status.contains("OPERATIONAL"));

    // Trigger quarantine
    use adapteros_orchestrator::FederationVerificationReport;
    use chrono::Utc;

    let failed_report = FederationVerificationReport {
        ok: false,
        hosts_verified: 0,
        errors: vec!["Chain break detected".to_string()],
        verified_at: Utc::now().to_rfc3339(),
    };
    daemon.handle_verification_report(failed_report).await;

    // Status should reflect quarantine
    let status = daemon.quarantine_status();
    assert!(status.contains("QUARANTINED"));
    assert!(status.contains("Chain break"));

    Ok(())
}

#[tokio::test]
async fn test_multiple_verification_errors() -> Result<()> {
    let (daemon, _temp) = setup_test_daemon().await;

    use adapteros_orchestrator::FederationVerificationReport;
    use chrono::Utc;

    let failed_report = FederationVerificationReport {
        ok: false,
        hosts_verified: 2,
        errors: vec![
            "host-1: Chain break".to_string(),
            "host-2: Signature invalid".to_string(),
            "host-3: Timestamp violation".to_string(),
        ],
        verified_at: Utc::now().to_rfc3339(),
    };

    daemon
        .handle_verification_report(failed_report.clone())
        .await;

    assert!(daemon.is_quarantined());
    assert_eq!(failed_report.errors.len(), 3);

    Ok(())
}
