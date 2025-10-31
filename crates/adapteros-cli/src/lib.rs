//! Library interface for AdapterOS CLI components.

pub mod app;
pub mod cli;
pub mod cli_telemetry;
pub mod commands;
pub mod error_codes;
pub mod logging;
pub mod output;

pub use app::{run, BackendType, Cli, Commands};
pub use commands::infer;
