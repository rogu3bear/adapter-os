# AdapterOS Developer Guide

**Purpose:** Developer-focused guide with code examples, architecture patterns, and coding standards.  
**For contribution process:** See [CONTRIBUTING.md](CONTRIBUTING.md)  
**Last Updated:** 2025-01-15

---

## Table of Contents

- [Code Standards](#code-standards)
- [Error Handling](#error-handling)
- [Logging](#logging)
- [Policy Packs](#policy-packs)
- [Architecture Patterns](#architecture-patterns)
- [Common Patterns](#common-patterns)
- [Anti-Patterns to Avoid](#anti-patterns-to-avoid)

---

## Code Standards

### Rust Style

```rust
// ✅ GOOD: Use cargo fmt for formatting
// Run: cargo fmt --all

// Use standard Rust naming conventions:
// - Types: PascalCase
// - Functions: snake_case
// - Constants: SCREAMING_SNAKE_CASE
// - Modules: snake_case
```

### Linting

```bash
# Always run clippy before committing
cargo clippy --workspace -- -D warnings

# Check for unused dependencies
cargo udeps
```

### Documentation

```rust
// ✅ GOOD: Document all public APIs
/// Loads an adapter from the specified path.
///
/// # Arguments
/// * `path` - Path to the adapter file
///
/// # Errors
/// Returns `AosError::NotFound` if the file doesn't exist.
/// Returns `AosError::InvalidManifest` if the manifest is malformed.
///
/// # Example
/// ```no_run
/// use adapteros_lora_lifecycle::AdapterLoader;
/// let loader = AdapterLoader::new();
/// let adapter = loader.load_from_path("./adapters/my_adapter.aos").await?;
/// ```
pub async fn load_from_path(path: &Path) -> Result<Adapter> {
    // Implementation
}
```

---

## Error Handling

### Error Type: `AosError`

All errors use the `AosError` enum from `adapteros-core`:

```rust
use adapteros_core::{AosError, Result};

// ✅ GOOD: Use Result<T> for error handling
pub async fn process_request(&self, req: Request) -> Result<Response> {
    let data = load_data(&req.id)
        .await
        .map_err(|e| AosError::NotFound(format!("Failed to load {}: {}", req.id, e)))?;
    
    Ok(Response::new(data))
}

// ❌ BAD: Using Option<T> for errors
pub fn get_value(&self, key: &str) -> Option<String> {
    // Should return Result<String, AosError>
}
```

### Error Propagation

```rust
// ✅ GOOD: Proper error propagation
use adapteros_core::Result;

pub async fn complex_operation(&self) -> Result<()> {
    // Use ? operator for error propagation
    let config = load_config().await?;
    let data = process_data(&config).await?;
    validate_data(&data)?;
    
    Ok(())
}

// ✅ GOOD: Adding context to errors
pub async fn load_adapter(path: &Path) -> Result<Adapter> {
    let bytes = std::fs::read(path)
        .map_err(|e| AosError::Io(format!("Failed to read {}: {}", path.display(), e)))?;
    
    // Further processing...
}
```

### Error Variants

Common error variants in `AosError`:

```rust
// Domain-specific errors
AosError::PolicyViolation("reason")          // Policy enforcement
AosError::DeterminismViolation("reason")    // Non-deterministic behavior
AosError::EgressViolation("reason")         // Network egress blocked
AosError::IsolationViolation("reason")     // Tenant isolation
AosError::Validation("reason")              // Input validation
AosError::Config("reason")                  // Configuration errors

// Infrastructure errors
AosError::Io("reason")                      // I/O errors
AosError::Database("reason")                // Database errors
AosError::Crypto("reason")                  // Cryptographic errors
AosError::Network("reason")                 // Network errors
```

---

## Logging

### Use `tracing` (Not `println!`)

```rust
use tracing::{info, warn, error, debug, trace};

// ✅ GOOD: Structured logging with tracing
pub async fn process_request(&self, req: Request) -> Result<Response> {
    info!(request_id = %req.id, "Processing request");
    
    let result = self.handle(req).await?;
    
    info!(
        request_id = %req.id,
        duration_ms = ?result.duration,
        "Request processed successfully"
    );
    
    Ok(result)
}

// ❌ BAD: Using println! for logging
pub fn log_event(&self, event: &str) {
    println!("Event: {}", event); // DON'T DO THIS
}
```

### Log Levels

```rust
// Use appropriate log levels
trace!("Detailed debugging information");
debug!("Debug information for development");
info!("General informational messages");
warn!("Warning messages that may need attention");
error!("Error messages that require action");
```

### Structured Fields

```rust
// ✅ GOOD: Use structured fields for querying
info!(
    tenant_id = %tenant.id,
    adapter_id = %adapter.id,
    request_size = req.len(),
    "Loading adapter for tenant"
);

// Fields can be queried in log aggregation systems
```

---

## Policy Packs

AdapterOS enforces 20 canonical policy packs. All code must comply with these policies.

### Core Policy Packs

1. **Egress Policy** - Zero network egress during inference
   ```rust
   // Production mode enforces UDS-only serving
   if cfg.server.production_mode {
       if cfg.server.uds_socket.is_none() {
           return Err(AosError::Config(
               "Production mode requires uds_socket".to_string()
           ));
       }
   }
   ```

2. **Determinism Policy** - Reproducible execution
   ```rust
   // All randomness must be seeded
   use adapteros_deterministic_exec::GlobalSeed;
   let seed = GlobalSeed::get_or_init(seed_hash);
   let mut rng = seed.rng();
   ```

3. **Router Policy** - K-sparse LoRA routing with Q15 gates
   ```rust
   // Router uses Q15 quantized gates
   let gate_value = quantize_to_q15(feature_value);
   ```

4. **Evidence Policy** - Audit trail for policy decisions
   ```rust
   use adapteros_policy::evidence_tracker::EvidenceTracker;
   evidence_tracker.record(policy_decision).await?;
   ```

5. **Telemetry Policy** - Structured event logging
   ```rust
   // All events logged as canonical JSON
   telemetry.log_event("event_type", metadata).await?;
   ```

### Policy Compliance Checklist

- [ ] No network egress in production (UDS-only)
- [ ] All randomness is seeded and deterministic
- [ ] Router uses Q15 quantization
- [ ] Evidence tracked for policy decisions
- [ ] Telemetry events use canonical JSON
- [ ] Input validation on all user inputs
- [ ] Tenant isolation enforced
- [ ] Error handling with typed errors

See `crates/adapteros-policy/src/packs/` for complete policy implementations.

---

## Architecture Patterns

### K-Sparse LoRA Routing

```rust
// Router selects top K adapters using Q15 quantized gates
use adapteros_lora_router::{Router, RouterRequest};

let router = Router::new(config);
let request = RouterRequest {
    prompt_tokens: tokens,
    model_id: model.id.clone(),
    tenant_id: tenant.id.clone(),
};

// Returns top K adapters (typically K=3)
let selected = router.select_adapters(request, k_sparse: 3).await?;
```

### Metal Kernel Pattern

```rust
// Metal kernels use deterministic compilation
use adapteros_lora_kernel_mtl::{FusedKernels, KernelParams};

let kernels = FusedKernels::load_from_metallib("./target/kernels.metallib")?;

let params = KernelParams {
    hidden_size: 4096,
    seq_len: 128,
    // ... other parameters
};

// Kernels are precompiled for deterministic execution
kernels.execute(&params, buffers)?;
```

### Configuration Pattern

```rust
// Configuration uses precedence rules
use adapteros_config::{Config, ConfigSource};

// Precedence: CLI > Environment > Config File > Defaults
let config = Config::load()
    .with_file("configs/cp.toml")?
    .with_env()?
    .with_cli(&args)?
    .build()?;
```

### Memory Management Pattern

```rust
// Adapter eviction maintains ≥15% headroom
use adapteros_memory::{MemoryManager, EvictionPolicy};

let memory = MemoryManager::new(
    EvictionPolicy::default()
        .with_min_headroom_pct(15)
        .with_evict_order(["ephemeral_ttl", "cold_lru"])
);

// Automatically evicts adapters when memory pressure detected
memory.ensure_headroom().await?;
```

---

## Common Patterns

### Database Access

```rust
use adapteros_db::Db;
use sqlx::query;

// ✅ GOOD: Parameterized queries
let results = query("SELECT * FROM adapters WHERE tenant_id = ?")
    .bind(&tenant_id)
    .fetch_all(&db.pool)
    .await
    .map_err(|e| AosError::Database(format!("Query failed: {}", e)))?;
```

### Async Task Spawning

```rust
use tokio::spawn;

// ✅ GOOD: Proper error handling for spawned tasks
let handle = spawn(async move {
    if let Err(e) = do_work().await {
        error!(error = %e, "Background task failed");
        // Don't panic - log and continue
    }
});

// Store handle for cancellation if needed
```

### CLI Command Pattern

```rust
use adapteros_cli::{Command, Context};
use tracing::info;

#[derive(Args)]
pub struct LoadArgs {
    path: PathBuf,
}

pub async fn execute(args: LoadArgs, ctx: &Context) -> Result<()> {
    info!(path = %args.path.display(), "Loading adapter");
    
    let loader = ctx.adapter_loader();
    let adapter = loader.load_from_path(&args.path).await?;
    
    info!(adapter_id = %adapter.id, "Adapter loaded");
    Ok(())
}
```

### Production Mode Enforcement

```rust
// Production mode enforces M1 security requirements
if config.server.production_mode {
    // UDS-only serving
    if config.server.uds_socket.is_none() {
        return Err(AosError::Config(
            "Production mode requires uds_socket".to_string()
        ));
    }
    
    // Ed25519 JWTs only (no HMAC)
    if config.security.jwt_mode.as_deref() != Some("eddsa") {
        return Err(AosError::Config(
            "Production mode requires jwt_mode='eddsa'".to_string()
        ));
    }
    
    // Zero egress enforced
    if !config.security.require_pf_deny {
        return Err(AosError::Config(
            "Production mode requires require_pf_deny=true".to_string()
        ));
    }
}
```

---

## Anti-Patterns to Avoid

### ❌ TODO Comments Without Plans

```rust
// ❌ BAD: TODO with no completion plan
pub async fn start(&mut self) -> Result<()> {
    // TODO: Implement start logic
    Ok(())
}

// ✅ GOOD: Complete implementation or explicit error
pub async fn start(&mut self) -> Result<()> {
    self.watcher.start().await?;
    self.daemon.start().await?;
    Ok(())
}
```

### ❌ Placeholder Logic

```rust
// ❌ BAD: Placeholder that doesn't perform intended function
pub fn process(&self, data: &[u8]) -> Result<Processed> {
    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(Processed::default())
}

// ✅ GOOD: Real implementation
pub fn process(&self, data: &[u8]) -> Result<Processed> {
    let parsed = parse_data(data)?;
    let validated = validate(&parsed)?;
    Ok(Processed::new(validated))
}
```

### ❌ Missing Error Handling

```rust
// ❌ BAD: No error handling for edge cases
pub async fn load(&self, path: &Path) -> Result<Data> {
    let bytes = std::fs::read(path)?;
    Ok(deserialize(&bytes)?)
}

// ✅ GOOD: Comprehensive error handling
pub async fn load(&self, path: &Path) -> Result<Data> {
    let bytes = std::fs::read(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AosError::NotFound(format!("File not found: {}", path.display()))
            }
            std::io::ErrorKind::PermissionDenied => {
                AosError::Io(format!("Permission denied: {}", path.display()))
            }
            _ => AosError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;
    
    deserialize(&bytes)
        .map_err(|e| AosError::Serialization(format!("Invalid data: {}", e)))
}
```

### ❌ Using `println!` for Logging

```rust
// ❌ BAD: println! for logging
pub fn log_event(&self, event: &str) {
    println!("Event: {}", event);
}

// ✅ GOOD: Use tracing
pub fn log_event(&self, event: &str) {
    info!(event = %event, "Event occurred");
}
```

### ❌ Unsafe Code in Production Crates

```rust
// ❌ BAD: Unsafe code in application crates
pub unsafe fn manipulate_data(ptr: *mut u8) {
    *ptr = 42;
}

// ✅ GOOD: Keep unsafe code isolated to designated crates
// Only use unsafe in:
// - adapteros-lora-kernel-mtl (Metal FFI)
// - adapteros-lora-mlx-ffi (PyO3 bindings)
// With extensive documentation and tests
```

See [docs/DEPRECATED_PATTERNS.md](docs/DEPRECATED_PATTERNS.md) for more anti-patterns found in deprecated code.

---

## Key Subsystems

### Router (K-Sparse Selection)

**Location:** `crates/adapteros-lora-router/src/`

```rust
use adapteros_lora_router::Router;

// Q15 quantized gates for selection
let router = Router::new(config);
let top_k = router.select_adapters(request, k: 3).await?;
```

### Metal Kernels

**Location:** `crates/adapteros-lora-kernel-mtl/src/`

```rust
use adapteros_lora_kernel_mtl::FusedKernels;

// Deterministic precompiled kernels
let kernels = FusedKernels::load("./target/kernels.metallib")?;
```

### Policy Enforcement

**Location:** `crates/adapteros-policy/src/`

```rust
use adapteros_policy::{PolicyEngine, PolicyPack};

let engine = PolicyEngine::new(policy_packs);
engine.enforce(request).await?;
```

### Memory Management

**Location:** `crates/adapteros-memory/src/`

```rust
use adapteros_memory::MemoryManager;

// Automatic eviction maintains headroom
let memory = MemoryManager::new(eviction_policy);
memory.ensure_headroom().await?;
```

---

## Citation Standards

When referencing code, use deterministic citations:

```markdown
[source: crates/adapteros-server/src/main.rs L173-L218]
```

Format: `[source: <path> L<start>-L<end>]`

See [CITATIONS.md](CITATIONS.md) for complete citation standards.

---

## Quick Reference

### Build Commands

```bash
# Build workspace
cargo build --release

# Run tests
cargo test --workspace

# Format code
cargo fmt --all

# Lint code
cargo clippy --workspace -- -D warnings

# Check specific crate
cargo check -p adapteros-server
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Run with output
cargo test --workspace -- --nocapture

# Run specific test
cargo test test_adapter_loading

# Integration tests
cargo test --test integration_tests
```

### Common Debugging

```bash
# Check compilation errors
cargo check --workspace --message-format=short

# Find dead code
cargo clippy --workspace -- -W dead_code

# Find unused dependencies
cargo udeps
```

---

## References

- **CONTRIBUTING.md** - Contribution process and PR guidelines
- **README.md** - Project overview and quick start
- **docs/DEPRECATED_PATTERNS.md** - Anti-patterns to avoid
- **docs/ARCHITECTURE_INDEX.md** - Complete architecture reference
- **crates/adapteros-policy/** - Policy pack implementations
- **crates/adapteros-core/src/error.rs** - Error type definitions

---

**Remember:** When in doubt, check existing code patterns in `crates/` and follow established conventions.

