# React Hooks Reference

Custom React hooks for AdapterOS UI, providing reusable logic for API integration, state management, and UI patterns.

## Overview

AdapterOS UI uses React hooks for:
- **API Integration:** React Query-based hooks for backend communication
- **State Management:** Local and global state with context integration
- **UI Patterns:** Reusable UI logic (pagination, filtering, sorting)
- **Specialized Logic:** RBAC, security, telemetry, and workflow management

## API Hooks (React Query)

API hooks use `@tanstack/react-query` for server state management with automatic caching, refetching, and cache invalidation.

### Common Pattern

```typescript
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';

// Query keys for cache management
export const resourceKeys = {
  all: ['resources'] as const,
  lists: () => [...resourceKeys.all, 'list'] as const,
  details: () => [...resourceKeys.all, 'detail'] as const,
  detail: (id: string) => [...resourceKeys.details(), id] as const,
};

// List hook
export function useResources() {
  return useQuery({
    queryKey: resourceKeys.list(),
    queryFn: () => apiClient.listResources(),
    staleTime: 30000, // Optional: 30 seconds
  });
}

// Detail hook
export function useResource(id: string | undefined) {
  return useQuery({
    queryKey: resourceKeys.detail(id ?? ''),
    queryFn: () => apiClient.getResource(id!),
    enabled: !!id, // Only fetch when id is defined
  });
}

// CRUD operations hook
export function useResourcesApi() {
  const queryClient = useQueryClient();

  const createMutation = useMutation({
    mutationFn: (data) => apiClient.createResource(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: resourceKeys.lists() });
    },
  });

  return {
    resources: useResources(),
    createResource: createMutation.mutateAsync,
    isCreating: createMutation.isPending,
    createError: createMutation.error,
  };
}
```

---

### useChatSessionsApi

Backend-aware chat session management with localStorage migration.

```typescript
import { useChatSessionsApi } from '@/hooks/useChatSessionsApi';

function ChatInterface() {
  const {
    sessions,           // Query result (data, isLoading, error)
    currentSession,     // Current active session

    // Session operations
    createSession,      // (name, stackId?, collectionId?) => Promise<ChatSession>
    deleteSession,      // (sessionId) => Promise<void>
    sendMessage,        // (sessionId, content) => Promise<ChatMessage>
    setCurrentSession,  // (sessionId) => void

    // State
    isCreating,
    isSending,

    // Cache management
    invalidateSessions, // () => void
  } = useChatSessionsApi('tenant-id');

  return (
    <div>
      {sessions.data?.map(session => (
        <div key={session.id}>{session.name}</div>
      ))}
      <button onClick={() => createSession('New Chat', 'stack-id')}>
        New Session
      </button>
    </div>
  );
}
```

**Features:**
- Automatic localStorage migration on first load
- Session list with messages
- Create, delete, send message operations
- Current session state management
- Cache invalidation on mutations

---

### useDocumentsApi

Document upload, download, and management.

```typescript
import { useDocumentsApi, useDocument } from '@/hooks/useDocumentsApi';

function DocumentLibrary() {
  const {
    documents,          // Query result
    uploadDocument,     // (file, name?) => Promise<Document>
    deleteDocument,     // (documentId) => Promise<void>
    downloadDocument,   // (documentId) => Promise<Blob>
    isUploading,
    isDeleting,
  } = useDocumentsApi();

  const { data: document } = useDocument('doc-123');

  const handleUpload = async (file: File) => {
    await uploadDocument({ file, name: 'My Document' });
  };

  const handleDownload = async (id: string) => {
    const blob = await downloadDocument(id);
    const url = URL.createObjectURL(blob);
    window.open(url);
  };

  return (
    <div>
      <input type="file" onChange={(e) => handleUpload(e.target.files[0])} />
      {documents.data?.map(doc => (
        <div key={doc.id}>
          {doc.name}
          <button onClick={() => handleDownload(doc.id)}>Download</button>
          <button onClick={() => deleteDocument(doc.id)}>Delete</button>
        </div>
      ))}
    </div>
  );
}
```

**Query Keys:**
```typescript
documentKeys.all          // ['documents']
documentKeys.list()       // ['documents', 'list']
documentKeys.detail(id)   // ['documents', 'detail', id]
documentKeys.chunks(id)   // ['documents', 'detail', id, 'chunks']
```

---

### useCollectionsApi

Collection CRUD operations with document management.

