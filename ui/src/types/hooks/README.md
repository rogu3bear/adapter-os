# Hook Types

Centralized type definitions for custom React hooks in the AdapterOS UI.

## Overview

This directory contains organized type definitions extracted from hook implementations to improve maintainability, reduce duplication, and enable easier reuse across the codebase.

## Structure

```
ui/src/types/hooks/
├── index.ts              # Central export point for all hook types
├── adapters.ts          # Adapter-related hook types
├── async.ts             # Async operation hook types
├── chat.ts              # Chat-related hook types
├── common.ts            # Common/shared hook types
├── inference.ts         # Inference hook types
├── model-loading.ts     # Model loading hook types
├── realtime.ts          # Realtime data hook types
├── training.ts          # Training hook types
└── ui.ts                # UI utility hook types
```

## Type Files

### `async.ts`
Type definitions for async operation hooks:
- `useAsyncAction` - React Query-compatible action hook
- `useAsyncOperation` - Basic async operation handling
- `useRetry` - Retry logic with exponential backoff
- `useCancellableOperation` - Cancellable async operations

**Key Types:**
- `AsyncActionState<TData>` - State for async actions
- `UseAsyncActionOptions<TData, TVariables>` - Configuration options
- `UseAsyncActionReturn<TData, TVariables>` - Return value
- `RetryConfig` - Retry configuration
- `RetryState<TData>` - Retry state tracking

### `adapters.ts`
Type definitions for adapter management hooks:
- `useAdapterBulkActions` - Bulk operations (load/unload/delete)
- `useAdapterExport` - Adapter export operations
- `useAdapterOperations` - Single adapter operations
- `useAdapterDetail` - Adapter detail fetching
- `useAdapterFilterState` - Filter state management
- `useAdapterDialogs` - Dialog state management

**Key Types:**
- `UseAdapterBulkActionsOptions` - Bulk action configuration
- `BulkOperationProgress` - Progress tracking
- `BulkActionConfirmationState` - Confirmation dialog state
- `UseAdapterBulkActionsReturn` - Hook return value

### `chat.ts`
Type definitions for chat-related hooks:
- `useChatStreaming` - SSE-based chat streaming
- `useChatSessionsApi` - Chat session management
- `useMessages` - Message loading and management
- `useChatSearch` - Chat search functionality
- `useChatRouterDecisions` - Router decision tracking
- `useChatAdapterState` - Active adapter state

**Key Types:**
- `UseChatStreamingOptions` - Streaming configuration
- `UseChatStreamingReturn` - Streaming state and controls
- `UseMessagesOptions` - Message fetching options
- `UseChatSearchOptions` - Search configuration

### `training.ts`
Type definitions for training hooks:
- `useTrainingPreflight` - Pre-training validation
- `useTrainingNotifications` - Training job notifications
- `useBatchedTrainingNotifications` - Batched notifications

**Key Types:**
- `TrainingPreflightResult` - Preflight check results
- `UseTrainingPreflightOptions` - Preflight configuration
- `UseTrainingJobOptions` - Job monitoring options

### `ui.ts`
Type definitions for UI utility hooks:
- `usePagination` - Pagination state management
- `useSelection` - Multi-select state
- `useFilter` - Filtering logic
- `useSort` - Sorting logic
- `useBulkActions` - Generic bulk operations
- `useDataLoader` - Data loading patterns
- `useConfirmation` - Confirmation dialogs
- `useDebouncedValue` - Debounced values
- `useInfiniteScroll` - Infinite scroll support

**Key Types:**
- `PaginationOptions` & `UsePaginationReturn` - Pagination
- `UseSelectionOptions<T, K>` & `UseSelectionReturn<T, K>` - Selection
- `BulkActionStatus` - Bulk operation status
- `BulkActionProgress` - Progress tracking
- `UseBulkActionsReturn<K>` - Bulk action state

### `realtime.ts`
Type definitions for realtime data hooks:
- `useSSE` - Server-Sent Events
- `useSSEWithPollingFallback` - SSE with polling fallback
- `useLiveData` - Live data with freshness tracking
- `usePolling` - Polling logic
- `useNotifications` - Notification management
- `useActivityEvents` - Activity event streaming
- `useActivityFeed` - Activity feed management

**Key Types:**
- `ConnectionStatus` - Connection state tracking
- `PollingSpeed` - Polling speed presets
- `DataFreshnessLevel` - Data freshness indicators
- `UseLiveDataOptions<T>` & `UseLiveDataReturn<T>` - Live data

### `inference.ts`
Type definitions for inference hooks:
- `useStreamingInference` - Streaming inference
- `useBatchInference` - Batch inference
- `useAdapterSelection` - Adapter selection state
- `useBackendSelection` - Backend selection
- `useInferenceConfig` - Inference configuration
- `useCoreMLManagement` - CoreML model management

