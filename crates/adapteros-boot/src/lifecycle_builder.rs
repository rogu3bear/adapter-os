//! Lifecycle builder for phase-gated boot sequences.
//!
//! The LifecycleBuilder provides a structured way to execute boot phases in order,
//! with validation, timing, and error handling.
//!
//! ## Design Principles
//!
//! - **NO Axum dependencies**: This crate must not depend on Axum, tower, or hyper
//! - **Phase-gated**: Each phase must complete before the next can start
//! - **Observable**: Phase timings are recorded for boot reports
//! - **Recoverable**: Failures emit structured errors with context
//!
//! ## Example
//!
//! ```rust,no_run
//! use adapteros_boot::lifecycle_builder::{LifecycleBuilder, LifecycleConfig};
//!
//! async fn boot() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = LifecycleConfig::default();
//!     let artifacts = LifecycleBuilder::new(config)
//!         .start().await?
//!         // ... more phases
//!         .build().await?;
//!     Ok(())
//! }
//! ```

use ed25519_dalek::{SigningKey, VerifyingKey};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::boot_report::{BootReport, BootReportBuilder};
use crate::error::{BootError, BootResult};
use crate::phase::{BootPhase, PhaseTiming, PhaseTransitions};
use crate::services::ServiceRegistry;
use crate::worker_auth::{derive_kid_from_verifying_key, load_or_generate_worker_keypair};

/// Configuration for the lifecycle builder.
#[derive(Debug, Clone)]
pub struct LifecycleConfig {
    /// Maximum time allowed for the entire boot sequence.
    pub boot_timeout: Duration,

    /// Whether to emit a boot report at the end.
    pub emit_boot_report: bool,

    /// Path to write the boot report file (if emit_boot_report is true).
    pub boot_report_path: Option<String>,

    /// Path to the keys directory (e.g., "var/keys").
    pub keys_dir: PathBuf,

    /// Server bind address for the boot report.
    pub bind_addr: String,

    /// Server port for the boot report.
    pub port: u16,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            boot_timeout: Duration::from_secs(300),
            emit_boot_report: true,
            boot_report_path: Some("var/run/boot_report.json".to_string()),
            keys_dir: PathBuf::from("var/keys"),
            bind_addr: "0.0.0.0".to_string(),
            port: 8080,
        }
    }
}

/// Artifacts produced by a successful boot.
///
/// These are the outputs of the boot process that can be used to construct
/// application state (e.g., AppState in adapteros-server-api).
#[derive(Debug)]
pub struct BootArtifacts {
    /// Service registry containing initialized services.
    pub services: ServiceRegistry,

    /// Phase timing information.
    pub phase_timings: Vec<PhaseTiming>,

    /// Boot report (if emit_boot_report was true).
    pub boot_report: Option<BootReport>,

    /// Worker signing keypair (private key for token generation).
    pub worker_signing_keypair: Option<Arc<SigningKey>>,

    /// Worker verifying key (public key for distribution to workers).
    pub worker_verifying_key: Option<Arc<VerifyingKey>>,

    /// Worker key ID (for kid header in tokens).
    pub worker_key_kid: Option<String>,

    /// Configuration used for boot.
    pub config: LifecycleConfig,
}

/// Phase-gated lifecycle builder.
///
/// Provides a builder pattern for executing boot phases in order.
/// Each phase method validates that the previous phase completed
/// before proceeding.
pub struct LifecycleBuilder {
    config: LifecycleConfig,
    current_phase: BootPhase,
    start_time: Instant,
    phase_timings: Vec<PhaseTiming>,
    services: ServiceRegistry,

    // Worker auth
    worker_keypair: Option<SigningKey>,

    // Auth key IDs for boot report
    auth_key_kids: Vec<String>,

    // Config hash for boot report
    config_hash: Option<String>,
}

impl LifecycleBuilder {
    /// Create a new lifecycle builder with the given configuration.
    pub fn new(config: LifecycleConfig) -> Self {
        Self {
            config,
            current_phase: BootPhase::Stopped,
            start_time: Instant::now(),
            phase_timings: Vec::new(),
            services: ServiceRegistry::new(),
            worker_keypair: None,
            auth_key_kids: Vec::new(),
            config_hash: None,
        }
    }

    /// Get the current boot phase.
    pub fn current_phase(&self) -> BootPhase {
        self.current_phase
    }

    /// Get the elapsed time since boot started.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Set the config hash for the boot report.
    pub fn set_config_hash(&mut self, hash: impl Into<String>) {
        self.config_hash = Some(hash.into());
    }

