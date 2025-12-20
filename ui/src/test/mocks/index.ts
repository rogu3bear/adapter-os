/**
 * Centralized Mock Factories for Testing
 *
 * Provides type-safe mock factories for React hooks and data objects
 * used across the AdapterOS UI test suite.
 *
 * @example
 * ```typescript
 * import { createUseAuthMock, mockUseAuth, setupChatTestMocks } from '@/test/mocks';
 *
 * // Use factories directly for custom mocks
 * const authMock = createUseAuthMock({ user: { role: 'viewer' } });
 *
 * // Use appliers to set up vi.mock calls
 * mockUseAuth({ user: { tenant_id: 'custom-tenant' } });
 *
 * // Use presets for common test scenarios
 * const mocks = setupChatTestMocks({ auth: { user: { role: 'admin' } } });
 * ```
 */

// Data factories
export * from './data';

// Hook return value factories
export * from './hooks';

// Mock appliers (vi.mock wrappers)
export * from './appliers';

// Test presets (composite mocks)
export * from './presets';
