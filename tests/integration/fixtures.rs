#![cfg(all(test, feature = "extended-tests"))]

//! Test fixtures and data for adapterOS integration tests
//!
//! Provides pre-defined test data, configurations, and setup utilities
//! for consistent and reproducible integration testing.

use std::collections::HashMap;
use serde_json::json;
use std::path::PathBuf;

fn test_temp_root() -> PathBuf {
    let root = PathBuf::from("var/tmp");
    let _ = std::fs::create_dir_all(&root);
    root
}

/// Standard test repositories for different tenants
pub struct TestRepositories;

impl TestRepositories {
    /// Get repository configuration for tenant A
    pub fn tenant_a() -> serde_json::Value {
        let temp_dir = test_temp_root();
        json!({
            "repo_id": "tenant-a/test-repo",
            "path": temp_dir.join("tenant-a-repo").to_string_lossy(),
            "languages": ["rust", "python"],
            "default_branch": "main"
        })
    }

    /// Get repository configuration for tenant B
    pub fn tenant_b() -> serde_json::Value {
        let temp_dir = test_temp_root();
        json!({
            "repo_id": "tenant-b/test-repo",
            "path": temp_dir.join("tenant-b-repo").to_string_lossy(),
            "languages": ["go", "typescript"],
            "default_branch": "main"
        })
    }

    /// Get repository configuration for tenant C
    pub fn tenant_c() -> serde_json::Value {
        let temp_dir = test_temp_root();
        json!({
            "repo_id": "tenant-c/test-repo",
            "path": temp_dir.join("tenant-c-repo").to_string_lossy(),
            "languages": ["java", "kotlin"],
            "default_branch": "main"
        })
    }
}

/// Standard test policies for different scenarios
pub struct TestPolicies;

impl TestPolicies {
    /// Basic policy with evidence requirements
    pub fn basic_evidence_policy() -> serde_json::Value {
        json!({
            "min_evidence_spans": 2,
            "allow_auto_apply": false,
            "test_coverage_min": 0.8,
            "max_patch_size": 500
        })
    }

    /// Strict policy for regulated tenants
    pub fn strict_regulated_policy() -> serde_json::Value {
        json!({
            "min_evidence_spans": 5,
            "allow_auto_apply": false,
            "test_coverage_min": 0.95,
            "max_patch_size": 200,
            "secret_patterns": ["password", "token", "key"],
            "path_denylist": ["*.log", "*.tmp"]
        })
    }

    /// Resource-limited policy
    pub fn resource_limited_policy() -> serde_json::Value {
        json!({
            "min_evidence_spans": 1,
            "allow_auto_apply": true,
            "test_coverage_min": 0.5,
            "max_patch_size": 1000,
            "max_memory_mb": 512,
            "max_cpu_percent": 50.0
        })
    }
}

/// Standard inference test cases
pub struct TestInferenceCases;

impl TestInferenceCases {
    /// Simple inference request
    pub fn simple_request() -> serde_json::Value {
        json!({
            "cpid": "test_cp_v1",
            "prompt": "Explain how a function works",
            "max_tokens": 100,
            "require_evidence": false
        })
    }

    /// Evidence-based inference request
    pub fn evidence_request() -> serde_json::Value {
        json!({
            "cpid": "test_cp_v1",
            "prompt": "What are best practices for error handling?",
            "max_tokens": 150,
            "require_evidence": true
        })
    }

    /// Resource-intensive inference request
    pub fn resource_intensive_request() -> serde_json::Value {
        json!({
            "cpid": "test_cp_v1",
            "prompt": "Write a comprehensive analysis of machine learning algorithms",
            "max_tokens": 500,
            "require_evidence": true
        })
    }

    /// Concurrent inference requests for load testing
    pub fn concurrent_requests(count: usize) -> Vec<serde_json::Value> {
        (0..count)
            .map(|i| json!({
                "cpid": "test_cp_v1",
                "prompt": format!("Generate test content for iteration {}", i),
                "max_tokens": 50,
                "require_evidence": false
            }))
            .collect()
    }
}

/// Test data for policy validation
pub struct TestPolicyData;

impl TestPolicyData {
    /// Valid patch that should pass policy checks
    pub fn valid_patch() -> serde_json::Value {
        json!({
            "description": "Add error handling to function",
            "changes": [
                {
                    "file": "src/main.rs",
                    "line_start": 10,
                    "line_end": 15,
                    "content": "if let Err(e) = operation() {\n    eprintln!(\"Error: {}\", e);\n    return Err(e);\n}"
                }
            ],
            "evidence": [
                {
                    "doc_id": "error_handling_guide",
                    "score": 0.95,
                    "spans": ["Error handling patterns", "Result types"]
                }
            ]
        })
    }

    /// Invalid patch that should fail policy checks
    pub fn invalid_patch() -> serde_json::Value {
        json!({
            "description": "Add dangerous code",
            "changes": [
                {
                    "file": "src/main.rs",
                    "line_start": 1,
                    "line_end": 1,
                    "content": "unsafe { std::process::exit(1); }"
                }
            ],
            "evidence": [] // No evidence provided
        })
    }

    /// Patch with security violations
    pub fn security_violation_patch() -> serde_json::Value {
        json!({
            "description": "Add hardcoded credentials",
            "changes": [
                {
                    "file": "config.rs",
                    "line_start": 1,
                    "line_end": 1,
                    "content": "const API_KEY: &str = \"sk-1234567890abcdef\";"
                }
            ],
            "evidence": [
                {
                    "doc_id": "config_guide",
                    "score": 0.8,
                    "spans": ["Configuration management"]
                }
            ]
        })
    }
}