**Key Types:**
- `UseStreamingInferenceOptions` - Streaming config
- `UseBatchInferenceOptions` - Batch config
- `UseAdapterSelectionReturn` - Selection state

### `model-loading.ts`
Type definitions for model loading hooks:
- `useModelStatus` - Model status tracking
- `useModelLoader` - Model load/unload operations
- `useAdapterStates` - Adapter state tracking
- `useAutoLoadModel` - Auto-load preferences
- `useChatLoadingPersistence` - Loading state persistence
- `useLoadingAnnouncements` - Loading announcements

**Key Types:**
- `ModelLoadingStatus` - Loading status enum
- `UseModelStatusReturn` - Status tracking
- `UseAdapterStatesOptions` - State monitoring config

## Usage

### Importing Types

```typescript
// Import specific types
import type {
  UseAsyncActionOptions,
  UseAsyncActionReturn,
} from '@/types/hooks';

// Import from category-specific file
import type {
  UseChatStreamingOptions,
  UseChatStreamingReturn,
} from '@/types/hooks/chat';

// Import everything from a category
import type * from '@/types/hooks/async';
```

### Using in Hook Implementations

Before:
```typescript
// In ui/src/hooks/async/useAsyncAction.ts
export interface UseAsyncActionOptions<TData, TVariables> {
  onSuccess?: (data: TData, variables: TVariables) => void;
  // ... more fields
}

export interface UseAsyncActionReturn<TData, TVariables> {
  execute: (variables: TVariables) => Promise<TData | null>;
  // ... more fields
}
```

After:
```typescript
// In ui/src/hooks/async/useAsyncAction.ts
import type {
  UseAsyncActionOptions,
  UseAsyncActionReturn,
} from '@/types/hooks';

// Implementation only - types imported from centralized location
export function useAsyncAction<TData, TVariables>(
  actionFn: (variables: TVariables) => Promise<TData>,
  options: UseAsyncActionOptions<TData, TVariables> = {}
): UseAsyncActionReturn<TData, TVariables> {
  // ...
}
```

### Using in Components

```typescript
import { useAsyncAction } from '@/hooks/async';
import type { UseAsyncActionReturn } from '@/types/hooks';

function MyComponent() {
  const action: UseAsyncActionReturn<string, { id: string }> = useAsyncAction(
    async ({ id }) => await api.fetchItem(id),
    { successToast: 'Item fetched!' }
  );

  return <button onClick={() => action.execute({ id: '123' })}>Fetch</button>;
}
```

## Migration Status

### Completed ✅
- [x] Created organized type files in `ui/src/types/hooks/`
- [x] Extracted types from async hooks
- [x] Extracted types from adapter hooks
- [x] Extracted types from chat hooks
- [x] Extracted types from training hooks
- [x] Extracted types from UI hooks
- [x] Extracted types from realtime hooks
- [x] Extracted types from inference hooks
- [x] Extracted types from model-loading hooks
- [x] Updated central export in `index.ts`
- [x] Updated sample hook files to import from `@/types/hooks`

### Updated Hook Files (Sample)
1. `ui/src/hooks/async/useAsyncAction.ts`
2. `ui/src/hooks/async/useAsyncOperation.ts`
3. `ui/src/hooks/adapters/useAdapterBulkActions.ts`
4. `ui/src/hooks/chat/useChatStreaming.ts`
5. `ui/src/hooks/training/useTrainingPreflight.ts`
6. `ui/src/hooks/ui/useBulkActions.ts`
7. `ui/src/hooks/realtime/useLiveData.ts`

### Remaining Work
- [ ] Update remaining hook files to import types from `@/types/hooks`
- [ ] Remove duplicate type definitions from hook files
- [ ] Update component imports to use centralized types
- [ ] Add JSDoc comments to all exported types
- [ ] Create type validation tests

## Benefits

1. **Reduced Duplication** - Types defined once, used everywhere
2. **Better Discoverability** - All hook types in one location
3. **Easier Refactoring** - Change types in one place
4. **Type Safety** - Consistent types across hooks and components
5. **Documentation** - Clear type organization aids understanding
6. **Testing** - Easier to test type compatibility

## Conventions

1. **Naming**
   - Options interfaces: `Use{HookName}Options`
   - Return interfaces: `Use{HookName}Return`
   - State interfaces: `{HookName}State`
   - Result interfaces: `{HookName}Result`

2. **Generics**
   - Use descriptive type parameters: `TData`, `TVariables`, `TItem`, `K` (for keys)
   - Constrain generics when needed: `K extends string | number`

3. **Documentation**
   - Add JSDoc comments for all exported types
   - Include usage examples for complex types
   - Document default values in comments

4. **Organization**
   - Group related types together
   - Export from category files
   - Re-export from index.ts

## See Also

- `ui/src/hooks/` - Hook implementations
- `ui/src/types/` - Other type definitions
- `ui/src/api/api-types.ts` - API type definitions
