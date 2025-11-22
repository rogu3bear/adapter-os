# AdapterRecord Integration Guide

**Status:** Implementation Ready
**Target:** Gradual integration into `crates/adapteros-db/src/adapters.rs`
**Complexity:** High - 36+ field refactoring
**Timeline:** Phased (v0.2 → v0.3 → v1.0)

---

## Quick Start (For Developers)

### Using the New Builder

Instead of manually constructing complex parameter objects, use the type-safe builder:

```rust
use adapteros_db::{
    AdapterRecordBuilder, AdapterIdentity, AccessControl, LoRAConfig, TierConfig,
    SemanticNaming, SchemaMetadata,
};

// Required fields only
let record = AdapterRecordBuilder::new()
    .identity(AdapterIdentity::new(
        "550e8400-e29b-41d4-a716-446655440000".to_string(),
        "my-adapter-v1".to_string(),
        "My Adapter".to_string(),
        "b3:9d5ed678fe57bcca610140957afab571".to_string(),
    ))
    .access(AccessControl::new("tenant-prod".to_string()))
    .lora(LoRAConfig::new(16, 32.0, r#"["q_proj", "v_proj"]"#.to_string()))
    .tier_config(TierConfig::new(
        "persistent".to_string(),
        "code".to_string(),
        "global".to_string(),
    ))
    .build()?;

// Full example with optional fields
let record = AdapterRecordBuilder::new()
    .identity(AdapterIdentity::new(
        "550e8400-e29b-41d4-a716-446655440000".to_string(),
        "transformers-qa-r042".to_string(),
        "Transformers QA Adapter".to_string(),
        "b3:9d5ed678fe57bcca610140957afab571".to_string(),
    ))
    .access(AccessControl::new("tenant-engineering".to_string()))
    .lora(LoRAConfig::new(32, 64.0, r#"["q_proj", "v_proj", "k_proj"]"#.to_string()))
    .tier_config(TierConfig::new(
        "warm".to_string(),
        "code".to_string(),
        "global".to_string(),
    ))
    .semantic_naming(SemanticNaming {
        adapter_name: Some("tenant-engineering/transformers/qa/r042".to_string()),
        tenant_namespace: Some("tenant-engineering".to_string()),
        domain: Some("transformers".to_string()),
        purpose: Some("qa".to_string()),
        revision: Some("r042".to_string()),
    })
    .expires_at(Some("2025-12-31T23:59:59Z".to_string()))
    .build()?;
```

### Converting to Database Row

The `SchemaCompatible` trait enables zero-copy conversion:

```rust
use adapteros_db::SchemaCompatible;

// Structured → Flat (for insertion)
let flat = record.to_flat()?;
sqlx::query("INSERT INTO adapters (...) VALUES (...)")
    .bind(&flat.tenant_id)
    .bind(&flat.adapter_id)
    // ... all 36+ fields
    .execute(pool)
    .await?;

// Flat → Structured (for validation/manipulation)
let flat_row = sqlx::query_as::<_, FlatAdapterRow>(query)
    .fetch_one(pool)
    .await?;
let record = AdapterRecordV1::from_flat(&flat_row)?;
```

---

## Integration Steps (Phased Approach)

### Phase 1: Parallel Operation (v0.2)

**Goal:** Introduce new structs without breaking existing code

**Step 1.1: Add the Module**
```rust
// In crates/adapteros-db/src/lib.rs
pub mod adapter_record;
pub use adapter_record::{
    AdapterRecordV1, AdapterRecordBuilder, AdapterIdentity, // ... etc
};
```

**Step 1.2: Update Documentation**
- Add docs/ADAPTER_RECORD_REFACTORING.md
- Update docs/ARCHITECTURE_INDEX.md to reference new module
- Add examples to docs/EXAMPLE_PATTERNS.md

**Step 1.3: Write Integration Tests**

Create `tests/adapter_record_integration.rs`:

