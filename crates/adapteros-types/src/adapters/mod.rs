//! Adapter lifecycle and metadata types

pub mod info;
pub mod metadata;
pub mod record;
pub mod stack;

pub use info::{AdapterInfo, AdapterMetrics, AdapterState};
pub use metadata::{AdapterMetadata, LifecycleState, RegisterAdapterRequest};
pub use record::AdapterRecord;
pub use stack::{CreateStackRequest, StackRecord};
