# UI Types Directory

This directory contains **UI-specific TypeScript types** for the AdapterOS frontend. These types are separate from backend API types and focus on presentation, state management, and user interactions.

## Directory Structure

```
types/
├── components/          # React component prop types
│   ├── common.ts       # Generic component interfaces (PageProps, CardProps, etc.)
│   └── index.ts        # Barrel export
├── forms/              # Form and input types
│   ├── common.ts       # Form field types, validation interfaces
│   └── index.ts        # Barrel export
├── state/              # UI state management types
│   ├── bulk-actions.ts # Bulk operation state and progress
│   ├── ui.ts           # Theme, modal, toast, drawer states
│   └── index.ts        # Barrel export
├── hooks/              # Custom React hook types
│   ├── common.ts       # Query, mutation, debounce, clipboard hooks
│   └── index.ts        # Barrel export
├── display/            # Data formatting and display types
│   ├── formatting.ts   # Date, number, bytes formatting
│   ├── status.ts       # Status indicators, health checks
│   └── index.ts        # Barrel export
├── common/             # Shared utility types
│   ├── utility.ts      # Generic TypeScript utilities
│   ├── api.ts          # API wrapper types (UI-specific)
│   └── index.ts        # Barrel export
├── index.ts            # Root barrel export
└── README.md           # This file
```

## Purpose and Scope

### What Goes Here

1. **Component Props**: Interfaces for React component properties
2. **UI State**: Types for modals, toasts, drawers, themes, preferences
3. **Form Types**: Form field configs, validation rules, input handlers
4. **Display Formatting**: Types for formatting data for presentation
5. **Hook Interfaces**: Return types and options for custom hooks
6. **Utility Types**: Generic TypeScript helpers and patterns

### What Does NOT Go Here

- **API Types**: Backend API request/response types belong in `api/api-types.ts`
- **Domain Models**: Business logic types are defined by the backend
- **Generated Types**: OpenAPI-generated types live in `api/` directory

## Usage

### Import from Root

```typescript
import {
  PageProps,
  CardProps,
  BulkOperationProgress,
  UseQueryResult
} from '@/types';
```

### Import from Category

```typescript
import { FormFieldProps, SelectOption } from '@/types/forms';
import { StatusIndicatorProps, HealthStatus } from '@/types/display';
```

### Import Specific File

```typescript
import { BulkActionConfig } from '@/types/state/bulk-actions';
```

## Design Principles

### 1. Separation of Concerns

- **UI types** describe presentation and interaction
- **API types** describe data contracts with backend
- Never mix the two in the same file

### 2. Composability

```typescript
// Good: Compose from primitives
export interface TableProps<T> {
  columns: TableColumn<T>[];
  data: T[];
  state: TableState;
}

// Bad: Monolithic type
export interface TableProps {
  columns: any[];
  data: any[];
  sort: string;
  page: number;
  filters: any;
  // ... 50 more fields
}
```

### 3. Generics for Reusability

```typescript
export interface UseQueryResult<TData = unknown, TError = Error> {
  data?: TData;
  error?: TError;
  isLoading: boolean;
}
```

### 4. Optional vs Required

- Use `?` for truly optional fields
- Provide sensible defaults in implementation
- Document required combinations with comments

### 5. Type Safety Over `any`

```typescript
// Good: Type-safe
export interface TableColumn<T = any> {
  accessor: (row: T) => React.ReactNode;
}

// Bad: Loses type information
export interface TableColumn {
  accessor: (row: any) => any;
}
```

## Common Patterns

### Entity Base Types

```typescript
import { EntityBase, TenantEntity } from '@/types/common';

interface MyEntity extends TenantEntity {
  name: string;
  // Inherits: id, createdAt, updatedAt, tenantId
}
```

### State Management

```typescript
import { AsyncOperationState } from '@/types/state';

const [operation, setOperation] = useState<AsyncOperationState>({
  status: 'idle',
  data: null,
  error: null
});
```

### Form Handling

