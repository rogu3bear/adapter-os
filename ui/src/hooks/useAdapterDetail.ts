// React Query hook for adapter detail data
// Provides comprehensive adapter information including detail, lineage, activations, manifest, and health
//
// Usage:
//   const { adapter, lineage, activations, manifest, health, isLoading, error, refetch } = useAdapterDetail(adapterId);

import { useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import apiClient from '../api/client';
import {
  AdapterDetailResponse,
  AdapterLineageResponse,
  AdapterActivation,
  AdapterManifest,
  AdapterHealthResponse,
  LifecycleTransitionResponse,
} from '../api/adapter-types';
import { logger } from '../utils/logger';

// Query keys for cache management
export const adapterDetailKeys = {
  all: ['adapter-detail'] as const,
  detail: (id: string) => [...adapterDetailKeys.all, 'detail', id] as const,
  lineage: (id: string) => [...adapterDetailKeys.all, 'lineage', id] as const,
  activations: (id: string) => [...adapterDetailKeys.all, 'activations', id] as const,
  manifest: (id: string) => [...adapterDetailKeys.all, 'manifest', id] as const,
  health: (id: string) => [...adapterDetailKeys.all, 'health', id] as const,
};

export interface UseAdapterDetailOptions {
  enabled?: boolean;
  refetchInterval?: number;
  onError?: (error: Error) => void;
}

export interface UseAdapterDetailReturn {
  // Data
  adapter: AdapterDetailResponse | null;
  lineage: AdapterLineageResponse | null;
  activations: AdapterActivation[] | null;
  manifest: AdapterManifest | null;
  health: AdapterHealthResponse | null;

  // Loading states
  isLoading: boolean;
  isLoadingDetail: boolean;
  isLoadingLineage: boolean;
  isLoadingActivations: boolean;
  isLoadingManifest: boolean;
  isLoadingHealth: boolean;

  // Error states
  error: Error | null;
  detailError: Error | null;
  lineageError: Error | null;
  activationsError: Error | null;
  manifestError: Error | null;
  healthError: Error | null;

  // Actions
  refetch: () => Promise<void>;
  refetchDetail: () => Promise<void>;
  refetchLineage: () => Promise<void>;
  refetchActivations: () => Promise<void>;
  refetchManifest: () => Promise<void>;
  refetchHealth: () => Promise<void>;

  // Mutations
  promoteLifecycle: (reason: string) => Promise<LifecycleTransitionResponse>;
  demoteLifecycle: (reason: string) => Promise<LifecycleTransitionResponse>;
  isPromoting: boolean;
  isDemoting: boolean;
  promotionError: Error | null;
  demotionError: Error | null;
}

export function useAdapterDetail(
  adapterId: string,
  options: UseAdapterDetailOptions = {}
): UseAdapterDetailReturn {
  const { enabled = true, refetchInterval, onError } = options;
  const queryClient = useQueryClient();

  // Adapter detail query
  const detailQuery = useQuery({
    queryKey: adapterDetailKeys.detail(adapterId),
    queryFn: async () => {
      logger.debug('Fetching adapter detail', { component: 'useAdapterDetail', adapterId });
      return apiClient.getAdapterDetail(adapterId);
    },
    enabled: enabled && !!adapterId,
    refetchInterval,
    staleTime: 30000, // Consider data fresh for 30 seconds
  });

  // Lineage query
  const lineageQuery = useQuery({
    queryKey: adapterDetailKeys.lineage(adapterId),
    queryFn: async () => {
      logger.debug('Fetching adapter lineage', { component: 'useAdapterDetail', adapterId });
      return apiClient.getAdapterLineage(adapterId);
    },
    enabled: enabled && !!adapterId,
    staleTime: 60000, // Lineage changes less frequently
  });

  // Activations query
  const activationsQuery = useQuery({
    queryKey: adapterDetailKeys.activations(adapterId),
    queryFn: async () => {
      logger.debug('Fetching adapter activations', { component: 'useAdapterDetail', adapterId });
      return apiClient.getAdapterActivations(adapterId);
    },
    enabled: enabled && !!adapterId,
    refetchInterval: refetchInterval || 30000, // Refresh activations more frequently
    staleTime: 15000,
  });

  // Manifest query
  const manifestQuery = useQuery({
    queryKey: adapterDetailKeys.manifest(adapterId),
    queryFn: async () => {
      logger.debug('Fetching adapter manifest', { component: 'useAdapterDetail', adapterId });
      return apiClient.downloadAdapterManifest(adapterId);
    },
    enabled: enabled && !!adapterId,
    staleTime: 300000, // Manifest rarely changes, cache for 5 minutes
  });

  // Health query
  const healthQuery = useQuery({
    queryKey: adapterDetailKeys.health(adapterId),
    queryFn: async () => {
      logger.debug('Fetching adapter health', { component: 'useAdapterDetail', adapterId });
      return apiClient.getAdapterHealth(adapterId);
    },
    enabled: enabled && !!adapterId,
    refetchInterval: refetchInterval || 60000, // Health checks every minute
    staleTime: 30000,
  });

  // Promote lifecycle mutation
  const promoteMutation = useMutation({
    mutationFn: async (reason: string) => {
      logger.info('Promoting adapter lifecycle', { component: 'useAdapterDetail', adapterId, reason });
      return apiClient.promoteAdapterLifecycle(adapterId, reason);
    },
    onSuccess: () => {
      // Invalidate relevant queries
      queryClient.invalidateQueries({ queryKey: adapterDetailKeys.detail(adapterId) });
      queryClient.invalidateQueries({ queryKey: adapterDetailKeys.health(adapterId) });
    },
    onError: (error: Error) => {
      logger.error('Failed to promote adapter lifecycle', { component: 'useAdapterDetail', adapterId }, error);
      onError?.(error);
    },
  });

  // Demote lifecycle mutation
  const demoteMutation = useMutation({
    mutationFn: async (reason: string) => {
      logger.info('Demoting adapter lifecycle', { component: 'useAdapterDetail', adapterId, reason });
      return apiClient.demoteAdapterLifecycle(adapterId, reason);
    },
    onSuccess: () => {
      // Invalidate relevant queries
      queryClient.invalidateQueries({ queryKey: adapterDetailKeys.detail(adapterId) });
      queryClient.invalidateQueries({ queryKey: adapterDetailKeys.health(adapterId) });
    },
    onError: (error: Error) => {
      logger.error('Failed to demote adapter lifecycle', { component: 'useAdapterDetail', adapterId }, error);
      onError?.(error);
    },
  });

  // Refetch functions
  const refetchDetail = useCallback(async () => {
    await detailQuery.refetch();
  }, [detailQuery]);

  const refetchLineage = useCallback(async () => {
    await lineageQuery.refetch();
  }, [lineageQuery]);

  const refetchActivations = useCallback(async () => {
    await activationsQuery.refetch();
  }, [activationsQuery]);

  const refetchManifest = useCallback(async () => {
    await manifestQuery.refetch();
  }, [manifestQuery]);

  const refetchHealth = useCallback(async () => {
    await healthQuery.refetch();
  }, [healthQuery]);

  const refetch = useCallback(async () => {
    await Promise.all([
      refetchDetail(),
      refetchLineage(),
      refetchActivations(),
      refetchManifest(),
      refetchHealth(),
    ]);
  }, [refetchDetail, refetchLineage, refetchActivations, refetchManifest, refetchHealth]);

  // Mutation wrappers
  const promoteLifecycle = useCallback(
    async (reason: string) => {
      return promoteMutation.mutateAsync(reason);
    },
    [promoteMutation]
  );

  const demoteLifecycle = useCallback(
    async (reason: string) => {
      return demoteMutation.mutateAsync(reason);
    },
    [demoteMutation]
  );

  // Combine loading states
  const isLoading =
    detailQuery.isLoading ||
    lineageQuery.isLoading ||
    activationsQuery.isLoading ||
    manifestQuery.isLoading ||
    healthQuery.isLoading;

  // Combine errors (return first error encountered)
  const error =
    detailQuery.error ||
    lineageQuery.error ||
    activationsQuery.error ||
    manifestQuery.error ||
    healthQuery.error;

  return {
    // Data
    adapter: detailQuery.data ?? null,
    lineage: lineageQuery.data ?? null,
    activations: activationsQuery.data ?? null,
    manifest: manifestQuery.data ?? null,
    health: healthQuery.data ?? null,

    // Loading states
    isLoading,
    isLoadingDetail: detailQuery.isLoading,
    isLoadingLineage: lineageQuery.isLoading,
    isLoadingActivations: activationsQuery.isLoading,
    isLoadingManifest: manifestQuery.isLoading,
    isLoadingHealth: healthQuery.isLoading,

    // Error states
    error: error as Error | null,
    detailError: detailQuery.error as Error | null,
    lineageError: lineageQuery.error as Error | null,
    activationsError: activationsQuery.error as Error | null,
    manifestError: manifestQuery.error as Error | null,
    healthError: healthQuery.error as Error | null,

    // Actions
    refetch,
    refetchDetail,
    refetchLineage,
    refetchActivations,
    refetchManifest,
    refetchHealth,

    // Mutations
    promoteLifecycle,
    demoteLifecycle,
    isPromoting: promoteMutation.isPending,
    isDemoting: demoteMutation.isPending,
    promotionError: promoteMutation.error as Error | null,
    demotionError: demoteMutation.error as Error | null,
  };
}

export default useAdapterDetail;
