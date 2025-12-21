//! Test fixtures for lifecycle database tests
//!
//! Provides:
//! - Database setup/teardown with migrations
//! - Test adapter fixtures with various states
//! - Helper functions for common test scenarios
//! - Isolation mechanisms for parallel test execution

#[allow(unused_imports)]
use adapteros_db::{AdapterRegistrationBuilder, Db};
#[allow(unused_imports)]
use adapteros_lora_lifecycle::AdapterState;
#[allow(unused_imports)]
use std::path::PathBuf;
#[allow(unused_imports)]
use tempfile::TempDir;
#[allow(unused_imports)]
use uuid::Uuid;

/// Test database fixture
///
/// Automatically runs migrations and cleans up on drop.
/// Safe for parallel test execution (each gets unique in-memory DB).
pub struct TestDbFixture {
    pub db: Db,
    _temp_dir: Option<TempDir>,
}

impl TestDbFixture {
    /// Create a new in-memory test database with migrations
    pub async fn new() -> Self {
        let db = Db::connect(":memory:")
            .await
            .expect("Failed to create test database");

        db.migrate().await.expect("Failed to run migrations");

        Self {
            db,
            _temp_dir: None,
        }
    }

    /// Get a reference to the database
    pub fn db(&self) -> &Db {
        &self.db
    }
}

/// Ensure a tenant exists for fixture data and return its ID
async fn ensure_fixture_tenant(db: &Db) -> String {
    if let Some(id) = sqlx::query_scalar::<_, String>("SELECT id FROM tenants LIMIT 1")
        .fetch_optional(db.pool())
        .await
        .expect("Failed to query tenants")
    {
        id
    } else {
        db.create_tenant("default-tenant", false)
            .await
            .expect("Failed to create default tenant for fixtures")
    }
}

/// Builder for creating test adapters with various configurations
pub struct TestAdapterBuilder {
    id: String,
    name: String,
    hash: String,
    rank: i32,    // Changed from u16 to i32 to match AdapterRegistrationBuilder
    tier: String, // Changed from u16 to String to match AdapterRegistrationBuilder
    category: String,
    state: String,
    memory_bytes: i64,
    activation_count: i64,
}

impl TestAdapterBuilder {
    /// Create a new test adapter builder
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: format!("test-{}", id),
            hash: format!("hash_{}", Uuid::new_v4()),
            rank: 16,
            tier: "warm".to_string(), // Changed from 2 to "warm"
            category: "code".to_string(),
            state: "unloaded".to_string(),
            memory_bytes: 0,
            activation_count: 0,
        }
    }

    /// Set adapter name
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Set adapter hash
    pub fn with_hash(mut self, hash: &str) -> Self {
        self.hash = hash.to_string();
        self
    }

    /// Set rank
    pub fn with_rank(mut self, rank: i32) -> Self {
        self.rank = rank;
        self
    }

    /// Set tier (e.g., "persistent", "warm", "ephemeral")
    pub fn with_tier(mut self, tier: &str) -> Self {
        self.tier = tier.to_string();
        self
    }

    /// Set category
    pub fn with_category(mut self, category: &str) -> Self {
        self.category = category.to_string();
        self
    }

    /// Set initial state
    pub fn with_state(mut self, state: &str) -> Self {
        self.state = state.to_string();
        self
    }

    /// Set memory bytes
    pub fn with_memory(mut self, bytes: i64) -> Self {
        self.memory_bytes = bytes;
        self
    }

    /// Set activation count
    pub fn with_activation_count(mut self, count: i64) -> Self {
        self.activation_count = count;
        self
    }

    /// Register adapter in database
    pub async fn register(self, db: &Db) -> String {
        // Ensure a tenant exists and use it for registrations to satisfy FKs
        let tenant_id = ensure_fixture_tenant(db).await;

        db.register_adapter(
            AdapterRegistrationBuilder::new()
                .tenant_id(&tenant_id)
                .adapter_id(&self.id)
                .name(&self.name)
                .hash_b3(&self.hash)
                .rank(self.rank)
                .tier(&self.tier)
                .category(&self.category)
                .build()
                .expect("Failed to build adapter params"),
        )
        .await
        .expect("Failed to register adapter");

        // Persist desired state and memory (dual-write to KV when enabled)
        db.update_adapter_state_and_memory(
            &self.id,
            &self.state,
            self.memory_bytes,
            "fixture_setup",
        )
        .await
        .expect("Failed to set adapter state and memory");

        // Seed activation count for tests (SQL primary; KV updates if storage_mode permits)
        if self.activation_count > 0 {
            sqlx::query(
                "UPDATE adapters
                 SET activation_count = ?, last_activated = datetime('now'), updated_at = datetime('now')
                 WHERE adapter_id = ?",
            )
            .bind(self.activation_count)
            .bind(&self.id)
            .execute(db.pool())
            .await
            .expect("Failed to set activation count for adapter");
        }

        self.id
    }
}