```typescript
import { useCollectionsApi, useCollection } from '@/hooks/useCollectionsApi';

function CollectionManager() {
  const {
    collections,              // Query result
    createCollection,         // (name, description?) => Promise<Collection>
    deleteCollection,         // (collectionId) => Promise<void>
    addDocumentToCollection,  // (collectionId, documentId) => Promise<void>
    removeDocumentFromCollection, // (collectionId, documentId) => Promise<void>
    isCreating,
    isAddingDocument,
  } = useCollectionsApi();

  const { data: collection } = useCollection('collection-123');

  return (
    <div>
      <button onClick={() => createCollection({ name: 'My Collection' })}>
        New Collection
      </button>
      {collections.data?.map(col => (
        <div key={col.id}>
          {col.name} ({col.document_count} documents)
        </div>
      ))}
    </div>
  );
}
```

**Query Keys:**
```typescript
collectionKeys.all          // ['collections']
collectionKeys.list()       // ['collections', 'list']
collectionKeys.detail(id)   // ['collections', 'detail', id]
```

---

### useEvidenceApi

Evidence entry management with filtering.

```typescript
import {
  useEvidenceApi,
  useDatasetEvidence,
  useAdapterEvidence,
} from '@/hooks/useEvidenceApi';

function EvidencePanel({ datasetId, adapterId }) {
  // Filter evidence by dataset/adapter
  const { data: datasetEvidence } = useDatasetEvidence(datasetId);
  const { data: adapterEvidence } = useAdapterEvidence(adapterId);

  // CRUD operations
  const {
    evidence,
    createEvidence,
    deleteEvidence,
    isCreating,
  } = useEvidenceApi({ dataset_id: datasetId });

  const handleCreate = async () => {
    await createEvidence({
      source_type: 'document',
      source_id: 'doc-123',
      chunk_ids: ['chunk-1', 'chunk-2'],
      relevance_score: 0.95,
      confidence_score: 0.88,
      dataset_id: datasetId,
    });
  };

  return (
    <div>
      <button onClick={handleCreate}>Add Evidence</button>
      {evidence.data?.map(ev => (
        <div key={ev.id}>
          {ev.source_type}: {ev.relevance_score}
        </div>
      ))}
    </div>
  );
}
```

**Query Keys:**
```typescript
evidenceKeys.all                // ['evidence']
evidenceKeys.list(filter)       // ['evidence', 'list', filter]
evidenceKeys.byDataset(id)      // ['evidence', 'dataset', id]
evidenceKeys.byAdapter(id)      // ['evidence', 'adapter', id]
```

---

### useSettings

Tenant settings management with optimistic updates.

```typescript
import { useSettings } from '@/hooks/useSettings';

function SettingsPage() {
  const {
    settings,         // Query result
    updateSettings,   // (updates) => Promise<Settings>
    isUpdating,
    updateError,
  } = useSettings('tenant-id');

  const handleUpdate = async () => {
    await updateSettings({
      default_stack_id: 'stack-xyz',
      preferences: { theme: 'dark' },
    });
  };

  return (
    <div>
      <div>Stack: {settings.data?.default_stack_id}</div>
      <button onClick={handleUpdate} disabled={isUpdating}>
        Save Settings
      </button>
    </div>
  );
}
```

---

## Cache Invalidation Patterns

### Manual Invalidation

```typescript
const queryClient = useQueryClient();

// Invalidate specific query
queryClient.invalidateQueries({ queryKey: documentKeys.detail('doc-123') });

// Invalidate all queries for a resource
queryClient.invalidateQueries({ queryKey: documentKeys.all });

// Remove query from cache
queryClient.removeQueries({ queryKey: documentKeys.detail('doc-123') });
```

### Automatic Invalidation

```typescript
const createMutation = useMutation({
  mutationFn: (data) => apiClient.create(data),
  onSuccess: (newItem) => {
    // Invalidate list queries (they now include new item)
    queryClient.invalidateQueries({ queryKey: resourceKeys.lists() });

    // Optionally set detail query data (skip refetch)
    queryClient.setQueryData(resourceKeys.detail(newItem.id), newItem);
  },
});
```

### Related Queries

```typescript
const addDocumentMutation = useMutation({
  mutationFn: ({ collectionId, documentId }) =>
    apiClient.addDocumentToCollection(collectionId, documentId),
  onSuccess: (_data, { collectionId, documentId }) => {
    // Invalidate collection detail (document count changed)
    queryClient.invalidateQueries({ queryKey: collectionKeys.detail(collectionId) });

    // Invalidate document detail (collection list changed)
    queryClient.invalidateQueries({ queryKey: documentKeys.detail(documentId) });
  },
});
```

---

## State Management Hooks

### useAdmin

Admin operations for tenant, node, and system management.

