//! End-to-end testing framework for AdapterOS
//!
//! This module provides comprehensive end-to-end tests that validate complete
//! inference pipelines, adapter lifecycle management, telemetry validation,
//! and failure scenario handling for the deterministic inference runtime.

pub mod orchestration;
pub mod inference_pipeline;
pub mod adapter_lifecycle;
pub mod telemetry_validation;
pub mod failure_scenarios;
pub mod determinism_workflow;

pub use orchestration::{TestOrchestrator, TestEnvironment, TestConfig};
pub use inference_pipeline::InferencePipelineTest;
pub use adapter_lifecycle::AdapterLifecycleTest;
pub use telemetry_validation::TelemetryValidationTest;
pub use failure_scenarios::FailureScenarioTest;
pub use determinism_workflow::DeterminismWorkflowTest;