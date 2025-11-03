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
    genericError: vi.fn(() => <div>Error Recovery</div>),
  },
}));

import apiClient from '../../api/client';
import { toast } from 'sonner';
import { logger } from '../../utils/logger';
import { ErrorRecoveryTemplates } from '../../components/ui/error-recovery';

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

  describe('evictAdapter', () => {
    it('sets loading state during operation', async () => {
      const mockApiClient = apiClient as any;
      mockApiClient.evictAdapter.mockResolvedValue({ success: true, message: 'Evicted' });

      const { result } = renderHook(() => useAdapterOperations());

      act(() => {
        result.current.evictAdapter('adapter-1');
      });

      expect(result.current.isEvicting).toBe(true);
      expect(result.current.isOperationLoading).toBe(true);

      await act(async () => {
        // Wait for the operation to complete
        await new Promise(resolve => setTimeout(resolve, 0));
      });

      expect(result.current.isEvicting).toBe(false);
      expect(result.current.isOperationLoading).toBe(false);
    });

    it('calls API client with correct parameters', async () => {
      const mockApiClient = apiClient as any;
      mockApiClient.evictAdapter.mockResolvedValue({ success: true, message: 'Evicted' });

      const { result } = renderHook(() => useAdapterOperations());

      await act(async () => {
        await result.current.evictAdapter('adapter-1');
      });

      expect(mockApiClient.evictAdapter).toHaveBeenCalledWith('adapter-1');
    });

    it('shows success toast on successful operation', async () => {
      const mockApiClient = apiClient as any;
      mockApiClient.evictAdapter.mockResolvedValue({ success: true, message: 'Evicted' });

      const { result } = renderHook(() => useAdapterOperations());

      await act(async () => {
        await result.current.evictAdapter('adapter-1');
      });

      expect(toast.success).toHaveBeenCalledWith('Adapter evicted successfully');
    });

    it('calls onAdapterEvict callback when provided', async () => {
      const mockApiClient = apiClient as any;
      mockApiClient.evictAdapter.mockResolvedValue({ success: true, message: 'Evicted' });

      const onAdapterEvict = vi.fn();
      const { result } = renderHook(() =>
        useAdapterOperations({ onAdapterEvict })
      );

      await act(async () => {
        await result.current.evictAdapter('adapter-1');
      });

      expect(onAdapterEvict).toHaveBeenCalledWith('adapter-1');
    });

    it('handles errors gracefully', async () => {
      const mockApiClient = apiClient as any;
      const error = new Error('API Error');
      mockApiClient.evictAdapter.mockRejectedValue(error);

      const { result } = renderHook(() => useAdapterOperations());

      await act(async () => {
        await result.current.evictAdapter('adapter-1');
      });

      expect(result.current.operationError).not.toBeNull();
      expect(logger.error).toHaveBeenCalled();
    });
  });

  describe('pinAdapter', () => {
    it('calls API client with correct parameters for pinning', async () => {
      const mockApiClient = apiClient as any;
      mockApiClient.pinAdapter.mockResolvedValue(undefined);

      const { result } = renderHook(() => useAdapterOperations());

      await act(async () => {
        await result.current.pinAdapter('adapter-1', true);
      });

      expect(mockApiClient.pinAdapter).toHaveBeenCalledWith('adapter-1', true);
    });

    it('shows appropriate success message for pinning', async () => {
      const mockApiClient = apiClient as any;
      mockApiClient.pinAdapter.mockResolvedValue(undefined);

      const { result } = renderHook(() => useAdapterOperations());

      await act(async () => {
        await result.current.pinAdapter('adapter-1', true);
      });

      expect(toast.success).toHaveBeenCalledWith('Adapter pinned successfully');
    });

    it('shows appropriate success message for unpinning', async () => {
      const mockApiClient = apiClient as any;
      mockApiClient.pinAdapter.mockResolvedValue(undefined);

      const { result } = renderHook(() => useAdapterOperations());

      await act(async () => {
        await result.current.pinAdapter('adapter-1', false);
      });

      expect(toast.success).toHaveBeenCalledWith('Adapter unpinned successfully');
    });
  });

  describe('deleteAdapter', () => {
    it('calls API client with correct parameters', async () => {
      const mockApiClient = apiClient as any;
      mockApiClient.deleteAdapter.mockResolvedValue(undefined);

      const { result } = renderHook(() => useAdapterOperations());

      await act(async () => {
        await result.current.deleteAdapter('adapter-1');
      });

      expect(mockApiClient.deleteAdapter).toHaveBeenCalledWith('adapter-1');
    });

    it('shows success message on deletion', async () => {
      const mockApiClient = apiClient as any;
      mockApiClient.deleteAdapter.mockResolvedValue(undefined);

      const { result } = renderHook(() => useAdapterOperations());

      await act(async () => {
        await result.current.deleteAdapter('adapter-1');
      });

      expect(toast.success).toHaveBeenCalledWith('Adapter deleted successfully');
    });
  });

  describe('updateCategoryPolicy', () => {
    it('calls API client with correct parameters', async () => {
      const mockApiClient = apiClient as any;
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
      mockApiClient.updateCategoryPolicy.mockResolvedValue(mockPolicy);

      const { result } = renderHook(() => useAdapterOperations());

      await act(async () => {
        await result.current.updateCategoryPolicy('code', mockPolicy);
      });

      expect(mockApiClient.updateCategoryPolicy).toHaveBeenCalledWith('code', mockPolicy);
    });

    it('shows success message with category name', async () => {
      const mockApiClient = apiClient as any;
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
      mockApiClient.updateCategoryPolicy.mockResolvedValue(mockPolicy);

      const { result } = renderHook(() => useAdapterOperations());

      await act(async () => {
        await result.current.updateCategoryPolicy('code', mockPolicy);
      });

      expect(toast.success).toHaveBeenCalledWith('Policy updated successfully for code');
    });
  });

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
