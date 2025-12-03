//! Common test utilities for KV integration tests
//!
//! This module provides reusable test utilities for testing the KV storage
//! backend integration, including database setup, storage mode management,
//! cleanup utilities, and test data factories.
//!
//! # Usage
//!
//! ```no_run
//! use adapteros_db::tests::common::{TestDb, TestAdapterFactory};
//!
//! #[tokio::test]
//! async fn test_my_feature() {
//!     let test_db = TestDb::new().await;
//!     let adapter = TestAdapterFactory::default().build();
//!     // ... test code
//!     test_db.cleanup().await;
//! }
//! ```

pub mod assertions;
pub mod cleanup;
pub mod db_helpers;
pub mod factories;

// Re-export commonly used items
pub use assertions::{assert_adapter_fields_match, assert_adapters_equal};
pub use cleanup::{cleanup_test_db, cleanup_test_files};
pub use db_helpers::{create_test_db, create_test_db_with_kv, create_test_db_with_mode, TestDb};
pub use factories::{TestAdapterFactory, TestStackFactory, TestTenantFactory};
