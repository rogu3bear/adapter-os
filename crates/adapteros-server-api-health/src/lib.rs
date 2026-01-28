//! Health check endpoints for adapteros server
//!
//! This crate provides health check endpoints for the AdapterOS control plane.
//! Split from adapteros-server-api for faster incremental builds.
//!
//! Handlers use the spoke pattern: they depend on adapteros-server-api for
//! shared types (AppState, BootState) while keeping handler logic in this crate.

pub mod handlers;
pub mod routes;

pub use handlers::{
    get_invariant_status, get_status, health, ready, BootPhaseDuration, DrainSection,
    InvariantStatusResponse, InvariantViolationDto, LifecycleStatusResponse, MaintenanceSection,
    ReadinessMode, ReadyMetrics, ReadyzCheck, ReadyzChecks, ReadyzResponse, RestartSection,
    SystemReadySection, TelemetrySection,
};
pub use routes::health_routes;
