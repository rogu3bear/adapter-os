//! Multi-Host Test Cluster Infrastructure
//!
//! Provides infrastructure for running determinism tests across multiple
//! simulated hosts to verify identical outputs.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

/// Configuration for test cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestClusterConfig {
    pub host_count: usize,
    pub global_seed: [u8; 32],
    pub db_path_template: String,
    pub verbose: bool,
}

/// Individual test host in the cluster
pub struct TestHost {
    pub id: usize,
    pub temp_dir: TempDir,
    pub db_path: PathBuf,
    pub results: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl TestHost {
    /// Create a new test host
    pub fn new(id: usize) -> Result<Self> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

        let db_path = temp_dir.path().join(format!("host_{}.db", id));

        Ok(Self {
            id,
            temp_dir,
            db_path,
            results: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Store a result from computation
    pub async fn store_result(&self, key: String, value: Vec<u8>) {
        let mut results = self.results.lock().await;
        results.insert(key, value);
    }

    /// Get a stored result
    pub async fn get_result(&self, key: &str) -> Option<Vec<u8>> {
        let results = self.results.lock().await;
        results.get(key).cloned()
    }

    /// Get all stored results
    pub async fn all_results(&self) -> HashMap<String, Vec<u8>> {
        let results = self.results.lock().await;
        results.clone()
    }
}

/// Test cluster simulating multiple hosts
pub struct TestCluster {
    pub config: TestClusterConfig,
    pub hosts: Vec<TestHost>,
}

impl TestCluster {
    /// Create a new test cluster
    pub async fn new(config: TestClusterConfig) -> Result<Self> {
        let mut hosts = Vec::new();

        for id in 0..config.host_count {
            let host = TestHost::new(id)?;
            hosts.push(host);
        }

        if config.verbose {
            println!("Created test cluster with {} hosts", config.host_count);
        }

        Ok(Self { config, hosts })
    }

    /// Run a test function on all hosts in parallel
    pub async fn run_on_all_hosts<F, Fut>(&self, test_fn: F) -> Result<()>
    where
        F: Fn(&TestHost) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send,
    {
        let test_fn = Arc::new(test_fn);
        let mut handles = Vec::new();

        for host in &self.hosts {
            // SAFETY: We're creating a raw pointer to host and dereferencing it in the spawned task.
            // This is safe because:
            // 1. The TestCluster owns all hosts and won't be dropped until this method completes
            // 2. We wait for all spawned tasks to complete before returning
            // 3. The hosts Vec is not modified during task execution
            let host_ptr = host as *const TestHost;
            let test_fn = test_fn.clone();

            let handle = tokio::spawn(async move {
                let host = unsafe { &*host_ptr };
                test_fn(host).await
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle
                .await
                .map_err(|e| AosError::Internal(format!("Task join error: {}", e)))??;
        }

        Ok(())
    }

    /// Verify determinism for a specific result key across all hosts
    pub async fn verify_determinism(&self, result_key: &str) -> Result<DeterminismReport> {
        let mut host_results = Vec::new();

        // Collect results from all hosts
        for host in &self.hosts {
            let result = host.get_result(result_key).await.ok_or_else(|| {
                AosError::Validation(format!(
                    "Host {} missing result for key '{}'",
                    host.id, result_key
                ))
            })?;

            let hash = hex::encode(blake3::hash(&result).as_bytes());
            host_results.push((host.id, result, hash));
        }

        // Compare all results to first host (baseline)
        let (baseline_host, baseline_result, baseline_hash) = &host_results[0];
        let mut divergences = Vec::new();

        for (host_id, result, hash) in &host_results[1..] {
            if result != baseline_result {
                divergences.push(DeterminismDivergence {
                    baseline_host: *baseline_host,
                    divergent_host: *host_id,
                    result_key: result_key.to_string(),
                    baseline_hash: baseline_hash.clone(),
                    divergent_hash: hash.clone(),
                });
            }
        }

        Ok(DeterminismReport {
            result_key: result_key.to_string(),
            host_count: self.config.host_count,
            deterministic: divergences.is_empty(),
            divergences,
        })
    }

    /// Verify determinism for all result keys
    pub async fn verify_all_results(&self) -> Result<Vec<DeterminismReport>> {
        // Get all keys from first host
        let first_host = &self.hosts[0];
        let all_results = first_host.all_results().await;
        let keys: Vec<String> = all_results.keys().cloned().collect();

        let mut reports = Vec::new();
        for key in keys {
            let report = self.verify_determinism(&key).await?;
            reports.push(report);
        }

        Ok(reports)
    }
}

/// Report on determinism verification for a single result key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismReport {
    pub result_key: String,
    pub host_count: usize,
    pub deterministic: bool,
    pub divergences: Vec<DeterminismDivergence>,
}

impl DeterminismReport {
    /// Check if determinism test passed
    pub fn passed(&self) -> bool {
        self.deterministic
    }

    /// Get a summary string
    pub fn summary(&self) -> String {
        if self.deterministic {
            format!(
                "✓ '{}': Deterministic across {} hosts",
                self.result_key, self.host_count
            )
        } else {
            format!(
                "✗ '{}': {} divergence(s) detected",
                self.result_key,
                self.divergences.len()
            )
        }
    }
}

/// Details about a determinism divergence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismDivergence {
    pub baseline_host: usize,
    pub divergent_host: usize,
    pub result_key: String,
    pub baseline_hash: String,
    pub divergent_hash: String,
}

/// Golden baseline for regression testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenBaseline {
    pub test_name: String,
    pub created_at: String,
    pub global_seed: [u8; 32],
    pub expected_outputs: HashMap<String, String>,
}

impl GoldenBaseline {
    /// Create golden baseline from a test cluster
    pub async fn from_cluster(test_name: String, cluster: &TestCluster) -> Result<Self> {
        let first_host = &cluster.hosts[0];
        let results = first_host.all_results().await;

        let mut expected_outputs = HashMap::new();
        for (key, value) in results {
            let hash = hex::encode(blake3::hash(&value).as_bytes());
            expected_outputs.insert(key, hash);
        }

        Ok(Self {
            test_name,
            created_at: chrono::Utc::now().to_rfc3339(),
            global_seed: cluster.config.global_seed,
            expected_outputs,
        })
    }

    /// Save golden baseline to file
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(AosError::Serialization)?;

        fs::write(path, json)
            .map_err(|e| AosError::Io(format!("Failed to write baseline: {}", e)))?;

        Ok(())
    }

    /// Load golden baseline from file
    pub fn load(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)
            .map_err(|e| AosError::Io(format!("Failed to read baseline: {}", e)))?;

        serde_json::from_str(&json).map_err(AosError::Serialization)
    }

