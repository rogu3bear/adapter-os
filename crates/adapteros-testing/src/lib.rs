//! Unified testing framework for AdapterOS
//!
//! Provides a centralized testing framework that consolidates all testing
//! patterns across the system with consistent setup, teardown, and assertions.

mod assertions;
mod step_executor;
pub mod types;
pub mod unified_framework;

pub use types::*;
pub use unified_framework::{TestingFramework, UnifiedTestingFramework};
