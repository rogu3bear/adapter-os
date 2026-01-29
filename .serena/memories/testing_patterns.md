# AdapterOS Testing Patterns

## Test Organization Structure

### Workspace-Level Tests (`tests/`)
Located at `/Users/star/Dev/adapter-os/tests/`, organized by category:

- **`determinism/`** - Determinism verification (HKDF seeding, hash chains, event sequences, cross-run validation)
- **`security/`** - Security compliance (policy rules, isolation, access control, audit trails, zero egress)
- **`integration/`** - Cross-component integration (team-based organization, tenant isolation, resource isolation)
- **`unit/`** - Unit test framework (`mocks.rs`, `isolation.rs`, `property.rs`, `evidence.rs`, `async_utils.rs`, `metal.rs`)
- **`benchmark/`** - Performance benchmarks (memory, throughput, kernel, isolation)
- **`determinism_tests/`** - Additional determinism tests (telemetry determinism)
- **`integration_tests/`** - Telemetry pipeline tests

### Per-Crate Tests
Each crate has its own `tests/` directory with integration tests:
- `crates/adapteros-db/tests/` - 50+ database tests
- `crates/adapteros-lora-router/src/tests.rs` - Router unit tests (inline)
- `crates/adapteros-lora-lifecycle/tests/fixtures.rs` - Database fixtures
- `crates/adapteros-crypto/tests/` - Cryptographic operation tests
- `crates/adapteros-server/tests/` - Server boot and CLI tests

## Feature Flags for Testing

```bash
--features extended-tests     # Extended test suite (most workspace tests)
--features hardware-residency # Hardware integration tests
--features loom              # Concurrency testing with loom
--features security_tests    # Security test modules
```

Most test modules use: `#![cfg(all(test, feature = "extended-tests"))]`

## Test Fixtures Pattern

### Database Fixtures (`crates/adapteros-lora-lifecycle/tests/fixtures.rs`)

```rust
// TestDbFixture - In-memory database with migrations
pub struct TestDbFixture {
    pub db: ProtectedDb,
    _temp_dir: Option<TempDir>,
}

impl TestDbFixture {
    pub async fn new() -> Self {
        let db = Db::connect(":memory:").await.expect("...");
        db.migrate().await.expect("...");
        Self { db: ProtectedDb::new(db), _temp_dir: None }
    }
}

// TestAdapterBuilder - Fluent builder for test adapters
let adapter_id = TestAdapterBuilder::new("test-adapter")
    .with_state("warm")
    .with_memory(1024 * 200)
    .with_activation_count(5)
    .register(db).await;

// Pre-built fixture sets
fixtures::single_unloaded(db).await;
fixtures::multi_state_lifecycle(db).await;  // Returns (cold, warm, hot)
fixtures::high_memory_pressure(db).await;   // Returns Vec<String>
fixtures::pinned_and_unpinned(db).await;
```

### API Testkit (`crates/adapteros-server-api/src/handlers/testkit.rs`)

E2E testing endpoints (requires `E2E_MODE=1` or `VITE_ENABLE_DEV_BYPASS=1`):

```rust
// Deterministic fixture constants
const TENANT_ID: &str = "tenant-test";
const MODEL_ID: &str = "model-qwen-test";
const ADAPTER_ID: &str = "adapter-test";
const FIXED_TS: &str = "2025-01-01T00:00:00Z";

// Endpoints:
POST /testkit/reset           // Clear all tables
POST /testkit/seed_minimal    // Seed deterministic fixtures
POST /testkit/create_trace_fixture
POST /testkit/create_evidence_fixture
POST /testkit/inference_stub  // Mock inference response
```

## Determinism Testing Patterns

### HKDF Seed Verification (`tests/determinism/hkdf_seeding.rs`)

