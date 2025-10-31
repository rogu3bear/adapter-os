#![cfg(all(test, feature = "extended-tests"))]

//! Test orchestration utilities for end-to-end testing
//!
//! Provides comprehensive test environment setup, lifecycle management,
//! and orchestration capabilities for running complex AdapterOS workflows.

use adapteros_core::{AosError, Result};
use adapteros_policy::PolicyEngine;
use adapteros_server_api::handlers::ApiHandler;
use adapteros_telemetry::{BundleStore, TelemetryWriter};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Test configuration for e2e scenarios
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Base directory for test artifacts
    pub test_dir: PathBuf,
    /// Telemetry output directory
    pub telemetry_dir: PathBuf,
    /// Model registry path
    pub model_registry: PathBuf,
    /// Policy configuration
    pub policy_config: serde_json::Value,
    /// Test timeout duration
    pub timeout: Duration,
    /// Enable verbose logging
    pub verbose: bool,
    /// CPID for deterministic execution
    pub cpid: String,
    /// Tenant configuration
    pub tenant_id: String,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            test_dir: PathBuf::from("/tmp/adapteros_e2e"),
            telemetry_dir: PathBuf::from("/tmp/adapteros_e2e/telemetry"),
            model_registry: PathBuf::from("/tmp/adapteros_e2e/models"),
            policy_config: serde_json::json!({
                "min_evidence_spans": 1,
                "require_evidence": true,
                "abstain_threshold": 0.55
            }),
            timeout: Duration::from_secs(300), // 5 minutes
            verbose: true,
            cpid: "e2e_test_cpid_001".to_string(),
            tenant_id: "e2e_test_tenant".to_string(),
        }
    }
}

/// Test environment state
#[derive(Debug)]
pub struct TestEnvironment {
    /// Configuration
    config: TestConfig,
    /// Telemetry writer
    telemetry: Option<TelemetryWriter>,
    /// Policy engine
    policy_engine: Option<PolicyEngine>,
    /// API handler
    api_handler: Option<ApiHandler>,
    /// Bundle store for telemetry
    bundle_store: Option<BundleStore>,
    /// Test artifacts and cleanup tracking
    artifacts: Vec<PathBuf>,
    /// Start time for timeout tracking
    start_time: Instant,
}

impl TestEnvironment {
    /// Create a new test environment
    pub async fn new(config: TestConfig) -> Result<Self> {
        // Create directories
        std::fs::create_dir_all(&config.test_dir)?;
        std::fs::create_dir_all(&config.telemetry_dir)?;
        std::fs::create_dir_all(&config.model_registry)?;

        // Initialize telemetry
        let telemetry = TelemetryWriter::new(
            &config.telemetry_dir,
            1000,             // max events per bundle
            10 * 1024 * 1024, // 10MB per bundle
        )?;

        // Initialize bundle store
        let bundle_store = BundleStore::new(&config.telemetry_dir)?;

        Ok(Self {
            config,
            telemetry: Some(telemetry),
            policy_engine: None,
            api_handler: None,
            bundle_store: Some(bundle_store),
            artifacts: Vec::new(),
            start_time: Instant::now(),
        })
    }

    /// Initialize policy engine
    pub async fn init_policy_engine(&mut self) -> Result<()> {
        let policy_engine = PolicyEngine::new(&self.config.policy_config)?;
        self.policy_engine = Some(policy_engine);
        Ok(())
    }

    /// Initialize API handler
    pub async fn init_api_handler(&mut self) -> Result<()> {
        let api_handler = ApiHandler::new(
            self.telemetry.as_ref().unwrap().clone(),
            self.policy_engine.as_ref().unwrap().clone(),
        )?;
        self.api_handler = Some(api_handler);
        Ok(())
    }

    /// Get telemetry writer
    pub fn telemetry(&self) -> &TelemetryWriter {
        self.telemetry.as_ref().unwrap()
    }

    /// Get policy engine
    pub fn policy_engine(&self) -> &PolicyEngine {
        self.policy_engine.as_ref().unwrap()
    }

    /// Get API handler
    pub fn api_handler(&self) -> &ApiHandler {
        self.api_handler.as_ref().unwrap()
    }

    /// Get bundle store
    pub fn bundle_store(&self) -> &BundleStore {
        self.bundle_store.as_ref().unwrap()
    }

    /// Check if test has timed out
    pub fn has_timed_out(&self) -> bool {
        self.start_time.elapsed() > self.config.timeout
    }

    /// Track artifact for cleanup
    pub fn track_artifact<P: AsRef<Path>>(&mut self, path: P) {
        self.artifacts.push(path.as_ref().to_path_buf());
    }

    /// Cleanup test artifacts
    pub async fn cleanup(&mut self) -> Result<()> {
        for artifact in &self.artifacts {
            if artifact.exists() {
                if artifact.is_dir() {
                    std::fs::remove_dir_all(artifact)?;
                } else {
                    std::fs::remove_file(artifact)?;
                }
            }
        }
        self.artifacts.clear();
        Ok(())
    }
}

