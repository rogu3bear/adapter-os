# Zod Schema Quick Reference

Quick lookup for common validation patterns.

## Import

```typescript
import {
  // Adapter
  AdapterSchema,
  AdapterSummarySchema,
  AdapterResponseSchema,

  // Stack
  AdapterStackSchema,
  PolicyPreflightResponseSchema,

  // Inference
  InferRequestSchema,
  InferResponseSchema,
  RunReceiptSchema,

  // Helpers
  safeParseApiResponse,
  parseApiResponse,
  safeParseApiArray,
} from '@/api/schemas';
```

## Common Patterns

### Safe Parse (Returns null on error)
```typescript
const adapter = safeParseApiResponse(
  AdapterSchema,
  data,
  'GET /api/adapters/:id'
);

if (!adapter) {
  // Handle validation error (already logged)
  return;
}

// Use validated data
console.log(adapter.adapter_id);
```

### Strict Parse (Throws on error)
```typescript
try {
  const adapter = parseApiResponse(
    AdapterResponseSchema,
    data,
    'GET /api/adapters/:id'
  );
  return adapter.adapter;
} catch (error) {
  console.error('Validation failed:', error);
  throw error;
}
```

### Parse Array (Filters invalid)
```typescript
const adapters = safeParseApiArray(
  AdapterSummarySchema,
  data.adapters,
  'GET /api/adapters'
);

// adapters contains only valid items
// invalid items are logged and filtered out
```

## Schema Lookup

### Adapter Schemas

| Schema | Use Case | Fields |
|--------|----------|--------|
| `AdapterSummarySchema` | List views, lightweight data | adapter_id, name, category, state |
| `AdapterSchema` | Full adapter details | 50+ fields including lineage, drift, health |
| `ActiveAdapterSchema` | Stack adapters | adapter_id, gate, priority |
| `AdapterResponseSchema` | GET /api/adapters/:id | { schema_version, adapter } |
| `ListAdaptersResponseSchema` | GET /api/adapters | { adapters[], total, page } |
| `AdapterHealthResponseSchema` | GET /api/adapters/:id/health | health, subcodes, drift_summary |
| `AdapterManifestSchema` | Manifest metadata | version, base_model, rank, alpha |

### Stack Schemas

| Schema | Use Case | Fields |
|--------|----------|--------|
| `AdapterStackSchema` | Stack details | id, name, adapters, workflow_type |
| `AdapterStackResponseSchema` | GET /api/stacks/:id | { schema_version, stack } |
| `ListAdapterStacksResponseSchema` | GET /api/stacks | { stacks[], total } |
| `PolicyPreflightResponseSchema` | Preflight checks | checks[], can_proceed |

### Inference Schemas

| Schema | Use Case | Fields |
|--------|----------|--------|
| `InferRequestSchema` | POST /api/infer request | prompt, model, max_tokens, adapters |
| `InferResponseSchema` | POST /api/infer response | text, tokens_generated, run_receipt |
| `RunReceiptSchema` | Receipt data | trace_id, token counts, kv_quota |
| `BatchInferRequestSchema` | Batch request | prompts[], requests[] |
| `BatchInferResponseSchema` | Batch response | results[], total_tokens |

## React Query Integration

### useQuery Hook
```typescript
import { useQuery } from '@tanstack/react-query';
import { AdapterResponseSchema, safeParseApiResponse } from '@/api/schemas';

function useAdapter(id: string) {
  return useQuery({
    queryKey: ['adapter', id],
    queryFn: async () => {
      const res = await fetch(`/api/adapters/${id}`);
      const data = await res.json();

      const validated = safeParseApiResponse(
        AdapterResponseSchema,
        data,
        'useAdapter'
      );

      if (!validated) {
        throw new Error('Invalid adapter response');
      }

      return validated.adapter;
    },
  });
}
```

### useMutation Hook
```typescript
import { useMutation } from '@tanstack/react-query';
import { InferRequestSchema, InferResponseSchema, parseApiResponse } from '@/api/schemas';

function useInference() {
  return useMutation({
    mutationFn: async (request: unknown) => {
      // Validate request
      const validRequest = parseApiResponse(InferRequestSchema, request);

      const res = await fetch('/api/infer', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(validRequest),
      });

      const data = await res.json();

      // Validate response
      return parseApiResponse(InferResponseSchema, data);
    },
  });
}
```

