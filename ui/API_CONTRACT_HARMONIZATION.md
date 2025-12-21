# API Contract Harmonization - Implementation Guide

## Overview

This document describes the API contract harmonization system that creates a strict type boundary between backend JSON responses and frontend domain types. This system **eliminates ALL `as any` assertions** and prevents field name ambiguity through runtime validation and transformation.

## Architecture

```
Backend API (snake_case JSON)
         ↓
    [Zod Validation] ← schemas.ts
         ↓
   [Raw Backend Types] ← api-types.ts (RawXxxResponse)
         ↓
   [Transformers] ← transformers.ts
         ↓
   [Domain Types] ← domain-types.ts (camelCase)
         ↓
    Frontend Components
```

## File Structure

### 1. `domain-types.ts` - Clean Frontend Types

**Purpose**: Define canonical domain types used throughout the frontend.

**Characteristics**:
- ✅ camelCase for all field names
- ✅ Single canonical ID field (`id`, not `adapter_id`)
- ✅ Typed enums where appropriate
- ✅ No ambiguous field names (`candidates`, not `candidate_adapters`)
- ✅ Required fields are non-optional

**Example**:
```typescript
export interface Adapter {
  id: string; // Canonical ID (not adapter_id)
  name: string;
  tenantId?: string;
  hashB3: string;
  lifecycleState?: string;
  createdAt: string; // camelCase (not created_at)
}

export interface InferResponse {
  id: string;
  text: string;
  tokensGenerated: number; // Canonical (not tokens_generated)
  latencyMs: number;
  adaptersUsed: string[];
}

export interface RouterDecision {
  requestId: string;
  selectedAdapters: string[];
  candidates: RouterCandidateInfo[]; // Canonical (not candidate_adapters)
}
```

### 2. `schemas.ts` - Zod Validation Schemas

**Purpose**: Runtime validation of backend JSON responses.

**Characteristics**:
- ✅ Validate ALL incoming data
- ✅ Match exact backend field names (snake_case)
- ✅ Provide detailed error context
- ✅ Use strict validation (`.passthrough()` only when necessary)

**Example**:
```typescript
export const rawAdapterSchema = z.object({
  adapter_id: z.string(),
  id: z.string().optional(), // Some responses include both
  name: z.string(),
  tenant_id: z.string().optional(),
  hash_b3: z.string(),
  rank: z.number(),
  tier: z.string(),
  lifecycle_state: z.string().optional(),
  created_at: z.string(),
}).passthrough();

export const rawInferResponseSchema = z.object({
  schema_version: z.string().optional(),
  id: z.string(),
  text: z.string(),
  tokens_generated: z.number(), // Backend uses snake_case
  latency_ms: z.number(),
  adapters_used: z.array(z.string()),
}).passthrough();
```

### 3. `transformers.ts` - Response Transformers

**Purpose**: Convert validated backend responses to clean domain types.

**Characteristics**:
- ✅ Input is already validated by Zod
- ✅ Transform snake_case to camelCase
- ✅ Normalize field names
- ✅ Handle backward compatibility
- ✅ Return strongly-typed domain objects

**Example**:
```typescript
export function transformAdapter(raw: {
  adapter_id?: string;
  id?: string;
  name: string;
  tenant_id?: string;
  created_at: string;
}): Domain.Adapter {
  return {
    id: raw.adapter_id || raw.id || '', // Normalize to single ID
    name: raw.name,
    tenantId: raw.tenant_id, // Transform to camelCase
    createdAt: raw.created_at,
  };
}

export function transformInferResponse(raw: {
  id: string;
  text: string;
  tokens_generated: number;
  latency_ms: number;
  adapters_used: string[];
}): Domain.InferResponse {
  return {
    id: raw.id,
    text: raw.text,
    tokensGenerated: raw.tokens_generated, // Canonical name
    latencyMs: raw.latency_ms,
    adaptersUsed: raw.adapters_used,
  };
}
```

### 4. `validation.ts` - Validation Helpers

**Purpose**: Provide reusable validation and transformation utilities.