```typescript
import { FormState, FormValidationError } from '@/types/forms';

const formState: FormState<MyFormData> = {
  values: initialValues,
  errors: {},
  touched: {},
  isSubmitting: false,
  isValid: true,
  isDirty: false
};
```

## Type Naming Conventions

| Pattern | Example | Use Case |
|---------|---------|----------|
| `*Props` | `ButtonProps` | React component props |
| `*State` | `ModalState` | State container interfaces |
| `*Config` | `BulkActionConfig` | Configuration objects |
| `*Options` | `UseQueryOptions` | Function/hook options |
| `*Result` | `UseMutationResult` | Hook return values |
| `*Handler` | `FormSubmitHandler` | Callback function types |
| `Use*` | `UseQueryResult` | Hook-specific types |

## Utility Types Reference

### Basic Utilities

```typescript
Nullable<T>           // T | null
Optional<T>           // T | undefined
DeepPartial<T>        // Recursive Partial
DeepReadonly<T>       // Recursive Readonly
Dictionary<T>         // Record<string, T>
ValueOf<T>            // Union of all values in T
```

### Advanced Utilities

```typescript
RequireAtLeastOne<T>  // At least one property required
RequireOnlyOne<T>     // Exactly one property required
PickByValue<T, V>     // Pick properties with value type V
OmitByValue<T, V>     // Omit properties with value type V
AsyncReturnType<T>    // Extract Promise return type
```

## Best Practices

### 1. Document Complex Types

```typescript
/**
 * Bulk operation state tracking
 *
 * Manages selection, progress, and results for operations
 * performed on multiple items simultaneously.
 *
 * @example
 * const state: BulkOperationState<Adapter> = {
 *   selectedItems: [adapter1, adapter2],
 *   isProcessing: true,
 *   progress: { current: 1, total: 2, successCount: 1, failureCount: 0 }
 * };
 */
export interface BulkOperationState<T = any> {
  // ...
}
```

### 2. Use Discriminated Unions

```typescript
type AsyncState<T> =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'success'; data: T }
  | { status: 'error'; error: Error };
```

### 3. Avoid Circular Dependencies

```typescript
// Bad: Circular import
import { ComponentA } from './ComponentA';
export interface ComponentBProps {
  parent: ComponentA;
}

// Good: Use generic or interface
export interface ComponentBProps<T = any> {
  parent: T;
}
```

### 4. Export from Index Files

Each directory should have an `index.ts` that re-exports all types:

```typescript
// types/components/index.ts
export * from './common';
export * from './table';
export * from './forms';
```

## Maintenance

### Adding New Types

1. Identify the correct category (components, forms, state, etc.)
2. Add type to the appropriate file or create a new file
3. Update the category's `index.ts` to export the new type
4. Document complex types with JSDoc comments
5. Add usage examples to this README if needed

### Refactoring Types

1. Check all usages with TypeScript's "Find All References"
2. Update types incrementally, one file at a time
3. Run `pnpm type-check` to verify no breaks
4. Update tests that use the refactored types

### Deprecating Types

1. Mark as deprecated with `@deprecated` JSDoc tag
2. Provide migration path in comment
3. Update this README with migration guide
4. Remove after 2 release cycles

```typescript
/**
 * @deprecated Use NewType instead. Will be removed in v2.0.
 *
 * Migration:
 * ```typescript
 * // Old
 * const x: OldType = { ... };
 *
 * // New
 * const x: NewType = { ... };
 * ```
 */
export interface OldType {
  // ...
}
```

## Related Documentation

- [API Types](../api/api-types.ts) - Backend API types
- [Component Library](../components/README.md) - React components
- [Hooks](../hooks/README.md) - Custom React hooks
- [State Management](../contexts/README.md) - Global state patterns

## Questions?

For questions about type design or organization, see:
- [TypeScript Handbook](https://www.typescriptlang.org/docs/handbook/intro.html)
- [React TypeScript Cheatsheet](https://react-typescript-cheatsheet.netlify.app/)
- AdapterOS Contributing Guide: `/CONTRIBUTING.md`
