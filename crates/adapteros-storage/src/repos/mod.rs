//! Repository pattern implementations
//!
//! This module provides high-level repositories that encapsulate
//! KV storage operations and replace SQL queries.

pub mod adapter;

pub use adapter::{AdapterRepository, PaginatedResult};
