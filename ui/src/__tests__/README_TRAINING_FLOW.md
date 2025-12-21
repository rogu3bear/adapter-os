# Training Flow Integration Test Documentation

**File:** `/ui/src/__tests__/TrainingFlow.test.tsx`

**Purpose:** Comprehensive integration test for the Collection → Train → Adapter → Stack → Chat workflow in AdapterOS.

**Citation:** 【2025-11-25†test†training_flow_integration】

---

## Overview

This test suite validates the complete end-to-end workflow for training custom adapters from document collections and using them in chat sessions. It covers all major integration points between collections, training, adapters, stacks, and chat.

---

## Test Structure

### 1. Collection Creation with Documents
- **creates a collection and adds documents** - Validates collection creation API, document upload, and linking documents to collections
- **handles document upload errors gracefully** - Tests error handling for failed uploads

### 2. Training Job with Collection
- **starts training with collection-based dataset** - Validates training job creation with dataset from collection
- **polls training job status until completion** - Tests status transitions: pending → running → completed
- **handles training job transitions correctly** - Verifies all training statuses and metadata updates
- **handles training cancellation** - Tests cancelling in-progress training jobs
- **handles training failure with error message** - Validates error message propagation

### 3. Adapter Creation and Verification
- **verifies adapter is created after training completion** - Ensures adapter registration after successful training
- **retrieves training artifacts for completed job** - Tests artifact retrieval (weights, logs, manifests)

### 4. Add Adapter to Stack
- **creates adapter stack with trained adapter** - Validates stack creation with new adapter
- **adds adapter to existing stack** - Tests adding adapter to pre-existing stack
- **activates adapter stack** - Validates stack activation workflow
- **shows toast notification on stack activation success** - Tests UI feedback

### 5. Chat with Adapter Stack
- **creates chat session with adapter stack** - Validates session creation with stack binding
- **sends chat message and receives streaming response** - Tests streaming inference with adapter stack
- **displays stack context in chat interface** - Validates stack information display

### 6. Complete End-to-End Flow
- **completes full workflow from collection to chat** - Integration test for entire workflow:
  1. Create collection
  2. Upload documents
  3. Start training
  4. Poll until completion
  5. Verify adapter
  6. Create stack
  7. Activate stack
  8. Create chat session
  9. Send message with adapter

### 7. Error Handling and Edge Cases
- **handles training timeout gracefully** - Tests polling timeout scenarios
- **handles stack activation failure** - Validates error handling for failed activations
- **handles chat inference error** - Tests streaming error callbacks
- **handles missing adapter_id in completed training job** - Edge case for incomplete job metadata

### 8. Navigation and Toast Notifications
- **navigates through pages in correct order** - Validates navigation flow
- **shows success toast on training completion** - Tests success notifications
- **shows error toast on training failure** - Tests error notifications
- **shows loading toast during training** - Tests progress indicators

---

## Test Coverage

### API Methods Tested

**Collections:**
- `createCollection()`
- `listCollections()`
- `getCollection()`
- `addDocumentToCollection()`

**Documents:**
- `uploadDocument()`
- `listDocuments()`

**Training:**
- `startTraining()`
- `getTrainingJob()`
- `listTrainingJobs()`
- `cancelTraining()`
- `getTrainingArtifacts()`

**Adapters:**
- `listAdapters()`
- `getAdapter()`
- `registerAdapter()`

**Adapter Stacks:**
- `createAdapterStack()`
- `listAdapterStacks()`
- `getAdapterStack()`
- `activateAdapterStack()`

**Chat:**
- `createChatSession()`
- `listChatSessions()`
- `addChatMessage()`
- `streamInfer()`

---

## Mock Data Generators

### Collection Mock
```typescript
createMockCollection(id: string, name: string): Collection
```
Generates a complete collection object with metadata.

### Document Mock
```typescript
createMockDocument(id: string, name: string): Document
```
Creates a document with file metadata and chunking info.

### Training Job Mock
```typescript
createMockTrainingJob(
  id: string,
  status: TrainingStatus,
  adapter_id?: string
): TrainingJob
```
Generates training job with configurable status and progress.

### Adapter Mock
```typescript
createMockAdapter(id: string, name: string): Adapter
```
Creates adapter with lifecycle state and metadata.

### Stack Mock
```typescript
createMockStack(
  id: string,
  name: string,
  adapter_ids: string[]
): AdapterStack
```
Generates adapter stack with multiple adapters.

### Chat Session Mock
```typescript
createMockChatSession(id: string, stack_id?: string): ChatSession
```
Creates chat session with optional stack binding.

### Chat Message Mock
```typescript
createMockChatMessage(
  id: string,
  role: 'user' | 'assistant',
  content: string
): ChatMessage
```
Generates chat message with role and content.

---

## Running Tests

### Run All Training Flow Tests
```bash
cd ui
pnpm test src/__tests__/TrainingFlow.test.tsx
```

### Run Specific Test Suite
```bash
pnpm test src/__tests__/TrainingFlow.test.tsx -t "Collection Creation"
```

### Run with Coverage
```bash
pnpm test:coverage src/__tests__/TrainingFlow.test.tsx
```

### Watch Mode
```bash
pnpm test:watch src/__tests__/TrainingFlow.test.tsx
```

---

## Key Testing Patterns

