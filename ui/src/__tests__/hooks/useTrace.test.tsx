import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { useTrace } from '@/hooks/observability/useTrace';
import type { TraceResponseV1 } from '@/api/types';

// Mock API client
const mockGetTrace = vi.fn();

vi.mock('@/api/client', () => ({
  default: {
    getTrace: (...args: unknown[]) => mockGetTrace(...args),
  },
}));

// Test data
const mockTrace: TraceResponseV1 = {
  trace_id: 'trace-123',
  tenant_id: 'tenant-1',
  request_id: 'req-456',
  tokens: [
    {
      token_id: 'tok-1',
      position: 0,
      text: 'Hello',
      adapter_contributions: [
        { adapter_id: 'adapter-1', weight: 0.8 },
        { adapter_id: 'adapter-2', weight: 0.2 },
      ],
    },
    {
      token_id: 'tok-2',
      position: 1,
      text: ' world',
      adapter_contributions: [
        { adapter_id: 'adapter-1', weight: 0.6 },
        { adapter_id: 'adapter-2', weight: 0.4 },
      ],
    },
  ],
  router_decision: {
    selected_adapters: ['adapter-1', 'adapter-2'],
    scores: { 'adapter-1': 0.9, 'adapter-2': 0.7 },
    timestamp: '2025-01-01T00:00:00Z',
  },
  created_at: '2025-01-01T00:00:00Z',
};

// Test wrapper
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

