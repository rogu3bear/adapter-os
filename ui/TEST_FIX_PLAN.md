# UI Test Fix Plan

## Problem Summary

The UI test suite has **187 failing tests across 31 test files**. These failures are caused by **mock configuration issues**, not actual code bugs. The main patterns are:

1. **Incorrect mock paths** - Tests mock hooks at wrong import paths
2. **Incomplete mock returns** - Mocks don't return all properties components expect
3. **Missing context providers** - Components require providers not included in test wrappers

## Priority Order

Fix in this order (highest impact first):

1. **ChatInterface tests** (affects ~20 tests)
2. **ChatTagsManager tests** (affects ~17 tests)
3. **DocumentChatFlow tests** (affects ~14 tests)
4. **Training-related tests** (affects ~30 tests)
5. **Remaining hook tests**

---

## Pattern 1: Incorrect Mock Paths

### Problem
Tests mock hooks at incorrect paths that don't match actual imports.

### Files Affected
- `src/__tests__/ChatInterfaceTagsModal.test.tsx`
- `src/__tests__/ChatInterface.test.tsx`
- `src/__tests__/DocumentChatFlow.test.tsx`
- `src/__tests__/RoleDashboards.test.tsx`
- `src/__tests__/ChatTagsManager.test.tsx`

### Fix Pattern

**WRONG:**
```typescript
vi.mock('@/hooks/useCollectionsApi', () => ({...}));
vi.mock('@/hooks/useAdmin', () => ({...}));
vi.mock('@/hooks/useChatSessionsApi', () => ({...}));
```

**CORRECT:**
```typescript
vi.mock('@/hooks/api/useCollectionsApi', () => ({...}));
vi.mock('@/hooks/admin/useAdmin', () => ({...}));
vi.mock('@/hooks/chat/useChatSessionsApi', () => ({...}));
```

### How to Find Correct Paths
1. Open the component being tested
2. Look at its imports
3. Match mock paths exactly to import paths

**Example:** If `ChatInterface.tsx` has:
```typescript
import { useCollections } from '@/hooks/api/useCollectionsApi';
```
Then mock must be:
```typescript
vi.mock('@/hooks/api/useCollectionsApi', () => ({...}));
```

---

## Pattern 2: Incomplete Mock Returns

### Problem
Components destructure properties from hooks that mocks don't provide.

### Example Error
```
TypeError: Cannot read properties of undefined (reading 'length')
```

### Files Affected
All ChatInterface-related tests need these complete mocks:

### Required Mock Structures

#### `useModelLoadingState` (from `@/hooks/model-loading`)
```typescript
vi.mock('@/hooks/model-loading', () => ({
  useModelLoadingState: () => ({
    isLoading: false,
    loadingModel: null,
    progress: 0,
    overallReady: true,
    baseModelReady: true,
    failedAdapters: [],
    loadingAdapters: [],
    readyAdapters: [],
    adapterStates: new Map(),
    error: null,
  }),
  useModelLoader: () => ({
    loadModel: vi.fn(),
    cancelLoading: vi.fn(),
  }),
  useChatLoadingPersistence: () => ({
    persistedModel: null,
    setPersistedModel: vi.fn(),
  }),
  useLoadingAnnouncements: () => ({
    announce: vi.fn(),
  }),
}));
```

#### `useChatAdapterState` (from `@/hooks/chat`)
```typescript
vi.mock('@/hooks/chat', () => ({
  useChatStreaming: () => ({
    sendMessage: vi.fn(),
    isStreaming: false,
    streamingContent: '',
    error: null,
  }),
  useChatAdapterState: () => ({
    adapters: [],
    adapterStates: new Map(),
    allAdaptersReady: true,
    isLoading: false,
    loadAdapter: vi.fn(),
    selectedAdapter: null,
  }),
  useChatRouterDecisions: () => ({
    decisions: [],
    isLoading: false,
  }),
  useSessionManager: () => ({
    currentSession: null,
    selectSession: vi.fn(),
  }),
  useChatModals: () => ({
    isRenameOpen: false,
    openRename: vi.fn(),
    closeRename: vi.fn(),
  }),
}));
```

#### `useChatSessionsApi` (from `@/hooks/chat/useChatSessionsApi`)
```typescript
vi.mock('@/hooks/chat/useChatSessionsApi', () => ({
  useChatSessionsApi: () => ({
    sessions: [],
    isLoading: false,
    isUnsupported: false,
    unsupportedReason: null,
    createSession: vi.fn(),
    updateSession: vi.fn(),
    addMessage: vi.fn(),
    updateMessage: vi.fn(),
    deleteSession: vi.fn(),
    getSession: vi.fn(),
    updateSessionCollection: vi.fn(),
  }),
}));
```

#### `useCollections` (from `@/hooks/api/useCollectionsApi`)
```typescript
vi.mock('@/hooks/api/useCollectionsApi', () => ({
  useCollections: () => ({ data: [], isLoading: false }),
}));
```

#### `useAdapterStacks` and `useGetDefaultStack` (from `@/hooks/admin/useAdmin`)
```typescript
vi.mock('@/hooks/admin/useAdmin', () => ({
  useAdapterStacks: () => ({ data: [], isLoading: false }),
  useGetDefaultStack: () => ({ data: null, isLoading: false }),
}));
```

---

## Pattern 3: Missing Context Providers

### Problem
Components use `useTenant` which requires `FeatureProviders` context.

### Error
```
Error: useTenant must be used within FeatureProviders
```