```rust
#[test]
fn test_hkdf_seed_derivation() {
    let seed1 = derive_seed(&B3Hash::from_bytes([0x42; 32]), "test_label");
    let seed2 = derive_seed(&B3Hash::from_bytes([0x42; 32]), "test_label");
    assert_eq!(seed1, seed2, "HKDF derivation should be deterministic");
}

#[test]
fn test_hkdf_domain_separation() {
    let seed_router = derive_seed(&global, "router");
    let seed_dropout = derive_seed(&global, "dropout");
    assert_ne!(seed_router, seed_dropout, "Different domains should have different seeds");
}
```

### Determinism Test Context (`tests/determinism/utils.rs`)

```rust
pub struct DeterminismTestContext {
    pub global_seed: [u8; 32],
    pub executor: DeterministicExecutor,
    pub event_log: Vec<TelemetryEvent>,
}

pub struct HashChainValidator { pub chains: HashMap<String, Vec<B3Hash>> }
pub struct EventSequenceComparator { pub sequences: HashMap<String, EventSequence> }
pub struct HkdfSeedingVerifier { pub global_seed: [u8; 32] }
pub struct CanonicalHashingVerifier { pub hasher: B3Hash }
```

## Mock Patterns (`tests/unit/mocks.rs`)

### Deterministic Mocks

```rust
// DeterministicRng - Reproducible random numbers
pub struct DeterministicRng { seed: [u8; 32], counter: u64 }
impl DeterministicRng {
    pub fn from_seed(seed: u64) -> Self;
    pub fn gen_range(&mut self, range: Range<i32>) -> i32;
    pub fn gen_float(&mut self) -> f32;
}

// MockTelemetryCollector - Deterministic timestamps
pub struct MockTelemetryCollector {
    events: Arc<Mutex<Vec<MockTelemetryEvent>>>,
    seed: B3Hash,
}

// MockPolicyEngine - Seed-based policy decisions
pub struct MockPolicyEngine {
    policies: HashMap<String, serde_json::Value>,
    seed: B3Hash,
}

// MockAdapterRegistry - Thread-safe adapter registry
pub struct MockAdapterRegistry {
    adapters: Arc<Mutex<HashMap<String, MockAdapter>>>,
    seed: B3Hash,
}

// TestDataGenerator - Deterministic test data
pub struct TestDataGenerator { seed: B3Hash, counter: u64 }
impl TestDataGenerator {
    pub fn gen_string(&mut self, length: usize) -> String;
    pub fn gen_floats(&mut self, count: usize, range: Range<f32>) -> Vec<f32>;
    pub fn gen_json(&mut self) -> serde_json::Value;
}
```

## Component Isolation (`tests/unit/isolation.rs`)

```rust
// TestSandbox - Isolated filesystem operations
pub struct TestSandbox {
    root: PathBuf,
    files: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
    seed: B3Hash,
}
impl TestSandbox {
    pub fn new() -> Self;
    pub fn with_seed(seed: u64) -> Self;
    pub fn create_file(&self, relative_path: &str, size: usize) -> PathBuf;
}

// IsolatedComponent - Wrap component with mocks
pub struct IsolatedComponent<T> {
    component: T,
    sandbox: TestSandbox,
    mocks: HashMap<String, Box<dyn Any + Send + Sync>>,
}

// ResourcePool - Pooled test resources with RAII guards
pub struct ResourcePool<R> { resources: Arc<Mutex<Vec<R>>>, factory: Box<dyn Fn() -> R> }

// DependencyContainer - Service locator for tests
pub struct DependencyContainer { services: HashMap<String, Box<dyn Any + Send + Sync>> }

// TestHarness - Full test environment
pub struct TestHarness {
    environment: TestEnvironment,
    container: DependencyContainer,
    sandbox: TestSandbox,
}
```

## Property-Based Testing (`tests/unit/property.rs`)

```rust
pub trait Property {
    fn test(&self, input: &[u8]) -> bool;
    fn name(&self) -> &str;
}

pub trait Generator {
    fn generate(&mut self, size: usize, seed: &B3Hash) -> Vec<u8>;
    fn shrink(&self, input: &[u8]) -> Vec<u8>;
}

// Built-in properties
pub struct HashDeterminismProperty;
pub struct SeedDerivationDeterminismProperty;
pub struct CommutativityProperty<F>;
pub struct AssociativityProperty<F>;
pub struct IdentityProperty<F>;

// Property test runner
let runner = adapteros_property_runner();
let results = runner.run();
```

