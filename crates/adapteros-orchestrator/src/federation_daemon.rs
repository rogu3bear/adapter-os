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

use adapteros_core::{identity::IdentityEnvelope, AosError, Result};
use adapteros_db::Db;
use adapteros_federation::FederationManager;
use adapteros_policy::{PolicyHashWatcher, QuarantineManager, QuarantineOperation};
use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder, TelemetryWriter};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
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
    /// Minimum connected peers required to allow write operations
    pub quorum_min_peers: usize,
}

impl Default for FederationDaemonConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300, // 5 minutes
            max_hosts_per_sweep: 10,
            enable_quarantine: true,
            quorum_min_peers: 2,
        }
    }
}

/// Federation Daemon - runs periodic federation verification.
///
/// The `FederationDaemon` implements continuous federation verification with
/// policy enforcement. It runs periodic verification sweeps to ensure all
/// federation hosts maintain valid chains, and triggers quarantine on failures.
///
/// ## Policy Compliance
///
/// - **Determinism Ruleset (#2)**: Reproducible verification
/// - **Telemetry Ruleset (#9)**: 100% sampling for federation events
/// - **Incident Ruleset (#17)**: Quarantine on chain breaks
///
/// ## Features
///
/// - Periodic verification sweeps (configurable interval)
/// - Quorum checking (blocks writes when insufficient peers)
/// - Automatic quarantine on verification failures
/// - Read-only mode when quorum is lost
/// - Comprehensive telemetry logging
///
/// # Usage
///
/// ```rust,no_run
/// use adapteros_orchestrator::FederationDaemon;
/// use std::sync::Arc;
/// use tokio::sync::broadcast;
///
/// # async fn example() -> adapteros_core::Result<()> {
/// # let daemon: Arc<FederationDaemon> = todo!();
/// let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
/// let handle = daemon.start(shutdown_rx);
///
/// // Later, to shutdown gracefully:
/// shutdown_tx.send(()).ok();
/// handle.await?;
/// # Ok(())
/// # }
/// ```
pub struct FederationDaemon {
    /// Federation manager
    federation: Arc<FederationManager>,
    /// Policy hash watcher (reserved for policy change detection)
    _policy_watcher: Arc<PolicyHashWatcher>,
    /// Quarantine manager
    quarantine: Arc<parking_lot::RwLock<QuarantineManager>>,
    /// Telemetry writer
    telemetry: Arc<TelemetryWriter>,
    /// Configuration
    config: FederationDaemonConfig,
    /// Database handle
    db: Arc<Db>,
    /// Read-only flag when quorum is lost
    read_only: Arc<parking_lot::RwLock<bool>>,
}

impl FederationDaemon {
    /// Create a new federation daemon.
    ///
    /// Initializes the daemon with federation manager, policy watcher,
    /// telemetry writer, database, and configuration. The daemon is ready
    /// to start verification sweeps after construction.
    ///
    /// # Arguments
    /// * `federation` - Federation manager for chain verification
    /// * `policy_watcher` - Policy hash watcher (reserved for policy change detection)
    /// * `telemetry` - Telemetry writer for event logging
    /// * `db` - Database handle for querying federation data
    /// * `config` - Daemon configuration (intervals, quorum, etc.)
    ///
    /// # Returns
    /// A new `FederationDaemon` instance.
    pub fn new(
        federation: Arc<FederationManager>,
        policy_watcher: Arc<PolicyHashWatcher>,
        telemetry: Arc<TelemetryWriter>,
        db: Arc<Db>,
        config: FederationDaemonConfig,
    ) -> Self {
        Self {
            federation,
            _policy_watcher: policy_watcher,
            quarantine: Arc::new(parking_lot::RwLock::new(QuarantineManager::new())),
            telemetry,
            config,
            db,
            read_only: Arc::new(parking_lot::RwLock::new(false)),
        }
    }