```typescript
import { useAdmin } from '@/hooks/useAdmin';

function AdminPanel() {
  const {
    tenants,
    nodes,
    createTenant,
    updateTenant,
    deleteTenant,
    registerNode,
    evictNode,
    isLoading,
  } = useAdmin();

  return (
    <div>
      <h2>Tenants: {tenants.data?.length}</h2>
      <h2>Nodes: {nodes.data?.length}</h2>
    </div>
  );
}
```

---

### useAdapterOperations

Adapter lifecycle operations (load, unload, pin, promote).

```typescript
import { useAdapterOperations } from '@/hooks/useAdapterOperations';

function AdapterControls({ adapterId }) {
  const {
    loadAdapter,
    unloadAdapter,
    pinAdapter,
    unpinAdapter,
    promoteAdapter,
    isLoading,
    error,
  } = useAdapterOperations();

  return (
    <div>
      <button onClick={() => loadAdapter(adapterId)}>Load</button>
      <button onClick={() => pinAdapter(adapterId, 'production')}>Pin</button>
      <button onClick={() => promoteAdapter(adapterId)}>Promote</button>
    </div>
  );
}
```

---

### useStreamingInference

Streaming inference with SSE (Server-Sent Events).

```typescript
import { useStreamingInference } from '@/hooks/useStreamingInference';

function ChatInterface() {
  const {
    messages,
    isStreaming,
    error,
    sendMessage,
    stopStreaming,
    clearMessages,
  } = useStreamingInference({
    apiUrl: '/v1/infer',
    onComplete: (fullResponse) => console.log('Done:', fullResponse),
  });

  const handleSend = () => {
    sendMessage({
      prompt: 'Explain async Rust',
      max_tokens: 200,
      stream: true,
    });
  };

  return (
    <div>
      {messages.map((msg, i) => (
        <div key={i}>{msg.content}</div>
      ))}
      {isStreaming && <button onClick={stopStreaming}>Stop</button>}
    </div>
  );
}
```

---

### useTrainingNotifications

Training job notifications with SSE.

```typescript
import { useTrainingNotifications } from '@/hooks/useTrainingNotifications';

function TrainingMonitor({ jobId }) {
  const {
    notification,   // Latest notification
    history,        // All notifications
    isConnected,
    error,
  } = useTrainingNotifications(jobId);

  return (
    <div>
      {isConnected && <div>Connected</div>}
      {notification && (
        <div>
          Status: {notification.status}
          Progress: {notification.progress}%
        </div>
      )}
    </div>
  );
}
```

---

## UI Pattern Hooks

### usePagination

Pagination state and controls.

```typescript
import { usePagination } from '@/hooks/usePagination';

function DataTable({ items }) {
  const {
    page,
    pageSize,
    totalPages,
    paginatedData,
    nextPage,
    prevPage,
    setPage,
    setPageSize,
  } = usePagination(items, { initialPageSize: 20 });

  return (
    <div>
      <table>
        {paginatedData.map(item => <tr key={item.id}>...</tr>)}
      </table>
      <div>
        <button onClick={prevPage} disabled={page === 1}>Prev</button>
        <span>Page {page} of {totalPages}</span>
        <button onClick={nextPage} disabled={page === totalPages}>Next</button>
      </div>
    </div>
  );
}
```

---

### useFilter

Filtering with search, facets, and presets.

```typescript
import { useFilter } from '@/hooks/useFilter';

function FilteredList({ items }) {
  const {
    filteredData,
    searchTerm,
    setSearchTerm,
    activeFilters,
    addFilter,
    removeFilter,
    clearFilters,
  } = useFilter(items, {
    searchFields: ['name', 'description'],
    filters: {
      status: (item, value) => item.status === value,
      tier: (item, value) => item.tier === value,
    },
  });

  return (
    <div>
      <input
        value={searchTerm}
        onChange={(e) => setSearchTerm(e.target.value)}
        placeholder="Search..."
      />
      <button onClick={() => addFilter('status', 'active')}>
        Active Only
      </button>
      <div>Results: {filteredData.length}</div>
    </div>
  );
}
```

---

### useSort

Sorting with multiple columns.

```typescript
import { useSort } from '@/hooks/useSort';

function SortableTable({ items }) {
  const {
    sortedData,
    sortConfig,
    setSortConfig,
    toggleSort,
  } = useSort(items, { key: 'name', direction: 'asc' });

  return (
    <table>
      <thead>
        <tr>
          <th onClick={() => toggleSort('name')}>
            Name {sortConfig.key === 'name' && sortConfig.direction}
          </th>
          <th onClick={() => toggleSort('createdAt')}>
            Created {sortConfig.key === 'createdAt' && sortConfig.direction}
          </th>
        </tr>
      </thead>
      <tbody>
        {sortedData.map(item => <tr key={item.id}>...</tr>)}
      </tbody>
    </table>
  );
}
```

