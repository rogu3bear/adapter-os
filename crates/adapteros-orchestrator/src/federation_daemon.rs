//! Federation Daemon - Continuous Federation Verification
//!
//! Implements continuous federation verification with policy enforcement.
//! Runs periodic verification sweeps and triggers quarantine on failures.
//!
//! ## Policy Compliance
//!
//! - Determinism Ruleset (#2): Reproducible verification
//! - Telemetry Ruleset (#9): 100% sampling for federation events
//! - Incident Ruleset (#17): Quarantine on chain breaks

use adapteros_core::{AosError, Result};
use adapteros_db::Database;
use adapteros_federation::FederationManager;
use adapteros_policy::{PolicyHashWatcher, QuarantineManager, QuarantineOperation};
use adapteros_telemetry::{LogLevel, TelemetryEventBuilder, TelemetryWriter};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Federation verification report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationVerificationReport {
    /// Whether all hosts passed verification
    pub ok: bool,
    /// Number of hosts verified
    pub hosts_verified: usize,
    /// Verification errors
    pub errors: Vec<String>,
    /// Timestamp of verification
    pub verified_at: String,
}

/// Federation daemon configuration
#[derive(Debug, Clone)]
pub struct FederationDaemonConfig {
    /// Verification interval in seconds
    pub interval_secs: u64,
    /// Maximum number of hosts to verify per sweep
    pub max_hosts_per_sweep: usize,
    /// Whether to trigger quarantine on failures
    pub enable_quarantine: bool,
}

impl Default for FederationDaemonConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300, // 5 minutes
            max_hosts_per_sweep: 10,
            enable_quarantine: true,
        }
    }
}

/// Federation Daemon - runs periodic federation verification
pub struct FederationDaemon {
    /// Federation manager
    federation: Arc<FederationManager>,
    /// Policy hash watcher
    #[allow(dead_code)]
    policy_watcher: Arc<PolicyHashWatcher>,
    /// Quarantine manager
    quarantine: Arc<parking_lot::RwLock<QuarantineManager>>,
    /// Telemetry writer
    telemetry: Arc<TelemetryWriter>,
    /// Configuration
    config: FederationDaemonConfig,
    /// Database handle
    db: Arc<Database>,
}

impl FederationDaemon {
    /// Create a new federation daemon
    pub fn new(
        federation: Arc<FederationManager>,
        policy_watcher: Arc<PolicyHashWatcher>,
        telemetry: Arc<TelemetryWriter>,
        db: Arc<Database>,
        config: FederationDaemonConfig,
    ) -> Self {
        Self {
            federation,
            policy_watcher,
            quarantine: Arc::new(parking_lot::RwLock::new(QuarantineManager::new())),
            telemetry,
            config,
            db,
        }
    }

    /// Run the daemon (spawns background task)
    pub fn start(self: Arc<Self>) -> JoinHandle<()> {
        info!(
            interval_secs = self.config.interval_secs,
            "Starting federation daemon"
        );

        tokio::spawn(async move {
            self.run_loop().await;
        })
    }

