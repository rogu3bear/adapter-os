# Validation Schemas

This directory contains Zod validation schemas for type-safe form validation and API request validation in the AdapterOS UI.

## Overview

The schemas are organized into two categories:

1. **Form Schemas** (`forms.ts`) - UI-specific validation for complex multi-step forms
2. **Backend-Mapped Schemas** - Direct mappings to Rust backend types for API validation

## Architecture

```
schemas/
├── index.ts              # Central export point
├── forms.ts              # UI form schemas (TrainingWizard, DatasetBuilder, etc.)
├── training.schema.ts    # Backend training types
├── adapter.schema.ts     # Backend adapter types
├── inference.schema.ts   # Backend inference types
├── common.schema.ts      # Common validation schemas
├── utils.ts              # Validation utilities
└── README.md             # This file
```

## Usage Examples

### 1. Form Validation (UI Components)

Use the form schemas for UI components like TrainingWizard:

```typescript
import { TrainingConfigSchema } from '@/schemas';

// In a form component
const form = useForm({
  resolver: zodResolver(TrainingConfigSchema),
  defaultValues: {
    name: '',
    category: 'code',
    rank: 16,
    alpha: 32,
    // ...
  },
});

// Validation happens automatically
const onSubmit = (data: TrainingConfigFormData) => {
  // data is type-safe and validated
  console.log(data);
};
```

### 2. API Request Validation (Backend Calls)

Use backend-mapped schemas for API calls:

```typescript
import { StartTrainingRequestSchema, BackendTrainingConfig } from '@/schemas';

// Validate before API call
const requestData = {
  adapter_name: 'tenant-a/engineering/code-review/r001',
  config: {
    rank: 16,
    alpha: 32,
    targets: ['q_proj', 'v_proj'],
    epochs: 3,
    learning_rate: 0.001,
    batch_size: 32,
  },
};

const result = StartTrainingRequestSchema.safeParse(requestData);

if (!result.success) {
  // Handle validation errors
  console.error(result.error.flatten());
  return;
}

// Make API call with validated data
const response = await fetch('/api/training/start', {
  method: 'POST',
  body: JSON.stringify(result.data),
});
```

### 3. Programmatic Validation

Validate individual fields or objects:

```typescript
import { AdapterNameSchema, AdapterNameUtils } from '@/schemas';

// Validate adapter name
const result = AdapterNameSchema.safeParse('tenant-a/engineering/code-review/r001');

if (!result.success) {
  console.error(result.error.issues);
} else {
  // Parse the name
  const parsed = AdapterNameUtils.parse(result.data);
  console.log(parsed.tenant); // 'tenant-a'
  console.log(parsed.domain); // 'engineering'
  console.log(parsed.revisionNumber); // 1
}
```

### 4. Error Handling

Format validation errors for display:

```typescript
import { formatValidationError, parseValidationErrors } from '@/schemas';

const result = TrainingConfigSchema.safeParse(formData);

if (!result.success) {
  // Get all errors
  const errors = parseValidationErrors(result.error);

  // Format for display
  errors.forEach(error => {
    const message = formatValidationError(error.path, error.message);
    toast.error(message);
  });
}
```

## Schema Categories

### Training Schemas

**Backend Types:**
- `BackendTrainingConfigSchema` - Training hyperparameters (matches `TrainingConfig` in Rust)
- `StartTrainingRequestSchema` - Start training API request
- `TrainingJobStatusSchema` - Job status enum (pending, running, completed, etc.)
- `UploadDatasetRequestSchema` - Dataset upload
- `ValidateDatasetRequestSchema` - Dataset validation

**UI Form:**
- `TrainingConfigSchema` - Complete wizard form validation (includes all UI fields)

**Presets:**
- `TrainingTemplates.quick` - Fast training (rank 8, 1 epoch)
- `TrainingTemplates.standard` - Balanced (rank 16, 3 epochs)
- `TrainingTemplates.deep` - Comprehensive (rank 32, 5 epochs)

### Adapter Schemas

**Core Schemas:**
- `AdapterNameSchema` - Semantic naming validation (`tenant/domain/purpose/rXXX`)
- `RegisterAdapterRequestSchema` - Adapter registration
- `AdapterLifecycleStateSchema` - State machine (unloaded, cold, warm, hot, resident)
- `StackNameSchema` - Stack naming (`stack.namespace`)
- `CreateAdapterStackRequestSchema` - Stack creation
- `PinAdapterRequestSchema` - Pin adapters (prevent eviction)

**Utilities:**
- `AdapterNameUtils.parse()` - Parse adapter name into components
- `AdapterNameUtils.isSameLineage()` - Check if adapters share lineage
- `AdapterNameUtils.nextRevision()` - Get next revision number
- `AdapterNameUtils.validateRevisionGap()` - Check revision gap (max 5)

**Constants:**
- `SupportedLanguages` - Allowed programming languages
- `ReservedTenants` - Reserved tenant names (system, admin, root, default, test)
- `ReservedDomains` - Reserved domain names (core, internal, deprecated)

