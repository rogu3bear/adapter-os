# CLI HTTP Client Consolidation Plan

**Created:** 2026-01-23
**Status:** Design Document (No Implementation)
**Scope:** `crates/adapteros-cli/src/commands/`

## Problem Statement

The CLI crate contains 70+ HTTP client instantiation patterns spread across 20+ command files. Each command that needs to call the control plane API creates its own `reqwest::Client`, constructs URLs manually, and duplicates error handling logic.

### Current Duplication Patterns

**Pattern A: Basic Client with URL Construction (most common)**
```rust
let client = reqwest::Client::new();
let url = format!("{}/v1/endpoint", base_url.trim_end_matches('/'));
let resp = client
    .post(&url)
    .json(&body)
    .send()
    .await
    .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

if !resp.status().is_success() {
    let text = resp.text().await.unwrap_or_default();
    return Err(AosError::Http(format!("Failed to do X: {} {}", status, text)));
}

let result: ResponseType = resp.json().await.map_err(|e| AosError::Http(e.to_string()))?;
```

**Pattern B: Client with Timeout Builder**
```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(30))
    .build()
    .map_err(|e| AosError::Io(format!("HTTP client build failed: {e}")))?;
```

**Pattern C: Client with Cookie Store (for auth flows)**
```rust
let client = Client::builder()
    .cookie_store(true)
    .build()
    .map_err(|e| AosError::Io(format!("HTTP client build failed: {e}")))?;
```

## Existing Infrastructure

The crate already has partial infrastructure in `crates/adapteros-cli/src/http_client.rs`:

```rust
// Extracts cookies from response headers
pub fn extract_cookie(headers: &HeaderMap, name: &str) -> Option<String>

// Refreshes access/refresh tokens
pub async fn refresh_tokens(client: &Client, auth: &mut AuthStore) -> Result<()>

// Sends request with auto-retry on 401
pub async fn send_with_refresh<F>(client: &Client, auth: &mut AuthStore, build: F) -> Result<Response>

// Sends request using stored auth with auto-refresh
pub async fn send_with_refresh_from_store<F>(client: &Client, build: F) -> Result<Response>
```

**Files already using `http_client`:**
- `commands/adapter.rs`
- `commands/datasets.rs`
- `commands/train_cli.rs`
- `commands/diag_health.rs`
- `commands/verify.rs`
- `commands/auth_cli.rs`
- `commands/storage.rs`
- `commands/verify_receipt.rs`

**Files NOT using shared infrastructure (sampling):**
- `commands/chat.rs` - 5 instances of `reqwest::Client::new()`
- `commands/stack.rs` - 7 instances
- `commands/node.rs` - 5 instances
- `commands/rollback.rs` - 1 instance
- `commands/code.rs` - 5 instances
- `commands/review.rs` - 5 instances
- `commands/coreml_status.rs` - 1 instance
- `commands/aos.rs` - 1 instance
- `commands/node_sync.rs` - 3 instances
- `commands/node_list.rs` - 1 instance
- `commands/coreml_export.rs` - 2 instances
- `commands/init.rs` - 1 instance
- `commands/check.rs` - 1 instance
- `commands/doctor.rs` - 1 instance
- `commands/status.rs` - 1 instance

## Metrics

| Metric | Count |
|--------|-------|
| Files with `reqwest::Client::new()` | 27 |
| Total `reqwest::Client` instantiations | 70+ |
| Uses of `trim_end_matches('/')` for URL | 50+ |
| Distinct error mapping patterns | 3 (Io, Http, Network) |
| Files already using `http_client` module | 8 |

## Proposed Solution

### 1. `HttpApiClient` Struct

Create a typed API client that encapsulates:
- Base URL with trailing slash normalization
- Optional authentication (bearer token or cookie-based)
- Configurable timeout
- Consistent error handling
- JSON/SSE response parsing

```rust
// crates/adapteros-cli/src/http_client.rs (extension)

pub struct HttpApiClient {
    inner: reqwest::Client,
    base_url: String,
    auth: Option<AuthMode>,
}

pub enum AuthMode {
    Bearer(String),
    Cookie { name: String, value: String },
    FromStore,  // Use AuthStore
}

impl HttpApiClient {
    pub fn new(base_url: &str) -> Result<Self>;
    pub fn with_timeout(base_url: &str, timeout: Duration) -> Result<Self>;
    pub fn with_auth(self, auth: AuthMode) -> Self;
    pub fn with_cookie_store(base_url: &str) -> Result<Self>;

    // Request builders
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T>;
    pub async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T>;
    pub async fn put<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T>;
    pub async fn delete(&self, path: &str) -> Result<()>;

    // For custom request building
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder;

    // Auto-refresh enabled variants
    pub async fn get_with_refresh<T: DeserializeOwned>(&self, path: &str) -> Result<T>;
    pub async fn post_with_refresh<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T>;
}
```

### 2. Error Normalization

Consolidate the three error patterns into a single, consistent approach:

