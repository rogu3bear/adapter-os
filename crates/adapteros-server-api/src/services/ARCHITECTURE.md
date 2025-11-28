# Service Layer Architecture

## Overview

The service layer sits between HTTP handlers and the data/infrastructure layers, providing a clean separation of concerns and encapsulating business logic.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         HTTP Layer                               │
│                     (Handlers/Routes)                            │
│  - Authentication/Authorization (RBAC)                           │
│  - Request parsing and validation                                │
│  - Response formatting                                           │
│  - Audit logging                                                 │
│  - HTTP status code mapping                                      │
└────────────┬────────────────────────────────────┬────────────────┘
             │                                    │
             │ Calls                         Calls│
             ▼                                    ▼
┌─────────────────────────┐         ┌──────────────────────────────┐
│   AdapterService        │         │    TrainingService           │
│  ┌──────────────────┐   │         │   ┌──────────────────────┐   │
│  │ Business Logic:  │   │         │   │ Business Logic:      │   │
│  │ - State machine  │   │         │   │ - Validation         │   │
│  │ - Transitions    │   │         │   │ - Capacity checks    │   │
│  │ - Health checks  │   │         │   │ - Policy enforcement │   │
│  │ - Tenant checks  │   │         │   │ - Resource checks    │   │
│  └──────────────────┘   │         │   └──────────────────────┘   │
└────────┬────────────────┘         └────────┬─────────────────────┘
         │                                    │
         │ Uses                          Uses│
         ▼                                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Infrastructure Layer                          │
│                                                                  │
│  ┌──────────────┐  ┌─────────────────┐  ┌──────────────────┐   │
│  │   Database   │  │ LifecycleManager│  │   Orchestrator   │   │
│  │   (Db trait) │  │                 │  │ TrainingService  │   │
│  └──────────────┘  └─────────────────┘  └──────────────────┘   │
│                                                                  │
│  ┌──────────────┐  ┌─────────────────┐  ┌──────────────────┐   │
│  │ PolicyManager│  │  UmaMonitor     │  │   Telemetry      │   │
│  └──────────────┘  └─────────────────┘  └──────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Request Flow

### Example: Promote Adapter Lifecycle

```
1. HTTP Request
   POST /v1/adapters/{id}/lifecycle/promote
   Authorization: Bearer <jwt>
   Body: { "reason": "Production deployment" }
   │
   ▼
2. Handler (adapters.rs::promote_adapter_lifecycle)
   ├─ Extract claims from JWT
   ├─ Check RBAC permissions (Operator/Admin)
   ├─ Parse request body
   │
   ▼
3. Service Layer (DefaultAdapterService::promote_lifecycle)
   ├─ Fetch adapter from database
   ├─ Validate tenant isolation
   ├─ Determine next state (state machine logic)
   ├─ Execute transition via LifecycleManager
   │  ├─ Try lifecycle manager promotion
   │  └─ Fallback to direct DB update if needed
   ├─ Emit telemetry event
   └─ Return LifecycleTransitionResult
   │
   ▼
4. Handler (continued)
   ├─ Log audit event (success/failure)
   ├─ Convert result to HTTP response
   └─ Return JSON response
   │
   ▼
5. HTTP Response
   200 OK
   Body: {
     "adapter_id": "...",
     "old_state": "cold",
     "new_state": "warm",
     "reason": "Production deployment",
     "actor": "user@example.com",
     "timestamp": "2025-11-28T..."
   }
```

## Layer Responsibilities

### HTTP Layer (Handlers)

**Should Handle:**
- ✅ HTTP request parsing (extractors: Path, Query, Json, State, Extension)
- ✅ Authentication (JWT validation, session management)
- ✅ Authorization (RBAC permission checks)
- ✅ Input validation (basic structure, required fields)
- ✅ Service invocation (calling service methods)
- ✅ Error to HTTP status code mapping
- ✅ Response formatting (JSON serialization)
- ✅ Audit logging (success/failure events)
- ✅ HTTP-specific concerns (headers, cookies, status codes)

