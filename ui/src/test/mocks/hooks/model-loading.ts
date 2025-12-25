/**
 * Model Loading Hook Mock Factories
 *
 * Factory functions for creating mock useModelLoadingState and related hook return values.
 */

import { vi, type Mock } from 'vitest';
import type {
  UseModelLoadingStateResult,
  AdapterLoadingItem,
  ModelStatusState,
  ChatLoadingError,
} from '@/hooks/model-loading/types';
import { createMockAdapterLoadingItem } from '@/test/mocks/data/adapters';

/**
 * Options for createUseModelLoadingStateMock factory
 */
export interface UseModelLoadingStateMockOptions {
  /** Overall ready state (default: true) */
  isReady?: boolean;
  /** Loading state (default: false) */
  isLoading?: boolean;
  /** Base model ready (default: true) */
  baseModelReady?: boolean;
  /** All adapters ready (default: true) */
  allAdaptersReady?: boolean;
  /** Progress percentage 0-100 (default: computed from ready state) */
  progress?: number;
  /** Estimated time remaining in seconds (default: null) */
  estimatedTimeRemaining?: number | null;
  /** Adapters currently loading */
  loadingAdapters?: Partial<AdapterLoadingItem>[];
  /** Adapters that are ready */
  readyAdapters?: Partial<AdapterLoadingItem>[];
  /** Adapters that failed to load */
  failedAdapters?: Partial<AdapterLoadingItem>[];
  /** Base model status (default: 'ready') */
  baseModelStatus?: ModelStatusState;
  /** Base model name (default: 'Test Model') */
  baseModelName?: string | null;
  /** Base model ID (default: 'model-1') */
  baseModelId?: string | null;
  /** Base model memory usage in MB (default: 4096) */
  baseModelMemoryMb?: number | null;
  /** Base model error message (default: null) */
  baseModelError?: string | null;
  /** SSE connection status (default: true) */
  sseConnected?: boolean;
  /** Combined error (default: null) */
  error?: ChatLoadingError | null;
}

/**
 * Create a mock return value for useModelLoadingState hook
 *
 * @example
 * ```typescript
 * // Default: ready state
 * const ready = createUseModelLoadingStateMock();
 *
 * // Loading state
 * const loading = createUseModelLoadingStateMock({
 *   isReady: false,
 *   isLoading: true,
 *   progress: 50,
 *   loadingAdapters: [{ adapterId: 'adapter-1', name: 'Loading Adapter' }],
 * });
 *
 * // Error state
 * const error = createUseModelLoadingStateMock({
 *   isReady: false,
 *   baseModelStatus: 'error',
 *   baseModelError: 'Failed to load model',
 * });
 *
 * // With ready adapters
 * const withAdapters = createUseModelLoadingStateMock({
 *   readyAdapters: [
 *     { adapterId: 'adapter-1', name: 'Code Review' },
 *     { adapterId: 'adapter-2', name: 'Writing' },
 *   ],
 * });
 * ```
 */