```rust
// Current: inconsistent
AosError::Io(format!("HTTP request failed: {}", e))
AosError::Http(format!("Failed to X: {} {}", status, text))
AosError::Network(format!("Failed to connect: {}", e))

// Proposed: unified
impl From<ClientError> for AosError {
    fn from(err: ClientError) -> Self {
        match err {
            ClientError::Network(e) => AosError::Network(e),
            ClientError::Http { status, body, context } =>
                AosError::Http(format!("{}: {} {}", context, status, body)),
            ClientError::Decode(e) => AosError::Http(format!("Response decode failed: {}", e)),
        }
    }
}
```

### 3. Migration Strategy

**Phase 1: Foundation (non-breaking)**
- Extend `http_client.rs` with `HttpApiClient` struct
- Add comprehensive tests for new API
- Document usage patterns

**Phase 2: Migrate Auth-Required Commands**
Commands already importing `http_client`:
- `adapter.rs` - refactor remaining direct client uses
- `datasets.rs` - already mostly compliant
- `train_cli.rs` - already mostly compliant
- `diag_health.rs` - minor cleanup
- `verify.rs` - already using `send_with_refresh_from_store`
- `storage.rs` - already using `send_with_refresh`

**Phase 3: Migrate Simple Commands**
Commands with basic patterns (no auth required):
- `chat.rs`
- `stack.rs`
- `node.rs`, `node_sync.rs`, `node_list.rs`
- `rollback.rs`
- `aos.rs`
- `check.rs`, `doctor.rs`, `status.rs`

**Phase 4: Migrate Complex Commands**
Commands with special requirements:
- `code.rs`, `review.rs` - multiple endpoints, complex flows
- `coreml_status.rs`, `coreml_export.rs` - timeout-sensitive
- `init.rs` - cookie store requirements

## Commands to Refactor

### High Priority (frequent usage, simple patterns)
| File | Instances | Notes |
|------|-----------|-------|
| `stack.rs` | 7 | All same pattern, no auth |
| `chat.rs` | 5 | Streaming responses |
| `node.rs` | 5 | Custom timeout, error type |
| `code.rs` | 5 | Multiple endpoints |
| `review.rs` | 5 | Multiple endpoints |

### Medium Priority (moderate complexity)
| File | Instances | Notes |
|------|-----------|-------|
| `node_sync.rs` | 3 | Simple pattern |
| `coreml_export.rs` | 2 | Custom timeout |
| `adapter.rs` | ~10 | Mixed patterns, some already migrated |
| `scenario.rs` | 3 | Test/probe scenarios |

### Low Priority (single instance or special cases)
| File | Instances | Notes |
|------|-----------|-------|
| `rollback.rs` | 1 | Simple |
| `aos.rs` | 1 | Simple |
| `node_list.rs` | 1 | Simple |
| `check.rs` | 1 | With builder |
| `doctor.rs` | 1 | With builder |
| `status.rs` | 1 | With builder |
| `init.rs` | 1 | Cookie store |
| `coreml_status.rs` | 1 | With builder |

## API Design Examples

### Before (current pattern in `stack.rs`)
```rust
async fn create_stack(name: &str, adapters: &[String], base_url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/adapter-stacks", base_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AosError::Http(format!("Failed to create stack: {} {}", status, text)));
    }

    let stack: StackResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

    // ...
}
```

### After (proposed pattern)
```rust
async fn create_stack(name: &str, adapters: &[String], base_url: &str) -> Result<()> {
    let client = HttpApiClient::new(base_url)?;

    let stack: StackResponse = client
        .post("/v1/adapter-stacks", &request)
        .await
        .context("Failed to create stack")?;

    // ...
}
```

### After with Auth
```rust
async fn create_stack(name: &str, adapters: &[String]) -> Result<()> {
    let client = HttpApiClient::from_auth_store()?;

    let stack: StackResponse = client
        .post_with_refresh("/v1/adapter-stacks", &request)
        .await
        .context("Failed to create stack")?;

    // ...
}
```

## Testing Strategy

1. **Unit tests** for `HttpApiClient` methods
2. **Integration tests** with mock server (using existing `axum` test patterns)
3. **Migration tests** - verify behavior parity with existing implementations
4. **Error handling tests** - verify error messages match expected format

## Non-Goals

- Changing the underlying `reqwest` library
- Modifying the `AuthStore` persistence format
- Adding retry logic (beyond existing 401 refresh)
- Connection pooling optimization (defer to future work)

## Open Questions

1. Should `HttpApiClient` be `Clone`? (Currently, `reqwest::Client` is cheap to clone)
2. Should we add request/response logging at this layer?
3. Should streaming (SSE) responses use a different API surface?
4. Should we add rate limiting at the client level?

## Success Criteria

- [ ] All 70+ client instantiations reduced to ~20 shared instances
- [ ] Consistent error messages across all HTTP operations
- [ ] No direct `reqwest::Client::new()` calls in command files
- [ ] Unit test coverage for new `HttpApiClient` API
- [ ] No behavioral changes to existing CLI commands

## Timeline Estimate

| Phase | Effort |
|-------|--------|
| Phase 1: Foundation | 2-3 hours |
| Phase 2: Auth commands | 1-2 hours |
| Phase 3: Simple commands | 2-3 hours |
| Phase 4: Complex commands | 2-3 hours |
| **Total** | **7-11 hours** |

---

*This is a documentation-only plan. No code changes have been made.*
