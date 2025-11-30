# AdapterRecord Refactoring: Schema Drift Prevention

**Status:** Reference Implementation
**Location:** `crates/adapteros-db/src/adapter_record.rs`
**Purpose:** Prevent schema drift by organizing 36+ Adapter fields into logical, versioned sub-structures.
**Last Updated:** 2025-11-21

---

## Overview

The current `Adapter` struct in `crates/adapteros-db/src/adapters.rs` contains 36+ fields spread across multiple migrations (0001-0080). This flat structure creates several problems:

1. **Schema Drift:** Adding fields is easy; coordinating across migrations is hard
2. **Unclear Grouping:** Related fields scattered across the struct
3. **Migration Burden:** Each new migration bloats the `register_adapter()` SQL query
4. **Type Safety:** No validation of field relationships (e.g., if `fork_type` is set, `parent_id` must be set)
5. **Future Extensibility:** Hard to version schema without breaking existing code

## Solution: Structured Composition with Versioning

The refactoring introduces **9 specialized sub-structures** organized by concern, plus a comprehensive builder pattern and schema versioning infrastructure.

### Sub-Structures (Logical Grouping)

#### 1. AdapterIdentity
**Immutable fields that uniquely identify an adapter.**

```rust
pub struct AdapterIdentity {
    pub id: String,              // UUIDv7 from migration 0001
    pub adapter_id: String,      // External ID for lookups
    pub name: String,            // Human-readable name
    pub hash_b3: String,         // BLAKE3 content hash
}
```

**Origin:** Migration 0001 (core fields)
**Invariants:** None can be empty; immutable after creation
**Validation:** `validate()` ensures no field is empty

---

#### 2. AccessControl
**Multi-tenancy and access control boundaries.**

```rust
pub struct AccessControl {
    pub tenant_id: String,       // Isolation boundary (from migration 0001)
    pub acl_json: Option<String>,// Access control list (from migration 0012)
}
```

**Origin:** Migrations 0001, 0012
**Invariants:** `tenant_id` cannot be empty; `acl_json` must be valid JSON if present
**Validation:** `validate()` checks both constraints

---

#### 3. LoRAConfig
**LoRA model configuration and training parameters.**

```rust
pub struct LoRAConfig {
    pub rank: i32,               // LoRA rank (from migration 0001)
    pub alpha: f64,              // LoRA alpha (usually rank * 2)
    pub targets_json: String,    // Target modules (from migration 0001)
}
```

**Origin:** Migration 0001
**Invariants:** `rank >= 1`, `alpha >= 0.0`, `targets_json` is valid JSON array
**Validation:** `validate()` enforces all constraints

---

#### 4. TierConfig
**Deployment configuration and memory prioritization.**

```rust
pub struct TierConfig {
    pub tier: String,            // "persistent"|"warm"|"ephemeral" (from migration 0001)
    pub category: String,        // e.g., "code" (from migration 0012)
    pub scope: String,           // e.g., "global" (from migration 0012)
    pub active: i32,             // Soft-delete flag
}
```

**Origin:** Migrations 0001, 0012
**Invariants:** `tier` in ["persistent", "warm", "ephemeral"], both `category` and `scope` non-empty
**Validation:** `validate()` enforces enum values and non-empty constraints

---

#### 5. LifecycleState
**Runtime lifecycle tracking and state management.**

```rust
pub struct LifecycleState {
    pub current_state: String,          // "unloaded"|"cold"|"warm"|"hot"|"resident"
    pub load_state: String,             // "cold"|"warm"|"hot"
    pub lifecycle_state: String,        // "draft"|"active"|"deprecated"|"retired"
    pub memory_bytes: i64,              // Current memory usage
    pub activation_count: i64,          // Total activations
    pub last_activated: Option<String>, // Timestamp
    pub last_loaded_at: Option<String>,// Timestamp
    pub pinned: i32,                    // Boolean (0/1)
}
```

**Origin:** Migrations 0012, 0031, 0068
**Invariants:** All numeric fields >= 0, state enums valid
**Validation:** `validate()` checks all constraints; `default_unloaded()` factory method

---

#### 6. CodeIntelligence
**Framework metadata and code origin tracking.**

