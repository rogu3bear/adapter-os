# AdapterMetadata Migration Checklist

**Status**: Analysis Complete
**Generated**: 2025-11-19
**Scope**: Consolidating fragmented AdapterMetadata types across codebase

## Executive Summary

The codebase currently has **3 conflicting definitions** of `AdapterMetadata` across different layers:

1. **adapteros-types** (canonical, core types)
2. **adapteros-api-types** (API response wrapper)
3. **adapteros-policy** (policy-specific with metrics)
4. **adapteros-domain** (domain adapter traits)

This creates duplication, inconsistency, and maintenance burden. No `AdapterResponse` type exists in adapteros-types yet.

---

## Current Type Definitions

### 1. adapteros-types::AdapterMetadata (CANONICAL)
**File**: `/crates/adapteros-types/src/adapters/metadata.rs`
**Purpose**: Core adapter lifecycle and metadata
**Fields**:
- `adapter_id` (String)
- `name` (String)
- `hash_b3` (String)
- `rank` (i32)
- `tier` (i32)
- `languages` (Vec<String>)
- `framework` (Option<String>)
- `version` (Option<String>)
- `created_at` (Option<String>)
- `updated_at` (Option<String>)

**Builder Methods**:
- `new()` - core constructor
- `with_languages()`
- `with_framework()`
- `with_version()`
- `with_timestamps()`

**Current Status**: ✅ Well-designed, serves as system backbone

---

### 2. adapteros-api-types::AdapterResponse (API-SPECIFIC)
**File**: `/crates/adapteros-api-types/src/adapters.rs`
**Purpose**: HTTP API response envelope
**Fields**:
- `schema_version` (String)
- `id` (String) - database row ID (differs from adapter_id)
- `adapter_id` (String)
- `name` (String)
- `hash_b3` (String)
- `rank` (i32)
- `tier` (i32)
- `languages` (Vec<String>)
- `framework` (Option<String>)
- `created_at` (String)
- `stats` (Option<AdapterStats>)

**Supporting Types**:
- `AdapterStats`: total_activations, selected_count, avg_gate_value, selection_rate
- `AdapterActivationResponse`: gate_value, selected, request_id
- `AdapterStateResponse`: state transitions
- `AdapterManifest`: framework_version, repo_id, commit_sha, intent, category, scope

**Current Status**: ✅ API-specific, correctly separated concerns

---

### 3. adapteros-policy::AdapterMetadata (POLICY-SPECIFIC)
**File**: `/crates/adapteros-policy/src/packs/adapters.rs`
**Purpose**: Adapter lifecycle policy enforcement
**Fields**:
- `adapter_id` (String)
- `adapter_name` (String)
- `adapter_type` (AdapterType enum)
- `version` (String)
- `created_at` (u64)
- `last_accessed` (u64)
- `activation_count` (u64)
- `total_requests` (u64)
- `quality_metrics` (QualityMetrics)
- `registry_status` (RegistryStatus enum)
- `eviction_status` (EvictionStatus enum)

**Supporting Types**:
- `AdapterType`: Ephemeral, DirectorySpecific, Framework, Code, Base
- `RegistryStatus`: Registered, Pending, Rejected, Suspended, NotRegistered
- `EvictionStatus`: Active, MarkedForEviction, Evicted, Protected
- `QualityMetrics`: accuracy, precision, recall, f1_score, latency_ms, memory_usage_mb

**Current Status**: ⚠️ Policy-specific, not a generic type

---

### 4. adapteros-domain::AdapterMetadata (DOMAIN-SPECIFIC)
**File**: `/crates/adapteros-domain/src/adapter.rs`
**Purpose**: Domain adapter trait execution
**Fields**:
- `name` (String)
- `version` (String)
- `model_hash` (B3Hash)
- `input_format` (String)
- `output_format` (String)
- `epsilon_threshold` (f64)
- `deterministic` (bool)
- `custom` (HashMap<String, serde_json::Value>)

**Current Status**: ⚠️ Completely different use case (domain adapters, not LoRA)

---

## File Usage Map

### Files Using adapteros-types::AdapterMetadata
| File | Usage | Pattern |
|------|-------|---------|
| `/crates/adapteros-types/src/lib.rs` | Re-export | Public API |
| `/crates/adapteros-types/src/adapters/mod.rs` | Re-export | Public API |
| `/crates/adapteros-domain/src/vision.rs` | Import | Type parameter |
| `/crates/adapteros-domain/src/text.rs` | Import | Type parameter |
| `/crates/adapteros-domain/src/telemetry.rs` | Import | Type parameter |
| `/crates/adapteros-domain/src/lib.rs` | Import | Type parameter |

