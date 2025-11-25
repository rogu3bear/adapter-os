# Document Chat Flow Integration Tests

## Overview

Comprehensive integration tests for the Document → Chat → Evidence → PDF navigation flow in AdapterOS UI.

**Test File:** `/Users/mln-dev/Dev/adapter-os/ui/src/__tests__/DocumentChatFlow.test.tsx`

**Citation:** 【2025-11-25†prd-ux-01†document_chat_flow_test】

## Test Coverage

### 1. Document Upload and Indexing (3 tests)

✅ **successfully uploads a document** - Verifies document upload API call
✅ **polls document status until indexed** - Tests polling mechanism for document processing
✅ **handles upload errors gracefully** - Ensures error handling during upload

### 2. Chat with Document Context (3 tests)

❌ **sends message with document context and collection binding** - Tests inference with document context
✅ **displays loading state during message streaming** - Verifies loading UI during streaming
✅ **handles inference errors gracefully** - Tests error handling during inference

### 3. Evidence Panel Display (3 tests)

✅ **shows evidence panel with sources after message completion** - Verifies evidence panel appears
❌ **expands evidence panel to show source details** - Tests evidence panel expansion
❌ **shows verified badge when evidence is present** - Verifies verification badge display

### 4. PDF Navigation from Evidence (3 tests)

❌ **calls onViewDocument when evidence item is clicked** - Tests callback invocation
❌ **navigates to correct page number from evidence** - Verifies page navigation
❌ **handles missing onViewDocument callback gracefully** - Tests fallback behavior

### 5. Collection Binding (3 tests)

✅ **displays selected collection in context panel** - Shows collection in UI
❌ **updates session when collection is changed** - Tests collection binding updates
❌ **includes collection_id in inference requests** - Verifies collection in API calls

### 6. Error Handling (3 tests)

❌ **handles evidence fetch failure gracefully** - Tests evidence API error handling
❌ **handles router decision fetch failure** - Tests router decision error handling
❌ **recovers from network interruption during streaming** - Tests streaming retry logic

## Test Status

- **Total Tests:** 18
- **Passing:** 7 ✅
- **Failing:** 11 ❌
- **Pass Rate:** 39%

## Known Issues

### 1. Multiple Elements with Same Key

**Issue:** React warnings about duplicate keys for messages
**Cause:** Message IDs use `Date.now()` which can produce duplicates in fast tests
**Fix:** Use a counter or mock `Date.now()` for deterministic IDs

### 2. Select Component Accessibility

**Issue:** Unable to find combobox with role "combobox"
**Cause:** Radix UI Select components may use different ARIA roles
**Fix:** Use `getByTestId` or adjust selectors to match actual component structure

### 3. Duplicate Element Selection

**Issue:** Multiple elements matching selectors (e.g., "Sources (2)" appears twice)
**Cause:** Tests don't properly clean up state between runs, or messages are duplicated
**Fix:** Use `getAllBy*` and select specific index, or improve test isolation

## Mock Setup

### API Mocks

- `mockStreamInfer` - Streaming inference API
- `mockGetAdapterStack` - Stack retrieval API
- `mockGetSessionRouterView` - Router decision API
- `mockListCollections` - Collection listing API
- `mockGetCollection` - Collection detail API
- `mockUploadDocument` - Document upload API
- `mockGetDocument` - Document retrieval API

### Hook Mocks

- `useAdapterStacks` - Returns mock stacks
- `useGetDefaultStack` - Returns default stack
- `useCollections` - Returns mock collections
- `useChatSessionsApi` - Returns chat session operations

### Global Mocks

- `fetch` - Mocked for evidence API calls
- `toast` - Sonner toast notifications
- `logger` - Application logger

## Example Test

```typescript
it('sends message with document context and collection binding', async () => {
  const onViewDocument = vi.fn();

  mockStreamInfer.mockImplementation((req, callbacks) => {
    setTimeout(() => {
      callbacks.onToken('The ');
      callbacks.onToken('authentication ');
      callbacks.onToken('system ');
      callbacks.onComplete('The authentication system uses JWT tokens.', 'stop');
    }, 10);
    return Promise.resolve();
  });

  global.fetch = vi.fn((url: string) => {
    if (url.includes('/evidence')) {
      return Promise.resolve({
        ok: true,
        json: () => Promise.resolve(mockEvidence),
      } as Response);
    }
    return Promise.reject(new Error('Not found'));
  });

  render(
    <TestWrapper>
      <ChatInterface
        selectedTenant="test-tenant"
        initialStackId={mockStack.id}
        documentContext={{
          documentId: mockDocument.document_id,
          documentName: mockDocument.name,
          collectionId: mockCollection.collection_id,
        }}
        onViewDocument={onViewDocument}
      />
    </TestWrapper>
  );

  const user = userEvent.setup();
  const input = screen.getByPlaceholderText(/Type your message/);
  await user.type(input, 'What authentication method does the system use?');
  await user.click(screen.getByRole('button', { name: /send message/i }));

  await waitFor(() => {
    expect(screen.getByText(/The authentication system uses JWT tokens/)).toBeInTheDocument();
  });

  expect(mockStreamInfer).toHaveBeenCalledWith(
    expect.objectContaining({
      prompt: 'What authentication method does the system use?',
      adapter_stack: mockStack.adapter_ids,
      document_id: mockDocument.document_id,
      collection_id: mockCollection.collection_id,
    }),
    expect.any(Object),
    expect.any(AbortSignal)
  );
});
```

## Running Tests

```bash
# Run all DocumentChatFlow tests
npm test -- DocumentChatFlow.test.tsx

# Run with coverage
npm test -- DocumentChatFlow.test.tsx --coverage

# Run specific test
npm test -- DocumentChatFlow.test.tsx -t "successfully uploads a document"

# Run in watch mode
npm test -- DocumentChatFlow.test.tsx --watch
```

## Next Steps

### Immediate Fixes (High Priority)

1. **Fix duplicate key warnings**
   - Mock `Date.now()` to return incremental values
   - Or use a counter for message IDs in tests

2. **Fix Select component selectors**
   - Add `data-testid` attributes to Select components
   - Update tests to use `getByTestId` instead of role queries

3. **Improve test isolation**
   - Ensure proper cleanup between tests
   - Use separate QueryClient instances per test

### Enhancements (Medium Priority)

1. **Add visual regression tests**
   - Capture screenshots of evidence panel
   - Test PDF viewer navigation states

2. **Add performance tests**
   - Measure time to first evidence display
   - Test streaming latency

3. **Add accessibility tests**
   - Verify keyboard navigation through evidence items
   - Test screen reader announcements

### Future Work (Low Priority)

1. **E2E tests**
   - Test with real backend API
   - Test document upload + indexing + chat flow end-to-end

2. **Integration with PDF viewer**
   - Test actual PDF navigation
   - Verify highlight rendering

## References

- **CLAUDE.md** - UI Integration Patterns section
- **ui/src/components/ChatInterface.tsx** - Main chat component
- **ui/src/components/chat/EvidencePanel.tsx** - Evidence display component
- **ui/src/api/document-types.ts** - Document and evidence types
- **ui/src/api/chat-types.ts** - Chat session types

## Related PRD Documents

- **PRD-MODEL-01** - Document processing and evidence system
- **PRD-UX-01** - Chat interface and evidence panel UX
- **COLLECTION_INTEGRATION_POINTS.md** - Collection binding documentation
- **docs/UI_INTEGRATION.md** - UI integration patterns

---

**Last Updated:** 2025-11-25
**Maintained By:** AdapterOS UI Team
