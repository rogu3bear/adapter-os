# AdapterMetadata Type Fragmentation Analysis

## Problem Statement

The codebase has **4 conflicting definitions** of adapter-related metadata across different architectural layers, causing:
- Type duplication and confusion
- Maintenance burden
- Inconsistent semantics across boundaries
- Difficult to reason about data flow

---

## Current Architecture: Fragmented Types

```
┌─────────────────────────────────────────────────────────────────────┐
│                       CONTROL PLANE API                             │
│  (adapteros-server-api/handlers)                                    │
│                                                                       │
│  GET /adapters → Vec<AdapterResponse>                              │
│  POST /adapters → AdapterResponse                                  │
│  GET /adapters/{id} → AdapterResponse                              │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
                        (CONVERSION FROM)
                               │
        ┌──────────────────────┴──────────────────────┐
        │                                             │
        ▼                                             ▼
┌──────────────────────┐              ┌──────────────────────┐
│  adapteros-api-types │              │   adapteros-db       │
│                      │              │                      │
│ pub struct          │              │ SELECT * FROM        │
│ AdapterResponse {   │              │ adapters → AdapterRow│
│   schema_version,   │              │                      │
│   id,               │              │ (Database row        │
│   adapter_id,       │              │  mapped to response) │
│   name,             │              │                      │
│   hash_b3,          │              │                      │
│   rank,             │              │                      │
│   tier,             │              │                      │
│   languages,        │              │                      │
│   framework,        │              │                      │
│   created_at,       │              │                      │
│   stats: Option<    │              │                      │
│     AdapterStats>   │              │                      │
│ }                   │              │                      │
└──────────────────────┘              └──────────────────────┘
        │                                      ▲
        │ (duplicates fields)                  │
        │                                      │
        └──────────────────────┬───────────────┘
                               │
                               ▼
                     ┌──────────────────────┐
                     │ adapteros-types      │
                     │                      │
                     │ pub struct          │
                     │ AdapterMetadata {   │
                     │   adapter_id,       │
                     │   name,             │
                     │   hash_b3,          │
                     │   rank,             │
                     │   tier,             │
                     │   languages,        │
                     │   framework,        │
                     │   version,          │
                     │   created_at,       │
                     │   updated_at        │
                     │ }                   │
                     │                     │
                     │ + RegisterAdapter   │
                     │   Request           │
                     └──────────────────────┘
                               │
                               │ (used by policy)
                               │
                               ▼
                     ┌──────────────────────┐
                     │ adapteros-policy     │
                     │                      │
                     │ pub struct          │
                     │ AdapterMetadata {   │
                     │   adapter_id,       │
                     │   adapter_name,     │
                     │   adapter_type,     │ ← DIFFERENT!
                     │   version,          │
                     │   created_at,       │
                     │   activation_count, │ ← DIFFERENT!
                     │   total_requests,   │ ← DIFFERENT!
                     │   quality_metrics,  │ ← DIFFERENT!
                     │   registry_status,  │ ← DIFFERENT!
                     │   eviction_status   │ ← DIFFERENT!
                     │ }                   │
                     └──────────────────────┘

┌──────────────────────────────────────────────────────────────────────┐
│                        DOMAIN LAYER                                   │
│  (adapteros-domain)                                                  │
│                                                                       │
│  Completely different: DomainAdapter trait implementations           │
│                                                                       │
│  pub struct AdapterMetadata {                                        │
│    name,                   ← Field intersection: SAME NAME, DIFF TYPE│
│    version,                ← Field intersection: SAME NAME, DIFF TYPE│
│    model_hash,             ← COMPLETELY DIFFERENT!                  │
│    input_format,           ← COMPLETELY DIFFERENT!                  │
│    output_format,          ← COMPLETELY DIFFERENT!                  │
│    epsilon_threshold,      ← COMPLETELY DIFFERENT!                  │
│    deterministic,          ← COMPLETELY DIFFERENT!                  │
│    custom                  ← COMPLETELY DIFFERENT!                  │
│  }                                                                    │
└──────────────────────────────────────────────────────────────────────┘
```

---

## Type Comparison Matrix

