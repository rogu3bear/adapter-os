//! # Gates Subsystem
//!
//! This module provides preflight validation gates for the orchestrator.
//! Gates are checks that must pass before certain operations can proceed.
//!
//! ## Overview
//!
//! Gates perform quality checks and validations before allowing operations
//! such as adapter promotion, deployment, or system state changes. Each gate
//! implements the [`Gate`] trait and can validate different aspects of the system:
//! determinism, security, performance, compliance, and more.
//!
//! ## How Gates Work
//!
//! 1. **Preflight Checks**: Gates run before operations to ensure prerequisites are met
//! 2. **Dependency Validation**: Each gate can declare required dependencies (tools, paths)
//! 3. **Graceful Degradation**: Gates can run in degraded mode when optional dependencies are missing
//! 4. **Timeout Protection**: Each gate has a configurable timeout to prevent hangs
//! 5. **Evidence Collection**: Gates can provide evidence (logs, metrics) for audit trails
//!
//! ## Available Gates
//!
//! - [`DeterminismGate`] - Ensures determinism requirements are met (reproducible builds, seed usage)
//! - [`MetricsGate`] - Validates metrics collection and telemetry infrastructure
//! - [`MetallibGate`] - Checks Metal shader library compilation and availability
//! - [`SbomGate`] - Validates Software Bill of Materials generation and compliance
//! - [`PerformanceGate`] - Ensures performance benchmarks and thresholds are met
//! - [`SecurityGate`] - Validates security checks (audits, vulnerability scanning)
//! - [`TelemetryGate`] - Checks telemetry bundle generation and export
//!
//! ## Using Gates
//!
//! Gates are typically used through the [`Orchestrator`](crate::Orchestrator), which:
//!
//! 1. Runs dependency checks for all gates
//! 2. Executes gates sequentially (or stops on first failure if configured)
//! 3. Collects results into a [`GateReport`](crate::GateReport)
//!
//! ## Adding New Gates
//!
//! To add a new gate:
//!
//! 1. Create a new module in this directory (e.g., `my_gate.rs`)
//! 2. Implement the [`Gate`](crate::Gate) trait:
//!    ```rust,no_run
//!    # use adapteros_orchestrator::{Gate, OrchestratorConfig};
//!    # use anyhow::Result;
//!    # use async_trait::async_trait;
//!    #
//!    # struct MyGate;
//!    #
//!    #[async_trait]
//!    impl Gate for MyGate {
//!        fn name(&self) -> String {
//!            "my_gate".to_string()
//!        }
//!
//!        async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
//!            // Perform validation checks
//!            Ok(())
//!        }
//!    }
//!    ```
//! 3. Register the gate in [`Orchestrator::new()`](crate::Orchestrator::new)
//! 4. Add dependency definitions in [`dependencies`] module if needed
//!
//! ## Dependency Management
//!
//! Gates can declare dependencies using [`GateDependencies`](dependencies::GateDependencies):
//!
//! - **Required paths**: Must exist or gate fails
//! - **Optional paths**: Can fall back to alternatives
//! - **Required tools**: CLI tools that must be available
//! - **Severity**: Critical gates block promotion; warning gates log but continue

pub mod dependencies;
pub mod determinism;
pub mod metallib;
pub mod metrics;
pub mod performance;
pub mod sbom;
pub mod security;
pub mod telemetry;

pub use dependencies::{
    DependencyCheckResult, DependencyChecker, GateDependencies, GateSeverity, PathResolution,
    PathStatus, ToolStatus,
};
pub use determinism::DeterminismGate;
pub use metallib::MetallibGate;
pub use metrics::MetricsGate;
pub use performance::PerformanceGate;
pub use sbom::SbomGate;
pub use security::SecurityGate;
pub use telemetry::TelemetryGate;
