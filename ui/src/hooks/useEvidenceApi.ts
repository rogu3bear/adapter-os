/**
 * React Query hooks for Evidence API
 *
 * Provides hooks for CRUD operations on evidence entries with filtering.
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import type {
  Evidence,
  CreateEvidenceRequest,
  ListEvidenceQuery,
} from '../api/document-types';

// Query keys for cache management
export const evidenceKeys = {
  all: ['evidence'] as const,
  lists: () => [...evidenceKeys.all, 'list'] as const,
  list: (filter?: ListEvidenceQuery) => [...evidenceKeys.lists(), filter] as const,
  details: () => [...evidenceKeys.all, 'detail'] as const,
  detail: (id: string) => [...evidenceKeys.details(), id] as const,
  byDataset: (datasetId: string) => [...evidenceKeys.all, 'dataset', datasetId] as const,
  byAdapter: (adapterId: string) => [...evidenceKeys.all, 'adapter', adapterId] as const,
};

/**
 * Hook for listing evidence with optional filters
 */
export function useEvidence(filter?: ListEvidenceQuery) {
  return useQuery({
    queryKey: evidenceKeys.list(filter),
    queryFn: () => apiClient.listEvidence(filter),
  });
}

/**
 * Hook for getting a single evidence entry
 */
export function useEvidenceEntry(evidenceId: string | undefined) {
  return useQuery({
    queryKey: evidenceKeys.detail(evidenceId ?? ''),
    queryFn: () => apiClient.getEvidence(evidenceId!),
    enabled: !!evidenceId,
  });
}

/**
 * Hook for getting evidence entries for a dataset
 */
export function useDatasetEvidence(datasetId: string | undefined) {
  return useQuery({
    queryKey: evidenceKeys.byDataset(datasetId ?? ''),
    queryFn: () => apiClient.getDatasetEvidence(datasetId!),
    enabled: !!datasetId,
  });
}

/**
 * Hook for getting evidence entries for an adapter
 */
export function useAdapterEvidence(adapterId: string | undefined) {
  return useQuery({
    queryKey: evidenceKeys.byAdapter(adapterId ?? ''),
    queryFn: () => apiClient.getAdapterEvidence(adapterId!),
    enabled: !!adapterId,
  });
}

/**
 * Hook providing all evidence CRUD operations with cache invalidation
 */
export function useEvidenceApi(filter?: ListEvidenceQuery) {
  const queryClient = useQueryClient();

  const createMutation = useMutation({
    mutationFn: (request: CreateEvidenceRequest) => apiClient.createEvidence(request),
    onSuccess: (newEvidence) => {
      queryClient.invalidateQueries({ queryKey: evidenceKeys.lists() });
      // Also invalidate dataset/adapter specific queries
      if (newEvidence.dataset_id) {
        queryClient.invalidateQueries({
          queryKey: evidenceKeys.byDataset(newEvidence.dataset_id),
        });
      }
      if (newEvidence.adapter_id) {
        queryClient.invalidateQueries({
          queryKey: evidenceKeys.byAdapter(newEvidence.adapter_id),
        });
      }
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (evidenceId: string) => apiClient.deleteEvidence(evidenceId),
    onSuccess: (_data, evidenceId) => {
      queryClient.invalidateQueries({ queryKey: evidenceKeys.lists() });
      queryClient.removeQueries({ queryKey: evidenceKeys.detail(evidenceId) });
    },
  });

  return {
    // Queries
    evidence: useEvidence(filter),

    // Mutations
    createEvidence: createMutation.mutateAsync,
    isCreating: createMutation.isPending,
    createError: createMutation.error,

    deleteEvidence: deleteMutation.mutateAsync,
    isDeleting: deleteMutation.isPending,
    deleteError: deleteMutation.error,

    // Cache invalidation
    invalidateEvidence: () =>
      queryClient.invalidateQueries({ queryKey: evidenceKeys.all }),
  };
}

export default useEvidenceApi;
