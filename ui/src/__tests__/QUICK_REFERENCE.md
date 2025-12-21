# Test Utilities Quick Reference

Quick reference for common testing patterns using AdapterOS test utilities.

## Import

```typescript
import {
  // Mock Factories
  createMockDocument,
  createMockCollection,
  createMockEvidence,
  createMockChatSession,
  createMockDocumentList,

  // Test Providers
  renderWithProviders,
  renderWithQuery,
  QueryWrapper,

  // Mock API
  createMockApiClient,
  type MockApiClient,
} from '@/__tests__/utils';
```

## Common Patterns

### 1. Basic Component Test

```typescript
test('renders component', () => {
  const { getByText } = renderWithProviders(<MyComponent />);
  expect(getByText('Expected Text')).toBeInTheDocument();
});
```

### 2. Test with Mock Data

```typescript
test('displays document', () => {
  const doc = createMockDocument({
    id: 'test-doc',
    metadata: { title: 'Test Document' }
  });

  const { getByText } = renderWithProviders(<DocumentCard document={doc} />);
  expect(getByText('Test Document')).toBeInTheDocument();
});
```

### 3. Test with Mock API

```typescript
let mockApi: MockApiClient;

beforeEach(() => {
  mockApi = createMockApiClient();
});

test('loads data from API', async () => {
  const { getByText } = renderWithProviders(<DocumentList />);

  await waitFor(() => {
    expect(getByText('Document 1')).toBeInTheDocument();
  });
});
```

### 4. Test API Operations

```typescript
test('creates document', async () => {
  const mockApi = createMockApiClient();

  const doc = await mockApi.createDocument({
    collection_id: 'col-1',
    content: 'New document',
    metadata: { title: 'New', source: 'test', filename: 'new.txt', mime_type: 'text/plain', size_bytes: 100 }
  });

  expect(doc.content).toBe('New document');
  expect(mockApi.getState().getDocuments()).toHaveLength(6); // 5 default + 1 new
});
```

### 5. Test Error Handling

```typescript
test('handles errors', async () => {
  const mockApi = createMockApiClient({
    shouldError: true,
    errorMessage: 'Network error'
  });

  await expect(mockApi.getDocuments()).rejects.toMatchObject({
    error: 'Network error'
  });
});
```

### 6. Test with Delays

```typescript
test('shows loading state', async () => {
  const mockApi = createMockApiClient({ delay: 100 });

  const { getByText } = renderWithProviders(<DocumentList />);

  // Should show loading state
  expect(getByText('Loading...')).toBeInTheDocument();

  // After delay, should show data
  await waitFor(() => {
    expect(getByText('Document 1')).toBeInTheDocument();
  });
});
```

### 7. Test Hooks

```typescript
test('uses custom hook', () => {
  const { result } = renderHook(() => useDocuments(), {
    wrapper: QueryWrapper
  });

  expect(result.current.isLoading).toBe(false);
  expect(result.current.data).toBeDefined();
});
```

### 8. Test with Router

```typescript
test('navigates to route', () => {
  const { getByText } = renderWithProviders(<App />, {
    routerProps: {
      initialEntries: ['/documents']
    }
  });

  expect(getByText('Documents')).toBeInTheDocument();
});
```

### 9. Test Filtering

```typescript
test('filters documents', async () => {
  const mockApi = createMockApiClient();

  const response = await mockApi.getDocuments({
    collection_id: 'collection-1'
  });

  expect(response.items.every(d => d.collection_id === 'collection-1')).toBe(true);
});
```

### 10. Test Search

```typescript
test('searches documents', async () => {
  const mockApi = createMockApiClient();

  mockApi.getState().addDocument(
    createMockDocument({
      id: 'searchable',
      content: 'This contains TypeScript code'
    })
  );

  const response = await mockApi.getDocuments({ search: 'TypeScript' });

  expect(response.items.length).toBeGreaterThan(0);
});
```

### 11. Test User Interactions

```typescript
test('clicks button', async () => {
  const user = userEvent.setup();
  const { getByText } = renderWithProviders(<DocumentForm />);

  await user.click(getByText('Submit'));

  await waitFor(() => {
    expect(getByText('Success')).toBeInTheDocument();
  });
});
```

### 12. Test Form Input

```typescript
test('fills form', async () => {
  const user = userEvent.setup();
  const { getByLabelText, getByText } = renderWithProviders(<DocumentForm />);

  await user.type(getByLabelText('Title'), 'New Document');
  await user.type(getByLabelText('Content'), 'Document content');
  await user.click(getByText('Submit'));

  await waitFor(() => {
    expect(getByText('Document created')).toBeInTheDocument();
  });
});
```

