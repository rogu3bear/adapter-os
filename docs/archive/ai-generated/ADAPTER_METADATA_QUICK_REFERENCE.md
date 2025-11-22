# AdapterMetadata Migration: Quick Reference

## What's the Problem?

**4 conflicting `AdapterMetadata` types** exist across the codebase:

```
adapteros-types::AdapterMetadata
  ↓ (duplicated in)
adapteros-api-types::AdapterResponse  ← Contains subset of metadata fields
adapteros-policy::AdapterMetadata      ← Completely different fields! (name collision)
adapteros-domain::AdapterMetadata      ← Domain-specific, unrelated to LoRA (name collision)
```

**Impact**: Type confusion, duplication, maintenance burden

---

## The Three Types (Simplified View)

### 1. LoRA Adapter Metadata (Core)
**Location**: `adapteros-types::AdapterMetadata`

**Purpose**: Single source of truth for LoRA adapter properties
```rust
struct AdapterMetadata {
    adapter_id,    // "tenant/domain/purpose/r001"
    name,          // Human-readable name
    hash_b3,       // Content hash
    rank,          // LoRA rank (16, 32, 64)
    tier,          // Memory tier (0=Metal, 1=RAM, 2=Disk)
    languages,     // ["rust", "go", "python"]
    framework,     // "qwen2.5-7b"
    version,       // Adapter version
    created_at,    // ISO 8601 timestamp
    updated_at,    // ISO 8601 timestamp
}
```

### 2. API Response Envelope
**Current Location**: `adapteros-api-types::AdapterResponse` (SHOULD BE IN types)

**Purpose**: HTTP response with metadata + metadata + stats
```rust
struct AdapterResponse {
    schema_version,              // API version
    id,                          // DB row ID
    adapter_id,                  // (DUPLICATE from metadata)
    name,                        // (DUPLICATE from metadata)
    hash_b3,                     // (DUPLICATE from metadata)
    rank,                        // (DUPLICATE from metadata)
    tier,                        // (DUPLICATE from metadata)
    languages,                   // (DUPLICATE from metadata)
    framework,                   // (DUPLICATE from metadata)
    created_at,                  // (DUPLICATE from metadata)
    stats: Option<AdapterStats>, // Router selection stats
}
```

### 3. Policy Enforcement Metadata (CONFLICT!)
**Location**: `adapteros-policy::AdapterMetadata` ⚠️ NAME COLLISION

**Purpose**: Policy-specific adapter lifecycle tracking
```rust
struct AdapterMetadata {  // ← DIFFERENT MEANING!
    adapter_id,
    adapter_name,              // Field rename (confusing!)
    adapter_type,              // ← NEW: Ephemeral/Framework/Base
    version,
    created_at,                // ← DIFFERENT TYPE: u64!
    activation_count,          // ← NEW: Policy tracking
    total_requests,            // ← NEW: Policy tracking
    quality_metrics,           // ← NEW: Accuracy/Precision/F1
    registry_status,           // ← NEW: Registered/Pending/Rejected
    eviction_status,           // ← NEW: Active/Protected/Evicted
}
```

### 4. Domain Adapter Metadata (CONFLICT!)
**Location**: `adapteros-domain::AdapterMetadata` ⚠️ WRONG DOMAIN

**Purpose**: NOT for LoRA adapters - for domain-specific adapters (vision, text, etc.)
```rust
struct AdapterMetadata {  // ← COMPLETELY DIFFERENT PURPOSE!
    name,              // Domain adapter name
    version,           // Domain adapter version
    model_hash,        // ← NEW: Model hash
    input_format,      // ← NEW: I/O format
    output_format,     // ← NEW: I/O format
    epsilon_threshold, // ← NEW: Numerical drift threshold
    deterministic,     // ← NEW: Determinism flag
    custom,            // ← NEW: Custom metadata map
}
```

---

## Why This Is Bad

| Problem | Impact |
|---------|--------|
| **Name collision** | Importing wrong type can crash at runtime |
| **Field duplication** | 7 fields duplicated in AdapterResponse |
| **Type inconsistency** | `created_at` is Option<String> vs String vs u64 |
| **Semantic confusion** | "adapter_type" only in policy, not core |
| **Maintenance burden** | Changes in 3 places instead of 1 |
| **Scalability** | Adding new fields requires 3+ edits |
| **Testing complexity** | Must verify consistency across types |

---

## The Solution: Layered Architecture

### Before: Fragmented
```
┌─ adapteros-types::AdapterMetadata (canonical)
├─ adapteros-api-types::AdapterResponse (DUPLICATE)
├─ adapteros-policy::AdapterMetadata (CONFLICT)
└─ adapteros-domain::AdapterMetadata (CONFLICT)
```

### After: Layered
```
┌─ adapteros-types::AdapterMetadata (LoRA core)
│  └─ adapteros-types::AdapterResponse (API envelope, wraps metadata)
│
├─ adapteros-policy::PolicyAdapterMetadata (wrapper, clear name!)
│  └─ core: AdapterMetadata (composition)
│  └─ + policy fields: adapter_type, quality_metrics, etc.
│
└─ adapteros-domain::DomainAdapterMetadata (renamed, clear purpose!)
   └─ Completely separate from LoRA adapters
```

---

## Files to Change (Summary)

### MUST CHANGE
1. **adapteros-types** - ADD AdapterResponse struct
2. **adapteros-api-types** - REMOVE AdapterResponse, re-export from types
3. **adapteros-policy** - RENAME AdapterMetadata → PolicyAdapterMetadata
4. **adapteros-domain** - RENAME AdapterMetadata → DomainAdapterMetadata

### SHOULD UPDATE IMPORTS
5. **adapteros-server-api** - Import from types instead of api-types (10+ locations)
6. **adapteros-client** - Import unified types
7. **tests** - Update test assertions