### Files Using adapteros-api-types::AdapterResponse
| File | Usage | Pattern |
|------|-------|---------|
| `/crates/adapteros-server-api/src/handlers.rs` | Response type | 10+ endpoint handlers |
| `/crates/adapteros-server-api/src/handlers/adapter_stacks.rs` | Response type | Adapter stack ops |
| `/crates/adapteros-server-api/src/routes.rs` | OpenAPI schema | Documentation |
| `/crates/adapteros-server-api/tests/api_contracts.rs` | Test type | Contract verification |
| `/crates/adapteros-server-api/src/types.rs` | Re-export | Public API |

### Files Using adapteros-policy::AdapterMetadata
| File | Usage | Pattern |
|------|-------|---------|
| `/crates/adapteros-policy/src/packs/adapters.rs` | Local definition | Policy enforcement |
| `/crates/adapteros-policy/src/packs/mod.rs` | Reference | Policy registry |

### Files Using adapteros-domain::AdapterMetadata
| File | Usage | Pattern |
|------|-------|---------|
| `/crates/adapteros-domain/src/adapter.rs` | Local definition | Trait requirement |
| `/crates/adapteros-domain/src/vision.rs` | Reference | Vision domain |
| `/crates/adapteros-domain/src/text.rs` | Reference | Text domain |

### Files With Conversions
| File | Conversion | Target |
|------|-----------|--------|
| `/crates/adapteros-types/src/adapters/metadata.rs` | `From<AdapterMetadata>` → `RegisterAdapterRequest` | Request type |
| `/crates/adapteros-server-api/src/handlers.rs` | Database row → `AdapterResponse` | API response |

---

## Migration Strategy by Type

### Option A: Consolidate All to adapteros-types (RECOMMENDED)

**Approach**: Make `adapteros-types` the single source of truth for all adapter metadata.

#### Step 1: Create Unified Type in adapteros-types

```rust
// File: crates/adapteros-types/src/adapters/metadata.rs

/// Core adapter metadata (canonical, unified type)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMetadata {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: i32,
    pub languages: Vec<String>,
    pub framework: Option<String>,
    pub version: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// API response envelope wrapping AdapterMetadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,  // Database row ID
    #[serde(flatten)]
    pub metadata: AdapterMetadata,  // Flatten core fields
    pub created_at: String,  // Override as required
    pub stats: Option<AdapterStats>,
}

/// Adapter statistics for response envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterStats {
    pub total_activations: i64,
    pub selected_count: i64,
    pub avg_gate_value: f64,
    pub selection_rate: f64,
}
```

#### Step 2: Migrate adapteros-api-types

- **Remove** duplicate `AdapterResponse` definition
- **Import** from `adapteros-types`
- **Create** type alias for backward compatibility:
  ```rust
  pub use adapteros_types::adapters::{AdapterResponse, AdapterStats};
  ```

#### Step 3: Migrate adapteros-policy

- **Replace** policy-specific `AdapterMetadata` with wrapper type:
  ```rust
  pub struct PolicyAdapterMetadata {
      pub core: adapteros_types::AdapterMetadata,
      pub adapter_type: AdapterType,
      pub activation_count: u64,
      pub total_requests: u64,
      pub quality_metrics: QualityMetrics,
      pub registry_status: RegistryStatus,
      pub eviction_status: EvictionStatus,
  }
  ```

#### Step 4: Migrate adapteros-domain

- **Keep separate** - Domain adapters are fundamentally different
- **Rename** to `DomainAdapterMetadata` to avoid confusion:
  ```rust
  pub struct DomainAdapterMetadata {
      pub name: String,
      pub version: String,
      pub model_hash: B3Hash,
      pub input_format: String,
      pub output_format: String,
      pub epsilon_threshold: f64,
      pub deterministic: bool,
      pub custom: HashMap<String, serde_json::Value>,
  }
  ```

---

## Migration Checklist

### Phase 1: Foundation (adapteros-types)
- [ ] Add `AdapterResponse` struct to `crates/adapteros-types/src/adapters/metadata.rs`
- [ ] Add `AdapterStats` struct to support response stats
- [ ] Add builder methods: `with_stats()`, `with_response_id()`
- [ ] Add `From<AdapterMetadata>` impl for `AdapterResponse`
- [ ] Add test cases for new types
- [ ] Update module exports in `crates/adapteros-types/src/adapters/mod.rs`
- [ ] Update re-exports in `crates/adapteros-types/src/lib.rs`
- [ ] Verify no breaking changes to public API

