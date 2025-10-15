//! Component Isolation Helpers
//!
//! This module provides utilities for testing AdapterOS components in isolation,
//! with controlled dependencies and minimal external interactions.
//!
//! ## Key Features
//!
//! - **Dependency Injection**: Mock external dependencies
//! - **Sandboxing**: Isolate file system and network operations
//! - **Resource Management**: Control resource allocation and cleanup
//! - **Deterministic Execution**: Ensure predictable test environments
//!
//! ## Usage
//!
//! ```rust
//! use tests_unit::isolation::*;
//!
//! #[test]
//! fn test_component_in_isolation() {
//!     let sandbox = TestSandbox::new();
//!     let isolated = IsolatedComponent::new(sandbox);
//!
//!     // Test component with isolated dependencies
//!     let result = isolated.run_test_operation();
//!     assert!(result.is_ok());
//! }
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use adapteros_core::{B3Hash, derive_seed};

/// Test sandbox for isolating file system operations
pub struct TestSandbox {
    root: PathBuf,
    files: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
    seed: B3Hash,
}

impl TestSandbox {
    /// Create a new test sandbox with a temporary root directory
    pub fn new() -> Self {
        let seed = B3Hash::hash(b"test_sandbox");
        let root = std::env::temp_dir().join(format!("adapteros_test_{}", seed.to_hex()));
        std::fs::create_dir_all(&root).expect("Failed to create test sandbox");

        Self {
            root,
            files: Arc::new(Mutex::new(HashMap::new())),
            seed,
        }
    }

    /// Create a new test sandbox with a specific seed for deterministic behavior
    pub fn with_seed(seed: u64) -> Self {
        let seed_hash = B3Hash::hash(&seed.to_le_bytes());
        let root = std::env::temp_dir().join(format!("adapteros_test_{}", seed_hash.to_hex()));
        std::fs::create_dir_all(&root).expect("Failed to create test sandbox");

        Self {
            root,
            files: Arc::new(Mutex::new(HashMap::new())),
            seed: seed_hash,
        }
    }

    /// Get the sandbox root path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create a file in the sandbox with deterministic content
    pub fn create_file(&self, relative_path: &str, size: usize) -> PathBuf {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create parent directories");
        }

        // Generate deterministic content based on path and seed
        let content_seed = derive_seed(&self.seed, &format!("file:{}", relative_path));
        let content: Vec<u8> = (0..size)
            .map(|i| content_seed[i % 32])
            .collect();

        std::fs::write(&path, &content).expect("Failed to write test file");
        self.files.lock().unwrap().insert(path.clone(), content);
        path
    }

    /// Read a file from the sandbox
    pub fn read_file(&self, relative_path: &str) -> Option<Vec<u8>> {
        let path = self.root.join(relative_path);
        self.files.lock().unwrap().get(&path).cloned()
    }

    /// Create a directory in the sandbox
    pub fn create_dir(&self, relative_path: &str) -> PathBuf {
        let path = self.root.join(relative_path);
        std::fs::create_dir_all(&path).expect("Failed to create test directory");
        path
    }

    /// Clean up the sandbox
    pub fn cleanup(self) {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root).expect("Failed to cleanup test sandbox");
        }
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

/// Isolated component wrapper for testing components with mocked dependencies
pub struct IsolatedComponent<T> {
    component: T,
    sandbox: TestSandbox,
    mocks: HashMap<String, Box<dyn std::any::Any + Send + Sync>>,
}

impl<T> IsolatedComponent<T> {
    /// Create a new isolated component
    pub fn new(component: T) -> Self {
        Self {
            component,
            sandbox: TestSandbox::new(),
            mocks: HashMap::new(),
        }
    }

    /// Create a new isolated component with a specific seed
    pub fn with_seed(component: T, seed: u64) -> Self {
        Self {
            component,
            sandbox: TestSandbox::with_seed(seed),
            mocks: HashMap::new(),
        }
    }

