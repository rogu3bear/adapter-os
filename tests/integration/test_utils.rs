#![cfg(all(test, feature = "extended-tests"))]
//! Reusable test utilities for adapterOS integration tests
//!
//! Provides common functionality for multi-tenant testing, resource monitoring,
//! policy validation, and isolation checking.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use adapteros_client::{CpClient, DefaultClient, LoginRequest};
use adapteros_api_types::{HealthResponse, TenantResponse, RepoResponse, ListAdaptersResponse};

/// Configuration for test tenants
#[derive(Debug, Clone)]
pub struct TenantConfig {
    pub id: String,
    pub name: String,
    pub token: String,
    pub base_url: String,
}

/// Wrapper for tenant-specific operations
pub struct TestTenant {
    config: TenantConfig,
    client: DefaultClient,
}

impl TestTenant {
    /// Create a new test tenant wrapper
    pub fn new(config: TenantConfig) -> Self {
        let client = DefaultClient::new(config.base_url.clone());
        Self { config, client }
    }

    /// Get tenant configuration
    pub fn config(&self) -> &TenantConfig {
        &self.config
    }

    /// Get authenticated client
    pub fn client(&self) -> &DefaultClient {
        &self.client
    }

    /// Perform health check
    pub async fn health_check(&self) -> Result<HealthResponse> {
        self.client.health().await
    }

    /// List tenant repositories
    pub async fn list_repos(&self) -> Result<Vec<RepoResponse>> {
        self.client.list_repos().await
    }

    /// List tenant adapters
    pub async fn list_adapters(&self, tenant_id: &str) -> Result<ListAdaptersResponse> {
        self.client.list_adapters(tenant_id).await
    }

    /// Run inference request
    pub async fn run_inference(&self, request: serde_json::Value) -> Result<serde_json::Value> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/inference", self.config.base_url);

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Inference request failed: {}", response.status()));
        }

        let body: serde_json::Value = response.json().await?;
        Ok(body)
    }
}

/// Multi-tenant test harness for setup and teardown
pub struct MultiTenantHarness {
    tenants: HashMap<String, TestTenant>,
    base_url: String,
}

impl MultiTenantHarness {
    /// Create a new multi-tenant harness
    pub fn new(base_url: String) -> Self {
        Self {
            tenants: HashMap::new(),
            base_url,
        }
    }

    /// Add a test tenant
    pub fn add_tenant(&mut self, config: TenantConfig) {
        let tenant = TestTenant::new(config.clone());
        self.tenants.insert(config.id.clone(), tenant);
    }

    /// Get a tenant by ID
    pub fn get_tenant(&self, tenant_id: &str) -> Option<&TestTenant> {
        self.tenants.get(tenant_id)
    }

    /// Get all tenants
    pub fn tenants(&self) -> &HashMap<String, TestTenant> {
        &self.tenants
    }

    /// Setup all tenants (verify connectivity)
    pub async fn setup(&self) -> Result<()> {
        for (tenant_id, tenant) in &self.tenants {
            println!("Setting up tenant: {}", tenant_id);
            tenant.health_check().await?;
        }
        Ok(())
    }

    /// Cleanup test resources
    pub async fn cleanup(&self) -> Result<()> {
        // Cleanup logic would go here
        // For now, just log completion
        println!("Multi-tenant test cleanup completed");
        Ok(())
    }
}

/// Resource usage monitor
#[derive(Debug, Clone)]
pub struct ResourceMetrics {
    pub memory_mb: f64,
    pub cpu_percent: f64,
    pub storage_mb: f64,
    pub timestamp: Instant,
}

pub struct ResourceMonitor {
    metrics: Arc<Mutex<HashMap<String, Vec<ResourceMetrics>>>>,
}

impl ResourceMonitor {
    /// Create a new resource monitor
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record resource usage for a tenant
    pub fn record_usage(&self, tenant_id: &str, metrics: ResourceMetrics) {
        let mut metrics_map = self.metrics.lock().unwrap();
        metrics_map.entry(tenant_id.to_string())
            .or_insert_with(Vec::new)
            .push(metrics);
    }

