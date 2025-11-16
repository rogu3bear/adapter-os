<<<<<<< HEAD
//! Integration tests for AdapterOS server API
//!
//! These tests verify end-to-end functionality across multiple components.
//!
//! Citations:
//! - Server API structure: [source: crates/adapteros-server-api/src/lib.rs]
//! - Test utilities: [source: tests/unit/async_utils.rs]

pub mod alert_streaming;
pub mod operation_tracking;
=======
//! AdapterOS Integration Tests
//!
//! Comprehensive integration tests for multi-tenant scenarios,
//! policy enforcement, and resource isolation.

pub mod tenant_isolation;
pub mod concurrent_workloads;
pub mod cross_tenant_interference;
pub mod policy_enforcement;
pub mod resource_isolation;
pub mod test_utils;
pub mod fixtures;

// Re-export commonly used test utilities
pub use test_utils::*;
pub use fixtures::*;
>>>>>>> integration-branch
