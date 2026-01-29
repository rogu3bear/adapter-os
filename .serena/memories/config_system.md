# AdapterOS Configuration System

## Overview

The AdapterOS configuration system is a **deterministic configuration framework** located in `crates/adapteros-config/`. It provides strict precedence rules, type validation, and immutability after initialization to ensure reproducible behavior across runs.

## File Formats

### TOML Config Files (Primary)
- Located in `configs/` directory
- Main config: `configs/cp.toml` (control plane)
- Uses nested sections: `[server]`, `[db]`, `[security]`, `[paths]`, etc.
- Example structure:
```toml
[server]
port = 8080
bind = "127.0.0.1"

[db]
path = "sqlite://var/aos-cp.sqlite3"
storage_mode = "sql_only"

[security]
jwt_secret = "..."
dev_bypass = true
```

### Environment Variables
- Prefix: `AOS_*` (canonical) or `ADAPTEROS_*` (legacy, deprecated)
- Loaded from `.env` file via `dotenvy`
- Examples: `AOS_SERVER_PORT`, `AOS_MODEL_PATH`, `AOS_DATABASE_URL`

## Precedence Rules (Highest to Lowest)

1. **CLI Arguments** (`--key value`) - Highest priority
2. **Environment Variables** (`AOS_*`) - Medium priority  
3. **TOML Manifest File** - Lower priority
4. **Schema Defaults** - Lowest priority (fallback)

Defined in `src/types.rs`:
```rust
pub enum PrecedenceLevel {
    Manifest = 0,    // Lowest
    Environment = 1,
    Cli = 2,         // Highest
}
```

## Key Structs

### EffectiveConfig (Recommended Entry Point)
Located in `src/effective.rs`. The unified configuration facade with type-safe sections:

```rust
pub struct EffectiveConfig {
    pub server: ServerSection,      // port, host, production_mode
    pub database: DatabaseSection,  // url, pool_size, timeout_secs
    pub security: SecuritySection,  // jwt_secret, jwt_mode, dev_bypass
    pub paths: PathsSection,        // var_dir, adapters_root, datasets_root
    pub logging: LoggingSection,    // level, log_dir, json_format
    pub model: ModelSection,        // path, backend, base_id
    pub inference: InferenceSection, // seed_mode, backend_profile
    // ... more sections
}
```

**Usage:**
```rust
use adapteros_config::{init_effective_config, effective_config};

// Initialize once at startup
init_effective_config(Some("configs/cp.toml"), vec![])?;

// Access anywhere
let cfg = effective_config()?;
println!("Port: {}", cfg.server.port);
```

### DeterministicConfig
Located in `src/precedence.rs`. Lower-level config with BLAKE3 hashing:
- Stores raw key-value pairs
- Tracks source of each value
- Computes deterministic hash after freezing
- Immutable once frozen

### ConfigSchema
Located in `src/schema.rs`. Defines all valid `AOS_*` variables with:
- Type constraints (String, Integer, Bool, Enum, Duration, ByteSize, Path, Url)
- Validation rules (ranges, allowed values)
- Default values
- Deprecation tracking
- Sensitive value redaction

## Adding New Config Options

### Step 1: Add to Schema (`src/schema.rs`)
```rust
schema.add_variable(
    ConfigVariable::new("AOS_MY_NEW_OPTION")
        .config_type(ConfigType::Integer { min: Some(1), max: Some(100) })
        .default_value("10")
        .description("Description of the option")
        .category("CATEGORY_NAME")
        .config_key("my.new.option")  // Dot notation for config_key
        .toml_key("section.key")      // Optional: TOML key if different
        .build(),
);
```

### Step 2: Add to EffectiveConfig Section (`src/effective.rs`)
Add field to appropriate section struct and build method:
```rust
// In section struct
pub my_new_option: u32,

// In build method
fn build_xxx_section(config: &DeterministicConfig) -> XxxSection {
    XxxSection {
        my_new_option: config
            .get("my.new.option")
            .and_then(|v| v.parse().ok())
            .unwrap_or(10),
    }
}
```

### Step 3: Add to TOML Types (`src/types.rs`)
If adding to a section that has serde structs:
```rust
pub struct MyConfig {
    #[serde(default = "default_my_option")]
    pub my_option: u32,
}

fn default_my_option() -> u32 { 10 }
```

## Configuration Guards

Located in `src/guards.rs`. Prevents environment variable access after configuration is frozen:

```rust
// Safe access before freeze
let value = ConfigGuards::safe_env_var("AOS_MODEL_PATH")?;

// After freeze, env access returns error and records violation
ConfigGuards::freeze()?;
```

## Path Resolution

Located in `src/path_resolver.rs`. Resolves paths with precedence and security:

- **Security**: Rejects paths under `/tmp` or `/private/tmp` (prevents data loss on reboot)
- **Symlink detection**: Canonicalizes paths to detect symlink attacks
- **Var rebasing**: Relative paths are rebased under `AOS_VAR_DIR`

Key functions:
- `resolve_base_model_location()` - Model path with cache root
- `resolve_adapters_root()` - Adapter weights directory
- `resolve_database_url()` - Database connection URL
- `resolve_worker_socket_for_cp()` - Unix domain socket

## Configuration Categories

From `default_schema()`:
- MODEL - Model paths, backends, cache
- SERVER - Port, host, production mode
- DATABASE - URL, pool size, storage mode
- SECURITY - JWT, auth, dev bypass
- LOGGING - Level, format, rotation
- INFERENCE - Seed mode, backend profile
- PATHS - Runtime directories
- TRAINING - Checkpoints, epochs
- And more...

## Production vs Development Mode

**Production mode** (`server.production_mode = true`) enforces:
- Absolute paths required
- JWT secret minimum 64 characters
- `dev_bypass = false` required
- `dev_login_enabled = false` required
- Strict seed mode for inference
- Explicit backend profile (no "auto")

**Development mode** allows:
- Relative paths
- Short/default JWT secrets
- Dev bypass enabled
- Best-effort seed mode

## Key Files Summary

| File | Purpose |
|------|---------|
| `src/lib.rs` | Module exports, `initialize_config()` |
| `src/effective.rs` | `EffectiveConfig` - recommended API |
| `src/schema.rs` | `ConfigSchema`, variable definitions |
| `src/loader.rs` | `ConfigLoader` - precedence loading |
| `src/precedence.rs` | `DeterministicConfig`, `ConfigBuilder` |
| `src/types.rs` | Business logic structs, serde types |
| `src/guards.rs` | `ConfigGuards`, `FeatureFlags` |
| `src/path_resolver.rs` | Path resolution with security |
| `src/global.rs` | Global `RuntimeConfig` access |
| `src/runtime.rs` | `RuntimeConfig` with validation |

## Config Hash for Determinism

Configuration produces a BLAKE3 hash for reproducibility:
```rust
let cfg = effective_config()?;
println!("Config hash: {}", cfg.config_hash());
```

This hash can be used to verify identical configuration across runs.