### Inference Schemas

**Backend Types:**
- `BackendInferRequestSchema` - Inference request (matches `InferRequest` in Rust)
- `StreamingInferenceRequestSchema` - Streaming inference
- `FinishReasonSchema` - Completion reason enum
- `RouterDecisionSchema` - Router observability
- `InferenceTraceSchema` - Complete inference trace

**UI Form:**
- `InferenceRequestSchema` - Playground form validation

**Presets:**
- `InferencePresets.creative` - High temperature (1.2) for creative writing
- `InferencePresets.balanced` - Standard settings (temp 0.7)
- `InferencePresets.precise` - Low temperature (0.2) for factual responses
- `InferencePresets.deterministic` - Zero temperature with seed

**Utilities:**
- `InferenceUtils.estimateTokenCount()` - Rough token estimation
- `InferenceUtils.validatePromptLength()` - Check prompt length
- `InferenceUtils.getRecommendedMaxTokens()` - Calculate max tokens
- `InferenceUtils.validateTemperature()` - Validate temperature range

### Common Schemas

Reusable validation schemas:

**Identifiers:**
- `TenantIdSchema` - Tenant validation (lowercase, 50 char max)
- `RepositoryIdSchema` - Repository format (`owner/repo`)
- `CommitShaSchema` - Git commit SHA (7-40 hex chars)
- `Blake3HashSchema` - BLAKE3 hash format (`b3:{64 hex}`)
- `UuidSchema` - UUID validation
- `EmailSchema` - Email validation

**Text:**
- `DescriptionSchema` - Description with security checks (no SQL injection, XSS)
- `FilePathSchema` - Secure file paths (no directory traversal, no absolute paths)
- `TimestampSchema` - RFC3339 timestamp validation

**Numbers:**
- `PercentageSchema` - Percentage (0-100)
- `ChunkSizeSchema` - File upload chunks (1-100 MB)
- `FileSizeSchema` - File size limits (max 100 MB)
- `BatchSizeSchema` - Batch operations (max 32)

**Pagination:**
- `PaginationSchema` - Page, limit, sort_by, sort_order

**Utilities:**
- `ValidationUtils.isValidJson()` - JSON validation
- `ValidationUtils.formatFileSize()` - Human-readable file sizes
- `ValidationUtils.sanitizeString()` - XSS prevention
- `ValidationUtils.isValidTimestamp()` - Timestamp validation
- `ValidationUtils.getRelativeTime()` - Relative time strings ("5m ago")

## Validation Rules Reference

### Adapter Naming Policy

**Format:** `{tenant}/{domain}/{purpose}/{revision}`

**Rules:**
1. All components must be lowercase with hyphens/underscores
2. Tenant cannot be reserved (system, admin, root, default, test)
3. Domain cannot be reserved (core, internal, deprecated)
4. Revision must be `rXXX` format (e.g., r001, r042)
5. Maximum revision gap: 5 (enforced by policy)
6. Names must match requesting tenant (isolation)

**Examples:**
- ✅ `tenant-a/engineering/code-review/r001`
- ✅ `shop-floor/hydraulics/troubleshooting/r003`
- ❌ `system/core/test/r001` (reserved tenant and domain)
- ❌ `Tenant-A/Engineering/Review/r1` (uppercase, wrong revision format)

### Training Configuration Limits

| Parameter | Min | Max | Default | Notes |
|-----------|-----|-----|---------|-------|
| `rank` | 1 | 256 | 16 | LoRA rank dimension |
| `alpha` | 1 | 512 | 32 | Typically 2x rank |
| `epochs` | 1 | 1000 | 3 | Training epochs |
| `learning_rate` | >0 | 1.0 | 0.001 | Must be positive |
| `batch_size` | 1 | 512 | 32 | Batch size |
| `warmup_steps` | 0 | 10000 | 100 | Optional |
| `max_seq_length` | 128 | 8192 | 2048 | Optional |
| `gradient_accumulation_steps` | 1 | 64 | 4 | Optional |

### Inference Configuration Limits

| Parameter | Min | Max | Default | Notes |
|-----------|-----|-----|---------|-------|
| `max_tokens` | 1 | 4096 | 512 | Tokens to generate |
| `temperature` | 0.0 | 2.0 | 0.7 | Sampling temperature |
| `top_k` | 1 | 100 | 50 | Top-k sampling |
| `top_p` | 0.0 | 1.0 | 0.9 | Nucleus sampling |
| `prompt` | 1 char | 8192 chars | - | Input text |

### String Length Limits

| Field | Max Length | Notes |
|-------|------------|-------|
| Tenant ID | 50 | Lowercase, alphanumeric, hyphens, underscores |
| Adapter name | 200 | Full semantic name |
| Repository ID | 100 | Format: `owner/repo` |
| Description | 5000 | Security checks applied |
| File path | 500 | Relative paths only |
| Commit SHA | 40 | 7-40 hex characters |

## Backend Type Mappings

### Training Types