    /// Restore quarantine state from the database on boot.
    ///
    /// Checks the `policy_quarantine` table for unreleased records. If any exist,
    /// the in-memory `QuarantineManager` is set to quarantined with the reason from
    /// the most recent unreleased record. This ensures quarantine survives server
    /// restarts — without this, a restart would clear the in-memory flag and allow
    /// serving despite unresolved policy violations.
    pub async fn restore_quarantine_from_db(&self) -> Result<()> {
        match self.db.get_active_quarantine_details().await {
            Ok(Some(details)) => {
                let reason = format!(
                    "{} (restored from DB, originally triggered at {})",
                    details.reason, details.triggered_at
                );
                let mut quarantine = self.quarantine.write();
                quarantine.set_quarantined(true, reason);
                warn!(
                    original_reason = %details.reason,
                    triggered_at = %details.triggered_at,
                    violation_type = %details.violation_type,
                    "Quarantine state restored from database — system remains quarantined"
                );
            }
            Ok(None) => {
                debug!("No active quarantine records in database — system starts clean");
            }
            Err(e) => {
                // Conservative: if we can't read the DB, log an error but don't quarantine.
                // The background sweep will catch violations within its first cycle.
                error!(error = %e, "Failed to check quarantine state on boot");
            }
        }
        Ok(())
    }

    /// Run the daemon with graceful shutdown support
    pub fn start(self: Arc<Self>, shutdown_rx: broadcast::Receiver<()>) -> JoinHandle<()> {
        info!(
            interval_secs = self.config.interval_secs,
            "Starting federation daemon"
        );

        tokio::spawn(async move {
            use futures_util::FutureExt;
            use std::panic::AssertUnwindSafe;
            if let Err(panic) = AssertUnwindSafe(self.run_loop(shutdown_rx))
                .catch_unwind()
                .await
            {
                tracing::error!(
                    task = "federation_daemon",
                    "federation daemon panicked — peer sync disabled: {:?}",
                    panic
                );
            }
        })
    }

    /// Legacy start method without shutdown support (for backward compatibility).
    ///
    /// Starts the daemon without graceful shutdown. The daemon will run indefinitely
    /// until the process terminates. Prefer [`start()`](Self::start) for new code.
    ///
    /// # Returns
    /// A `JoinHandle` that can be awaited (though it will never complete).
    pub fn start_legacy(self: Arc<Self>) -> JoinHandle<()> {
        let (_, shutdown_rx) = broadcast::channel(1); // Create a dummy receiver that never receives
        self.start(shutdown_rx)
    }

    /// Main daemon loop with graceful shutdown support
    async fn run_loop(&self, mut shutdown_rx: broadcast::Receiver<()>) {
        let mut interval = tokio::time::interval(Duration::from_secs(self.config.interval_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
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
                _ = shutdown_rx.recv() => {
                    info!("Federation daemon received shutdown signal");
                    break;
                }
            }
        }

        info!("Federation daemon stopped verification sweeps");
    }

    /// Check if the system is in read-only mode due to quorum loss.
    ///
    /// When insufficient peers are connected (below `quorum_min_peers`),
    /// the system enters read-only mode to prevent writes that could cause
    /// consistency issues.
    ///
    /// # Returns
    /// `true` if the system is read-only (quorum not met), `false` otherwise.
    pub fn is_read_only(&self) -> bool {
        *self.read_only.read()
    }

    /// Count connected peers based on active federation peers
    async fn connected_peers(&self) -> Result<usize> {
        let pool = self.db.pool();
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM federation_peers
            WHERE active = 1
            "#,
        )
        .fetch_one(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to count active peers: {}", e)))?;

        Ok(count as usize)
    }

    /// Ensure quorum before performing write operations; toggles read-only mode on loss
    async fn check_quorum(&self) -> Result<bool> {
        let connected = self.connected_peers().await?;
        let has_quorum = connected >= self.config.quorum_min_peers;

        {
            let mut read_only = self.read_only.write();
            if !has_quorum && !*read_only {
                *read_only = true;
                warn!(
                    connected_peers = connected,
                    quorum = self.config.quorum_min_peers,
                    "Insufficient quorum - entering read-only mode"
                );
            } else if has_quorum && *read_only {
                *read_only = false;
                info!(
                    connected_peers = connected,
                    quorum = self.config.quorum_min_peers,
                    "Quorum restored - exiting read-only mode"
                );
            }
        }

        Ok(has_quorum)
    }

