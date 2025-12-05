//! Shared test cleanup utilities for ensuring proper resource management
//! 【2025-11-10†refactor(tests)†consolidate-test-cleanup】
//!
//! Provides comprehensive cleanup functions for:
//! - Database files and connections
//! - Temporary directories and files
//! - Environment variables
//! - Process resources
//!
//! All cleanup functions are designed to work for both passing and failing tests.

#![allow(dead_code)]

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Test cleanup context that tracks resources to clean up
#[derive(Clone)]
pub struct TestCleanupContext {
    /// Database files to remove
    pub db_files: Vec<PathBuf>,
    /// Temporary directories to remove
    pub temp_dirs: Vec<PathBuf>,
    /// Environment variables to restore (original value)
    pub env_vars: HashMap<String, Option<String>>,
    /// Custom cleanup functions to run
    pub cleanup_fns: Vec<Arc<dyn Fn() -> Result<(), Box<dyn std::error::Error>> + Send + Sync>>,
}

impl Default for TestCleanupContext {
    fn default() -> Self {
        Self {
            db_files: Vec::new(),
            temp_dirs: Vec::new(),
            env_vars: HashMap::new(),
            cleanup_fns: Vec::new(),
        }
    }
}

impl TestCleanupContext {
    /// Create a new cleanup context
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a database file to be cleaned up
    pub fn add_db_file<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.db_files.push(path.as_ref().to_path_buf());
        self
    }

    /// Add a temporary directory to be cleaned up
    pub fn add_temp_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.temp_dirs.push(path.as_ref().to_path_buf());
        self
    }

    /// Add an environment variable to be restored
    pub fn add_env_var(mut self, key: &str) -> Self {
        let original_value = env::var(key).ok();
        self.env_vars.insert(key.to_string(), original_value);
        self
    }

    /// Add a custom cleanup function
    pub fn add_cleanup_fn<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), Box<dyn std::error::Error>> + Send + Sync + 'static,
    {
        self.cleanup_fns.push(Arc::new(f));
        self
    }

    /// Execute all cleanup operations
    pub fn cleanup(self) -> Result<(), Box<dyn std::error::Error>> {
        // Clean up database files
        for db_file in &self.db_files {
            if db_file.exists() {
                if let Err(e) = fs::remove_file(db_file) {
                    eprintln!(
                        "Warning: Failed to remove database file {:?}: {}",
                        db_file, e
                    );
                }
            }
        }

        // Clean up temporary directories
        for temp_dir in &self.temp_dirs {
            if temp_dir.exists() {
                if let Err(e) = fs::remove_dir_all(temp_dir) {
                    eprintln!(
                        "Warning: Failed to remove temp directory {:?}: {}",
                        temp_dir, e
                    );
                }
            }
        }

        // Restore environment variables
        for (key, original_value) in &self.env_vars {
            match original_value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
        }

        // Run custom cleanup functions
        for cleanup_fn in &self.cleanup_fns {
            if let Err(e) = cleanup_fn() {
                eprintln!("Warning: Custom cleanup function failed: {}", e);
            }
        }

        Ok(())
    }
}

/// Global cleanup registry for tracking test resources across the test suite
static CLEANUP_REGISTRY: std::sync::LazyLock<Arc<Mutex<Vec<TestCleanupContext>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(Vec::new())));

/// Register a cleanup context for global cleanup
pub async fn register_cleanup_context(context: TestCleanupContext) {
    let mut registry = CLEANUP_REGISTRY.lock().await;
    registry.push(context);
}

/// Execute global cleanup for all registered contexts
pub async fn execute_global_cleanup() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = CLEANUP_REGISTRY.lock().await;
    let contexts = std::mem::take(&mut *registry);

    for context in contexts {
        if let Err(e) = context.cleanup() {
            eprintln!("Warning: Global cleanup failed: {}", e);
        }
    }

    Ok(())
}

