# API Validation Schemas

This directory contains validation schemas and type definitions for API responses.

## Structure

### Strict Type Definitions (Manual Type Guards)
- `adapter.schema.ts` - Strict adapter type definitions with manual type guards
- `stack.schema.ts` - Strict stack type definitions with manual type guards

### Zod Validation Schemas
- `adapter.zod.ts` - Zod schemas for adapter types and responses
- `stack.zod.ts` - Zod schemas for stack types and responses
- `inference.zod.ts` - Zod schemas for inference requests and responses

### Helpers
- `validation.ts` - Safe validation helpers for API responses
- `index.ts` - Barrel exports for all schemas and helpers

## Usage

### Basic Validation

```typescript
import { AdapterSchema, safeParseApiResponse } from '@/api/schemas';

// Safe parsing (returns null on error, logs to console)
const adapter = safeParseApiResponse(AdapterSchema, apiData, 'GET /api/adapters/:id');

if (adapter) {
  // TypeScript knows this is a valid Adapter
  console.log(adapter.adapter_id);
}
```

### Strict Validation (Throws on Error)

```typescript
import { InferResponseSchema, parseApiResponse } from '@/api/schemas';

try {
  const response = parseApiResponse(InferResponseSchema, apiData, 'POST /api/infer');
  console.log(response.text);
} catch (error) {
  // Handle validation error
  console.error('Invalid API response:', error);
}
```

### Validating Arrays

```typescript
import { AdapterSummarySchema, safeParseApiArray } from '@/api/schemas';

// Filters out invalid items and logs errors
const adapters = safeParseApiArray(
  AdapterSummarySchema,
  apiData.adapters,
  'GET /api/adapters'
);
```

### Type Inference

All Zod schemas export inferred TypeScript types:

```typescript
import type { Adapter, InferResponse, AdapterStack } from '@/api/schemas';

// These types are inferred from Zod schemas
const adapter: Adapter = { /* ... */ };
const response: InferResponse = { /* ... */ };
const stack: AdapterStack = { /* ... */ };
```

## Schema Features

### Passthrough Mode

All schemas use `.passthrough()` to allow unknown fields from the API. This ensures forward compatibility when the backend adds new fields.

```typescript
// Schema allows unknown fields
export const AdapterSchema = z.object({
  adapter_id: z.string(),
  name: z.string(),
  // ... other fields
}).passthrough(); // ← Allows extra fields
```

### Optional vs Nullable

- Use `.optional()` for fields that may not exist
- Use `.nullable()` for fields that can be explicitly null
- Use `.optional().nullable()` for both

```typescript
z.object({
  description: z.string().optional(),        // May not exist
  parent_id: z.string().nullable(),          // Can be null
  metadata: z.string().optional().nullable() // May not exist or be null
})
```

## Validation Helpers

### `safeParseApiResponse<T>(schema, data, context?)`

Safely validates API response data. Logs errors but doesn't throw.

- **Returns**: Parsed data if valid, `null` if invalid
- **Use when**: You want graceful degradation on validation errors

### `parseApiResponse<T>(schema, data, context?)`

Strictly validates API response data. Throws on error.

- **Returns**: Parsed data
- **Throws**: ZodError if validation fails
- **Use when**: You need strict validation and want to handle errors explicitly

### `safeParseApiArray<T>(schema, data, context?)`

Validates an array of items. Invalid items are filtered out.

- **Returns**: Array of valid items (invalid items removed)
- **Use when**: You want to process partial data even if some items are invalid

## Examples

### Adapter Validation

```typescript
import {
  AdapterSchema,
  AdapterResponseSchema,
  ListAdaptersResponseSchema,
  safeParseApiResponse
} from '@/api/schemas';

// Single adapter
const adapter = safeParseApiResponse(
  AdapterSchema,
  data,
  'GET /api/adapters/:id'
);

// Adapter response with schema_version
const adapterResponse = safeParseApiResponse(
  AdapterResponseSchema,
  data,
  'GET /api/adapters/:id'
);

// List response
const listResponse = safeParseApiResponse(
  ListAdaptersResponseSchema,
  data,
  'GET /api/adapters'
);
```

### Stack Validation

```typescript
import { AdapterStackSchema, PolicyPreflightResponseSchema } from '@/api/schemas';

// Stack
const stack = safeParseApiResponse(AdapterStackSchema, data, 'GET /api/stacks/:id');

// Policy preflight
const preflight = safeParseApiResponse(
  PolicyPreflightResponseSchema,
  data,
  'POST /api/stacks/preflight'
);
```

### Inference Validation

```typescript
import { InferRequestSchema, InferResponseSchema, RunReceiptSchema } from '@/api/schemas';

// Request validation (before sending)
const validRequest = parseApiResponse(InferRequestSchema, requestData);

// Response validation (after receiving)
const response = safeParseApiResponse(InferResponseSchema, data, 'POST /api/infer');

if (response?.run_receipt) {
  // Receipt is automatically validated as part of InferResponse
  console.log(response.run_receipt.trace_id);
}
```

## Migration from Manual Type Guards

The Zod schemas complement the existing manual type guards. You can use both:

```typescript
// Old approach (still valid)
import { assertAdapter } from '@/api/schemas';
assertAdapter(data); // Throws if invalid
const adapter = data as Adapter;

// New approach (recommended)
import { AdapterSchema, parseApiResponse } from '@/api/schemas';
const adapter = parseApiResponse(AdapterSchema, data);
```

Benefits of Zod schemas:
- Runtime validation with detailed error messages
- Automatic type inference
- Better error handling (safe parsing)
- More maintainable (single source of truth)