    /// Register a mock dependency
    pub fn register_mock<M: 'static + Send + Sync>(&mut self, name: &str, mock: M) {
        self.mocks.insert(name.to_string(), Box::new(mock));
    }

    /// Get a mock dependency
    pub fn get_mock<M: 'static>(&self, name: &str) -> Option<&M> {
        self.mocks.get(name)?
            .downcast_ref::<M>()
    }

    /// Get mutable access to a mock dependency
    pub fn get_mock_mut<M: 'static>(&mut self, name: &str) -> Option<&mut M> {
        self.mocks.get_mut(name)?
            .downcast_mut::<M>()
    }

    /// Get the test sandbox
    pub fn sandbox(&self) -> &TestSandbox {
        &self.sandbox
    }

    /// Get mutable access to the component
    pub fn component_mut(&mut self) -> &mut T {
        &mut self.component
    }

    /// Get access to the component
    pub fn component(&self) -> &T {
        &self.component
    }

    /// Consume the isolated component and return the sandbox for cleanup
    pub fn into_parts(self) -> (T, TestSandbox) {
        (self.component, self.sandbox)
    }
}

/// Resource pool for managing test resources
pub struct ResourcePool<R> {
    resources: Arc<Mutex<Vec<R>>>,
    factory: Box<dyn Fn() -> R + Send + Sync>,
}

impl<R> ResourcePool<R> {
    /// Create a new resource pool with a factory function
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn() -> R + Send + Sync + 'static,
    {
        Self {
            resources: Arc::new(Mutex::new(Vec::new())),
            factory: Box::new(factory),
        }
    }

    /// Acquire a resource from the pool
    pub fn acquire(&self) -> ResourceGuard<R> {
        let resource = self.resources.lock().unwrap().pop()
            .unwrap_or_else(|| (self.factory)());

        ResourceGuard {
            resource: Some(resource),
            pool: Arc::clone(&self.resources),
        }
    }

    /// Pre-populate the pool with resources
    pub fn preallocate(&self, count: usize) {
        let mut resources = self.resources.lock().unwrap();
        for _ in 0..count {
            resources.push((self.factory)());
        }
    }
}

/// RAII guard for pooled resources
pub struct ResourceGuard<R> {
    resource: Option<R>,
    pool: Arc<Mutex<Vec<R>>>,
}

impl<R> ResourceGuard<R> {
    /// Get access to the resource
    pub fn get(&self) -> &R {
        self.resource.as_ref().unwrap()
    }

    /// Get mutable access to the resource
    pub fn get_mut(&mut self) -> &mut R {
        self.resource.as_mut().unwrap()
    }

    /// Consume the guard and return the resource
    pub fn into_inner(mut self) -> R {
        self.resource.take().unwrap()
    }
}

impl<R> Drop for ResourceGuard<R> {
    fn drop(&mut self) {
        if let Some(resource) = self.resource.take() {
            self.pool.lock().unwrap().push(resource);
        }
    }
}

impl<R> std::ops::Deref for ResourceGuard<R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<R> std::ops::DerefMut for ResourceGuard<R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

/// Dependency injection container for isolated testing
pub struct DependencyContainer {
    services: HashMap<String, Box<dyn std::any::Any + Send + Sync>>,
}

impl DependencyContainer {
    /// Create a new dependency container
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    /// Register a service
    pub fn register<S: 'static + Send + Sync>(&mut self, name: &str, service: S) {
        self.services.insert(name.to_string(), Box::new(service));
    }

    /// Get a service
    pub fn get<S: 'static>(&self, name: &str) -> Option<&S> {
        self.services.get(name)?
            .downcast_ref::<S>()
    }

    /// Get a mutable service
    pub fn get_mut<S: 'static>(&mut self, name: &str) -> Option<&mut S> {
        self.services.get_mut(name)?
            .downcast_mut::<S>()
    }

    /// Check if a service is registered
    pub fn has(&self, name: &str) -> bool {
        self.services.contains_key(name)
    }

    /// Remove a service
    pub fn remove(&mut self, name: &str) -> bool {
        self.services.remove(name).is_some()
    }
}

/// Test environment configuration
#[derive(Debug, Clone)]
pub struct TestEnvironment {
    pub seed: u64,
    pub temp_dir: PathBuf,
    pub log_level: String,
    pub features: Vec<String>,
}

impl TestEnvironment {
    /// Create a new test environment
    pub fn new() -> Self {
        Self {
            seed: 42,
            temp_dir: std::env::temp_dir().join("adapteros_test_env"),
            log_level: "error".to_string(),
            features: Vec::new(),
        }
    }

    /// Set the random seed for deterministic testing
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Set the temporary directory
    pub fn with_temp_dir(mut self, dir: PathBuf) -> Self {
        self.temp_dir = dir;
        self
    }

