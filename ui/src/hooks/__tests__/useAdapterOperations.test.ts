import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useAdapterOperations } from '../useAdapterOperations';

// Mock the API client
vi.mock('../../api/client', () => ({
  default: {
    evictAdapter: vi.fn(),
    pinAdapter: vi.fn(),
    promoteAdapterState: vi.fn(),
    deleteAdapter: vi.fn(),
    updateCategoryPolicy: vi.fn(),
  },
}));

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

// Mock logger
vi.mock('../../utils/logger', () => ({
  logger: {
    error: vi.fn(),
  },
}));

// Mock ErrorRecoveryTemplates
vi.mock('../../components/ui/error-recovery', () => ({
  ErrorRecoveryTemplates: {
    genericError: vi.fn(() => 'Error Recovery'),
  },
}));

import apiClient from '../../api/client';
import { toast } from 'sonner';
import { logger } from '../../utils/logger';
import { ErrorRecoveryTemplates } from '../../components/ui/error-recovery';

// Add helper function after imports and mocks, before describe
const testOperation = async (
  operationKey: string,
  args: any[],
  apiCall?: (mockApi: any, key: string) => void,
  successMessage: string,
  loadingState: string,
  options?: { withCallbacks?: boolean }
) => {
  const mockApiClient = apiClient as any;
  const apiMethod = operationKey.toLowerCase().replace('adapter', '');
  const successResponse = operationKey === 'updateCategoryPolicy' ? mockPolicy : { success: true, message: 'Success' };
  mockApiClient[apiMethod].mockResolvedValue(successResponse);

  const hookOptions = options?.withCallbacks ? { onDataRefresh: vi.fn(), onAdapterEvict: vi.fn() } : {};
  const { result } = renderHook(() => useAdapterOperations(hookOptions));

  await act(async () => {
    await result.current[operationKey as keyof ReturnType<typeof useAdapterOperations>](...args);
  });

  if (apiCall) {
    apiCall(mockApiClient, apiMethod);
  }
  expect(toast.success).toHaveBeenCalledWith(successMessage);
  expect(result.current[loadingState as keyof ReturnType<typeof useAdapterOperations>]).toBe(false);
  expect(result.current.isOperationLoading).toBe(false);
};

// Update the mockPolicy to be global or inside
const mockPolicy = {
  promotion_threshold_ms: 1800000,
  demotion_threshold_ms: 86400000,
  memory_limit: 200 * 1024 * 1024,
  eviction_priority: 'low' as const,
  auto_promote: true,
  auto_demote: false,
  max_in_memory: 10,
  routing_priority: 1.2,
};