---

### useSelection

Multi-select with bulk actions.

```typescript
import { useSelection } from '@/hooks/useSelection';

function SelectableList({ items }) {
  const {
    selected,
    isSelected,
    toggle,
    selectAll,
    clearSelection,
    selectedCount,
  } = useSelection<string>();

  return (
    <div>
      <button onClick={selectAll}>Select All</button>
      <button onClick={clearSelection}>Clear</button>
      <span>{selectedCount} selected</span>
      {items.map(item => (
        <div key={item.id}>
          <input
            type="checkbox"
            checked={isSelected(item.id)}
            onChange={() => toggle(item.id)}
          />
          {item.name}
        </div>
      ))}
    </div>
  );
}
```

---

## Specialized Hooks

### useRBAC

Role-based access control checks.

```typescript
import { useRBAC } from '@/hooks/useRBAC';

function ProtectedAction() {
  const { hasPermission, hasRole, user } = useRBAC();

  if (!hasPermission('AdapterDelete')) {
    return <div>Access Denied</div>;
  }

  if (hasRole(['admin', 'operator'])) {
    return <button>Delete Adapter</button>;
  }

  return null;
}
```

---

### useSecurity

Security and policy checks.

```typescript
import { useSecurity } from '@/hooks/useSecurity';

function SecureComponent() {
  const {
    checkPolicy,
    validateInput,
    sanitize,
  } = useSecurity();

  const handleSubmit = async (data) => {
    const isValid = await checkPolicy('egress', data);
    if (!isValid) {
      throw new Error('Policy violation');
    }
    // Submit data
  };

  return <form onSubmit={handleSubmit}>...</form>;
}
```

---

### useSSE

Generic Server-Sent Events (SSE) hook.

```typescript
import { useSSE } from '@/hooks/useSSE';

function LiveUpdates() {
  const {
    data,
    isConnected,
    error,
    reconnect,
  } = useSSE('/v1/stream/metrics', {
    onMessage: (event) => console.log('Event:', event),
    onError: (err) => console.error('SSE error:', err),
  });

  return (
    <div>
      {isConnected ? 'Connected' : 'Disconnected'}
      {error && <button onClick={reconnect}>Reconnect</button>}
      <pre>{JSON.stringify(data, null, 2)}</pre>
    </div>
  );
}
```

---

## Testing Hooks

### Unit Tests

```typescript
import { renderHook, act } from '@testing-library/react';
import { useDocumentsApi } from '@/hooks/useDocumentsApi';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

describe('useDocumentsApi', () => {
  it('fetches documents', async () => {
    const queryClient = new QueryClient();
    const wrapper = ({ children }) => (
      <QueryClientProvider client={queryClient}>
        {children}
      </QueryClientProvider>
    );

    const { result } = renderHook(() => useDocumentsApi(), { wrapper });

    await act(async () => {
      await result.current.documents.refetch();
    });

    expect(result.current.documents.data).toBeDefined();
  });
});
```

---

## Best Practices

1. **Query Keys:** Use hierarchical query keys for granular cache control
   ```typescript
   ['documents', 'list']           // All lists
   ['documents', 'detail', id]     // Specific document
   ['documents', 'detail', id, 'chunks'] // Document chunks
   ```

2. **Error Handling:** Always handle loading and error states
   ```typescript
   const { data, isLoading, error } = useQuery(...);
   if (isLoading) return <Loading />;
   if (error) return <Error message={error.message} />;
   ```

3. **Enabled Queries:** Use `enabled` to prevent unnecessary fetches
   ```typescript
   useQuery({
     queryKey: ['user', userId],
     queryFn: () => fetchUser(userId),
     enabled: !!userId, // Only fetch when userId exists
   });
   ```

4. **Cache Invalidation:** Invalidate related queries on mutations
   ```typescript
   onSuccess: () => {
     queryClient.invalidateQueries({ queryKey: ['related'] });
   }
   ```

5. **Optimistic Updates:** Use `onMutate` for instant UI feedback
   ```typescript
   onMutate: async (newData) => {
     await queryClient.cancelQueries({ queryKey });
     const previous = queryClient.getQueryData(queryKey);
     queryClient.setQueryData(queryKey, newData);
     return { previous };
   }
   ```

---

## Related Documentation

- [React Query Docs](https://tanstack.com/query/latest/docs/react/overview)
- [CLAUDE.md](../../CLAUDE.md) - AdapterOS patterns and conventions
- [ui/src/api/client.ts](../api/client.ts) - API client implementation
- [ui/src/providers/CoreProviders.tsx](../providers/CoreProviders.tsx) - Context providers
