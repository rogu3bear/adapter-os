import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type { Adapter, AdapterCategory, AdapterState } from '@/api/adapter-types';
import type { SystemMetrics } from '@/api/types';
import { logger } from '@/utils/logger';
import { toast } from 'sonner';

// Query key constants for cache management
export const ADAPTER_QUERY_KEYS = {
  all: ['adapters'] as const,
  list: (filters?: AdapterFilters) => ['adapters', 'list', filters] as const,
  detail: (id: string) => ['adapters', 'detail', id] as const,
  health: (id: string) => ['adapters', 'health', id] as const,
  metrics: () => ['adapters', 'metrics'] as const,
  systemMetrics: () => ['system', 'metrics'] as const,
};

export interface AdapterFilters {
  status?: AdapterState[];
  tier?: string[];
  tenant?: string;
  category?: AdapterCategory[];
  search?: string;
  pinned?: boolean;
}

export interface AdaptersData {
  adapters: Adapter[];
  totalMemory: number;
  systemMetrics: SystemMetrics | null;
}

// Main hook for fetching adapters list with filters
export function useAdapters(filters?: AdapterFilters) {
  const queryClient = useQueryClient();

  const query = useQuery({
    queryKey: ADAPTER_QUERY_KEYS.list(filters),
    queryFn: async (): Promise<AdaptersData> => {
      logger.debug('Fetching adapters', {
        component: 'useAdapters',
        operation: 'fetchAdapters',
        filters,
      });

      const [adaptersData, metrics] = await Promise.all([
        apiClient.listAdapters({
          tier: filters?.tier?.[0],
          framework: undefined,
        }),
        apiClient.getSystemMetrics().catch(() => null),
      ]);

      // Apply client-side filtering for filters not supported by API
      let filteredAdapters = adaptersData;

      if (filters?.status && filters.status.length > 0) {
        filteredAdapters = filteredAdapters.filter(a =>
          a.current_state && filters.status!.includes(a.current_state)
        );
      }

      if (filters?.category && filters.category.length > 0) {
        filteredAdapters = filteredAdapters.filter(a =>
          a.category && filters.category!.includes(a.category)
        );
      }

      if (filters?.pinned !== undefined) {
        filteredAdapters = filteredAdapters.filter(a =>
          a.pinned === filters.pinned
        );
      }

      if (filters?.search) {
        const searchLower = filters.search.toLowerCase();
        filteredAdapters = filteredAdapters.filter(a =>
          a.name.toLowerCase().includes(searchLower) ||
          a.adapter_id.toLowerCase().includes(searchLower) ||
          a.framework?.toLowerCase().includes(searchLower)
        );
      }

      const totalMemory = metrics
        ? (metrics.memory_total_gb ?? 0) * 1024 * 1024 * 1024
        : 0;

      return {
        adapters: filteredAdapters,
        totalMemory,
        systemMetrics: metrics,
      };
    },
    staleTime: 30 * 1000, // 30 seconds
    refetchInterval: 60 * 1000, // 1 minute auto-refresh
  });

  // Invalidate and refetch
  const invalidateAdapters = () => {
    queryClient.invalidateQueries({ queryKey: ADAPTER_QUERY_KEYS.all });
  };

  return {
    ...query,
    invalidateAdapters,
  };
}

// Hook for adapter detail
export function useAdapterDetail(adapterId: string | undefined) {
  return useQuery({
    queryKey: ADAPTER_QUERY_KEYS.detail(adapterId ?? ''),
    queryFn: () => apiClient.getAdapterDetail(adapterId!),
    enabled: !!adapterId,
    staleTime: 60 * 1000,
  });
}

// Hook for adapter health
export function useAdapterHealth(adapterId: string | undefined) {
  return useQuery({
    queryKey: ADAPTER_QUERY_KEYS.health(adapterId ?? ''),
    queryFn: () => apiClient.getAdapterHealth(adapterId!),
    enabled: !!adapterId,
    staleTime: 30 * 1000,
  });
}