/// Fixture sets for common test scenarios
pub mod fixtures {
    use super::*;

    /// Single unloaded adapter
    pub async fn single_unloaded(db: &Db) -> String {
        TestAdapterBuilder::new("test-unloaded")
            .with_state("unloaded")
            .register(db)
            .await
    }

    /// Single loaded adapter (cold state)
    pub async fn single_cold(db: &Db) -> String {
        let id = TestAdapterBuilder::new("test-cold")
            .with_state("cold")
            .with_memory(1024 * 100) // 100 KB
            .register(db)
            .await;

        id
    }

    /// Single warm adapter
    pub async fn single_warm(db: &Db) -> String {
        let id = TestAdapterBuilder::new("test-warm")
            .with_state("warm")
            .with_memory(1024 * 200) // 200 KB
            .with_activation_count(5)
            .register(db)
            .await;

        id
    }

    /// Single hot adapter
    pub async fn single_hot(db: &Db) -> String {
        let id = TestAdapterBuilder::new("test-hot")
            .with_state("hot")
            .with_memory(1024 * 300) // 300 KB
            .with_activation_count(15)
            .register(db)
            .await;

        id
    }

    /// Single resident (pinned) adapter
    pub async fn single_resident(db: &Db) -> String {
        let id = TestAdapterBuilder::new("test-resident")
            .with_state("resident")
            .with_memory(1024 * 400) // 400 KB
            .register(db)
            .await;

        // Pin the adapter
        let tenant_id = ensure_fixture_tenant(db).await;
        db.pin_adapter(
            &tenant_id,
            &id,
            Some("2099-12-31 23:59:59"),
            "test_reason",
            Some("test_user"),
        )
        .await
        .expect("Failed to pin adapter");

        id
    }

    /// Multi-state lifecycle (3 adapters in different states)
    pub async fn multi_state_lifecycle(db: &Db) -> (String, String, String) {
        let cold = TestAdapterBuilder::new("test-cold-lifecycle")
            .with_state("cold")
            .with_memory(1024 * 100)
            .with_activation_count(1)
            .register(db)
            .await;

        let warm = TestAdapterBuilder::new("test-warm-lifecycle")
            .with_state("warm")
            .with_memory(1024 * 200)
            .with_activation_count(5)
            .register(db)
            .await;

        let hot = TestAdapterBuilder::new("test-hot-lifecycle")
            .with_state("hot")
            .with_memory(1024 * 300)
            .with_activation_count(15)
            .register(db)
            .await;

        (cold, warm, hot)
    }

    /// High memory pressure scenario (many large adapters)
    pub async fn high_memory_pressure(db: &Db) -> Vec<String> {
        let mut ids = Vec::new();

        for i in 0..5 {
            let id = TestAdapterBuilder::new(&format!("test-memory-{}", i))
                .with_state("warm")
                .with_memory(1024 * 1024 * 10) // 10 MB each
                .with_activation_count(i as i64)
                .register(db)
                .await;

            ids.push(id);
        }

        ids
    }

    /// Category-based adapters (code, framework, codebase)
    pub async fn category_adapters(db: &Db) -> (String, String, String) {
        let code = TestAdapterBuilder::new("test-code-category")
            .with_category("code")
            .with_state("warm")
            .with_activation_count(10)
            .register(db)
            .await;

        let framework = TestAdapterBuilder::new("test-framework-category")
            .with_category("framework")
            .with_state("warm")
            .with_activation_count(5)
            .register(db)
            .await;

        let codebase = TestAdapterBuilder::new("test-codebase-category")
            .with_category("codebase")
            .with_state("warm")
            .with_activation_count(3)
            .register(db)
            .await;

        (code, framework, codebase)
    }

    /// Pinned and unpinned adapters mixed
    pub async fn pinned_and_unpinned(db: &Db) -> (String, String) {
        let pinned = TestAdapterBuilder::new("test-pinned-fixture")
            .with_state("resident")
            .with_memory(1024 * 200)
            .register(db)
            .await;

        let unpinned = TestAdapterBuilder::new("test-unpinned-fixture")
            .with_state("warm")
            .with_memory(1024 * 200)
            .register(db)
            .await;

        // Pin the first adapter
        let tenant_id = ensure_fixture_tenant(db).await;
        db.pin_adapter(&tenant_id, &pinned, None, "test_pinned", Some("test_user"))
            .await
            .expect("Failed to pin adapter");

        (pinned, unpinned)
    }

    /// TTL/expiring adapters (with expires_at timestamp)
    pub async fn ttl_adapters(db: &Db) -> (String, String) {
        // One that's already expired
        let expired = TestAdapterBuilder::new("test-expired-adapter")
            .with_state("warm")
            .register(db)
            .await;

        // One that expires in future
        let expiring = TestAdapterBuilder::new("test-expiring-adapter")
            .with_state("warm")
            .register(db)
            .await;

        (expired, expiring)
    }