    /// Set the log level
    pub fn with_log_level(mut self, level: &str) -> Self {
        self.log_level = level.to_string();
        self
    }

    /// Add a feature flag
    pub fn with_feature(mut self, feature: &str) -> Self {
        self.features.push(feature.to_string());
        self
    }

    /// Setup the test environment
    pub fn setup(&self) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&self.temp_dir)?;
        std::env::set_var("RUST_LOG", &self.log_level);
        Ok(())
    }

    /// Cleanup the test environment
    pub fn cleanup(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.temp_dir.exists() {
            std::fs::remove_dir_all(&self.temp_dir)?;
        }
        Ok(())
    }
}

/// Test harness for running isolated component tests
pub struct TestHarness {
    environment: TestEnvironment,
    container: DependencyContainer,
    sandbox: TestSandbox,
}

impl TestHarness {
    /// Create a new test harness
    pub fn new() -> Self {
        Self {
            environment: TestEnvironment::new(),
            container: DependencyContainer::new(),
            sandbox: TestSandbox::new(),
        }
    }

    /// Configure the test environment
    pub fn with_environment(mut self, env: TestEnvironment) -> Self {
        self.environment = env;
        self.sandbox = TestSandbox::with_seed(self.environment.seed);
        self
    }

    /// Register a service in the dependency container
    pub fn register_service<S: 'static + Send + Sync>(&mut self, name: &str, service: S) {
        self.container.register(name, service);
    }

    /// Get a service from the dependency container
    pub fn get_service<S: 'static>(&self, name: &str) -> Option<&S> {
        self.container.get(name)
    }

    /// Get the test sandbox
    pub fn sandbox(&self) -> &TestSandbox {
        &self.sandbox
    }

    /// Setup the test harness
    pub fn setup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.environment.setup()?;
        Ok(())
    }

    /// Cleanup the test harness
    pub fn cleanup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.environment.cleanup()?;
        Ok(())
    }

    /// Run a test function with the harness
    pub fn run_test<F, R>(&mut self, test_fn: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.setup().expect("Failed to setup test harness");
        let result = test_fn(self);
        self.cleanup().expect("Failed to cleanup test harness");
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_file_operations() {
        let sandbox = TestSandbox::with_seed(123);

        // Create a test file
        let file_path = sandbox.create_file("test.txt", 100);
        assert!(file_path.exists());

        // Read the file
        let content = sandbox.read_file("test.txt");
        assert!(content.is_some());
        assert_eq!(content.unwrap().len(), 100);

        // Cleanup
        sandbox.cleanup();
        assert!(!file_path.exists());
    }

    #[test]
    fn test_isolated_component_with_mocks() {
        let component = "test_component".to_string();
        let mut isolated = IsolatedComponent::with_seed(component, 456);

        // Register a mock
        isolated.register_mock("mock_service", 42i32);

        // Get the mock
        let mock_value = isolated.get_mock::<i32>("mock_service");
        assert_eq!(mock_value, Some(&42));

        let (component, sandbox) = isolated.into_parts();
        assert_eq!(component, "test_component");
        sandbox.cleanup();
    }

    #[test]
    fn test_resource_pool() {
        let pool = ResourcePool::new(|| vec![1, 2, 3]);

        // Acquire a resource
        let guard = pool.acquire();
        assert_eq!(*guard, vec![1, 2, 3]);

        // Resource is returned to pool when guard is dropped
        drop(guard);

        // Acquire again (should get the same resource)
        let guard2 = pool.acquire();
        assert_eq!(*guard2, vec![1, 2, 3]);
    }

    #[test]
    fn test_dependency_container() {
        let mut container = DependencyContainer::new();

        // Register a service
        container.register("test_service", "hello world".to_string());

        // Get the service
        let service = container.get::<String>("test_service");
        assert_eq!(service, Some(&"hello world".to_string()));

        // Check existence
        assert!(container.has("test_service"));
        assert!(!container.has("nonexistent"));

        // Remove service
        assert!(container.remove("test_service"));
        assert!(!container.has("test_service"));
    }

    #[test]
    fn test_test_harness() {
        let mut harness = TestHarness::new();

        harness.register_service("config", "test_config".to_string());

        let result = harness.run_test(|h| {
            let config = h.get_service::<String>("config");
            assert_eq!(config, Some(&"test_config".to_string()));
            "test_passed"
        });

        assert_eq!(result, "test_passed");
    }
}</code>
