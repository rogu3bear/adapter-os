/**
 * React Query hooks for Adapter API
 *
 * Provides hooks for CRUD operations on adapters with cache invalidation.
 * Uses the createResourceHooks factory for standard operations and extends
 * with adapter-specific operations (load, unload, pin, evict, promote, etc.)
 */

import { useMutation, useQueryClient, UseMutationOptions } from '@tanstack/react-query';
import { apiClient } from '@/api/client';
import { createResourceHooks } from './factories/createApiHooks';
import type {
  Adapter,
  RegisterAdapterRequest,
  AdapterStateResponse,
  LifecycleTransitionResponse,
} from '@/api/adapter-types';

// Create base resource hooks using the factory
const baseHooks = createResourceHooks<
  Adapter,
  Adapter,
  RegisterAdapterRequest,
  Partial<Adapter>
>({
  resourceName: 'adapters',
  api: {
    list: () => apiClient.listAdapters(),
    get: (id: string) => apiClient.getAdapter(id),
    create: (data: RegisterAdapterRequest) => apiClient.registerAdapter(data),
    delete: (id: string) => apiClient.deleteAdapter(id),
    // Note: There's no generic update endpoint for adapters
  },
  staleTime: 10000, // 10 seconds - adapters change frequently with lifecycle updates
  invalidatesOnMutate: ['metrics', 'system'], // Invalidate related queries
});

// Export base hooks with backwards-compatible names
export const adapterKeys = baseHooks.keys;
export const useAdapters = baseHooks.useList;
export const useAdapter = baseHooks.useDetail;
export const useCreateAdapter = baseHooks.useCreate;
export const useDeleteAdapter = baseHooks.useDelete;

/**
 * Hook for loading an adapter (lifecycle operation)
 */
export function useLoadAdapter(
  options?: UseMutationOptions<Adapter, Error, string, unknown>
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...restOptions } = options ?? {};
  return useMutation<Adapter, Error, string>({
    mutationFn: (adapterId: string) => apiClient.loadAdapter(adapterId),
    ...restOptions,
    onSuccess: async (data, adapterId, ...rest) => {
      queryClient.invalidateQueries({ queryKey: adapterKeys.detail(adapterId) });
      queryClient.invalidateQueries({ queryKey: adapterKeys.lists() });
      queryClient.invalidateQueries({ queryKey: ['metrics'] });
      await onSuccess?.(data, adapterId, ...rest);
    },
  });
}

/**
 * Hook for unloading an adapter (lifecycle operation)
 */
export function useUnloadAdapter(
  options?: UseMutationOptions<void, Error, string, unknown>
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...restOptions } = options ?? {};
  return useMutation<void, Error, string>({
    mutationFn: (adapterId: string) => apiClient.unloadAdapter(adapterId),
    ...restOptions,
    onSuccess: async (data, adapterId, ...rest) => {
      queryClient.invalidateQueries({ queryKey: adapterKeys.detail(adapterId) });
      queryClient.invalidateQueries({ queryKey: adapterKeys.lists() });
      queryClient.invalidateQueries({ queryKey: ['metrics'] });
      await onSuccess?.(data, adapterId, ...rest);
    },
  });
}

/**
 * Hook for pinning/unpinning an adapter
 */
export function usePinAdapter(
  options?: UseMutationOptions<void, Error, { adapterId: string; pinned: boolean | number; reason?: string }, unknown>
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...restOptions } = options ?? {};
  return useMutation<void, Error, { adapterId: string; pinned: boolean | number; reason?: string }>({
    mutationFn: ({ adapterId, pinned, reason }: { adapterId: string; pinned: boolean | number; reason?: string }) =>
      apiClient.pinAdapter(adapterId, pinned, reason),
    ...restOptions,
    onSuccess: async (data, variables, ...rest) => {
      queryClient.invalidateQueries({ queryKey: adapterKeys.detail(variables.adapterId) });
      queryClient.invalidateQueries({ queryKey: adapterKeys.lists() });
      await onSuccess?.(data, variables, ...rest);
    },
  });
}

/**
 * Hook for evicting an adapter from memory
 */
export function useEvictAdapter(
  options?: UseMutationOptions<{ success: boolean; message: string }, Error, string, unknown>
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...restOptions } = options ?? {};
  return useMutation<{ success: boolean; message: string }, Error, string>({
    mutationFn: (adapterId: string) => apiClient.evictAdapter(adapterId),
    ...restOptions,
    onSuccess: async (data, adapterId, ...rest) => {
      queryClient.invalidateQueries({ queryKey: adapterKeys.detail(adapterId) });
      queryClient.invalidateQueries({ queryKey: adapterKeys.lists() });
      queryClient.invalidateQueries({ queryKey: ['metrics'] });
      await onSuccess?.(data, adapterId, ...rest);
    },
  });
}

/**
 * Hook for promoting adapter state (lifecycle operation)
 */
