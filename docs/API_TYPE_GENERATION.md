# API Type Generation Guide

This document describes how API types are shared between the Rust backend and Leptos UI in adapterOS.

## Overview

adapterOS uses a **Rust-native** approach to API type sharing:

```
Rust Types (adapteros-api-types) ─────> Backend (adapteros-server-api)
                                 └────> Leptos UI (adapteros-ui, WASM)
```

Both the server and client use the same Rust types from the `adapteros-api-types` crate, ensuring compile-time type safety without code generation.

## Type Sharing Architecture

### Shared Types Crate

The `adapteros-api-types` crate contains all API request/response types:

```toml
# In crates/adapteros-api-types/Cargo.toml
[features]
default = []
wasm = ["serde-wasm-bindgen", "js-sys"]
```

### Server Usage

```rust
// In adapteros-server-api
use adapteros_api_types::adapters::Adapter;

pub async fn get_adapter() -> Json<Adapter> {
    // ...
}
```

### Leptos UI Usage

```rust
// In adapteros-ui (WASM target)
use adapteros_api_types::adapters::Adapter;

#[component]
fn AdapterList() -> impl IntoView {
    let adapters = create_resource(|| (), |_| async {
        api::get_adapters().await
    });
    // ...
}
```

### Cargo.toml Configuration

```toml
# In crates/adapteros-ui/Cargo.toml
[dependencies]
adapteros-api-types = { path = "../adapteros-api-types", features = ["wasm"] }
```

## Benefits

1. **Compile-time safety** - Type mismatches are caught at compile time
2. **No code generation** - No build steps for type synchronization
3. **Single source of truth** - Types defined once in Rust
4. **IDE support** - Full autocomplete and type checking in all consumers
5. **Refactoring safety** - Rename a field and all usages are updated

## OpenAPI Documentation

OpenAPI specs are still generated for external documentation and potential future SDK generation:

```bash
# Generate OpenAPI documentation
cargo xtask openapi-docs

# Validate OpenAPI spec
./scripts/validate_openapi_docs.sh
```

The OpenAPI spec is generated from utoipa annotations in `adapteros-server-api` and stored at `docs/api/openapi.json`.

## Workflow for API Changes

### 1. Modify Types

Edit types in `crates/adapteros-api-types/src/`:

```rust
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct MyNewType {
    pub id: String,
    pub name: String,
}
```

### 2. Update Server Handler

```rust
use adapteros_api_types::MyNewType;

#[utoipa::path(
    get,
    path = "/api/my-endpoint",
    responses(
        (status = 200, description = "Success", body = MyNewType)
    )
)]
pub async fn my_handler() -> Json<MyNewType> {
    // implementation
}
```

### 3. Update UI

```rust
use adapteros_api_types::MyNewType;

async fn fetch_my_data() -> Result<MyNewType, ApiError> {
    api::get("/api/my-endpoint").await
}
```

### 4. Build and Test

```bash
# Check WASM compilation
cargo check -p adapteros-ui --target wasm32-unknown-unknown

# Run UI unit tests
cargo test -p adapteros-ui --lib

# Build production UI
cd crates/adapteros-ui && trunk build --release
```

## Testing

```bash
# Full test suite (includes Leptos UI tests)
bash scripts/test/all.sh all

# Leptos UI tests only
bash scripts/test/all.sh ui

# Check WASM target compiles
cargo check -p adapteros-ui --target wasm32-unknown-unknown
```

## Related Documentation

- [Leptos UI](../crates/adapteros-ui/README.md)
- [API Types Crate](../crates/adapteros-api-types/README.md)
- [CLAUDE.md](../CLAUDE.md) - Development guide