| Field | types | api-types | policy | domain |
|-------|-------|-----------|--------|--------|
| **adapter_id** | ✓ String | ✓ String | ✓ String | ✗ |
| **name** | ✓ String | ✓ String | ✓ adapter_name | ✓ String |
| **hash_b3** | ✓ String | ✓ String | ✗ | ✗ |
| **rank** | ✓ i32 | ✓ i32 | ✗ | ✗ |
| **tier** | ✓ i32 | ✓ i32 | ✗ | ✗ |
| **languages** | ✓ Vec<String> | ✓ Vec<String> | ✗ | ✗ |
| **framework** | ✓ Option<String> | ✓ Option<String> | ✗ | ✗ |
| **version** | ✓ Option<String> | ✗ | ✓ String | ✓ String |
| **created_at** | ✓ Option<String> | ✓ String | ✓ u64 | ✗ |
| **updated_at** | ✓ Option<String> | ✗ | ✗ | ✗ |
| **id** (db row) | ✗ | ✓ String | ✗ | ✗ |
| **schema_version** | ✗ | ✓ String | ✗ | ✗ |
| **stats** | ✗ | ✓ Option<AdapterStats> | ✗ | ✗ |
| **adapter_type** | ✗ | ✗ | ✓ AdapterType | ✗ |
| **activation_count** | ✗ | ✗ | ✓ u64 | ✗ |
| **total_requests** | ✗ | ✗ | ✓ u64 | ✗ |
| **quality_metrics** | ✗ | ✗ | ✓ QualityMetrics | ✗ |
| **registry_status** | ✗ | ✗ | ✓ RegistryStatus | ✗ |
| **eviction_status** | ✗ | ✗ | ✓ EvictionStatus | ✗ |
| **model_hash** | ✗ | ✗ | ✗ | ✓ B3Hash |
| **input_format** | ✗ | ✗ | ✗ | ✓ String |
| **output_format** | ✗ | ✗ | ✗ | ✓ String |
| **epsilon_threshold** | ✗ | ✗ | ✗ | ✓ f64 |
| **deterministic** | ✗ | ✗ | ✗ | ✓ bool |
| **custom** | ✗ | ✗ | ✗ | ✓ HashMap |

**Key Observations**:
1. **Overlap without consistency**: name, version appear in 3+ types with different semantics
2. **API envelope mixing**: api-types mixes core metadata with API-specific fields (id, schema_version)
3. **Policy-specific extension**: 8 fields unique to policy layer
4. **Domain completely separate**: All 8 domain fields are unique (not extensible common type)

---

## File Dependency Graph

### Files That Import AdapterMetadata/AdapterResponse

```
adapteros-types (source)
├── Exported: public API
├── Used by: adapteros-domain (4 files)
├── Used by: adapteros-cli
├── Used by: adapteros-api-types (indirect)
└── Type: Core, authoritative

adapteros-api-types (wrapper)
├── Redefines: AdapterResponse
├── Redefines: AdapterStats
├── Used by: adapteros-server-api (handlers)
├── Used by: adapteros-server (main)
├── Used by: adapteros-client (UDS client)
├── Used by: tests
└── Type: API-specific, DUPLICATE

adapteros-policy (extension)
├── Redefines: AdapterMetadata (different!)
├── Extends: With lifecycle tracking
├── Used by: Policy enforcement
├── Scoped: To policy package
└── Type: Domain-specific, CONFLICTING NAME

adapteros-domain (separate)
├── Defines: AdapterMetadata (completely different!)
├── Used by: Domain adapter implementations
├── Scope: Vision, Text, Telemetry
└── Type: Domain-specific, CONFLICTING NAME

adapteros-server-api (consumer)
├── Imports: adapteros-api-types::AdapterResponse
├── Converts: Database rows → AdapterResponse
├── Uses: In 10+ HTTP endpoint handlers
├── Risk: High coupling to api-types definition
└── Pattern: Response construction

adapteros-db (persistence)
├── Defines: AdapterRow
├── Reads: adapters table
├── Outputs: Rows for conversion
├── Pattern: ORM-like mapping
└── Risk: Must match database schema
```

---

## Use Case Analysis

### LoRA Adapter Lifecycle (Canonical Path)

```
Registration Flow:
┌─────────────────────────────────┐
│ User: Register LoRA Adapter     │
└──────────────────┬──────────────┘
                   │
                   ▼
        ┌──────────────────────────┐
        │ RegisterAdapterRequest   │
        │ (from types)             │
        └──────────────┬───────────┘
                       │
                       ▼
        ┌──────────────────────────┐
        │ Database INSERT          │
        │ adapters table           │
        └──────────────┬───────────┘
                       │
                       ▼
        ┌──────────────────────────┐
        │ SELECT from adapters     │
        │ → AdapterRow             │
        │ (from db)                │
        └──────────────┬───────────┘
                       │
                       ▼
        ┌──────────────────────────┐
        │ Convert Row →            │
        │ AdapterResponse          │
        │ (in handler)             │
        └──────────────┬───────────┘
                       │
                       ▼
        ┌──────────────────────────┐
        │ HTTP 201 Created         │
        │ { AdapterResponse }      │
        └──────────────────────────┘

Problem Areas:
⚠️ RegisterAdapterRequest duplicates fields from AdapterMetadata
⚠️ AdapterResponse duplicates fields from AdapterMetadata
⚠️ Handler must manually construct response from row
⚠️ type-conversion spans 4 crates with no clear ownership
```

