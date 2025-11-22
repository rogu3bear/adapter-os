# Validation Schema Extraction Summary

This document details the validation rules extracted from the Rust backend and their corresponding Zod schemas.

## Extraction Sources

### 1. Backend Type Definitions

| Rust File | Purpose | Extracted Schemas |
|-----------|---------|-------------------|
| `crates/adapteros-api-types/src/training.rs` | Training API types | `BackendTrainingConfigSchema`, `StartTrainingRequestSchema`, `UploadDatasetRequestSchema` |
| `crates/adapteros-api-types/src/adapters.rs` | Adapter API types | `RegisterAdapterRequestSchema`, `AdapterManifest` |
| `crates/adapteros-api-types/src/inference.rs` | Inference API types | `BackendInferRequestSchema`, `RouterDecisionSchema`, `InferenceTraceSchema` |
| `crates/adapteros-types/src/training/mod.rs` | Core training types | `TrainingConfig`, `TrainingJob`, `TrainingJobStatus` |

### 2. Backend Validation Logic

| Rust File | Function | Extracted Rules |
|-----------|----------|-----------------|
| `adapteros-server-api/src/validation.rs` | `validate_repo_id()` | Repository format: `owner/repo`, max 100 chars |
| `adapteros-server-api/src/validation.rs` | `validate_commit_sha()` | Git SHA: 7-40 hex chars (lowercase) |
| `adapteros-server-api/src/validation.rs` | `validate_tenant_id()` | Tenant: lowercase, alphanumeric, hyphens, underscores, max 50 chars |
| `adapteros-server-api/src/validation.rs` | `validate_languages()` | Supported: python, rust, typescript, javascript, go, java, c, cpp, csharp |
| `adapteros-server-api/src/validation.rs` | `validate_file_paths()` | No directory traversal (..), no absolute paths, max 500 chars |
| `adapteros-server-api/src/validation.rs` | `validate_description()` | Max 5000 chars, security checks (SQL injection, XSS) |
| `adapteros-server-api/src/validation.rs` | `validate_hash_b3()` | BLAKE3 format: `b3:{64 hex chars}` |

### 3. Backend Policy Enforcement

| Rust File | Policy | Extracted Rules |
|-----------|--------|-----------------|
| `adapteros-policy/src/packs/naming_policy.rs` | Naming Policy | Semantic adapter names, reserved namespaces, tenant isolation, revision gap (max 5) |
| `adapteros-policy/src/packs/naming_policy.rs` | Reserved Tenants | system, admin, root, default, test |
| `adapteros-policy/src/packs/naming_policy.rs` | Reserved Domains | core, internal, deprecated |
| `adapteros-policy/src/packs/naming_policy.rs` | Profanity Filter | Configurable word list checking |
| `adapteros-policy/src/packs/naming_policy.rs` | Hierarchy Enforcement | Same lineage for parent/child adapters |

### 4. Backend Constants

| Rust File | Constant | Value | Mapped Schema |
|-----------|----------|-------|---------------|
| `adapteros-lora-router/src/lib.rs` | `MAX_K` | 8 | Stack adapter limit |
| `adapteros-server-api/src/handlers/datasets.rs` | `MAX_FILE_SIZE` | 100 MB | `FileSizeSchema` |
| `adapteros-server-api/src/handlers/datasets.rs` | `MAX_TOTAL_SIZE` | 500 MB | Total dataset size |
| `adapteros-server-api/src/handlers/chunked_upload.rs` | `MIN_CHUNK_SIZE` | 1 MB | `ChunkSizeSchema` min |
| `adapteros-server-api/src/handlers/chunked_upload.rs` | `DEFAULT_CHUNK_SIZE` | 10 MB | `ChunkSizeSchema` default |
| `adapteros-server-api/src/handlers/chunked_upload.rs` | `MAX_CHUNK_SIZE` | 100 MB | `ChunkSizeSchema` max |
| `adapteros-server-api/src/handlers/batch.rs` | `MAX_BATCH_SIZE` | 32 | `BatchSizeSchema` |
| `adapteros-api/src/streaming.rs` | `default_max_tokens()` | 512 | Inference default |
| `adapteros-api/src/streaming.rs` | `default_temperature()` | 0.7 | Inference default |