**Key Functions**:
- `validateAndTransform()` - Validate + transform in one step
- `validate()` - Validate without transformation
- `validateArray()` - Validate array responses
- `validatePaginated()` - Validate paginated responses

**Example**:
```typescript
import { validateAndTransform } from '@/api/validation';
import { rawAdapterResponseSchema } from '@/api/schemas';
import { transformAdapterResponse } from '@/api/transformers';

const raw = await fetch('/v1/adapters/123').then(r => r.json());
const adapter = validateAndTransform(
  raw,
  rawAdapterResponseSchema,
  transformAdapterResponse,
  { operation: 'getAdapter', endpoint: '/v1/adapters/123' }
);
// adapter is now Domain.Adapter with all camelCase fields
```

### 5. `api-types.ts` - Raw Backend Types (Updated)

**Purpose**: Document raw backend response structures.

**Changes**:
- ✅ Added `RawXxxResponse` types with `Raw` prefix
- ✅ Documented as backend types (snake_case)
- ✅ Legacy types preserved for backward compatibility

**Example**:
```typescript
// Raw backend response (snake_case field names)
export interface RawAdapterResponse {
  schema_version?: string;
  adapter_id?: string;
  id?: string;
  name: string;
  tenant_id?: string;
  created_at: string;
}

// Legacy type (deprecated, use domain-types.ts)
export interface Adapter {
  // ... mixed snake_case/camelCase
}
```

### 6. `client-validated.ts` - Example Implementations

**Purpose**: Demonstrate migration pattern for existing API methods.

**Example Migration**:
```typescript
// BEFORE (unsafe):
async getAdapter(id: string): Promise<Adapter> {
  return this.request<Adapter>(`/v1/adapters/${id}`);
}

// AFTER (type-safe):
async getAdapter(id: string): Promise<Domain.Adapter> {
  const raw = await this.request<unknown>(`/v1/adapters/${id}`);
  return validateAndTransform(
    raw,
    rawAdapterResponseSchema,
    transformAdapterResponse,
    { operation: 'getAdapter', endpoint: `/v1/adapters/${id}` }
  );
}
```

## Key Mappings

### Backend → Frontend Field Names

| Backend (snake_case) | Frontend (camelCase) | Notes |
|---------------------|---------------------|-------|
| `adapter_id` | `id` | Single canonical ID |
| `tokens_generated` | `tokensGenerated` | Consistent with JS conventions |
| `candidate_adapters` | `candidates` | Shorter, clearer name |
| `lifecycle_state` | `lifecycleState` | Direct camelCase conversion |
| `tenant_id` | `tenantId` | Direct camelCase conversion |
| `created_at` | `createdAt` | Direct camelCase conversion |

### Special Cases

1. **Dual ID Fields**: Backend sometimes returns both `adapter_id` and `id`
   - **Solution**: Transformer uses `raw.adapter_id || raw.id || ''`
   - **Frontend**: Always uses `adapter.id`

2. **Candidate Fields**: Backend uses `candidate_adapters` or `candidates`
   - **Solution**: Transformer checks both: `raw.candidate_adapters || raw.candidates`
   - **Frontend**: Always uses `decision.candidates`

3. **ITAR Compliance**: Backend uses `itar_compliant` or `itarCompliant`
   - **Solution**: Transformer handles both: `raw.itar_compliant ?? raw.itarCompliant`
   - **Frontend**: Always uses `tenant.itarCompliant`

## Migration Checklist

### For Each API Method

- [ ] Change return type from backend type to `Domain.XXX`
- [ ] Change `request<T>()` to `request<unknown>()`
- [ ] Add validation with `validateAndTransform()`
- [ ] Test with real API responses
- [ ] Update component usages to use camelCase

### For Each Component

- [ ] Update imports to use `Domain.XXX` types
- [ ] Change field access from snake_case to camelCase
- [ ] Remove defensive fallbacks (`adapter.adapter_id || adapter.id`)
- [ ] Remove type assertions (`as any`, `as unknown as`)
- [ ] Verify TypeScript compilation

## Success Criteria

✅ **Zero `as any` in codebase**
```bash
grep -r "as any" ui/src/api/domain-types.ts ui/src/api/schemas.ts \
  ui/src/api/transformers.ts ui/src/api/validation.ts
# Should return: No matches found
```