### Policy Enforcement (Side Path)

```
Policy Check Flow:
┌──────────────────────────────────┐
│ Adapter Loading                  │
└──────────────┬───────────────────┘
               │
               ▼
┌──────────────────────────────────┐
│ Fetch: SELECT * FROM adapters    │
│ → AdapterRow                     │
└──────────────┬───────────────────┘
               │
               ▼
┌──────────────────────────────────┐
│ Convert Row → policy::Metadata   │
│ + compute metrics                │
└──────────────┬───────────────────┘
               │
               ▼
┌──────────────────────────────────┐
│ AdapterLifecyclePolicy::enforce()│
│ - activation_requirements        │
│ - quality_metrics                │
│ - registry_status                │
└──────────────┬───────────────────┘
               │
               ▼
┌──────────────────────────────────┐
│ Audit { violations, warnings }   │
└──────────────────────────────────┘

Problem:
⚠️ Policy defines its own AdapterMetadata (name collision!)
⚠️ No way to know this is different from types::AdapterMetadata
⚠️ Can't use same metadata object for multiple purposes
⚠️ Runtime type confusion possible
```

### Domain Adapter (Isolated Path)

```
Domain Adapter Flow:
┌──────────────────────────────────┐
│ Domain Adapter: Vision           │
└──────────────┬───────────────────┘
               │
               ▼
┌──────────────────────────────────┐
│ Implement: DomainAdapter trait   │
│ metadata(): &AdapterMetadata     │
└──────────────┬───────────────────┘
               │
               ▼
┌──────────────────────────────────┐
│ Forward pass with metadata:      │
│ - input_format validation        │
│ - output_format transform        │
│ - epsilon tracking               │
│ - determinism verification       │
└──────────────────────────────────┘

Problem:
⚠️ Uses AdapterMetadata name but totally different semantics
⚠️ Not related to LoRA adapters at all
⚠️ Confuses type naming throughout domain layer
⚠️ Potential import conflicts if not careful
```

---

## Data Flow Diagram: Current vs. Proposed

### Current (Fragmented)

```
USER REGISTRATION REQUEST
        ↓
   [API Handler]
        ↓
   [RegisterAdapterRequest] ← types
        ↓
   [Database INSERT]
        ↓
   [AdapterRow] ← db
        ↓
   [Manual Construction] ← handler
        ↓
   [AdapterResponse] ← api-types
   (DUPLICATES types fields!)
        ↓
   [JSON RESPONSE]

SIDE-CHANNEL: POLICY
   [AdapterRow] ← db
        ↓
   [Manual Conversion] ← handler
        ↓
   [policy::AdapterMetadata] ← policy
   (NAME COLLISION with types!)
        ↓
   [AdapterLifecyclePolicy::enforce()]
```

### Proposed (Unified)

```
USER REGISTRATION REQUEST
        ↓
   [API Handler]
        ↓
   [RegisterAdapterRequest] ← types
        ↓
   [Database INSERT]
        ↓
   [AdapterRow] ← db
        ↓
   [AdapterMetadata] ← types (unified!)
        ↓
   [AdapterResponse] ← types (wraps metadata)
   (NO DUPLICATION!)
        ↓
   [JSON RESPONSE]

SIDE-CHANNEL: POLICY
   [AdapterRow] ← db
        ↓
   [AdapterMetadata] ← types
        ↓
   [PolicyAdapterMetadata] ← policy (wrapper, no name collision)
   (Adds: activation_count, quality_metrics, etc.)
        ↓
   [AdapterLifecyclePolicy::enforce()]
```

---

## Semantic Issues

### Issue 1: `created_at` Type Inconsistency

| Type | created_at | Reason |
|------|-----------|--------|
| types | Option<String> | ISO 8601, optional |
| api-types | String | Required in response |
| policy | u64 | Unix timestamp |
| domain | N/A | Not used |

**Problem**: Same field, 3 different types → conversion errors, data loss