describe('useTrace', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('returns correct initial loading state when enabled', () => {
      mockGetTrace.mockResolvedValue(mockTrace);

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(result.current.data).toBeUndefined();
      expect(result.current.error).toBeNull();
    });

    it('does not fetch when traceId is undefined', () => {
      const { result } = renderHook(() => useTrace(undefined, 'tenant-1'), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockGetTrace).not.toHaveBeenCalled();
    });

    it('does not fetch when traceId is empty string', () => {
      const { result } = renderHook(() => useTrace('', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockGetTrace).not.toHaveBeenCalled();
    });
  });

  describe('successful data fetching', () => {
    it('fetches and returns trace data successfully', async () => {
      mockGetTrace.mockResolvedValue(mockTrace);

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockTrace);
      expect(mockGetTrace).toHaveBeenCalledWith('trace-123', 'tenant-1');
    });

    it('calls API with correct parameters', async () => {
      mockGetTrace.mockResolvedValue(mockTrace);

      const { result } = renderHook(() => useTrace('trace-456', 'tenant-2'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(mockGetTrace).toHaveBeenCalledWith('trace-456', 'tenant-2');
      expect(mockGetTrace).toHaveBeenCalledTimes(1);
    });

    it('handles trace with empty tokens array', async () => {
      const emptyTrace: TraceResponseV1 = {
        ...mockTrace,
        tokens: [],
      };
      mockGetTrace.mockResolvedValue(emptyTrace);

      const { result } = renderHook(() => useTrace('trace-empty', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(emptyTrace);
      expect(result.current.data?.tokens).toHaveLength(0);
    });

    it('handles trace without router_decision', async () => {
      const traceWithoutDecision = {
        ...mockTrace,
        router_decision: undefined,
      };
      mockGetTrace.mockResolvedValue(traceWithoutDecision);

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data?.router_decision).toBeUndefined();
    });
  });

  describe('filtering non-trace responses', () => {
    it('returns null when response does not have tokens field', async () => {
      const invalidResponse = {
        trace_id: 'trace-123',
        tenant_id: 'tenant-1',
        // Missing tokens field
      };
      mockGetTrace.mockResolvedValue(invalidResponse);

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toBeNull();
    });

    it('returns valid trace when response has tokens field', async () => {
      mockGetTrace.mockResolvedValue(mockTrace);

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockTrace);
      expect(result.current.data).toHaveProperty('tokens');
    });

    it('returns null for null API response', async () => {
      mockGetTrace.mockResolvedValue(null);

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toBeNull();
    });

    it('returns null for undefined API response', async () => {
      mockGetTrace.mockResolvedValue(undefined);

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toBeNull();
    });
  });

  describe('error handling', () => {
    it('handles API error correctly', async () => {
      const error = new Error('Failed to fetch trace');
      mockGetTrace.mockRejectedValue(error);

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
      expect(result.current.data).toBeUndefined();
    });

    it('handles trace not found error', async () => {
      const notFoundError = new Error('Trace not found');
      mockGetTrace.mockRejectedValue(notFoundError);

      const { result } = renderHook(() => useTrace('nonexistent', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(notFoundError);
    });

    it('handles network error', async () => {
      const networkError = new Error('Network request failed');
      mockGetTrace.mockRejectedValue(networkError);

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error?.message).toBe('Network request failed');
    });

    it('does not retry on error', async () => {
      const error = new Error('Server error');
      mockGetTrace.mockRejectedValue(error);

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      // retry: false means it should only be called once
      expect(mockGetTrace).toHaveBeenCalledTimes(1);
    });
  });

  describe('query key generation', () => {
    it('generates unique keys for different trace IDs', async () => {
      mockGetTrace.mockResolvedValue(mockTrace);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // First trace
      const { result: result1 } = renderHook(() => useTrace('trace-1', 'tenant-1'), {
        wrapper,
      });

      await waitFor(() => {
        expect(result1.current.isSuccess).toBe(true);
      });

      // Second trace - should be a separate query
      const { result: result2 } = renderHook(() => useTrace('trace-2', 'tenant-1'), {
        wrapper,
      });

      await waitFor(() => {
        expect(result2.current.isSuccess).toBe(true);
      });

      expect(mockGetTrace).toHaveBeenCalledTimes(2);
      expect(mockGetTrace).toHaveBeenCalledWith('trace-1', 'tenant-1');
      expect(mockGetTrace).toHaveBeenCalledWith('trace-2', 'tenant-1');
    });

    it('generates unique keys for different tenant IDs', async () => {
      mockGetTrace.mockResolvedValue(mockTrace);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // Same trace, different tenants
      renderHook(() => useTrace('trace-123', 'tenant-1'), { wrapper });
      await waitFor(() => mockGetTrace.mock.calls.length > 0);

      renderHook(() => useTrace('trace-123', 'tenant-2'), { wrapper });
      await waitFor(() => mockGetTrace.mock.calls.length > 1);

      expect(mockGetTrace).toHaveBeenCalledWith('trace-123', 'tenant-1');
      expect(mockGetTrace).toHaveBeenCalledWith('trace-123', 'tenant-2');
    });
  });

  describe('caching behavior', () => {
    it('uses cached data for same trace ID', async () => {
      mockGetTrace.mockResolvedValue(mockTrace);

      const queryClient = new QueryClient({
        defaultOptions: {
          queries: {
            retry: false,
            staleTime: Infinity, // Keep data fresh
          }
        },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // First render
      const { result: result1, unmount: unmount1 } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper,
      });

      await waitFor(() => {
        expect(result1.current.isSuccess).toBe(true);
      });

      const callsAfterFirst = mockGetTrace.mock.calls.length;

      // Unmount first instance
      unmount1();

      // Second render - should use cache
      const { result: result2 } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper,
      });

      // Should immediately have data from cache
      await waitFor(() => {
        expect(result2.current.data).toEqual(mockTrace);
      });

      // Should not make another API call (or at most 1 more due to revalidation)
      expect(mockGetTrace).toHaveBeenCalledTimes(callsAfterFirst);
    });
  });

  describe('parameter edge cases', () => {
    it('handles missing tenant ID', async () => {
      mockGetTrace.mockResolvedValue(mockTrace);

      const { result } = renderHook(() => useTrace('trace-123', undefined), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(mockGetTrace).toHaveBeenCalledWith('trace-123', undefined);
    });

    it('handles special characters in trace ID', async () => {
      mockGetTrace.mockResolvedValue(mockTrace);

      const specialTraceId = 'trace-123-abc_xyz';
      const { result } = renderHook(() => useTrace(specialTraceId, 'tenant-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(mockGetTrace).toHaveBeenCalledWith(specialTraceId, 'tenant-1');
    });
  });

  describe('loading states', () => {
    it('shows loading state during fetch', async () => {
      mockGetTrace.mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve(mockTrace), 100))
      );

      const { result } = renderHook(() => useTrace('trace-123', 'tenant-1'), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(result.current.isFetching).toBe(true);
      expect(result.current.data).toBeUndefined();

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.isPending).toBe(false);
      expect(result.current.data).toEqual(mockTrace);
    });
  });
});
