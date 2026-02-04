//! Context providers for cross-component state sharing.
//!
//! This module provides Leptos context providers that expose shared state
//! to descendant components via `provide_context` / `expect_context`.
//!
//! # Provider Hierarchy
//!
//! Contexts should be mounted near the application root in a consistent order
//! to ensure dependent contexts can access their prerequisites:
//!
//! 1. **InFlightProvider** - Tracks in-flight API requests for loading indicators.
//!
//! # Cleanup
//!
//! All providers register cleanup via `on_cleanup` to release resources when
//! their owning scope is disposed. Components using these contexts do not need
//! to manually unsubscribe; Leptos reactive ownership handles disposal.

pub mod in_flight;

pub use in_flight::{try_use_in_flight, use_in_flight, InFlightContext, InFlightProvider};
