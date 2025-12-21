//! Shared telemetry metric types for AdapterOS
//!
//! This crate provides the core metric data structures used across
//! the telemetry system and API layer, breaking the dependency cycle
//! between adapteros-telemetry and adapteros-api-types.

pub mod bundle;
pub mod metrics;

pub use bundle::{BundleMetadata, BundleType, RetentionBundleMetadata, TraceBundleMetadata};
pub use metrics::*;
