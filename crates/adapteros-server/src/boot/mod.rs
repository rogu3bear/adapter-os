//! Boot sequence module for AdapterOS control plane.
//!
//! This module contains utilities and abstractions for the boot sequence,
//! including timing tracking, background task management, and server binding.
//!
//! # Module Structure
//!
//! - `timings`: Boot phase timing tracker
//! - `tasks`: Background task spawner
//! - `server`: Server binding utilities
//! - `executor`: Deterministic executor initialization
//! - `database`: Database initialization and connection management
//! - `migrations`: Database migrations and seeding
//! - `security`: Security initialization and preflight checks
//! - `background_tasks`: Background task spawning logic
//! - `app_state`: AppState construction and initialization
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_server::boot::{BootTimings, BackgroundTaskSpawner, BindMode, bind_and_serve};
//!
//! // Track boot phase timings
//! let mut timings = BootTimings::new();
//! timings.start_phase("config");
//! // ... do config loading ...
//! timings.end_phase("config");
//! timings.log_summary();
//!
//! // Spawn background tasks
//! let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator);
//! spawner.spawn("Status writer", async move {
//!     // task logic
//! });
//! let coordinator = spawner.into_coordinator();
//!
//! // Bind and serve
//! let mode = BindMode::tcp(addr);
//! let config = ServerBindConfig { boot_state, shutdown_coordinator, drain_timeout, in_flight_requests };
//! bind_and_serve(mode, app, config).await?;
//! ```
pub mod api_config;
pub mod app_state;
pub mod background_tasks;

mod config;
pub mod database;
pub mod executor;
pub mod federation;
mod finalization;
pub mod invariants;
mod metrics;
pub mod migrations;
pub mod runtime;
pub mod security;
mod server;
mod tasks;
mod timings;

pub use app_state::build_app_state;
pub use config::{initialize_config, ConfigContext};
pub use database::{initialize_database, DatabaseContext};
pub use executor::{derive_executor_seed, initialize_executor, ExecutorContext};
pub use federation::{initialize_federation, FederationContext};
pub use finalization::{finalize_boot, write_boot_report, BindConfig, BootArtifacts};
pub use invariants::{
    boot_invariant_metrics, enforce_invariants, validate_boot_invariants, BootInvariantMetrics,
    InvariantReport, InvariantViolation,
};
pub use metrics::{initialize_metrics, MetricsContext};
pub use runtime::{initialize_runtime, RuntimeContext};
pub use security::{
    initialize_security, log_effective_config, run_preflight_checks, SecurityContext,
};
pub use server::{
    bind_and_serve, bind_error_exit_code, precheck_tcp_port, BindError, BindMode, ServerBindConfig,
};
pub use tasks::{BackgroundTaskSpawner, SpawnError, SpawnResult};
pub use timings::BootTimings;