### Phase 2: adapteros-api-types Migration
- [ ] Update `crates/adapteros-api-types/src/adapters.rs`:
  - [ ] Remove `struct AdapterResponse` definition
  - [ ] Remove `struct AdapterStats` definition
  - [ ] Add: `pub use adapteros_types::adapters::{AdapterResponse, AdapterStats};`
- [ ] Keep `AdapterManifest`, `AdapterActivationResponse`, `AdapterStateResponse` (API-specific)
- [ ] Keep `RegisterAdapterRequest` (may inherit fields from `AdapterMetadata`)
- [ ] Run tests: `cargo test -p adapteros-api-types`
- [ ] Verify OpenAPI generation works

### Phase 3: Server API Handler Updates
- [ ] Update imports in `crates/adapteros-server-api/src/handlers.rs`:
  - [ ] Change: `use adapteros_api_types::AdapterResponse;`
  - [ ] To: `use adapteros_types::adapters::AdapterResponse;`
- [ ] Update handlers in `crates/adapteros-server-api/src/handlers/adapter_stacks.rs`
- [ ] Update response construction to use new type
- [ ] Run tests: `cargo test -p adapteros-server-api`
- [ ] Verify API contracts: `cargo test -p adapteros-server-api --test api_contracts`

### Phase 4: Policy Layer Migration
- [ ] Create `PolicyAdapterMetadata` wrapper in `crates/adapteros-policy/src/packs/adapters.rs`
- [ ] Update `AdapterLifecyclePolicy` to work with wrapper:
  ```rust
  fn check_activation_requirements(&self, adapter: &PolicyAdapterMetadata) -> Result<Vec<String>>
  ```
- [ ] Update tests to use new wrapper type
- [ ] Run tests: `cargo test -p adapteros-policy`

### Phase 5: Domain Layer Separation
- [ ] Rename `adapteros-domain::AdapterMetadata` → `DomainAdapterMetadata`
- [ ] Update trait definition in `crates/adapteros-domain/src/adapter.rs`
- [ ] Update vision, text, telemetry implementations
- [ ] Add documentation explaining distinction from LoRA adapters
- [ ] Run tests: `cargo test -p adapteros-domain`

### Phase 6: Database Layer
- [ ] Review adapter row mapping in `crates/adapteros-db/src/adapters.rs`
- [ ] Verify conversion from database rows → `AdapterResponse`
- [ ] Check domain adapter mappings in `crates/adapteros-db/src/domain_adapters.rs`
- [ ] Run tests: `cargo test -p adapteros-db`

### Phase 7: CLI Updates
- [ ] Update adapter listing in `crates/adapteros-cli/src/commands/adapters.rs`
- [ ] Update registration command to use unified metadata
- [ ] Run tests: `cargo test -p adapteros-cli`

### Phase 8: Integration Tests
- [ ] Run full integration suite: `cargo test --workspace --exclude adapteros-lora-mlx-ffi`
- [ ] Run e2e tests: `cargo test --test e2e_inference_complete -- --ignored --nocapture`
- [ ] Verify adapter lifecycle flow
- [ ] Test adapter hot-swap operations

### Phase 9: Client Library
- [ ] Update `crates/adapteros-client/src/types.rs` if needed
- [ ] Update `crates/adapteros-client/src/native.rs` if constructing responses
- [ ] Run tests: `cargo test -p adapteros-client`

### Phase 10: Documentation & Final Verification
- [ ] Update type documentation in metadata.rs
- [ ] Add migration notes to CLAUDE.md
- [ ] Run duplication check: `make dup`
- [ ] Run format check: `cargo fmt --all`
- [ ] Run clippy: `cargo clippy --workspace -- -D warnings`
- [ ] Final integration test: `cargo test --workspace --exclude adapteros-lora-mlx-ffi`

---

## Type Conversion Mappings

### adapteros-types::AdapterMetadata → adapteros-api-types::AdapterResponse

```rust
impl From<(AdapterMetadata, String)> for AdapterResponse {
    fn from((metadata, id): (AdapterMetadata, String)) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            id,
            adapter_id: metadata.adapter_id,
            name: metadata.name,
            hash_b3: metadata.hash_b3,
            rank: metadata.rank,
            tier: metadata.tier,
            languages: metadata.languages,
            framework: metadata.framework,
            created_at: metadata.created_at.unwrap_or_default(),
            stats: None,
        }
    }
}
```