    /// Main daemon loop
    async fn run_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(self.config.interval_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            debug!("Starting federation verification sweep");

            match self.verify_all_hosts().await {
                Ok(report) => {
                    self.handle_verification_report(report).await;
                }
                Err(e) => {
                    error!(error = %e, "Federation verification sweep failed");

                    // Log telemetry event
                    if let Err(te) = self.log_verification_error(&e) {
                        error!(error = %te, "Failed to log verification error");
                    }
                }
            }
        }
    }

    /// Verify all federation hosts
    async fn verify_all_hosts(&self) -> Result<FederationVerificationReport> {
        let start = std::time::Instant::now();

        // Get all host chains from database
        let hosts = self.get_all_host_ids().await?;
        let mut errors = Vec::new();
        let mut hosts_verified = 0;

        for host_id in hosts.iter().take(self.config.max_hosts_per_sweep) {
            match self.verify_host_chain(host_id).await {
                Ok(()) => {
                    hosts_verified += 1;
                    debug!(host_id = %host_id, "Host chain verified");
                }
                Err(e) => {
                    error!(host_id = %host_id, error = %e, "Host chain verification failed");
                    errors.push(format!("{}: {}", host_id, e));
                }
            }
        }

        let ok = errors.is_empty();
        let duration = start.elapsed();

        info!(
            hosts_verified = hosts_verified,
            total_hosts = hosts.len(),
            errors = errors.len(),
            duration_ms = duration.as_millis(),
            "Federation verification sweep completed"
        );

        Ok(FederationVerificationReport {
            ok,
            hosts_verified,
            errors,
            verified_at: Utc::now().to_rfc3339(),
        })
    }

    /// Verify a single host chain
    async fn verify_host_chain(&self, host_id: &str) -> Result<()> {
        // Get the last 10 signatures for this host
        let chain = self.federation.get_host_chain(host_id, 10).await?;

        if chain.is_empty() {
            debug!(host_id = %host_id, "No signatures found for host");
            return Ok(());
        }

        // Verify cross-host chain continuity
        self.federation.verify_cross_host_chain(&chain).await?;

        Ok(())
    }

    /// Get all unique host IDs from federation signatures
    async fn get_all_host_ids(&self) -> Result<Vec<String>> {
        use adapteros_db::DatabaseBackend;

        let rows = match self.db.backend() {
            DatabaseBackend::Sqlite(db) => sqlx::query_scalar::<_, String>(
                r#"
                    SELECT DISTINCT host_id
                    FROM federation_bundle_signatures
                    ORDER BY host_id ASC
                    "#,
            )
            .fetch_all(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to fetch host IDs: {}", e)))?,
            DatabaseBackend::Postgres(_) => {
                return Err(AosError::Database(
                    "Federation daemon requires SQLite backend. PostgreSQL support not yet implemented.".to_string()
                ));
            }
        };

        Ok(rows)
    }

    /// Handle verification report
    async fn handle_verification_report(&self, report: FederationVerificationReport) {
        // Log telemetry event (100% sampling per Telemetry Ruleset #9)
        if let Err(e) = self.log_verification_report(&report) {
            error!(error = %e, "Failed to log verification report");
        }

        // Trigger quarantine if verification failed
        if !report.ok && self.config.enable_quarantine {
            let reason = format!(
                "Federation verification failed: {} error(s) detected",
                report.errors.len()
            );

            warn!(
                errors = report.errors.len(),
                "Triggering quarantine due to federation verification failure"
            );

            // Set quarantine status
            {
                let mut quarantine = self.quarantine.write();
                quarantine.set_quarantined(true, reason.clone());
            }

            // Trigger policy watcher quarantine
            if let Err(e) = self.trigger_policy_quarantine(&reason).await {
                error!(error = %e, "Failed to trigger policy quarantine");
            }

            // Log quarantine telemetry
            if let Err(e) = self.log_quarantine_triggered(&reason) {
                error!(error = %e, "Failed to log quarantine event");
            }
        } else if report.ok {
            // Clear quarantine if verification passed
            let mut quarantine = self.quarantine.write();
            if quarantine.is_quarantined() {
                info!("Clearing quarantine - federation verification passed");
                quarantine.set_quarantined(false, String::new());
            }
        }
    }

    /// Trigger policy quarantine
    async fn trigger_policy_quarantine(&self, reason: &str) -> Result<()> {
        use adapteros_db::DatabaseBackend;

        match self.db.backend() {
            DatabaseBackend::Sqlite(db) => {
                sqlx::query(
                    r#"
                    INSERT INTO policy_quarantine (reason, created_at, released)
                    VALUES (?, CURRENT_TIMESTAMP, FALSE)
                    "#,
                )
                .bind(reason)
                .execute(db.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to insert quarantine record: {}", e))
                })?;
            }
            DatabaseBackend::Postgres(_) => {
                return Err(AosError::Database(
                    "Federation daemon requires SQLite backend. PostgreSQL support not yet implemented.".to_string()
                ));
            }
        }

        Ok(())
    }

    /// Log verification report to telemetry
    fn log_verification_report(&self, report: &FederationVerificationReport) -> Result<()> {
        let event = TelemetryEventBuilder::new(
            adapteros_telemetry::EventType::Custom("federation.periodic_verification".to_string()),
            if report.ok {
                LogLevel::Info
            } else {
                LogLevel::Error
            },
            format!(
                "Federation periodic verification: {}/{} hosts verified",
                report.hosts_verified,
                report.hosts_verified + report.errors.len()
            ),
        )
        .component("adapteros-orchestrator".to_string())
        .metadata(json!({
            "verified": report.ok,
            "hosts_verified": report.hosts_verified,
            "errors": report.errors,
            "verified_at": report.verified_at,
        }))
        .build();

        let _ = self.telemetry.log_event(event);
        Ok(())
    }

    /// Log verification error to telemetry
    fn log_verification_error(&self, error: &AosError) -> Result<()> {
        let event = TelemetryEventBuilder::new(
            adapteros_telemetry::EventType::Custom("federation.verification_error".to_string()),
            LogLevel::Error,
            format!("Federation verification error: {}", error),
        )
        .component("adapteros-orchestrator".to_string())
        .metadata(json!({
            "error": error.to_string(),
        }))
        .build();

        let _ = self.telemetry.log_event(event);
        Ok(())
    }

    /// Log quarantine triggered event
    fn log_quarantine_triggered(&self, reason: &str) -> Result<()> {
        let event = TelemetryEventBuilder::new(
            adapteros_telemetry::EventType::Custom("policy.quarantine_triggered".to_string()),
            LogLevel::Warn,
            format!("Policy quarantine triggered: {}", reason),
        )
        .component("adapteros-orchestrator".to_string())
        .metadata(json!({
            "reason": reason,
            "timestamp": Utc::now().to_rfc3339(),
        }))
        .build();

        let _ = self.telemetry.log_event(event);
        Ok(())
    }

    /// Check if system is quarantined
    pub fn is_quarantined(&self) -> bool {
        self.quarantine.read().is_quarantined()
    }

    /// Check if an operation is allowed
    pub fn check_operation(&self, operation: QuarantineOperation) -> Result<()> {
        self.quarantine.read().check_operation(operation)
    }

    /// Get quarantine status message
    pub fn quarantine_status(&self) -> String {
        self.quarantine.read().status_message()
    }

    /// Get latest verification report
    pub async fn get_latest_report(&self) -> Result<FederationVerificationReport> {
        // Run a single verification sweep
        self.verify_all_hosts().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_crypto::Keypair;
    use adapteros_db::Db;
    use tempfile::TempDir;

    async fn setup_test_daemon() -> (FederationDaemon, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_url = format!("sqlite://{}", db_path.display());

        // Use Database wrapper for consistency
        let db = Database::connect(&db_url).await.unwrap();
        db.migrate().await.unwrap();

        // FederationManager needs Db, so extract from Database wrapper
        // This is safe because we're using SQLite in tests
        let db_inner: Db = db.inner().clone();
        let keypair = Keypair::generate();
        let federation = FederationManager::new(db_inner, keypair).unwrap();

        let telemetry_dir = temp_dir.path().join("telemetry");
        std::fs::create_dir_all(&telemetry_dir).unwrap();
        let telemetry = TelemetryWriter::new(&telemetry_dir, 1000, 1024 * 1024).unwrap();

        // PolicyHashWatcher expects Arc<Database>
        let policy_watcher = PolicyHashWatcher::new(
            Arc::new(db.clone()),
            Arc::new(telemetry.clone()),
            Some("test-cp".to_string()),
        );

        let config = FederationDaemonConfig {
            interval_secs: 1,
            max_hosts_per_sweep: 10,
            enable_quarantine: true,
        };

        // FederationDaemon now expects Arc<Database>
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
    async fn test_verify_empty_hosts() {
        let (daemon, _temp) = setup_test_daemon().await;

        let report = daemon.verify_all_hosts().await.unwrap();
        assert!(report.ok);
        assert_eq!(report.hosts_verified, 0);
        assert!(report.errors.is_empty());
    }

    #[tokio::test]
    async fn test_quarantine_on_failure() {
        let (daemon, _temp) = setup_test_daemon().await;

        let report = FederationVerificationReport {
            ok: false,
            hosts_verified: 0,
            errors: vec!["Test error".to_string()],
            verified_at: Utc::now().to_rfc3339(),
        };

        daemon.handle_verification_report(report).await;
        assert!(daemon.is_quarantined());
    }

    #[tokio::test]
    async fn test_clear_quarantine_on_success() {
        let (daemon, _temp) = setup_test_daemon().await;

        // First trigger quarantine
        let fail_report = FederationVerificationReport {
            ok: false,
            hosts_verified: 0,
            errors: vec!["Test error".to_string()],
            verified_at: Utc::now().to_rfc3339(),
        };
        daemon.handle_verification_report(fail_report).await;
        assert!(daemon.is_quarantined());

        // Then clear it with success
        let success_report = FederationVerificationReport {
            ok: true,
            hosts_verified: 1,
            errors: vec![],
            verified_at: Utc::now().to_rfc3339(),
        };
        daemon.handle_verification_report(success_report).await;
        assert!(!daemon.is_quarantined());
    }
}
