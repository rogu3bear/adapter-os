/**
 * Chat Test Preset
 *
 * Provides all mock factories needed for ChatInterface and related component tests.
 *
 * IMPORTANT: Due to Vitest's vi.mock() hoisting, you cannot call vi.mock()
 * from within a function. Instead, this preset provides factory functions
 * and a convenience export for common patterns.
 *
 * @example
 * ```typescript
 * // At top of test file, import the factories
 * import {
 *   createChatTestMocks,
 *   chatTestViMocks,
 * } from '@/test/mocks/presets/chat-test';
 *
 * // Create mocks at module scope
 * const mocks = createChatTestMocks({
 *   auth: { user: { tenant_id: 'test-tenant' } },
 * });
 *
 * // Apply vi.mock calls (must be at module scope)
 * vi.mock('@/providers/CoreProviders', () => ({
 *   useAuth: () => mocks.auth.current,
 *   useResize: () => ({ getLayout: vi.fn(() => null), setLayout: vi.fn() }),
 *   TENANT_SELECTION_REQUIRED_KEY: 'aos-tenant-selection-required',
 * }));
 *
 * vi.mock('@/hooks/model-loading', () => ({
 *   useModelLoadingState: () => mocks.modelLoading,
 *   useModelLoader: () => mocks.modelLoader,
 *   useChatLoadingPersistence: () => mocks.loadingPersistence,
 *   useLoadingAnnouncements: () => mocks.loadingAnnouncements,
 * }));
 *
 * // ... more vi.mock calls as needed
 *
 * // In tests
 * beforeEach(() => {
 *   mocks.auth.reset();
 *   vi.clearAllMocks();
 * });
 * ```
 */

import { vi } from 'vitest';
import {
  createMutableAuthState,
  type UseAuthMockOptions,
} from '@/test/mocks/appliers/auth';
import {
  createUseModelLoadingStateMock,
  createUseModelLoaderMock,
  createUseChatLoadingPersistenceMock,
  createUseLoadingAnnouncementsMock,
  type UseModelLoadingStateMockOptions,
} from '@/test/mocks/hooks/model-loading';
import {
  createUseChatSessionsApiMock,
  createUseChatStreamingMock,
  createUseChatAdapterStateMock,
  createUseChatRouterDecisionsMock,
  createUseSessionManagerMock,
  createUseChatModalsMock,
  type UseChatSessionsApiMockOptions,
  type UseChatStreamingMockOptions,
  type UseChatAdapterStateMockOptions,
  type UseChatModalsMockOptions,
} from '@/test/mocks/hooks/chat';

/**
 * Options for createChatTestMocks
 */
export interface ChatTestMocksOptions {
  /** Auth mock options */
  auth?: UseAuthMockOptions;
  /** Model loading mock options */
  modelLoading?: UseModelLoadingStateMockOptions;
  /** Chat sessions API options */
  sessionsApi?: UseChatSessionsApiMockOptions;
  /** Chat streaming options */
  streaming?: UseChatStreamingMockOptions;
  /** Adapter state options */
  adapterState?: UseChatAdapterStateMockOptions;
  /** Chat modals options */
  modals?: UseChatModalsMockOptions;
}

/**
 * Create all mock instances needed for chat component tests
 *
 * Call this at module scope in your test file, then use the returned
 * objects with vi.mock() calls.
 *
 * @example
 * ```typescript
 * const mocks = createChatTestMocks({ auth: { user: { role: 'viewer' } } });
 *
 * vi.mock('@/providers/CoreProviders', () => ({
 *   useAuth: () => mocks.auth.current,
 *   // ...
 * }));
 * ```
 */
export function createChatTestMocks(options: ChatTestMocksOptions = {}) {
  return {
    // Auth with mutable state for per-test overrides
    auth: createMutableAuthState(options.auth),

    // Model loading hooks
    modelLoading: createUseModelLoadingStateMock(options.modelLoading),
    modelLoader: createUseModelLoaderMock(),
    loadingPersistence: createUseChatLoadingPersistenceMock(),
    loadingAnnouncements: createUseLoadingAnnouncementsMock(),

    // Chat hooks
    sessionsApi: createUseChatSessionsApiMock(options.sessionsApi),
    streaming: createUseChatStreamingMock(options.streaming),
    adapterState: createUseChatAdapterStateMock(options.adapterState),
    routerDecisions: createUseChatRouterDecisionsMock(),
    sessionManager: createUseSessionManagerMock(),
    modals: createUseChatModalsMock(options.modals),
  };
}

/**
 * Convenience vi.mock() factory generators for chat tests
 *
 * Use these with vi.mock() at module scope.
 *
 * @example
 * ```typescript
 * const mocks = createChatTestMocks();
 *
 * vi.mock('@/providers/CoreProviders', chatTestViMocks.coreProviders(mocks));
 * vi.mock('@/hooks/model-loading', chatTestViMocks.modelLoading(mocks));
 * vi.mock('@/hooks/chat', chatTestViMocks.chatHooks(mocks));
 * ```
 */
export const chatTestViMocks = {
  coreProviders: (mocks: ReturnType<typeof createChatTestMocks>) => () => ({
    useAuth: () => mocks.auth.current,
    useResize: () => ({
      getLayout: vi.fn(() => null),
      setLayout: vi.fn(),
    }),
    TENANT_SELECTION_REQUIRED_KEY: 'aos-tenant-selection-required',
  }),

  modelLoading: (mocks: ReturnType<typeof createChatTestMocks>) => () => ({
    useModelLoadingState: () => mocks.modelLoading,
    useModelLoader: () => mocks.modelLoader,
    useChatLoadingPersistence: () => mocks.loadingPersistence,
    useLoadingAnnouncements: () => mocks.loadingAnnouncements,
  }),

  chatSessionsApi: (mocks: ReturnType<typeof createChatTestMocks>) => () => ({
    useChatSessionsApi: () => mocks.sessionsApi,
  }),

  chatHooks: (mocks: ReturnType<typeof createChatTestMocks>) => () => ({
    useChatStreaming: () => mocks.streaming,
    useChatAdapterState: () => mocks.adapterState,
    useChatRouterDecisions: () => mocks.routerDecisions,
    useSessionManager: () => mocks.sessionManager,
    useChatModals: () => mocks.modals,
  }),

  logger: () => () => ({
    logger: {
      error: vi.fn(),
      warn: vi.fn(),
      info: vi.fn(),
      debug: vi.fn(),
    },
    toError: (error: unknown) => (error instanceof Error ? error : new Error(String(error))),
  }),

  toast: () => () => ({
    toast: {
      success: vi.fn(),
      error: vi.fn(),
      info: vi.fn(),
      warning: vi.fn(),
      loading: vi.fn(),
      dismiss: vi.fn(),
    },
  }),
};

// Re-export types for convenience
export type { UseAuthMockOptions } from '@/test/mocks/appliers/auth';
export type { UseModelLoadingStateMockOptions } from '@/test/mocks/hooks/model-loading';
export type {
  UseChatSessionsApiMockOptions,
  UseChatStreamingMockOptions,
} from '@/test/mocks/hooks/chat';