### Solution Options

**Option A: Mock the hook that uses useTenant**
If the hook is already mocked completely, the context isn't needed.

**Option B: Add FeatureProviders to TestWrapper**
```typescript
import { FeatureProviders } from '@/providers/FeatureProviders';

function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return (
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        <FeatureProviders tenant="default">
          {children}
        </FeatureProviders>
      </QueryClientProvider>
    </MemoryRouter>
  );
}
```

**Option C: Mock useTenant directly**
```typescript
vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({ tenantId: 'default', tenant: { id: 'default', name: 'Default' } }),
  FeatureProviders: ({ children }: { children: React.ReactNode }) => children,
}));
```

---

## Pattern 4: Missing Component Mocks

### Problem
Child components need mocking to avoid their own dependency issues.

### Common Components to Mock

```typescript
// Evidence drawer context
vi.mock('@/contexts/EvidenceDrawerContext', () => ({
  EvidenceDrawerProvider: ({ children }: { children: React.ReactNode }) => children,
  useEvidenceDrawerOptional: () => null,
}));

// Export hook
vi.mock('@/components/export', () => ({
  useChatExport: () => ({
    exportChat: vi.fn(),
    isExporting: false,
  }),
}));

// Feature flags
vi.mock('@/hooks/config/useFeatureFlags', () => ({
  useChatAutoLoadModels: () => false,
}));

// Chat sub-components (mock if not testing them)
vi.mock('@/components/chat/ChatLoadingOverlay', () => ({
  ChatLoadingOverlay: () => null,
}));
vi.mock('@/components/chat/ChatErrorDisplay', () => ({
  ChatErrorDisplay: () => null,
}));
vi.mock('@/components/chat/MissingPinnedAdaptersBanner', () => ({
  MissingPinnedAdaptersBanner: () => null,
}));
vi.mock('@/components/chat/EvidenceDrawer', () => ({
  EvidenceDrawer: () => null,
}));
vi.mock('@/components/chat/InlineModelLoadingBlock', () => ({
  InlineModelLoadingBlock: () => null,
}));
```

---

## Specific File Fixes

### 1. `src/__tests__/ChatInterfaceTagsModal.test.tsx`

**Current Issues:**
- Partially fixed mock paths
- Still missing complete mock returns

**Actions:**
1. Verify all mock paths match actual imports in `ChatInterface.tsx`
2. Ensure `useChatAdapterState` returns `adapterStates: new Map()` and `allAdaptersReady: true`
3. Ensure `useModelLoadingState` returns all required properties
4. Add mocks for all chat sub-components

### 2. `src/__tests__/ChatTagsManager.test.tsx`

**Actions:**
1. Check imports in `ChatTagsManager.tsx`
2. Mock `@/hooks/chat/useChatTags` with complete return:
```typescript
vi.mock('@/hooks/chat/useChatTags', () => ({
  useChatTags: () => ({
    tags: [],
    isLoading: false,
    createTag: vi.fn(),
    deleteTag: vi.fn(),
  }),
  useSessionTags: () => ({
    tags: [],
    isLoading: false,
    addTag: vi.fn(),
    removeTag: vi.fn(),
  }),
}));
```

### 3. `src/__tests__/DocumentChatFlow.test.tsx`

**Actions:**
1. Fix mock path for `useChatSessionsApi`
2. Add complete mocks for document-specific hooks
3. Mock RAG/evidence-related hooks

### 4. `src/__tests__/AdminPolicyConsole.test.tsx`

**Actions:**
1. Check what policy hooks are imported
2. Mock them with complete return values

### 5. `src/__tests__/DatasetsTab.test.tsx` and Training tests

**Actions:**
1. Mock training API hooks completely
2. Ensure `getTrainingJob` and similar functions are mocked

---

## Verification Process

After fixing each test file:

1. **Run single file:**
```bash
pnpm test -- --reporter=verbose src/__tests__/[filename].test.tsx
```

2. **Check for new errors** - Fix any remaining mock issues

3. **Run related tests:**
```bash
pnpm test -- --reporter=verbose src/__tests__/*Chat*.test.tsx
```

4. **Run full suite:**
```bash
pnpm test -- --run
```

---

## Quick Reference: Import Path Mapping

| Mock Path | Actual Path |
|-----------|-------------|
| `@/hooks/useAdmin` | `@/hooks/admin/useAdmin` |
| `@/hooks/useCollectionsApi` | `@/hooks/api/useCollectionsApi` |
| `@/hooks/useChatSessionsApi` | `@/hooks/chat/useChatSessionsApi` |
| `@/hooks/useChatTags` | `@/hooks/chat/useChatTags` |
| `@/hooks/useSSE` | `@/hooks/useSSE` (correct) |

---

## Success Criteria

- All 187 failing tests pass
- No new test failures introduced
- `pnpm test -- --run` exits with code 0
- `pnpm build` still passes

---

## Commands

```bash
# Run all tests
pnpm test -- --run

# Run specific test file with verbose output
pnpm test -- --reporter=verbose src/__tests__/ChatInterface.test.tsx

# Run tests matching pattern
pnpm test -- --reporter=verbose "Chat"

# Run with watch mode for iterative fixing
pnpm test src/__tests__/ChatInterface.test.tsx
```

---

## Notes

- The test failures are NOT caused by recent code changes
- The actual components work correctly (build passes, UI runs)
- Focus on mock configuration, not component logic
- When in doubt, check the actual component's imports to find correct mock paths