| Zod Schema | Rust Type | Crate |
|------------|-----------|-------|
| `BackendTrainingConfigSchema` | `TrainingConfig` | `adapteros-types/src/training/mod.rs` |
| `StartTrainingRequestSchema` | `StartTrainingRequest` | `adapteros-api-types/src/training.rs` |
| `TrainingJobStatusSchema` | `TrainingJobStatus` | `adapteros-types/src/training/mod.rs` |
| `UploadDatasetRequestSchema` | `UploadDatasetRequest` | `adapteros-api-types/src/training.rs` |

### Adapter Types

| Zod Schema | Rust Type | Crate |
|------------|-----------|-------|
| `AdapterNameSchema` | `AdapterName` | `adapteros-core/src/naming.rs` |
| `RegisterAdapterRequestSchema` | `RegisterAdapterRequest` | `adapteros-api-types/src/adapters.rs` |
| `AdapterLifecycleStateSchema` | `LifecycleState` | `adapteros-lora-lifecycle` |
| `StackNameSchema` | `StackName` | `adapteros-core/src/naming.rs` |

### Inference Types

| Zod Schema | Rust Type | Crate |
|------------|-----------|-------|
| `BackendInferRequestSchema` | `InferRequest` | `adapteros-api-types/src/inference.rs` |
| `StreamingInferenceRequestSchema` | `StreamingInferenceRequest` | `adapteros-api/src/streaming.rs` |
| `RouterDecisionSchema` | `RouterDecision` | `adapteros-api-types/src/inference.rs` |
| `InferenceTraceSchema` | `InferenceTrace` | `adapteros-api-types/src/inference.rs` |

### Common Types

| Zod Schema | Rust Validation Function | Source |
|------------|-------------------------|--------|
| `TenantIdSchema` | `validate_tenant_id()` | `adapteros-server-api/src/validation.rs` |
| `RepositoryIdSchema` | `validate_repo_id()` | `adapteros-server-api/src/validation.rs` |
| `CommitShaSchema` | `validate_commit_sha()` | `adapteros-server-api/src/validation.rs` |
| `Blake3HashSchema` | `validate_hash_b3()` | `adapteros-server-api/src/validation.rs` |
| `DescriptionSchema` | `validate_description()` | `adapteros-server-api/src/validation.rs` |
| `FilePathSchema` | `validate_file_paths()` | `adapteros-server-api/src/validation.rs` |

## Integration with Forms

### React Hook Form Integration

```typescript
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { TrainingConfigSchema } from '@/schemas';

function TrainingForm() {
  const form = useForm({
    resolver: zodResolver(TrainingConfigSchema),
    defaultValues: {
      name: '',
      category: 'code',
      // ...
    },
  });

  return (
    <form onSubmit={form.handleSubmit(onSubmit)}>
      <input {...form.register('name')} />
      {form.formState.errors.name && (
        <span>{form.formState.errors.name.message}</span>
      )}
    </form>
  );
}
```

### Manual Validation

```typescript
import { AdapterNameSchema } from '@/schemas';

function validateAdapterName(name: string) {
  const result = AdapterNameSchema.safeParse(name);

  if (!result.success) {
    return {
      valid: false,
      errors: result.error.issues.map(issue => issue.message),
    };
  }

  return {
    valid: true,
    data: result.data,
  };
}
```

## Testing

### Unit Testing Schemas

```typescript
import { describe, it, expect } from 'vitest';
import { AdapterNameSchema } from '@/schemas';

describe('AdapterNameSchema', () => {
  it('should validate correct adapter names', () => {
    const result = AdapterNameSchema.safeParse('tenant-a/engineering/code-review/r001');
    expect(result.success).toBe(true);
  });

  it('should reject reserved tenants', () => {
    const result = AdapterNameSchema.safeParse('system/engineering/code-review/r001');
    expect(result.success).toBe(false);
  });

  it('should reject uppercase names', () => {
    const result = AdapterNameSchema.safeParse('Tenant-A/Engineering/Review/r001');
    expect(result.success).toBe(false);
  });
});
```

## Best Practices

1. **Always validate before API calls** - Use backend-mapped schemas to catch errors early
2. **Use type inference** - Let TypeScript infer types from schemas (`z.infer<typeof Schema>`)
3. **Handle validation errors gracefully** - Display helpful error messages to users
4. **Reuse common schemas** - Don't duplicate validation logic
5. **Test schema edge cases** - Write tests for validation rules
6. **Keep schemas in sync** - Update when backend validation changes

## Contributing

When adding new schemas:

1. Map backend Rust types exactly (check field names, types, constraints)
2. Document validation rules in comments
3. Add helper utilities if needed
4. Export from `index.ts`
5. Update this README
6. Write tests

## References

- **Backend Types:** `/crates/adapteros-api-types/`
- **Backend Validation:** `/crates/adapteros-server-api/src/validation.rs`
- **Backend Policies:** `/crates/adapteros-policy/src/packs/`
- **Zod Documentation:** https://zod.dev/
