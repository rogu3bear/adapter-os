//! Repository pattern implementations
//!
//! This module provides high-level repositories that encapsulate
//! KV storage operations and replace SQL queries.

pub mod adapter;
pub mod adapter_version;
pub mod dataset;
pub mod rag;
pub mod replay;
pub mod telemetry;

pub use adapter::{AdapterRepository, PaginatedResult};
pub use adapter_version::AdapterVersionRepository;
pub use dataset::DatasetRepository;
pub use rag::RagRepository;
pub use replay::ReplayRepository;
pub use telemetry::TelemetryRepository;
