# AdapterOS Naming Conventions

This document establishes naming conventions for the AdapterOS codebase. All new code and refactors must follow these conventions for consistency and maintainability.

---

## Function Naming

### Initialization Functions

Use `init_*` for lightweight, single-purpose initialization functions:

```rust
// GOOD
pub fn init_logging() -> Result<()>
pub fn init_cpu_affinity() -> Result<()>
pub fn init_metrics()
pub fn init_guards(config: GuardConfig)

// BAD - use init_ prefix
pub fn setup_logging() -> Result<()>
pub fn start_metrics()
```

**Exception**: The `boot` module in `adapteros-server` uses `initialize_*` for complex, multi-step boot phase functions that return context structs:

```rust
// Boot module pattern (acceptable exception)
pub async fn initialize_config(cli: &Cli) -> Result<ConfigContext>
pub async fn initialize_database(...) -> Result<DatabaseContext>
pub async fn initialize_security(...) -> Result<SecurityContext>
```

This exception exists because boot phases are semantically different from simple initialization - they perform multiple operations, manage state transitions, and return rich context objects.

### CRUD Operations

| Prefix | Usage | Example |
|--------|-------|---------|
| `create_*` | Create new entities | `create_adapter()`, `create_policy()` |
| `get_*` | Retrieve single entity by ID | `get_adapter()`, `get_config()` |
| `list_*` | Retrieve multiple entities | `list_adapters()`, `list_collections()` |
| `update_*` | Modify existing entity | `update_pack_config()`, `update_adapter_state()` |
| `delete_*` | **Remove entity from storage** | `delete_adapter()`, `delete_stack()` |
| `remove_*` | **Remove relationship/association** (entity still exists) | `remove_adapter()` (from cache), `remove_region()` |

```rust
// delete_* - Entity is deleted from storage
pub async fn delete_adapter(db: &Db, adapter_id: &str) -> Result<()> {
    sqlx::query!("DELETE FROM adapters WHERE id = ?", adapter_id)
        .execute(db)
        .await?;
    Ok(())
}

// remove_* - Entity removed from collection/cache, still exists elsewhere
pub fn remove_adapter(&mut self, adapter_id: &str) {
    self.adapters.remove(adapter_id);  // HashMap removal, adapter still in DB
}
```

### Router Methods

| Prefix | Usage | Example |
|--------|-------|---------|
| `route_*` | Core routing logic | `route_and_infer()` |
| `route_with_*` | Routing with additional context | `route_with_adapter_info()`, `route_with_k0_detection()` |
| `score_*` | Scoring operations | `score_frameworks()`, `score_paths()` |

```rust
// Core routing
pub fn route_with_adapter_info(&mut self, features: &[f32], priors: &[f32]) -> Decision

// Routing with additional context
pub fn route_with_adapter_info_and_scope(&mut self, ..., scope: ScopeHint) -> Decision

// Scoring
pub fn score_frameworks(&self, input: &str) -> Vec<(String, f32)>
```

### Async Conventions

- Async functions do **not** need `_async` suffix (Rust convention)
- Use `spawn_*` for fire-and-forget background tasks
- Use `run_*` for blocking/long-running operations that complete

```rust
// GOOD
pub async fn get_adapter(db: &Db, id: &str) -> Result<Adapter>
pub fn spawn_telemetry_workers(&self) -> JoinHandle<()>
pub async fn run_preflight_checks(...) -> Result<PreflightResult>

// BAD - unnecessary _async suffix
pub async fn get_adapter_async(db: &Db, id: &str) -> Result<Adapter>
```

### Builder Pattern

Use `with_*` methods for builder-style configuration:

```rust
impl LayerFeatures {
    pub fn new(layer_idx: u32, layer_type: LayerType, hidden_state_norm: f32) -> Self

    pub fn with_attention_entropy(mut self, entropy: f32) -> Self {
        self.attention_entropy = Some(entropy);
        self
    }

    pub fn with_activation_stats(mut self, mean: f32, variance: f32) -> Self {
        self.activation_mean = Some(mean);
        self.activation_variance = Some(variance);
        self
    }
}
```

---

## Type Naming

### Structs

Use PascalCase. Suffix indicates purpose:

| Suffix | Usage | Example |
|--------|-------|---------|
| `Config` | Configuration structs | `RouterConfig`, `ModelConfig` |
| `Context` | Runtime context with multiple fields | `DatabaseContext`, `SecurityContext` |
| `Result` | Operation result with metadata | `PreflightResult`, `VerificationResult` |
| `Stats` | Statistics/metrics containers | `CacheStats`, `GcStats` |
| `Builder` | Builder pattern structs | `TraceBuilder`, `ConfigBuilder` |
| `Manager` | Resource managers | `BootStateManager`, `EvidenceManager` |
| `Handler` | Request/event handlers | `InferenceHandler` |