    /// Add an auth key ID for the boot report.
    pub fn add_auth_key_kid(&mut self, kid: impl Into<String>) {
        self.auth_key_kids.push(kid.into());
    }

    /// Get a reference to the service registry.
    pub fn services(&self) -> &ServiceRegistry {
        &self.services
    }

    /// Get a mutable reference to the service registry.
    pub fn services_mut(&mut self) -> &mut ServiceRegistry {
        &mut self.services
    }

    /// Transition to a new phase with validation.
    fn transition(&mut self, to: BootPhase, reason: &str) -> BootResult<()> {
        if !PhaseTransitions::is_valid(self.current_phase, to) {
            return Err(BootError::InvalidTransition {
                from: self.current_phase,
                to,
            });
        }

        // Complete current phase timing
        if let Some(timing) = self.phase_timings.last_mut() {
            timing.complete();
        }

        // Start new phase timing
        self.phase_timings.push(PhaseTiming::start(to));

        let elapsed_ms = self.elapsed().as_millis() as u64;
        info!(
            phase = %to,
            previous = %self.current_phase,
            reason = reason,
            elapsed_ms = elapsed_ms,
            "Boot phase transition"
        );

        self.current_phase = to;
        Ok(())
    }

    /// Transition to failed state with a reason.
    fn fail(&mut self, error: BootError) -> BootError {
        if let Err(e) = self.transition(BootPhase::Failed, &error.to_string()) {
            warn!(
                error = %e,
                "Failed to transition to Failed state"
            );
        }
        error
    }

    // ============ Boot Phase Methods ============

    /// Phase 1: Start the boot sequence.
    ///
    /// This marks the beginning of the boot process.
    pub async fn start(mut self) -> BootResult<Self> {
        self.transition(BootPhase::Starting, "boot-initiated")?;
        Ok(self)
    }

    /// Phase 2: Database connecting phase marker.
    ///
    /// The actual database connection should be done by the caller
    /// and the result registered in the service registry.
    pub async fn db_connecting(mut self) -> BootResult<Self> {
        self.transition(BootPhase::DbConnecting, "starting-db-connection")?;
        Ok(self)
    }

    /// Phase 3: Migration phase marker.
    pub async fn migrating(mut self) -> BootResult<Self> {
        self.transition(BootPhase::Migrating, "db-connected")?;
        Ok(self)
    }

    /// Phase 4: Seeding phase marker.
    pub async fn seeding(mut self) -> BootResult<Self> {
        self.transition(BootPhase::Seeding, "migrations-complete")?;
        Ok(self)
    }

    /// Phase 5: Load policies phase marker.
    pub async fn loading_policies(mut self) -> BootResult<Self> {
        self.transition(BootPhase::LoadingPolicies, "seeding-complete")?;
        Ok(self)
    }

    /// Phase 6: Initialize crypto, including worker auth keypair.
    ///
    /// This generates or loads the Ed25519 keypair used for worker authentication.
    pub async fn init_crypto(mut self) -> BootResult<Self> {
        let worker_key_path = self.config.keys_dir.join("worker_signing.key");

        match load_or_generate_worker_keypair(&worker_key_path) {
            Ok(keypair) => {
                self.worker_keypair = Some(keypair);
                info!(
                    path = %worker_key_path.display(),
                    "Worker signing keypair ready"
                );
            }
            Err(e) => {
                warn!(
                    error = %e,
                    path = %worker_key_path.display(),
                    "Failed to load/generate worker keypair, worker auth disabled"
                );
            }
        }

        Ok(self)
    }

    /// Phase 7: Starting backend phase marker.
    pub async fn starting_backend(mut self) -> BootResult<Self> {
        self.transition(BootPhase::StartingBackend, "policies-loaded")?;
        Ok(self)
    }

    /// Phase 8: Loading base models phase marker.
    pub async fn loading_base_models(mut self) -> BootResult<Self> {
        self.transition(BootPhase::LoadingBaseModels, "backend-started")?;
        Ok(self)
    }

    /// Phase 9: Loading adapters phase marker.
    pub async fn loading_adapters(mut self) -> BootResult<Self> {
        self.transition(BootPhase::LoadingAdapters, "base-models-loaded")?;
        Ok(self)
    }

    /// Phase 10: Worker discovery phase marker.
    pub async fn worker_discovery(mut self) -> BootResult<Self> {
        self.transition(BootPhase::WorkerDiscovery, "adapters-loaded")?;
        Ok(self)
    }

