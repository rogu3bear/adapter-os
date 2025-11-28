# Service Layer Quick Start

Fast guide to using the service layer in AdapterOS.

## TL;DR

**Handlers**: Auth, parse, format, respond
**Services**: Validate, orchestrate, execute, return data

## Using Existing Services

### AdapterService

```rust
use crate::services::{AdapterService, DefaultAdapterService};

pub async fn my_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Response>, (StatusCode, Json<ErrorResponse>)> {
    // Create service
    let service = DefaultAdapterService::new(state.clone());

    // Call service method
    let result = service.promote_lifecycle(
        "adapter-id",
        &claims.tenant_id,
        "reason for promotion",
        &claims.sub,
    ).await.map_err(convert_error)?;

    Ok(Json(Response::from(result)))
}
```

### TrainingService

```rust
use crate::services::{TrainingService, DefaultTrainingService};

pub async fn my_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Response>, (StatusCode, Json<ErrorResponse>)> {
    let service = DefaultTrainingService::new(state.clone());

    // Check if training can start
    service.can_start_training().await.map_err(convert_error)?;

    // Validate request
    let validation = service.validate_training_request(
        &claims.tenant_id,
        Some("dataset-id"),
        None,
        true, // check evidence policy
    ).await.map_err(convert_error)?;

    if !validation.is_valid {
        return Err(validation_error(validation));
    }

    // Start training via orchestrator
    let job = state.training_service.start_training(...).await?;

    Ok(Json(Response::from(job)))
}
```

## Creating a New Service

### Step 1: Define the trait

```rust
// crates/adapteros-server-api/src/services/my_service.rs

use async_trait::async_trait;
use adapteros_core::error::AosError;

pub type Result<T> = std::result::Result<T, AosError>;

#[async_trait]
pub trait MyService: Send + Sync {
    async fn do_something(&self, id: &str) -> Result<MyResult>;
}

#[derive(Debug, Clone)]
pub struct MyResult {
    pub id: String,
    pub status: String,
}
```

### Step 2: Implement the service

```rust
use crate::state::AppState;
use std::sync::Arc;
use tracing::{error, info};

pub struct DefaultMyService {
    state: Arc<AppState>,
}

impl DefaultMyService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl MyService for DefaultMyService {
    async fn do_something(&self, id: &str) -> Result<MyResult> {
        // 1. Fetch data
        let data = self.state.db.get_something(id).await
            .map_err(|e| {
                error!(id = %id, error = %e, "Failed to fetch");
                AosError::Database(format!("Failed to fetch: {}", e))
            })?
            .ok_or_else(|| AosError::NotFound(format!("Not found: {}", id)))?;

        // 2. Validate
        if !self.validate_something(&data) {
            return Err(AosError::Validation("Invalid data".to_string()));
        }

        // 3. Execute business logic
        let result = self.execute_logic(&data).await?;

        // 4. Emit telemetry
        info!(id = %id, status = %result.status, "Operation completed");

        Ok(result)
    }
}
```

### Step 3: Export from mod.rs

```rust
// crates/adapteros-server-api/src/services/mod.rs

pub mod my_service;
pub use my_service::{DefaultMyService, MyResult, MyService};
```

### Step 4: Use in handler

```rust
pub async fn my_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MyResult>, (StatusCode, Json<ErrorResponse>)> {
    let service = DefaultMyService::new(state.clone());
    let result = service.do_something(&id).await
        .map_err(convert_aos_error_to_http)?;
    Ok(Json(result))
}
```

## Error Conversion

```rust
fn convert_aos_error_to_http(e: AosError) -> (StatusCode, Json<ErrorResponse>) {
    match e {
        AosError::NotFound(msg) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(&msg).with_code("NOT_FOUND")),
        ),
        AosError::Validation(msg) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(&msg).with_code("VALIDATION_ERROR")),
        ),
        AosError::Database(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&msg).with_code("DATABASE_ERROR")),
        ),
        AosError::Other(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&msg).with_code("INTERNAL_ERROR")),
        ),
    }
}
```

## Common Patterns

### Pattern 1: Validate then Execute

```rust
async fn my_operation(&self, id: &str) -> Result<MyResult> {
    // Validate
    let entity = self.get_and_validate(id).await?;

    // Execute
    let result = self.execute_operation(&entity).await?;

    // Emit telemetry
    self.emit_event(&result);

    Ok(result)
}
```

### Pattern 2: Check Capacity then Execute

```rust
async fn my_operation(&self) -> Result<MyResult> {
    // Check capacity
    let capacity = self.check_capacity().await?;
    if !capacity.can_proceed {
        return Err(AosError::Validation("Capacity exceeded".to_string()));
    }

    // Execute
    let result = self.execute_operation().await?;

    Ok(result)
}
```

### Pattern 3: Orchestrate Multiple Operations

