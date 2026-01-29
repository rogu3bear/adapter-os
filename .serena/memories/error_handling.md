# AdapterOS Error Handling Patterns

## Overview

AdapterOS implements a comprehensive, multi-layered error handling system designed for:
- Type-safe error handling without string parsing
- Compile-time exhaustive error code mapping
- HTTP API error serialization with redaction
- CLI-friendly error codes and exit codes
- Automatic error chain preservation

## Core Error Types

### 1. AosError (adapteros-core/src/error.rs)

The original flat enum with 70+ variants covering all error domains. Uses `thiserror` derive macro.

**Key characteristics:**
- Flat structure - all variants at one level
- Rich structured variants for debugging (e.g., `CacheBudgetExceeded` with memory details)
- Convenience constructors like `AosError::config()`, `AosError::validation()`
- `ResultExt` trait for `.context()` and `.with_context()` chaining
- `WithContext` variant for error chain preservation

**Error message standards (documented in module header):**
1. Start with capital letter
2. Use "Action failed: reason" format
3. Use `format!()` for dynamic values
4. No trailing periods
5. Be specific and actionable

### 2. Hierarchical Errors (adapteros-core/src/errors/)

A newer structured hierarchy with categorical sub-enums:

```
AosError (wrapper)
├── Network    (AosNetworkError)
├── Storage    (AosStorageError)
├── Policy     (AosPolicyError)
├── Crypto     (AosCryptoError)
├── Adapter    (AosAdapterError)
├── Model      (AosModelError)
├── Validation (AosValidationError)
├── Resource   (AosResourceError)
├── Auth       (AosAuthError)
├── Operations (AosOperationsError)
└── Internal   (AosInternalError)
```

Each sub-enum uses `#[error(transparent)]` for automatic delegation.

## Error Code System

### String Error Codes (adapteros-core/src/error_codes.rs)

~90+ canonical string constants for API responses organized by HTTP status:

**400 Bad Request:**
- `BAD_REQUEST`, `VALIDATION_ERROR`, `PARSE_ERROR`
- `INVALID_HASH`, `INVALID_CPID`, `INVALID_MANIFEST`
- `ADAPTER_NOT_IN_MANIFEST`, `ADAPTER_NOT_IN_EFFECTIVE_SET`

**401 Unauthorized:**
- `TOKEN_MISSING`, `TOKEN_INVALID`, `TOKEN_EXPIRED`
- `INVALID_CREDENTIALS`, `SESSION_EXPIRED`

**403 Forbidden:**
- `POLICY_VIOLATION`, `DETERMINISM_VIOLATION`
- `EGRESS_VIOLATION`, `ISOLATION_VIOLATION`
- `TENANT_ISOLATION_ERROR`

**404 Not Found:**
- `NOT_FOUND`, `ADAPTER_NOT_FOUND`, `MODEL_NOT_FOUND`

**409 Conflict:**
- `ADAPTER_HASH_MISMATCH`, `POLICY_HASH_MISMATCH`
- `DUPLICATE_REQUEST`

**429 Too Many Requests:**
- `TOO_MANY_REQUESTS`, `BACKPRESSURE`

**5xx Server Errors:**
- `INTERNAL_ERROR`, `DATABASE_ERROR`, `CRYPTO_ERROR`
- `SERVICE_UNAVAILABLE`, `CIRCUIT_BREAKER_OPEN`
- `CACHE_BUDGET_EXCEEDED`, `OUT_OF_MEMORY`

### Typed Error Codes (ECode enum in CLI)

Numeric error codes E1xxx-E9xxx by category:
- E1xxx: Crypto/Signing
- E2xxx: Policy/Determinism
- E3xxx: Kernels/Build/Manifest
- E4xxx: Telemetry/Chain
- E5xxx: Artifacts/CAS
- E6xxx: Adapters/DIR
- E7xxx: Node/Cluster
- E8xxx: CLI/Config
- E9xxx: OS/Environment

Each ECode has:
- `title`, `cause`, `fix` - human-readable explanations
- `category` - for grouping
- `related_docs` - links to relevant documentation

### CLI Exit Codes (ExitCode enum)

Numeric exit codes 0-127 for shell scripting:
- 0: Success
- 1-9: General errors
- 10-19: Configuration errors
- 20-29: Database errors
- 30-39: Network errors
- 40-49: Crypto errors
- 50-59: Policy errors
- 60-69: Validation errors
- 70-79: Auth errors
- 80-89: Worker/Job errors
- 90-99: Subsystem errors
- 100-119: Domain errors
- 120+: Model Hub errors

