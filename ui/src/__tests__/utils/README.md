# Test Utilities

Comprehensive test utilities for AdapterOS UI testing, including mock factories, test providers, and mock API clients.

## Overview

This directory contains reusable testing utilities that provide:

- **Mock Factories**: Create consistent mock data objects
- **Test Providers**: Wrap components with necessary providers (QueryClient, Router)
- **Mock API Client**: Simulate API responses with configurable behavior

## Mock Factories

Location: `mockFactories.ts`

### Creating Mock Objects

```typescript
import {
  createMockDocument,
  createMockCollection,
  createMockEvidence,
  createMockChatSession,
} from '@/__tests__/utils';

// Create with defaults
const doc = createMockDocument();

// Create with overrides
const customDoc = createMockDocument({
  id: 'custom-id',
  content: 'Custom content',
  metadata: {
    title: 'Custom Title',
    tags: ['custom', 'test'],
  },
});

// Create lists
const docs = createMockDocumentList(10); // Creates 10 mock documents
const collections = createMockCollectionList(5); // Creates 5 mock collections
```

### Available Factory Functions

- `createMockDocument(overrides?)` - Single document
- `createMockDocumentMetadata(overrides?)` - Document metadata
- `createMockCollection(overrides?)` - Single collection
- `createMockCollectionMetadata(overrides?)` - Collection metadata
- `createMockEvidence(overrides?)` - Single evidence item
- `createMockChatSession(overrides?)` - Single chat session
- `createMockPolicyCheck(overrides?)` - Single policy check
- `createMockPolicyCheckResult(overrides?)` - Policy check result
- `createMockDocumentList(count)` - List of documents
- `createMockCollectionList(count)` - List of collections
- `createMockEvidenceList(count)` - List of evidence items
- `createMockChatSessionList(count)` - List of chat sessions

### Utility Functions

```typescript
// Create error responses
const error = createMockError('Not found', 'NOT_FOUND');

// Create paginated responses
const response = createMockPaginatedResponse(items, total, page, limit);
```

## Test Providers

Location: `testProviders.tsx`

### Rendering with Providers

```typescript
import { renderWithProviders } from '@/__tests__/utils';

test('renders component with all providers', () => {
  const { getByText } = renderWithProviders(<MyComponent />);
  expect(getByText('Hello')).toBeInTheDocument();
});
```

### Custom Render Options

```typescript
// With custom QueryClient
const queryClient = new QueryClient();
renderWithProviders(<MyComponent />, { queryClient });

// With initial route
renderWithRoute(<MyComponent />, '/documents');

// With MemoryRouter configuration
renderWithProviders(<MyComponent />, {
  routerProps: {
    initialEntries: ['/documents', '/collections'],
    initialIndex: 1,
  },
});
```

### Query-Only Rendering

For components that only need QueryClient (no router):

```typescript
import { renderWithQuery } from '@/__tests__/utils';

test('renders with query provider only', () => {
  const { getByText } = renderWithQuery(<MyQueryComponent />);
  expect(getByText('Data loaded')).toBeInTheDocument();
});
```

### Testing Hooks

```typescript
import { renderHook } from '@testing-library/react';
import { QueryWrapper } from '@/__tests__/utils';

test('uses custom hook', () => {
  const { result } = renderHook(() => useDocuments(), {
    wrapper: QueryWrapper,
  });

  expect(result.current.isLoading).toBe(false);
});
```

### Provider Components

- `AllProviders` - QueryClient + Router providers
- `QueryWrapper` - QueryClient provider only

### Helper Functions

- `waitForQueries(queryClient)` - Wait for all queries to settle
- `clearQueryCache(queryClient)` - Clear all query cache
- `createRouterEntries(paths)` - Create router initial entries
- `renderWithRoute(ui, initialRoute, options)` - Render with specific route

### Mock Contexts

```typescript
import { mockAuthContext, mockRouterContext } from '@/__tests__/utils';

// Use in tests that need auth context
console.log(mockAuthContext.user); // { id: 'test-user', ... }

// Use in tests that need router context
console.log(mockRouterContext.location.pathname); // '/test'
```

## Mock API Client

Location: `mockApiClient.ts`

### Basic Usage

```typescript
import { createMockApiClient } from '@/__tests__/utils';

const mockApi = createMockApiClient();

// Use in tests
const documents = await mockApi.getDocuments();
const doc = await mockApi.getDocument('doc-1');
```

### Configuration

```typescript
// Simulate delays
const slowApi = createMockApiClient({ delay: 1000 });

// Simulate errors
const errorApi = createMockApiClient({
  shouldError: true,
  errorMessage: 'Network error',
  errorCode: 'NETWORK_ERROR',
});

// Update config at runtime
mockApi.setConfig({ delay: 500 });
```