```rust
#[tokio::test]
async fn test_register_with_new_builder() {
    let db = Db::new_in_memory().await.unwrap();

    let record = AdapterRecordBuilder::new()
        .identity(AdapterIdentity::new(
            "id-1".to_string(),
            "adapter-1".to_string(),
            "Test".to_string(),
            "b3:hash".to_string(),
        ))
        .access(AccessControl::new("tenant-1".to_string()))
        .lora(LoRAConfig::new(16, 32.0, r#"["q_proj"]"#.to_string()))
        .tier_config(TierConfig::new(
            "warm".to_string(),
            "code".to_string(),
            "global".to_string(),
        ))
        .build()
        .unwrap();

    // Convert to flat and insert
    let flat = record.to_flat().unwrap();

    // Insert using existing adapter infrastructure
    // ...verify record round-trips correctly
}

#[test]
fn test_semantic_naming_validation() {
    let valid = SemanticNaming {
        adapter_name: Some("tenant/domain/purpose/r001".to_string()),
        tenant_namespace: Some("tenant".to_string()),
        domain: Some("domain".to_string()),
        purpose: Some("purpose".to_string()),
        revision: Some("r001".to_string()),
    };
    assert!(valid.validate().is_ok());

    // Partial fields = error
    let partial = SemanticNaming {
        adapter_name: Some("name".to_string()),
        ..Default::default()
    };
    assert!(partial.validate().is_err());
}
```

**Step 1.4: Migration Path**

Add optional parameter to existing `register_adapter()`:

```rust
impl Db {
    /// Register adapter (legacy API - keep for compat)
    pub async fn register_adapter(
        &self,
        params: AdapterRegistrationParams,
    ) -> Result<String> {
        // ... existing implementation
    }

    /// Register adapter (new structured API)
    #[allow(dead_code)]  // Newly added, not yet in use
    pub async fn register_adapter_structured(
        &self,
        record: AdapterRecordV1,
    ) -> Result<String> {
        record.validate()?;
        let flat = record.to_flat()?;

        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO adapters (id, tenant_id, adapter_id, name, hash_b3, ...) VALUES (...)"
        )
        .bind(&id)
        .bind(&flat.tenant_id)
        // ... all fields
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }
}
```

**Commits in Phase 1:**
- `refactor: add adapter_record module with 9 sub-structures`
- `docs: add ADAPTER_RECORD_REFACTORING guide`
- `test: add integration tests for new structures`

---

### Phase 2: Internal Adoption (v0.3)

**Goal:** Gradually migrate internal code to use new structures

**Step 2.1: Update Handler Registration**

In `crates/adapteros-server-api/src/handlers/adapters.rs`:

```rust
// Before
pub async fn register_adapter(
    body: AdapterRegistrationParams,
) -> Result<String> {
    // Manual validation...
    db.register_adapter(body).await
}

// After
pub async fn register_adapter(
    body: AdapterRegistrationParams,
) -> Result<String> {
    // Convert to new structure with validation
    let record = AdapterRecordBuilder::new()
        .identity(AdapterIdentity::new(
            Uuid::now_v7().to_string(),
            body.adapter_id.clone(),
            body.name.clone(),
            body.hash_b3.clone(),
        ))
        .access(AccessControl {
            tenant_id: body.tenant_id.clone(),
            acl_json: body.acl_json.clone(),
        })
        .lora(LoRAConfig::new(body.rank, body.alpha, body.targets_json.clone()))
        .tier_config(TierConfig::new(
            body.tier.clone(),
            body.category.clone(),
            body.scope.clone(),
        ))
        // ... optional fields
        .build()?;

    db.register_adapter_structured(record).await
}
```

**Step 2.2: Implement Missing `SchemaCompatible` Helpers**

Add convenience methods to `AdapterRecordV1`:

```rust
impl AdapterRecordV1 {
    /// Create from old AdapterRegistrationParams
    pub fn from_params(params: AdapterRegistrationParams) -> Result<Self> {
        let identity = AdapterIdentity::new(
            Uuid::now_v7().to_string(),
            params.adapter_id,
            params.name,
            params.hash_b3,
        );
        // ... build record from params
    }

    /// Convert to AdapterRegistrationParams (for backward compat)
    pub fn to_params(&self) -> AdapterRegistrationParams {
        AdapterRegistrationParams {
            tenant_id: self.access.tenant_id.clone(),
            adapter_id: self.identity.adapter_id.clone(),
            // ...
        }
    }
}
```

**Step 2.3: Update Query Builders**

Create helper method for bulk queries:

```rust
impl Db {
    /// List adapters with structured records
    pub async fn list_adapters_structured(
        &self,
    ) -> Result<Vec<AdapterRecordV1>> {
        let rows = sqlx::query_as::<_, FlatAdapterRow>(
            "SELECT ... FROM adapters WHERE active = 1"
        )
        .fetch_all(self.pool())
        .await?;

        rows.iter()
            .map(|flat| AdapterRecordV1::from_flat(flat))
            .collect()
    }
}
```

**Step 2.4: Add Telemetry**

Instrument with `tracing` for observability:

```rust
pub async fn register_adapter_structured(
    &self,
    record: AdapterRecordV1,
) -> Result<String> {
    info!(
        adapter_id = %record.identity.adapter_id,
        tier = %record.tier_config.tier,
        rank = record.lora.rank,
        "Registering adapter with new structured API"
    );

    record.validate().map_err(|e| {
        warn!(error = ?e, "Adapter validation failed");
        e
    })?;

    // ... rest of implementation
}
```

**Commits in Phase 2:**
- `refactor: migrate adapter registration to use AdapterRecordV1`
- `refactor: add SchemaCompatible conversion helpers`
- `refactor: implement list_adapters_structured()`
- `observability: add tracing for structured adapter operations`

---

### Phase 3: Public API Migration (v1.0)

**Goal:** Deprecate old API, standardize on new structures

**Step 3.1: Deprecate Old Types**

```rust
#[deprecated(
    since = "0.3.0",
    note = "Use AdapterRecordV1 with AdapterRecordBuilder instead"
)]
pub struct AdapterRegistrationParams { /* ... */ }

#[deprecated(
    since = "0.3.0",
    note = "Use AdapterRecordBuilder::new() instead"
)]
pub struct AdapterRegistrationBuilder { /* ... */ }
```

**Step 3.2: Update REST API**

In `crates/adapteros-server-api/src/routes.rs`:

```rust
/// Register adapter (v2 - structured)
#[post("/v2/adapters/register")]
pub async fn register_adapter_v2(
    State(state): State<ApiState>,
    Json(payload): Json<AdapterRecordPayload>,
) -> Result<Json<AdapterRegistrationResponse>> {
    let record = AdapterRecordV1::from_payload(payload)?;
    let id = state.db.register_adapter_structured(record).await?;
    Ok(Json(AdapterRegistrationResponse { id }))
}
```

**Step 3.3: Update CLAUDE.md**

Add to coding standards:

```markdown
### Adapter Registration

Use the structured builder pattern:

\`\`\`rust
use adapteros_db::AdapterRecordBuilder;

let record = AdapterRecordBuilder::new()
    .identity(AdapterIdentity::new(...))
    .access(AccessControl::new(...))
    .lora(LoRAConfig::new(...))
    .tier_config(TierConfig::new(...))
    .build()?;

db.register_adapter_structured(record).await?;
\`\`\`

**Never** construct flat rows manually. Always use the builder pattern
for type safety and validation.
```

**Step 3.4: Finalize Migration**

```rust
// Remove deprecation warnings at v1.0 by deleting old code entirely
// Keep only new structured APIs
```

**Commits in Phase 3:**
- `deprecation: mark old AdapterRegistration types as deprecated`
- `api: add v2/adapters/register endpoint with structured payloads`
- `docs: update CLAUDE.md with structured adapter patterns`
- `breaking: remove deprecated AdapterRegistrationParams (v1.0)`

---

## Testing Strategy

### Unit Tests (in adapter_record.rs)