    /// High activation scenario (frequently used adapters)
    pub async fn high_activation(db: &Db) -> String {
        let id = TestAdapterBuilder::new("test-high-activation")
            .with_state("hot")
            .with_activation_count(100)
            .register(db)
            .await;

        id
    }

    /// Low activation scenario (rarely used adapters)
    pub async fn low_activation(db: &Db) -> String {
        let id = TestAdapterBuilder::new("test-low-activation")
            .with_state("cold")
            .with_activation_count(0)
            .register(db)
            .await;

        id
    }
}

/// Test utilities for common operations
pub mod utils {
    use super::*;

    /// Verify adapter state matches expected value
    pub async fn verify_adapter_state(db: &Db, adapter_id: &str, expected_state: &str) -> bool {
        if let Ok(Some(adapter)) = db.get_adapter(adapter_id).await {
            adapter.current_state == expected_state
        } else {
            false
        }
    }

    /// Get current memory usage for an adapter
    pub async fn get_adapter_memory(db: &Db, adapter_id: &str) -> i64 {
        if let Ok(Some(adapter)) = db.get_adapter(adapter_id).await {
            adapter.memory_bytes
        } else {
            0
        }
    }

    /// Count adapters in a specific state
    pub async fn count_adapters_in_state(db: &Db, state: &str) -> i64 {
        let result: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM adapters WHERE current_state = ?")
                .bind(state)
                .fetch_one(db.pool())
                .await
                .unwrap_or((0,));

        result.0
    }

    /// Count all adapters
    pub async fn count_all_adapters(db: &Db) -> i64 {
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM adapters")
            .fetch_one(db.pool())
            .await
            .unwrap_or((0,));

        result.0
    }

    /// Get total memory usage across all adapters
    pub async fn total_memory_usage(db: &Db) -> i64 {
        let result: (Option<i64>,) = sqlx::query_as("SELECT SUM(memory_bytes) FROM adapters")
            .fetch_one(db.pool())
            .await
            .unwrap_or((None,));

        result.0.unwrap_or(0)
    }

    /// List all adapters with their current state
    pub async fn list_adapters_with_state(db: &Db) -> Vec<(String, String)> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT adapter_id, current_state FROM adapters")
                .fetch_all(db.pool())
                .await
                .unwrap_or_default();

        rows
    }

    /// Cleanup: reset database between tests
    pub async fn cleanup_adapters(db: &Db) {
        let _ = sqlx::query("DELETE FROM adapters").execute(db.pool()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Pending Db::ping method implementation [tracking: STAB-IGN-001]"]
    async fn test_fixture_creation() {
        let _fixture = TestDbFixture::new().await;
        // Note: Db::ping() method doesn't exist, would need to be added
        // assert!(fixture.db().clone().ping().await.is_ok());
    }

    #[tokio::test]
    #[ignore = "Pending fixture API refactoring [tracking: STAB-IGN-001]"]
    async fn test_single_unloaded_fixture() {
        let fixture = TestDbFixture::new().await;
        let adapter_id = fixtures::single_unloaded(fixture.db()).await;

        let adapter = fixture.db().get_adapter(&adapter_id).await.unwrap();
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().current_state, "unloaded");
    }

    #[tokio::test]
    #[ignore = "Pending fixture API refactoring [tracking: STAB-IGN-001]"]
    async fn test_multi_state_fixture() {
        let fixture = TestDbFixture::new().await;
        let (cold, warm, hot) = fixtures::multi_state_lifecycle(fixture.db()).await;

        let cold_adapter = fixture.db().get_adapter(&cold).await.unwrap().unwrap();
        let warm_adapter = fixture.db().get_adapter(&warm).await.unwrap().unwrap();
        let hot_adapter = fixture.db().get_adapter(&hot).await.unwrap().unwrap();

        assert_eq!(cold_adapter.current_state, "cold");
        assert_eq!(warm_adapter.current_state, "warm");
        assert_eq!(hot_adapter.current_state, "hot");
    }

    #[tokio::test]
    #[ignore = "Pending fixture API refactoring [tracking: STAB-IGN-001]"]
    async fn test_parallel_fixture_isolation() {
        // Two tests running in parallel should have independent databases
        let fixture1 = TestDbFixture::new().await;
        let fixture2 = TestDbFixture::new().await;

        let id1 = fixtures::single_cold(fixture1.db()).await;
        let id2 = fixtures::single_warm(fixture2.db()).await;

        let adapter1 = fixture1.db().get_adapter(&id1).await.unwrap().unwrap();
        let adapter2 = fixture2.db().get_adapter(&id2).await.unwrap().unwrap();

        assert_eq!(adapter1.current_state, "cold");
        assert_eq!(adapter2.current_state, "warm");
    }
}
