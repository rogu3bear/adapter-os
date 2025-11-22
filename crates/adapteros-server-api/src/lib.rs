pub mod audit_helper;
pub mod auth;
pub mod cab_workflow;
pub mod errors;
pub mod handlers;
pub mod health;
pub mod ip_extraction;
pub mod lifecycle;
pub mod middleware;
pub mod middleware_security;
pub mod operation_tracker;
pub mod permissions;
pub mod plugin_registry;
pub mod routes;
pub mod security;
pub mod signing;
pub mod state;
pub mod supervisor_client;
pub mod telemetry;
pub mod telemetry_ext;
pub mod types;
pub mod uds_client;
pub mod validation;

pub use plugin_registry::PluginRegistry;

pub use lifecycle::{
    ShutdownCoordinator, ShutdownConfig, ShutdownError, ShutdownStatus, ShutdownProgress,
    LifecycleHook, LifecycleHookRegistry, LifecyclePhase, LifecycleContext,
};
pub use state::{AppState, CryptoState};
pub use telemetry::{
    spawn_telemetry_workers, SpanStatus, TelemetryWorkerConfig, TraceBuffer, TraceEvent,
    TraceSearchQuery,
};
pub use telemetry_ext::StackMetadataExt;
pub use types::*;
pub use uds_client::{UdsClient, UdsClientError};
