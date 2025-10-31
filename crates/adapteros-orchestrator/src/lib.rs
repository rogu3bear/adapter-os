//! Orchestrator for AdapterOS promotion gates
//!
//! Runs all quality gates and reports pass/fail status with evidence.

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[cfg(feature = "cdp")]
pub mod code_jobs;
pub mod dataset_builder;
#[cfg(feature = "federation")]
pub mod federation_daemon;
pub mod gates;
pub mod report;
pub mod supervisor;
pub mod training;

#[cfg(feature = "cdp")]
pub use code_jobs::{CodeJobManager, CommitDeltaJob, ScanRepositoryJob, UpdateIndicesJob};
pub use dataset_builder::{build_from_directory, DatasetBuilderConfig};
#[cfg(feature = "federation")]
pub use federation_daemon::{
    FederationDaemon, FederationDaemonConfig, FederationVerificationReport,
};
pub use gates::*;
pub use report::{GateReport, GateResult, ReportFormat};
pub use training::{
    TrainingConfig, TrainingJob, TrainingJobStatus, TrainingService, TrainingTemplate,
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
    /// Base model path for training
    pub base_model: String,
    /// TTL in hours for ephemeral adapters
    pub ephemeral_adapter_ttl_hours: i32,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            continue_on_error: false,
            cpid: String::new(),
            db_path: "var/aos-cp.sqlite3".to_string(),
            bundles_path: "/srv/aos/bundles".to_string(),
            manifests_path: "manifests".to_string(),
            base_model: "models/qwen2.5-7b-mlx".to_string(),
            ephemeral_adapter_ttl_hours: 24,
        }
    }
}

/// Main orchestrator that runs all gates
pub struct Orchestrator {
    config: OrchestratorConfig,
    gates: Vec<Box<dyn Gate>>,
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

        Self { config, gates }
    }

    /// Run all gates and return report
    pub async fn run(&self) -> Result<GateReport> {
        let mut report = GateReport::new(self.config.cpid.clone());

        for gate in &self.gates {
            let gate_name = gate.name();
            println!("Running gate: {}...", gate_name);

            let result = gate.check(&self.config).await;

            match &result {
                Ok(()) => {
                    println!("  ✓ {} PASSED", gate_name);
                    report.add_result(gate_name, GateResult::passed());
                }
                Err(e) => {
                    println!("  ✗ {} FAILED: {}", gate_name, e);
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
