# API Type Migration Guide

## Overview

This guide documents the migration from manually maintained API types to OpenAPI-generated types with domain-specific wrappers.

### Why We're Migrating

**Problem**: Manual type maintenance leads to:
- Type drift between backend (Rust) and frontend (TypeScript)
- Missing fields when backend APIs evolve
- Inconsistent snake_case/camelCase handling
- Duplicated type definitions across files
- Runtime errors from type mismatches

**Solution**: Generate types from backend OpenAPI specification, then wrap with domain-specific camelCase types.

### Benefits

- **Type Safety**: Compile-time errors catch API contract violations
- **Reduced Drift**: Types auto-update when backend changes
- **Less Manual Work**: No hand-writing type definitions
- **Single Source of Truth**: Backend OpenAPI spec drives frontend types
- **Consistent Transforms**: Standardized snake_case → camelCase conversion

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    BACKEND (Rust)                               │
│  adapteros-server-api/src/handlers/*                            │
│  ↓                                                               │
│  OpenAPI Specification (auto-generated)                         │
└─────────────────────────────────────────────────────────────────┘
                            ↓
                   Code Generation
                   (openapi-typescript)
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│              GENERATED TYPES (snake_case)                       │
│  ui/src/api/generated/index.ts                                  │
│  ⚠️  DO NOT EDIT - Auto-generated from OpenAPI                 │
│                                                                  │
│  type components = {                                            │
│    schemas: {                                                   │
│      Adapter: {                                                 │
│        adapter_id: string;        // snake_case                 │
│        tenant_id: string;                                       │
│        created_at: string;                                      │
│        ...                                                      │
│      }                                                          │
│    }                                                            │
│  }                                                              │
└─────────────────────────────────────────────────────────────────┘
                            ↓
                   Transformation Layer
                   (transformers.ts)
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│              DOMAIN TYPES (camelCase)                           │
│  ui/src/api/domain-types.ts                                     │
│                                                                  │
│  export interface Adapter {                                     │
│    adapterId: string;             // camelCase                  │
│    tenantId: string;                                            │
│    createdAt: string;                                           │
│    ...                                                          │
│  }                                                              │
│                                                                  │
│  ✅ Use these types in UI components                           │
└─────────────────────────────────────────────────────────────────┘
                            ↓
                   React Components
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│                    UI LAYER                                     │
│  Components, hooks, state management                            │
│  All properties in camelCase for consistency                    │
└─────────────────────────────────────────────────────────────────┘
```

### Data Flow

**Incoming (Backend → Frontend)**:
```typescript
Backend Response (snake_case)
  ↓ toCamelCase()
Domain Type (camelCase)
  ↓
React Component
```

**Outgoing (Frontend → Backend)**:
```typescript
Form Data (camelCase)
  ↓ toSnakeCase()
API Request (snake_case)
  ↓
Backend Handler
```

## Type Categories

### 1. Generated API Types (`@/api/generated`)

**Location**: `ui/src/api/generated/index.ts`

**Purpose**: Direct TypeScript representation of OpenAPI specification

**Characteristics**:
- Auto-generated from backend OpenAPI spec
- Uses snake_case (matches Rust/backend conventions)
- **DO NOT EDIT** - Changes will be overwritten
- Updated via `pnpm generate:types` or `make generate-sdks`

**Usage**: API client layer only, never directly in components

```typescript
// INTERNAL USE ONLY (inside api/client.ts)
import type { components } from '@/api/generated';

type BackendAdapter = components['schemas']['Adapter'];
// { adapter_id: string; tenant_id: string; ... }
```

### 2. Domain Types (`@/api/domain-types`)

**Location**: `ui/src/api/domain-types.ts`

**Purpose**: Frontend-friendly wrappers with camelCase properties

**Characteristics**:
- Manually maintained (for now)
- Uses camelCase (matches JavaScript/TypeScript conventions)
- Extends generated types with UI-specific fields when needed
- **PRIMARY INTERFACE** for UI components

**Usage**: Import and use everywhere in the UI

```typescript
// CORRECT: Use in components
import type { Adapter, InferResponse, ChatSession } from '@/api/domain-types';

interface AdapterCardProps {
  adapter: Adapter;  // ✅ camelCase
  onSelect: (id: string) => void;
}
```

### 3. UI Types (`@/types/*`)

**Location**: `ui/src/types/`

**Purpose**: Component-specific types, state, forms, UI logic

**Characteristics**:
- Not tied to backend API contracts
- Component props, local state, form schemas
- UI-only concerns (modals, tabs, filters, etc.)

**Usage**: Component-specific interfaces

```typescript
// ui/src/types/components.ts
export interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
  title?: string;
}

// ui/src/types/forms.ts
export interface TrainingFormData {
  datasetName: string;
  learningRate: number;
  epochs: number;
}
```

### 4. Legacy Types (Being Migrated)

**Locations**:
- `ui/src/api/api-types.ts`
- `ui/src/api/streaming-types.ts`
- `ui/src/types/*.ts` (mixed usage)

**Status**: Being replaced with domain types

**Migration Path**: See "How to Migrate" section below

## How to Use

### For New Features

```typescript
// 1. Import domain types for API data
import type { Adapter, InferResponse } from '@/api/domain-types';

// 2. Use API client with transformers
import { apiClient } from '@/api/client';
import { toCamelCase } from '@/api/transformers';

// 3. Fetch and transform
const fetchAdapter = async (id: string): Promise<Adapter> => {
  const response = await apiClient.get(`/api/adapters/${id}`);
  return toCamelCase(response.data);  // snake_case → camelCase
};

// 4. Use in component
const AdapterDetail: React.FC<{ adapterId: string }> = ({ adapterId }) => {
  const { data: adapter } = useQuery({
    queryKey: ['adapter', adapterId],
    queryFn: () => fetchAdapter(adapterId),
  });

  return <div>{adapter?.name}</div>;
};
```

### Transformation Best Practices

```typescript
// ✅ CORRECT: Transform at API boundary
export const getAdapters = async (): Promise<Adapter[]> => {
  const response = await apiClient.get('/api/adapters');
  return response.data.map(toCamelCase);  // Transform immediately
};

// ✅ CORRECT: Transform request payload
export const createAdapter = async (data: CreateAdapterRequest) => {
  const payload = toSnakeCase(data);  // camelCase → snake_case
  const response = await apiClient.post('/api/adapters', payload);
  return toCamelCase(response.data);
};

// ❌ WRONG: Using generated types in components
import type { components } from '@/api/generated';
type BackendAdapter = components['schemas']['Adapter'];

const MyComponent: React.FC<{ adapter: BackendAdapter }> = ({ adapter }) => {
  return <div>{adapter.adapter_id}</div>;  // ❌ snake_case in UI
};

// ❌ WRONG: Manual transformation in components
const MyComponent: React.FC<{ adapter: any }> = ({ adapter }) => {
  const id = adapter.adapter_id || adapter.adapterId;  // ❌ Fragile
  return <div>{id}</div>;
};
```

### Type Narrowing and Validation

```typescript
// Use type guards for runtime validation
function isAdapter(obj: unknown): obj is Adapter {
  return (
    typeof obj === 'object' &&
    obj !== null &&
    'adapterId' in obj &&
    'tenantId' in obj
  );
}

// Use in API error handling
try {
  const response = await apiClient.get('/api/adapters/123');
  const adapter = toCamelCase(response.data);

  if (!isAdapter(adapter)) {
    throw new Error('Invalid adapter response');
  }

  return adapter;
} catch (error) {
  console.error('Failed to fetch adapter:', error);
  throw error;
}
```

## Migration Checklist

### Step-by-Step Component Migration

**1. Identify Types to Replace**

```typescript
// BEFORE (Legacy)
interface Adapter {
  adapter_id: string;  // Manual definition, snake_case
  name: string;
  tenant_id: string;
}
```

**2. Check Domain Types**

```typescript
// ui/src/api/domain-types.ts
export interface Adapter {
  adapterId: string;  // ✅ Already defined
  name: string;
  tenantId: string;
}
```

If missing, add to domain-types.ts based on generated schema.

**3. Update Imports**

```typescript
// BEFORE
import type { Adapter } from '@/api/api-types';

// AFTER
import type { Adapter } from '@/api/domain-types';
```

**4. Update API Calls**

```typescript
// BEFORE
const fetchAdapter = async (id: string) => {
  const response = await fetch(`/api/adapters/${id}`);
  return response.json();  // No transformation
};

// AFTER
import { toCamelCase } from '@/api/transformers';

const fetchAdapter = async (id: string): Promise<Adapter> => {
  const response = await apiClient.get(`/api/adapters/${id}`);
  return toCamelCase(response.data);  // ✅ Transform
};
```

**5. Update Component Props**

```typescript
// BEFORE
interface Props {
  adapter: any;  // or manual type
}

// AFTER
import type { Adapter } from '@/api/domain-types';

interface Props {
  adapter: Adapter;  // ✅ Typed
}
```

**6. Update Property Access**

```typescript
// BEFORE
<div>{adapter.adapter_id}</div>

// AFTER
<div>{adapter.adapterId}</div>
```

**7. Run Type Check**

```bash
pnpm type-check
# or
pnpm tsc --noEmit
```

**8. Run Tests**

```bash
pnpm test
```

**9. Manual Testing**

Test the component in dev environment to ensure data flows correctly.

### Migration Example: Complete Flow

**Before**:
```typescript
// components/AdapterCard.tsx
interface Adapter {
  adapter_id: string;
  name: string;
}

const AdapterCard = ({ adapter }: { adapter: Adapter }) => {
  return <div>{adapter.adapter_id}: {adapter.name}</div>;
};

// hooks/useAdapters.ts
const useAdapters = () => {
  return useQuery(['adapters'], async () => {
    const res = await fetch('/api/adapters');
    return res.json();  // Raw backend response
  });
};
```

**After**:
```typescript
// components/AdapterCard.tsx
import type { Adapter } from '@/api/domain-types';

const AdapterCard = ({ adapter }: { adapter: Adapter }) => {
  return <div>{adapter.adapterId}: {adapter.name}</div>;
};

// hooks/useAdapters.ts
import { apiClient } from '@/api/client';
import { toCamelCase } from '@/api/transformers';
import type { Adapter } from '@/api/domain-types';

const useAdapters = () => {
  return useQuery({
    queryKey: ['adapters'],
    queryFn: async (): Promise<Adapter[]> => {
      const response = await apiClient.get('/api/adapters');
      return response.data.map(toCamelCase);
    },
  });
};
```

## Troubleshooting

### Issue: Type Mismatch Errors

**Symptom**:
```typescript
Type '{ adapter_id: string; }' is not assignable to type 'Adapter'.
  Property 'adapterId' is missing
```

**Cause**: Forgot to transform backend response

**Solution**:
```typescript
// Add transformation
import { toCamelCase } from '@/api/transformers';
const adapter = toCamelCase(rawResponse);
```

### Issue: Missing Properties

**Symptom**:
```typescript
Property 'someNewField' does not exist on type 'Adapter'
```

**Cause**: Backend added new field, domain types not updated

**Solution**:
1. Regenerate types: `pnpm generate:types`
2. Check `ui/src/api/generated/index.ts` for new field
3. Update `ui/src/api/domain-types.ts` to include camelCase version
4. Update transformers if needed

### Issue: Runtime `undefined` Values

**Symptom**: Component receives `undefined` for properties that should exist

**Cause**: Property name mismatch (snake_case vs camelCase)

**Debug**:
```typescript
// Add logging to see raw response
console.log('Raw response:', response.data);
console.log('Transformed:', toCamelCase(response.data));

// Check if transformation is applied
const transformed = toCamelCase(response.data);
console.log('adapterId' in transformed);  // Should be true
console.log('adapter_id' in transformed); // Should be false
```

**Solution**: Ensure `toCamelCase()` is called on all API responses

### Issue: Type Generation Fails

**Symptom**:
```bash
pnpm generate:types
Error: Failed to fetch OpenAPI spec
```

**Cause**: Backend not running or OpenAPI endpoint unavailable

**Solution**:
```bash
# 1. Start backend
make dev

# 2. Verify OpenAPI endpoint
curl http://localhost:8080/api/openapi.json

# 3. Regenerate types
pnpm generate:types
```

### Issue: Deep Nested Objects Not Transformed

**Symptom**: Top-level properties are camelCase, nested objects still snake_case

**Cause**: `toCamelCase()` is recursive, but type might not reflect it

**Solution**:
```typescript
// Ensure recursive transformation
import { toCamelCase } from '@/api/transformers';

const data = toCamelCase(response.data);  // Transforms all levels

// If specific nested types needed, define them
interface NestedConfig {
  maxTokens: number;
  temperature: number;
}

interface InferRequest {
  prompt: string;
  config: NestedConfig;  // Explicitly typed
}
```

### Issue: Array Transformations

**Symptom**: Array of objects not all transformed

**Solution**:
```typescript
// ✅ CORRECT: Map over array
const adapters = response.data.map(toCamelCase);

// ❌ WRONG: Transform array itself
const adapters = toCamelCase(response.data);  // Only transforms outer array
```

## Advanced Patterns

### Custom Transformers for Specific Types

```typescript
// ui/src/api/transformers/inference.ts
import { toCamelCase } from '@/api/transformers';
import type { InferResponse } from '@/api/domain-types';

export const transformInferResponse = (raw: any): InferResponse => {
  const base = toCamelCase(raw);

  // Additional custom logic
  return {
    ...base,
    createdAt: new Date(base.createdAt),  // Parse dates
    metadata: {
      ...base.metadata,
      tokenCount: Number(base.metadata.tokenCount),  // Ensure numbers
    },
  };
};
```

### Conditional Fields

```typescript
// Handle optional/conditional fields
interface Adapter {
  adapterId: string;
  name: string;
  description?: string;  // Optional
  metadata?: Record<string, unknown>;  // Optional object
}

// Type guard for checking optional fields
function hasMetadata(adapter: Adapter): adapter is Adapter & { metadata: Record<string, unknown> } {
  return adapter.metadata !== undefined;
}

// Usage
if (hasMetadata(adapter)) {
  console.log(adapter.metadata.version);  // Type-safe
}
```

### Discriminated Unions

```typescript
// For polymorphic responses
type WorkerStatus =
  | { status: 'idle' }
  | { status: 'busy'; requestId: string }
  | { status: 'error'; error: string };

function renderStatus(status: WorkerStatus) {
  switch (status.status) {
    case 'idle':
      return <Badge>Idle</Badge>;
    case 'busy':
      return <Badge>Busy: {status.requestId}</Badge>;
    case 'error':
      return <Badge variant="error">{status.error}</Badge>;
  }
}
```

## Future Improvements

### Automated Domain Type Generation

**Goal**: Auto-generate domain types from OpenAPI with camelCase conversion

**Approach**:
```typescript
// Potential script: scripts/generate-domain-types.ts
// 1. Parse OpenAPI spec
// 2. Generate TypeScript interfaces with camelCase
// 3. Write to domain-types.ts
```

**Benefits**:
- Zero manual maintenance
- Guaranteed sync with backend
- Automatic camelCase conversion

### Runtime Validation

**Goal**: Validate API responses match expected types

**Approach**: Use Zod or similar library
```typescript
import { z } from 'zod';

const AdapterSchema = z.object({
  adapterId: z.string(),
  name: z.string(),
  tenantId: z.string(),
  createdAt: z.string(),
});

const validateAdapter = (data: unknown): Adapter => {
  return AdapterSchema.parse(data);  // Throws if invalid
};
```

### API Client Generator

**Goal**: Generate API client functions from OpenAPI

**Example**:
```typescript
// Auto-generated from OpenAPI operations
export const adaptersApi = {
  list: () => apiClient.get('/api/adapters').then(toCamelCase),
  get: (id: string) => apiClient.get(`/api/adapters/${id}`).then(toCamelCase),
  create: (data: CreateAdapterRequest) =>
    apiClient.post('/api/adapters', toSnakeCase(data)).then(toCamelCase),
};
```

## References

- OpenAPI Specification: `http://localhost:8080/api/openapi.json` (when backend running)
- Generated Types: `ui/src/api/generated/index.ts`
- Domain Types: `ui/src/api/domain-types.ts`
- Transformers: `ui/src/api/transformers/index.ts`
- Backend API Handlers: `crates/adapteros-server-api/src/handlers/`

## Questions?

For issues or clarification:
1. Check this guide first
2. Review existing migrated components for patterns
3. Check TypeScript compiler errors for hints
4. Test with backend running to verify data flow

---

**Last Updated**: 2025-12-19
**Status**: Active migration in progress