## API Error Layer

### ApiError (adapteros-server-api/src/api_error.rs)

Unified API error type implementing `IntoResponse`:

```rust
pub struct ApiError {
    pub status: StatusCode,
    pub code: Cow<'static, str>,  // String error code
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub request_id: Option<String>,
    pub tenant_id: Option<String>,
}
```

**Key features:**
- Builder pattern: `.with_details()`, `.with_request_id()`, `.with_tenant_id()`
- Automatic redaction: `.with_redacted_details()` masks sensitive data
- Static constructors: `ApiError::not_found()`, `ApiError::internal()`, etc.
- Full `From<AosError>` implementation mapping all 70+ variants to HTTP codes

**Redaction patterns** (redact_error_details):
- File paths (`/Users/...`, `C:\Users\...`, `~/...`)
- JWT tokens
- Bearer tokens
- Database connection strings
- API keys
- Secrets

### InferenceError (types/error.rs)

Domain-specific error for inference operations:

```rust
pub enum InferenceError {
    ValidationError(String),
    WorkerNotAvailable(String),
    Timeout(String),
    NoCompatibleWorker { ... },
    CacheBudgetExceeded { ... },
    // ...
}
```

Provides:
- `status_code()` - maps to HTTP status
- `error_code()` - maps to string code
- `failure_code()` - maps to FailureCode enum for observability
- `ecode()` - maps to ECode for compile-time checked codes

### Error Code Enforcement Middleware

`ErrorCodeEnforcementLayer` ensures all JSON error responses have a `code` field:
- Intercepts 4xx/5xx JSON responses
- Injects derived code if missing or empty
- Logs warning when code is missing
- Updates Content-Length header after injection

## Error Conversion Macros

`impl_error_from!` macro in `error_macros.rs`:

```rust
// Basic conversion
impl_error_from!(std::io::Error => Io);

// With error chain preservation
impl_error_from!(std::io::Error => Io, chain);

// With prefix
impl_error_from!(ZipError => Io, prefix = "Zip operation failed");

// With custom transform
impl_error_from!(MyError => Internal, |e| format!("ctx: {}", e));
```

The `chain` variant walks `.source()` and joins with " -> ".

## Best Practices

### Creating Errors

1. Use typed constructors when available:
   ```rust
   AosError::config("Invalid port number")
   AosError::validation(format!("Field {} required", field))
   ```

2. For structured data, use rich variants:
   ```rust
   AosError::AdapterHashMismatch {
       adapter_id: id.to_string(),
       expected: expected_hash,
       actual: actual_hash,
   }
   ```

3. Add context with `.context()`:
   ```rust
   result.context("loading adapter manifest")?
   ```

### API Handlers

1. Return `ApiResult<T>` or `Result<Json<T>, ApiError>`
2. Use `ApiError::not_found()`, `ApiError::bad_request()`, etc.
3. Add request context: `.with_request_context(&ctx)`
4. Use redacted details for sensitive info: `.with_redacted_details(e.to_string())`

### CLI Commands

1. Map errors to `ExitCode` via `From` impl
2. Use `aosctl explain E3001` to get human-readable help
3. Return typed exit codes for scripting

## Error Registry Usage

```rust
// Get error info by typed code
use adapteros_cli::error_codes::{get, ECode};
let info = get(ECode::E3001);
println!("{}", info);  // Formatted with title, cause, fix

// Get from AosError (compile-time exhaustive)
use adapteros_core::errors::{AosError, HasECode};
let error: AosError = ...;
let code = error.ecode();  // Returns ECode enum
let action = error.recovery_action();
```

## Key Files

- `crates/adapteros-core/src/error.rs` - Core AosError enum (flat)
- `crates/adapteros-core/src/errors/mod.rs` - Hierarchical error types
- `crates/adapteros-core/src/error_codes.rs` - String error code constants
- `crates/adapteros-core/src/error_macros.rs` - Conversion macros
- `crates/adapteros-server-api/src/api_error.rs` - API error with IntoResponse
- `crates/adapteros-server-api/src/types/error.rs` - InferenceError
- `crates/adapteros-server-api/src/middleware/error_code_enforcement.rs` - Enforcement layer
- `crates/adapteros-cli/src/error_codes.rs` - ECode enum, ExitCode, ErrorCode registry