**Should NOT Handle:**
- ❌ Complex business logic
- ❌ State machine logic
- ❌ Direct database operations (except via services)
- ❌ Complex orchestration
- ❌ Policy enforcement logic
- ❌ Resource management

**Example:**
```rust
pub async fn promote_adapter_lifecycle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<LifecycleTransitionRequest>,
) -> Result<Json<LifecycleTransitionResponse>, (StatusCode, Json<ErrorResponse>)> {
    // RBAC check
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Service invocation
    let service = DefaultAdapterService::new(state.clone());
    let result = service.promote_lifecycle(
        &adapter_id,
        &claims.tenant_id,
        &req.reason,
        &claims.sub,
    ).await.map_err(convert_aos_error_to_http)?;

    // Audit logging
    audit_log_success(&state.db, &claims, "adapter_promote", &adapter_id).await;

    // Response formatting
    Ok(Json(LifecycleTransitionResponse::from(result)))
}
```

### Service Layer

**Should Handle:**
- ✅ Core business logic
- ✅ State validation and transitions
- ✅ Domain rule enforcement
- ✅ Orchestration of multiple operations
- ✅ Database operations (via Db trait)
- ✅ Telemetry emission
- ✅ Tenant isolation validation
- ✅ Policy compliance checking
- ✅ Resource availability checks
- ✅ Complex validation logic

**Should NOT Handle:**
- ❌ HTTP concerns (status codes, headers)
- ❌ Request parsing
- ❌ Response serialization
- ❌ RBAC authorization (handled by handlers)
- ❌ Audit logging (handlers log the result)

**Example:**
```rust
impl AdapterService for DefaultAdapterService {
    async fn promote_lifecycle(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        reason: &str,
        actor: &str,
    ) -> Result<LifecycleTransitionResult> {
        // Business logic
        let adapter = self.state.db.get_adapter(adapter_id).await?
            .ok_or(AosError::NotFound(...))?;

        // Tenant isolation
        if adapter.tenant_id != tenant_id {
            return Err(AosError::Validation(...));
        }

        // State machine
        let new_state = Self::next_state(&adapter.current_state)?;

        // Orchestration
        self.execute_transition(adapter_id, new_state, reason, ...).await?;

        // Telemetry
        info!(event = %telemetry_event, ...);

        Ok(LifecycleTransitionResult { ... })
    }
}
```

### Infrastructure Layer

**Should Handle:**
- ✅ Database queries and transactions
- ✅ External service communication
- ✅ File system operations
- ✅ Memory management
- ✅ Resource allocation
- ✅ Low-level operations

**Should NOT Handle:**
- ❌ Business logic
- ❌ HTTP concerns
- ❌ Complex orchestration

## Service Types

### 1. Domain Services
Encapsulate business logic for a specific domain.

Examples:
- `AdapterService` - Adapter lifecycle management
- `TrainingService` - Training job management
- `PolicyService` - Policy enforcement
- `TenantService` - Tenant management

### 2. Orchestration Services
Coordinate multiple domain services or infrastructure components.

Examples:
- `InferenceService` - Orchestrates adapter loading, routing, inference
- `FederationService` - Coordinates peer communication, consensus

### 3. Validation Services
Perform complex validation that spans multiple domains.

Examples:
- `TrainingService::validate_training_request()` - Validates datasets, collections, policies

## Benefits

### 1. Separation of Concerns
```
Handler:  HTTP ← → JSON
Service:  Logic ← → Data
Database: Data ← → Storage
```

Each layer has clear responsibilities.

### 2. Testability

**Unit Tests (Fast)**
```rust
// Test service logic without HTTP
#[tokio::test]
async fn test_service() {
    let service = DefaultAdapterService::new(test_state());
    let result = service.promote_lifecycle(...).await;
    assert!(result.is_ok());
}
```

**Integration Tests (Slower)**
```rust
// Test handler with real/mock service
#[tokio::test]
async fn test_handler() {
    let app = create_test_app();
    let response = app.post("/adapters/test/promote").send().await;
    assert_eq!(response.status(), 200);
}
```

### 3. Reusability