### Issue 2: `version` Field Confusion

| Type | version | Meaning |
|------|---------|---------|
| types | Option<String> | Adapter software version |
| policy | String | Adapter software version (required) |
| domain | String | Domain adapter version |

**Problem**: Same name, potentially different semantics

### Issue 3: Name Field Shadowing

| Type | name | Notes |
|------|------|-------|
| types | String | Adapter human-readable name |
| policy | adapter_name | Adapter human-readable name (renamed!) |
| domain | String | Domain adapter name |

**Problem**: Name inconsistency forces developers to check crate context

---

## Recommended Solution: Layered Types

```
┌────────────────────────────────────────────────────────────────┐
│                                                                 │
│  LAYER 1: Core Types (adapteros-types)                         │
│  ────────────────────────────────────────────────────────────  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ pub struct AdapterMetadata {                             │  │
│  │   // Core LoRA adapter properties                        │  │
│  │   adapter_id, name, hash_b3, rank, tier,               │  │
│  │   languages, framework, version, created_at, updated_at│  │
│  │ }                                                        │  │
│  │                                                          │  │
│  │ pub struct AdapterResponse {                            │  │
│  │   // API response envelope                              │  │
│  │   schema_version,                                       │  │
│  │   id,                  // db row ID                      │  │
│  │   #[flatten]                                            │  │
│  │   metadata: AdapterMetadata,  // core fields            │  │
│  │   created_at,          // override from db              │  │
│  │   stats: Option<AdapterStats>,                          │  │
│  │ }                                                        │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  LAYER 2: API Types (adapteros-api-types)                      │
│  ────────────────────────────────────────────────────────────  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ // Re-export from types                                  │  │
│  │ pub use adapteros_types::adapters::*;                   │  │
│  │                                                          │  │
│  │ // API-specific wrappers (beyond core metadata)         │  │
│  │ pub struct AdapterManifest { ... }                      │  │
│  │ pub struct AdapterActivationResponse { ... }            │  │
│  │ pub struct AdapterStateResponse { ... }                 │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  LAYER 3: Policy Types (adapteros-policy)                      │
│  ────────────────────────────────────────────────────────────  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ pub struct PolicyAdapterMetadata {                       │  │
│  │   // Wrap core metadata, add policy-specific fields      │  │
│  │   core: adapteros_types::AdapterMetadata,              │  │
│  │   // Policy extensions                                  │  │
│  │   adapter_type, activation_count, total_requests,       │  │
│  │   quality_metrics, registry_status, eviction_status     │  │
│  │ }                                                        │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  LAYER 4: Domain Types (adapteros-domain)                      │
│  ────────────────────────────────────────────────────────────  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ // SEPARATE from LoRA adapters - completely different   │  │
│  │ pub struct DomainAdapterMetadata {                       │  │
│  │   // Domain-specific: inference, I/O, determinism       │  │
│  │   name, version, model_hash, input_format,             │  │
│  │   output_format, epsilon_threshold, deterministic,      │  │
│  │   custom                                                 │  │
│  │ }                                                        │  │
│  │                                                          │  │
│  │ // Clear trait boundary                                 │  │
│  │ pub trait DomainAdapter {                               │  │
│  │   fn metadata(&self) -> &DomainAdapterMetadata;        │  │
│  │   // ... other methods                                  │  │
│  │ }                                                        │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
└────────────────────────────────────────────────────────────────┘
```

---

## Migration Impact Assessment

| Component | Impact | Effort | Risk |
|-----------|--------|--------|------|
| adapteros-types | ADD AdapterResponse | Low | None |
| adapteros-api-types | Remove dups, re-export | Low | Low |
| adapteros-policy | Create wrapper type | Medium | Medium |
| adapteros-domain | Rename DomainAdapterMetadata | Low | Low |
| adapteros-server-api | Update imports | Low | Low |
| adapteros-db | No changes | None | None |
| tests | Update assertions | Low | Low |
| **TOTAL** | | **Low-Med** | **Low** |

**Timeline Estimate**: 4-6 focused development steps

---

## Success Metrics

After migration:
- [ ] Zero duplicate `AdapterMetadata` definitions
- [ ] Zero ambiguous type names (DomainAdapterMetadata is clear)
- [ ] `AdapterResponse` clearly defined as API envelope
- [ ] Policy layer extensible via wrapper pattern
- [ ] All tests passing
- [ ] Duplication checker shows <5% improvement
- [ ] Type relationship graph is acyclic
- [ ] Import chains are unidirectional (no circular)

