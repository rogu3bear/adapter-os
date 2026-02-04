#![cfg(all(test, feature = "extended-tests"))]

//! adapterOS Integration Tests
//!
//! Comprehensive integration tests covering multi-tenant scenarios,
//! policy enforcement, resource isolation, alert streaming, and operation tracking.

pub mod alert_streaming;
pub mod operation_tracking;
pub mod tenant_isolation;
pub mod concurrent_workloads;
pub mod cross_tenant_interference;
pub mod policy_enforcement;
pub mod resource_isolation;
pub mod test_utils;
pub mod fixtures;
pub mod chunked_upload_idempotency;
pub mod dev_login_dataset_creation;

// Re-export commonly used test utilities
pub use test_utils::*;
pub use fixtures::*;
