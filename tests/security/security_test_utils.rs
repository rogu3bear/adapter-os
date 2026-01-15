#![cfg(all(test, feature = "extended-tests"))]
//! Specialized Security Testing Utilities
//!
//! This module provides utilities specifically designed for security testing in adapterOS,
//! including mock security contexts, policy engines, compliance validators, and security
//! test harnesses.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use adapteros_core::{B3Hash, Evidence, Policy, PolicyEngine, SecurityContext};
use adapteros_policy::{PolicyPack, PolicyRule};
use tests_unit::isolation::{TestSandbox, IsolatedComponent};

/// Mock security context for testing
#[derive(Debug, Clone)]
pub struct MockSecurityContext {
    pub tenant_id: String,
    pub user_id: String,
    pub roles: HashSet<String>,
    pub permissions: HashSet<String>,
    pub attributes: HashMap<String, String>,
}

impl MockSecurityContext {
    /// Create a new mock security context
    pub fn new(tenant_id: &str, user_id: &str) -> Self {
        Self {
            tenant_id: tenant_id.to_string(),
            user_id: user_id.to_string(),
            roles: HashSet::new(),
            permissions: HashSet::new(),
            attributes: HashMap::new(),
        }
    }

    /// Add a role to the context
    pub fn with_role(mut self, role: &str) -> Self {
        self.roles.insert(role.to_string());
        self
    }

    /// Add a permission to the context
    pub fn with_permission(mut self, permission: &str) -> Self {
        self.permissions.insert(permission.to_string());
        self
    }

    /// Add an attribute to the context
    pub fn with_attribute(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }

    /// Check if the context has a specific role
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(role)
    }

    /// Check if the context has a specific permission
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }
}

impl From<MockSecurityContext> for SecurityContext {
    fn from(mock: MockSecurityContext) -> Self {
        SecurityContext {
            tenant_id: mock.tenant_id,
            user_id: mock.user_id,
            roles: mock.roles,
            permissions: mock.permissions,
            attributes: mock.attributes,
        }
    }
}

/// Mock policy engine for testing policy enforcement
pub struct MockPolicyEngine {
    policies: Arc<Mutex<HashMap<String, Policy>>>,
    violations: Arc<Mutex<Vec<String>>>,
}