### NO CHANGE NEEDED
- **adapteros-db** - Database schema stays same
- **CLI commands** - Just update imports
- **Domain layer logic** - Rename is backward compatible

---

## Conversion Patterns

### Pattern 1: Register Request → Metadata
```rust
// BEFORE: RegisterAdapterRequest duplicates fields
pub struct RegisterAdapterRequest {
    adapter_id, name, hash_b3, rank, tier, languages, framework,
}

// AFTER: Can construct metadata from request
let metadata = AdapterMetadata::from(request);
```

### Pattern 2: Database Row → Response
```rust
// BEFORE: Handler manually constructs AdapterResponse
let response = AdapterResponse {
    schema_version, id, adapter_id, name, hash_b3, rank, tier,
    languages, framework, created_at, stats,
};

// AFTER: Row → Metadata → Response
let metadata = AdapterMetadata::from(&row);
let response = AdapterResponse {
    schema_version,
    id: row.id,
    metadata,  // Flattened in JSON
    stats: Some(AdapterStats { ... }),
};
```

### Pattern 3: Policy Enforcement
```rust
// BEFORE: Policy defines its own confusing AdapterMetadata
fn check(&self, adapter: &policy::AdapterMetadata) { ... }

// AFTER: Clear wrapper type
fn check(&self, adapter: &PolicyAdapterMetadata) { ... }
// Inside: access adapter.core for LoRA fields, adapter.quality_metrics for policy
```

### Pattern 4: Domain Adapter Trait
```rust
// BEFORE: Trait uses confusing name (same as LoRA!)
pub trait DomainAdapter {
    fn metadata(&self) -> &AdapterMetadata;  // Which one?!
}

// AFTER: Crystal clear
pub trait DomainAdapter {
    fn metadata(&self) -> &DomainAdapterMetadata;  // Unambiguous
}
```

---

## Migration Steps (Sequential)

### Step 1: Add AdapterResponse to types (15 min)
```bash
# File: crates/adapteros-types/src/adapters/metadata.rs
# ADD: pub struct AdapterResponse { ... }
# ADD: pub struct AdapterStats { ... }
```

### Step 2: Update api-types (10 min)
```bash
# File: crates/adapteros-api-types/src/adapters.rs
# DELETE: struct AdapterResponse { ... }
# DELETE: struct AdapterStats { ... }
# ADD: pub use adapteros_types::adapters::{AdapterResponse, AdapterStats};
```

### Step 3: Fix policy naming (20 min)
```bash
# File: crates/adapteros-policy/src/packs/adapters.rs
# RENAME: pub struct AdapterMetadata → pub struct PolicyAdapterMetadata
# UPDATE: All usages in same file (search/replace)
```

### Step 4: Fix domain naming (15 min)
```bash
# Files: adapteros-domain/src/*.rs
# RENAME: pub struct AdapterMetadata → pub struct DomainAdapterMetadata
# UPDATE: 4 files that use it
```

### Step 5: Update server-api imports (20 min)
```bash
# File: crates/adapteros-server-api/src/handlers.rs
# REPLACE: use adapteros_api_types::AdapterResponse;
# WITH: use adapteros_types::adapters::AdapterResponse;
# DO THIS in 10+ locations
```

### Step 6: Run tests (5 min)
```bash
cargo test --workspace --exclude adapteros-lora-mlx-ffi
```

### Step 7: Check duplication (5 min)
```bash
make dup
```

---

## Validation Checklist

- [ ] No compilation errors: `cargo build --release`
- [ ] All tests pass: `cargo test --workspace --exclude adapteros-lora-mlx-ffi`
- [ ] No duplicate code: `make dup` shows improvement
- [ ] Format checks: `cargo fmt --all` with no changes
- [ ] Lint checks: `cargo clippy --workspace -- -D warnings`
- [ ] No circular imports: Check with `cargo tree`
- [ ] Type names unambiguous: Can't accidentally import wrong type
- [ ] Documentation updated: Comments explain layer boundaries

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Breaking API | Use type aliases during transition |
| Test failures | Run full suite after each step |
| Import confusion | Rename PolicyAdapterMetadata, DomainAdapterMetadata clearly |
| Database issues | Zero DB schema changes needed |
| Compilation | Use `cargo check` iteratively |

---

## Success Criteria

When done, verify:
1. ✅ Single `AdapterMetadata` definition in adapteros-types
2. ✅ `AdapterResponse` properly layered in types
3. ✅ Policy uses `PolicyAdapterMetadata` (no name collision)
4. ✅ Domain uses `DomainAdapterMetadata` (no name collision)
5. ✅ All tests pass
6. ✅ No circular dependencies
7. ✅ Duplication reduced by >10%
8. ✅ Type hierarchy is clear and documented

---

## Related Documents

- **ADAPTER_METADATA_MIGRATION_CHECKLIST.md** - Detailed checklist with 10 phases
- **ADAPTER_METADATA_ANALYSIS.md** - Complete technical analysis with diagrams
- **crates/adapteros-types/src/adapters/metadata.rs** - Current canonical definition
- **CLAUDE.md** - Project guidelines (check before implementing)

---

## Questions & Answers

**Q: Can we keep both types during migration?**
A: Yes! Use type aliases: `pub type LegacyAdapterResponse = adapteros_types::AdapterResponse;`

**Q: Does this break the API?**
A: No! The JSON response structure stays identical. Only internal type names change.

**Q: What about database queries?**
A: Database schema unchanged. ORM mappings just import from different module.

**Q: How long will this take?**
A: About 1-2 hours for an experienced developer (5 sequential steps, ~15 min each).

**Q: Should we backport to older versions?**
A: Only if actively supported. Main branch first, then cherry-pick if needed.

