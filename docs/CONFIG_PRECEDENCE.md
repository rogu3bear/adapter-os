# AdapterOS Configuration Precedence

This document describes the deterministic configuration system with strict precedence rules and freeze enforcement.

## Overview

AdapterOS uses a deterministic configuration system with the following precedence order:
1. **CLI arguments** (highest priority)
2. **Environment variables** (medium priority)  
3. **Manifest file** (lowest priority)

Once frozen at startup, configuration becomes immutable and all environment variable access is banned to ensure deterministic behavior.

## Precedence Rules

### 1. CLI Arguments (Highest Priority)
Command-line arguments override all other sources:

```bash
# Override server port via CLI
adapteros serve --server.port 9090

# Override multiple values
adapteros serve --server.host 0.0.0.0 --server.port 8080 --policy.strict_mode false
```

**Format**: `--key value` or `--key` (for boolean flags)

### 2. Environment Variables (Medium Priority)
Environment variables override manifest files but are overridden by CLI arguments:

```bash
# Set server configuration via environment
export ADAPTEROS_SERVER_HOST=0.0.0.0
export ADAPTEROS_SERVER_PORT=8080
export ADAPTEROS_POLICY_STRICT_MODE=true

# Run application
adapteros serve
```

**Format**: `ADAPTEROS_` prefix, underscore to dot conversion, lowercase

### 3. Manifest File (Lowest Priority)
TOML configuration files provide defaults:

```toml
# configs/cp.toml
[server]
host = "127.0.0.1"
port = 8080
workers = 4

[database]
url = "sqlite://var/aos.db"
pool_size = 10

[policy]
strict_mode = true
audit_logging = true

[logging]
level = "info"
format = "json"
```

## Configuration Schema

The system validates configuration against a predefined schema:

### Server Configuration
- `server.host` (string): Server bind address (default: "127.0.0.1")
- `server.port` (integer): Server port number (default: 8080, range: 1-65535)
- `server.workers` (integer): Number of worker threads (default: 4, range: 1-64)

### Database Configuration
- `database.url` (string): Database connection URL (required)
- `database.pool_size` (integer): Connection pool size (default: 10, range: 1-100)

### Policy Configuration
- `policy.strict_mode` (boolean): Enable strict policy enforcement (default: true)
- `policy.audit_logging` (boolean): Enable policy audit logging (default: true)

### Logging Configuration
- `logging.level` (string): Logging level (default: "info", enum: debug,info,warn,error)
- `logging.format` (string): Logging format (default: "json", enum: json,text)

## Freeze Mechanism

### Configuration Freeze
Once initialized, configuration is frozen and becomes immutable:

```rust
use adapteros_config::{initialize_config, get_config};

// Initialize and freeze configuration
let config = initialize_config(cli_args, Some("configs/cp.toml".to_string()))?;

// Configuration is now frozen
assert!(config.is_frozen());

// Access configuration values
let host = config.get_or_default("server.host", "127.0.0.1");
let port: u16 = config.get("server.port")
    .and_then(|s| s.parse().ok())
    .unwrap_or(8080);
```

### Environment Variable Ban
After freeze, direct environment variable access is prohibited:

```rust
use adapteros_config::{safe_env_var, ConfigGuards};

// This will fail after freeze
let result = safe_env_var("PATH");
// Returns: Err(AosError::Config("Environment variable access prohibited after freeze"))

// Check if guards are frozen
if ConfigGuards::is_frozen() {
    println!("Configuration is frozen - env access prohibited");
}
```

## Usage Examples

### Basic Configuration Loading

```rust
use adapteros_config::{ConfigLoader, LoaderOptions};

// Create loader with default options
let loader = ConfigLoader::new();

// Load configuration from multiple sources
let config = loader.load(
    vec!["--server.port".to_string(), "9090".to_string()],
    Some("configs/cp.toml".to_string())
)?;

// Configuration is automatically frozen
assert!(config.is_frozen());
```

### Custom Loader Options

```rust
use adapteros_config::{ConfigLoader, LoaderOptions};

// Create custom loader options
let options = LoaderOptions {
    strict_mode: true,
    validate_types: true,
    allow_unknown_keys: false,
    env_prefix: "ADAPTEROS_".to_string(),
};

let loader = ConfigLoader::with_options(options);
```

### Configuration Validation

```rust
// Validate configuration against schema
let validation_errors = config.validate()?;
if !validation_errors.is_empty() {
    for error in validation_errors {
        eprintln!("Validation error: {} - {}", error.key, error.message);
    }
    return Err(AosError::Config("Configuration validation failed"));
}
```

## Failure Modes

### 1. Invalid Configuration File
```bash
# Malformed TOML file
echo "invalid toml content" > config.toml
adapteros serve --config config.toml
# Error: Failed to parse manifest file config.toml: expected `=`
```

### 2. Missing Required Fields
```bash
# Missing required database.url
adapteros serve
# Error: Configuration validation failed: database.url - Required field missing
```

### 3. Invalid Field Types
```bash
# Invalid port number
export ADAPTEROS_SERVER_PORT=invalid
adapteros serve
# Error: Configuration validation failed: server.port - Invalid integer value
```