Service methods can be called from:
- HTTP handlers
- Background jobs
- CLI commands
- WebSocket handlers
- SSE streams
- Tests

### 4. Maintainability

Changes are isolated:
- HTTP format change → Update handlers only
- Business logic change → Update services only
- Database schema change → Update Db trait only

### 5. Mocking

```rust
struct MockAdapterService;

#[async_trait]
impl AdapterService for MockAdapterService {
    async fn promote_lifecycle(...) -> Result<...> {
        // Return test data
        Ok(LifecycleTransitionResult { ... })
    }
}

// Use in tests
let handler = create_handler(mock_service);
```

## Error Handling Flow

```
Service Layer               Handler Layer
     │                           │
     ├─ Business error           │
     │  (AosError::Validation)   │
     │                           │
     └──────────────────────────►│
                                 ├─ Map to HTTP
                                 │  (400 Bad Request)
                                 │
                                 ├─ Create ErrorResponse
                                 │  with code & message
                                 │
                                 └─ Return JSON
```

Error mapping:
- `AosError::NotFound` → 404 Not Found
- `AosError::Validation` → 400 Bad Request
- `AosError::Database` → 500 Internal Server Error
- `AosError::Other` → 500 Internal Server Error

## Telemetry Flow

```
Service Layer                   Infrastructure
     │                               │
     ├─ Emit telemetry event         │
     │  (structured JSON)            │
     │                               │
     └──────────────────────────────►│
                                     ├─ TelemetryBuffer
                                     ├─ MetricsCollector
                                     └─ BundleStore
```

Services emit events, infrastructure handles collection/storage.

## Standards Compliance

All services follow AdapterOS standards:

| Standard | Requirement | Implementation |
|----------|-------------|----------------|
| Error Handling | `Result<T, AosError>` | ✅ All service methods return `Result` |
| Logging | `tracing` macros | ✅ Uses `info!`, `warn!`, `error!` |
| Telemetry | Canonical JSON | ✅ Emits structured events |
| Determinism | HKDF-seeded | ⚠️ Ready for `spawn_deterministic` |
| Database | Db trait methods | ✅ Prefers trait over direct SQL |
| Testing | Unit + integration | ✅ Tests included |

## Future Directions

### 1. Service Registry
```rust
pub struct ServiceRegistry {
    adapter: Arc<dyn AdapterService>,
    training: Arc<dyn TrainingService>,
    policy: Arc<dyn PolicyService>,
    // ...
}

impl ServiceRegistry {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            adapter: Arc::new(DefaultAdapterService::new(state.clone())),
            training: Arc::new(DefaultTrainingService::new(state.clone())),
            // ...
        }
    }
}
```

### 2. Service Composition
```rust
impl InferenceService {
    async fn execute_inference(&self, request: InferenceRequest) -> Result<Response> {
        // Compose multiple services
        let adapter = self.adapter_service.get_adapter(&request.adapter_id).await?;
        let policy_ok = self.policy_service.check_inference_allowed(&adapter).await?;
        let result = self.inference_engine.infer(request).await?;
        Ok(result)
    }
}
```

### 3. Service Middleware
```rust
struct CachingService<S: AdapterService> {
    inner: S,
    cache: Arc<Cache>,
}

#[async_trait]
impl<S: AdapterService> AdapterService for CachingService<S> {
    async fn get_adapter(&self, id: &str) -> Result<Option<Adapter>> {
        if let Some(cached) = self.cache.get(id) {
            return Ok(Some(cached));
        }
        let adapter = self.inner.get_adapter(id).await?;
        if let Some(ref a) = adapter {
            self.cache.set(id, a.clone());
        }
        Ok(adapter)
    }
}
```

## References

- [README.md](README.md) - Service layer guide
- [CLAUDE.md](../../../../CLAUDE.md) - Architecture patterns
- [docs/ARCHITECTURE_PATTERNS.md](../../../../docs/ARCHITECTURE_PATTERNS.md) - Design patterns
- [docs/ERROR_HANDLING_PATTERNS.md](../../../../ERROR_HANDLING_PATTERNS.md) - Error handling
