//! Orchestrator for AdapterOS promotion gates
//!
//! Runs all quality gates and reports pass/fail status with evidence.

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub mod code_jobs;
pub mod codebase_ingestion;
pub mod dataset_cleanup;
pub mod federation_daemon;
pub mod gates;
pub mod report;
pub mod supervisor;
pub mod training;
pub mod training_dataset_integration;

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
pub use training::{
    TrainingConfig, TrainingJob, TrainingJobStatus, TrainingService, TrainingTemplate,
};
pub use training_dataset_integration::{
    CreateDatasetFromDocumentsRequest, DatasetCreationResult, TrainingDatasetManager,
};

/// Gate runner configuration
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Continue running gates even if one fails
    pub continue_on_error: bool,
    /// CPID to check gates for
    pub cpid: String,
    /// Path to database
    pub db_path: String,
    /// Path to telemetry bundles
    pub bundles_path: String,
    /// Path to manifests
    pub manifests_path: String,
    /// Skip dependency checks before running gates
    pub skip_dependency_checks: bool,
    /// Allow gates to run with degraded dependencies
    pub allow_degraded_mode: bool,
    /// Require telemetry bundles to exist
    pub require_telemetry_bundles: bool,
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
        }
    }
}

/// Main orchestrator that runs all gates
pub struct Orchestrator {
    config: OrchestratorConfig,
    gates: Vec<Box<dyn Gate>>,
    dependency_checker: DependencyChecker,
}

impl Orchestrator {
    /// Create a new orchestrator with standard gates
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

    /// Run dependency checks before gates
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
                .ok_or_else(|| anyhow::anyhow!("Unknown gate: {}", result.gate_id))?;

            if deps.severity == GateSeverity::Critical && result.degradation_level == 2 {
                if !self.config.allow_degraded_mode {
                    anyhow::bail!(
                        "Critical dependencies missing for gate '{}': {:?}",
                        result.gate_id,
                        result.messages
                    );
                }
            }
        }

        Ok(results)
    }

    /// Run all gates and return report
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

            let result = gate.check(&self.config).await;

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

/// Trait for promotion gates
#[async_trait::async_trait]
pub trait Gate: Send + Sync {
    /// Gate name
    fn name(&self) -> String;

    /// Check if gate passes
    async fn check(&self, config: &OrchestratorConfig) -> Result<()>;
}

/// Gate check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheckResult {
    pub passed: bool,
    pub message: String,
    pub evidence: Option<String>,
}