export function createUseModelLoadingStateMock(
  options: UseModelLoadingStateMockOptions = {}
): UseModelLoadingStateResult {
  // Process adapter arrays
  const loadingAdapters = (options.loadingAdapters ?? []).map((a, i) =>
    createMockAdapterLoadingItem({
      adapterId: `loading-adapter-${i + 1}`,
      state: 'cold',
      isLoading: true,
      isReady: false,
      ...a,
    })
  );

  const readyAdapters = (options.readyAdapters ?? []).map((a, i) =>
    createMockAdapterLoadingItem({
      adapterId: `ready-adapter-${i + 1}`,
      state: 'warm',
      isLoading: false,
      isReady: true,
      ...a,
    })
  );

  const failedAdapters = (options.failedAdapters ?? []).map((a, i) =>
    createMockAdapterLoadingItem({
      adapterId: `failed-adapter-${i + 1}`,
      state: 'unloaded',
      isLoading: false,
      hasError: true,
      isReady: false,
      errorMessage: 'Load failed',
      ...a,
    })
  );

  // Build adapter states map
  const adapterStates = new Map<string, AdapterLoadingItem>();
  [...loadingAdapters, ...readyAdapters, ...failedAdapters].forEach((a) => {
    adapterStates.set(a.adapterId, a);
  });

  // Compute derived state
  const baseModelReady = options.baseModelReady ?? true;
  const allAdaptersReady = options.allAdaptersReady ?? (loadingAdapters.length === 0 && failedAdapters.length === 0);
  const isReady = options.isReady ?? (baseModelReady && allAdaptersReady);
  const isLoading = options.isLoading ?? false;
  const progress = options.progress ?? (isReady ? 100 : 0);

  // Mock functions
  const refetch = vi.fn().mockResolvedValue(undefined);
  const refreshBaseModel = vi.fn().mockResolvedValue(undefined);
  const refreshAdapters = vi.fn().mockResolvedValue(undefined);
  const refreshAll = vi.fn().mockResolvedValue(undefined);

  return {
    // Primary properties
    isReady,
    isLoading,
    loadingAdapters,
    readyAdapters,
    failedAdapters,
    progress,
    estimatedTimeRemaining: options.estimatedTimeRemaining ?? null,

    // Base model info
    baseModel: {
      status: options.baseModelStatus ?? 'ready',
      modelName: options.baseModelName ?? 'Test Model',
      modelId: options.baseModelId ?? 'model-1',
      memoryUsageMb: options.baseModelMemoryMb ?? 4096,
      errorMessage: options.baseModelError ?? null,
    },

    // SSE status
    sseConnected: options.sseConnected ?? true,

    // Error
    error: options.error ?? null,

    // Actions
    refetch,

    // Backwards compatibility
    overallReady: isReady,
    baseModelReady,
    allAdaptersReady,
    adapterStates,
    unreadyAdapters: loadingAdapters.map((a) => a.adapterId),
    loadingAdapterCount: loadingAdapters.length,
    errorAdapterCount: failedAdapters.length,
    baseModelStatus: options.baseModelStatus ?? 'ready',
    baseModelId: options.baseModelId ?? 'model-1',
    baseModelName: options.baseModelName ?? 'Test Model',
    baseModelMemoryMb: options.baseModelMemoryMb ?? 4096,
    baseModelError: options.baseModelError ?? null,
    isConnected: options.sseConnected ?? true,
    refreshBaseModel,
    refreshAdapters,
    refreshAll,
  };
}

/**
 * Return type for useModelLoader hook mock
 */
export interface UseModelLoaderMockReturn {
  loadModels: Mock<() => Promise<void>>;
  retryFailed: Mock<() => Promise<void>>;
  cancelLoading: Mock<() => void>;
}

/**
 * Create a mock return value for useModelLoader hook
 */
export function createUseModelLoaderMock(): UseModelLoaderMockReturn {
  return {
    loadModels: vi.fn().mockResolvedValue(undefined),
    retryFailed: vi.fn().mockResolvedValue(undefined),
    cancelLoading: vi.fn(),
  };
}

/**
 * Return type for useChatLoadingPersistence hook mock
 */
export interface UseChatLoadingPersistenceMockReturn {
  persistedState: null;
  persist: Mock<() => void>;
  clear: Mock<() => void>;
  isRecoverable: boolean;
}

/**
 * Create a mock return value for useChatLoadingPersistence hook
 */
export function createUseChatLoadingPersistenceMock(): UseChatLoadingPersistenceMockReturn {
  return {
    persistedState: null,
    persist: vi.fn(),
    clear: vi.fn(),
    isRecoverable: false,
  };
}

/**
 * Return type for useLoadingAnnouncements hook mock
 */
export interface UseLoadingAnnouncementsMockReturn {
  announcement: string | null;
}

/**
 * Create a mock return value for useLoadingAnnouncements hook
 */
export function createUseLoadingAnnouncementsMock(
  announcement: string | null = null
): UseLoadingAnnouncementsMockReturn {
  return { announcement };
}