### 4. Environment Variable Access After Freeze
```rust
// This will panic in strict mode or return error
let config = initialize_config(vec![], None)?;
let _ = std::env::var("PATH"); // Prohibited after freeze
```

### 5. Configuration Already Frozen
```rust
let config = initialize_config(vec![], None)?;
let result = initialize_config(vec![], None); // Second initialization fails
// Error: Configuration already initialized
```

## Debugging

### Enable Debug Logging
```bash
export RUST_LOG=adapteros_config=debug
adapteros serve
```

### Configuration Inspection
```rust
// Get configuration metadata
let metadata = config.get_metadata();
println!("Frozen at: {}", metadata.frozen_at);
println!("Hash: {}", metadata.hash);
println!("Sources: {} entries", metadata.sources.len());

// Get configuration as JSON
let json = config.to_json()?;
println!("Configuration: {}", json);
```

### Violation Tracking
```rust
use adapteros_config::ConfigGuards;

// Get all freeze violations
let violations = ConfigGuards::get_violations()?;
for violation in violations {
    println!("Violation: {} - {}", violation.attempted_operation, violation.message);
    if let Some(stack_trace) = violation.stack_trace {
        println!("Stack trace: {}", stack_trace);
    }
}
```

## Best Practices

### 1. Initialize Early
Initialize configuration as early as possible in application startup:

```rust
fn main() -> Result<()> {
    // Initialize configuration first
    let config = initialize_config(std::env::args().collect(), None)?;
    
    // Initialize other systems with frozen config
    let server = Server::new(config)?;
    server.run()
}
```

### 2. Use Safe Access Methods
Always use the provided safe access methods:

```rust
// Good: Use safe access methods
let host = config.get_or_default("server.host", "127.0.0.1");
let port = config.get("server.port")
    .and_then(|s| s.parse().ok())
    .unwrap_or(8080);

// Bad: Direct environment access after freeze
let host = std::env::var("ADAPTEROS_SERVER_HOST")?; // Prohibited
```

### 3. Validate Configuration
Always validate configuration before use:

```rust
let config = loader.load(cli_args, manifest_path)?;
let validation_errors = config.validate()?;
if !validation_errors.is_empty() {
    return Err(AosError::Config("Invalid configuration"));
}
```

### 4. Handle Errors Gracefully
Provide meaningful error messages for configuration issues:

```rust
match initialize_config(cli_args, manifest_path) {
    Ok(config) => {
        println!("Configuration loaded successfully");
        // Use config...
    }
    Err(AosError::Config(msg)) => {
        eprintln!("Configuration error: {}", msg);
        std::process::exit(1);
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
        std::process::exit(1);
    }
}
```

## Integration with Other Systems

### Trace Integration
Configuration hash is included in root trace node:

```rust
use adapteros_trace::TraceWriter;

let config = initialize_config(cli_args, manifest_path)?;
let trace_writer = TraceWriter::new()?;

// Include configuration hash in trace
trace_writer.log_event("config_initialized", serde_json::json!({
    "hash": config.get_metadata().hash,
    "frozen_at": config.get_metadata().frozen_at,
    "sources": config.get_metadata().sources.len()
}))?;
```

### Policy Integration
Configuration values can be used by policy enforcement:

```rust
use adapteros_policy::PolicyEngine;

let config = initialize_config(cli_args, manifest_path)?;
let policy_engine = PolicyEngine::new()?;

// Check policy configuration
let strict_mode = config.get("policy.strict_mode")
    .map(|s| s == "true")
    .unwrap_or(true);

if strict_mode {
    policy_engine.enable_strict_mode()?;
}
```

## Migration Guide

### From Ad-hoc Environment Access
Replace direct environment variable access with configuration system:

```rust
// Old: Direct environment access
let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
let port = std::env::var("SERVER_PORT")
    .unwrap_or_else(|_| "8080".to_string())
    .parse::<u16>()
    .unwrap_or(8080);

// New: Configuration system
let config = initialize_config(cli_args, manifest_path)?;
let host = config.get_or_default("server.host", "127.0.0.1");
let port = config.get("server.port")
    .and_then(|s| s.parse().ok())
    .unwrap_or(8080);
```

### From Manual Configuration Parsing
Replace manual configuration parsing with schema validation:

```rust
// Old: Manual parsing
let config: Config = toml::from_str(&config_content)?;
if config.server.port < 1 || config.server.port > 65535 {
    return Err("Invalid port number");
}

// New: Schema validation
let config = loader.load(cli_args, manifest_path)?;
// Validation happens automatically
```

## References

- [AdapterOS Configuration API](../crates/adapteros-config/src/lib.rs)
- [Configuration Types](../crates/adapteros-config/src/types.rs)
- [Precedence System](../crates/adapteros-config/src/precedence.rs)
- [Configuration Loader](../crates/adapteros-config/src/loader.rs)
- [Configuration Guards](../crates/adapteros-config/src/guards.rs)
- [Policy Registry](../crates/adapteros-policy/src/registry.rs)
- [Trace System](../crates/adapteros-trace/src/lib.rs)