// Mutation hooks for adapter actions
export function useLoadAdapter() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (adapterId: string) => apiClient.loadAdapter(adapterId),
    onMutate: async (adapterId) => {
      await queryClient.cancelQueries({ queryKey: ADAPTER_QUERY_KEYS.all });

      logger.info('Loading adapter', {
        component: 'useAdapters',
        operation: 'loadAdapter',
        adapterId,
      });
    },
    onSuccess: (_, adapterId) => {
      toast.success('Adapter loaded successfully');
      queryClient.invalidateQueries({ queryKey: ADAPTER_QUERY_KEYS.all });
    },
    onError: (error, adapterId) => {
      logger.error('Failed to load adapter', {
        component: 'useAdapters',
        operation: 'loadAdapter',
        adapterId,
      }, error instanceof Error ? error : new Error(String(error)));
      toast.error(`Failed to load adapter: ${error instanceof Error ? error.message : 'Unknown error'}`);
    },
  });
}

export function useUnloadAdapter() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (adapterId: string) => apiClient.unloadAdapter(adapterId),
    onSuccess: (_, adapterId) => {
      toast.success('Adapter unloaded successfully');
      queryClient.invalidateQueries({ queryKey: ADAPTER_QUERY_KEYS.all });
    },
    onError: (error, adapterId) => {
      logger.error('Failed to unload adapter', {
        component: 'useAdapters',
        operation: 'unloadAdapter',
        adapterId,
      }, error instanceof Error ? error : new Error(String(error)));
      toast.error(`Failed to unload adapter: ${error instanceof Error ? error.message : 'Unknown error'}`);
    },
  });
}

export function useDeleteAdapter() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (adapterId: string) => apiClient.deleteAdapter(adapterId),
    onSuccess: (_, adapterId) => {
      toast.success('Adapter deleted successfully');
      queryClient.invalidateQueries({ queryKey: ADAPTER_QUERY_KEYS.all });
    },
    onError: (error, adapterId) => {
      logger.error('Failed to delete adapter', {
        component: 'useAdapters',
        operation: 'deleteAdapter',
        adapterId,
      }, error instanceof Error ? error : new Error(String(error)));
      toast.error(`Failed to delete adapter: ${error instanceof Error ? error.message : 'Unknown error'}`);
    },
  });
}

export function usePinAdapter() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ adapterId, pinned }: { adapterId: string; pinned: boolean }) =>
      apiClient.pinAdapter(adapterId, pinned),
    onSuccess: (_, { adapterId, pinned }) => {
      toast.success(pinned ? 'Adapter pinned successfully' : 'Adapter unpinned successfully');
      queryClient.invalidateQueries({ queryKey: ADAPTER_QUERY_KEYS.all });
    },
    onError: (error, { adapterId, pinned }) => {
      logger.error('Failed to pin/unpin adapter', {
        component: 'useAdapters',
        operation: 'pinAdapter',
        adapterId,
        pinned,
      }, error instanceof Error ? error : new Error(String(error)));
      toast.error(`Failed to ${pinned ? 'pin' : 'unpin'} adapter: ${error instanceof Error ? error.message : 'Unknown error'}`);
    },
  });
}

export function usePromoteAdapter() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (adapterId: string) => apiClient.promoteAdapterState(adapterId),
    onSuccess: (result, adapterId) => {
      toast.success(`Adapter state promoted: ${result.old_state} -> ${result.new_state}`);
      queryClient.invalidateQueries({ queryKey: ADAPTER_QUERY_KEYS.all });
    },
    onError: (error, adapterId) => {
      logger.error('Failed to promote adapter state', {
        component: 'useAdapters',
        operation: 'promoteAdapter',
        adapterId,
      }, error instanceof Error ? error : new Error(String(error)));
      toast.error(`Failed to promote adapter: ${error instanceof Error ? error.message : 'Unknown error'}`);
    },
  });
}

export function useEvictAdapter() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (adapterId: string) => apiClient.evictAdapter(adapterId),
    onSuccess: (_, adapterId) => {
      toast.success('Adapter evicted successfully');
      queryClient.invalidateQueries({ queryKey: ADAPTER_QUERY_KEYS.all });
    },
    onError: (error, adapterId) => {
      logger.error('Failed to evict adapter', {
        component: 'useAdapters',
        operation: 'evictAdapter',
        adapterId,
      }, error instanceof Error ? error : new Error(String(error)));
      toast.error(`Failed to evict adapter: ${error instanceof Error ? error.message : 'Unknown error'}`);
    },
  });
}