export function usePromoteAdapter(
  options?: UseMutationOptions<AdapterStateResponse, Error, string, unknown>
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...restOptions } = options ?? {};
  return useMutation<AdapterStateResponse, Error, string>({
    mutationFn: (adapterId: string) => apiClient.promoteAdapterState(adapterId),
    ...restOptions,
    onSuccess: async (data, adapterId, ...rest) => {
      queryClient.invalidateQueries({ queryKey: adapterKeys.detail(adapterId) });
      queryClient.invalidateQueries({ queryKey: adapterKeys.lists() });
      queryClient.invalidateQueries({ queryKey: ['metrics'] });
      await onSuccess?.(data, adapterId, ...rest);
    },
  });
}

/**
 * Hook for importing an adapter from a file
 */
export function useImportAdapter(
  options?: UseMutationOptions<Adapter, Error, { file: File; load?: boolean }, unknown>
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...restOptions } = options ?? {};
  return useMutation<Adapter, Error, { file: File; load?: boolean }>({
    mutationFn: ({ file, load }: { file: File; load?: boolean }) =>
      apiClient.importAdapter(file, load),
    ...restOptions,
    onSuccess: async (data, variables, ...rest) => {
      queryClient.invalidateQueries({ queryKey: adapterKeys.lists() });
      await onSuccess?.(data, variables, ...rest);
    },
  });
}

/**
 * Hook for promoting adapter lifecycle state
 */
export function usePromoteAdapterLifecycle(
  options?: UseMutationOptions<LifecycleTransitionResponse, Error, { adapterId: string; reason: string }, unknown>
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...restOptions } = options ?? {};
  return useMutation<LifecycleTransitionResponse, Error, { adapterId: string; reason: string }>({
    mutationFn: ({ adapterId, reason }: { adapterId: string; reason: string }) =>
      apiClient.promoteAdapterLifecycle(adapterId, reason),
    ...restOptions,
    onSuccess: async (data, variables, ...rest) => {
      queryClient.invalidateQueries({ queryKey: adapterKeys.detail(variables.adapterId) });
      queryClient.invalidateQueries({ queryKey: adapterKeys.lists() });
      await onSuccess?.(data, variables, ...rest);
    },
  });
}

/**
 * Hook for demoting adapter lifecycle state
 */
export function useDemoteAdapterLifecycle(
  options?: UseMutationOptions<LifecycleTransitionResponse, Error, { adapterId: string; reason: string }, unknown>
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...restOptions } = options ?? {};
  return useMutation<LifecycleTransitionResponse, Error, { adapterId: string; reason: string }>({
    mutationFn: ({ adapterId, reason }: { adapterId: string; reason: string }) =>
      apiClient.demoteAdapterLifecycle(adapterId, reason),
    ...restOptions,
    onSuccess: async (data, variables, ...rest) => {
      queryClient.invalidateQueries({ queryKey: adapterKeys.detail(variables.adapterId) });
      queryClient.invalidateQueries({ queryKey: adapterKeys.lists() });
      await onSuccess?.(data, variables, ...rest);
    },
  });
}

/**
 * Combined hook providing all adapter operations
 *
 * This is a convenience hook that returns all available adapter operations
 * in a single object. For most use cases, prefer using individual hooks.
 */
export function useAdaptersApi() {
  const queryClient = useQueryClient();

  const loadMutation = useLoadAdapter();
  const unloadMutation = useUnloadAdapter();
  const pinMutation = usePinAdapter();
  const evictMutation = useEvictAdapter();
  const promoteMutation = usePromoteAdapter();
  const deleteMutation = useDeleteAdapter();
  const importMutation = useImportAdapter();
  const promoteLifecycleMutation = usePromoteAdapterLifecycle();
  const demoteLifecycleMutation = useDemoteAdapterLifecycle();

  return {
    // Query hooks (use these directly in components)
    useAdapters,
    useAdapter,

    // Mutation methods
    loadAdapter: loadMutation.mutateAsync,
    isLoading: loadMutation.isPending,
    loadError: loadMutation.error,

    unloadAdapter: unloadMutation.mutateAsync,
    isUnloading: unloadMutation.isPending,
    unloadError: unloadMutation.error,

    pinAdapter: pinMutation.mutateAsync,
    isPinning: pinMutation.isPending,
    pinError: pinMutation.error,

    evictAdapter: evictMutation.mutateAsync,
    isEvicting: evictMutation.isPending,
    evictError: evictMutation.error,

    promoteAdapter: promoteMutation.mutateAsync,
    isPromoting: promoteMutation.isPending,
    promoteError: promoteMutation.error,

    deleteAdapter: deleteMutation.mutateAsync,
    isDeleting: deleteMutation.isPending,
    deleteError: deleteMutation.error,

    importAdapter: importMutation.mutateAsync,
    isImporting: importMutation.isPending,
    importError: importMutation.error,

    promoteLifecycle: promoteLifecycleMutation.mutateAsync,
    isPromotingLifecycle: promoteLifecycleMutation.isPending,
    promoteLifecycleError: promoteLifecycleMutation.error,

    demoteLifecycle: demoteLifecycleMutation.mutateAsync,
    isDemotingLifecycle: demoteLifecycleMutation.isPending,
    demoteLifecycleError: demoteLifecycleMutation.error,

    // Cache invalidation
    invalidateAdapters: () =>
      queryClient.invalidateQueries({ queryKey: adapterKeys.all }),
  };
}

export default useAdaptersApi;