## Schema Mappings

### Training Configuration

**Source:** `crates/adapteros-types/src/training/mod.rs::TrainingConfig`

```rust
pub struct TrainingConfig {
    pub rank: u32,                      // 1-256
    pub alpha: u32,                     // 1-512
    pub targets: Vec<String>,           // 1-20 items
    pub epochs: u32,                    // 1-1000
    pub learning_rate: f32,             // 0.0-1.0
    pub batch_size: u32,                // 1-512
    pub warmup_steps: Option<u32>,      // 0-10000
    pub max_seq_length: Option<u32>,    // 128-8192
    pub gradient_accumulation_steps: Option<u32>, // 1-64
    pub weight_group_config: Option<serde_json::Value>,
}
```

**Zod Schema:** `BackendTrainingConfigSchema`
- All numeric ranges extracted from default configurations
- Optional fields properly marked
- Validation messages added for user feedback

### Adapter Name

**Source:** `crates/adapteros-policy/src/packs/naming_policy.rs::NamingPolicy`

```rust
// Format: {tenant}/{domain}/{purpose}/{revision}
// Example: tenant-a/engineering/code-review/r001

// Validation rules:
// 1. Tenant: lowercase, alphanumeric, hyphens, underscores, max 50 chars
// 2. Domain: lowercase, alphanumeric, hyphens, underscores, max 50 chars
// 3. Purpose: lowercase, alphanumeric, hyphens, underscores, max 50 chars
// 4. Revision: format rXXX (e.g., r001, r042)
// 5. Reserved tenants: system, admin, root, default, test
// 6. Reserved domains: core, internal, deprecated
// 7. Tenant isolation: adapter tenant must match requesting tenant
// 8. Revision gap: max 5 between consecutive revisions
```

**Zod Schema:** `AdapterNameSchema`
- Regex pattern for full validation
- Component validators for each part
- Reserved namespace checks
- Helper utilities for parsing and manipulation

### Inference Request

**Source:** `crates/adapteros-api/src/streaming.rs::StreamingInferenceRequest`

```rust
pub struct StreamingInferenceRequest {
    pub prompt: String,                 // 1-8192 chars
    pub model: Option<String>,          // 1-100 chars
    pub max_tokens: usize,              // 1-4096 (default: 512)
    pub temperature: f32,               // 0.0-2.0 (default: 0.7)
    pub top_p: Option<f32>,             // 0.0-1.0
    pub stop: Vec<String>,              // 0-10 items
    pub stream: bool,                   // default: true
    pub adapter_stack: Option<String>,  // 1-100 chars
    pub stack_id: Option<String>,
    pub stack_version: Option<i64>,
}
```

**Zod Schema:** `StreamingInferenceRequestSchema`
- All defaults from backend functions
- Range validations from backend limits
- Optional fields properly typed

## Validation Rule Extraction Process

### Step 1: Identify Backend Types

Searched for:
- `struct.*Request` - API request types
- `struct.*Config` - Configuration types
- `struct.*Response` - Response types (for reference)

### Step 2: Extract Validation Logic

Located validation in:
- Direct validation functions (`validate_*()`)
- Type constructors (`new()`, `default()`)
- Policy enforcement (`Policy::enforce()`)
- Struct field attributes (e.g., `#[serde(default = "...")]`)

### Step 3: Extract Constants

Found constants via:
- `const MAX_*` - Maximum limits
- `const MIN_*` - Minimum limits
- `const DEFAULT_*` - Default values
- Function defaults (e.g., `fn default_max_tokens() -> usize { 512 }`)

### Step 4: Map to Zod Schemas

For each backend type:
1. Created corresponding Zod schema
2. Added validation rules with exact limits
3. Documented source in comments
4. Added helpful error messages
5. Created helper utilities where needed

### Step 5: Create Utilities

Added utility functions for:
- Parsing complex types (e.g., `AdapterNameUtils.parse()`)
- Validation helpers (e.g., `InferenceUtils.validatePromptLength()`)
- Formatting (e.g., `ValidationUtils.formatFileSize()`)
- Type checking (e.g., `ValidationUtils.isValidJson()`)