    /// Finalize the boot and transition to Ready state.
    ///
    /// Returns the boot artifacts that can be used to construct application state.
    pub async fn ready(mut self) -> BootResult<Self> {
        self.transition(BootPhase::Ready, "workers-discovered")?;
        Ok(self)
    }

    /// Build the final artifacts after reaching Ready state.
    pub async fn build(mut self) -> BootResult<BootArtifacts> {
        // Ensure we're in Ready state
        if !self.current_phase.is_ready() {
            return Err(BootError::InvalidTransition {
                from: self.current_phase,
                to: BootPhase::Ready,
            });
        }

        // Complete final phase timing
        if let Some(timing) = self.phase_timings.last_mut() {
            timing.complete();
        }

        // Extract worker key info
        let (worker_signing_keypair, worker_verifying_key, worker_key_kid) =
            if let Some(ref keypair) = self.worker_keypair {
                let verifying_key = keypair.verifying_key();
                let kid = derive_kid_from_verifying_key(&verifying_key);
                (
                    Some(Arc::new(keypair.clone())),
                    Some(Arc::new(verifying_key)),
                    Some(kid),
                )
            } else {
                (None, None, None)
            };

        // Generate boot report if enabled
        let boot_report = if self.config.emit_boot_report {
            let mut builder = BootReportBuilder::new()
                .phase_timings(self.phase_timings.clone())
                .bind_addr(&self.config.bind_addr)
                .port(self.config.port)
                .auth_key_kids(self.auth_key_kids.clone());

            if let Some(ref hash) = self.config_hash {
                builder = builder.config_hash(hash);
            }

            if let Some(ref kid) = worker_key_kid {
                builder = builder.add_worker_key_kid(kid);
            }

            let report = builder.build();

            // Write and emit
            if let Some(ref path) = self.config.boot_report_path {
                if let Err(e) = report.write_and_emit(path) {
                    warn!(error = %e, path = path, "Failed to write boot report");
                }
            } else {
                report.emit_log();
            }

            Some(report)
        } else {
            None
        };

        Ok(BootArtifacts {
            services: self.services,
            phase_timings: self.phase_timings,
            boot_report,
            worker_signing_keypair,
            worker_verifying_key,
            worker_key_kid,
            config: self.config,
        })
    }

    /// Fail the boot with an error.
    ///
    /// Transitions to Failed state and returns the error.
    pub fn fail_with(mut self, error: BootError) -> BootError {
        self.fail(error)
    }
}

/// A simplified boot sequence for common use cases.
///
/// This provides a quick way to boot through all phases without
/// custom logic at each phase.
pub async fn simple_boot(config: LifecycleConfig) -> BootResult<BootArtifacts> {
    LifecycleBuilder::new(config)
        .start()
        .await?
        .db_connecting()
        .await?
        .migrating()
        .await?
        .seeding()
        .await?
        .loading_policies()
        .await?
        .init_crypto()
        .await?
        .starting_backend()
        .await?
        .loading_base_models()
        .await?
        .loading_adapters()
        .await?
        .worker_discovery()
        .await?
        .ready()
        .await?
        .build()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> LifecycleConfig {
        LifecycleConfig {
            emit_boot_report: false,
            keys_dir: std::env::temp_dir().join("adapteros-boot-test"),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_boot_sequence() {
        let builder = LifecycleBuilder::new(test_config()).start().await.unwrap();

        assert_eq!(builder.current_phase(), BootPhase::Starting);

        let builder = builder.db_connecting().await.unwrap();
        assert_eq!(builder.current_phase(), BootPhase::DbConnecting);
    }

    #[tokio::test]
    async fn test_invalid_transition() {
        let builder = LifecycleBuilder::new(test_config());

        // Try to skip directly to Ready without going through boot phases
        let result = builder.ready().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_phase_timings() {
        let builder = LifecycleBuilder::new(test_config())
            .start()
            .await
            .unwrap()
            .db_connecting()
            .await
            .unwrap();

        // Should have 2 phase timings
        assert_eq!(builder.phase_timings.len(), 2);
        assert_eq!(builder.phase_timings[0].phase, BootPhase::Starting);
        assert_eq!(builder.phase_timings[1].phase, BootPhase::DbConnecting);
    }

    #[tokio::test]
    async fn test_service_registry() {
        let mut builder = LifecycleBuilder::new(test_config()).start().await.unwrap();

        // Register a service
        builder.services_mut().register(42i32);

        assert!(builder.services().contains::<i32>());
        assert_eq!(*builder.services().get::<i32>().unwrap(), 42);
    }
}
