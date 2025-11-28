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
pub mod repo_url;
pub mod training_service;

// Re-export commonly used types
pub use adapter_service::{
    AdapterHealthResponse, AdapterService, DefaultAdapterService, LifecycleTransitionResult,
};
pub use training_service::{
    DefaultTrainingService, TrainingCapacityInfo, TrainingService, TrainingValidationResult,
};
