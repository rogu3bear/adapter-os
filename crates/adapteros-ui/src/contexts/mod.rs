//! Context providers for cross-component state sharing
//!
//! This module provides Leptos context providers that expose shared state
//! to descendant components via `provide_context` / `expect_context`.

pub mod in_flight;

pub use in_flight::{use_in_flight, InFlightContext, InFlightProvider};