    /// Verify all federation hosts
    async fn verify_all_hosts(&self) -> Result<FederationVerificationReport> {
        let start = std::time::Instant::now();

        if !self.check_quorum().await? {
            // Avoid writes when quorum is lost; return a read-only report
            return Ok(FederationVerificationReport {
                ok: false,
                hosts_verified: 0,
                errors: vec![format!(
                    "Insufficient quorum: connected peers < {}",
                    self.config.quorum_min_peers
                )],
                verified_at: Utc::now().to_rfc3339(),
            });
        }

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
        let pool = self.db.pool();

        let rows = sqlx::query_scalar::<_, String>(
            r#"
            SELECT DISTINCT host_id
            FROM federation_bundle_signatures
            ORDER BY host_id ASC
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch host IDs: {}", e)))?;

        Ok(rows)
    }

    /// Handle verification report
    async fn handle_verification_report(&self, report: FederationVerificationReport) {
        if self.is_read_only() {
            warn!("Skipping verification handling while in read-only mode (quorum not met)");
            return;
        }

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
        // Insert into policy_quarantine table
        let pool = self.db.pool();

        sqlx::query(
            r#"
            INSERT INTO policy_quarantine (reason, created_at, released)
            VALUES (?, CURRENT_TIMESTAMP, FALSE)
            "#,
        )
        .bind(reason)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert quarantine record: {}", e)))?;

        Ok(())
    }

    /// Create default identity envelope for telemetry events
    fn create_identity(&self) -> IdentityEnvelope {
        IdentityEnvelope::new(
            "system".to_string(),
            "orchestrator".to_string(),
            "federation-verification".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        )
    }

    /// Log verification report to telemetry
    fn log_verification_report(&self, report: &FederationVerificationReport) -> Result<()> {
        let event = TelemetryEventBuilder::new(
            EventType::Custom("federation.periodic_verification".to_string()),
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
            self.create_identity(),
        )
        .component("adapteros-orchestrator".to_string())
        .metadata(json!({
            "verified": report.ok,
            "hosts_verified": report.hosts_verified,
            "errors": report.errors,
            "verified_at": report.verified_at,
        }))
        .build();

        if let Ok(evt) = event {
            let _ = self.telemetry.log_event(evt);
        }
        Ok(())
    }

    /// Log verification error to telemetry
    fn log_verification_error(&self, error: &AosError) -> Result<()> {
        let event = TelemetryEventBuilder::new(
            EventType::Custom("federation.verification_error".to_string()),
            LogLevel::Error,
            format!("Federation verification error: {}", error),
            self.create_identity(),
        )
        .component("adapteros-orchestrator".to_string())
        .metadata(json!({
            "error": error.to_string(),
        }))
        .build();

        if let Ok(evt) = event {
            let _ = self.telemetry.log_event(evt);
        }
        Ok(())
    }

    /// Log quarantine triggered event
    fn log_quarantine_triggered(&self, reason: &str) -> Result<()> {
        let event = TelemetryEventBuilder::new(
            EventType::Custom("policy.quarantine_triggered".to_string()),
            LogLevel::Warn,
            format!("Policy quarantine triggered: {}", reason),
            self.create_identity(),
        )
        .component("adapteros-orchestrator".to_string())
        .metadata(json!({
            "reason": reason,
            "timestamp": Utc::now().to_rfc3339(),
        }))
        .build();

        if let Ok(evt) = event {
            let _ = self.telemetry.log_event(evt);
        }
        Ok(())
    }

    /// Check if the system is currently quarantined.
    ///
    /// Quarantine is triggered when federation verification fails, indicating
    /// potential chain breaks or consistency issues.
    ///
    /// # Returns
    /// `true` if the system is quarantined, `false` otherwise.
    pub fn is_quarantined(&self) -> bool {
        self.quarantine.read().is_quarantined()
    }

    /// Check if an operation is allowed given the current quarantine status.
    ///
    /// Some operations may be blocked when the system is quarantined, depending
    /// on the operation type and quarantine policy.
    ///
    /// # Arguments
    /// * `operation` - The operation to check
    ///
    /// # Returns
    /// `Ok(())` if the operation is allowed, or an error if blocked.
    ///
    /// # Errors
    /// Returns an error if the operation is blocked by quarantine policy.
    pub fn check_operation(&self, operation: QuarantineOperation) -> Result<()> {
        self.quarantine.read().check_operation(operation)
    }

    /// Get a human-readable quarantine status message.
    ///
    /// # Returns
    /// A status message describing the current quarantine state, or an empty
    /// string if not quarantined.
    pub fn quarantine_status(&self) -> String {
        self.quarantine.read().status_message()
    }

    /// Get the latest verification report by running a single verification sweep.
    ///
    /// This is useful for on-demand verification checks without waiting for
    /// the periodic sweep. Note that this does not trigger quarantine actions;
    /// only the periodic sweeps handle quarantine.
    ///
    /// # Returns
    /// A verification report containing results for all checked hosts.
    ///
    /// # Errors
    /// Returns an error if verification fails or database queries fail.
    pub async fn get_latest_report(&self) -> Result<FederationVerificationReport> {
        // Run a single verification sweep
        self.verify_all_hosts().await
    }

    /// Release quarantine status.
    ///
    /// This clears the quarantine flag and violation summary. Should be called
    /// after policy violations have been resolved.
    pub fn release_quarantine(&self) {
        info!("Releasing quarantine status via federation daemon");
        self.quarantine.write().release_quarantine();
    }

    /// Release quarantine for a specific policy pack.
    ///
    /// Returns true if the quarantine was released, false if the current
    /// quarantine was not related to the specified pack.
    pub fn release_quarantine_for_pack(&self, pack_id: &str) -> bool {
        info!(pack_id = %pack_id, "Attempting to release quarantine for specific pack");
        let released = self.quarantine.write().release_quarantine_for_pack(pack_id);
        if released {
            info!(pack_id = %pack_id, "Quarantine released for pack");
        } else {
            warn!(pack_id = %pack_id, "Quarantine not released - pack not matched or not quarantined");
        }
        released
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_crypto::Keypair;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect(
            "Failed to create temporary directory for federation daemon tests. \
             Expected: OS should allow temp directory creation with 'aos-test-' prefix. \
             Context: Tests require writable temp space for isolated database instances and telemetry data. \
            This typically fails only when: (1) the system temp directory is full, (2) permissions are restricted, \
             or (3) OS temp directory is misconfigured."
        )
    }

    async fn setup_test_daemon() -> (FederationDaemon, TempDir) {
        setup_test_daemon_with_config(FederationDaemonConfig {
            interval_secs: 1,
            max_hosts_per_sweep: 10,
            enable_quarantine: true,
            quorum_min_peers: 1,
        })
        .await
    }

    async fn setup_test_daemon_with_config(
        config: FederationDaemonConfig,
    ) -> (FederationDaemon, TempDir) {
        let temp_dir = new_test_tempdir();
        let db_path = temp_dir.path().join("test.db");
        let db_url = format!("sqlite://{}", db_path.display());

        let db = Db::connect(&db_url).await.unwrap();
        db.migrate().await.unwrap();

        let keypair = Keypair::generate();
        let federation =
            FederationManager::new(db.clone(), keypair, "test-tenant".to_string()).unwrap();

        let telemetry_dir = temp_dir.path().join("telemetry");
        std::fs::create_dir_all(&telemetry_dir).unwrap();
        let telemetry = TelemetryWriter::new(&telemetry_dir, 1000, 1024 * 1024).unwrap();

        let policy_watcher = PolicyHashWatcher::new(
            Arc::new(db.clone()),
            Arc::new(telemetry.clone()),
            Some("test-cp".to_string()),
        );

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
        let (daemon, _temp) = setup_test_daemon_with_config(FederationDaemonConfig {
            interval_secs: 1,
            max_hosts_per_sweep: 10,
            enable_quarantine: true,
            quorum_min_peers: 0,
        })
        .await;

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

    #[tokio::test]
    async fn test_read_only_when_quorum_missing() {
        let (daemon, _temp) = setup_test_daemon_with_config(FederationDaemonConfig {
            interval_secs: 1,
            max_hosts_per_sweep: 10,
            enable_quarantine: true,
            quorum_min_peers: 2,
        })
        .await;

        let report = daemon.verify_all_hosts().await.unwrap();

        assert!(daemon.is_read_only());
        assert!(!report.ok);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("Insufficient quorum")),
            "expected insufficient quorum error, got {:?}",
            report.errors
        );
    }
}
