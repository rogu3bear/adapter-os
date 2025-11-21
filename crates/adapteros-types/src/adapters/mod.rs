//! Adapter lifecycle and metadata types

pub mod info;
pub mod metadata;

pub use info::{AdapterInfo, AdapterMetrics, AdapterState};
pub use metadata::{AdapterMetadata, LifecycleState, RegisterAdapterRequest};
