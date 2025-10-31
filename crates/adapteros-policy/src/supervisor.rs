//! Policy Supervisor Daemon
//!
//! Timer-based policy supervisor that periodically runs the policy engine
//! to enforce compliance and generate reports.

use adapteros_core::Result;
use adapteros_telemetry::TelemetryWriter;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{error, info};

/// Policy supervisor configuration
#[derive(Clone)]
pub struct PolicySupervisorConfig {
    /// Interval between policy checks (seconds)
    pub check_interval_secs: u64,
    /// Enable telemetry reporting
    pub enable_telemetry: bool,
    /// Telemetry writer (if enabled)
    pub telemetry: Option<Arc<TelemetryWriter>>,
}

impl Default for PolicySupervisorConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 60, // Check every minute
            enable_telemetry: true,
            telemetry: None,
        }
    }
}

/// Policy supervisor daemon
pub struct PolicySupervisor {
    config: PolicySupervisorConfig,
    engine: Arc<RwLock<crate::PolicyEngine>>,
    running: Arc<AtomicBool>,
}

impl PolicySupervisor {
    /// Create a new policy supervisor
    pub fn new(engine: crate::PolicyEngine, config: PolicySupervisorConfig) -> Self {
        Self {
            config,
            engine: Arc::new(RwLock::new(engine)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the supervisor daemon
    pub async fn start(self: Arc<Self>) -> Result<()> {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(adapteros_core::AosError::PolicyViolation(
                "Policy supervisor already running".to_string(),
            ));
        }

        info!(
            interval_secs = self.config.check_interval_secs,
            "Starting policy supervisor daemon"
        );

        let mut check_interval = interval(Duration::from_secs(self.config.check_interval_secs));
        check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            check_interval.tick().await;

            // Check if supervisor should stop
            if !self.running.load(Ordering::SeqCst) {
                info!("Policy supervisor stopping");
                break;
            }

            // Run policy checks
            if let Err(e) = self.run_policy_checks().await {
                error!(error = %e, "Policy check failed");
            }
        }

        Ok(())
    }

    /// Stop the supervisor daemon
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!("Policy supervisor stop requested");
    }

    /// Run policy compliance checks
    async fn run_policy_checks(&self) -> Result<()> {
        let engine = self.engine.read().await;

        // Get compliance report from unified enforcer
        // Note: In a full implementation, this would use UnifiedPolicyEnforcer
        // For now, we'll do basic checks via the pack manager
        let pack_manager = engine.pack_manager();

        // Check each policy pack
        // Note: Accessing configs requires a method - we'll iterate via pack IDs
        for pack_id in crate::policy_packs::PolicyPackId::all() {
            let Some(config) = pack_manager.get_config(&pack_id) else {
                continue;
            };

            if !config.enabled {
                continue;
            }

            // Validate pack configuration
            let hash = config.calculate_hash();
            info!(
                pack_id = ?pack_id,
                version = %config.version,
                hash = %hash.to_hex(),
                "Policy pack check"
            );

            // Emit telemetry if enabled
            if let Some(ref telemetry) = self.config.telemetry {
                let _ = telemetry.log(
                    "policy.pack_check",
                    serde_json::json!({
                        "pack_id": format!("{:?}", pack_id),
                        "version": config.version,
                        "hash": hash.to_hex(),
                        "enabled": config.enabled,
                        "enforcement_level": format!("{:?}", config.enforcement_level),
                    }),
                );
            }
        }

        Ok(())
    }

    /// Get current supervisor status
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for PolicySupervisor {
    fn drop(&mut self) {
        self.stop();
    }
}
