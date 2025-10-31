#![cfg(all(test, feature = "extended-tests"))]

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