✅ **Zero `as unknown as` in API layer**
```bash
grep -r "as unknown as" ui/src/api/domain-types.ts ui/src/api/schemas.ts \
  ui/src/api/transformers.ts ui/src/api/validation.ts
# Should return: No matches found
```

✅ **All API responses validated with Zod**
- Every `client.ts` method uses `validateAndTransform()`
- Schema coverage for all critical endpoints

✅ **Type errors caught at API boundary**
- Invalid backend responses fail validation immediately
- Detailed error context for debugging
- No type errors propagate to components

## Testing Strategy

### Unit Tests

```typescript
import { transformAdapter } from '@/api/transformers';
import { rawAdapterSchema } from '@/api/schemas';

describe('transformAdapter', () => {
  it('transforms adapter_id to id', () => {
    const raw = { adapter_id: '123', name: 'test', /* ... */ };
    const validated = rawAdapterSchema.parse(raw);
    const result = transformAdapter(validated);
    expect(result.id).toBe('123');
    expect(result).not.toHaveProperty('adapter_id');
  });

  it('handles both adapter_id and id fields', () => {
    const raw = { adapter_id: '123', id: '456', name: 'test', /* ... */ };
    const validated = rawAdapterSchema.parse(raw);
    const result = transformAdapter(validated);
    expect(result.id).toBe('123'); // Prefers adapter_id
  });
});
```

### Integration Tests

```typescript
import { getAdapterValidated } from '@/api/client-validated';

describe('getAdapterValidated', () => {
  it('validates and transforms real API response', async () => {
    const mockRequest = vi.fn().mockResolvedValue({
      schema_version: '1.0',
      adapter_id: '123',
      name: 'Test Adapter',
      created_at: '2025-01-01T00:00:00Z',
    });

    const result = await getAdapterValidated(mockRequest, '123');

    expect(result).toEqual({
      id: '123',
      name: 'Test Adapter',
      createdAt: '2025-01-01T00:00:00Z',
    });
  });

  it('throws on invalid response', async () => {
    const mockRequest = vi.fn().mockResolvedValue({
      // Missing required fields
      adapter_id: '123',
    });

    await expect(
      getAdapterValidated(mockRequest, '123')
    ).rejects.toThrow('Invalid API response');
  });
});
```

## Error Handling

### Validation Errors

When validation fails, the system provides detailed context:

```typescript
{
  "code": "RESPONSE_VALIDATION_ERROR",
  "message": "Invalid API response for getAdapter: Required field 'name' is missing",
  "details": {
    "operation": "getAdapter",
    "endpoint": "/v1/adapters/123",
    "zodError": [
      {
        "code": "invalid_type",
        "path": ["name"],
        "message": "Required"
      }
    ]
  }
}
```

### Transformation Errors

Transformers handle missing/null fields gracefully:

```typescript
export function transformAdapter(raw: RawAdapter): Domain.Adapter {
  return {
    id: raw.adapter_id || raw.id || '', // Safe fallback
    name: raw.name,
    tenantId: raw.tenant_id, // undefined if not present
    // ...
  };
}
```

## Performance Considerations

1. **Validation Overhead**: Zod validation adds ~1-2ms per response
   - ✅ Acceptable for production use
   - ✅ Prevents runtime errors that cost 100x more

2. **Memory Usage**: Transformation creates new objects
   - ✅ Minimal impact (typically <1KB per object)
   - ✅ JavaScript GC handles cleanup efficiently

3. **Bundle Size**: Zod adds ~14KB gzipped
   - ✅ Already included in dependency tree
   - ✅ Tree-shaking removes unused schemas

## Next Steps

1. **Phase 1**: Validate critical endpoints (inference, adapters, models)
2. **Phase 2**: Migrate remaining endpoints systematically
3. **Phase 3**: Remove legacy types from `api-types.ts`
4. **Phase 4**: Enable strict TypeScript mode project-wide

## Support

For questions or issues:
- See example implementations in `client-validated.ts`
- Review test cases in `__tests__/api/`
- Check validation errors in browser DevTools console