describe('useAdapterOperations', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('returns correct initial state', () => {
      const { result } = renderHook(() => useAdapterOperations());

      expect(result.current.isOperationLoading).toBe(false);
      expect(result.current.operationError).toBeNull();
      expect(result.current.isEvicting).toBe(false);
      expect(result.current.isPinning).toBe(false);
      expect(result.current.isPromoting).toBe(false);
      expect(result.current.isDeleting).toBe(false);
      expect(result.current.isUpdatingPolicy).toBe(false);
    });

    it('returns all operation functions', () => {
      const { result } = renderHook(() => useAdapterOperations());

      expect(typeof result.current.evictAdapter).toBe('function');
      expect(typeof result.current.pinAdapter).toBe('function');
      expect(typeof result.current.promoteAdapter).toBe('function');
      expect(typeof result.current.deleteAdapter).toBe('function');
      expect(typeof result.current.updateCategoryPolicy).toBe('function');
    });
  });

  // For evictAdapter
  describe('evictAdapter', () => {
    it('sets loading state during operation', async () => {
      const { result } = renderHook(() => useAdapterOperations());

      act(() => {
        result.current.evictAdapter('adapter-1');
      });

      expect(result.current.isEvicting).toBe(true);
      expect(result.current.isOperationLoading).toBe(true);

      await act(async () => {
        await new Promise(resolve => setTimeout(resolve, 0));
      });

      expect(result.current.isEvicting).toBe(false);
      expect(result.current.isOperationLoading).toBe(false);
    });

    it('calls API client with correct parameters', async () => {
      await testOperation('evictAdapter', ['adapter-1'], (mock, key) => expect(mock[key]).toHaveBeenCalledWith('adapter-1'), 'Adapter evicted successfully', 'isEvicting');
    });

    it('shows success toast on successful operation', async () => {
      await testOperation('evictAdapter', ['adapter-1'], undefined, 'Adapter evicted successfully', 'isEvicting');
    });

    // Keep unique tests as is
    it('calls onAdapterEvict callback when provided', async () => {
      const onAdapterEvict = vi.fn();
      const { result } = renderHook(() => useAdapterOperations({ onAdapterEvict }));
      await act(async () => {
        await result.current.evictAdapter('adapter-1');
      });
      expect(onAdapterEvict).toHaveBeenCalledWith('adapter-1');
    });

    it('handles errors gracefully', async () => {
      const error = new Error('API Error');
      const mockApiClient = apiClient as any;
      mockApiClient.evictAdapter.mockRejectedValue(error);
      const { result } = renderHook(() => useAdapterOperations());
      await act(async () => {
        await result.current.evictAdapter('adapter-1');
      });
      expect(result.current.operationError).not.toBeNull();
      expect(logger.error).toHaveBeenCalled();
    });
  });

  // For pinAdapter
  describe('pinAdapter', () => {
    it('calls API client with correct parameters for pinning', async () => {
      await testOperation('pinAdapter', ['adapter-1', true], (mock, key) => expect(mock[key]).toHaveBeenCalledWith('adapter-1', true), 'Adapter pinned successfully', 'isPinning');
    });

    it('shows appropriate success message for pinning', async () => {
      await testOperation('pinAdapter', ['adapter-1', true], undefined, 'Adapter pinned successfully', 'isPinning');
    });

    it('shows appropriate success message for unpinning', async () => {
      await testOperation('pinAdapter', ['adapter-1', false], undefined, 'Adapter unpinned successfully', 'isPinning');
    });
  });

  // For deleteAdapter
  describe('deleteAdapter', () => {
    it('calls API client with correct parameters', async () => {
      await testOperation('deleteAdapter', ['adapter-1'], (mock, key) => expect(mock[key]).toHaveBeenCalledWith('adapter-1'), 'Adapter deleted successfully', 'isDeleting');
    });

    it('shows success message on deletion', async () => {
      await testOperation('deleteAdapter', ['adapter-1'], undefined, 'Adapter deleted successfully', 'isDeleting');
    });
  });

  // For updateCategoryPolicy
  describe('updateCategoryPolicy', () => {
    it('calls API client with correct parameters', async () => {
      await testOperation('updateCategoryPolicy', ['code', mockPolicy], (mock, key) => expect(mock[key]).toHaveBeenCalledWith('code', mockPolicy), 'Policy updated successfully for code', 'isUpdatingPolicy');
    });

    it('shows success message with category name', async () => {
      await testOperation('updateCategoryPolicy', ['code', mockPolicy], undefined, 'Policy updated successfully for code', 'isUpdatingPolicy');
    });
  });

  // The onDataRefresh test remains as is
  describe('onDataRefresh callback', () => {
    it('calls onDataRefresh callback when provided', async () => {
      const mockApiClient = apiClient as any;
      mockApiClient.evictAdapter.mockResolvedValue({ success: true, message: 'Evicted' });

      const onDataRefresh = vi.fn().mockResolvedValue(undefined);
      const { result } = renderHook(() =>
        useAdapterOperations({ onDataRefresh })
      );

      await act(async () => {
        await result.current.evictAdapter('adapter-1');
      });

      expect(onDataRefresh).toHaveBeenCalled();
    });
  });
});
