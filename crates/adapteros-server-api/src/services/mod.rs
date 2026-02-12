//! Service modules for reusable business logic
//!
//! This module contains service layer abstractions that extract business logic
//! from HTTP handlers. Services encapsulate domain logic, state management,
//! and orchestration while handlers focus on HTTP concerns.
//!
//! Pattern:
//! - Define trait for service operations
//! - Implement default service using AppState
//! - Handlers call service methods instead of direct business logic
//!
//! Benefits:
//! - Separation of concerns (HTTP vs business logic)
//! - Testability (mock services in tests)
//! - Reusability (services can be used by multiple handlers)
//! - Maintainability (business logic in one place)

pub mod adapter_service;
pub mod dataset_domain;
pub mod error_alert_evaluator;
pub mod key_distribution;
pub mod repo_url;
pub mod synthesis;
pub mod training_dataset;
pub mod training_service;

// Re-export commonly used types
pub use adapter_service::{
    AdapterHealthResponse, AdapterService, DefaultAdapterService, LifecycleTransitionResult,
};
pub use dataset_domain::{
    CanonicalRow, DatasetDomain, DatasetDomainService, DatasetManifest, DatasetVersionDescriptor,
    NormalizationNotes, RawDialect, RawFileDescriptor, RawIngestRequest, SamplingConfig,
    SplitStats,
};
pub use error_alert_evaluator::ErrorAlertEvaluator;
pub use synthesis::{
    compute_synthesis_model_hash, derive_synthesis_seed, derive_synthesis_seed_bytes_v1,
    derive_synthesis_seed_u64_v1, DeterministicSynthesisConfig, SynthesisProvenance,
    SynthesisService,
};
pub use training_dataset::{
    DatasetFromCollectionParams, DatasetFromDocumentIdsParams, DatasetFromUploadParams,
    DatasetFromUploadedDocumentParams, DefaultTrainingDatasetService, TrainingDatasetService,
};
pub use training_service::{
    DefaultTrainingService, TrainingCapacityInfo, TrainingService, TrainingValidationResult,
};