```rust
pub struct CodeIntelligence {
    pub framework: Option<String>,        // e.g., "transformers"
    pub framework_version: Option<String>,
    pub repo_id: Option<String>,          // Source repo
    pub commit_sha: Option<String>,       // Source commit
    pub languages_json: Option<String>,   // Supported languages (JSON)
    pub intent: Option<String>,           // Purpose description
}
```

**Origin:** Migrations 0005, 0012
**Invariants:** `languages_json` must be valid JSON array if present
**Validation:** `validate()` checks JSON format

---

#### 7. SemanticNaming
**Semantic naming taxonomy for discovery and organization.**

```rust
pub struct SemanticNaming {
    pub adapter_name: Option<String>,      // Full name: {tenant}/{domain}/{purpose}/{rev}
    pub tenant_namespace: Option<String>,  // e.g., "acme-corp"
    pub domain: Option<String>,            // e.g., "engineering"
    pub purpose: Option<String>,           // e.g., "code-review"
    pub revision: Option<String>,          // e.g., "r001"
}
```

**Origin:** Migration 0061
**Invariants:** All-or-nothing constraint: either all fields present or all absent
**Validation:** `validate()` enforces all-or-nothing + revision format `rNNN`

---

#### 8. ForkMetadata
**Adapter lineage and fork tracking.**

```rust
pub struct ForkMetadata {
    pub parent_id: Option<String>,        // Parent adapter ID
    pub fork_type: Option<String>,        // "parameter"|"data"|"architecture"
    pub fork_reason: Option<String>,      // Why this fork was created
}
```

**Origin:** Migration 0061
**Invariants:** If `fork_type` is set, `parent_id` must also be set
**Validation:** `validate()` enforces relationship constraint

---

#### 9. ArtifactInfo
**File and artifact management (.aos archive support).**

```rust
pub struct ArtifactInfo {
    pub aos_file_path: Option<String>,   // Path to .aos file
    pub aos_file_hash: Option<String>,   // BLAKE3 hash of .aos file
}
```

**Origin:** Migration 0045
**Invariants:** If `aos_file_path` is set, `aos_file_hash` must also be set
**Validation:** `validate()` enforces relationship constraint

---

### Comprehensive Adapter Record (V1)

```rust
pub struct AdapterRecordV1 {
    pub identity: AdapterIdentity,
    pub access: AccessControl,
    pub lora: LoRAConfig,
    pub tier_config: TierConfig,
    pub lifecycle: LifecycleState,
    pub code_info: CodeIntelligence,
    pub semantic_naming: SemanticNaming,
    pub fork_metadata: ForkMetadata,
    pub artifacts: ArtifactInfo,
    pub schema: SchemaMetadata,
    pub expires_at: Option<String>,  // TTL support from migration 0044
}
```

**Total Fields:** 36 (organized into 9 logical groups)
**Validation:** Recursive across all sub-structures
**Schema Version:** "1.0.0" (supports future versioning)

---

## Builder Pattern for Type-Safe Construction

The `AdapterRecordBuilder` enforces required fields at compile time:

```rust
let record = AdapterRecordBuilder::new()
    .identity(AdapterIdentity::new(
        "id-uuid".to_string(),
        "adapter-1".to_string(),
        "My Adapter".to_string(),
        "b3:abc123".to_string(),
    ))
    .access(AccessControl::new("tenant-1".to_string()))
    .lora(LoRAConfig::new(16, 32.0, r#"["q_proj", "v_proj"]"#.to_string()))
    .tier_config(TierConfig::new(
        "warm".to_string(),
        "code".to_string(),
        "global".to_string(),
    ))
    // Optional fields
    .semantic_naming(SemanticNaming {
        adapter_name: Some("tenant/engineering/code-review/r001".to_string()),
        tenant_namespace: Some("tenant".to_string()),
        domain: Some("engineering".to_string()),
        purpose: Some("code-review".to_string()),
        revision: Some("r001".to_string()),
    })
    .build()?; // Validates all constraints
```

---

## Backward Compatibility: Flat to Structured Conversion

The `SchemaCompatible` trait enables zero-copy serialization between database rows and structured records:

```rust
pub trait SchemaCompatible: Sized {
    fn from_flat(flat: &FlatAdapterRow) -> Result<Self>;
    fn to_flat(&self) -> Result<FlatAdapterRow>;
}

impl SchemaCompatible for AdapterRecordV1 {
    // Implementation converts between flat and structured representations
}
```

### Migration Path (Step-by-Step)

**Step 1: Dual Representation (No Breaking Changes)**

```rust
// In adapters.rs, keep the old flat Adapter struct alongside new logic
pub struct Adapter { /* existing 36 fields */ }

// New code uses AdapterRecordV1 internally
// Old code continues using Adapter
// Bridge queries convert as needed:
let flat = sqlx::query_as::<_, FlatAdapterRow>(query).fetch_one(pool).await?;
let record = AdapterRecordV1::from_flat(&flat)?;
```

**Step 2: Gradual API Migration**

```rust
// Old API (keep for backward compat)
pub async fn register_adapter(&self, params: AdapterRegistrationParams) -> Result<String>

// New API (use structured builder)
pub async fn register_adapter_v2(&self, record: AdapterRecordV1) -> Result<String> {
    let flat = record.to_flat()?;
    // Insert using flat representation
}
```

**Step 3: Future Deprecation (Post v1.0)**

Once v2.0 is stable, deprecate old APIs and migrate all code to structured approach.

---

## Schema Versioning Strategy

The `SchemaMetadata` sub-structure enables future schema evolution:

```rust
pub struct SchemaMetadata {
    pub version: String,      // e.g., "1.0.0", "1.1.0", "2.0.0"
    pub created_at: String,   // ISO 8601
    pub updated_at: String,   // ISO 8601
}
```

### Version Bumping Rules

- **Patch (1.0.1):** Bug fixes, no field additions
- **Minor (1.1.0):** New optional fields in existing sub-structures, additive only
- **Major (2.0.0):** Breaking changes (field removals, renames, type changes)

### Migration Helper for Future Versions

When adding new fields (e.g., for migration 0082):

```rust
// Example: Adding new sub-structure in future
pub struct AdapterRecordV2 {
    pub identity: AdapterIdentity,
    // ... existing fields ...
    pub new_feature: Option<NewFeatureStruct>,  // Minor version bump
}

// Implement SchemaCompatible for V2
impl SchemaCompatible for AdapterRecordV2 {
    fn from_flat(flat: &FlatAdapterRow) -> Result<Self> {
        // Convert V1, add defaults for new fields
    }
}
```

---

## Field Mapping from Current Schema

| Sub-Structure | Fields | Migrations |
|---------------|--------|-----------|
| **AdapterIdentity** | id, adapter_id, name, hash_b3 | 0001 |
| **AccessControl** | tenant_id, acl_json | 0001, 0012 |
| **LoRAConfig** | rank, alpha, targets_json | 0001 |
| **TierConfig** | tier, category, scope, active | 0001, 0012 |
| **LifecycleState** | current_state, load_state, lifecycle_state, memory_bytes, activation_count, last_activated, last_loaded_at, pinned | 0012, 0031, 0068 |
| **CodeIntelligence** | framework, framework_version, repo_id, commit_sha, languages_json, intent | 0005, 0012 |
| **SemanticNaming** | adapter_name, tenant_namespace, domain, purpose, revision | 0061 |
| **ForkMetadata** | parent_id, fork_type, fork_reason | 0061 |
| **ArtifactInfo** | aos_file_path, aos_file_hash | 0045 |
| **SchemaMetadata** | version, created_at, updated_at | 0068 |
| **Expiration** | expires_at | 0044 |

---

## Validation Rules

Each sub-structure validates its own fields independently:

```rust
record.validate()?; // Recursively validates all sub-structures

// Sub-structure validation examples:
identity.validate()?;               // Check no field is empty
lora.validate()?;                   // Check rank >= 1, alpha >= 0, valid JSON
tier_config.validate()?;            // Check tier enum values
semantic_naming.validate()?;        // Check all-or-nothing + format
fork_metadata.validate()?;          // Check parent_id set if fork_type set
```

---

## Testing Coverage

The refactoring includes comprehensive test coverage:

```rust
#[cfg(test)]
mod tests {
    test_adapter_identity_validation()
    test_semantic_naming_validation()
    test_lora_config_validation()
    test_tier_config_validation()
    test_lifecycle_state_defaults()
    test_adapter_record_builder()
    test_flat_to_structured_conversion()
}
```

Run tests with:
```bash
cargo test -p adapteros-db adapter_record --lib
```

---

## Integration with Existing Code

### Current Code (adapters.rs)
```rust
pub async fn register_adapter(&self, params: AdapterRegistrationParams) -> Result<String> {
    // Inserts 28 fields in single SQL query
    sqlx::query("INSERT INTO adapters (...) VALUES (...)")
        .bind(&params.tenant_id)
        // ... 27 more binds
}
```

### Future Integration (with AdapterRecordV1)
```rust
pub async fn register_adapter_structured(
    &self,
    record: AdapterRecordV1,
) -> Result<String> {
    record.validate()?;  // Validate structure first
    let flat = record.to_flat()?;

    sqlx::query("INSERT INTO adapters (...) VALUES (...)")
        .bind(&flat.tenant_id)
        // ... etc
}
```

---

## Anti-Patterns to Avoid

1. **Bypassing Validation:** Don't construct sub-structures manually without calling `validate()`
2. **Partial Updates:** Don't update some fields of SemanticNaming without updating all
3. **Schema Version Drift:** Always increment `schema.version` when adding fields
4. **Flat Conversions:** Don't manually construct `FlatAdapterRow`; use `to_flat()`

---

## Future Enhancements

### Phase 2: Automated Migration Generation

```bash
# Future CLI command (not yet implemented)
cargo aos migrate add-field adapter_record.new_field String
# Generates: migrations/NNNN_add_new_field.sql
# Updates: src/adapter_record.rs with new field
# Updates: SchemaMetadata::version to "1.1.0"
```

### Phase 3: Multi-Version Support

```rust
// Load any version, auto-upgrade to current
pub async fn load_adapter_any_version(id: &str) -> Result<AdapterRecordV1> {
    let flat = sqlx::query_as::<_, FlatAdapterRow>(...)
        .fetch_one(pool)
        .await?;

    match flat.version.as_str() {
        "1.0.0" => AdapterRecordV1::from_flat(&flat),
        "2.0.0" => {
            let v2 = AdapterRecordV2::from_flat(&flat)?;
            v2.into_v1()  // Auto-downgrade if needed
        }
        _ => Err(AosError::Validation(format!("Unknown version: {}", flat.version))),
    }
}
```

---

## Migration Checklist

- [ ] Copy `adapter_record.rs` to production codebase
- [ ] Update `lib.rs` to export new module
- [ ] Add `chrono` dependency if building with timestamps
- [ ] Write integration tests in `tests/adapter_record_integration.rs`
- [ ] Update `adapters.rs` to use new builders internally
- [ ] Create feature flag `new-adapter-record` for gradual rollout
- [ ] Write docs/ADR for architectural decision
- [ ] Plan deprecation timeline for old `AdapterRegistrationParams`
- [ ] Update CLAUDE.md with new patterns

---

## References

- **File:** `/Users/star/Dev/aos/crates/adapteros-db/src/adapter_record.rs`
- **Parent File:** `/Users/star/Dev/aos/crates/adapteros-db/src/adapters.rs`
- **Related Docs:**
  - `docs/DATABASE_REFERENCE.md` - Full schema
  - `docs/MIGRATIONS.md` - Migration history (if exists)
  - `CLAUDE.md` - Coding standards

---

## Summary

This refactoring prevents schema drift by:

1. **Organizing Fields:** 36+ fields into 9 logical sub-structures
2. **Enforcing Validation:** Each structure validates its own constraints
3. **Supporting Versioning:** Schema version tracking for future evolution
4. **Maintaining Compatibility:** Flat ↔ Structured conversion for existing code
5. **Type Safety:** Builder pattern ensures required fields are set
6. **Clarity:** Each sub-structure documents its migration origin and invariants

The implementation is ready for integration and provides a foundation for long-term schema evolution without sacrificing backward compatibility.