/// Resource usage patterns for testing
pub struct TestResourcePatterns;

impl TestResourcePatterns {
    /// Normal resource usage pattern
    pub fn normal_usage() -> Vec<(String, f64, f64, f64)> {
        vec![
            ("memory_mb".to_string(), 256.0, 512.0, 100.0),
            ("cpu_percent".to_string(), 10.0, 30.0, 5.0),
            ("storage_mb".to_string(), 50.0, 200.0, 20.0),
        ]
    }

    /// High resource usage pattern
    pub fn high_usage() -> Vec<(String, f64, f64, f64)> {
        vec![
            ("memory_mb".to_string(), 1024.0, 2048.0, 500.0),
            ("cpu_percent".to_string(), 50.0, 90.0, 20.0),
            ("storage_mb".to_string(), 500.0, 1000.0, 200.0),
        ]
    }

    /// Resource violation pattern (exceeds limits)
    pub fn violation_usage() -> Vec<(String, f64, f64, f64)> {
        vec![
            ("memory_mb".to_string(), 4096.0, 8192.0, 2000.0),
            ("cpu_percent".to_string(), 95.0, 100.0, 50.0),
            ("storage_mb".to_string(), 5000.0, 10000.0, 3000.0),
        ]
    }
}

/// Test tenant configurations
pub struct TestTenantConfigs;

impl TestTenantConfigs {
    /// Standard tenant A configuration
    pub fn tenant_a() -> HashMap<String, serde_json::Value> {
        let mut config = HashMap::new();
        config.insert("name".to_string(), json!("Tenant A"));
        config.insert("tier".to_string(), json!("standard"));
        config.insert("max_memory_mb".to_string(), json!(1024));
        config.insert("max_cpu_percent".to_string(), json!(50.0));
        config.insert("max_storage_mb".to_string(), json!(5000));
        config.insert("policies".to_string(), TestPolicies::basic_evidence_policy());
        config
    }

    /// Premium tenant B configuration
    pub fn tenant_b() -> HashMap<String, serde_json::Value> {
        let mut config = HashMap::new();
        config.insert("name".to_string(), json!("Tenant B"));
        config.insert("tier".to_string(), json!("premium"));
        config.insert("max_memory_mb".to_string(), json!(2048));
        config.insert("max_cpu_percent".to_string(), json!(75.0));
        config.insert("max_storage_mb".to_string(), json!(10000));
        config.insert("policies".to_string(), TestPolicies::strict_regulated_policy());
        config
    }

    /// Basic tenant C configuration
    pub fn tenant_c() -> HashMap<String, serde_json::Value> {
        let mut config = HashMap::new();
        config.insert("name".to_string(), json!("Tenant C"));
        config.insert("tier".to_string(), json!("basic"));
        config.insert("max_memory_mb".to_string(), json!(512));
        config.insert("max_cpu_percent".to_string(), json!(25.0));
        config.insert("max_storage_mb".to_string(), json!(1000));
        config.insert("policies".to_string(), TestPolicies::resource_limited_policy());
        config
    }
}

/// Deterministic test data generator
pub struct DeterministicDataGenerator {
    seed: u64,
}

impl DeterministicDataGenerator {
    /// Create a new deterministic data generator
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// Generate deterministic test data
    pub fn generate_data(&self, size: usize) -> Vec<u8> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.seed.hash(&mut hasher);
        let hash = hasher.finish();

        (0..size)
            .map(|i| ((hash.wrapping_add(i as u64)) % 256) as u8)
            .collect()
    }

    /// Generate deterministic string
    pub fn generate_string(&self, length: usize) -> String {
        self.generate_data(length)
            .iter()
            .map(|&b| (b % 26 + b'a') as char)
            .collect()
    }

    /// Generate deterministic number sequence
    pub fn generate_numbers(&self, count: usize, min: f64, max: f64) -> Vec<f64> {
        let data = self.generate_data(count * 8);
        data.chunks(8)
            .take(count)
            .map(|chunk| {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(chunk);
                let normalized = u64::from_le_bytes(bytes) as f64 / u64::MAX as f64;
                min + normalized * (max - min)
            })
            .collect()
    }
}

/// Test scenario definitions
pub struct TestScenarios;

impl TestScenarios {
    /// Basic tenant isolation scenario
    pub fn tenant_isolation() -> serde_json::Value {
        json!({
            "name": "tenant_isolation",
            "description": "Verify complete tenant data and resource isolation",
            "tenants": ["tenant_a", "tenant_b", "tenant_c"],
            "tests": [
                "data_access_isolation",
                "resource_usage_isolation",
                "repository_isolation",
                "adapter_isolation"
            ]
        })
    }

    /// Concurrent workload scenario
    pub fn concurrent_workload() -> serde_json::Value {
        json!({
            "name": "concurrent_workload",
            "description": "Test multiple tenants running inference simultaneously",
            "tenants": ["tenant_a", "tenant_b"],
            "workload": {
                "requests_per_tenant": 10,
                "concurrency_per_tenant": 3,
                "max_duration_seconds": 60
            },
            "validations": [
                "no_cross_tenant_interference",
                "resource_fairness",
                "performance_isolation"
            ]
        })
    }

    /// Policy enforcement scenario
    pub fn policy_enforcement() -> serde_json::Value {
        json!({
            "name": "policy_enforcement",
            "description": "Validate tenant-specific policy application",
            "tenants": ["tenant_a", "tenant_b"],
            "policies": [
                "evidence_requirements",
                "resource_limits",
                "security_constraints"
            ],
            "test_cases": [
                "valid_operations",
                "policy_violations",
                "edge_cases"
            ]
        })
    }
}