## Validation Coverage

### Training Types: ✅ Complete

- [x] TrainingConfig (all fields validated)
- [x] StartTrainingRequest (semantic naming, references)
- [x] TrainingJobStatus (enum values)
- [x] UploadDatasetRequest (name, description, format)
- [x] ValidateDatasetRequest (dataset_id)

### Adapter Types: ✅ Complete

- [x] AdapterName (semantic naming with all policy rules)
- [x] RegisterAdapterRequest (all fields, BLAKE3 hash)
- [x] AdapterLifecycleState (enum values)
- [x] StackName (format validation)
- [x] CreateAdapterStackRequest (max 8 adapters)
- [x] PinAdapterRequest (TTL, reason)

### Inference Types: ✅ Complete

- [x] InferRequest (prompt, parameters)
- [x] StreamingInferenceRequest (all fields, defaults)
- [x] FinishReason (enum values)
- [x] RouterDecision (observability)
- [x] InferenceTrace (complete trace)

### Common Types: ✅ Complete

- [x] TenantId (lowercase, max 50)
- [x] RepositoryId (owner/repo format)
- [x] CommitSha (7-40 hex)
- [x] Blake3Hash (b3:{64 hex})
- [x] Description (security checks)
- [x] FilePath (no traversal, relative only)
- [x] Pagination (page, limit, sort)
- [x] Timestamp (RFC3339)
- [x] Email, UUID, URL (standard formats)
- [x] Percentage (0-100)
- [x] ChunkSize (1-100 MB)
- [x] FileSize (max 100 MB)
- [x] BatchSize (max 32)
- [x] Language (supported list)
- [x] ValidationStatus (enum)

## Discrepancies & Resolutions

### 1. Name Naming Conflicts

**Issue:** Frontend has UI-focused `TrainingConfigSchema` in `forms.ts`, backend has API-focused `TrainingConfig` in Rust.

**Resolution:**
- UI form: `TrainingConfigSchema` (exported from `forms.ts`)
- Backend API: `BackendTrainingConfigSchema` (exported from `training.schema.ts`)
- Both available via `index.ts` with clear naming

### 2. Temperature Ranges

**Issue:** Backend allows 0.0-2.0, but high values (>1.5) may produce incoherent output.

**Resolution:**
- Schema allows full range (0.0-2.0) to match backend
- Added `InferenceUtils.validateTemperature()` with suggestions
- Presets use recommended values (0.2-1.2)

### 3. Prompt Length

**Issue:** Backend has no explicit max, but token limit is 4096 total (prompt + generation).

**Resolution:**
- Schema sets max prompt length to 8192 chars (~2048 tokens)
- Added `InferenceUtils.getRecommendedMaxTokens()` to calculate based on prompt
- Added `InferenceUtils.estimateTokenCount()` for rough estimation

### 4. Reserved Namespaces

**Issue:** Backend reserves namespaces but list may grow.

**Resolution:**
- Hardcoded core reserved names (system, admin, root, default, test, core, internal, deprecated)
- Added note in documentation that list may expand
- Policy can add additional reserved names via configuration

## Testing Validation Schemas

### Manual Testing Checklist

Training:
- [x] Valid training config with all required fields
- [x] Training config with optional fields
- [x] Invalid rank (below min, above max)
- [x] Invalid learning rate (negative, above 1.0)
- [x] Invalid adapter name format
- [x] Missing required targets array

Adapters:
- [x] Valid semantic adapter name
- [x] Reserved tenant names rejected
- [x] Reserved domain names rejected
- [x] Invalid revision format
- [x] Uppercase in name rejected
- [x] Revision gap validation
- [x] Valid BLAKE3 hash format
- [x] Invalid hash format rejected

Inference:
- [x] Valid inference request with defaults
- [x] Temperature range validation
- [x] Max tokens validation
- [x] Prompt length validation
- [x] Stop sequences limit

