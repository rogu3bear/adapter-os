pub mod auth;
pub mod cleanup;
pub mod fixtures;
pub mod fixtures_consolidated;
pub mod migration_setup;
pub mod test_harness;

// Re-export consolidated fixtures for easier access
pub use fixtures_consolidated::{
    TestAdapterFactory, TestAppStateBuilder, TestAssertions, TestAuth, TestDatasetFactory,
    TestDbBuilder, TestDbConfig, TestTrainingJobFactory, TestUser,
};
