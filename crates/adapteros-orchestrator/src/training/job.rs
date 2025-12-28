//! Training job types and re-exports from adapteros_types.

// Re-export canonical types from adapteros_types
pub use adapteros_types::training::{
    DataLineageMode, DatasetVersionSelection, DatasetVersionTrustSnapshot, LoraTier,
    TrainingBackendKind, TrainingBackendPolicy, TrainingConfig, TrainingJob, TrainingJobStatus,
    TrainingTemplate,
};
