#![cfg(all(test, feature = "extended-tests"))]
//! End-to-end testing framework for adapterOS
//!
//! This module provides comprehensive end-to-end tests that validate complete
//! inference pipelines, adapter lifecycle management, telemetry validation,
//! and failure scenario handling for the deterministic inference runtime.

pub mod adapter_lifecycle;
pub mod determinism_workflow;
pub mod failure_scenarios;
pub mod inference_pipeline;
pub mod orchestration;
pub mod telemetry_validation;
pub mod test_cluster;

pub use adapter_lifecycle::AdapterLifecycleTest;
pub use determinism_workflow::DeterminismWorkflowTest;
pub use failure_scenarios::FailureScenarioTest;
pub use inference_pipeline::InferencePipelineTest;
pub use orchestration::{TestConfig, TestEnvironment, TestOrchestrator};
pub use telemetry_validation::TelemetryValidationTest;

// Re-export test cluster infrastructure for multi-host determinism tests
pub use test_cluster::{
    BaselineMismatch, BaselineVerification, DeterminismDivergence, DeterminismReport,
    GoldenBaseline, TestCluster, TestClusterConfig, TestHost,
};
