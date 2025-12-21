//! Test data factories for creating test fixtures
//!
//! Provides builder-pattern factories for creating test data with sensible
//! defaults, making tests more readable and maintainable.

use adapteros_db::adapters::{AdapterRegistrationBuilder, AdapterRegistrationParams};
use uuid::Uuid;

/// Factory for creating test adapters with sensible defaults
///
/// Provides a fluent interface for creating adapter test data with
/// automatic generation of unique IDs and reasonable defaults.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::TestAdapterFactory;
///
/// let adapter = TestAdapterFactory::new("test-adapter")
///     .rank(16)
///     .tier("warm")
///     .category("code")
///     .build()
///     .unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct TestAdapterFactory {
    adapter_id: String,
    name: Option<String>,
    hash_b3: Option<String>,
    rank: i32,
    tier: String,
    alpha: Option<f64>,
    tenant_id: String,
    category: String,
    scope: String,
    framework: Option<String>,
    framework_id: Option<String>,
    framework_version: Option<String>,
    repo_id: Option<String>,
    commit_sha: Option<String>,
    intent: Option<String>,
    expires_at: Option<String>,
    aos_file_path: Option<String>,
    aos_file_hash: Option<String>,
    adapter_name: Option<String>,
    tenant_namespace: Option<String>,
    domain: Option<String>,
    purpose: Option<String>,
    revision: Option<String>,
    parent_id: Option<String>,
    fork_type: Option<String>,
    fork_reason: Option<String>,
}

impl TestAdapterFactory {
    /// Create a new factory with the given adapter ID
    ///
    /// Uses sensible defaults for all other fields.
    pub fn new(adapter_id: impl Into<String>) -> Self {
        let adapter_id = adapter_id.into();
        Self {
            adapter_id: adapter_id.clone(),
            name: None,
            hash_b3: None,
            rank: 8, // Default rank
            tier: "warm".to_string(),
            alpha: None,
            tenant_id: "default-tenant".to_string(),
            category: "code".to_string(),
            scope: "global".to_string(),
            framework: None,
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            expires_at: None,
            aos_file_path: None,
            aos_file_hash: None,
            adapter_name: None,
            tenant_namespace: None,
            domain: None,
            purpose: None,
            revision: None,
            parent_id: None,
            fork_type: None,
            fork_reason: None,
        }
    }

    /// Create a factory with a randomly generated unique adapter ID
    pub fn random() -> Self {
        let id = format!("test-adapter-{}", Uuid::new_v4());
        Self::new(id)
    }

    /// Set the adapter name (defaults to adapter_id if not set)
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the B3 hash (auto-generated if not set)
    pub fn hash_b3(mut self, hash: impl Into<String>) -> Self {
        self.hash_b3 = Some(hash.into());
        self
    }

    /// Set the rank
    pub fn rank(mut self, rank: i32) -> Self {
        self.rank = rank;
        self
    }

    /// Set the tier (persistent, warm, or ephemeral)
    pub fn tier(mut self, tier: impl Into<String>) -> Self {
        self.tier = tier.into();
        self
    }

    /// Set the alpha parameter
    pub fn alpha(mut self, alpha: f64) -> Self {
        self.alpha = Some(alpha);
        self
    }

