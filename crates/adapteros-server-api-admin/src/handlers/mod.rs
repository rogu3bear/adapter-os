//! Admin handlers module
//!
//! Organizes administrative handlers into submodules:
//! - `users`: User management (list users)
//! - `lifecycle`: Server lifecycle control (shutdown, restart, drain)
//! - `services`: Service control (start, stop, restart services)
//! - `plugins`: Plugin management (enable, disable, status)
//! - `settings`: System settings management
//! - `status`: Simple status and configuration endpoints

pub mod lifecycle;
pub mod plugins;
pub mod services;
pub mod settings;
pub mod status;
pub mod users;

// Re-export all handlers for convenience
pub use lifecycle::*;
pub use plugins::*;
pub use services::*;
pub use settings::*;
pub use status::*;
pub use users::*;