```rust
async fn my_operation(&self, id: &str) -> Result<MyResult> {
    // Step 1: Prepare
    let data = self.prepare(id).await?;

    // Step 2: Validate
    self.validate(&data).await?;

    // Step 3: Execute
    let result = self.execute(&data).await?;

    // Step 4: Finalize
    self.finalize(&result).await?;

    Ok(result)
}
```

### Pattern 4: Fallback Logic

```rust
async fn my_operation(&self, id: &str) -> Result<MyResult> {
    // Try primary method
    if let Some(manager) = &self.state.some_manager {
        match manager.do_something(id).await {
            Ok(result) => return Ok(result),
            Err(e) => warn!(error = %e, "Primary method failed, trying fallback"),
        }
    }

    // Fallback to direct method
    self.direct_method(id).await
}
```

## Testing Services

### Unit Test

```rust
#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_state() -> Arc<AppState> {
        // Create test database, config, etc.
        todo!()
    }

    #[tokio::test]
    async fn test_my_operation() {
        let state = create_test_state().await;
        let service = DefaultMyService::new(state);

        let result = service.do_something("test-id").await;

        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.id, "test-id");
    }

    #[tokio::test]
    async fn test_not_found_error() {
        let state = create_test_state().await;
        let service = DefaultMyService::new(state);

        let result = service.do_something("nonexistent").await;

        assert!(matches!(result, Err(AosError::NotFound(_))));
    }
}
```

### Integration Test with Mock

```rust
struct MockMyService;

#[async_trait]
impl MyService for MockMyService {
    async fn do_something(&self, id: &str) -> Result<MyResult> {
        Ok(MyResult {
            id: id.to_string(),
            status: "mocked".to_string(),
        })
    }
}

#[tokio::test]
async fn test_handler_with_mock() {
    let service = Arc::new(MockMyService);
    // Use service in handler test
}
```

## Checklist for New Services

- [ ] Define trait with `#[async_trait]`
- [ ] Use `Result<T> = std::result::Result<T, AosError>`
- [ ] Implement `DefaultXxxService` with `Arc<AppState>`
- [ ] Include `new(state: Arc<AppState>) -> Self` constructor
- [ ] Use `tracing` for logging (`info!`, `warn!`, `error!`)
- [ ] Emit telemetry events for important operations
- [ ] Return domain-specific result types (not HTTP types)
- [ ] Add unit tests for business logic
- [ ] Export from `mod.rs`
- [ ] Document public methods with `///` comments
- [ ] Follow AdapterOS error handling patterns

## Common Mistakes

❌ **Don't do this:**
```rust
// Service returning HTTP types
async fn my_operation(&self) -> Result<Json<MyResponse>, (StatusCode, Json<ErrorResponse>)>

// Service handling HTTP concerns
async fn my_operation(&self, claims: &Claims) {
    if !claims.has_permission() { ... }  // RBAC in service
}

// Service doing audit logging
async fn my_operation(&self) {
    audit_log(&self.db, "action", "resource");  // Handler's job
}
```

✅ **Do this instead:**
```rust
// Service returning domain types
async fn my_operation(&self) -> Result<MyResult>

// Validate business rules
async fn my_operation(&self, tenant_id: &str) {
    if entity.tenant_id != tenant_id {
        return Err(AosError::Validation("Tenant mismatch".to_string()));
    }
}

// Return result, let handler audit
async fn my_operation(&self) -> Result<MyResult> {
    let result = self.execute().await?;
    Ok(result)  // Handler will audit
}
```

## Quick Reference Card

| Layer | Responsibilities |
|-------|-----------------|
| **Handler** | Auth, Parse, Format, Audit, HTTP |
| **Service** | Validate, Execute, Orchestrate, Telemetry |
| **Infrastructure** | DB, External APIs, File System |

| Error Type | HTTP Status |
|------------|-------------|
| `NotFound` | 404 |
| `Validation` | 400 |
| `Database` | 500 |
| `Other` | 500 |

| Service Method | Should Return |
|----------------|---------------|
| Domain operation | `Result<DomainType>` |
| Validation check | `Result<ValidationResult>` |
| Health check | `Result<HealthInfo>` |

## Next Steps

1. Read [README.md](README.md) for detailed patterns
2. See [ARCHITECTURE.md](ARCHITECTURE.md) for flow diagrams
3. Check existing services for examples
4. Start extracting handler business logic

## Getting Help

- Check existing services: `adapter_service.rs`, `training_service.rs`
- Read handler code: `handlers/adapters.rs`, `handlers/training.rs`
- See [CLAUDE.md](../../../../CLAUDE.md) for AdapterOS standards
- Look at [docs/ARCHITECTURE_PATTERNS.md](../../../../docs/ARCHITECTURE_PATTERNS.md)