    /// Set the tenant ID
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = tenant_id.into();
        self
    }

    /// Set the category
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// Set the scope
    pub fn scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = scope.into();
        self
    }

    /// Set the framework
    pub fn framework(mut self, framework: impl Into<String>) -> Self {
        self.framework = Some(framework.into());
        self
    }

    /// Set the framework ID
    pub fn framework_id(mut self, framework_id: impl Into<String>) -> Self {
        self.framework_id = Some(framework_id.into());
        self
    }

    /// Set the framework version
    pub fn framework_version(mut self, version: impl Into<String>) -> Self {
        self.framework_version = Some(version.into());
        self
    }

    /// Set the repository ID
    pub fn repo_id(mut self, repo_id: impl Into<String>) -> Self {
        self.repo_id = Some(repo_id.into());
        self
    }

    /// Set the commit SHA
    pub fn commit_sha(mut self, sha: impl Into<String>) -> Self {
        self.commit_sha = Some(sha.into());
        self
    }

    /// Set the intent
    pub fn intent(mut self, intent: impl Into<String>) -> Self {
        self.intent = Some(intent.into());
        self
    }

    /// Set expiration timestamp
    pub fn expires_at(mut self, timestamp: impl Into<String>) -> Self {
        self.expires_at = Some(timestamp.into());
        self
    }

    /// Set the .aos file path
    pub fn aos_file_path(mut self, path: impl Into<String>) -> Self {
        self.aos_file_path = Some(path.into());
        self
    }

    /// Set the .aos file hash
    pub fn aos_file_hash(mut self, hash: impl Into<String>) -> Self {
        self.aos_file_hash = Some(hash.into());
        self
    }

    /// Set the semantic adapter name
    pub fn adapter_name(mut self, name: impl Into<String>) -> Self {
        self.adapter_name = Some(name.into());
        self
    }

    /// Set the tenant namespace
    pub fn tenant_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.tenant_namespace = Some(namespace.into());
        self
    }

    /// Set the domain
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Set the purpose
    pub fn purpose(mut self, purpose: impl Into<String>) -> Self {
        self.purpose = Some(purpose.into());
        self
    }

    /// Set the revision
    pub fn revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    /// Set the parent adapter ID
    pub fn parent_id(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Set the fork type
    pub fn fork_type(mut self, fork_type: impl Into<String>) -> Self {
        self.fork_type = Some(fork_type.into());
        self
    }

    /// Set the fork reason
    pub fn fork_reason(mut self, reason: impl Into<String>) -> Self {
        self.fork_reason = Some(reason.into());
        self
    }

    /// Build the adapter registration parameters
    ///
    /// Generates defaults for any unset fields.
    pub fn build(self) -> adapteros_core::Result<AdapterRegistrationParams> {
        let mut builder = AdapterRegistrationBuilder::new()
            .adapter_id(self.adapter_id.clone())
            .name(
                self.name
                    .unwrap_or_else(|| format!("{} Test", self.adapter_id)),
            )
            .hash_b3(
                self.hash_b3
                    .unwrap_or_else(|| format!("b3:test_{}", Uuid::new_v4())),
            )
            .rank(self.rank)
            .tier(self.tier)
            .tenant_id(self.tenant_id)
            .category(self.category)
            .scope(self.scope);

        if let Some(alpha) = self.alpha {
            builder = builder.alpha(alpha);
        }

        if let Some(framework) = self.framework {
            builder = builder.framework(Some(framework));
        }

        if let Some(framework_id) = self.framework_id {
            builder = builder.framework_id(Some(framework_id));
        }

        if let Some(framework_version) = self.framework_version {
            builder = builder.framework_version(Some(framework_version));
        }

        if let Some(repo_id) = self.repo_id {
            builder = builder.repo_id(Some(repo_id));
        }

        if let Some(commit_sha) = self.commit_sha {
            builder = builder.commit_sha(Some(commit_sha));
        }

        if let Some(intent) = self.intent {
            builder = builder.intent(Some(intent));
        }

        if let Some(expires_at) = self.expires_at {
            builder = builder.expires_at(Some(expires_at));
        }

        if let Some(aos_file_path) = self.aos_file_path {
            builder = builder.aos_file_path(Some(aos_file_path));
        }

        if let Some(aos_file_hash) = self.aos_file_hash {
            builder = builder.aos_file_hash(Some(aos_file_hash));
        }

        if let Some(adapter_name) = self.adapter_name {
            builder = builder.adapter_name(Some(adapter_name));
        }

        if let Some(tenant_namespace) = self.tenant_namespace {
            builder = builder.tenant_namespace(Some(tenant_namespace));
        }

        if let Some(domain) = self.domain {
            builder = builder.domain(Some(domain));
        }

        if let Some(purpose) = self.purpose {
            builder = builder.purpose(Some(purpose));
        }

        if let Some(revision) = self.revision {
            builder = builder.revision(Some(revision));
        }

        if let Some(parent_id) = self.parent_id {
            builder = builder.parent_id(Some(parent_id));
        }

        if let Some(fork_type) = self.fork_type {
            builder = builder.fork_type(Some(fork_type));
        }

        if let Some(fork_reason) = self.fork_reason {
            builder = builder.fork_reason(Some(fork_reason));
        }

        builder.build()
    }

    /// Convenience method to build and return the adapter ID
    ///
    /// Useful when you just need a valid adapter ID for testing.
    pub fn adapter_id(&self) -> &str {
        &self.adapter_id
    }
}

impl Default for TestAdapterFactory {
    fn default() -> Self {
        Self::random()
    }
}