    /// Verify cluster outputs match this golden baseline
    pub async fn verify_cluster(&self, cluster: &TestCluster) -> Result<BaselineVerification> {
        let first_host = &cluster.hosts[0];
        let actual_results = first_host.all_results().await;

        let mut mismatches = Vec::new();

        // Check all expected outputs
        for (key, expected_hash) in &self.expected_outputs {
            if let Some(actual_value) = actual_results.get(key) {
                let actual_hash = hex::encode(blake3::hash(actual_value).as_bytes());

                if &actual_hash != expected_hash {
                    mismatches.push(BaselineMismatch {
                        key: key.clone(),
                        expected_hash: expected_hash.clone(),
                        actual_hash,
                    });
                }
            } else {
                mismatches.push(BaselineMismatch {
                    key: key.clone(),
                    expected_hash: expected_hash.clone(),
                    actual_hash: "MISSING".to_string(),
                });
            }
        }

        // Check for unexpected outputs
        for key in actual_results.keys() {
            if !self.expected_outputs.contains_key(key) {
                let actual_value = actual_results.get(key).unwrap();
                let actual_hash = hex::encode(blake3::hash(actual_value).as_bytes());

                mismatches.push(BaselineMismatch {
                    key: key.clone(),
                    expected_hash: "NOT_IN_BASELINE".to_string(),
                    actual_hash,
                });
            }
        }

        Ok(BaselineVerification {
            test_name: self.test_name.clone(),
            passed: mismatches.is_empty(),
            mismatches,
        })
    }
}

/// Result of verifying against golden baseline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineVerification {
    pub test_name: String,
    pub passed: bool,
    pub mismatches: Vec<BaselineMismatch>,
}

impl BaselineVerification {
    pub fn passed(&self) -> bool {
        self.passed
    }

    pub fn summary(&self) -> String {
        if self.passed {
            format!(
                "✓ Golden baseline verification passed for '{}'",
                self.test_name
            )
        } else {
            format!(
                "✗ Golden baseline verification failed: {} mismatch(es)",
                self.mismatches.len()
            )
        }
    }
}

/// Details about a baseline mismatch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineMismatch {
    pub key: String,
    pub expected_hash: String,
    pub actual_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cluster_creation() {
        let config = TestClusterConfig {
            host_count: 3,
            global_seed: [42u8; 32],
            db_path_template: "test_{}.db".to_string(),
            verbose: false,
        };

        let cluster = TestCluster::new(config).await.unwrap();
        assert_eq!(cluster.hosts.len(), 3);
    }

    #[tokio::test]
    async fn test_host_result_storage() {
        let host = TestHost::new(0).unwrap();

        host.store_result("test_key".to_string(), vec![1, 2, 3])
            .await;

        let result = host.get_result("test_key").await;
        assert_eq!(result, Some(vec![1, 2, 3]));
    }

    #[tokio::test]
    async fn test_determinism_verification() {
        let config = TestClusterConfig {
            host_count: 3,
            global_seed: [42u8; 32],
            db_path_template: "test_{}.db".to_string(),
            verbose: false,
        };

        let cluster = TestCluster::new(config).await.unwrap();

        // Store identical results on all hosts
        let data = vec![1, 2, 3, 4, 5];
        for host in &cluster.hosts {
            host.store_result("test_output".to_string(), data.clone())
                .await;
        }

        // Verify determinism
        let report = cluster.verify_determinism("test_output").await.unwrap();
        assert!(report.passed());
        assert_eq!(report.divergences.len(), 0);
    }

    #[tokio::test]
    async fn test_divergence_detection() {
        let config = TestClusterConfig {
            host_count: 3,
            global_seed: [42u8; 32],
            db_path_template: "test_{}.db".to_string(),
            verbose: false,
        };

        let cluster = TestCluster::new(config).await.unwrap();

        // Store different results on hosts
        cluster.hosts[0]
            .store_result("test_output".to_string(), vec![1, 2, 3])
            .await;
        cluster.hosts[1]
            .store_result("test_output".to_string(), vec![1, 2, 3])
            .await;
        cluster.hosts[2]
            .store_result("test_output".to_string(), vec![4, 5, 6])
            .await; // Different!

        // Verify determinism
        let report = cluster.verify_determinism("test_output").await.unwrap();
        assert!(!report.passed());
        assert_eq!(report.divergences.len(), 1);
        assert_eq!(report.divergences[0].divergent_host, 2);
    }
}
