//! Test module for adapteros-storage
//!
//! Comprehensive test suite covering:
//! - Quota management (quota_tests)
//! - Cleanup operations (cleanup_tests)
//! - Storage monitoring (monitor_tests)
//! - Policy enforcement (policy_tests)
//! - Integration tests (integration_tests)

use crate::platform::common::PlatformUtils;
use adapteros_core::Result;
use tempfile::{Builder, TempDir};

pub(super) fn new_test_tempdir() -> Result<TempDir> {
    let root = PlatformUtils::temp_dir();
    std::fs::create_dir_all(&root)?;
    Ok(Builder::new().prefix("aos-test-").tempdir_in(&root)?)
}

mod cleanup_tests;
mod integration_tests;
mod monitor_tests;
mod policy_tests;
mod quota_tests;
