# State Management Types

Comprehensive TypeScript type definitions for all state management patterns in the AdapterOS UI.

## Structure

```
ui/src/types/state/
├── async.ts         # Async operations, retry, cancellation, progress
├── bulk-actions.ts  # Legacy bulk action types
├── filters.ts       # Sort, filter, search, pagination
├── index.ts         # Central export
├── modals.ts        # Modals, dialogs, drawers, panels
├── navigation.ts    # Breadcrumbs, tabs, sidebar, routing
├── selection.ts     # Multi-select, bulk operations
└── ui.ts           # Legacy UI state types
```

## Usage

### Import State Types

```typescript
import type {
  SelectionState,
  FilterState,
  PaginationState,
  ModalState,
  AsyncOperationState,
} from '@/types/state';
```

## Type Categories

### Selection State (`selection.ts`)

Generic selection state for lists and tables with multi-select support.

```typescript
interface SelectionState<T> {
  selectedIds: Set<string>;
  selectedItems: T[];
  lastSelectedId: string | null;
  totalCount?: number;
}

interface SelectionActions<T> {
  toggle: (id: string, item?: T) => void;
  selectAll: (ids: string[], items?: T[]) => void;
  clearSelection: () => void;
  isSelected: (id: string) => boolean;
}
```

**Example:**
```typescript
const selection = useSelection<Adapter>({
  onSelectionChange: (selected) => {
    console.log('Selected adapters:', selected);
  },
});

// Toggle selection
selection.toggle('adapter-123', adapter);

// Select all
selection.selectAll(adapterIds, adapters);
```

### Filter & Sort State (`filters.ts`)

Filtering, sorting, searching, and pagination for data lists.

```typescript
interface FilterState<T> {
  filters: T;
  hasActiveFilters: boolean;
}

interface SortState<T extends string> {
  column: T;
  direction: 'asc' | 'desc';
}

interface PaginationState {
  currentPage: number;
  pageSize: number;
  totalItems: number;
  totalPages: number;
  // ... computed properties
}
```

**Example:**
```typescript
const { filters, sort, pagination } = useFilteredList({
  totalItems: adapters.length,
  pageSize: 20,
});

// Apply filters
filters.updateFilters({ state: 'active', category: 'code' });

// Sort
sort.setSort('name', 'asc');

// Paginate
pagination.goToPage(2);
```

### Modal & Dialog State (`modals.ts`)

Generic modal, dialog, drawer, and panel state management.

```typescript
interface ModalState<T> {
  open: boolean;
  data: T | null;
}

interface DialogState<T> {
  open: boolean;
  data: T | null;
  isPending?: boolean;
}

interface ConfirmationDialogData {
  title: string;
  message: string;
  confirmText?: string;
  severity?: 'info' | 'warning' | 'error' | 'success';
  destructive?: boolean;
}
```

**Example:**
```typescript
const deleteDialog = useDialog<ConfirmationDialogData>();

// Open confirmation dialog
deleteDialog.openDialog({
  title: 'Delete Adapter',
  message: 'This action cannot be undone.',
  confirmText: 'Delete',
  severity: 'error',
  destructive: true,
});

// Handle confirmation
const handleConfirm = async () => {
  await deleteAdapter(adapterId);
  deleteDialog.closeDialog();
};
```

### Async Operation State (`async.ts`)

State management for async operations with loading, error, retry, and cancellation.

```typescript
interface AsyncOperationState<T, E = Error> {
  status: 'idle' | 'pending' | 'success' | 'error';
  data: T | null;
  error: E | null;
  isLoading: boolean;
  isSuccess: boolean;
  isError: boolean;
}

interface RetryState {
  attempt: number;
  maxAttempts: number;
  isRetrying: boolean;
  nextRetryIn?: number;
}

interface ProgressState {
  percentage: number;
  current: number;
  total: number;
  message?: string;
  estimatedTimeRemaining?: number;
}
```

**Example:**
```typescript
const { execute, status, data, error, retry } = useAsyncOperation(
  fetchAdapterDetails,
  {
    maxRetries: 3,
    retryDelay: 1000,
  }
);

// Execute operation
await execute(adapterId);

// Check status
if (status === 'error') {
  retry();
}
```

### Navigation State (`navigation.ts`)

Breadcrumbs, tabs, sidebar, and routing state.

```typescript
interface BreadcrumbState {
  items: BreadcrumbItem[];
  currentLabel: string;
}

interface TabNavigationState {
  tabs: NavigationTab[];
  activeTabId: string;
}

interface SidebarState {
  open: boolean;
  collapsed: boolean;
  activeSection?: string;
  width?: number;
}
```

## Common Patterns

### State + Actions Pattern

Many types separate state from actions:

```typescript
interface SelectionState<T> {
  selectedIds: Set<string>;
  selectedItems: T[];
}

interface SelectionActions<T> {
  toggle: (id: string) => void;
  selectAll: (ids: string[]) => void;
}

interface SelectionStateWithActions<T>
  extends SelectionState<T>,
    SelectionActions<T> {}
```

### Async Status Flow

```
idle → pending → success
                ↓
              error → (retry) → pending
```

### Optimistic Updates

```typescript
interface OptimisticUpdateState<T> {
  originalData: T | null;
  optimisticData: T | null;
  isOptimistic: boolean;
  isConfirmed: boolean;
  isRolledBack: boolean;
}

// Apply optimistic update
state.applyOptimistic(newData);

// Confirm on success
state.confirm();

// Rollback on error
state.rollback();
```

## Type Composition

Combine state types for complex UI patterns:

```typescript
interface AdapterListState {
  selection: SelectionState<Adapter>;
  filters: FilterState<AdapterFilters>;
  sort: SortState<AdapterSortColumn>;
  pagination: PaginationState;
}

interface AdapterListActions {
  selection: SelectionActions<Adapter>;
  filters: FilterActions<AdapterFilters>;
  sort: SortActions<AdapterSortColumn>;
  pagination: PaginationActions;
}
```

## Persisted State

Use `PersistedStateConfig` for localStorage/sessionStorage:

```typescript
interface PersistedStateConfig<T> {
  key: string;
  storage?: 'local' | 'session';
  serialize?: (value: T) => string;
  deserialize?: (value: string) => T;
  version?: number;
}

// Example
const filterState = usePersistedState<AdapterFilters>({
  key: 'adapter-filters',
  storage: 'local',
  version: 1,
});
```

## Feature Flags

Manage feature toggles:

```typescript
interface FeatureFlagManager {
  flags: FeatureFlags;
  isEnabled: (feature: string) => boolean;
  enable: (feature: string) => void;
  toggle: (feature: string) => void;
}

// Usage
const flags = useFeatureFlags();
if (flags.isEnabled('new-router')) {
  // Show new router UI
}
```

## Best Practices

1. **Separate state from actions** for clarity
2. **Use generic types** for reusability
3. **Follow naming conventions**:
   - `*State` - State properties only
   - `*Actions` - Action functions only
   - `*StateWithActions` - Combined state + actions
4. **Include computed properties** (isLoading, hasActiveFilters, etc.)
5. **Use TypeScript discriminated unions** for status/phase enums

## Related Files

- `/ui/src/hooks/` - Hooks that use these state types
- `/ui/src/contexts/` - React Context providers
- `/ui/src/types/forms/` - Form-specific state types
- `/ui/src/services/` - State machines and business logic
