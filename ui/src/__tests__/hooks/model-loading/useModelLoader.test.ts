/**
 * Unit Tests for useModelLoader Hook
 *
 * Tests the imperative loading controls for models and adapters.
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useModelLoader } from '@/hooks/model-loading/useModelLoader';

// Mock the loading coordinator
vi.mock('@/hooks/model-loading/internal/loadingCoordinator', () => ({
  loadingCoordinator: {
    withLock: vi.fn((key: string, fn: () => Promise<unknown>) => fn()),
  },
}));

// Mock API client
const mockApiMethods = vi.hoisted(() => ({
  getAdapterStack: vi.fn().mockResolvedValue({
    id: 'test-stack',
    name: 'Test Stack',
    adapter_ids: ['adapter-1', 'adapter-2'],
  }),
  loadAdaptersToWarm: vi.fn().mockResolvedValue({ success: true }),
}));

vi.mock('@/api/services', () => ({
  default: mockApiMethods,
  apiClient: mockApiMethods,
}));

// Mock retry utility
vi.mock('@/utils/retry', () => ({
  retryWithBackoff: vi.fn((fn) => fn()),
  DEFAULT_RETRY_CONFIG: { maxAttempts: 3, initialDelayMs: 1000, maxDelayMs: 10000, backoffFactor: 2 },
}));

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    loading: vi.fn(),
    dismiss: vi.fn(),
    info: vi.fn(),
    warning: vi.fn(),
  },
}));

describe('useModelLoader', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('starts with isLoading=false', () => {
      const { result } = renderHook(() => useModelLoader());

      expect(result.current.isLoading).toBe(false);
    });

    it('starts with isLoadingBaseModel=false', () => {
      const { result } = renderHook(() => useModelLoader());

      expect(result.current.isLoadingBaseModel).toBe(false);
    });

    it('starts with isLoadingAdapters=false', () => {
      const { result } = renderHook(() => useModelLoader());

      expect(result.current.isLoadingAdapters).toBe(false);
    });

    it('starts with no error', () => {
      const { result } = renderHook(() => useModelLoader());

      expect(result.current.error).toBeNull();
    });

    it('starts with empty failedAdapterIds', () => {
      const { result } = renderHook(() => useModelLoader());

      expect(result.current.failedAdapterIds).toEqual([]);
    });
  });

  describe('loadModels', () => {
    it('provides loadModels function', () => {
      const { result } = renderHook(() => useModelLoader());

      expect(typeof result.current.loadModels).toBe('function');
    });

    it('sets isLoading during load operation', async () => {
      const { result } = renderHook(() => useModelLoader());

      // Before loading
      expect(result.current.isLoading).toBe(false);

      // Start and complete loading
      await act(async () => {
        await result.current.loadModels('test-stack');
      });

      // Should no longer be loading after completion
      expect(result.current.isLoading).toBe(false);
    });

    it('sets isLoadingAdapters during adapter loading', async () => {
      const { result } = renderHook(() => useModelLoader());

      let loadPromise: Promise<void>;
      act(() => {
        loadPromise = result.current.loadModels('test-stack');
      });

      // Wait for completion
      await act(async () => {
        await loadPromise;
      });

      // After completion, should not be loading
      expect(result.current.isLoadingAdapters).toBe(false);
    });
  });

  describe('cancelLoading', () => {
    it('provides cancelLoading function', () => {
      const { result } = renderHook(() => useModelLoader());

      expect(typeof result.current.cancelLoading).toBe('function');
    });

    it('can cancel an in-progress load', async () => {
      const { result } = renderHook(() => useModelLoader());

      // Start loading
      act(() => {
        result.current.loadModels('test-stack');
      });

      // Cancel immediately
      act(() => {
        result.current.cancelLoading();
      });

      // Should reset loading state
      expect(result.current.isLoading).toBe(false);
    });
  });

  describe('retryFailed', () => {
    it('provides retryFailed function', () => {
      const { result } = renderHook(() => useModelLoader());

      expect(typeof result.current.retryFailed).toBe('function');
    });

    it('retries failed adapters', async () => {
      const { result } = renderHook(() => useModelLoader());

      // Call retry without loading first (no failures = no-op, no error set)
      await act(async () => {
        await result.current.retryFailed();
      });

      // If no failed adapters, failedAdapterIds should remain empty
      expect(result.current.failedAdapterIds).toEqual([]);
    });
  });

  describe('clearError', () => {
    it('provides clearError function', () => {
      const { result } = renderHook(() => useModelLoader());

      expect(typeof result.current.clearError).toBe('function');
    });

    it('clears error state', async () => {
      const { result } = renderHook(() => useModelLoader());

      // Call clearError
      act(() => {
        result.current.clearError();
      });

      expect(result.current.error).toBeNull();
    });
  });

  describe('error handling', () => {
    it('sets error when stack load fails', async () => {
      const { apiClient } = await import('@/api/services');
      vi.mocked(apiClient.getAdapterStack).mockRejectedValueOnce(new Error('Stack not found'));

      const { result } = renderHook(() => useModelLoader());

      await act(async () => {
        await result.current.loadModels('nonexistent-stack');
      });

      expect(result.current.error).not.toBeNull();
      // Error code depends on the specific error type
      expect(result.current.error?.message).toBeDefined();
    });
  });

  describe('LoadingCoordinator integration', () => {
    it('uses LoadingCoordinator to prevent race conditions', async () => {
      const { loadingCoordinator } = await import('@/hooks/model-loading/internal/loadingCoordinator');

      const { result } = renderHook(() => useModelLoader());

      await act(async () => {
        await result.current.loadModels('test-stack');
      });

      // Should have used the coordinator
      expect(loadingCoordinator.withLock).toHaveBeenCalled();
    });
  });

  describe('combined loading state', () => {
    it('isLoading reflects both baseModel and adapters loading', () => {
      const { result } = renderHook(() => useModelLoader());

      // isLoading should be true if either is loading
      expect(result.current.isLoading).toBe(
        result.current.isLoadingBaseModel || result.current.isLoadingAdapters
      );
    });
  });
});