Already included in module - cover:
- Individual sub-structure validation
- Builder pattern behavior
- Flat ↔ Structured conversion
- Error cases

Run with:
```bash
cargo test -p adapteros-db adapter_record --lib
```

### Integration Tests (new file)

Create `tests/adapter_record_integration.rs`:

```rust
#[tokio::test]
async fn test_full_adapter_lifecycle_structured() {
    let db = Db::new_in_memory().await.unwrap();

    // 1. Create with builder
    let record = AdapterRecordBuilder::new()
        // ... set all fields
        .build()
        .unwrap();

    // 2. Convert to flat and insert
    let id = {
        let flat = record.to_flat().unwrap();
        // Insert...
    };

    // 3. Retrieve and verify round-trip
    let retrieved = /* fetch from DB */;
    let record2 = AdapterRecordV1::from_flat(&retrieved).unwrap();

    assert_eq!(record.identity.adapter_id, record2.identity.adapter_id);
    assert_eq!(record.lora.rank, record2.lora.rank);
    // ... etc
}

#[tokio::test]
async fn test_validation_error_handling() {
    // Test that invalid inputs are caught
    let invalid = AdapterRecordBuilder::new()
        // Only set required fields, leave lora.rank = 0
        // Should fail on build()
}
```

### Backward Compatibility Tests

```rust
#[tokio::test]
async fn test_old_and_new_apis_interop() {
    let db = Db::new_in_memory().await.unwrap();

    // Register with OLD API
    let params = AdapterRegistrationParams { /* ... */ };
    let id1 = db.register_adapter(params).await.unwrap();

    // Register with NEW API
    let record = AdapterRecordBuilder::new()
        // ... same data
        .build()
        .unwrap();
    let id2 = db.register_adapter_structured(record).await.unwrap();

    // Both should be queryable and equivalent
    let old = db.get_adapter(&id1).await.unwrap().unwrap();
    let new = db.get_adapter(&id2).await.unwrap().unwrap();

    // Compare fields...
    assert_eq!(old.adapter_id, new.adapter_id);
}
```

### Property-Based Tests (Optional)