/// Test orchestrator for managing complex e2e workflows
pub struct TestOrchestrator {
    /// Test environments by name
    environments: HashMap<String, Arc<Mutex<TestEnvironment>>>,
    /// Global test configuration
    config: TestConfig,
    /// Test results
    results: HashMap<String, TestResult>,
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub test_name: String,
    pub status: TestStatus,
    pub duration: Duration,
    pub error: Option<String>,
    pub artifacts: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestStatus {
    Pending,
    Running,
    Passed,
    Failed,
    Skipped,
    TimedOut,
}

impl TestOrchestrator {
    /// Create a new test orchestrator
    pub fn new(config: TestConfig) -> Self {
        Self {
            environments: HashMap::new(),
            config,
            results: HashMap::new(),
        }
    }

    /// Create a named test environment
    pub async fn create_environment(&mut self, name: &str) -> Result<()> {
        let mut env_config = self.config.clone();
        env_config.test_dir = self.config.test_dir.join(name);
        env_config.telemetry_dir = env_config.test_dir.join("telemetry");
        env_config.model_registry = env_config.test_dir.join("models");

        let env = TestEnvironment::new(env_config).await?;
        self.environments
            .insert(name.to_string(), Arc::new(Mutex::new(env)));
        Ok(())
    }

    /// Get test environment
    pub fn get_environment(&self, name: &str) -> Option<&Arc<Mutex<TestEnvironment>>> {
        self.environments.get(name)
    }

    /// Run a test with orchestration
    pub async fn run_test<F, Fut>(&mut self, test_name: &str, test_fn: F) -> Result<TestResult>
    where
        F: FnOnce(Arc<Mutex<TestEnvironment>>) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let start_time = Instant::now();
        let test_name = test_name.to_string();

        // Mark as running
        self.results.insert(
            test_name.clone(),
            TestResult {
                test_name: test_name.clone(),
                status: TestStatus::Running,
                duration: Duration::ZERO,
                error: None,
                artifacts: Vec::new(),
            },
        );

        // Create environment if it doesn't exist
        if !self.environments.contains_key(&test_name) {
            self.create_environment(&test_name).await?;
        }

        let env = self.environments.get(&test_name).unwrap().clone();

        // Initialize environment
        {
            let mut env_lock = env.lock().await;
            env_lock.init_policy_engine().await?;
            env_lock.init_api_handler().await?;
        }

        // Run the test
        let result = match tokio::time::timeout(self.config.timeout, test_fn(env.clone())).await {
            Ok(Ok(())) => TestResult {
                test_name: test_name.clone(),
                status: TestStatus::Passed,
                duration: start_time.elapsed(),
                error: None,
                artifacts: Vec::new(),
            },
            Ok(Err(e)) => TestResult {
                test_name: test_name.clone(),
                status: TestStatus::Failed,
                duration: start_time.elapsed(),
                error: Some(e.to_string()),
                artifacts: Vec::new(),
            },
            Err(_) => TestResult {
                test_name: test_name.clone(),
                status: TestStatus::TimedOut,
                duration: start_time.elapsed(),
                error: Some("Test timed out".to_string()),
                artifacts: Vec::new(),
            },
        };

        // Cleanup environment
        {
            let mut env_lock = env.lock().await;
            env_lock.cleanup().await?;
        }

        // Store result
        self.results.insert(test_name, result.clone());

        Ok(result)
    }

    /// Get test results summary
    pub fn get_summary(&self) -> TestSummary {
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut timed_out = 0;

        for result in self.results.values() {
            match result.status {
                TestStatus::Passed => passed += 1,
                TestStatus::Failed => failed += 1,
                TestStatus::Skipped => skipped += 1,
                TestStatus::TimedOut => timed_out += 1,
                _ => {}
            }
        }

        TestSummary {
            total: self.results.len(),
            passed,
            failed,
            skipped,
            timed_out,
        }
    }

    /// Get all test results
    pub fn get_results(&self) -> &HashMap<String, TestResult> {
        &self.results
    }
}

#[derive(Debug, Clone)]
pub struct TestSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub timed_out: usize,
}

impl TestSummary {
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.passed as f64) / (self.total as f64)
        }
    }

    pub fn has_failures(&self) -> bool {
        self.failed > 0 || self.timed_out > 0
    }
}

/// Helper macro for defining e2e tests
#[macro_export]
macro_rules! e2e_test {
    ($name:ident, $config:expr, $body:block) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            let mut orchestrator = TestOrchestrator::new($config);
            let result = orchestrator.run_test(stringify!($name), |env| async move {
                $body
                Ok(())
            }).await?;

            match result.status {
                TestStatus::Passed => Ok(()),
                TestStatus::Failed => Err(AosError::Test(format!("Test {} failed: {}", result.test_name, result.error.unwrap_or_default()))),
                TestStatus::TimedOut => Err(AosError::Test(format!("Test {} timed out", result.test_name))),
                _ => Err(AosError::Test(format!("Test {} had unexpected status: {:?}", result.test_name, result.status))),
            }
        }
    };
}