/// Factory for creating test tenants
#[derive(Debug, Clone)]
pub struct TestTenantFactory {
    tenant_id: String,
    name: Option<String>,
    is_system: bool,
}

impl TestTenantFactory {
    /// Create a new tenant factory
    pub fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            name: None,
            is_system: false,
        }
    }

    /// Create a factory with a random tenant ID
    pub fn random() -> Self {
        let id = format!("test-tenant-{}", Uuid::new_v4());
        Self::new(id)
    }

    /// Set the tenant name
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Mark as system tenant
    pub fn system(mut self) -> Self {
        self.is_system = true;
        self
    }

    /// Get the tenant ID
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// Get the tenant name (or generate default)
    pub fn tenant_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("Test Tenant {}", self.tenant_id))
    }

    /// Check if system tenant
    pub fn is_system(&self) -> bool {
        self.is_system
    }
}

impl Default for TestTenantFactory {
    fn default() -> Self {
        Self::random()
    }
}

/// Factory for creating test adapter stacks
#[derive(Debug, Clone)]
pub struct TestStackFactory {
    name: String,
    adapter_ids: Vec<String>,
    tenant_id: String,
    description: Option<String>,
}

impl TestStackFactory {
    /// Create a new stack factory
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            adapter_ids: Vec::new(),
            tenant_id: "default-tenant".to_string(),
            description: None,
        }
    }

    /// Create a factory with a random name
    pub fn random() -> Self {
        let name = format!("test-stack-{}", Uuid::new_v4());
        Self::new(name)
    }

    /// Add an adapter to the stack
    pub fn add_adapter(mut self, adapter_id: impl Into<String>) -> Self {
        self.adapter_ids.push(adapter_id.into());
        self
    }

    /// Add multiple adapters to the stack
    pub fn add_adapters<I, S>(mut self, adapter_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.adapter_ids
            .extend(adapter_ids.into_iter().map(|id| id.into()));
        self
    }

    /// Set the tenant ID
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = tenant_id.into();
        self
    }

    /// Set the description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Get the stack name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the adapter IDs
    pub fn adapter_ids(&self) -> &[String] {
        &self.adapter_ids
    }

    /// Get the tenant ID
    pub fn tenant_id_ref(&self) -> &str {
        &self.tenant_id
    }

    /// Get the description (or generate default)
    pub fn description_text(&self) -> String {
        self.description
            .clone()
            .unwrap_or_else(|| format!("Test stack {}", self.name))
    }
}

impl Default for TestStackFactory {
    fn default() -> Self {
        Self::random()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_factory_defaults() {
        let factory = TestAdapterFactory::new("test-1");
        assert_eq!(factory.adapter_id(), "test-1");
        assert_eq!(factory.rank, 8);
        assert_eq!(factory.tier, "warm");
        assert_eq!(factory.category, "code");
        assert_eq!(factory.scope, "global");
    }

    #[test]
    fn test_adapter_factory_random() {
        let factory1 = TestAdapterFactory::random();
        let factory2 = TestAdapterFactory::random();
        assert_ne!(factory1.adapter_id(), factory2.adapter_id());
    }

    #[test]
    fn test_adapter_factory_build() {
        let params = TestAdapterFactory::new("test-adapter")
            .name("Test Adapter")
            .rank(16)
            .tier("persistent")
            .category("framework")
            .framework("rust")
            .build()
            .unwrap();

        assert_eq!(params.adapter_id, "test-adapter");
        assert_eq!(params.name, "Test Adapter");
        assert_eq!(params.rank, 16);
        assert_eq!(params.tier, "persistent");
        assert_eq!(params.category, "framework");
        assert_eq!(params.framework, Some("rust".to_string()));
    }

    #[test]
    fn test_tenant_factory() {
        let factory = TestTenantFactory::new("test-tenant")
            .name("Test Tenant")
            .system();

        assert_eq!(factory.tenant_id(), "test-tenant");
        assert_eq!(factory.tenant_name(), "Test Tenant");
        assert!(factory.is_system());
    }

    #[test]
    fn test_stack_factory() {
        let factory = TestStackFactory::new("test-stack")
            .add_adapter("adapter-1")
            .add_adapter("adapter-2")
            .description("Test stack");

        assert_eq!(factory.name(), "test-stack");
        assert_eq!(factory.adapter_ids().len(), 2);
        assert_eq!(factory.description_text(), "Test stack");
    }
}