Using `proptest` for generated test cases:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_structured_to_flat_roundtrips(
        id in ".*",
        adapter_id in ".*",
        name in ".*",
        hash in ".*",
    ) {
        let record = AdapterRecordBuilder::new()
            .identity(AdapterIdentity::new(id, adapter_id, name, hash))
            // ... other required fields
            .build()
            .unwrap();

        let flat = record.to_flat().unwrap();
        let record2 = AdapterRecordV1::from_flat(&flat).unwrap();

        assert_eq!(record.identity.id, record2.identity.id);
        // ... etc
    }
}
```

---

## Field-by-Field Migration Reference

### Core Identity (Migration 0001)
```
Adapter.id                  → AdapterIdentity.id
Adapter.adapter_id          → AdapterIdentity.adapter_id
Adapter.name                → AdapterIdentity.name
Adapter.hash_b3             → AdapterIdentity.hash_b3
```

### Access Control (Migrations 0001, 0012)
```
Adapter.tenant_id           → AccessControl.tenant_id
Adapter.acl_json            → AccessControl.acl_json
```

### LoRA Configuration (Migration 0001)
```
Adapter.rank                → LoRAConfig.rank
Adapter.alpha               → LoRAConfig.alpha
Adapter.targets_json        → LoRAConfig.targets_json
```

### Tier Configuration (Migrations 0001, 0012)
```
Adapter.tier                → TierConfig.tier
Adapter.category            → TierConfig.category
Adapter.scope               → TierConfig.scope
Adapter.active              → TierConfig.active
```

### Lifecycle State (Migrations 0012, 0031, 0068)
```
Adapter.current_state       → LifecycleState.current_state
Adapter.load_state          → LifecycleState.load_state
Adapter.lifecycle_state     → LifecycleState.lifecycle_state
Adapter.memory_bytes        → LifecycleState.memory_bytes
Adapter.activation_count    → LifecycleState.activation_count
Adapter.last_activated      → LifecycleState.last_activated
Adapter.last_loaded_at      → LifecycleState.last_loaded_at
Adapter.pinned              → LifecycleState.pinned
```

### Code Intelligence (Migrations 0005, 0012)
```
Adapter.framework           → CodeIntelligence.framework
Adapter.framework_version   → CodeIntelligence.framework_version
Adapter.repo_id             → CodeIntelligence.repo_id
Adapter.commit_sha          → CodeIntelligence.commit_sha
Adapter.languages_json      → CodeIntelligence.languages_json
Adapter.intent              → CodeIntelligence.intent
```

### Semantic Naming (Migration 0061)
```
Adapter.adapter_name        → SemanticNaming.adapter_name
Adapter.tenant_namespace    → SemanticNaming.tenant_namespace
Adapter.domain              → SemanticNaming.domain
Adapter.purpose             → SemanticNaming.purpose
Adapter.revision            → SemanticNaming.revision
```

### Fork Metadata (Migration 0061)
```
Adapter.parent_id           → ForkMetadata.parent_id
Adapter.fork_type           → ForkMetadata.fork_type
Adapter.fork_reason         → ForkMetadata.fork_reason
```

### Artifact Info (Migration 0045)
```
Adapter.aos_file_path       → ArtifactInfo.aos_file_path
Adapter.aos_file_hash       → ArtifactInfo.aos_file_hash
```

### Schema Metadata (Migration 0068)
```
Adapter.version             → SchemaMetadata.version
Adapter.created_at          → SchemaMetadata.created_at
Adapter.updated_at          → SchemaMetadata.updated_at
```

### Expiration (Migration 0044)
```
Adapter.expires_at          → AdapterRecordV1.expires_at
```

---

## Validation Rules Quick Reference

| Sub-Structure | Rule | Error |
|---------------|------|-------|
| AdapterIdentity | All fields non-empty | `Validation("field cannot be empty")` |
| AccessControl | tenant_id non-empty, acl_json valid JSON | `Validation(...)` |
| LoRAConfig | rank ≥ 1, alpha ≥ 0, targets_json valid JSON array | `Validation(...)` |
| TierConfig | tier in enum, category/scope non-empty, active ∈ {0,1} | `Validation(...)` |
| LifecycleState | All state enums valid, all counts ≥ 0 | `Validation(...)` |
| CodeIntelligence | languages_json valid JSON array if present | `Validation(...)` |
| SemanticNaming | All-or-nothing, revision format `rNNN` | `Validation(...)` |
| ForkMetadata | If fork_type set, parent_id must be set | `Validation(...)` |
| ArtifactInfo | If aos_file_path set, aos_file_hash must be set | `Validation(...)` |

---

## Common Pitfalls & How to Avoid Them

### Pitfall 1: Bypassing Validation
```rust
// DON'T DO THIS
let record = AdapterRecordV1 {
    identity: AdapterIdentity { /* ... */ },
    // ...
};
// record.validate()? is NOT called

// DO THIS
let record = AdapterRecordBuilder::new()
    .identity(identity)
    // ...
    .build()?; // build() calls validate()
```

### Pitfall 2: Partial Semantic Naming
```rust
// DON'T DO THIS
let naming = SemanticNaming {
    adapter_name: Some("name".to_string()),
    tenant_namespace: Some("tenant".to_string()),
    ..Default::default()  // Others are None
};
naming.validate()?;  // FAILS

// DO THIS
let naming = SemanticNaming {
    adapter_name: Some("tenant/domain/purpose/r001".to_string()),
    tenant_namespace: Some("tenant".to_string()),
    domain: Some("domain".to_string()),
    purpose: Some("purpose".to_string()),
    revision: Some("r001".to_string()),
};
naming.validate()?;  // OK
```

### Pitfall 3: Forgetting Fork Invariant
```rust
// DON'T DO THIS
let fork = ForkMetadata {
    parent_id: None,
    fork_type: Some("parameter".to_string()),
    fork_reason: Some("test".to_string()),
};
fork.validate()?;  // FAILS