## Integration Test Utilities (`tests/integration/test_utils.rs`)

```rust
// Multi-tenant test harness
pub struct MultiTenantHarness {
    tenants: HashMap<String, TestTenant>,
    base_url: String,
}

// Resource monitoring
pub struct ResourceMonitor {
    metrics: Arc<Mutex<HashMap<String, Vec<ResourceMetrics>>>>,
}

// Policy validation
pub struct PolicyValidator {
    violations: Arc<Mutex<Vec<PolicyViolation>>>,
}

// Isolation checking
pub struct IsolationChecker {
    access_attempts: Arc<Mutex<Vec<IsolationAttempt>>>,
}

// Test configuration from environment
pub struct TestConfig {
    pub base_url: String,
    pub tenant_configs: HashMap<String, TenantConfig>,
}
impl TestConfig {
    pub fn from_env() -> Self;  // Reads MPLORA_TEST_URL, TENANT_A_TOKEN, etc.
}
```

## Environment Lock Pattern (`crates/adapteros-core/src/test_support.rs`)

```rust
#[cfg(test)]
pub(crate) fn env_lock() -> MutexGuard<'static, ()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK.get_or_init(|| Mutex::new(())).lock().expect("env lock")
}
```

Use when tests modify environment variables to prevent races.

## Router Test Patterns (`crates/adapteros-lora-router/src/tests.rs`)

### Q15 Quantization Tests
```rust
#[test]
fn test_q15_constants_validation() {
    assert_eq!(ROUTER_GATE_Q15_DENOM, 32767.0);
    assert_eq!(ROUTER_GATE_Q15_MAX, 32767);
}

#[test]
fn test_q15_round_trip_precision() {
    for original in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
        let q15 = (original * ROUTER_GATE_Q15_DENOM).round() as i16;
        let recovered = q15 as f32 / ROUTER_GATE_Q15_DENOM;
        assert!((recovered - original).abs() <= 1.0 / ROUTER_GATE_Q15_DENOM);
    }
}
```

### Deterministic Routing Tests
```rust
#[test]
fn test_deterministic_softmax_reproducibility() {
    let result1 = Router::deterministic_softmax(&scores, tau);
    let result2 = Router::deterministic_softmax(&scores, tau);
    assert_eq!(result1, result2);
}

#[test]
fn test_seeded_routing_reproducible_for_same_inputs() {
    let seed = derive_seed_full(&global, &manifest, &adapter_dir_hash, 7, "router", 0);
    let priors = seeded_priors(seed, 4);
    // Route twice with same priors -> same results
}
```

## Key Testing Commands

```bash
# Run all workspace tests
cargo test --workspace

# Run extended test suite
cargo test --workspace --features extended-tests

# Run specific determinism tests
cargo test --test determinism_core_suite -- --test-threads=8
cargo test -p adapteros-lora-router --test determinism

# Run with nextest (progress bar)
cargo nt          # alias from .cargo/config.toml
cargo ntf         # with immediate failure output

# Run ignored tests (hardware dependent)
cargo test --workspace -- --ignored
```

## Best Practices

1. **Always use deterministic seeds** - All mocks and generators accept seeds for reproducibility
2. **Use in-memory databases** - `Db::connect(":memory:")` for parallel test isolation
3. **Clean up test artifacts** - TestSandbox implements Drop for automatic cleanup
4. **Use feature flags** - Gate expensive/hardware tests behind features
5. **Test determinism explicitly** - Run operations twice and compare results
6. **Use property-based testing** - For mathematical invariants (hash determinism, Q15 quantization)
7. **Isolate environment changes** - Use `env_lock()` when modifying env vars
8. **Follow var/ policy** - Tests must not create persistent files outside `./var/`
