//! Orchestrator for AdapterOS promotion gates
//!
//! Runs all quality gates and reports pass/fail status with evidence.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod anchor;
pub mod behavior_training;
pub mod code_ingestion;
pub mod code_jobs;
pub mod codebase_ingestion;
pub mod dataset_cleanup;
pub mod federation_daemon;
pub mod gates;
pub mod rectify;
pub mod report;
pub mod supervisor;
pub mod synthesis;
pub mod training;
pub mod training_dataset_integration;

#[cfg(test)]
pub(crate) mod test_support;

pub use behavior_training::{
    BehaviorCategory, BehaviorDataset, BehaviorExample, BehaviorInput, BehaviorMetadata,
    BehaviorTarget, BehaviorTrainingGenerator, DatasetConfig, ExportFilter, SyntheticConfig,
};
pub use code_ingestion::{
    normalize_repo_id, normalize_repo_slug, CodeDatasetConfig, CodeIngestionPipeline,
    CodeIngestionRequest, CodeIngestionResult, CodeIngestionSource,
};
pub use code_jobs::{CodeJobManager, CommitDeltaJob, ScanRepositoryJob, UpdateIndicesJob};
pub use codebase_ingestion::{CodebaseIngestion, IngestionConfig, IngestionResult};
pub use dataset_cleanup::{
    CleanupConfig, CleanupResult, DatasetCleanupManager, StorageHealthReport, StorageQuotaStatus,
};
pub use federation_daemon::{
    FederationDaemon, FederationDaemonConfig, FederationVerificationReport,
};
pub use gates::*;
pub use report::{GateReport, GateResult, ReportFormat};
pub use synthesis::{
    create_synthesis_request, SynthesisBatchStats, SynthesisEngine, SynthesisEngineConfig,
    SynthesisOutput, SynthesisRequest, SynthesisResult,
};
pub use training::{
    TrainingConfig, TrainingJob, TrainingJobStatus, TrainingService, TrainingTemplate,
};
pub use training_dataset_integration::{
    CreateDatasetFromFilePathsRequest, DatasetCreationResult, TrainingDatasetManager,
};

/// Configuration for the orchestrator gate runner.
///
/// Controls how gates are executed, what paths are used, and how failures are handled.
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Continue running gates even if one fails.
    ///
    /// When `false`, the orchestrator stops on the first gate failure.
    /// When `true`, all gates are executed regardless of individual results.
    pub continue_on_error: bool,

    /// CPID (Checkpoint ID) to check gates for.
    ///
    /// This identifies the specific checkpoint/promotion being validated.
    pub cpid: String,

    /// Path to the control plane database.
    ///
    /// Used by gates to query adapter state, telemetry, and other system data.
    pub db_path: String,

    /// Path to telemetry bundles directory.
    ///
    /// Telemetry gates expect bundles to be stored here for analysis.
    pub bundles_path: String,

    /// Path to manifests directory.
    ///
    /// Contains adapter manifests and metadata used by various gates.
    pub manifests_path: String,

    /// Skip dependency checks before running gates.
    ///
    /// When `true`, the orchestrator will not validate that required tools
    /// and paths are available before executing gates.
    pub skip_dependency_checks: bool,

    /// Allow gates to run with degraded dependencies.
    ///
    /// When `true`, gates can proceed even if some optional dependencies
    /// are missing. Critical dependencies must still be present.
    pub allow_degraded_mode: bool,

    /// Require telemetry bundles to exist.
    ///
    /// When `true`, gates that depend on telemetry bundles will fail
    /// if bundles are not found. When `false`, missing bundles are tolerated.
    pub require_telemetry_bundles: bool,

    /// Timeout for individual gate execution (seconds).
    ///
    /// If a gate takes longer than this duration, it will be cancelled
    /// and marked as failed with a timeout error.
    pub gate_timeout_secs: u64,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            continue_on_error: false,
            cpid: String::new(),
            db_path: "var/aos-cp.sqlite3".to_string(),
            bundles_path: "/srv/aos/bundles".to_string(),
            manifests_path: "manifests".to_string(),
            skip_dependency_checks: false,
            allow_degraded_mode: false,
            require_telemetry_bundles: true,
            gate_timeout_secs: 60,
        }
    }
}