// DO THIS
let fork = ForkMetadata {
    parent_id: Some("parent-adapter-id".to_string()),
    fork_type: Some("parameter".to_string()),
    fork_reason: Some("test".to_string()),
};
fork.validate()?;  // OK
```

### Pitfall 4: Manually Constructing FlatAdapterRow
```rust
// DON'T DO THIS
let flat = FlatAdapterRow {
    // ... manually set all 36+ fields
};

// DO THIS
let record = AdapterRecordV1 { /* ... */ };
let flat = record.to_flat()?;
```

---

## Performance Considerations

### Memory Overhead
- **Before:** 1 `Adapter` struct = flat layout with all 36 fields
- **After:** 1 `AdapterRecordV1` = composed of 9 sub-structs
- **Impact:** Negligible (<5% overhead for pointers/vtables)

### Conversion Cost
```rust
// from_flat() - O(1) field copies, no allocations beyond String clones
// to_flat() - O(1) field copies, no allocations beyond String clones
```

### Query Performance
- **No change:** Database queries remain identical (flat structure)
- **Optimization:** Lazy validation only when needed

### Caching Strategy
```rust
// Cache flat row from DB, convert on-demand
let flat = sqlx::query_as::<_, FlatAdapterRow>(query).fetch_one(pool).await?;

// Only convert if needed for validation/manipulation
if needs_mutation {
    let mut record = AdapterRecordV1::from_flat(&flat)?;
    // ... modify record
    let new_flat = record.to_flat()?;
}
```

---

## Debugging & Troubleshooting

### Validation Errors
```rust
match record.validate() {
    Ok(()) => println!("Valid record"),
    Err(AosError::Validation(msg)) => eprintln!("Validation error: {}", msg),
    Err(e) => eprintln!("Unexpected error: {:?}", e),
}
```

### Conversion Issues
```rust
// from_flat() failures typically indicate:
// 1. Invalid JSON in optional fields
// 2. Enum values out of range

match AdapterRecordV1::from_flat(&flat_row) {
    Ok(record) => { /* use record */ },
    Err(e) => eprintln!("Conversion failed: {:?}", e),
}
```

### Builder Errors
```rust
// Most common: missing required fields
match AdapterRecordBuilder::new()
    .identity(identity)
    // Forgot to set access!
    .lora(lora)
    .tier_config(tier)
    .build()
{
    Ok(record) => { /* use record */ },
    Err(AosError::Validation(msg)) => eprintln!("Missing field: {}", msg),
    Err(e) => eprintln!("Build failed: {:?}", e),
}
```

---

## Resources

- **Implementation:** `/Users/star/Dev/aos/crates/adapteros-db/src/adapter_record.rs`
- **Tests:** (to be created) `tests/adapter_record_integration.rs`
- **Docs:** `/Users/star/Dev/aos/docs/ADAPTER_RECORD_REFACTORING.md`
- **Related:**
  - `crates/adapteros-db/src/adapters.rs` - Current implementation
  - `crates/adapteros-db/src/lib.rs` - Module exports
  - `docs/DATABASE_REFERENCE.md` - Full schema
  - `CLAUDE.md` - Coding standards

---

## Sign-off Checklist

- [ ] Module compiles without warnings
- [ ] All unit tests pass
- [ ] Integration tests written and passing
- [ ] Backward compatibility verified
- [ ] Documentation reviewed and complete
- [ ] CLAUDE.md updated with new patterns
- [ ] Performance benchmarks (if applicable)
- [ ] Code review approved
- [ ] Merged to main branch
- [ ] Release notes prepared

---

## Support & Questions

For questions or issues with the new AdapterRecord infrastructure:

1. Check the test cases in `adapter_record.rs` for usage examples
2. Review the field mapping reference above
3. Consult `ADAPTER_RECORD_REFACTORING.md` for architecture details
4. File an issue with specific validation or conversion errors

---

**Last Updated:** 2025-11-21
**Maintained By:** [Your Team]
**Status:** Implementation Ready for Phase 1