impl MockPolicyEngine {
    /// Create a new mock policy engine
    pub fn new() -> Self {
        Self {
            policies: Arc::new(Mutex::new(HashMap::new())),
            violations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Add a policy to the engine
    pub fn add_policy(&self, name: &str, policy: Policy) {
        self.policies.lock().unwrap().insert(name.to_string(), policy);
    }

    /// Record a policy violation
    pub fn record_violation(&self, violation: &str) {
        self.violations.lock().unwrap().push(violation.to_string());
    }

    /// Get recorded violations
    pub fn get_violations(&self) -> Vec<String> {
        self.violations.lock().unwrap().clone()
    }

    /// Check if a policy exists
    pub fn has_policy(&self, name: &str) -> bool {
        self.policies.lock().unwrap().contains_key(name)
    }
}

/// Mock evidence collector for testing evidence validation
pub struct MockEvidenceCollector {
    evidence: Arc<Mutex<Vec<Evidence>>>,
    corrupted_evidence: Arc<Mutex<HashSet<B3Hash>>>,
}

impl MockEvidenceCollector {
    /// Create a new mock evidence collector
    pub fn new() -> Self {
        Self {
            evidence: Arc::new(Mutex::new(Vec::new())),
            corrupted_evidence: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Add evidence to the collector
    pub fn add_evidence(&self, evidence: Evidence) {
        self.evidence.lock().unwrap().push(evidence);
    }

    /// Mark evidence as corrupted
    pub fn corrupt_evidence(&self, hash: B3Hash) {
        self.corrupted_evidence.lock().unwrap().insert(hash);
    }

    /// Get all collected evidence
    pub fn get_evidence(&self) -> Vec<Evidence> {
        self.evidence.lock().unwrap().clone()
    }

    /// Check if evidence is corrupted
    pub fn is_corrupted(&self, hash: &B3Hash) -> bool {
        self.corrupted_evidence.lock().unwrap().contains(hash)
    }
}

/// Security test harness for comprehensive security testing
pub struct SecurityTestHarness {
    pub sandbox: TestSandbox,
    pub security_context: MockSecurityContext,
    pub policy_engine: MockPolicyEngine,
    pub evidence_collector: MockEvidenceCollector,
    pub audit_log: Arc<Mutex<Vec<String>>>,
}

impl SecurityTestHarness {
    /// Create a new security test harness
    pub fn new() -> Self {
        Self {
            sandbox: TestSandbox::new(),
            security_context: MockSecurityContext::new("test_tenant", "test_user"),
            policy_engine: MockPolicyEngine::new(),
            evidence_collector: MockEvidenceCollector::new(),
            audit_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a harness with a specific security context
    pub fn with_security_context(mut self, context: MockSecurityContext) -> Self {
        self.security_context = context;
        self
    }

    /// Add an audit log entry
    pub fn log_audit_event(&self, event: &str) {
        self.audit_log.lock().unwrap().push(event.to_string());
    }

    /// Get audit log entries
    pub fn get_audit_log(&self) -> Vec<String> {
        self.audit_log.lock().unwrap().clone()
    }

    /// Setup the harness for testing
    pub fn setup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Setup sandbox
        self.sandbox.create_dir("security_test_data")?;
        self.sandbox.create_file("security/policies.json", 1024)?;
        self.sandbox.create_file("security/evidence.db", 2048)?;

        // Setup basic policies
        let isolation_policy = Policy {
            name: "tenant_isolation".to_string(),
            rules: vec![
                PolicyRule::new("isolate_tenants", "Ensure tenant data isolation"),
                PolicyRule::new("prevent_cross_tenant_access", "Prevent cross-tenant data access"),
            ],
        };
        self.policy_engine.add_policy("tenant_isolation", isolation_policy);

        let evidence_policy = Policy {
            name: "evidence_integrity".to_string(),
            rules: vec![
                PolicyRule::new("validate_evidence", "Validate evidence integrity"),
                PolicyRule::new("prevent_evidence_tampering", "Prevent evidence tampering"),
            ],
        };
        self.policy_engine.add_policy("evidence_integrity", evidence_policy);

        Ok(())
    }

    /// Cleanup the harness
    pub fn cleanup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.sandbox.cleanup();
        Ok(())
    }

    /// Run a security test with the harness
    pub fn run_security_test<F, R>(&mut self, test_fn: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.setup().expect("Failed to setup security test harness");
        let result = test_fn(self);
        self.cleanup().expect("Failed to cleanup security test harness");
        result
    }
}

/// Compliance validator for security testing
pub struct ComplianceValidator {
    checks: HashMap<String, Box<dyn Fn(&SecurityTestHarness) -> Result<(), String> + Send + Sync>>,
}

impl ComplianceValidator {
    /// Create a new compliance validator
    pub fn new() -> Self {
        Self {
            checks: HashMap::new(),
        }
    }

    /// Add a compliance check
    pub fn add_check<F>(&mut self, name: &str, check: F)
    where
        F: Fn(&SecurityTestHarness) -> Result<(), String> + Send + Sync + 'static,
    {
        self.checks.insert(name.to_string(), Box::new(check));
    }

    /// Run all compliance checks
    pub fn validate(&self, harness: &SecurityTestHarness) -> Vec<(String, Result<(), String>)> {
        self.checks.iter()
            .map(|(name, check)| {
                let result = check(harness);
                (name.clone(), result)
            })
            .collect()
    }

    /// Check if all validations pass
    pub fn all_pass(&self, harness: &SecurityTestHarness) -> bool {
        self.validate(harness).iter().all(|(_, result)| result.is_ok())
    }
}

/// Security test utilities
pub mod security_utils {
    use super::*;

    /// Create a standard security test harness for tenant isolation testing
    pub fn create_tenant_isolation_harness() -> SecurityTestHarness {
        let mut harness = SecurityTestHarness::new();
        harness.security_context = MockSecurityContext::new("tenant_a", "user_1")
            .with_role("tenant_admin")
            .with_permission("read_tenant_data");

        harness
    }

    /// Create a harness for evidence validation testing
    pub fn create_evidence_validation_harness() -> SecurityTestHarness {
        let mut harness = SecurityTestHarness::new();
        harness.security_context = MockSecurityContext::new("system", "evidence_validator")
            .with_role("evidence_processor")
            .with_permission("validate_evidence");

        harness
    }

    /// Create a harness for access control testing
    pub fn create_access_control_harness() -> SecurityTestHarness {
        let mut harness = SecurityTestHarness::new();
        harness.security_context = MockSecurityContext::new("tenant_b", "user_2")
            .with_role("data_analyst")
            .with_permission("access_analytics");

        harness
    }

    /// Generate test evidence with specific properties
    pub fn generate_test_evidence(tenant_id: &str, data_type: &str) -> Evidence {
        Evidence {
            id: B3Hash::hash(format!("evidence_{}_{}", tenant_id, data_type).as_bytes()),
            tenant_id: tenant_id.to_string(),
            data_type: data_type.to_string(),
            content_hash: B3Hash::hash(b"test_content"),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            signature: Some(B3Hash::hash(b"test_signature")),
        }
    }

    /// Validate that evidence belongs to the correct tenant
    pub fn validate_evidence_tenant_isolation(evidence: &Evidence, tenant_id: &str) -> bool {
        evidence.tenant_id == tenant_id
    }

    /// Check if a security context has required permissions
    pub fn has_required_permissions(context: &MockSecurityContext, required: &[&str]) -> bool {
        required.iter().all(|perm| context.has_permission(perm))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_security_context() {
        let context = MockSecurityContext::new("tenant_1", "user_1")
            .with_role("admin")
            .with_permission("read")
            .with_attribute("department", "engineering");

        assert_eq!(context.tenant_id, "tenant_1");
        assert_eq!(context.user_id, "user_1");
        assert!(context.has_role("admin"));
        assert!(context.has_permission("read"));
        assert_eq!(context.attributes.get("department"), Some(&"engineering".to_string()));
    }

    #[test]
    fn test_mock_policy_engine() {
        let engine = MockPolicyEngine::new();

        let policy = Policy {
            name: "test_policy".to_string(),
            rules: vec![PolicyRule::new("rule1", "Test rule")],
        };

        engine.add_policy("test_policy", policy);
        assert!(engine.has_policy("test_policy"));

        engine.record_violation("Test violation");
        let violations = engine.get_violations();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0], "Test violation");
    }

    #[test]
    fn test_security_test_harness() {
        let mut harness = SecurityTestHarness::new();

        let result = harness.run_security_test(|h| {
            // Test that harness is properly set up
            assert!(h.policy_engine.has_policy("tenant_isolation"));
            assert!(h.policy_engine.has_policy("evidence_integrity"));

            // Test audit logging
            h.log_audit_event("Test security event");
            let log = h.get_audit_log();
            assert_eq!(log.len(), 1);
            assert_eq!(log[0], "Test security event");

            "test_passed"
        });

        assert_eq!(result, "test_passed");
    }

    #[test]
    fn test_compliance_validator() {
        let mut validator = ComplianceValidator::new();

        validator.add_check("tenant_isolation", |h| {
            if h.policy_engine.has_policy("tenant_isolation") {
                Ok(())
            } else {
                Err("Tenant isolation policy missing".to_string())
            }
        });

        validator.add_check("evidence_integrity", |h| {
            if h.policy_engine.has_policy("evidence_integrity") {
                Ok(())
            } else {
                Err("Evidence integrity policy missing".to_string())
            }
        });

        let harness = SecurityTestHarness::new();
        let results = validator.validate(&harness);

        assert_eq!(results.len(), 2);
        assert!(validator.all_pass(&harness));
    }

    #[test]
    fn test_security_utils() {
        let harness = security_utils::create_tenant_isolation_harness();
        assert_eq!(harness.security_context.tenant_id, "tenant_a");
        assert!(harness.security_context.has_role("tenant_admin"));

        let evidence = security_utils::generate_test_evidence("tenant_a", "inference_result");
        assert_eq!(evidence.tenant_id, "tenant_a");
        assert_eq!(evidence.data_type, "inference_result");

        assert!(security_utils::validate_evidence_tenant_isolation(&evidence, "tenant_a"));
        assert!(!security_utils::validate_evidence_tenant_isolation(&evidence, "tenant_b"));

        let context = MockSecurityContext::new("test", "user")
            .with_permission("read")
            .with_permission("write");

        assert!(security_utils::has_required_permissions(&context, &["read"]));
        assert!(security_utils::has_required_permissions(&context, &["read", "write"]));
        assert!(!security_utils::has_required_permissions(&context, &["read", "delete"]));
    }
}