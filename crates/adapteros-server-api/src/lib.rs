pub mod api_error;
pub mod audit_helper;
pub mod auth;
pub mod auth_common;
pub mod boot_state;
pub mod cab_workflow;
pub mod caching;
pub mod chat_context;
pub mod compression;
pub mod config;
pub mod error_helpers;
pub mod errors;
pub mod event_bus;
pub mod handlers;
pub mod health;
pub mod inference_core;
pub mod ip_extraction;
pub mod lifecycle;
pub mod load_coordinator;
pub mod middleware;
pub mod middleware_security;
pub mod operation_tracker;
pub mod permissions;
pub mod plugin_registry;
pub mod request_id;
pub mod routes;
pub mod runtime_mode;
pub mod security;
pub mod services;
pub mod settings_loader;
pub mod signing;
pub mod state;
pub mod supervisor_client;
pub mod telemetry;
pub mod telemetry_ext;
pub mod types;
pub mod uds_client;
pub mod validation;
pub mod versioning;
pub mod worker_health;

pub use auth::Claims;
pub use event_bus::EventBus;
pub use load_coordinator::{LoadCoordinator, LoadCoordinatorMetrics};
pub use plugin_registry::PluginRegistry;

pub use config::PathsConfig;
pub use inference_core::InferenceCore;
pub use lifecycle::{
    LifecycleContext, LifecycleHook, LifecycleHookRegistry, LifecyclePhase, ShutdownConfig,
    ShutdownCoordinator, ShutdownError, ShutdownProgress, ShutdownStatus,
};
pub use state::{ApiConfig, AppState, CryptoState};
pub use telemetry::{
    spawn_telemetry_workers, SpanStatus, TelemetryWorkerConfig, TraceBuffer, TraceEvent,
    TraceSearchQuery,
};
pub use telemetry_ext::StackMetadataExt;
pub use types::*;
pub use uds_client::{enter_routed_context, exit_routed_context, UdsClient, UdsClientError};
pub use worker_health::{
    HealthConfig, WorkerHealthMonitor, WorkerHealthStatus, WorkerHealthSummary,
};

// Export the router builder function
pub use routes::build as create_app;
