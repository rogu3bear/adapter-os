# Error Helpers

Extension traits for ergonomic error handling in AdapterOS.

## Quick Reference

```rust
use adapteros_core::error_helpers::{
    DbErrorExt, IoErrorExt, CryptoErrorExt,
    ValidationErrorExt, ConfigErrorExt
};

// Database operations
sqlx::query("...").fetch_one(&pool).await.db_err("fetch adapter")?;
query_result.db_context(|| format!("update adapter {}", id))?;

// I/O operations
fs::read_to_string(path).io_err("read file")?;
fs::read_to_string(path).io_err_path("read manifest", path)?;

// Cryptographic operations
sign_data(&key, &data).crypto_err("sign manifest")?;

// Validation
if name.is_empty() {
    return Err("cannot be empty").validation_err("adapter_name");
}
port_str.parse::<u16>().validation_err("port")?;

// Configuration
env_var.parse::<u16>().config_err("AOS_SERVER_PORT")?;
```

## Benefits

- **Less boilerplate**: Replace verbose `.map_err()` calls with concise helpers
- **Consistent formatting**: Standardized error messages across codebase
- **Type safety**: Compile-time guarantees for error types
- **Performance**: Zero overhead on success path, lazy evaluation on error path

## Documentation

See [docs/ERROR_HELPERS.md](../../../../docs/ERROR_HELPERS.md) for detailed usage guide.

## Examples

Run the demo:

```bash
cargo run --example error_helpers_demo -p adapteros-core
```

## Tests

```bash
cargo test -p adapteros-core error_helpers
```