### 13. Test State Updates

```typescript
test('updates state', async () => {
  const mockApi = createMockApiClient();

  await mockApi.updateDocument('doc-1', { content: 'Updated' });

  const doc = mockApi.getState().getDocument('doc-1');
  expect(doc?.content).toBe('Updated');
});
```

### 14. Test Pagination

```typescript
test('paginates results', async () => {
  const mockApi = createMockApiClient();

  // Add more documents
  for (let i = 6; i <= 25; i++) {
    mockApi.getState().addDocument(
      createMockDocument({ id: `doc-${i}` })
    );
  }

  const page1 = await mockApi.getDocuments({ limit: 10, offset: 0 });
  expect(page1.items).toHaveLength(10);

  const page2 = await mockApi.getDocuments({ limit: 10, offset: 10 });
  expect(page2.items).toHaveLength(10);
});
```

### 15. Test Clean Up

```typescript
let mockApi: MockApiClient;
let queryClient: QueryClient;

beforeEach(() => {
  mockApi = createMockApiClient();
});

afterEach(async () => {
  mockApi.reset();
  if (queryClient) {
    await waitForQueries(queryClient);
    clearQueryCache(queryClient);
  }
});
```

## Factory Function Cheat Sheet

```typescript
// Documents
createMockDocument({ id, content, metadata })
createMockDocumentList(count)

// Collections
createMockCollection({ id, name, description, document_count })
createMockCollectionList(count)

// Evidence
createMockEvidence({ id, message_id, relevance_score, snippet })
createMockEvidenceList(count)

// Chat Sessions
createMockChatSession({ id, title, collection_id })
createMockChatSessionList(count)

// Policy Checks
createMockPolicyCheck({ policy_id, status })
createMockPolicyCheckResult({ message_id, overall_status })

// Utilities
createMockError(message, code)
createMockPaginatedResponse(items, total, page, limit)
```

## Mock API Client Cheat Sheet

```typescript
const mockApi = createMockApiClient({
  delay: 100,           // Network delay (ms)
  shouldError: false,   // Throw errors
  errorMessage: 'Err',  // Error message
  errorCode: 'CODE'     // Error code
});

// Documents
await mockApi.getDocuments(params?)
await mockApi.getDocument(id)
await mockApi.createDocument(req)
await mockApi.updateDocument(id, req)
await mockApi.deleteDocument(id)

// Collections
await mockApi.getCollections(params?)
await mockApi.getCollection(id)
await mockApi.createCollection(req)
await mockApi.updateCollection(id, req)
await mockApi.deleteCollection(id)

// Evidence
await mockApi.getEvidence(messageId)

// Chat Sessions
await mockApi.getChatSessions(tenantId)
await mockApi.getChatSession(id)
await mockApi.createChatSession(req)
await mockApi.updateChatSession(id, req)
await mockApi.deleteChatSession(id)

// Policy Checks
await mockApi.checkMessagePolicy(messageId)

// State
mockApi.reset()
mockApi.setConfig({ delay: 200 })
const state = mockApi.getState()
```

## Best Practices Checklist

- [ ] Reset mock API state in `beforeEach`
- [ ] Use factory functions instead of manual object creation
- [ ] Use `waitFor` for async operations
- [ ] Clean up query cache in `afterEach`
- [ ] Use realistic delays for UX testing
- [ ] Test both success and error states
- [ ] Keep tests isolated and independent
- [ ] Use descriptive test names
- [ ] Group related tests with `describe`
- [ ] Add comments for complex test logic

## Troubleshooting

**Problem**: Tests timing out
**Solution**: Check for missing `await` or increase timeout

**Problem**: State persisting between tests
**Solution**: Call `mockApi.reset()` in `beforeEach`

**Problem**: Query cache errors
**Solution**: Use `waitForQueries()` before cleanup

**Problem**: Router errors
**Solution**: Use `renderWithProviders` instead of plain `render`

**Problem**: Hook errors
**Solution**: Wrap with `QueryWrapper` when using `renderHook`

## Additional Resources

- Full Documentation: `/ui/src/__tests__/utils/README.md`
- Examples: `/ui/src/__tests__/utils/example.test.tsx`
- Summary: `/ui/src/__tests__/UTILITIES_SUMMARY.md`
- React Query Docs: https://tanstack.com/query/latest/docs/react/guides/testing
- Testing Library Docs: https://testing-library.com/docs/react-testing-library/intro/
