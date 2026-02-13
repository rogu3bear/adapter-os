# Service Layer Quick Start

Fast guide to using the service layer in adapterOS.

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
    use crate::config::PathsConfig;
    use crate::state::{ApiConfig, AppState, MetricsConfig};
    use crate::telemetry::MetricsRegistry;
    use crate::test_utils;
    use adapteros_core::{BackendKind, SeedMode};
    use adapteros_db::Db;
    use adapteros_lora_worker::memory::UmaPressureMonitor;
    use adapteros_metrics_exporter::MetricsExporter;
    use adapteros_telemetry::MetricsCollector;
    use std::sync::{Arc, RwLock};

    async fn create_test_state() -> (Arc<AppState>, tempfile::TempDir) {
        let db = Db::new_in_memory()
            .await
            .expect("create in-memory DB for quickstart tests");
        let jwt_secret = b"test-jwt-secret-for-quickstart-32bytes!".to_vec();

        // Use the repo's canonical test temp dir location (`./var/tmp`) and keep the TempDir
        // alive for the duration of the test so AppState paths remain valid.
        let base_tempdir = test_utils::tempdir_with_prefix("aos-test-quickstart-");
        let base_dir = base_tempdir.path().to_path_buf();

        let artifacts_root = base_dir.join("artifacts");
        let bundles_root = base_dir.join("bundles");
        let adapters_root = base_dir.join("adapters");
        let plan_dir = base_dir.join("plan");
        let datasets_root = base_dir.join("datasets");
        let documents_root = base_dir.join("documents");
        for dir in [
            &artifacts_root,
            &bundles_root,
            &adapters_root,
            &plan_dir,
            &datasets_root,
            &documents_root,
        ] {
            std::fs::create_dir_all(dir).expect("create quickstart test dir");
        }

        let config = Arc::new(RwLock::new(ApiConfig {
            metrics: MetricsConfig {
                enabled: false,
                bearer_token: "test".to_string(),
            },
            directory_analysis_timeout_secs: 120,
            use_session_stack_for_routing: false,
            capacity_limits: Default::default(),
            general: None,
            server: Default::default(),
            security: Default::default(),
            auth: Default::default(),
            self_hosting: Default::default(),
            performance: Default::default(),
            streaming: Default::default(),
            paths: PathsConfig {
                artifacts_root: artifacts_root.to_string_lossy().to_string(),
                bundles_root: bundles_root.to_string_lossy().to_string(),
                adapters_root: adapters_root.to_string_lossy().to_string(),
                plan_dir: plan_dir.to_string_lossy().to_string(),
                datasets_root: datasets_root.to_string_lossy().to_string(),
                documents_root: documents_root.to_string_lossy().to_string(),
                synthesis_model_path: None,
            },
            chat_context: Default::default(),
            seed_mode: SeedMode::BestEffort,
            backend_profile: BackendKind::Auto,
            worker_id: 0,
            timeouts: Default::default(),
            rate_limit: None,
            inference_cache: Default::default(),
        }));

        let metrics_exporter = Arc::new(
            MetricsExporter::new(vec![0.1, 1.0]).expect("create metrics exporter for tests"),
        );
        let metrics_collector = Arc::new(MetricsCollector::new(Default::default()));
        let metrics_registry = Arc::new(MetricsRegistry::new());
        let uma_monitor = Arc::new(UmaPressureMonitor::new(15, None));

        let state = Arc::new(AppState::new(
            db,
            jwt_secret,
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            uma_monitor,
        ));

        (state, base_tempdir)
    }

    #[tokio::test]
    async fn test_my_operation() {
        let (state, _tmp) = create_test_state().await;
        let service = DefaultMyService::new(state);

        let result = service.do_something("test-id").await;

        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.id, "test-id");
    }

    #[tokio::test]
    async fn test_not_found_error() {
        let (state, _tmp) = create_test_state().await;
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
- [ ] Follow adapterOS error handling patterns

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
- See [AGENTS.md](../../../../AGENTS.md) for adapterOS standards
- Look at [docs/ARCHITECTURE.md#core-concepts](../../../../docs/ARCHITECTURE.md#core-concepts)
