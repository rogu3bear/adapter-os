# Service Layer Architecture

This directory contains service layer abstractions that extract business logic from HTTP handlers in AdapterOS.

## Purpose

The service layer provides:
- **Separation of concerns**: HTTP handlers focus on request/response handling, services contain business logic
- **Testability**: Services can be mocked for testing handlers
- **Reusability**: Business logic can be shared across multiple handlers or contexts
- **Maintainability**: Domain logic centralized in one place

## Pattern

### 1. Define Service Trait

```rust
#[async_trait]
pub trait AdapterService: Send + Sync {
    async fn promote_lifecycle(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        reason: &str,
        actor: &str,
    ) -> Result<LifecycleTransitionResult>;

    // ... other methods
}
```

### 2. Implement Service

```rust
pub struct DefaultAdapterService {
    state: Arc<AppState>,
}

#[async_trait]
impl AdapterService for DefaultAdapterService {
    async fn promote_lifecycle(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        reason: &str,
        actor: &str,
    ) -> Result<LifecycleTransitionResult> {
        // Business logic here:
        // - Validate adapter exists
        // - Check tenant isolation
        // - Execute state transition
        // - Emit telemetry
        // - Update database
        // - Return result
    }
}
```

### 3. Use in Handlers

```rust
pub async fn promote_adapter_lifecycle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<LifecycleTransitionRequest>,
) -> Result<Json<LifecycleTransitionResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Handler focuses on HTTP concerns:
    // - Permission checks
    // - Request validation
    // - Service call
    // - Response formatting

    require_permission(&claims, Permission::AdapterRegister)?;

    let service = DefaultAdapterService::new(state);
    let result = service.promote_lifecycle(
        &adapter_id,
        &claims.tenant_id,
        &req.reason,
        &claims.sub,
    ).await.map_err(|e| convert_to_http_error(e))?;

    // Audit logging
    audit_log_success(...);

    Ok(Json(LifecycleTransitionResponse {
        adapter_id: result.adapter_id,
        old_state: result.old_state,
        new_state: result.new_state,
        reason: result.reason,
        actor: claims.sub,
        timestamp: result.timestamp,
    }))
}
```

## Service Responsibilities

Services should handle:
- ✅ Core business logic
- ✅ State validation
- ✅ State transitions
- ✅ Database operations
- ✅ Telemetry emission
- ✅ Orchestration of multiple operations
- ✅ Domain-specific error handling

Services should NOT handle:
- ❌ HTTP request parsing
- ❌ HTTP response formatting
- ❌ Authentication/authorization (RBAC checks)
- ❌ Audit logging (handler responsibility)
- ❌ HTTP status code mapping

## Handler Responsibilities

Handlers should handle:
- ✅ HTTP request parsing (extractors)
- ✅ Authentication/authorization checks
- ✅ Request validation (basic structure)
- ✅ Service invocation
- ✅ Error to HTTP status code mapping
- ✅ Response formatting
- ✅ Audit logging
- ✅ HTTP-specific concerns (headers, status codes)

Handlers should NOT handle:
- ❌ Complex business logic
- ❌ Direct database operations (use service)
- ❌ State machine logic
- ❌ Complex orchestration

## Existing Services

### AdapterService (`adapter_service.rs`)

Manages adapter lifecycle operations:
- `promote_lifecycle()` - Promote adapter to next state
- `demote_lifecycle()` - Demote adapter to previous state
- `get_health()` - Get adapter health status
- `get_adapter()` - Fetch adapter by ID

**States**: Unloaded → Cold → Warm → Hot → Resident

**Features**:
- Uses LifecycleManager when available
- Falls back to direct DB updates
- Emits telemetry events
- Validates tenant isolation

### TrainingService (`training_service.rs`)

Manages training job validation and capacity:
- `validate_training_request()` - Validate prerequisites
- `check_training_capacity()` - Check resource availability
- `can_start_training()` - Determine if new jobs can start

**Validations**:
- Dataset validation and tenant isolation
- Collection validation and tenant isolation
- Evidence policy compliance
- Concurrent job limits
- Memory pressure checks

**Note**: This wraps `adapteros_orchestrator::TrainingService` and adds API-level concerns.

## Creating New Services

To add a new service:

1. **Create service file**: `crates/adapteros-server-api/src/services/my_service.rs`

2. **Define trait**:
```rust
#[async_trait]
pub trait MyService: Send + Sync {
    async fn my_operation(&self, arg: &str) -> Result<MyResult>;
}
```

3. **Implement service**:
```rust
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
    async fn my_operation(&self, arg: &str) -> Result<MyResult> {
        // Business logic here
    }
}
```

4. **Add to mod.rs**:
```rust
pub mod my_service;
pub use my_service::{DefaultMyService, MyService};
```

5. **Use in handlers**:
```rust
let service = DefaultMyService::new(state.clone());
let result = service.my_operation(&arg).await?;
```

## Testing Services

Services are easier to test than handlers:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_operation() {
        // Setup test state
        let state = create_test_app_state().await;
        let service = DefaultMyService::new(state);

        // Test operation
        let result = service.my_operation("test").await;

        // Assert results
        assert!(result.is_ok());
    }
}
```

For handlers, you can mock services:

```rust
struct MockAdapterService;

#[async_trait]
impl AdapterService for MockAdapterService {
    async fn promote_lifecycle(...) -> Result<LifecycleTransitionResult> {
        Ok(LifecycleTransitionResult {
            adapter_id: "test".to_string(),
            old_state: "cold".to_string(),
            new_state: "warm".to_string(),
            reason: "test".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
        })
    }
}
```

## Future Services

Candidates for service extraction:

1. **PolicyService** - Policy enforcement, validation, assignment
2. **TenantService** - Tenant management, isolation, validation
3. **InferenceService** - Inference request orchestration, routing
4. **AuditService** - Audit logging, compliance checks
5. **MetricsService** - Metrics collection, aggregation
6. **FederationService** - Federation operations, peer management

## Error Handling

Services use `adapteros_core::error::AosError`:

```rust
pub type Result<T> = std::result::Result<T, AosError>;
```

Common error types:
- `AosError::NotFound` - Entity not found
- `AosError::Validation` - Validation failure
- `AosError::Database` - Database operation failure
- `AosError::Other` - Generic error

Handlers convert `AosError` to HTTP status codes:
- `NotFound` → 404
- `Validation` → 400
- `Database` → 500

## AdapterOS Standards

All services must follow AdapterOS standards:

- **Error Handling**: Use `Result<T, AosError>`, never `Option<T>` for errors
- **Logging**: Use `tracing` macros (`info!`, `warn!`, `error!`)
- **Determinism**: Use `spawn_deterministic` for deterministic operations
- **Telemetry**: Emit canonical JSON events (Policy Pack #9)
- **Audit**: Return results to handlers for audit logging

## References

- [CLAUDE.md](../../../../CLAUDE.md) - Architecture patterns
- [docs/ARCHITECTURE.md#architecture-components](../../../../docs/ARCHITECTURE.md#architecture-components) - Design patterns
- [docs/LIFECYCLE.md](../../../../docs/LIFECYCLE.md) - Adapter lifecycle states
- [docs/TRAINING_PIPELINE.md](../../../../docs/TRAINING_PIPELINE.md) - Training flow