/// Cleanup database files and connections
pub fn cleanup_database_files<P: AsRef<Path>>(
    paths: &[P],
) -> Result<(), Box<dyn std::error::Error>> {
    for path in paths {
        let path = path.as_ref();
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// Cleanup temporary directories recursively
pub fn cleanup_temp_dirs<P: AsRef<Path>>(paths: &[P]) -> Result<(), Box<dyn std::error::Error>> {
    for path in paths {
        let path = path.as_ref();
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
    }
    Ok(())
}

/// Restore environment variables to their original values
pub fn restore_env_vars(
    vars: &HashMap<String, Option<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    for (key, original_value) in vars {
        match original_value {
            Some(value) => env::set_var(key, value),
            None => env::remove_var(key),
        }
    }
    Ok(())
}

/// Create a test database path with unique identifier
pub fn create_test_db_path(prefix: &str) -> PathBuf {
    use uuid::Uuid;
    let uuid = Uuid::new_v4().simple();
    PathBuf::from(format!("/tmp/{}_{}.db", prefix, uuid))
}

/// Create a test temporary directory with unique identifier
pub fn create_test_temp_dir(prefix: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    use uuid::Uuid;
    let uuid = Uuid::new_v4().simple();
    let path = PathBuf::from(format!("/tmp/{}_{}", prefix, uuid));
    fs::create_dir_all(&path)?;
    Ok(path)
}

/// Test cleanup guard that automatically cleans up when dropped
pub struct TestCleanupGuard {
    context: TestCleanupContext,
}

impl TestCleanupGuard {
    /// Create a new cleanup guard
    pub fn new(context: TestCleanupContext) -> Self {
        Self { context }
    }

    /// Add a database file to be cleaned up
    pub fn add_db_file<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.context.db_files.push(path.as_ref().to_path_buf());
        self
    }

    /// Add a temporary directory to be cleaned up
    pub fn add_temp_dir<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.context.temp_dirs.push(path.as_ref().to_path_buf());
        self
    }

    /// Add an environment variable to be restored
    pub fn add_env_var(&mut self, key: &str) -> &mut Self {
        let original_value = std::env::var(key).ok();
        self.context
            .env_vars
            .insert(key.to_string(), original_value);
        self
    }

    /// Add a custom cleanup function
    pub fn add_cleanup_fn<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() -> Result<(), Box<dyn std::error::Error>> + Send + Sync + 'static,
    {
        self.context.cleanup_fns.push(Arc::new(f));
        self
    }

    /// Manually trigger cleanup
    pub fn cleanup_now(self) -> Result<(), Box<dyn std::error::Error>> {
        self.context.clone().cleanup()
    }
}

impl Drop for TestCleanupGuard {
    fn drop(&mut self) {
        if let Err(e) = self.context.clone().cleanup() {
            eprintln!("Warning: Automatic cleanup failed: {}", e);
        }
    }
}

/// Convenience macro for creating a test cleanup guard
#[macro_export]
macro_rules! test_cleanup {
    () => {
        $crate::common::cleanup::TestCleanupGuard::new(
            $crate::common::cleanup::TestCleanupContext::new()
        )
    };
    ($($tt:tt)*) => {
        $crate::common::cleanup::TestCleanupGuard::new(
            $crate::common::cleanup::TestCleanupContext::new()
        )
        $($tt)*
    };
}

/// Convenience macro for setting up test environment with cleanup
#[macro_export]
macro_rules! setup_test_env {
    ($prefix:expr) => {{
        use std::env;
        use $crate::common::cleanup::*;

        // Create unique database path
        let db_path = create_test_db_path($prefix);

        // Create unique temp directory
        let temp_dir = create_test_temp_dir($prefix).expect("Failed to create temp dir");

        // Track environment variables to restore
        let mut env_vars = std::collections::HashMap::new();
        if let Ok(val) = env::var("DATABASE_URL") {
            env_vars.insert("DATABASE_URL".to_string(), Some(val));
        }
        if let Ok(val) = env::var("TEST_ENV") {
            env_vars.insert("TEST_ENV".to_string(), Some(val));
        }

        // Set test environment variables
        env::set_var("DATABASE_URL", db_path.to_str().unwrap());
        env::set_var("TEST_ENV", "true");

        // Create cleanup guard with mutable builder pattern
        let mut guard = TestCleanupGuard::new(TestCleanupContext::new());
        guard.add_db_file(&db_path);
        guard.add_temp_dir(&temp_dir);
        guard.add_cleanup_fn(move || {
            restore_env_vars(&env_vars)?;
            Ok(())
        });

        (db_path, temp_dir, guard)
    }};
}