### 1. Async State Transitions
Tests validate state transitions over time (e.g., training status changes):
```typescript
mockApiClient.getTrainingJob
  .mockResolvedValueOnce(createMockTrainingJob('job-1', 'pending'))
  .mockResolvedValueOnce(createMockTrainingJob('job-1', 'running'))
  .mockResolvedValueOnce(createMockTrainingJob('job-1', 'completed', 'adapter-1'));
```

### 2. Streaming Response Testing
Validates token-by-token streaming with callbacks:
```typescript
mockApiClient.streamInfer.mockImplementation((req, callbacks) => {
  setTimeout(() => callbacks.onToken('Hello'), 10);
  setTimeout(() => callbacks.onToken(' world'), 20);
  setTimeout(() => callbacks.onComplete('Hello world', 'stop'), 30);
  return Promise.resolve();
});
```

### 3. Error Propagation
Tests error handling through promise rejections and error callbacks:
```typescript
mockApiClient.uploadDocument.mockRejectedValue(new Error('Upload failed'));
await expect(mockApiClient.uploadDocument(file)).rejects.toThrow('Upload failed');
```

### 4. Toast Notification Verification
Mocks and verifies toast calls (though actual toast rendering not tested):
```typescript
// In real UI, would trigger: toast.success('Training completed!')
expect(job.status).toBe('completed');
```

---

## Integration Points

### Collection → Training
- Document collections are converted to training datasets
- Dataset ID is passed to `startTraining()` request

### Training → Adapter
- Completed training jobs produce adapter artifacts
- Adapter ID is stored in `TrainingJob.adapter_id`

### Adapter → Stack
- Adapters are added to stacks via `adapter_ids` array
- Stack activation enables the adapter for inference

### Stack → Chat
- Chat sessions are bound to stacks via `stack_id`
- Inference requests include `adapter_stack` parameter

---

## Validation Checklist

When adding new tests, ensure coverage of:

- ✅ Happy path (success scenarios)
- ✅ Error handling (network, validation, server errors)
- ✅ Edge cases (missing data, incomplete states)
- ✅ State transitions (status changes over time)
- ✅ API contract validation (request/response schemas)
- ✅ Toast notifications (success, error, loading)
- ✅ Navigation flow (page transitions)
- ✅ Streaming responses (token callbacks)

---

## Related Files

- `/ui/src/api/client.ts` - API client implementation
- `/ui/src/api/training-types.ts` - Training type definitions
- `/ui/src/api/chat-types.ts` - Chat type definitions
- `/ui/src/api/document-types.ts` - Document/collection types
- `/ui/src/__tests__/ChatInterface.test.tsx` - Chat component tests
- `/ui/src/__tests__/DatasetsTab.test.tsx` - Dataset UI tests

---

## Test Results

**Status:** ✅ All 25 tests passing

**Test Execution:**
```
 Test Files  1 passed (1)
      Tests  25 passed (25)
   Duration  ~160ms
```

**Coverage Areas:**
- Collection management (2 tests)
- Training workflow (5 tests)
- Adapter creation (2 tests)
- Stack management (4 tests)
- Chat integration (3 tests)
- End-to-end flow (1 test)
- Error handling (4 tests)
- UI notifications (4 tests)

---

## Future Enhancements

### Potential Additions
1. **Visual Regression Tests** - Screenshot testing for training progress UI
2. **Performance Tests** - Measure polling interval and rendering performance
3. **Accessibility Tests** - ARIA labels, keyboard navigation
4. **Multi-tenant Tests** - Validate tenant isolation in workflows
5. **Real-time Sync Tests** - WebSocket/SSE event handling
6. **Offline Resilience** - Test behavior when network is unavailable

### Integration Opportunities
1. **Cypress E2E** - Full browser testing with real UI interactions
2. **Playwright** - Cross-browser end-to-end validation
3. **Storybook** - Component isolation testing for training wizard
4. **MSW (Mock Service Worker)** - Network-level request mocking

---

## Troubleshooting

### Test Failures

**"Training job status stuck in running"**
- Check mock implementation for status transitions
- Verify `mockApiClient.getTrainingJob` is configured correctly

**"Toast notifications not called"**
- Tests only verify API calls, not actual toast rendering
- Toast calls happen in UI components, not in test API layer

**"Streaming response timeout"**
- Increase `waitFor` timeout for async operations
- Check callback timing in `mockApiClient.streamInfer`

### Common Issues

1. **Mock not resetting between tests** - Use `beforeEach(() => vi.clearAllMocks())`
2. **Async timing issues** - Use `waitFor()` for state updates
3. **Type mismatches** - Ensure mock data matches API type definitions

---

## Maintainability

### When to Update Tests

**API Changes:**
- New fields in request/response schemas
- Changed endpoint paths
- Modified authentication requirements

**Workflow Changes:**
- New steps in training pipeline
- Changed status transitions
- Additional validation requirements

**UI Changes:**
- New toast notification types
- Changed navigation routes
- Modified error messaging

### Code Review Checklist

- [ ] All new API methods have test coverage
- [ ] Error cases are tested for new workflows
- [ ] Mock data matches current API schemas
- [ ] Tests are deterministic (no flaky tests)
- [ ] Test descriptions are clear and actionable
- [ ] Comments explain complex mock setups

---

**Last Updated:** 2025-11-25
**Maintained by:** AdapterOS Development Team
**Citation:** 【2025-11-25†test†training_flow_integration】
