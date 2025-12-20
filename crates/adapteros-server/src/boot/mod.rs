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

mod server;
mod tasks;
mod timings;

pub use server::{bind_and_serve, BindError, BindMode, ServerBindConfig};
pub use tasks::{BackgroundTaskSpawner, SpawnError, SpawnResult};
pub use timings::BootTimings;