    /// Get resource usage history for a tenant
    pub fn get_usage(&self, tenant_id: &str) -> Vec<ResourceMetrics> {
        self.metrics.lock().unwrap()
            .get(tenant_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Calculate average resource usage
    pub fn average_usage(&self, tenant_id: &str) -> Option<ResourceMetrics> {
        let usage = self.get_usage(tenant_id);
        if usage.is_empty() {
            return None;
        }

        let total_memory: f64 = usage.iter().map(|m| m.memory_mb).sum();
        let total_cpu: f64 = usage.iter().map(|m| m.cpu_percent).sum();
        let total_storage: f64 = usage.iter().map(|m| m.storage_mb).sum();
        let count = usage.len() as f64;

        Some(ResourceMetrics {
            memory_mb: total_memory / count,
            cpu_percent: total_cpu / count,
            storage_mb: total_storage / count,
            timestamp: Instant::now(),
        })
    }
}

/// Policy enforcement validator
pub struct PolicyValidator {
    violations: Arc<Mutex<Vec<PolicyViolation>>>,
}

#[derive(Debug, Clone)]
pub struct PolicyViolation {
    pub tenant_id: String,
    pub policy_type: String,
    pub description: String,
    pub severity: String,
    pub timestamp: Instant,
}

impl PolicyValidator {
    /// Create a new policy validator
    pub fn new() -> Self {
        Self {
            violations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Record a policy violation
    pub fn record_violation(&self, violation: PolicyViolation) {
        self.violations.lock().unwrap().push(violation);
    }

    /// Get all violations for a tenant
    pub fn get_violations(&self, tenant_id: &str) -> Vec<PolicyViolation> {
        self.violations.lock().unwrap()
            .iter()
            .filter(|v| v.tenant_id == tenant_id)
            .cloned()
            .collect()
    }

    /// Check if any violations occurred
    pub fn has_violations(&self, tenant_id: &str) -> bool {
        !self.get_violations(tenant_id).is_empty()
    }

    /// Clear all violations
    pub fn clear_violations(&self) {
        self.violations.lock().unwrap().clear();
    }
}

/// Isolation checker for tenant boundaries
pub struct IsolationChecker {
    access_attempts: Arc<Mutex<Vec<IsolationAttempt>>>,
}

#[derive(Debug, Clone)]
pub struct IsolationAttempt {
    pub source_tenant: String,
    pub target_tenant: String,
    pub resource_type: String,
    pub allowed: bool,
    pub timestamp: Instant,
}

impl IsolationChecker {
    /// Create a new isolation checker
    pub fn new() -> Self {
        Self {
            access_attempts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Record an isolation attempt
    pub fn record_attempt(&self, attempt: IsolationAttempt) {
        self.access_attempts.lock().unwrap().push(attempt);
    }

    /// Check if cross-tenant access occurred
    pub fn cross_tenant_access_detected(&self) -> bool {
        self.access_attempts.lock().unwrap()
            .iter()
            .any(|attempt| attempt.source_tenant != attempt.target_tenant && attempt.allowed)
    }

    /// Get all isolation violations
    pub fn get_violations(&self) -> Vec<IsolationAttempt> {
        self.access_attempts.lock().unwrap()
            .iter()
            .filter(|attempt| attempt.source_tenant != attempt.target_tenant && attempt.allowed)
            .cloned()
            .collect()
    }
}

/// Test configuration utilities
pub struct TestConfig {
    pub base_url: String,
    pub tenant_configs: HashMap<String, TenantConfig>,
}

impl TestConfig {
    /// Load test configuration from environment
    pub fn from_env() -> Self {
        let base_url = std::env::var("MPLORA_TEST_URL")
            .unwrap_or_else(|_| "http://localhost:9443".to_string());

        let mut tenant_configs = HashMap::new();

        // Load tenant configurations
        if let Ok(token) = std::env::var("TENANT_A_TOKEN") {
            tenant_configs.insert("tenant_a".to_string(), TenantConfig {
                id: "tenant_a".to_string(),
                name: "Tenant A".to_string(),
                token,
                base_url: base_url.clone(),
            });
        }

        if let Ok(token) = std::env::var("TENANT_B_TOKEN") {
            tenant_configs.insert("tenant_b".to_string(), TenantConfig {
                id: "tenant_b".to_string(),
                name: "Tenant B".to_string(),
                token,
                base_url: base_url.clone(),
            });
        }

        if let Ok(token) = std::env::var("TENANT_C_TOKEN") {
            tenant_configs.insert("tenant_c".to_string(), TenantConfig {
                id: "tenant_c".to_string(),
                name: "Tenant C".to_string(),
                token,
                base_url: base_url.clone(),
            });
        }

        Self {
            base_url,
            tenant_configs,
        }
    }

    /// Get base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get tenant configuration
    pub fn get_tenant(&self, tenant_id: &str) -> Option<&TenantConfig> {
        self.tenant_configs.get(tenant_id)
    }

    /// Get all tenant configurations
    pub fn tenants(&self) -> &HashMap<String, TenantConfig> {
        &self.tenant_configs
    }
}

/// Utility function to create test inference request
pub fn create_inference_request(cpid: &str, prompt: &str, max_tokens: u32, require_evidence: bool) -> serde_json::Value {
    serde_json::json!({
        "cpid": cpid,
        "prompt": prompt,
        "max_tokens": max_tokens,
        "require_evidence": require_evidence
    })
}

/// Utility function to wait for operation completion with timeout
pub async fn wait_for_completion<F, Fut>(operation: F, timeout: Duration) -> Result<()>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<bool>>,
{
    let start = Instant::now();

    while start.elapsed() < timeout {
        if operation().await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Err(anyhow::anyhow!("Operation did not complete within timeout"))
}