## Type Inference

All schemas export TypeScript types:

```typescript
import type {
  Adapter,
  AdapterSummary,
  AdapterStack,
  InferRequest,
  InferResponse,
  RunReceipt,
} from '@/api/schemas';

// Use as regular TypeScript types
const adapter: Adapter = { /* ... */ };
const request: InferRequest = { prompt: 'Hello' };
```

## Error Handling

### Safe Parse Error
```typescript
const adapter = safeParseApiResponse(AdapterSchema, data);

// Logs to console:
// [API Validation Error] (GET /api/adapters/:id):
// {
//   errors: [
//     { path: ['adapter_id'], message: 'Required' },
//     { path: ['name'], message: 'Required' }
//   ],
//   data: { /* ... */ }
// }
```

### Strict Parse Error
```typescript
try {
  const adapter = parseApiResponse(AdapterSchema, data);
} catch (error) {
  if (error instanceof z.ZodError) {
    // Access detailed errors
    error.issues.forEach(issue => {
      console.log(`${issue.path.join('.')}: ${issue.message}`);
    });
  }
}
```

## Common Enum Values

### AdapterCategory
```typescript
'code' | 'framework' | 'codebase' | 'ephemeral'
```

### AdapterState
```typescript
'unloaded' | 'loading' | 'cold' | 'warm' | 'hot' | 'resident' | 'error'
```

### BackendName
```typescript
'mlx' | 'metal' | 'coreml' | 'cpu'
```

### AttachMode
```typescript
'free' | 'requires_dataset'
```

### StopReasonCode
```typescript
'end_of_sequence' | 'max_tokens' | 'stop_sequence' | 'safety' | 'error' | 'user_abort' | 'budget' | 'repetition'
```

## Field Patterns

### Optional Fields
```typescript
z.string().optional()        // May not exist in response
z.number().optional()        // May not exist in response
```

### Nullable Fields
```typescript
z.string().nullable()        // Can be explicitly null
z.boolean().nullable()       // Can be explicitly null
```

### Both Optional and Nullable
```typescript
z.string().optional().nullable()  // May not exist OR be null
```

### Records (Key-Value Maps)
```typescript
z.record(z.string(), z.number())  // { [key: string]: number }
z.record(z.string(), z.unknown()) // { [key: string]: any }
```

### Arrays
```typescript
z.array(z.string())              // string[]
z.array(AdapterSummarySchema)    // AdapterSummary[]
```

### Unions
```typescript
z.union([z.string(), z.number()]) // string | number
z.enum(['a', 'b', 'c'])          // 'a' | 'b' | 'c'
```

## Best Practices

### ✅ DO
- Use `safeParseApiResponse` for normal API calls
- Provide context strings for better error logging
- Use `.passthrough()` for forward compatibility
- Export inferred types from schemas
- Validate at API boundaries (fetch, query hooks)

### ❌ DON'T
- Don't use `as any` casts
- Don't ignore validation errors
- Don't validate internal data (only API boundaries)
- Don't remove `.passthrough()` (breaks forward compat)
- Don't duplicate schemas (import from here)

## Performance

All validation is fast:
- Simple object: <1ms
- Complex nested: 1-2ms
- Array of 100 items: <10ms

Use validation at API boundaries, not in hot loops.

## Debugging

Enable detailed logging:
```typescript
const result = AdapterSchema.safeParse(data);

if (!result.success) {
  console.log('Validation errors:', result.error.issues);
  console.log('Invalid data:', data);
}
```

## Testing

```typescript
import { describe, it, expect } from 'vitest';
import { AdapterSchema } from '@/api/schemas';

describe('Adapter validation', () => {
  it('validates valid adapter', () => {
    const result = AdapterSchema.safeParse({
      id: '1',
      adapter_id: 'test',
      name: 'Test',
      hash_b3: 'abc',
      rank: 8,
      tier: 'persistent',
      created_at: '2025-01-01',
    });

    expect(result.success).toBe(true);
  });

  it('rejects invalid adapter', () => {
    const result = AdapterSchema.safeParse({
      adapter_id: 'test',
      // missing required fields
    });

    expect(result.success).toBe(false);
  });
});
```
