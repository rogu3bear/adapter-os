//! AdapterOS Determinism Runtime Guards
//!
//! This crate provides runtime guards to prevent developers from introducing
//! nondeterminism into the AdapterOS codebase. It detects common patterns that
//! violate determinism guarantees:
//!
//! - `tokio::task::spawn_blocking` calls
//! - Wall-clock time usage (`SystemTime::now()`, `Instant::now()`)
//! - Random number generation without proper seeding
//! - File I/O operations
//! - System calls
//!
//! # Usage
//!
//! Add to your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! adapteros-lint = { path = "crates/adapteros-lint" }
//! ```
//!
//! Then initialize guards:
//! ```rust
//! use adapteros_lint::{runtime_guards, strict_mode};
//!
//! // Initialize guards
//! runtime_guards::init_guards(runtime_guards::GuardConfig {
//!     enabled: true,
//!     strict_mode: false,
//!     max_violations: 10,
//!     log_violations: true,
//! });
//! ```

pub mod runtime_guards;
pub mod strict_mode;