```rust
// GOOD
pub struct RouterConfig { ... }
pub struct DatabaseContext { ... }
pub struct PreflightResult { ... }

// BAD - unclear purpose
pub struct RouterData { ... }    // Use RouterConfig or RouterState
pub struct DbStuff { ... }       // Use DatabaseContext
```

### Enums

Use PascalCase for type name and variants:

```rust
// GOOD
pub enum CheckStatus {
    Passed,
    Failed,
    Skipped,
}

pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

// BAD - lowercase variants
pub enum CheckStatus {
    passed,
    failed,
}
```

### Acronyms in Types

Preserve uppercase casing for acronyms in type names (see `docs/design/NAMING_MOE_LORA.md` for full spec):

| Acronym | Types (PascalCase) | Fields/Methods (snake_case) |
|---------|-------------------|----------------------------|
| MoE | `MoEConfig` | `moe_forward()` |
| LoRA | `LoRAAdapter` | `lora_rank` |
| KV | `KVCacheConfig` | `kv_cache` |
| MLX | `MLXFFIBackend` | `mlx_backend` |
| FFI | `*FFI*` | `ffi_*` |

```rust
// GOOD
struct MoEConfig { ... }
struct LoRAAdapter { ... }
let lora_rank = 16;

// BAD
struct MoeConfig { ... }    // Use MoE
struct LoraAdapter { ... }  // Use LoRA
```

---

## Error Naming

### Error Enums

Name error enums with `Error` suffix. Use domain prefix for clarity:

```rust
// GOOD - domain-prefixed error types
pub enum AosCryptoError { ... }
pub enum AosNetworkError { ... }
pub enum AosStorageError { ... }

// Specific error enums
pub enum IngestionError { ... }
pub enum ContextIdError { ... }
```

### Error Variants

Use descriptive noun phrases. Include context fields where helpful:

```rust
// GOOD
pub enum AosNetworkError {
    ConnectionFailed { host: String, port: u16 },
    TlsCertificateError { path: String, reason: String },
    Timeout { duration_secs: u64 },
}

// BAD - verb phrases, missing context
pub enum AosNetworkError {
    FailedToConnect,
    CertWrong,
    TimedOut,
}
```

---

## Module Naming

### Crate Naming

Use `adapteros-` prefix for workspace crates:

```
adapteros-core        # Shared types, errors, utilities
adapteros-lora-*      # LoRA-specific functionality
adapteros-server-*    # Server/API components
adapteros-db          # Database layer
adapteros-config      # Configuration system
```

### Module Organization

```
src/
  lib.rs              # Public API, re-exports
  types.rs            # Shared types for the crate
  errors.rs           # Error types (or errors/ directory)
  handlers/           # Request handlers
  services/           # Business logic services
  utils.rs            # Internal utilities
```

---

## Constant Naming

Use SCREAMING_SNAKE_CASE:

```rust
// GOOD
pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;
pub const DEFAULT_DB_PATH: &str = "var/adapteros.sqlite3";
pub const MAX_RETRY_ATTEMPTS: u32 = 3;

// BAD
pub const routerGateDenom: f32 = 32767.0;
pub const defaultDbPath: &str = "...";
```

---

## Test Function Naming

Use descriptive snake_case with verb phrase:

```rust
#[test]
fn route_with_adapter_info_rejects_non_finite_priors() { ... }

#[test]
fn delete_adapter_removes_adapter() { ... }

#[test]
fn delete_adapter_cross_tenant_returns_404() { ... }
```

Pattern: `{action}_{condition_or_scenario}` or `{subject}_{expected_behavior}`

---

## Naming Violations to Avoid

### Common Mistakes

1. **Using `initialize_*` outside boot module** - Use `init_*` instead
2. **Using `remove_*` when deleting entities** - Use `delete_*` for storage deletion
3. **Inconsistent acronym casing** - Follow the acronym table above
4. **Missing domain prefix on errors** - Use `Aos*Error` pattern
5. **Vague type suffixes** - Use specific suffixes like `Config`, `Context`, `Result`

<a id="grandfathered-exceptions"></a>
### Legacy Exceptions

The following patterns are legacy exceptions but should not be used in new code:

- `initialize_*` in `adapteros-server/src/boot/` (boot phase convention - returns rich Context objects)

---

## Changelog

| Date | Change |
|------|--------|
| 2026-01-17 | Initial naming conventions document |
