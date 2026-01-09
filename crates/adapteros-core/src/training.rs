//! Training types - re-exported from canonical adapteros-types
//!
//! This module re-exports the canonical training types defined in adapteros-types.
//! The canonical definitions in adapteros-types consolidate previous definitions
//! from adapteros-core, adapteros-orchestrator, and adapteros-api-types into
//! a single source of truth.
//!
//! # Type Exports
//!
//! - [`TrainingJob`] - Complete training job information with metadata
//! - [`TrainingJobStatus`] - Training job lifecycle state machine
//! - [`TrainingConfig`] - Training hyperparameters and configuration
//! - [`TrainingTemplate`] - Reusable training templates

// Re-export canonical types from adapteros-types
pub use adapteros_types::training::{
    OptimizerConfigSummary, TrainingConfig, TrainingJob, TrainingJobStatus, TrainingReportCurves,
    TrainingReportMetricDefinitions, TrainingReportSummary, TrainingReportV1, TrainingTemplate,
    TRAINING_REPORT_VERSION,
};
