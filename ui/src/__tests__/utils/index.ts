/**
 * Test utilities index
 *
 * Central export point for all test utilities, mock factories, and test providers.
 */

// Mock factories
export {
  createMockDocument,
  createMockDocumentMetadata,
  createMockCollection,
  createMockCollectionMetadata,
  createMockEvidence,
  createMockChatSession,
  createMockPolicyCheck,
  createMockPolicyCheckResult,
  createMockDocumentList,
  createMockCollectionList,
  createMockEvidenceList,
  createMockChatSessionList,
  createMockError,
  createMockPaginatedResponse,
} from './mockFactories';

// Test providers
export {
  AllProviders,
  QueryWrapper,
  renderWithProviders,
  renderWithQuery,
  renderWithRoute,
  waitForQueries,
  clearQueryCache,
  mockAuthContext,
  mockRouterContext,
  createRouterEntries,
} from './testProviders';

// Mock API client
export {
  MockApiClient,
  MockApiState,
  createMockApiClient,
  setupMockApiResponses,
  type MockApiConfig,
} from './mockApiClient';