Common:
- [x] Valid tenant ID
- [x] Valid repository ID (owner/repo)
- [x] Valid commit SHA (7-40 hex)
- [x] Directory traversal blocked in file paths
- [x] Absolute paths blocked
- [x] SQL injection patterns detected in descriptions
- [x] XSS patterns detected in descriptions

### Automated Tests

Unit tests should be added for:
1. Each schema's basic validation
2. Boundary conditions (min/max values)
3. Format validations (regex patterns)
4. Helper utilities
5. Error message formatting

Example test structure:
```typescript
describe('AdapterNameSchema', () => {
  describe('valid names', () => {
    it('accepts correct format', () => { /* ... */ });
    it('accepts all allowed characters', () => { /* ... */ });
  });

  describe('invalid names', () => {
    it('rejects reserved tenants', () => { /* ... */ });
    it('rejects reserved domains', () => { /* ... */ });
    it('rejects uppercase', () => { /* ... */ });
    it('rejects invalid revision format', () => { /* ... */ });
  });

  describe('utilities', () => {
    it('parses name components', () => { /* ... */ });
    it('checks same lineage', () => { /* ... */ });
    it('calculates next revision', () => { /* ... */ });
  });
});
```

## Future Enhancements

### 1. Async Validation

Add async validators for:
- Check if adapter name already exists
- Check if repository is accessible
- Verify BLAKE3 hash matches uploaded file

### 2. Custom Error Messages

Add context-aware error messages:
- "Rank must be between 1 and 256. Typical values: 8 (fast), 16 (balanced), 32 (deep)"
- "Temperature controls randomness. Use 0.7 for balanced output, 0.2 for precise, 1.2 for creative"

### 3. Schema Versioning

Track schema versions to handle API changes:
- `v1/training.schema.ts`
- `v2/training.schema.ts`
- Automatic migration helpers

### 4. Dynamic Schema Updates

Fetch validation rules from backend:
- Reserved namespace lists
- Supported languages
- Configuration limits
- Feature flags

### 5. Integration Tests

Test full flow:
- UI form → validation → API call → backend validation
- Ensure frontend validation matches backend exactly
- Test error propagation

## Maintenance

### When to Update Schemas

Update schemas when:
1. Backend Rust types change (new fields, different types)
2. Validation logic changes (new rules, different limits)
3. Constants change (MAX_K, file size limits, etc.)
4. Policy rules change (new reserved namespaces, etc.)

### Update Process

1. Identify changed backend file
2. Extract new validation rules
3. Update corresponding Zod schema
4. Update tests
5. Update documentation (README, QUICK_REFERENCE)
6. Verify with API testing

### Schema Ownership

| Schema Category | Owner | Review Required |
|----------------|-------|-----------------|
| Training schemas | Backend team | Yes |
| Adapter schemas | Core team | Yes |
| Inference schemas | Backend team | Yes |
| Common schemas | All teams | No (general purpose) |

## References

### Backend Source Files

Training:
- `/crates/adapteros-api-types/src/training.rs`
- `/crates/adapteros-types/src/training/mod.rs`
- `/crates/adapteros-lora-worker/src/training/`

Adapters:
- `/crates/adapteros-api-types/src/adapters.rs`
- `/crates/adapteros-policy/src/packs/naming_policy.rs`
- `/crates/adapteros-core/src/naming.rs`

Inference:
- `/crates/adapteros-api-types/src/inference.rs`
- `/crates/adapteros-api/src/streaming.rs`
- `/crates/adapteros-lora-worker/src/inference_pipeline.rs`

Validation:
- `/crates/adapteros-server-api/src/validation.rs`
- `/crates/adapteros-policy/src/packs/`

### Frontend Implementation

Schemas:
- `/ui/src/schemas/training.schema.ts`
- `/ui/src/schemas/adapter.schema.ts`
- `/ui/src/schemas/inference.schema.ts`
- `/ui/src/schemas/common.schema.ts`
- `/ui/src/schemas/forms.ts` (UI-specific)

Documentation:
- `/ui/src/schemas/README.md`
- `/ui/src/schemas/QUICK_REFERENCE.md`
- `/ui/src/schemas/EXTRACTION_SUMMARY.md` (this file)