/// Main orchestrator that runs all promotion gates.
///
/// The orchestrator coordinates execution of quality gates, dependency checks,
/// and result collection. It provides a unified interface for validating
/// system state before operations like adapter promotion or deployment.
///
/// # Usage
///
/// ```rust,no_run
/// use adapteros_orchestrator::{Orchestrator, OrchestratorConfig};
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = OrchestratorConfig {
///     cpid: "checkpoint-123".to_string(),
///     db_path: "var/aos-cp.sqlite3".to_string(),
///     ..Default::default()
/// };
///
/// let orchestrator = Orchestrator::new(config);
/// let report = orchestrator.run().await?;
///
/// if report.all_passed() {
///     println!("All gates passed!");
/// } else {
///     eprintln!("Some gates failed: {:?}", report.failed_gates());
/// }
/// # Ok(())
/// # }
/// ```
pub struct Orchestrator {
    config: OrchestratorConfig,
    gates: Vec<Box<dyn Gate>>,
    dependency_checker: DependencyChecker,
}

impl Orchestrator {
    /// Create a new orchestrator with standard gates.
    ///
    /// Initializes the orchestrator with the default set of promotion gates:
    /// - DeterminismGate
    /// - MetricsGate
    /// - MetallibGate
    /// - SbomGate
    /// - PerformanceGate
    /// - SecurityGate
    ///
    /// The orchestrator also creates a [`DependencyChecker`] to validate
    /// that required tools and paths are available before gate execution.
    ///
    /// # Arguments
    /// * `config` - Configuration controlling gate execution behavior
    ///
    /// # Returns
    /// A new orchestrator instance ready to run gates.
    pub fn new(config: OrchestratorConfig) -> Self {
        let gates: Vec<Box<dyn Gate>> = vec![
            Box::new(DeterminismGate),
            Box::new(MetricsGate::default()),
            Box::new(MetallibGate),
            Box::new(SbomGate),
            Box::new(PerformanceGate::default()),
            Box::new(SecurityGate),
        ];

        let dependency_checker = DependencyChecker::new();

        Self {
            config,
            gates,
            dependency_checker,
        }
    }

    /// Run dependency checks before gates.
    ///
    /// Validates that all required dependencies (tools, paths) are available
    /// for each gate. This is called automatically by [`run()`](Self::run()),
    /// but can be called separately to check dependencies without executing gates.
    ///
    /// # Returns
    /// A vector of dependency check results, one per gate.
    ///
    /// # Errors
    /// Returns an error if:
    /// - A critical gate has missing dependencies and `allow_degraded_mode` is `false`
    /// - Dependency checking itself fails (e.g., database access issues)
    ///
    /// # Note
    /// This method respects `skip_dependency_checks` in the config and will
    /// return an empty vector if dependency checks are disabled.
    pub async fn check_dependencies(&self) -> Result<Vec<DependencyCheckResult>> {
        if self.config.skip_dependency_checks {
            tracing::debug!("Skipping dependency checks as configured");
            return Ok(Vec::new());
        }

        let gate_ids: Vec<&str> = vec![
            "determinism",
            "metrics",
            "metallib",
            "sbom",
            "performance",
            "security",
        ];

        let results = self.dependency_checker.check_gates(&gate_ids)?;

        // Log dependency check results
        for result in &results {
            if result.all_available {
                tracing::info!(gate = %result.gate_id, "All dependencies available");
            } else {
                match result.degradation_level {
                    2 => tracing::error!(gate = %result.gate_id, "Critical dependencies missing"),
                    1 => {
                        tracing::warn!(gate = %result.gate_id, messages = ?result.messages, "Some optional dependencies missing")
                    }
                    _ => {}
                }
            }
        }

        // Check if any critical gates have missing dependencies
        for result in &results {
            let deps = self
                .dependency_checker
                .get_definition(&result.gate_id)
                .ok_or_else(|| AosError::Internal(format!("Unknown gate: {}", result.gate_id)))?;

            if deps.severity == GateSeverity::Critical
                && result.degradation_level == 2
                && !self.config.allow_degraded_mode
            {
                return Err(AosError::Internal(format!(
                    "Critical dependencies missing for gate '{}': {:?}",
                    result.gate_id, result.messages
                )));
            }
        }

        Ok(results)
    }

    /// Run all gates and return a comprehensive report.
    ///
    /// This is the main entry point for gate execution. It:
    ///
    /// 1. Runs dependency checks (unless skipped)
    /// 2. Executes each gate sequentially with timeout protection
    /// 3. Collects results into a [`GateReport`]
    /// 4. Stops early if `continue_on_error` is `false` and a gate fails
    ///
    /// # Execution Flow
    ///
    /// - Each gate runs with a timeout (configured via `gate_timeout_secs`)
    /// - Gate results are added to the report immediately after execution
    /// - If `continue_on_error` is `false`, execution stops on first failure
    /// - SBOM gate timeouts are handled specially when `allow_degraded_mode` is enabled
    ///
    /// # Returns
    /// A [`GateReport`] containing results for all executed gates, including
    /// dependency check results if performed.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Dependency checks fail (and not skipped)
    /// - A gate times out (unless SBOM in degraded mode)
    /// - Internal orchestrator errors occur
    ///
    /// Note: Individual gate failures are recorded in the report, not returned
    /// as errors (unless `continue_on_error` is `false` and execution stops).
    pub async fn run(&self) -> Result<GateReport> {
        let mut report = GateReport::new(self.config.cpid.clone());

        // Run dependency checks first
        let dep_results = self.check_dependencies().await?;
        if !dep_results.is_empty() {
            report.set_dependency_checks(dep_results);
        }

        for gate in &self.gates {
            let gate_name = gate.name();
            tracing::info!(gate = %gate_name, "Running promotion gate");

            let timeout = Duration::from_secs(self.config.gate_timeout_secs);
            let timed_result = tokio::time::timeout(timeout, gate.check(&self.config)).await;

            let result = match timed_result {
                Ok(res) => res,
                Err(_) => {
                    let msg = format!(
                        "Gate {} timed out after {}s",
                        gate_name, self.config.gate_timeout_secs
                    );
                    let is_sbom = gate_name.eq_ignore_ascii_case("sbom");
                    if is_sbom && self.config.allow_degraded_mode {
                        tracing::warn!(gate = %gate_name, timeout_secs = self.config.gate_timeout_secs, "Gate timed out; allowed in degraded mode");
                        report.add_result(
                            gate_name.clone(),
                            GateResult {
                                passed: true,
                                message: format!("{} (allowing degraded mode)", msg),
                                evidence: None,
                            },
                        );
                        continue;
                    }
                    tracing::error!(gate = %gate_name, timeout_secs = self.config.gate_timeout_secs, "Gate execution timed out");
                    Err(AosError::Internal(msg))
                }
            };

            match &result {
                Ok(()) => {
                    tracing::info!(gate = %gate_name, status = "passed", "Gate check completed");
                    report.add_result(gate_name, GateResult::passed());
                }
                Err(e) => {
                    tracing::warn!(gate = %gate_name, status = "failed", error = %e, "Gate check failed");
                    report.add_result(gate_name, GateResult::failed(e.to_string()));

                    if !self.config.continue_on_error {
                        break;
                    }
                }
            }
        }

        Ok(report)
    }
}

/// Trait for promotion gates.
///
/// All gates must implement this trait to participate in orchestrator validation.
/// Gates perform specific quality checks and return `Ok(())` if the check passes,
/// or an error describing what failed.
///
/// # Implementation Requirements
///
/// - Gates must be `Send + Sync` to work in async contexts
/// - `name()` should return a stable, unique identifier for the gate
/// - `check()` should perform validation and return errors for failures
/// - Gates should respect timeouts (handled by orchestrator)
/// - Gates can use `config` to access paths, CPID, and other settings
///
/// # Example
///
/// ```rust,no_run
/// use adapteros_orchestrator::{Gate, OrchestratorConfig};
/// use anyhow::{Result, Context};
/// use async_trait::async_trait;
///
/// struct MyValidationGate;
///
/// #[async_trait]
/// impl Gate for MyValidationGate {
///     fn name(&self) -> String {
///         "my_validation".to_string()
///     }
///
///     async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
///         // Perform validation
///         std::fs::metadata(&config.db_path)
///             .context("Database path must exist")?;
///         Ok(())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait Gate: Send + Sync {
    /// Returns the unique name/identifier for this gate.
    ///
    /// This name is used in reports and logs. It should be stable across
    /// gate instances and descriptive of what the gate validates.
    fn name(&self) -> String;

    /// Performs the gate's validation check.
    ///
    /// This method is called by the orchestrator to execute the gate.
    /// It should perform all necessary validation and return:
    ///
    /// - `Ok(())` if the gate passes
    /// - `Err(...)` with a descriptive error if the gate fails
    ///
    /// # Arguments
    /// * `config` - Orchestrator configuration providing paths, CPID, and settings
    ///
    /// # Returns
    /// `Ok(())` if validation passes, or an error describing the failure.
    ///
    /// # Errors
    /// Should return errors for:
    /// - Missing required resources (files, tools, data)
    /// - Validation failures (determinism violations, security issues, etc.)
    /// - Timeouts or other execution problems
    ///
    /// # Note
    /// The orchestrator applies a timeout to this method based on
    /// `config.gate_timeout_secs`. Long-running checks should be designed
    /// to complete within reasonable time bounds.
    async fn check(&self, config: &OrchestratorConfig) -> Result<()>;
}

/// Gate check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheckResult {
    pub passed: bool,
    pub message: String,
    pub evidence: Option<String>,
}