### Database Row → AdapterResponse (Handler Pattern)

```rust
// Current pattern in handlers.rs
let response = AdapterResponse {
    schema_version: SCHEMA_VERSION.to_string(),
    id: row.id,
    adapter_id: row.adapter_id,
    name: row.name,
    hash_b3: row.hash_b3,
    rank: row.rank,
    tier: row.tier,
    languages: serde_json::from_str(&row.languages_json).unwrap_or_default(),
    framework: row.framework,
    created_at: row.created_at,
    stats: Some(AdapterStats {
        total_activations: row.total_activations,
        selected_count: row.selected_count,
        avg_gate_value: row.avg_gate_value,
        selection_rate: row.selection_rate,
    }),
};

// Should become:
let metadata = AdapterMetadata {
    adapter_id: row.adapter_id.clone(),
    name: row.name.clone(),
    hash_b3: row.hash_b3.clone(),
    rank: row.rank,
    tier: row.tier,
    languages: serde_json::from_str(&row.languages_json).unwrap_or_default(),
    framework: row.framework.clone(),
    version: row.version.clone(),
    created_at: Some(row.created_at.clone()),
    updated_at: Some(row.updated_at.clone()),
};

let response = AdapterResponse {
    schema_version: SCHEMA_VERSION.to_string(),
    id: row.id,
    metadata,
    stats: Some(AdapterStats { ... }),
};
```

---

## Risk Assessment

### Low Risk
- ✅ adapteros-types consolidation (backward compatible re-exports)
- ✅ adapteros-api-types simplification (internal re-org)
- ✅ Database layer (no schema changes)

### Medium Risk
- ⚠️ Policy layer wrapper (must maintain all existing functionality)
- ⚠️ Domain layer rename (affects 4 files, but clear search/replace)

### Mitigation
- Run comprehensive test suite after each phase
- Use type aliases for backward compatibility during transition
- Keep git history clean with logical commits per phase

---

## Files Requiring Changes

### Direct Changes Required
1. `/crates/adapteros-types/src/adapters/metadata.rs` - ADD AdapterResponse
2. `/crates/adapteros-api-types/src/adapters.rs` - REMOVE/REPLACE definitions
3. `/crates/adapteros-policy/src/packs/adapters.rs` - REFACTOR with wrapper
4. `/crates/adapteros-domain/src/adapter.rs` - RENAME to DomainAdapterMetadata
5. `/crates/adapteros-domain/src/vision.rs` - UPDATE imports
6. `/crates/adapteros-domain/src/text.rs` - UPDATE imports

### Import Updates Required
- `/crates/adapteros-server-api/src/handlers.rs` (10+ locations)
- `/crates/adapteros-server-api/src/handlers/adapter_stacks.rs`
- `/crates/adapteros-server-api/src/routes.rs`
- `/crates/adapteros-client/src/lib.rs`
- `/crates/adapteros-client/src/native.rs`

### Test Updates Required
- `/crates/adapteros-server-api/tests/api_contracts.rs`
- `/crates/adapteros-policy/src/packs/adapters.rs` (tests)
- `/crates/adapteros-domain/src/adapter.rs` (tests)

---

## Expected Outcome

**After migration**:
- ✅ Single canonical `AdapterMetadata` in adapteros-types
- ✅ `AdapterResponse` properly layered (core + API envelope)
- ✅ Policy-specific logic separated via wrapper type
- ✅ Domain adapters clearly distinguished by type name
- ✅ No type duplication across crates
- ✅ Reduced maintenance burden
- ✅ Improved type safety and consistency

**Type Hierarchy After**:
```
adapteros-types::AdapterMetadata
├── adapteros-api-types::AdapterResponse (API wrapper)
├── adapteros-policy::PolicyAdapterMetadata (wrapper with lifecycle)
├── adapteros-db conversions (from rows)
└── adapteros-types::RegisterAdapterRequest (conversion)

adapteros-domain::DomainAdapterMetadata (SEPARATE - domain-specific)
```

---

## Success Criteria

- [ ] All existing tests pass
- [ ] No breaking changes to public APIs
- [ ] Duplication checker shows improvement
- [ ] Type names are unambiguous
- [ ] Documentation is updated
- [ ] No orphaned struct definitions
- [ ] Conversion layers are type-safe