### State Management

```typescript
const mockApi = createMockApiClient();

// Reset to initial state
mockApi.reset();

// Access internal state
const state = mockApi.getState();
console.log(state.getDocuments()); // All documents

// Manually add data
state.addDocument(createMockDocument({ id: 'custom-doc' }));
```

### API Methods

**Documents:**
- `getDocuments(params?)` - List documents with filtering/pagination
- `getDocument(id)` - Get single document
- `createDocument(req)` - Create document
- `updateDocument(id, req)` - Update document
- `deleteDocument(id)` - Delete document

**Collections:**
- `getCollections(params?)` - List collections with filtering/pagination
- `getCollection(id)` - Get single collection
- `createCollection(req)` - Create collection
- `updateCollection(id, req)` - Update collection
- `deleteCollection(id)` - Delete collection

**Evidence:**
- `getEvidence(messageId)` - Get evidence for message

**Chat Sessions:**
- `getChatSessions(tenantId)` - List sessions for tenant
- `getChatSession(id)` - Get single session
- `createChatSession(req)` - Create session
- `updateChatSession(id, req)` - Update session
- `deleteChatSession(id)` - Delete session

**Policy Checks:**
- `checkMessagePolicy(messageId)` - Check policy for message

### Integration with MSW

```typescript
import { setupMockApiResponses } from '@/__tests__/utils';
import { rest } from 'msw';
import { setupServer } from 'msw/node';

const mockApi = createMockApiClient();
const handlers = setupMockApiResponses(mockApi);

const server = setupServer(
  rest.get('/v1/documents', handlers['GET /v1/documents']),
  rest.get('/v1/collections', handlers['GET /v1/collections'])
  // ... other handlers
);

beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());
```

## Complete Example

```typescript
import {
  renderWithProviders,
  createMockApiClient,
  createMockDocumentList,
  waitForQueries,
} from '@/__tests__/utils';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

describe('DocumentList', () => {
  let mockApi: ReturnType<typeof createMockApiClient>;

  beforeEach(() => {
    mockApi = createMockApiClient();
  });

  test('displays documents', async () => {
    // Setup mock data
    const docs = createMockDocumentList(3);
    mockApi.getState().reset();
    docs.forEach((doc) => mockApi.getState().addDocument(doc));

    // Render component
    const { queryClient } = renderWithProviders(<DocumentList />);

    // Wait for data to load
    await waitFor(() => {
      expect(screen.getByText('Document 1')).toBeInTheDocument();
    });

    // Verify all documents rendered
    expect(screen.getByText('Document 2')).toBeInTheDocument();
    expect(screen.getByText('Document 3')).toBeInTheDocument();

    // Clean up
    await waitForQueries(queryClient);
  });

  test('handles errors', async () => {
    // Configure API to return errors
    mockApi.setConfig({
      shouldError: true,
      errorMessage: 'Failed to load documents',
    });

    renderWithProviders(<DocumentList />);

    // Verify error message displayed
    await waitFor(() => {
      expect(screen.getByText(/failed to load/i)).toBeInTheDocument();
    });
  });

  test('creates new document', async () => {
    const user = userEvent.setup();
    renderWithProviders(<DocumentList />);

    // Click create button
    await user.click(screen.getByText('Create Document'));

    // Fill form
    await user.type(screen.getByLabelText('Title'), 'New Doc');
    await user.type(screen.getByLabelText('Content'), 'Document content');

    // Submit
    await user.click(screen.getByText('Submit'));

    // Verify document created
    await waitFor(() => {
      expect(mockApi.getState().getDocuments()).toHaveLength(6); // 5 default + 1 new
    });
  });
});
```

## Best Practices

1. **Reset state between tests**: Always call `mockApi.reset()` in `beforeEach`
2. **Use factory functions**: Prefer `createMockDocument()` over manual object creation
3. **Wait for async operations**: Use `waitFor()` and `waitForQueries()` properly
4. **Clean up**: Clear query cache after tests that modify it
5. **Configure realistically**: Use realistic delays for UX testing
6. **Test error states**: Use `shouldError` to test error handling
7. **Isolate tests**: Each test should be independent and not affect others

## TypeScript Support

All utilities are fully typed. Import types from `api/chat-types`:

```typescript
import type { Document, Collection } from '@/api/chat-types';

const doc: Document = createMockDocument();
```

## Related Documentation

- [React Query Testing](https://tanstack.com/query/latest/docs/react/guides/testing)
- [React Testing Library](https://testing-library.com/docs/react-testing-library/intro/)
- [MSW (Mock Service Worker)](https://mswjs.io/)
